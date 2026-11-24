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
    let mut token_file: Option<PathBuf> = None;
    let mut auto_open = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--open" => auto_open = true,
            "--file" | "--project" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--file` (or `--project`) needs a path".to_string())?;
                file = Some(PathBuf::from(value));
            }
            "--listen" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--listen` needs an address or port (e.g. 127.0.0.1:8080 or 8080)".to_string())?;
                if let Some((ip_str, port_str)) = value.split_once(':') {
                    bind = Some(
                        ip_str
                            .parse::<IpAddr>()
                            .map_err(|_| format!("`--listen {value}` has an invalid IP address"))?,
                    );
                    let p = port_str
                        .parse::<u16>()
                        .map_err(|_| format!("`--listen {value}` has an invalid port number"))?;
                    if p == 0 {
                        return Err("port 0 is reserved for tests; choose a real port".into());
                    }
                    port = p;
                } else if let Ok(p) = value.parse::<u16>() {
                    if p == 0 {
                        return Err("port 0 is reserved for tests; choose a real port".into());
                    }
                    port = p;
                } else if let Ok(ip) = value.parse::<IpAddr>() {
                    bind = Some(ip);
                } else {
                    return Err(format!("`--listen {value}` is not a valid address or port"));
                }
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
                token = Some(validate_token(&value, "`--token`")?);
            }
            "--token-file" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--token-file` needs a path".to_string())?;
                token_file = Some(PathBuf::from(value));
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

    // Check environment variables for SaaS / Cloud container deployments:
    if port == DEFAULT_PORT {
        if let Ok(val) = std::env::var("PORT").or_else(|_| std::env::var("ATLAS_PORT")) {
            if let Ok(p) = val.trim().parse::<u16>() {
                if p > 0 {
                    port = p;
                }
            }
        }
    }

    if bind.is_none() {
        if let Ok(val) = std::env::var("HOST").or_else(|_| std::env::var("ATLAS_HOST")) {
            if let Ok(ip) = val.trim().parse::<IpAddr>() {
                bind = Some(ip);
            }
        } else if let Ok(val) = std::env::var("ATLAS_LISTEN") {
            if let Some((ip_str, port_str)) = val.split_once(':') {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    bind = Some(ip);
                }
                if let Ok(p) = port_str.parse::<u16>() {
                    if p > 0 {
                        port = p;
                    }
                }
            } else if let Ok(p) = val.trim().parse::<u16>() {
                if p > 0 {
                    port = p;
                }
            } else if let Ok(ip) = val.trim().parse::<IpAddr>() {
                bind = Some(ip);
            }
        }
    }

    if advertise.is_none() {
        if let Ok(val) =
            std::env::var("STUDIO_PUBLIC_HOST").or_else(|_| std::env::var("ATLAS_PUBLIC_HOST"))
        {
            if !val.trim().is_empty() {
                advertise = Some(val.trim().to_string());
            }
        }
    }

    if file.is_none() {
        if let Ok(val) = std::env::var("ATLAS_PROJECT").or_else(|_| std::env::var("PROJECT_FILE"))
        {
            if !val.trim().is_empty() {
                file = Some(PathBuf::from(val.trim()));
            }
        }
    }

    let is_default_fallback = file.is_none();
    let path = file.unwrap_or_else(|| PathBuf::from("workspace.atlas"));

    if share && bind.is_none() {
        bind = Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    // `--share --host 127.0.0.1` is the reverse-proxy deployment from deploy/: the socket stays on
    // loopback and the token is still required, because the public name reaches it through Caddy.
    let shared = share || bind.is_some_and(|ip| !ip.is_loopback());

    // A deployed instance should not carry its secret in `ps` or in a shell history. The token can
    // come from a file or the environment instead, and when it does the printed link is redacted
    // because the operator already holds the secret in a file they control.
    let mut token_origin = "option";
    if token.is_none()
        && let Some(path) = &token_file
    {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read the token file {}: {error}", path.display()))?;
        token = Some(
            validate_token(&text, "the token in the token file")
                .map_err(|error| format!("{}: {error}", path.display()))?,
        );
        token_origin = "file";
    }
    if token.is_none()
        && let Ok(value) = std::env::var("ATLAS_STUDIO_TOKEN")
        && !value.trim().is_empty()
    {
        token = Some(validate_token(&value, "ATLAS_STUDIO_TOKEN")?);
        token_origin = "environment";
    }
    if (token.is_some() || token_file.is_some()) && !shared {
        return Err(
            "a token only applies to a shared studio; add `--share` or `--host <address>`".into(),
        );
    }
    let advertised = if shared {
        advertise.or_else(|| share_host(bind))
    } else {
        None
    };
    if shared && advertised.is_none() && bind.is_some_and(|ip| ip.is_loopback()) {
        return Err(
            "`--share` on loopback needs `--advertise HOST`: the link has to name the proxy\n\
             in front of this studio, and no address on this machine can be discovered for it"
                .into(),
        );
    }
    if shared && advertised.is_none() {
        eprintln!(
            "cad-studio: no network address found; pass `--host <this machine's IP>` so the share link is usable"
        );
    }

    let mut recovered = None;
    let project = if path.exists() {
        if create && !is_default_fallback {
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
        if token_origin == "option" {
            println!("  share: {share_url}");
        } else {
            println!("  share: {}", redact_token(&share_url));
            println!(
                "         token: loaded from the {token_origin}; append ?token=... to the link (it is not printed here)"
            );
        }
        println!("         anyone with this link can view and edit until you stop the studio");
    }
    println!("  stop: Ctrl+C");
    println!();

    if auto_open {
        open_browser(&handle.url());
    }

    loop {
        std::thread::park();
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "start", url]).spawn();
    }
}

