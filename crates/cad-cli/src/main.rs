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
//! atlas artifacts add --registry part.artifacts.json --document 1 --revision 3
//!                     --id 1 --kind drawing --label "shop drawing"
//! atlas artifacts review --registry part.artifacts.json --id 1 --revision 4
//! atlas artifacts check --project part.zcad --registry part.artifacts.json
//! ```
//!
//! `artifacts check` exits 3 when any derived artifact needs recomputation or review, so a release
//! step can gate on it without parsing the JSON.
//!
//! The summary is printed to stdout as JSON; progress and file writes go to stderr, so a caller can
//! pipe stdout into another tool.

use cad_cae::{Axis, FaceLoad, FaceSelection, StaticStudy, Support, parse_study, run_static};
use cad_cli::{
    BoxSpec, Built, CliError, build_box, build_union, draft_plan_view, draft_summary, draft_to_dxf,
    evaluate_project, import_dxf, plot_sheet, write_draft, write_stl_binary,
};
use cad_contract::document::Revision;
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::Kernel;
use cad_contract::tol::DEFAULT_LINEAR_MM;
use cad_contract::units::Unit;
use cad_document::Project;
use cad_engineering_model::{
    ArtifactId, ArtifactKind, ArtifactRegistry, DerivedArtifact, DocumentRef, MaterialLibrary,
};
use cad_kernel_truck::TruckKernel;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("atlas: {error}");
            error.exit_code()
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
        "artifacts" => cmd_artifacts(&rest),
        "cae" => cmd_cae(&rest),
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
    println!(
        "  atlas artifacts add --project PROJECT.zcad --revision N --id ID --kind KIND [--label TEXT]"
    );
    println!("              [--entity ID[,ID...]] [--upstream ID[,ID...]] [--summary TEXT]");
    println!("  atlas artifacts review --project PROJECT.zcad --id ID --revision N");
    println!("  atlas artifacts check --project PROJECT.zcad");
    println!(
        "  atlas artifacts ... --registry FILE --document ID  (the older sidecar form; also accepted)"
    );
    println!(
        "  atlas cae static --file PROJECT.zcad --study STUDY.json --out RESULT.json [--record ID]"
    );
    println!(
        "  atlas cae static --file PROJECT.zcad --voxel MM --material ID --fix AXIS=COORD[@TOL]"
    );
    println!(
        "                   [--hold AXIS=COORD:COMPONENT] --load AXIS=COORD[@TOL] --force X,Y,Z"
    );
    println!(
        "                   [--out RESULT.json] [--record ARTIFACT_ID] [--label TEXT] [--name TEXT]"
    );
    println!("                   [--solver-tol T] [--max-iter N]");
    println!();
    println!("Exit: 0 ok, 1 internal or I/O fault, 2 bad input, 3 analysis refused.");
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

    fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.pairs
            .iter()
            .filter(move |(name, _)| name == key)
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

/// `atlas artifacts <add|review|check>` — revision-pinned derived-artifact tracking.
///
/// A toolpath, drawing, simulation or inspection report is not current merely because the file
/// exists. This command records which design revision produced it and refuses to call it released
/// when the design has moved on without a recompute or an explicit review.
fn cmd_artifacts(arguments: &[String]) -> Result<i32, CliError> {
    let (subcommand, rest) = arguments.split_first().ok_or_else(|| {
        CliError::Input("artifacts needs a subcommand: add, review or check".into())
    })?;
    match subcommand.as_str() {
        "add" => cmd_artifacts_add(rest),
        "review" => cmd_artifacts_review(rest),
        "check" => cmd_artifacts_check(rest),
        other => Err(CliError::Input(format!(
            "unknown artifacts subcommand `{other}`; use add, review or check"
        ))),
    }
}

