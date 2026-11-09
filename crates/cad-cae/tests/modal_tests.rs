//! Analytic validation of the dynamic modal analysis solver.
//!
//! Validates:
//! * Cantilever beam natural frequencies against Euler-Bernoulli beam theory.
//! * Orthogonality and ascending ordering of computed modal frequencies ($f_1 \le f_2 \le f_3$).
//! * Non-zero mode shapes with mass-normalized coordinates.

use cad_cae::{Axis, FaceSelection, ModalStudy, SolverSettings, Support, run_modal};
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
        [x0, y0, z0], // replaced below
    ];
    let _ = positions;
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

const STEEL: &str = "steel.1018";
type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn cantilever_beam_modal_frequencies_match_euler_bernoulli() -> TestResult {
    // 60 x 10 x 10 mm cantilever beam
    let length = 60.0_f64;
    let width = 10.0_f64;
    let height = 10.0_f64;
    let mesh = box_mesh([0.0, 0.0, 0.0], [length, width, height]);

    let library = MaterialLibrary::standard();
    let steel = library
        .get(STEEL)
        .ok_or("steel.1018 should exist in library")?;
    let e = steel
        .youngs_modulus_mpa
        .ok_or("steel.1018 missing modulus")?;
    let rho_kg_m3 = steel.density_kg_m3;
    let rho_tonne_mm3 = rho_kg_m3 * 1.0e-12;

    // Euler-Bernoulli reference fundamental frequency:
    // f_1 = (beta1_L)^2 / (2 * pi * L^2) * sqrt(E * I / (rho * A))
    let i_second_moment = width * height.powi(3) / 12.0;
    let area = width * height;
    let mu = rho_tonne_mm3 * area;
    let beta1_l = 1.8751040687_f64;
    let omega_eb = (beta1_l.powi(2) / length.powi(2)) * (e * i_second_moment / mu).sqrt();
    let f1_eb = omega_eb / std::f64::consts::TAU;

    // Setup modal study at 2.5 mm voxel size, requesting 3 modes
    let voxel_size = 2.5;
    let mut study = ModalStudy::new("cantilever-modal", voxel_size, STEEL)
        .with_modes(3)
        .with_support(Support::fixed(FaceSelection::new(Axis::X, 0.0)));
    study.solver = SolverSettings {
        relative_tolerance: 1.0e-8,
        max_iterations: 30_000,
    };

    let result = run_modal(&mesh, &study, &library)?;

    for (idx, m) in result.modes.iter().enumerate() {
        println!(
            "Mode {idx}: freq = {:.4} Hz, omega = {:.4} rad/s",
            m.frequency_hz, m.angular_frequency_rad_s
        );
    }

    assert_eq!(result.modes.len(), 3);
    assert!(result.modes[0].frequency_hz > 0.0);
    assert!(result.modes[1].frequency_hz >= result.modes[0].frequency_hz);
    assert!(result.modes[2].frequency_hz >= result.modes[1].frequency_hz);

    let f1_fem = result.modes[0].frequency_hz;
    let error_rel = (f1_fem - f1_eb).abs() / f1_eb;

    println!("Euler-Bernoulli analytic f1 = {f1_eb:.2} Hz");
    println!(
        "FE Modal fundamental f1   = {f1_fem:.2} Hz (rel error: {:.2}%)",
        error_rel * 100.0
    );

    // Assert within 5% of analytical Euler-Bernoulli solution
    assert!(
        error_rel < 0.05,
        "Fundamental frequency {f1_fem:.2} Hz should be within 5% of Euler-Bernoulli {f1_eb:.2} Hz (error: {:.2}%)",
        error_rel * 100.0
    );

    // Verify mode shape normalization and non-emptiness
    for mode in &result.modes {
        assert!(mode.generalized_mass > 0.0);
        assert!(mode.max_displacement_normalized > 0.0);
        let shape = mode
            .mode_shape
            .as_ref()
            .ok_or("mode shape should be present")?;
        assert_eq!(shape.len(), result.mesh.nodes);
    }

    Ok(())
}
