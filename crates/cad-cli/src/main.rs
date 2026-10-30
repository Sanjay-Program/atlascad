//! `atlas` — the headless CAD command line.
//!
//! ```text
//! atlas capabilities
//! atlas box   --size 10,10,5 [--unit mm] [--tol 0.05] [--out box.stl] [--header box.json]
//! atlas union --a 10,10,10 --b 4,4,10 [--offset 8,8,0] [--unit mm] [--tol 0.05] [--out u.stl]
//! atlas draft --file part.zcad [--save part.zdraft] [--out part.dxf]
//! atlas dxf   --from sheet.zdraft --out sheet.dxf
//! atlas dxf-import --from sheet.dxf [--save sheet.zdraft] [--out sheet.dxf]
//! atlas sheet --file part.zcad --pdf part.pdf [--paper A3] [--scale fit|N]
//! ```
//!
//! The summary is printed to stdout as JSON; progress and file writes go to stderr, so a caller can
//! pipe stdout into another tool.

use cad_cli::{
    BoxSpec, Built, CliError, build_box, build_union, draft_plan_view, draft_summary, draft_to_dxf,
    evaluate_project, import_dxf, plot_sheet, write_draft, write_stl_binary,
};
use cad_contract::ids::DocumentId;
use cad_contract::kernel::Kernel;
use cad_contract::tol::DEFAULT_LINEAR_MM;
use cad_contract::units::Unit;
use cad_document::Project;
use cad_kernel_truck::TruckKernel;
use std::path::{Path, PathBuf};

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("atlas: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32, CliError> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".to_string());
    let rest: Vec<String> = arguments.collect();

    match command.as_str() {
        "capabilities" => cmd_capabilities(),
        "box" => cmd_box(&rest),
        "union" => cmd_union(&rest),
        "project" => cmd_project(&rest),
        "draft" => cmd_draft(&rest),
        "dxf" => cmd_dxf(&rest),
        "dxf-import" => cmd_dxf_import(&rest),
        "sheet" => cmd_sheet(&rest),
        "help" | "--help" | "-h" => {
            usage();
            Ok(0)
        }
        other => Err(CliError::Input(format!(
            "unknown command `{other}`; run `atlas help`"
        ))),
    }
}

fn usage() {
    println!("atlas {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  atlas capabilities");
    println!(
        "  atlas box   --size X,Y,Z [--unit mm|cm|m|in|ft] [--tol MM] [--out PATH] [--header PATH]"
    );
    println!(
        "  atlas union --a X,Y,Z --b X,Y,Z [--offset X,Y,Z] [--unit UNIT] [--tol MM] [--out PATH]"
    );
    println!("  atlas project new --file PATH [--name NAME] [--size X,Y,Z] [--unit UNIT]");
    println!("  atlas project set --file PATH --param NAME --value NUMBER");
    println!("  atlas project undo --file PATH");
    println!("  atlas project redo --file PATH");
    println!("  atlas project export --file PATH --out STL");
    println!("  atlas draft --file PROJECT.zcad [--save DRAWING.zdraft] [--out DRAWING.dxf]");
    println!("  atlas dxf   --from DRAWING.zdraft --out DRAWING.dxf");
    println!("  atlas dxf-import --from DRAWING.dxf [--save DRAWING.zdraft] [--out DRAWING.dxf]");
    println!(
        "  atlas sheet --file PROJECT.zcad|DRAWING.zdraft --pdf SHEET.pdf [--paper A3] [--scale fit|N]"
    );
    println!(
        "              [--orientation landscape|portrait] [--title T] [--number N] [--revision R]"
    );
    println!(
        "              [--author A] [--date D] [--notes N] [--margin MM] [--dimensions on|off]"
    );
    println!();
    println!("Geometry is modelled in millimetres; --unit describes the values you typed.");
    println!("STL carries no units, so write --header next to it when the unit matters.");
    println!("DXF is written as R12 (AC1009) in the drawing's own unit.");
}

fn cmd_capabilities() -> Result<i32, CliError> {
    let kernel = TruckKernel::new();
    let info = kernel.info();
    println!("kernel: {} {}", info.name, info.version);
    println!(
        "supports_boolean_difference: {}",
        info.supports(cad_contract::capability::Capability::BooleanDifference)
    );
    for capability in info.capabilities.iter() {
        println!("capability: {}", capability.label());
    }
    Ok(0)
}

