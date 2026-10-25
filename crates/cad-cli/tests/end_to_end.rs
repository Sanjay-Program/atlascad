//! End-to-end tests: units → document header → kernel → mesh → STL on disk → mesh again.
//!
//! The assertions are analytic (a box's volume is `x*y*z`), not self-consistency checks against the
//! same code path that produced the value.

use cad_cli::{
    BoxSpec, build_box, build_union, evaluate_project, read_stl_binary, stl_bytes, write_stl_binary,
};
use cad_contract::document::{CURRENT_SCHEMA, DocumentHeader};
use cad_contract::ids::DocumentId;
use cad_contract::kernel::Kernel;
use cad_contract::units::Unit;
use cad_document::Project;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> std::io::Result<Self> {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("cad-cli-{tag}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(TempDir(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn box_spec(size: [f64; 3]) -> Result<BoxSpec, Box<dyn std::error::Error>> {
    Ok(BoxSpec::new(size, [0.0, 0.0, 0.0])?)
}

/// Weld vertices by position, the way an STL consumer does.
///
/// truck's tessellation emits per-face vertices, so a raw `open_edges()` count on the kernel mesh
/// reports every edge as unshared. That is a representation fact, not a hole in the solid; welding
/// first makes the watertightness check meaningful.
fn welded(mesh: &cad_contract::mesh::Mesh) -> cad_contract::mesh::Mesh {
    let mut map: std::collections::BTreeMap<(i64, i64, i64), u32> =
        std::collections::BTreeMap::new();
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut remap = vec![0u32; mesh.positions.len()];

    for (index, point) in mesh.positions.iter().enumerate() {
        let key = (
            (point[0] * 1e6).round() as i64,
            (point[1] * 1e6).round() as i64,
            (point[2] * 1e6).round() as i64,
        );
        let next = positions.len() as u32;
        let mapped = *map.entry(key).or_insert(next);
        if mapped == next {
            positions.push(*point);
        }
        remap[index] = mapped;
    }

    let triangles = mesh
        .triangles
        .iter()
        .map(|tri| {
            [
                remap[tri[0] as usize],
                remap[tri[1] as usize],
                remap[tri[2] as usize],
            ]
        })
        .collect();

    cad_contract::mesh::Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    }
}

fn assert_watertight(mesh: &cad_contract::mesh::Mesh) -> TestResult {
    let welded = welded(mesh);
    assert_eq!(
        welded.open_edges(),
        0,
        "a solid's surface must be closed once coincident vertices are welded"
    );
    assert!(welded.is_closed());
    Ok(())
}

