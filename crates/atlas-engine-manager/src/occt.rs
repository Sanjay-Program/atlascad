//! Real Open CASCADE (OCCT) benchmark adapter, driven through pythonocc-core.
//!
//! OCCT is a B-Rep kernel shipped as a C++ library, so this adapter drives it through its
//! Python bindings: it builds a box, cuts a cylindrical through-hole with the boolean engine and
//! asks OCCT's own geometric-properties algorithm for the volume, surface area and centre of mass
//! of the resulting solid. The references are the closed-form values of the same solid
//! (`a·b·c − πr²h` for the volume, the box faces minus two hole circles plus the bore wall for the
//! area, the box centre for the centroid), so the check covers the boolean operation, the
//! tessellation-free mass properties and the solid's validity, not one function call.
//!
//! The interpreter is discovered by [`crate::python`]; `OCC` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Box dimensions of the test case, in millimetres.
const BOX_LENGTH_MM: f64 = 40.0;
const BOX_WIDTH_MM: f64 = 30.0;
const BOX_HEIGHT_MM: f64 = 20.0;
/// Radius of the cylindrical through-hole drilled along the box height, in millimetres.
const HOLE_RADIUS_MM: f64 = 6.0;
/// Density used for the derived mass, in kg/m³ (steel).
const DENSITY_KG_M3: f64 = 7_850.0;

/// Verified result of one real OCCT run.
#[derive(Debug, Clone, Serialize)]
pub struct OcctBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub box_length_mm: f64,
    pub box_width_mm: f64,
    pub box_height_mm: f64,
    pub hole_radius_mm: f64,
    /// Volume of the cut solid from OCCT's geometric properties, in mm³.
    pub measured_volume_mm3: f64,
    /// `a·b·c − πr²h`, in mm³.
    pub analytic_volume_mm3: f64,
    /// Surface area of the cut solid, in mm².
    pub measured_area_mm2: f64,
    /// Box faces, less the two hole circles, plus the bore wall, in mm².
    pub analytic_area_mm2: f64,
    /// Centre of mass of the cut solid, in mm.
    pub centroid_mm: [f64; 3],
    /// `measured_volume_mm3 × density`, in kg.
    pub mass_kg: f64,
    /// Whether `BRepCheck_Analyzer` accepts the boolean result as a valid solid.
    pub valid_solid: bool,
    /// Volume error against the closed form, in percent.
    pub error_percent: f64,
    /// Surface-area error against the closed form, in percent.
    pub area_error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the OCCT version, when the bindings are importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    // pythonocc-core ships no distribution metadata, so the version is the module's `VERSION`.
    let version =
        python::module_version_or_attribute("OCC", "VERSION", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected OCCT version, or an error when the bindings cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version_or_attribute("OCC", "VERSION", Duration::from_secs(60))
}

