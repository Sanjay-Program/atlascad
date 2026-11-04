//! Analytic validation of the linear static solver.
//!
//! The suite has two levels of evidence:
//!
//! * an **exact patch test** on a bar under uniaxial tension, where the applied nodal loads are
//!   consistent (one element across each lateral direction) and the uniform-strain field is
//!   representable, so the displacement, stress and strain energy are analytic;
//! * a **cantilever convergence series** against the Timoshenko beam solution, which is the honest
//!   way to test a hex8 bending result: the coarse mesh is stiff, and the error must fall as the
//!   mesh is refined rather than matching the analytic value at any voxel size.
//!
//! The measured numbers are recorded in `docs/cae/CAE_CAPABILITIES.md`.

use cad_cae::{
    Axis, CaeError, FaceLoad, FaceSelection, SolverSettings, StaticStudy, Support, run_static,
};
use cad_contract::mesh::Mesh;
use cad_engineering_model::MaterialLibrary;
use std::error::Error;

/// A closed box as 12 outward triangles.
fn box_mesh(min: [f64; 3], max: [f64; 3]) -> Mesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let positions = vec![
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    }
}

fn library() -> MaterialLibrary {
    MaterialLibrary::standard()
}

/// 1018 steel: E = 200 GPa, ν = 0.3 in the shared library.
const STEEL: &str = "steel.1018";

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn a_uniaxial_tension_patch_matches_the_analytic_bar_exactly() -> TestResult {
    // 40 x 10 x 10 mm, 4 x 1 x 1 elements at a 10 mm voxel: four nodes on each end face, so the
    // equal nodal split is the consistent load vector for a uniform traction.
    let mesh = box_mesh([0.0, 0.0, 0.0], [40.0, 10.0, 10.0]);
    let mut study = StaticStudy::new("uniaxial patch", 10.0, STEEL);
    study.supports = vec![
        // The symmetry plane x = 0 only holds the axial component; fixing all three would create
        // plane strain at the support and destroy the uniform stress state.
        Support::hold(FaceSelection::new(Axis::X, 0.0), Axis::X),
        // Symmetry planes: u_y(x, 0, z) = 0 and u_z(x, y, 0) = 0 are both satisfied exactly by the
        // uniform-strain solution of a bar in tension, and they remove the remaining rigid-body
        // modes.
        Support::hold(FaceSelection::new(Axis::Y, 0.0), Axis::Y),
        Support::hold(FaceSelection::new(Axis::Z, 0.0), Axis::Z),
    ];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 40.0),
        [1000.0, 0.0, 0.0],
    )];

    let library = library();
    let steel = library
        .get(STEEL)
        .ok_or("steel.1018 should be in the library")?;
    let youngs = steel
        .youngs_modulus_mpa
        .ok_or("steel.1018 should have a modulus")?;
    let result = run_static(&mesh, &study, &library)?;
    let stress = 1000.0 / 100.0; // 10 MPa
    let extension = stress * 40.0 / youngs;
    assert!(
        (result.results.max_von_mises_mpa - stress).abs() < 1.0e-8 * stress,
        "von Mises {} MPa should be {stress} MPa",
        result.results.max_von_mises_mpa
    );
    // Strain energy of a uniformly stressed bar is 0.5 * F * delta.
    let energy = 0.5 * 1000.0 * extension;
    assert!(
        (result.results.strain_energy_nmm - energy).abs() < 1.0e-8 * energy,
        "strain energy {} should be {energy}",
        result.results.strain_energy_nmm
    );
    assert!(
        result.results.max_displacement_mm >= extension
            && result.results.max_displacement_mm < extension * 1.01,
        "max displacement {} should be just above the {extension} mm axial extension",
        result.results.max_displacement_mm
    );
    assert!(result.solver.converged);
    assert!(result.results.equilibrium_residual_ratio < 1.0e-8);
    assert_eq!(result.mesh.elements, 4);
    assert_eq!(result.quality, cad_cae::QUALITY_LABEL);
    Ok(())
}

