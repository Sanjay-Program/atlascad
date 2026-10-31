//! The studio binary's deployment paths.
//!
//! A deployed instance must not carry its secret on a command line: `ps` is world-readable, and a
//! shell history is a file. These tests drive the real binary with `--token-file` and
//! `ATLAS_STUDIO_TOKEN`, check that it starts and answers over the network, and check that the
//! printed link does not leak the token when it came from either source.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// A studio process that is killed when the test ends, pass or fail.
struct Studio(Child);

impl Drop for Studio {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "atlas-cli-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A port the OS says is free. The listener is dropped before the studio binds it, which is a small
/// race the caller absorbs by retrying nothing: a collision fails the test loudly rather than
/// silently testing the wrong server.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(0)
}

/// Create the fixture without launching a server: `--new` also starts the studio and parks, so it is
/// the wrong tool for writing a file in a test.
fn project(dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = dir.join("part.zcad");
    let project = cad_studio::default_project("part", [20.0, 10.0, 5.0])?;
    project.save(&path)?;
    Ok(path)
}

/// Start the studio and return it with the `share:` line it printed, or the failure it printed.
fn start(
    args: &[&str],
    env: &[(&str, &str)],
) -> Result<(Studio, String), Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cad-studio"))
        .args(args)
        // Clear the ambient variable first, then apply what the test asked for: a test that wants
        // the environment path must not depend on the environment that ran it.
        .env_remove("ATLAS_STUDIO_TOKEN")
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
    let mut share = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(line) => {
                if let Some(rest) = line.trim().strip_prefix("share: ") {
                    share = Some(rest.to_string());
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let mut studio = Studio(child);
    match share {
        Some(url) => Ok((studio, url)),
        None => {
            let mut stderr = String::new();
            if let Some(mut pipe) = studio.0.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            Err(format!("the studio did not print a share link: {stderr}").into())
        }
    }
}

fn get(addr: SocketAddr, path: &str, token: Option<&str>) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n"
    )?;
    if let Some(token) = token {
        write!(stream, "X-Atlas-Token: {token}\r\n")?;
    }
    write!(stream, "\r\n")?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

fn status(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

const TOKEN: &str = "deploymenttoken0123456789abcdef";

#[test]
fn a_token_file_starts_a_shared_studio_without_printing_the_token()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new("token-file")?;
    let path = project(&dir.0)?;
    let token_path = dir.0.join("token");
    std::fs::write(&token_path, format!("{TOKEN}\n"))?;

    let port = free_port();
    let port_text = port.to_string();
    let path_text = path.to_string_lossy().into_owned();
    let token_text = token_path.to_string_lossy().into_owned();
    let (_studio, share) = start(
        &[
            "--file",
            &path_text,
            "--share",
            "--host",
            "0.0.0.0",
            "--advertise",
            "127.0.0.1",
            "--token-file",
            &token_text,
            "--port",
            &port_text,
        ],
        &[],
    )?;

    // The link is printed, but the secret is not: the operator already has the file.
    assert!(
        share.contains("token=<from token file>"),
        "the link should be redacted, got {share}"
    );
    assert!(!share.contains(TOKEN), "the token must not be printed");

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let hidden = get(addr, "/api/state", None)?;
    assert_eq!(status(&hidden), 401, "no token is refused");
    let shown = get(addr, "/api/state", Some(TOKEN))?;
    assert_eq!(status(&shown), 200, "the file's token is accepted");
    assert!(shown.contains("\"name\""), "the state payload is served");
    Ok(())
}

#[test]
fn the_environment_can_supply_the_token_and_a_short_one_is_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new("token-env")?;
    let path = project(&dir.0)?;
    let path_text = path.to_string_lossy().into_owned();

    let port = free_port();
    let port_text = port.to_string();
    let (_studio, share) = start(
        &[
            "--file",
            &path_text,
            "--share",
            "--host",
            "0.0.0.0",
            "--advertise",
            "127.0.0.1",
            "--port",
            &port_text,
        ],
        &[("ATLAS_STUDIO_TOKEN", TOKEN)],
    )?;
    assert!(
        share.contains("token=<from token file>"),
        "an environment token is not printed either, got {share}"
    );
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
    assert_eq!(status(&get(addr, "/api/state", None)?), 401);
    assert_eq!(status(&get(addr, "/api/state", Some(TOKEN))?), 200);

    // A token that is too short is a refusal before anything binds.
    let refused = Command::new(env!("CARGO_BIN_EXE_cad-studio"))
        .args(["--file", &path_text, "--share", "--token", "short"])
        .output()?;
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("at least 16 characters"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );

    // A token with whitespace cannot be put in a URL, so it is refused as well.
    let whitespace = Command::new(env!("CARGO_BIN_EXE_cad-studio"))
        .args([
            "--file",
            &path_text,
            "--share",
            "--token",
            "deployment token 0123456789",
        ])
        .output()?;
    assert_eq!(whitespace.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&whitespace.stderr).contains("whitespace"),
        "{}",
        String::from_utf8_lossy(&whitespace.stderr)
    );

    // A token file that cannot be read fails with the path, not with a panic or an empty token.
    let missing = Command::new(env!("CARGO_BIN_EXE_cad-studio"))
        .args([
            "--file",
            &path_text,
            "--share",
            "--token-file",
            "/nonexistent/token-file",
        ])
        .output()?;
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("token file"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    Ok(())
}