/// Cut the hole, measure the solid and compare with the closed-form values.
pub fn run_boolean_volume_benchmark(timeout: Duration) -> Result<OcctBenchmark, String> {
    let (executable, version) = available().ok_or(
        "OCC is not importable: set ATLAS_PYTHON to an interpreter with pythonocc-core installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let measured_volume_mm3 = python::number(&result, "volume_mm3")?;
    let measured_area_mm2 = python::number(&result, "area_mm2")?;
    let valid_solid = python::boolean(&result, "valid")?;
    if !valid_solid {
        return Err("OCCT's boolean result is not a valid solid".to_string());
    }
    let centroid = result
        .get("centroid_mm")
        .and_then(serde_json::Value::as_array)
        .filter(|values| values.len() == 3)
        .ok_or("OCCT returned no three-component centre of mass")?;
    let mut centroid_mm = [0.0; 3];
    for (slot, value) in centroid_mm.iter_mut().zip(centroid) {
        *slot = value
            .as_f64()
            .ok_or("OCCT returned a non-numeric centre of mass")?;
    }

    let analytic_volume_mm3 = analytic_volume_mm3();
    let analytic_area_mm2 = analytic_area_mm2();
    let error_percent = (measured_volume_mm3 - analytic_volume_mm3).abs() / analytic_volume_mm3
        * 100.0;
    let area_error_percent =
        (measured_area_mm2 - analytic_area_mm2).abs() / analytic_area_mm2 * 100.0;

    Ok(OcctBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        box_length_mm: BOX_LENGTH_MM,
        box_width_mm: BOX_WIDTH_MM,
        box_height_mm: BOX_HEIGHT_MM,
        hole_radius_mm: HOLE_RADIUS_MM,
        measured_volume_mm3,
        analytic_volume_mm3,
        measured_area_mm2,
        analytic_area_mm2,
        centroid_mm,
        mass_kg: measured_volume_mm3 * DENSITY_KG_M3 * 1e-9,
        valid_solid,
        error_percent,
        area_error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

/// `a·b·c − πr²h`, the volume of the box with a centred through-hole.
fn analytic_volume_mm3() -> f64 {
    BOX_LENGTH_MM * BOX_WIDTH_MM * BOX_HEIGHT_MM
        - std::f64::consts::PI * HOLE_RADIUS_MM * HOLE_RADIUS_MM * BOX_HEIGHT_MM
}

/// Box faces, less the two circles the hole removes, plus the cylindrical bore wall.
fn analytic_area_mm2() -> f64 {
    let box_faces = 2.0
        * (BOX_LENGTH_MM * BOX_WIDTH_MM
            + BOX_LENGTH_MM * BOX_HEIGHT_MM
            + BOX_WIDTH_MM * BOX_HEIGHT_MM);
    let hole_circles = 2.0 * std::f64::consts::PI * HOLE_RADIUS_MM * HOLE_RADIUS_MM;
    let bore_wall = 2.0 * std::f64::consts::PI * HOLE_RADIUS_MM * BOX_HEIGHT_MM;
    box_faces - hole_circles + bore_wall
}

fn benchmark_script() -> &'static str {
    r#"import json
import math
from OCC.Core.BRepAlgoAPI import BRepAlgoAPI_Cut
from OCC.Core.BRepCheck import BRepCheck_Analyzer
from OCC.Core.BRepGProp import brepgprop
from OCC.Core.BRepPrimAPI import BRepPrimAPI_MakeBox, BRepPrimAPI_MakeCylinder
from OCC.Core.GProp import GProp_GProps
from OCC.Core.gp import gp_Ax2, gp_Dir, gp_Pnt

box = BRepPrimAPI_MakeBox(40.0, 30.0, 20.0).Shape()
hole = BRepPrimAPI_MakeCylinder(
    gp_Ax2(gp_Pnt(20.0, 15.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), 6.0, 20.0
).Shape()
cut = BRepAlgoAPI_Cut(box, hole).Shape()

volume = GProp_GProps()
brepgprop.VolumeProperties(cut, volume)
area = GProp_GProps()
brepgprop.SurfaceProperties(cut, area)
centre = volume.CentreOfMass()

print(json.dumps({
    "volume_mm3": volume.Mass(),
    "area_mm2": area.Mass(),
    "centroid_mm": [centre.X(), centre.Y(), centre.Z()],
    "valid": bool(BRepCheck_Analyzer(cut).IsValid()),
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closed_forms_match_the_hand_calculation() {
        // 40 x 30 x 20 box, six-millimetre-radius bore through the twenty-millimetre height.
        let volume = analytic_volume_mm3();
        assert!((volume - (24_000.0 - std::f64::consts::PI * 720.0)).abs() < 1e-9);
        assert!((volume - 21_738.053_289_415_35).abs() < 1e-6);
        let area = analytic_area_mm2();
        assert!((area - 5_727.787_565_803_085).abs() < 1e-6);
    }

    #[test]
    fn benchmark_runs_when_pythonocc_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: pythonocc-core is not importable in this environment");
            return Ok(());
        }
        let result =
            run_boolean_volume_benchmark(Duration::from_secs(180)).map_err(std::io::Error::other)?;
        assert!(result.valid_solid);
        assert!(
            result.error_percent < 1e-9,
            "OCCT volume {} mm³ differs from {} mm³ by {:.3e}%",
            result.measured_volume_mm3,
            result.analytic_volume_mm3,
            result.error_percent
        );
        assert!(
            result.area_error_percent < 1e-9,
            "OCCT area {} mm² differs from {} mm² by {:.3e}%",
            result.measured_area_mm2,
            result.analytic_area_mm2,
            result.area_error_percent
        );
        // The bore is centred on the box, so the centroid stays at the box centre.
        let expected_centroid = [BOX_LENGTH_MM / 2.0, BOX_WIDTH_MM / 2.0, BOX_HEIGHT_MM / 2.0];
        for (axis, (measured, expected)) in ["x", "y", "z"]
            .iter()
            .zip(result.centroid_mm.iter().zip(expected_centroid))
        {
            assert!(
                (measured - expected).abs() < 1e-9,
                "centroid {axis} is {measured} mm, expected {expected} mm"
            );
        }
        assert!(
            (result.mass_kg - result.measured_volume_mm3 * DENSITY_KG_M3 * 1e-9).abs() < 1e-12
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}