fn cmd_artifacts_add(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    if flags.get("--project").is_some() {
        return cmd_artifacts_add_to_project(&flags);
    }
    let path = PathBuf::from(flags.require("--registry")?);
    let document = parse_document(&flags)?;
    let revision = parse_revision(&flags, "--revision")?;
    let id = parse_artifact_id(&flags, "--id")?;
    let kind_name = flags.require("--kind")?;
    let kind = ArtifactKind::parse(kind_name).ok_or_else(|| {
        CliError::Input(format!(
            "`{kind_name}` is not an artifact kind; use drawing, simulation, manufacturing_setup, \
             toolpath, inspection, pcb_layout, documentation or other"
        ))
    })?;
    let label = flags.require("--label")?;
    let mut registry = if path.exists() {
        ArtifactRegistry::load(&path)?
    } else {
        ArtifactRegistry::new(document)
    };
    let mut artifact =
        DerivedArtifact::new(id, kind, label, DocumentRef::new(document, revision, None));
    artifact.source_entities = parse_entities(flags.all("--entity"))?;
    artifact.upstream = parse_artifact_ids(flags.all("--upstream"))?;
    if let Some(summary) = flags.get("--summary") {
        artifact.result.summary = summary.to_string();
    }
    registry.add(artifact)?;
    registry.save(&path)?;
    print_json(&serde_json::json!({
        "registry": path.display().to_string(),
        "artifact": id.get(),
        "kind": kind.label(),
        "source_revision": revision.get(),
        "artifacts": registry.artifacts().len(),
    }))?;
    eprintln!(
        "recorded artifact {} ({}) at revision {} in {}",
        id.get(),
        kind.label(),
        revision.get(),
        path.display()
    );
    Ok(0)
}

/// Record an artifact in the project file itself, so it travels with the design.
fn cmd_artifacts_add_to_project(flags: &Flags) -> Result<i32, CliError> {
    let path = PathBuf::from(flags.require("--project")?);
    let mut project = Project::load(&path)?;
    let document = project.header.document;
    if let Some(named) = flags.get("--document") {
        let named = parse_document_value(named)?;
        if named != document {
            return Err(CliError::Input(format!(
                "`--document {}` does not match the project's document {}",
                named.get(),
                document.get()
            )));
        }
    }
    let revision = parse_revision(flags, "--revision")?;
    let id = parse_artifact_id(flags, "--id")?;
    let kind_name = flags.require("--kind")?;
    let kind = ArtifactKind::parse(kind_name).ok_or_else(|| {
        CliError::Input(format!(
            "`{kind_name}` is not an artifact kind; use drawing, simulation, manufacturing_setup, \
             toolpath, inspection, pcb_layout, documentation or other"
        ))
    })?;
    let label = flags.get("--label").unwrap_or(kind.label());
    let mut artifact =
        DerivedArtifact::new(id, kind, label, DocumentRef::new(document, revision, None));
    artifact.source_entities = parse_entities(flags.all("--entity"))?;
    artifact.upstream = parse_artifact_ids(flags.all("--upstream"))?;
    if let Some(summary) = flags.get("--summary") {
        artifact.result.summary = summary.to_string();
    }
    project.record_artifact(artifact)?;
    project.save(&path)?;
    let recorded = project
        .artifacts()
        .map(|registry| registry.artifacts().len());
    print_json(&serde_json::json!({
        "project": path.display().to_string(),
        "document": document.get(),
        "artifact": id.get(),
        "kind": kind.label(),
        "source_revision": revision.get(),
        "revision": project.revision.get(),
        "artifacts": recorded,
    }))?;
    eprintln!(
        "recorded artifact {} ({}) at revision {} in {}",
        id.get(),
        kind.label(),
        revision.get(),
        path.display()
    );
    Ok(0)
}

fn cmd_artifacts_review(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    if flags.get("--project").is_some() {
        let path = PathBuf::from(flags.require("--project")?);
        let id = parse_artifact_id(&flags, "--id")?;
        let revision = parse_revision(&flags, "--revision")?;
        let mut project = Project::load(&path)?;
        project.review_artifact(id, revision)?;
        project.save(&path)?;
        print_json(&serde_json::json!({
            "project": path.display().to_string(),
            "artifact": id.get(),
            "reviewed_revision": revision.get(),
        }))?;
        eprintln!(
            "artifact {} reviewed at revision {} in {} (the result was not recomputed)",
            id.get(),
            revision.get(),
            path.display()
        );
        return Ok(0);
    }
    let path = PathBuf::from(flags.require("--registry")?);
    let id = parse_artifact_id(&flags, "--id")?;
    let revision = parse_revision(&flags, "--revision")?;
    let mut registry = ArtifactRegistry::load(&path)?;
    registry.review(id, revision)?;
    registry.save(&path)?;
    print_json(&serde_json::json!({
        "registry": path.display().to_string(),
        "artifact": id.get(),
        "reviewed_revision": revision.get(),
    }))?;
    eprintln!(
        "artifact {} reviewed at revision {} (the result was not recomputed)",
        id.get(),
        revision.get()
    );
    Ok(0)
}