struct Flags {
    pairs: Vec<(String, String)>,
}

impl Flags {
    fn parse(arguments: &[String]) -> Result<Self, CliError> {
        let mut pairs = Vec::new();
        let mut index = 0;
        while index < arguments.len() {
            let key = arguments
                .get(index)
                .ok_or_else(|| CliError::Input("unexpected end of arguments".to_string()))?;
            if !key.starts_with("--") {
                return Err(CliError::Input(format!(
                    "unexpected argument `{key}`; flags start with --"
                )));
            }
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| CliError::Input(format!("`{key}` needs a value")))?;
            pairs.push((key.clone(), value.clone()));
            index += 2;
        }
        Ok(Flags { pairs })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    fn require(&self, key: &str) -> Result<&str, CliError> {
        self.get(key)
            .ok_or_else(|| CliError::Input(format!("`{key}` is required")))
    }

    fn unit(&self) -> Result<Unit, CliError> {
        match self.get("--unit") {
            None => Ok(Unit::Millimeter),
            Some(text) => {
                Unit::parse(text).ok_or_else(|| CliError::Input(format!("unknown unit `{text}`")))
            }
        }
    }

    fn tolerance(&self) -> Result<f64, CliError> {
        match self.get("--tol") {
            None => Ok(DEFAULT_LINEAR_MM),
            Some(text) => text
                .parse::<f64>()
                .map_err(|_| CliError::Input(format!("`--tol {text}` is not a number"))),
        }
    }
}

fn parse_triple(value: &str, what: &str) -> Result<[f64; 3], CliError> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(CliError::Input(format!(
            "{what} needs three comma-separated numbers, got `{value}`"
        )));
    }
    let mut out = [0.0f64; 3];
    for (axis, (slot, part)) in out.iter_mut().zip(parts).enumerate() {
        *slot = part.parse::<f64>().map_err(|_| {
            CliError::Input(format!(
                "{what} component {} (`{part}`) is not a number",
                axis + 1
            ))
        })?;
    }
    Ok(out)
}

fn cmd_box(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let size = parse_triple(flags.require("--size")?, "--size")?;
    let spec = BoxSpec::new(size, [0.0, 0.0, 0.0])?;
    let built = build_box(spec, flags.unit()?, flags.tolerance()?)?;
    emit(&built, &flags)
}

fn cmd_union(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let a_size = parse_triple(flags.require("--a")?, "--a")?;
    let b_size = parse_triple(flags.require("--b")?, "--b")?;
    let offset = match flags.get("--offset") {
        Some(value) => parse_triple(value, "--offset")?,
        None => [0.0, 0.0, 0.0],
    };
    let a = BoxSpec::new(a_size, [0.0, 0.0, 0.0])?;
    let b = BoxSpec::new(b_size, offset)?;
    let built = build_union(a, b, flags.unit()?, flags.tolerance()?)?;
    emit(&built, &flags)
}

fn cmd_project(arguments: &[String]) -> Result<i32, CliError> {
    let (subcommand, rest) = arguments.split_first().ok_or_else(|| {
        CliError::Input("project needs a subcommand: new, set, undo, redo, export".into())
    })?;
    match subcommand.as_str() {
        "new" => cmd_project_new(rest),
        "set" => cmd_project_set(rest),
        "undo" => cmd_project_undo(rest),
        "redo" => cmd_project_redo(rest),
        "export" => cmd_project_export(rest),
        other => Err(CliError::Input(format!(
            "unknown project subcommand `{other}`"
        ))),
    }
}

fn load_project(flags: &Flags) -> Result<(PathBuf, Project), CliError> {
    let path = PathBuf::from(flags.require("--file")?);
    let project = Project::load(&path)?;
    Ok((path, project))
}

fn save_and_summarise(path: &Path, project: &Project) -> Result<i32, CliError> {
    project.save(path)?;
    let (evaluated, mut kernel) = evaluate_project(project)?;
    kernel.release(evaluated.solid);
    println!(
        "{{\n  \"file\": \"{}\",\n  \"revision\": {},\n  \"volume_mm3\": {},\n  \"triangles\": {}\n}}",
        path.display(),
        project.revision.get(),
        evaluated.volume_mm3,
        evaluated.triangles
    );
    Ok(0)
}

