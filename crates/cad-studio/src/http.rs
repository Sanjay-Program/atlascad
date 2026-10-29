//! A deliberately small HTTP/1.1 server for the local application.
//!
//! The studio is a single-user program that talks to its own browser tab on loopback, so it does not
//! need routing macros, async runtimes, TLS, or HTTP/2. It does need the parts that keep an untrusted
//! local page honest: bounded headers and bodies, a short read timeout, no request pipelining, and
//! `Connection: close` on every response.
//!
//! Only `GET` and `POST` are accepted; everything else is `405`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Largest request line plus headers. A browser sends well under 8 KiB.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;
/// Largest JSON request body the studio accepts. Project files are not sent over HTTP.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
/// How long a client has to send its request.
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a client has to accept the response.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a request could not be read.
#[derive(Debug)]
pub enum HttpError {
    /// The peer went away or the socket timed out. There is nobody to answer.
    Io,
    HeadersTooLarge,
    BodyTooLarge,
    Malformed(String),
}

impl From<std::io::Error> for HttpError {
    fn from(_error: std::io::Error) -> Self {
        HttpError::Io
    }
}

/// A parsed request. Header names are lowercased; values are trimmed.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    /// The origin-form target **with** its query string, if any. Routing strips the query; the
    /// share token is read from it, so it must survive parsing.
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// A fully written response body.
pub struct Response {
    pub status: u16,
    pub reason: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub extra_headers: Vec<(String, String)>,
}

impl Response {
    pub fn new(
        status: u16,
        reason: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    ) -> Self {
        Response {
            status,
            reason,
            content_type,
            body,
            extra_headers: Vec::new(),
        }
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((name.into(), value.into()));
        self
    }
}

pub fn reason_for(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Read exactly one request from the socket.
///
/// The header block is parsed before the body is read so an oversized declared body is refused
/// without allocating it. Bytes after the declared body are rejected rather than treated as a
/// pipelined request, which removes a request-smuggling surface this server does not need.
pub fn read_request(stream: &mut TcpStream) -> Result<Request, HttpError> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;

    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
        if buffer.len() > MAX_HEADER_BYTES {
            return Err(HttpError::HeadersTooLarge);
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Err(HttpError::Malformed(
                "connection closed before the request headers ended".into(),
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    };
    if header_end > MAX_HEADER_BYTES {
        return Err(HttpError::HeadersTooLarge);
    }

    let header_text = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| HttpError::Malformed("missing request line".into()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| HttpError::Malformed("missing method".into()))?
        .to_string();
    let target = parts
        .next()
        .ok_or_else(|| HttpError::Malformed("missing request target".into()))?;
    let version = parts
        .next()
        .ok_or_else(|| HttpError::Malformed("missing HTTP version".into()))?;
    if !version.starts_with("HTTP/1.") {
        return Err(HttpError::Malformed(format!(
            "unsupported protocol version `{version}`"
        )));
    }
    if !target.starts_with('/') {
        return Err(HttpError::Malformed(
            "only origin-form request targets are accepted".into(),
        ));
    }
    let path = target.to_string();

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':').ok_or_else(|| {
            HttpError::Malformed(format!("malformed header line `{}`", truncate(line)))
        })?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(HttpError::Malformed("empty header name".into()));
        }
        headers.insert(name, value.trim().to_string());
    }

    if headers.contains_key("transfer-encoding") {
        return Err(HttpError::Malformed(
            "chunked transfer encoding is not accepted".into(),
        ));
    }

    let content_length = match headers.get("content-length") {
        None => 0,
        Some(raw) => raw
            .parse::<usize>()
            .map_err(|_| HttpError::Malformed("content-length is not a number".into()))?,
    };
    if content_length > MAX_BODY_BYTES {
        return Err(HttpError::BodyTooLarge);
    }
    if matches!(method.as_str(), "GET" | "HEAD") && content_length != 0 {
        return Err(HttpError::Malformed(
            "a GET/HEAD request must not carry a body".into(),
        ));
    }

    let body_start = header_end + 4;
    let available = buffer.len().saturating_sub(body_start);
    if available > content_length {
        return Err(HttpError::Malformed(
            "request carries bytes beyond its declared body".into(),
        ));
    }
    let mut body = Vec::with_capacity(content_length);
    body.extend_from_slice(&buffer[body_start..]);
    while body.len() < content_length {
        let remaining = content_length - body.len();
        let want = remaining.min(chunk.len());
        let read = stream.read(&mut chunk[..want])?;
        if read == 0 {
            return Err(HttpError::Malformed(
                "connection closed before the declared body arrived".into(),
            ));
        }
        body.extend_from_slice(&chunk[..read]);
    }

    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

/// Write a complete response. Every response closes the connection.
pub fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'\r\n",
        response.status,
        response.reason,
        response.content_type,
        response.body.len()
    );
    for (name, value) in &response.extra_headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if !response.body.is_empty() {
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn truncate(line: &str) -> String {
    line.chars().take(80).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_end_is_found_at_the_terminator() {
        assert_eq!(find_header_end(b"GET / HTTP/1.1\r\n\r\n"), Some(14));
        assert_eq!(find_header_end(b"GET / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn reason_strings_are_not_empty() {
        for status in [200, 400, 403, 404, 405, 413, 415, 422, 431, 500, 503] {
            assert!(!reason_for(status).is_empty());
        }
    }
}