fn cmd_artifacts_check(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let project_path = PathBuf::from(flags.require("--project")?);
    let project = Project::load(&project_path)?;
    let report = match flags.get("--registry") {
        Some(registry_path) => {
            let registry = ArtifactRegistry::load(Path::new(registry_path))?;
            registry.assess(DocumentRef::new(
                project.header.document,
                project.revision,
                None,
            ))?
        }
        None => project.assess_artifacts()?.ok_or_else(|| {
            CliError::Input(
                "this project records no derived artifacts; record one with \
                 `atlas artifacts add --project`"
                    .into(),
            )
        })?,
    };
    print_json(&report)?;
    if report.requires_action {
        let stale = report
            .artifacts
            .iter()
            .filter(|artifact| !artifact.releasable)
            .count();
        eprintln!(
            "{stale} of {} derived artifacts need recomputation or review at revision {}",
            report.artifacts.len(),
            report.revision.get()
        );
        Ok(3)
    } else {
        eprintln!(
            "all {} derived artifacts are releasable at revision {}",
            report.artifacts.len(),
            report.revision.get()
        );
        Ok(0)
    }
}

fn cmd_cae(arguments: &[String]) -> Result<i32, CliError> {
    let (subcommand, rest) = arguments
        .split_first()
        .ok_or_else(|| CliError::Input("cae needs a subcommand: static".into()))?;
    match subcommand.as_str() {
        "static" => cmd_cae_static(rest),
        other => Err(CliError::Input(format!(
            "unknown cae subcommand `{other}`; use `atlas cae static`"
        ))),
    }
}

/// `atlas cae static` — one linear static study on the project's evaluated solid.
///
/// The study comes from a versioned `--study FILE` (what a deployment job passes) or from flags
/// (what a person types at a terminal); the two are exclusive. The result is printed as JSON and
/// can be written with `--out`. `--record ID` records (or replaces) a simulation artifact at the
/// project's current revision and saves the project, so the result travels with the design and the
/// release gate can tell when it has gone stale.
fn cmd_cae_static(arguments: &[String]) -> Result<i32, CliError> {
    let flags = Flags::parse(arguments)?;
    let path = PathBuf::from(flags.require("--file")?);
    if flags.get("--out").is_none() && flags.get("--record").is_none() {
        return Err(CliError::Input(
            "pass `--out result.json`, `--record ID`, or both".into(),
        ));
    }

    // A flag silently overriding a field of the study file would make the recorded inputs
    // disagree with the result, so mixing them is refused rather than resolved.
    let study = match flags.get("--study") {
        Some(study_path) => {
            reject_study_flags(&flags)?;
            read_study(study_path)?
        }
        None => study_from_flags(&flags)?,
    };

    let mut project = Project::load(&path)?;
    let (evaluated, mut kernel) = evaluate_project(&project)?;
    let library = MaterialLibrary::standard();
    let result = run_static(&evaluated.mesh, &study, &library);
    kernel.release(evaluated.solid);
    let result = result
        .map_err(|error| CliError::Analysis(format!("cae: {error}")))?
        .with_source(project.header.document, project.revision);

    if let Some(out) = flags.get("--out") {
        let out = PathBuf::from(out);
        let json = serde_json::to_string_pretty(&result)
            .map_err(|error| CliError::Analysis(format!("cannot serialise the result: {error}")))?;
        write_atomic(&out, json.as_bytes())?;
        eprintln!("wrote {} ({})", out.display(), result.summary_line());
    }
    if let Some(value) = flags.get("--record") {
        let raw = value
            .parse::<u64>()
            .map_err(|_| CliError::Input(format!("artifact id `{value}` is not a number")))?;
        let id = ArtifactId::new(raw)
            .ok_or_else(|| CliError::Input("`--record` needs a positive artifact id".into()))?;
        let label = flags.get("--label").unwrap_or("linear static study");
        let mut artifact = DerivedArtifact::new(
            id,
            ArtifactKind::Simulation,
            label,
            DocumentRef::new(project.header.document, project.revision, None),
        );
        artifact.source_entities = project
            .sketches()
            .iter()
            .map(|sketch| sketch.id)
            .chain(project.features().iter().map(|feature| feature.id))
            .collect();
        artifact.result.summary = result.summary_line();
        artifact.result.warnings = result.warnings.clone();
        project.record_artifact(artifact)?;
        project.save(&path)?;
        eprintln!(
            "recorded simulation artifact {} at revision {} in {}",
            id.get(),
            project.revision.get(),
            path.display()
        );
    }
    print_json(&result)?;
    Ok(0)
}

