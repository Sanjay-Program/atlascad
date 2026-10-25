//! Launch the local Atlas CAD studio.
//!
//! ```text
//! cad-studio --file part.zcad
//! cad-studio --new part.zcad --name bracket --size 40,30,10
//! cad-studio --file part.zcad --port 7600
//! ```

use cad_studio::{DEFAULT_PORT, default_project, start};
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

    let project = if path.exists() {
        if create {
            return Err(format!(
                "{} already exists; refusing to overwrite it with --new",
                path.display()
            ));
        }
        cad_document::Project::load(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?
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

    let handle = start(project, path.clone(), port).map_err(|error| error.to_string())?;
    println!("Atlas CAD Studio");
    println!("  file: {}", path.display());
    println!("  url:  {}", handle.url());
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
    println!("  cad-studio --file PATH [--port PORT]");
    println!("  cad-studio --new PATH [--name NAME] [--size X,Y,Z] [--port PORT]");
    println!();
    println!("The studio binds 127.0.0.1 only. Geometry stays on this machine.");
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