/// A token must be long enough to be worth guessing and must survive being put in a URL.
fn validate_token(value: &str, origin: &str) -> Result<String, String> {
    let token = value.trim();
    if token.len() < 16 {
        return Err(format!("{origin} must be at least 16 characters"));
    }
    if token.chars().count() > 128 {
        return Err(format!("{origin} must be at most 128 characters"));
    }
    if token
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(format!(
            "{origin} must not contain whitespace or control characters"
        ));
    }
    Ok(token.to_string())
}

/// The share link without its token, for a deployment whose token lives in a file or the environment.
fn redact_token(url: &str) -> String {
    match url.split_once("?token=") {
        Some((base, _)) => format!("{base}?token=<from token file>"),
        None => url.to_string(),
    }
}

fn usage() {
    println!("cad-studio {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  cad-studio [--open] [--file PATH] [--port PORT] [--listen [IP:]PORT] [--share]");
    println!("  cad-studio --new PATH [--name NAME] [--size X,Y,Z] [--port PORT] [--listen [IP:]PORT]");
    println!();
    println!("OPTIONS:");
    println!("  --open                 Automatically open the studio in the default web browser");
    println!("  --file, --project PATH Open an existing engineering project (default: workspace.atlas)");
    println!("  --new [PATH]           Create a new project file (optionally with --name and --size)");
    println!("  --listen [IP:]PORT     Bind address and port (e.g. 127.0.0.1:8080 or 8080)");
    println!("  --port PORT            Set port (default: 7519, or $PORT / $ATLAS_PORT)");
    println!("  --host IP              Set bind IP address (default: 127.0.0.1, or $HOST / $ATLAS_HOST)");
    println!("  --share                Bind 0.0.0.0 with secure token for LAN / remote collaboration");
    println!("  --advertise HOST       Set public hostname in generated share link ($STUDIO_PUBLIC_HOST)");
    println!("  --token TOKEN          Specify security token ($ATLAS_STUDIO_TOKEN)");
    println!("  --token-file PATH      Load security token from file");
    println!("  --help, -h             Print this help guide");
    println!();
    println!("ENVIRONMENT VARIABLES:");
    println!("  PORT, ATLAS_PORT       Listening port");
    println!("  HOST, ATLAS_HOST       Bind IP address");
    println!("  ATLAS_LISTEN           Full listen string (e.g. 0.0.0.0:8080)");
    println!("  ATLAS_PROJECT          Default project path to open or create");
    println!("  ATLAS_STUDIO_TOKEN     Bearer security token for shared instances");
    println!("  STUDIO_PUBLIC_HOST     Public domain / URL for reverse proxy (Caddy / Nginx)");
}

/// The host to advertise for a shared session: the explicit `--host`, or this machine's address on
/// the interface the default route uses.
fn share_host(bind: Option<IpAddr>) -> Option<String> {
    match bind {
        // A loopback bind is never what a colleague should type: the link would point at their own
        // machine. The discoverable LAN address is advertised instead.
        Some(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip.to_string()),
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