#[test]
fn the_cantilever_converges_towards_the_timoshenko_solution() -> TestResult {
    // 60 x 10 x 10 mm cantilever, 100 N transverse at the free end, E = 200 GPa, ν = 0.3.
    let mesh = box_mesh([0.0, 0.0, 0.0], [60.0, 10.0, 10.0]);
    let library = library();
    let steel = library
        .get(STEEL)
        .ok_or("steel.1018 should be in the library")?;
    let e = steel
        .youngs_modulus_mpa
        .ok_or("steel.1018 should have a modulus")?;
    let nu = steel
        .poisson_ratio
        .ok_or("steel.1018 should have a Poisson ratio")?;
    let load = 100.0_f64;
    let length = 60.0_f64;
    let second_moment = 10.0_f64.powi(4) / 12.0;
    let area = 100.0;
    let shear_modulus = e / (2.0 * (1.0 + nu));
    let bending = load * length.powi(3) / (3.0 * e * second_moment);
    let shear = load * length / (5.0 / 6.0 * shear_modulus * area);
    let reference = bending + shear;

    let run = |voxel: f64| {
        let mut study = StaticStudy::new("cantilever", voxel, STEEL);
        study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
        study.loads = vec![FaceLoad::new(
            FaceSelection::new(Axis::X, 60.0),
            [0.0, 0.0, -load],
        )];
        study.solver = SolverSettings {
            relative_tolerance: 1.0e-9,
            max_iterations: 40_000,
        };
        run_static(&mesh, &study, &library)
    };

    let coarse = run(5.0)?;
    let fine = run(2.5)?;
    assert!(coarse.solver.converged);
    assert!(fine.solver.converged);
    assert_eq!(fine.mesh.elements, 24 * 4 * 4);

    let coarse_error = (coarse.results.max_displacement_mm - reference).abs() / reference;
    let fine_error = (fine.results.max_displacement_mm - reference).abs() / reference;
    assert!(
        fine_error < coarse_error,
        "refining 5.0 -> 2.5 mm should reduce the error: coarse {coarse_error:.4}, fine {fine_error:.4}"
    );
    assert!(
        fine_error < 0.10,
        "the 2.5 mm mesh should be within 10% of the Timoshenko reference {reference:.6} mm; \
         it is {:.6} mm ({fine_error:.4})",
        fine.results.max_displacement_mm
    );
    // The bending result must be a displacement, not a sign or magnitude error.
    assert!(fine.results.max_displacement_mm > 0.0);
    assert!(fine.results.strain_energy_nmm > 0.0);
    Ok(())
}

#[test]
fn an_open_mesh_is_refused_before_any_solve() {
    let mut mesh = box_mesh([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    mesh.triangles.pop();
    let mut study = StaticStudy::new("open", 5.0, STEEL);
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 10.0),
        [100.0, 0.0, 0.0],
    )];
    assert!(matches!(
        run_static(&mesh, &study, &library()),
        Err(CaeError::OpenMesh(edges)) if edges > 0
    ));
}

#[test]
fn a_voxel_mesh_that_loses_too_much_volume_is_refused() {
    // A 25 mm voxel on a 50 x 20 x 20 mm bar leaves two solid voxels: 31 250 mm3 against
    // 20 000 mm3, over the 25% volume-error limit. Both loaded and supported planes still have
    // nodes, so the refusal is the volume check rather than a bad selection.
    let mesh = box_mesh([0.0, 0.0, 0.0], [50.0, 20.0, 20.0]);
    let mut study = StaticStudy::new("coarse", 25.0, STEEL);
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 50.0),
        [100.0, 0.0, 0.0],
    )];
    let error = run_static(&mesh, &study, &library());
    assert!(
        matches!(error, Err(CaeError::Invalid(ref message)) if message.contains("volume")),
        "an unusable voxel mesh must be refused, got {error:?}"
    );
}

#[test]
fn a_missing_material_or_an_unsupported_face_is_reported() {
    let mesh = box_mesh([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    let mut study = StaticStudy::new("missing", 5.0, "unobtainium");
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 10.0),
        [100.0, 0.0, 0.0],
    )];
    assert!(matches!(
        run_static(&mesh, &study, &library()),
        Err(CaeError::UnknownMaterial(_))
    ));

    study.material = STEEL.to_string();
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, -5.0))];
    assert!(matches!(
        run_static(&mesh, &study, &library()),
        Err(CaeError::Invalid(message)) if message.contains("no node")
    ));
}

#[test]
fn a_study_without_supports_or_loads_is_refused() {
    let empty = StaticStudy::new("empty", 5.0, STEEL);
    assert!(matches!(empty.validate(), Err(CaeError::Invalid(_))));

    let mut study = StaticStudy::new("no load", 5.0, STEEL);
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    assert!(matches!(study.validate(), Err(CaeError::Invalid(_))));

    let mut study = StaticStudy::new("zero load", 5.0, STEEL);
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 10.0),
        [0.0, 0.0, 0.0],
    )];
    assert!(matches!(study.validate(), Err(CaeError::Invalid(_))));

    let mut study = StaticStudy::new("negative voxel", -1.0, STEEL);
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 10.0),
        [1.0, 0.0, 0.0],
    )];
    assert!(matches!(study.validate(), Err(CaeError::Invalid(_))));
}
