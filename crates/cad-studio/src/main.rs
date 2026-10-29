//! Launch the local Atlas CAD studio.
//!
//! ```text
//! cad-studio --file part.zcad
//! cad-studio --new part.zcad --name bracket --size 40,30,10
//! cad-studio --file part.zcad --port 7600
//! cad-studio --file part.zcad --share
//! ```

use cad_studio::{DEFAULT_PORT, ShareOptions, default_project, start_with_options};
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::PathBuf;

fn main() {
    match run() {
        Ok(()) => {}
        Err(message) => {
            eprintln!("cad-studio: {message}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1).peekable();
    let mut file: Option<PathBuf> = None;
    let mut create = false;
    let mut name: Option<String> = None;
    let mut size = [40.0f64, 30.0, 10.0];
    let mut port = DEFAULT_PORT;
    let mut share = false;
    let mut bind: Option<IpAddr> = None;
    let mut advertise: Option<String> = None;
    let mut token: Option<String> = None;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--file" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--file` needs a path".to_string())?;
                file = Some(PathBuf::from(value));
            }
            "--new" => {
                create = true;
                // `--new PATH` is the documented form; `--file PATH --new` also works.
                if file.is_none()
                    && arguments.peek().is_some_and(|next| !next.starts_with("--"))
                    && let Some(value) = arguments.next()
                {
                    file = Some(PathBuf::from(value));
                }
            }
            "--name" => {
                name = Some(
                    arguments
                        .next()
                        .ok_or_else(|| "`--name` needs a value".to_string())?,
                );
            }
            "--size" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--size` needs X,Y,Z".to_string())?;
                size = parse_triple(&value)?;
            }
            "--port" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--port` needs a number".to_string())?;
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("`--port {value}` is not a port number"))?;
                if port == 0 {
                    return Err("port 0 is reserved for tests; choose a real port".into());
                }
            }
            "--share" => share = true,
            "--advertise" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--advertise` needs a host".to_string())?;
                if value.trim().is_empty() {
                    return Err("`--advertise` needs a host".into());
                }
                advertise = Some(value.trim().to_string());
            }
            "--host" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--host` needs an IP address".to_string())?;
                bind = Some(
                    value
                        .parse::<IpAddr>()
                        .map_err(|_| format!("`--host {value}` is not an IP address"))?,
                );
            }
            "--token" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--token` needs a value".to_string())?;
                if value.len() < 16 {
                    return Err("`--token` must be at least 16 characters".into());
                }
                token = Some(value);
            }
            "--help" | "-h" => {
                usage();
                return Ok(());
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`; run `cad-studio --help`"
                ));
            }
        }
    }

    let path = file
        .ok_or_else(|| "a project file is required: `cad-studio --file part.zcad`".to_string())?;

    if share && bind.is_none() {
        bind = Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    let shared = bind.is_some_and(|ip| !ip.is_loopback());
    if token.is_some() && !shared {
        return Err("`--token` only applies to a shared studio; add `--share` or `--host`".into());
    }
    let advertised = if shared {
        advertise.or_else(|| share_host(bind))
    } else {
        None
    };
    if shared && advertised.is_none() {
        eprintln!(
            "cad-studio: no network address found; pass `--host <this machine's IP>` so the share link is usable"
        );
    }

    let mut recovered = None;
    let project = if path.exists() {
        if create {
            return Err(format!(
                "{} already exists; refusing to overwrite it with --new",
                path.display()
            ));
        }
        // Prefer an unsaved-edit snapshot that is newer than the file, and fall back to it when the
        // file itself cannot be read. The state payload reports which of those happened.
        let (project, info) = cad_document::open_with_recovery(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        recovered = info;
        project
    } else {
        if !create {
            return Err(format!(
                "{} does not exist; pass --new to create it",
                path.display()
            ));
        }
        let name = name.unwrap_or_else(|| file_stem(&path));
        let project = default_project(&name, size).map_err(|error| error.to_string())?;
        project
            .save(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        project
    };

    let handle = start_with_options(
        project,
        path.clone(),
        port,
        recovered,
        ShareOptions {
            bind,
            token,
            host: advertised,
        },
    )
    .map_err(|error| error.to_string())?;
    println!("Atlas CAD Studio");
    println!("  file: {}", path.display());
    if let Some(info) = recovered {
        println!(
            "  recovered: unsaved changes from the recovery snapshot{}",
            if info.file_unreadable {
                " (the project file could not be read)"
            } else {
                ""
            }
        );
    }
    println!("  url:  {}", handle.url());
    if let Some(share_url) = handle.share_url() {
        println!("  share: {share_url}");
        println!("         anyone with this link can view and edit until you stop the studio");
    }
    println!("  stop: Ctrl+C");
    println!();

    loop {
        std::thread::park();
    }
}

fn usage() {
    println!("cad-studio {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!(
        "  cad-studio --file PATH [--port PORT] [--share] [--host IP] [--advertise HOST] [--token TOKEN]"
    );
    println!("  cad-studio --new PATH [--name NAME] [--size X,Y,Z] [--port PORT]");
    println!();
    println!("The studio binds 127.0.0.1 unless shared. `--share` binds 0.0.0.0 and prints a");
    println!(
        "tokenised link a colleague on the same network can open; `--advertise HOST` sets the"
    );
    println!("host in that link (a DNS name, for example). Anyone with the link can edit the open");
    println!("document until the studio stops; do not expose it to the internet.");
}

/// The host to advertise for a shared session: the explicit `--host`, or this machine's address on
/// the interface the default route uses.
fn share_host(bind: Option<IpAddr>) -> Option<String> {
    match bind {
        Some(ip) if !ip.is_unspecified() => Some(ip.to_string()),
        _ => lan_address().map(|ip| ip.to_string()),
    }
}

/// This host's outbound address, without sending a packet (a UDP connect only consults the routing
/// table). `None` when there is no route, in which case the bound address is advertised instead.
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip)
    }
}

fn parse_triple(value: &str) -> Result<[f64; 3], String> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(format!(
            "`--size {value}` needs three comma-separated numbers"
        ));
    }
    let mut out = [0.0f64; 3];
    for (index, part) in parts.iter().enumerate() {
        let number = part
            .parse::<f64>()
            .map_err(|_| format!("`--size` component `{part}` is not a number"))?;
        if !number.is_finite() || number <= 0.0 {
            return Err(format!(
                "`--size` component {} must be a finite positive number",
                index + 1
            ));
        }
        out[index] = number;
    }
    Ok(out)
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("untitled")
        .to_string()
}