#[test]
fn a_box_has_its_analytic_volume_and_is_closed() -> TestResult {
    let built = build_box(box_spec([10.0, 10.0, 5.0])?, Unit::Millimeter, 0.01)?;
    assert!(
        (built.summary.volume_mm3 - 500.0).abs() < 1e-6,
        "volume was {}",
        built.summary.volume_mm3
    );
    assert_eq!(built.summary.triangles, 12);
    assert_watertight(&built.mesh)?;
    assert!(
        (built.summary.surface_area_mm2 - 400.0).abs() < 1e-6,
        "area was {}",
        built.summary.surface_area_mm2
    );
    let bounding = built.summary.bounding_box_mm.ok_or("no bounding box")?;
    assert!((bounding[0]).abs() < 1e-9);
    assert!((bounding[5] - 5.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn the_stl_written_to_disk_round_trips() -> TestResult {
    let dir = TempDir::new("stl")?;
    let built = build_box(box_spec([10.0, 10.0, 5.0])?, Unit::Millimeter, 0.01)?;

    let bytes = stl_bytes(&built.mesh)?;
    assert_eq!(
        bytes.len(),
        84 + 50 * built.mesh.triangle_count(),
        "binary STL is 84 bytes of preamble plus 50 per triangle"
    );

    let path = dir.path().join("box.stl");
    let written = write_stl_binary(&built.mesh, &path)?;
    assert_eq!(written, bytes.len());

    let on_disk = std::fs::read(&path)?;
    let mesh = read_stl_binary(&on_disk)?;
    assert_eq!(mesh.triangle_count(), 12);
    // Binary STL stores f32 coordinates, so allow the precision the format actually has.
    assert!(
        (mesh.signed_volume() - 500.0).abs() < 1e-3,
        "round-tripped volume was {}",
        mesh.signed_volume()
    );
    assert_watertight(&mesh)?;
    Ok(())
}

#[test]
fn input_units_are_converted_to_millimetres() -> TestResult {
    let built = build_box(box_spec([1.0, 1.0, 1.0])?, Unit::Inch, 0.01)?;
    for value in &built.summary.requested_size_mm {
        assert!((value - 25.4).abs() < 1e-9, "got {value}");
    }
    let expected = 25.4f64.powi(3);
    assert!(
        (built.summary.volume_mm3 - expected).abs() < 1e-6,
        "expected {expected}, got {}",
        built.summary.volume_mm3
    );
    assert_eq!(built.summary.units, "in");
    Ok(())
}

#[test]
fn a_union_counts_the_overlap_once() -> TestResult {
    // 10x10x10 box plus a 4x4x10 box at (8,8,0): 1000 + 160 - 40 = 1120 mm3.
    let a = box_spec([10.0, 10.0, 10.0])?;
    let b = BoxSpec::new([4.0, 4.0, 10.0], [8.0, 8.0, 0.0])?;
    let built = build_union(a, b, Unit::Millimeter, 0.01)?;
    assert!(
        (built.summary.volume_mm3 - 1120.0).abs() < 1.0,
        "union volume was {}, expected 1120 (+/-1 for the coplanar retry)",
        built.summary.volume_mm3
    );
    assert_watertight(&built.mesh)?;
    Ok(())
}

#[test]
fn the_summary_reports_only_the_capabilities_that_were_exercised() -> TestResult {
    let built = build_box(box_spec([2.0, 2.0, 2.0])?, Unit::Millimeter, 0.01)?;
    let capabilities = &built.summary.capabilities;
    assert!(capabilities.iter().any(|c| c == "extrude"));
    assert!(capabilities.iter().any(|c| c == "tessellate"));
    assert!(
        !capabilities.iter().any(|c| c == "fillet"),
        "fillet is not implemented and must not be advertised: {capabilities:?}"
    );
    assert!(
        !capabilities.iter().any(|c| c == "boolean difference"),
        "difference returns a wrong solid through this path and must not be advertised"
    );
    Ok(())
}

#[test]
fn the_document_header_records_units_and_tolerance() -> TestResult {
    let built = build_box(box_spec([3.0, 4.0, 5.0])?, Unit::Centimeter, 0.02)?;
    let json = built.header_json()?;
    let (header, migrations) = DocumentHeader::from_json(json.as_bytes())?;
    assert_eq!(header.schema, CURRENT_SCHEMA);
    assert_eq!(header.units, Unit::Centimeter);
    assert!((header.tolerance.linear_mm() - 0.02).abs() < 1e-12);
    assert!(
        migrations.is_empty(),
        "a header written at the current schema needs no migration"
    );
    Ok(())
}

#[test]
fn a_negative_size_is_rejected_before_it_reaches_the_kernel() -> TestResult {
    match BoxSpec::new([-1.0, 1.0, 1.0], [0.0; 3]) {
        Err(cad_cli::CliError::Input(message)) => {
            assert!(message.contains("positive"), "message was {message}");
            Ok(())
        }
        Err(other) => Err(format!("unexpected error: {other}").into()),
        Ok(_) => Err("a negative size must not be accepted".into()),
    }
}

#[test]
fn a_saved_project_rebuilds_after_a_parameter_edit() -> TestResult {
    let dir = TempDir::new("project")?;
    let path = dir.path().join("bracket.zcad");
    let project = Project::box_part(
        "bracket",
        DocumentId::new(1),
        Unit::Millimeter,
        [10.0, 10.0, 5.0],
    )?;
    project.save(&path)?;

    let mut loaded = Project::load(&path)?;
    loaded.set_parameter("depth", 8.0)?;
    loaded.save(&path)?;

    let reopened = Project::load(&path)?;
    let (evaluated, mut kernel) = evaluate_project(&reopened)?;
    assert!(
        (evaluated.volume_mm3 - 800.0).abs() < 1e-6,
        "recomputed volume was {}",
        evaluated.volume_mm3
    );
    assert_watertight(&evaluated.mesh)?;
    kernel.release(evaluated.solid);
    Ok(())
}

#[test]
fn a_truncated_stl_is_rejected() -> TestResult {
    let built = build_box(box_spec([1.0, 1.0, 1.0])?, Unit::Millimeter, 0.01)?;
    let mut bytes = stl_bytes(&built.mesh)?;
    bytes.truncate(bytes.len() - 10);
    match read_stl_binary(&bytes) {
        Err(cad_cli::CliError::Input(message)) => {
            assert!(message.contains("bytes"), "message was {message}");
            Ok(())
        }
        Err(other) => Err(format!("unexpected error: {other}").into()),
        Ok(_) => Err("a truncated file must not parse".into()),
    }
}