fn cmd_project_new(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let path = PathBuf::from(flags.require("--file")?);
    let name = flags.get("--name").unwrap_or("untitled").to_string();
    let size = match flags.get("--size") {
        Some(value) => parse_triple(value, "--size")?,
        None => [10.0, 10.0, 5.0],
    };
    let project = Project::box_part(name, DocumentId::new(1), flags.unit()?, size)?;
    save_and_summarise(&path, &project)
}

fn cmd_project_set(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let (path, mut project) = load_project(&flags)?;
    let name = flags.require("--param")?;
    let value = flags.require("--value")?;
    let value = value
        .parse::<f64>()
        .map_err(|_| CliError::Input(format!("parameter value `{value}` is not a number")))?;
    project.set_parameter(name, value)?;
    save_and_summarise(&path, &project)
}

fn cmd_project_undo(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let (path, mut project) = load_project(&flags)?;
    project.undo()?;
    save_and_summarise(&path, &project)
}

fn cmd_project_redo(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let (path, mut project) = load_project(&flags)?;
    project.redo()?;
    save_and_summarise(&path, &project)
}

fn cmd_project_export(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let (_path, project) = load_project(&flags)?;
    let out = PathBuf::from(flags.require("--out")?);
    let (evaluated, mut kernel) = evaluate_project(&project)?;
    let bytes = write_stl_binary(&evaluated.mesh, &out)?;
    kernel.release(evaluated.solid);
    eprintln!(
        "wrote {} ({bytes} bytes, volume {} mm3)",
        out.display(),
        evaluated.volume_mm3
    );
    Ok(0)
}

/// `atlas draft --file part.zcad [--save part.zdraft] [--out part.dxf]`
///
/// Builds the plan-view drawing of a project (the profile the body was extruded from, in
/// millimetres), optionally saves it as a native `.zdraft` and optionally exports DXF in the same
/// run. At least one output is required: a command that computed a drawing and wrote nothing would
/// silently look like success.
fn cmd_draft(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let file = Path::new(flags.require("--file")?);
    // Check the kind of file before loading it: a `.zdraft` parsed as a project fails on schema
    // validation with a message about sketches, which hides the real mistake.
    if file.extension().and_then(|e| e.to_str()) == Some("zdraft") {
        return Err(CliError::Input(format!(
            "`{}` is a drawing, not a project; use `atlas dxf --from {0} --out sheet.dxf`",
            file.display()
        )));
    }
    let project = Project::load(file)?;
    let save = flags.get("--save").map(PathBuf::from);
    let out = flags.get("--out").map(PathBuf::from);
    if save.is_none() && out.is_none() {
        return Err(CliError::Input(
            "pass `--save part.zdraft`, `--out part.dxf`, or both".into(),
        ));
    }
    let draft = draft_plan_view(&project)?;
    let summary = write_draft(&draft, save.as_deref(), out.as_deref())?;
    print_json(&summary)?;
    eprintln!(
        "wrote {} ({} entities on {} layers, {} mm of geometry)",
        summary.file, summary.entities, summary.layers, summary.total_length
    );
    Ok(0)
}

/// `atlas dxf --from sheet.zdraft --out sheet.dxf`
fn cmd_dxf(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let from = PathBuf::from(flags.require("--from")?);
    let out = PathBuf::from(flags.require("--out")?);
    let summary = draft_to_dxf(&from, &out)?;
    print_json(&summary)?;
    let described = draft_summary(&from)?;
    eprintln!(
        "wrote {} from {} ({} entities, {} layers)",
        out.display(),
        from.display(),
        described.entities,
        described.layers
    );
    Ok(0)
}

