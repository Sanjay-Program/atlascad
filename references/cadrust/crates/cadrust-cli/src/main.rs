//! cadrust CLI. Every subcommand is a thin wrapper over cadrust-core so the web app
//! and the command line cannot diverge.

mod serve;

use anyhow::{bail, Context, Result};
use cadrust_core::{evaluate, Document, Solid};
use clap::{Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "cadrust", version, about = "Parametric CAD kernel CLI on OpenCascade")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Evaluate a .cadrust.json document and export the result.
    Run {
        doc: PathBuf,
        /// Override a parameter, e.g. --set width=80 (repeatable).
        #[arg(long = "set", value_parser = parse_kv)]
        set: Vec<(String, String)>,
        /// Output file; format from extension (.step .glb .stl .brep .png). Repeatable.
        #[arg(long = "export", short = 'o')]
        export: Vec<PathBuf>,
        /// Print body info as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Convert between STEP / BRep / glTF / STL.
    Convert {
        input: PathBuf,
        #[arg(long, short = 'o')]
        out: PathBuf,
    },
    /// Wrap a STEP / BRep file as an `Import` feature: a new document, or appended to `--into`.
    Import {
        input: PathBuf,
        #[arg(long, short = 'o')]
        out: PathBuf,
        /// Existing document to append the feature to (written to `--out`).
        #[arg(long)]
        into: Option<PathBuf>,
        /// Feature id; default is the file stem.
        #[arg(long)]
        id: Option<String>,
    },
    /// Print the feature dependency graph of a document as Graphviz DOT.
    Graph {
        input: PathBuf,
    },
    /// Print bodies, volume, bounding box of a STEP/BRep/.cadrust.json file.
    Info {
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Serve the web app for a document.
    Serve {
        doc: PathBuf,
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Bind address; use 0.0.0.0 to reach the app from another machine on the LAN.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Directory of add-ins (<id>.wasm or <id>/manifest.json).
        #[arg(long, default_value = "addins/dist")]
        addins: PathBuf,
    },
    /// Inspect or run a wasm add-in on a document.
    Addin {
        #[command(subcommand)]
        cmd: AddinCmd,
    },
}

#[derive(Subcommand)]
enum AddinCmd {
    /// Print the add-in manifest (id, commands, parameters).
    Info { wasm: PathBuf },
    /// Run one add-in command on a document and write the resulting document.
    Run {
        wasm: PathBuf,
        doc: PathBuf,
        /// Command id from the manifest; defaults to the first one.
        #[arg(long)]
        command: Option<String>,
        /// Parameter, e.g. --param teeth=24 (repeatable).
        #[arg(long = "param", value_parser = parse_kv)]
        param: Vec<(String, String)>,
        /// Output document; defaults to overwriting the input.
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
    },
}

fn parse_kv(s: &str) -> std::result::Result<(String, String), String> {
    s.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).ok_or_else(|| format!("expected name=value, got '{s}'"))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { doc, set, export, json } => {
            let document = load_doc(&doc)?;
            let overrides: BTreeMap<String, String> = set.into_iter().collect();
            let ev = evaluate(&document, &overrides)?;
            for s in &ev.status {
                let mark = if s.ok { "ok " } else { "ERR" };
                eprintln!("{mark} {:<10} {:<14} {:>5} ms {}", s.kind, s.id, s.millis, s.message);
            }
            for path in &export {
                write_solids(&ev.solids(), path)?;
                eprintln!("wrote {}", path.display());
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": ev.status, "bodies": ev.info(), "edges": ev.edges(), "sketches": ev.sketches }))?);
            } else {
                for b in ev.info() {
                    println!("{:<12} volume {:>12.3}  faces {:>4}  bbox {:?} .. {:?}", b.id, b.volume, b.faces, round3(b.bbox_min), round3(b.bbox_max));
                }
            }
            if ev.has_errors() {
                std::process::exit(1);
            }
        }
        Cmd::Convert { input, out } => {
            let solids = load_solids(&input)?;
            write_solids(&solids.iter().collect::<Vec<_>>(), &out)?;
            eprintln!("wrote {} ({} bodies)", out.display(), solids.len());
        }
        Cmd::Import { input, out, into, id } => {
            let bytes = std::fs::read(&input).with_context(|| format!("read {}", input.display()))?;
            let (data, n) = serve::import_to_brep(&ext(&input), &bytes).map_err(|e| anyhow::anyhow!(e))?;
            let mut doc = match &into {
                Some(p) => load_doc(p)?,
                None => Document::default(),
            };
            let id = id.unwrap_or_else(|| input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "import".into()));
            let source = input.file_name().map(|s| s.to_string_lossy().to_string());
            doc.timeline.push(cadrust_core::Feature::Import { id: id.clone(), source, data, at: Default::default(), op: Default::default(), target: None });
            std::fs::write(&out, doc.to_json()?).with_context(|| format!("write {}", out.display()))?;
            eprintln!("wrote {} ({n} solids as feature '{id}')", out.display());
        }
        Cmd::Graph { input } => {
            print!("{}", cadrust_core::graph::dot(&load_doc(&input)?));
        }
        Cmd::Info { input, json } => {
            let solids = load_solids(&input)?;
            let info: Vec<_> = solids.iter().enumerate().map(|(i, s)| cadrust_core::eval::body_info(&format!("body{i}"), s)).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                for b in info {
                    println!("{:<12} volume {:>12.3}  area {:>12.3}  faces {:>4}  edges {:>4}", b.id, b.volume, b.area, b.faces, b.edges);
                }
            }
        }
        Cmd::Serve { doc, port, host, addins } => {
            let document = load_doc(&doc)?;
            serve::run(document, doc, addins, &host, port)?;
        }
        Cmd::Addin { cmd: AddinCmd::Info { wasm } } => {
            let mut addin = cadrust_addin::Addin::load_file(&wasm)?;
            println!("{}", serde_json::to_string_pretty(&addin.manifest()?)?);
        }
        Cmd::Addin { cmd: AddinCmd::Run { wasm, doc, command, param, out } } => {
            let mut addin = cadrust_addin::Addin::load_file(&wasm)?;
            let manifest = addin.manifest()?;
            let command = match command {
                Some(c) => c,
                None => manifest.commands.first().map(|c| c.id.clone()).context("add-in has no commands")?,
            };
            let spec = manifest.commands.iter().find(|c| c.id == command).with_context(|| format!("unknown command '{command}'"))?;
            // Start from the manifest defaults, then apply --param values (numbers stay numbers).
            let mut params = serde_json::Map::new();
            for p in &spec.params {
                params.insert(p.k.clone(), p.default.clone());
            }
            for (k, v) in param {
                let val = v.parse::<f64>().map(serde_json::Value::from).unwrap_or(serde_json::Value::String(v));
                params.insert(k, val);
            }
            let text = std::fs::read_to_string(&doc).with_context(|| format!("read {}", doc.display()))?;
            let document: serde_json::Value = serde_json::from_str(&text)?;
            let resp = addin.run(&cadrust_addin::Request { command: &command, params: serde_json::Value::Object(params), document: &document, selection: vec![] })?;
            if let Some(m) = &resp.message {
                eprintln!("{}: {m}", manifest.id);
            }
            let result = resp.document.context("add-in returned no document")?;
            // Round-trip through the typed document so invalid output fails here, not later.
            let typed = Document::from_json(&result.to_string())?;
            let target = out.unwrap_or(doc);
            std::fs::write(&target, typed.to_json()?)?;
            eprintln!("wrote {}", target.display());
        }
    }
    Ok(())
}