/// Flags that define the study themselves. They cannot accompany `--study`.
const STUDY_FLAGS: [&str; 9] = [
    "--voxel",
    "--material",
    "--fix",
    "--hold",
    "--load",
    "--force",
    "--solver-tol",
    "--max-iter",
    "--name",
];

fn reject_study_flags(flags: &Flags) -> Result<(), CliError> {
    if let Some((name, _)) = flags
        .pairs
        .iter()
        .find(|(name, _)| STUDY_FLAGS.contains(&name.as_str()))
    {
        return Err(CliError::Input(format!(
            "`{name}` cannot be combined with `--study`; the study file defines it"
        )));
    }
    Ok(())
}

/// Read a versioned study file. A malformed, oversized or newer file is bad input (exit 2); a valid
/// file whose study the model refuses is an analysis refusal (exit 3).
fn read_study(path: &str) -> Result<StaticStudy, CliError> {
    let path = PathBuf::from(path);
    let bytes = std::fs::read(&path).map_err(|source| CliError::Io {
        path: path.display().to_string(),
        source,
    })?;
    parse_study(&bytes).map_err(|error| match error {
        cad_cae::CaeError::StudyFile(message) => {
            CliError::Input(format!("study {}: {message}", path.display()))
        }
        other => CliError::Analysis(format!("study {}: {other}", path.display())),
    })
}

fn study_from_flags(flags: &Flags) -> Result<StaticStudy, CliError> {
    let voxel = flags
        .require("--voxel")?
        .parse::<f64>()
        .map_err(|_| CliError::Input("`--voxel` is not a number".into()))?;
    let material = flags.require("--material")?.to_string();
    let mut study = StaticStudy::new(
        flags.get("--name").unwrap_or("linear static"),
        voxel,
        material,
    );
    for value in flags.all("--fix") {
        let (axis, coordinate, tolerance) = parse_face_spec(value, "--fix")?;
        study
            .supports
            .push(Support::fixed(FaceSelection::with_tolerance(
                axis, coordinate, tolerance,
            )));
    }
    for value in flags.all("--hold") {
        let (spec, component) = value.split_once(':').ok_or_else(|| {
            CliError::Input(format!(
                "`--hold {value}` must be AXIS=COORD:COMPONENT, for example y=0:y"
            ))
        })?;
        let (axis, coordinate, tolerance) = parse_face_spec(spec, "--hold")?;
        let component = Axis::parse(component).ok_or_else(|| {
            CliError::Input(format!("`{component}` is not a held component x, y or z"))
        })?;
        study.supports.push(Support::hold(
            FaceSelection::with_tolerance(axis, coordinate, tolerance),
            component,
        ));
    }
    let (axis, coordinate, tolerance) = parse_face_spec(flags.require("--load")?, "--load")?;
    let force = parse_triple(flags.require("--force")?, "--force")?;
    study.loads.push(FaceLoad::new(
        FaceSelection::with_tolerance(axis, coordinate, tolerance),
        force,
    ));
    if let Some(value) = flags.get("--solver-tol") {
        study.solver.relative_tolerance = value
            .parse::<f64>()
            .map_err(|_| CliError::Input("`--solver-tol` is not a number".into()))?;
    }
    if let Some(value) = flags.get("--max-iter") {
        study.solver.max_iterations = value
            .parse::<usize>()
            .map_err(|_| CliError::Input("`--max-iter` is not an integer".into()))?;
    }
    study
        .validate()
        .map_err(|error| CliError::Analysis(format!("study: {error}")))?;
    Ok(study)
}