/// `atlas dxf-import --from sheet.dxf [--save sheet.zdraft] [--out normalised.dxf]`
///
/// Reads a DXF written elsewhere. Anything this model cannot represent is printed in the summary's
/// `skipped` list rather than disappearing, so a caller can decide whether a partial drawing is
/// acceptable before it relies on one.
fn cmd_dxf_import(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let from = PathBuf::from(flags.require("--from")?);
    let save = flags.get("--save").map(PathBuf::from);
    let out = flags.get("--out").map(PathBuf::from);
    if save.is_none() && out.is_none() {
        return Err(CliError::Input(
            "pass `--save sheet.zdraft`, `--out sheet.dxf`, or both".into(),
        ));
    }
    let summary = import_dxf(&from, save.as_deref(), out.as_deref())?;
    print_json(&summary)?;
    let skipped: usize = summary.skipped.iter().map(|entry| entry.count).sum();
    if skipped > 0 {
        let kinds: Vec<String> = summary
            .skipped
            .iter()
            .map(|entry| format!("{} x{}", entry.kind, entry.count))
            .collect();
        eprintln!(
            "read {} of {} entities; {} entit(ies) not represented: {}",
            summary.draft.entities,
            summary.draft.entities + skipped,
            skipped,
            kinds.join(", ")
        );
    } else {
        eprintln!(
            "read {} entities on {} layers from {}",
            summary.draft.entities,
            summary.draft.layers,
            from.display()
        );
    }
    Ok(0)
}

/// `atlas sheet --file part.zcad --pdf part.pdf [--paper A3] [--scale fit|N] ...`
///
/// The default scale is `fit`: a standard ratio (1:1, 1:2, 1:5 …) chosen so the drawing fills the
/// page. An explicit `--scale 2` means 1:2 and is refused when the drawing does not fit, naming the
/// smallest standard scale that does.
fn cmd_sheet(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let from = PathBuf::from(flags.require("--file")?);
    let out = PathBuf::from(flags.require("--pdf")?);

    let mut sheet = cad_drafting::Sheet::new(flags.get("--title").unwrap_or_default());
    sheet.scale = None;
    if let Some(paper) = flags.get("--paper") {
        sheet.paper = cad_drafting::PaperSize::parse(paper).ok_or_else(|| {
            CliError::Input(format!(
                "`{paper}` is not a paper size; use A4, A3, A2, A1, A0 or Letter"
            ))
        })?;
    }
    if let Some(orientation) = flags.get("--orientation") {
        sheet.orientation = cad_drafting::Orientation::parse(orientation).ok_or_else(|| {
            CliError::Input(format!(
                "`{orientation}` is not an orientation; use landscape or portrait"
            ))
        })?;
    }
    if let Some(scale) = flags.get("--scale")
        && !scale.eq_ignore_ascii_case("fit")
    {
        let value: f64 = scale
            .parse()
            .map_err(|_| CliError::Input(format!("scale `{scale}` is not a number")))?;
        sheet.scale = Some(value);
    }
    if let Some(margin) = flags.get("--margin") {
        sheet.margin_mm = margin
            .parse()
            .map_err(|_| CliError::Input(format!("margin `{margin}` is not a number")))?;
    }
    for (name, field) in [
        ("--number", &mut sheet.number),
        ("--revision", &mut sheet.revision),
        ("--author", &mut sheet.author),
        ("--date", &mut sheet.date),
        ("--notes", &mut sheet.notes),
    ] {
        if let Some(value) = flags.get(name) {
            *field = value.to_string();
        }
    }
    if let Some(dimensions) = flags.get("--dimensions") {
        sheet.dimensions = !matches!(dimensions, "off" | "0" | "false" | "no");
    }

    let summary = plot_sheet(&from, &sheet, &out)?;
    print_json(&summary)?;
    eprintln!(
        "wrote {} ({} at {}, {} lines, {} texts)",
        out.display(),
        summary.paper,
        summary.scale,
        summary.lines,
        summary.texts
    );
    Ok(0)
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::Input(format!("cannot serialise the summary: {error}")))?;
    println!("{json}");
    Ok(())
}

fn emit(built: &Built, flags: &Flags) -> Result<i32, CliError> {
    let json = serde_json::to_string_pretty(&built.summary)
        .map_err(|error| CliError::Input(format!("cannot serialise the summary: {error}")))?;
    println!("{json}");

    if let Some(path) = flags.get("--out") {
        let bytes = write_stl_binary(&built.mesh, Path::new(path))?;
        eprintln!("wrote {path} ({bytes} bytes, binary STL)");
    }
    if let Some(path) = flags.get("--header") {
        let header = built.header_json()?;
        let path = PathBuf::from(path);
        std::fs::write(&path, header).map_err(|source| CliError::Io {
            path: path.display().to_string(),
            source,
        })?;
        eprintln!("wrote {}", path.display());
    }
    Ok(0)
}