fn round3(v: [f64; 3]) -> [f64; 3] {
    v.map(|x| (x * 1000.0).round() / 1000.0)
}

fn load_doc(path: &Path) -> Result<Document> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(Document::from_json(&text)?)
}

fn ext(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn load_solids(path: &Path) -> Result<Vec<Solid>> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    match ext(path).as_str() {
        "step" | "stp" => Ok(Solid::read_step(&mut file).map_err(|e| anyhow::anyhow!("STEP read failed: {e:?}"))?),
        "brep" => Ok(Solid::read_brep(&mut file).map_err(|e| anyhow::anyhow!("BRep read failed: {e:?}"))?),
        "json" => {
            let ev = evaluate(&load_doc(path)?, &BTreeMap::new())?;
            Ok(ev.bodies.into_iter().map(|(_, s)| s).collect())
        }
        other => bail!("unsupported input format '{other}'"),
    }
}

pub fn write_solids(solids: &[&Solid], path: &Path) -> Result<()> {
    let mut file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let it = solids.iter().copied();
    match ext(path).as_str() {
        "step" | "stp" => Solid::write_step(it, &mut file).map_err(|e| anyhow::anyhow!("STEP write failed: {e:?}"))?,
        "brep" => Solid::write_brep(it, &mut file).map_err(|e| anyhow::anyhow!("BRep write failed: {e:?}"))?,
        "glb" => Solid::mesh(it, Default::default()).and_then(|m| m.write_gltf_binary(&mut file)).map_err(|e| anyhow::anyhow!("glTF write failed: {e:?}"))?,
        "stl" => Solid::mesh(it, Default::default()).and_then(|m| m.write_stl(&mut file)).map_err(|e| anyhow::anyhow!("STL write failed: {e:?}"))?,
        "png" => Solid::mesh(it, Default::default())
            .and_then(|m| m.scene(cadrum::SceneOption { view: cadrum::DVec3::new(1.0, -1.0, 0.8), shading: true, ..Default::default() }).write_png([1200, 900], &mut file))
            .map_err(|e| anyhow::anyhow!("PNG write failed: {e:?}"))?,
        other => bail!("unsupported output format '{other}'"),
    }
    Ok(())
}