/// Publish a result file atomically: a watcher or supervisor never observes a half-written result,
/// and a kill during the write leaves the previous file untouched.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    static RESULT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        CliError::Input(format!("result path {} has no file name", path.display()))
    })?;
    let sequence = RESULT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{}.tmp-{}-{sequence}",
        name.to_string_lossy(),
        std::process::id()
    ));

    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|source| CliError::Io {
                path: temp.display().to_string(),
                source,
            })?;
        file.write_all(bytes).map_err(|source| CliError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        file.sync_all().map_err(|source| CliError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|source| CliError::Io {
            path: path.display().to_string(),
            source,
        })?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| CliError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        Ok(())
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Parse `AXIS=COORD[@TOL]` into a plane selection. The default tolerance is 1 µm.
fn parse_face_spec(value: &str, flag: &str) -> Result<(Axis, f64, f64), CliError> {
    let (plane, tolerance) = match value.split_once('@') {
        Some((plane, tolerance)) => (plane, Some(tolerance)),
        None => (value, None),
    };
    let (axis, coordinate) = plane.split_once('=').ok_or_else(|| {
        CliError::Input(format!(
            "`{flag} {value}` must be AXIS=COORD, for example x=0"
        ))
    })?;
    let axis = Axis::parse(axis)
        .ok_or_else(|| CliError::Input(format!("`{axis}` is not an axis; use x, y or z")))?;
    let coordinate = coordinate
        .parse::<f64>()
        .map_err(|_| CliError::Input(format!("`{coordinate}` is not a coordinate")))?;
    let tolerance = match tolerance {
        Some(value) => value
            .parse::<f64>()
            .map_err(|_| CliError::Input(format!("`{value}` is not a tolerance")))?,
        None => 1.0e-3,
    };
    Ok((axis, coordinate, tolerance))
}

fn parse_document(flags: &Flags) -> Result<DocumentId, CliError> {
    let value = flags.require("--document")?;
    parse_document_value(value)
}

fn parse_document_value(value: &str) -> Result<DocumentId, CliError> {
    let raw = value
        .parse::<u64>()
        .map_err(|_| CliError::Input(format!("document `{value}` is not a number")))?;
    Ok(DocumentId::new(raw))
}

fn parse_revision(flags: &Flags, key: &str) -> Result<Revision, CliError> {
    let value = flags.require(key)?;
    let raw = value
        .parse::<u64>()
        .map_err(|_| CliError::Input(format!("revision `{value}` is not a number")))?;
    Revision::from_raw(raw)
        .ok_or_else(|| CliError::Input(format!("`{key}` must be a positive revision")))
}

fn parse_artifact_id(flags: &Flags, key: &str) -> Result<ArtifactId, CliError> {
    let value = flags.require(key)?;
    let raw = value
        .parse::<u64>()
        .map_err(|_| CliError::Input(format!("artifact id `{value}` is not a number")))?;
    ArtifactId::new(raw)
        .ok_or_else(|| CliError::Input(format!("`{key}` must be a positive artifact id")))
}

fn parse_artifact_ids<'a>(
    values: impl Iterator<Item = &'a str>,
) -> Result<Vec<ArtifactId>, CliError> {
    let mut ids = Vec::new();
    for value in values {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let raw = part
                .parse::<u64>()
                .map_err(|_| CliError::Input(format!("upstream id `{part}` is not a number")))?;
            let id = ArtifactId::new(raw).ok_or_else(|| {
                CliError::Input("an upstream artifact id must be positive".into())
            })?;
            ids.push(id);
        }
    }
    Ok(ids)
}

fn parse_entities<'a>(values: impl Iterator<Item = &'a str>) -> Result<Vec<EntityId>, CliError> {
    let mut entities = Vec::new();
    for value in values {
        for part in value.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let raw = part
                .parse::<u64>()
                .map_err(|_| CliError::Input(format!("entity id `{part}` is not a number")))?;
            let id = EntityId::new(raw).map_err(|error| CliError::Input(error.to_string()))?;
            entities.push(id);
        }
    }
    Ok(entities)
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
