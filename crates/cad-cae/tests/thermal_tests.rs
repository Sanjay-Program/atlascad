//! Analytic validation of the steady-state thermal conduction FEA solver.
//!
//! Benchmark:
//! 1D Fourier heat conduction through a solid steel bar.
//! - Length L = 20 mm, cross-section 5 mm x 5 mm.
//! - T(x=0) = 100 °C, T(x=20) = 20 °C.
//! - Steel thermal conductivity k = 51.9 W/(m·K).
//!
//! Analytic solution:
//! - Temperature profile: T(x) = 100 - 4*x (°C)
//! - Temperature gradient: dT/dx = -4.0 K/mm = -4000 K/m
//! - Heat flux: q_x = -k * dT/dx = 51.9 * 4000 = 207,600 W/m²
//! - Total heat rate: Q = q_x * A = 207,600 * 25e-6 m² = 5.19 W

use cad_cae::{
    Axis, CaeError, FaceHeatFlux, FaceSelection, FixedTemperature, HeatLoad, ThermalStudy,
    run_thermal, run_thermal_with_fields,
};
use cad_contract::mesh::Mesh;
use cad_engineering_model::MaterialLibrary;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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

#[test]
fn pure_1d_conduction_matches_fourier_analytic_solution() -> TestResult {
    let library = MaterialLibrary::standard();
    let mesh = box_mesh([0.0, 0.0, 0.0], [20.0, 5.0, 5.0]);

    // 5 mm voxels: 4 elements along X
    let study = ThermalStudy::new("fourier-1d", 5.0, "steel.1018")
        .with_fixed_temperature(FixedTemperature::new(
            FaceSelection::new(Axis::X, 0.0),
            100.0,
        ))
        .with_fixed_temperature(FixedTemperature::new(
            FaceSelection::new(Axis::X, 20.0),
            20.0,
        ));

    let result = run_thermal_with_fields(&mesh, &study, &library, true)?;

    assert_eq!(result.element_count, 4);
    assert!((result.min_temperature_c - 20.0).abs() < 1.0e-3);
    assert!((result.max_temperature_c - 100.0).abs() < 1.0e-3);
    assert!((result.avg_temperature_c - 60.0).abs() < 1.0e-3);

    // Analytic heat flux is exactly 207,600 W/m²
    assert!(
        (result.max_heat_flux_w_m2 - 207_600.0).abs() < 100.0,
        "max flux should match analytic: got {}",
        result.max_heat_flux_w_m2
    );
    assert!(
        (result.avg_heat_flux_w_m2 - 207_600.0).abs() < 100.0,
        "avg flux should match analytic: got {}",
        result.avg_heat_flux_w_m2
    );

    // Check detailed element fluxes
    let fluxes = result
        .element_heat_fluxes
        .as_ref()
        .ok_or("expected element fluxes")?;
    assert_eq!(fluxes.len(), 4);
    for q in fluxes {
        assert!((q[0] - 207_600.0).abs() < 100.0, "q_x mismatch");
        assert!(q[1].abs() < 1.0, "q_y should be zero");
        assert!(q[2].abs() < 1.0, "q_z should be zero");
    }

    assert!(
        result
            .summary_line()
            .contains("thermal: temp [20.0..100.0] °C")
    );
    Ok(())
}

#[test]
fn thermal_study_validation_and_neumann_bc() -> TestResult {
    let library = MaterialLibrary::standard();
    let mesh = box_mesh([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);

    // Study without any fixed temperature should be refused
    let invalid_study = ThermalStudy::new("unconstrained", 5.0, "steel.1018")
        .with_heat_load(HeatLoad::new(FaceSelection::new(Axis::X, 0.0), 10.0));
    assert!(matches!(
        run_thermal(&mesh, &invalid_study, &library),
        Err(CaeError::Invalid(_))
    ));

    // Study with valid Dirichlet and Neumann BC
    let valid_study = ThermalStudy::new("conduction-with-flux", 5.0, "steel.1018")
        .with_fixed_temperature(FixedTemperature::new(
            FaceSelection::new(Axis::X, 0.0),
            25.0,
        ))
        .with_heat_flux(FaceHeatFlux::new(
            FaceSelection::new(Axis::X, 10.0),
            50_000.0,
        ))
        .with_volumetric_heat(100_000.0);

    let result = run_thermal(&mesh, &valid_study, &library)?;
    assert!(
        result.max_temperature_c > 25.0,
        "applied heat should increase max temperature above boundary"
    );
    assert!(result.total_reaction_heat_w.abs() > 0.0);
    Ok(())
}
