//! Real IfcOpenShell benchmark adapter.
//!
//! IfcOpenShell is a Python/C++ IFC toolkit, so this adapter builds an IFC4 wall through its API,
//! re-reads the geometry through the same library's mesher and sums the signed tetrahedron volumes
//! of the triangles it produced. The reference is the analytic volume of the extruded rectangle, so
//! the check covers the IFC representation, the unit assignment and the mesher, not just one call.
//!
//! The interpreter is discovered by [`crate::python`]; `ifcopenshell` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Wall dimensions of the test case, in metres.
const LENGTH_M: f64 = 2.0;
const HEIGHT_M: f64 = 3.0;
const THICKNESS_M: f64 = 0.2;

/// Verified result of one real IfcOpenShell run.
#[derive(Debug, Clone, Serialize)]
pub struct IfcopenshellBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub wall_length_m: f64,
    pub wall_height_m: f64,
    pub wall_thickness_m: f64,
    /// Volume of the triangulated IFC geometry, in m³.
    pub measured_volume_m3: f64,
    /// `length × height × thickness`, in m³.
    pub analytic_volume_m3: f64,
    pub error_percent: f64,
    pub vertices: u64,
    pub triangles: u64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the IfcOpenShell version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("ifcopenshell", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected IfcOpenShell version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("ifcopenshell", Duration::from_secs(60))
}

/// Build and re-mesh the IFC wall, then compare the enclosed volume with the analytic value.
pub fn run_wall_volume_benchmark(timeout: Duration) -> Result<IfcopenshellBenchmark, String> {
    let (executable, version) = available().ok_or(
        "ifcopenshell is not importable: set ATLAS_PYTHON to an interpreter with ifcopenshell installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let measured_volume_m3 = python::number(&result, "volume_m3")?;
    let vertices = python::number(&result, "vertices")? as u64;
    let triangles = python::number(&result, "triangles")? as u64;
    let analytic_volume_m3 = LENGTH_M * HEIGHT_M * THICKNESS_M;
    let error_percent =
        (measured_volume_m3 - analytic_volume_m3).abs() / analytic_volume_m3 * 100.0;
    if triangles == 0 {
        return Err("IfcOpenShell returned geometry with no triangles".to_string());
    }

    Ok(IfcopenshellBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        wall_length_m: LENGTH_M,
        wall_height_m: HEIGHT_M,
        wall_thickness_m: THICKNESS_M,
        measured_volume_m3,
        analytic_volume_m3,
        error_percent,
        vertices,
        triangles,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

fn benchmark_script() -> &'static str {
    r#"import json
import ifcopenshell
import ifcopenshell.api.aggregate
import ifcopenshell.api.context
import ifcopenshell.api.geometry
import ifcopenshell.api.project
import ifcopenshell.api.root
import ifcopenshell.api.spatial
import ifcopenshell.api.unit
import ifcopenshell.geom

model = ifcopenshell.api.project.create_file()
project = ifcopenshell.api.root.create_entity(model, ifc_class="IfcProject", name="AtlasWall")
length = ifcopenshell.api.unit.add_si_unit(model, unit_type="LENGTHUNIT")
ifcopenshell.api.unit.assign_unit(model, units=[length])
site = ifcopenshell.api.root.create_entity(model, ifc_class="IfcSite", name="Site")
ifcopenshell.api.aggregate.assign_object(model, products=[site], relating_object=project)
context = ifcopenshell.api.context.add_context(model, context_type="Model")
representation = ifcopenshell.api.geometry.add_wall_representation(
    model, context=context, length=2.0, height=3.0, thickness=0.2
)
wall = ifcopenshell.api.root.create_entity(model, ifc_class="IfcWall", name="Wall")
ifcopenshell.api.geometry.assign_representation(
    model, product=wall, representation=representation
)
ifcopenshell.api.spatial.assign_container(model, products=[wall], relating_structure=site)

shape = ifcopenshell.geom.create_shape(ifcopenshell.geom.settings(), wall)
verts = shape.geometry.verts
faces = shape.geometry.faces
volume = 0.0
for index in range(0, len(faces), 3):
    a, b, c = faces[index] * 3, faces[index + 1] * 3, faces[index + 2] * 3
    ax, ay, az = verts[a : a + 3]
    bx, by, bz = verts[b : b + 3]
    cx, cy, cz = verts[c : c + 3]
    volume += (
        ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)
    ) / 6.0

print(json.dumps({
    "volume_m3": abs(volume),
    "vertices": len(verts) // 3,
    "triangles": len(faces) // 3,
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_volume_is_one_point_two_cubic_metres() {
        assert!((LENGTH_M * HEIGHT_M * THICKNESS_M - 1.2).abs() < 1e-12);
    }

    #[test]
    fn benchmark_runs_when_ifcopenshell_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: ifcopenshell is not importable in this environment");
            return Ok(());
        }
        let result =
            run_wall_volume_benchmark(Duration::from_secs(180)).map_err(std::io::Error::other)?;
        assert_eq!(result.vertices, 8);
        assert_eq!(result.triangles, 12);
        assert!(
            result.error_percent < 1e-9,
            "IFC volume {} m³ differs from {} m³ by {:.3e}%",
            result.measured_volume_m3,
            result.analytic_volume_m3,
            result.error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}
