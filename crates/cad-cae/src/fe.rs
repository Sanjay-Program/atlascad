//! A small linear-elastic finite-element core.
//!
//! One element type: an 8-node hexahedron, integrated with 2x2x2 Gauss quadrature, for a
//! homogeneous isotropic material under small strain. Because every voxel is the same cube, one
//! local stiffness matrix is reused for every element, which keeps the matrix-free solver cheap.
//!
//! The limitations are the element's, not the code's: fully integrated hex8 elements are stiff in
//! bending on a coarse mesh, so a bending result converges from below and needs a mesh-convergence
//! check. Stresses are recovered at the element centre only.

use crate::SolverSettings;

pub(crate) const ELEMENT_NODES: usize = 8;
pub(crate) const ELEMENT_DOFS: usize = 24;

/// The isotropic elasticity matrix in `[xx, yy, zz, xy, yz, xz]` order.
pub(crate) struct Elasticity {
    pub d: [[f64; 6]; 6],
}

impl Elasticity {
    pub fn isotropic(youngs_modulus_mpa: f64, poisson_ratio: f64) -> Self {
        let e = youngs_modulus_mpa;
        let nu = poisson_ratio;
        let lambda = e * nu / ((1.0 + nu) * (1.0 - 2.0 * nu));
        let mu = e / (2.0 * (1.0 + nu));
        let mut d = [[0.0; 6]; 6];
        for (index, row) in d.iter_mut().enumerate().take(3) {
            row[index] = lambda + 2.0 * mu;
        }
        for (i, j) in [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)] {
            d[i][j] = lambda;
        }
        for (index, row) in d.iter_mut().enumerate().skip(3) {
            row[index] = mu;
        }
        Elasticity { d }
    }
}

/// Natural coordinates in the standard hexahedral node order: the bottom face
/// `(-,-)`, `(+,-)`, `(+,+)`, `(-,+)` at zeta = -1, then the same four at zeta = +1.
/// This is a table rather than bit arithmetic because bit order would put node 3 at
/// `(+,+,-)`, which is node 2 in this ordering.
const fn natural_coordinates(node: usize) -> [f64; 3] {
    match node {
        0 => [-1.0, -1.0, -1.0],
        1 => [1.0, -1.0, -1.0],
        2 => [1.0, 1.0, -1.0],
        3 => [-1.0, 1.0, -1.0],
        4 => [-1.0, -1.0, 1.0],
        5 => [1.0, -1.0, 1.0],
        6 => [1.0, 1.0, 1.0],
        _ => [-1.0, 1.0, 1.0],
    }
}

/// Shape-function derivatives with respect to the natural coordinates at `point`.
fn shape_derivatives(node: usize, point: [f64; 3]) -> [f64; 3] {
    let n = natural_coordinates(node);
    [
        0.125 * n[0] * (1.0 + n[1] * point[1]) * (1.0 + n[2] * point[2]),
        0.125 * n[1] * (1.0 + n[0] * point[0]) * (1.0 + n[2] * point[2]),
        0.125 * n[2] * (1.0 + n[0] * point[0]) * (1.0 + n[1] * point[1]),
    ]
}

/// The strain-displacement matrix at `point` (natural coordinates) for a cube of side `side_mm`.
fn strain_displacement(side_mm: f64, point: [f64; 3]) -> [[f64; ELEMENT_DOFS]; 6] {
    let scale = 2.0 / side_mm;
    let mut b = [[0.0; ELEMENT_DOFS]; 6];
    for node in 0..ELEMENT_NODES {
        let dn = shape_derivatives(node, point);
        let dx = dn[0] * scale;
        let dy = dn[1] * scale;
        let dz = dn[2] * scale;
        let column = node * 3;
        b[0][column] = dx;
        b[1][column + 1] = dy;
        b[2][column + 2] = dz;
        b[3][column] = dy;
        b[3][column + 1] = dx;
        b[4][column + 1] = dz;
        b[4][column + 2] = dy;
        b[5][column] = dz;
        b[5][column + 2] = dx;
    }
    b
}

/// Local stiffness matrix, row-major 24x24, for a cube of side `side_mm`.
pub(crate) fn hex_stiffness(side_mm: f64, elasticity: &Elasticity) -> Vec<f64> {
    let gauss = 1.0 / 3.0_f64.sqrt();
    let weight = 1.0;
    let det_j = (side_mm * 0.5).powi(3);
    let mut stiffness = vec![0.0; ELEMENT_DOFS * ELEMENT_DOFS];
    for &gx in &[-gauss, gauss] {
        for &gy in &[-gauss, gauss] {
            for &gz in &[-gauss, gauss] {
                let b = strain_displacement(side_mm, [gx, gy, gz]);
                for (row, stiffness_row) in stiffness.chunks_mut(ELEMENT_DOFS).enumerate() {
                    for (column, entry) in stiffness_row.iter_mut().enumerate() {
                        let mut sum = 0.0;
                        for (i, b_row) in b.iter().enumerate() {
                            let mut db = 0.0;
                            for (d_row, b_other) in elasticity.d[i].iter().zip(b.iter()) {
                                db += d_row * b_other[column];
                            }
                            sum += b_row[row] * db;
                        }
                        *entry += sum * det_j * weight;
                    }
                }
            }
        }
    }
    stiffness
}

/// Strain at the element centre for a local displacement vector.
fn centre_strain(ue: &[f64; ELEMENT_DOFS], side_mm: f64) -> [f64; 6] {
    let b = strain_displacement(side_mm, [0.0, 0.0, 0.0]);
    let mut strain = [0.0; 6];
    for (index, row) in b.iter().enumerate() {
        let mut sum = 0.0;
        for (column, value) in row.iter().enumerate() {
            sum += value * ue[column];
        }
        strain[index] = sum;
    }
    strain
}

/// Stress at the element centre, in `[xx, yy, zz, xy, yz, xz]` order.
pub(crate) fn centre_stress(
    ue: &[f64; ELEMENT_DOFS],
    side_mm: f64,
    elasticity: &Elasticity,
) -> [f64; 6] {
    let strain = centre_strain(ue, side_mm);
    let mut stress = [0.0; 6];
    for (index, row) in elasticity.d.iter().enumerate() {
        let mut sum = 0.0;
        for (column, value) in row.iter().enumerate() {
            sum += value * strain[column];
        }
        stress[index] = sum;
    }
    stress
}

/// Von Mises equivalent stress from a stress vector.
pub(crate) fn von_mises(stress: &[f64; 6]) -> f64 {
    let deviatoric = (stress[0] - stress[1]).powi(2)
        + (stress[1] - stress[2]).powi(2)
        + (stress[2] - stress[0]).powi(2);
    0.5f64
        .mul_add(
            deviatoric,
            3.0 * (stress[3].powi(2) + stress[4].powi(2) + stress[5].powi(2)),
        )
        .max(0.0)
        .sqrt()
}

/// The outcome of a linear solve: the full displacement vector plus the solver's own evidence.
pub(crate) struct SolveOutcome {
    pub displacement: Vec<f64>,
    pub iterations: usize,
    pub relative_residual: f64,
    /// `K u`, zeroed at constrained degrees of freedom.
    pub internal_force: Vec<f64>,
}

/// Multiply by the global stiffness matrix, matrix-free over the element list.
///
/// Constrained degrees of freedom are zeroed in the result, which is what makes the conjugate
/// gradient iterate in the free subspace without assembling a reduced matrix.
fn apply(
    element_nodes: &[[u32; ELEMENT_NODES]],
    local: &[f64],
    u: &[f64],
    fixed: &[bool],
    out: &mut [f64],
) {
    out.fill(0.0);
    let mut ue = [0.0; ELEMENT_DOFS];
    let mut fe = [0.0; ELEMENT_DOFS];
    for nodes in element_nodes {
        for (local_node, &node) in nodes.iter().enumerate() {
            let base = node as usize * 3;
            for component in 0..3 {
                ue[local_node * 3 + component] = u[base + component];
            }
        }
        for (row, value) in fe.iter_mut().enumerate() {
            let row_start = row * ELEMENT_DOFS;
            let mut sum = 0.0;
            for (column, u) in ue.iter().enumerate() {
                sum += local[row_start + column] * u;
            }
            *value = sum;
        }
        for (local_node, &node) in nodes.iter().enumerate() {
            let base = node as usize * 3;
            for component in 0..3 {
                out[base + component] += fe[local_node * 3 + component];
            }
        }
    }
    for (value, &is_fixed) in out.iter_mut().zip(fixed.iter()) {
        if is_fixed {
            *value = 0.0;
        }
    }
}

fn diagonal(element_nodes: &[[u32; ELEMENT_NODES]], local: &[f64], ndof: usize) -> Vec<f64> {
    let mut diagonal = vec![0.0; ndof];
    for nodes in element_nodes {
        for (local_node, &node) in nodes.iter().enumerate() {
            let base = node as usize * 3;
            for component in 0..3 {
                let local_dof = local_node * 3 + component;
                diagonal[base + component] += local[local_dof * ELEMENT_DOFS + local_dof];
            }
        }
    }
    diagonal
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn norm(a: &[f64]) -> f64 {
    dot(a, a).sqrt()
}

/// Local consistent mass matrix, row-major 24x24, for a cube of side `side_mm` and density in kg/m³.
pub(crate) fn hex_mass(side_mm: f64, density_kg_m3: f64) -> Vec<f64> {
    // In consistent mm-tonne-s units, 1 kg/m³ = 1e-12 tonne/mm³.
    let rho = density_kg_m3 * 1.0e-12;
    let elem_mass = rho * side_mm.powi(3);
    let mut mass = vec![0.0; ELEMENT_DOFS * ELEMENT_DOFS];
    for i in 0..ELEMENT_NODES {
        let ni = natural_coordinates(i);
        for j in 0..ELEMENT_NODES {
            let nj = natural_coordinates(j);
            let factor =
                (3.0 + ni[0] * nj[0]) * (3.0 + ni[1] * nj[1]) * (3.0 + ni[2] * nj[2]) / 1728.0;
            let entry = elem_mass * factor;
            for c in 0..3 {
                let row = i * 3 + c;
                let col = j * 3 + c;
                mass[row * ELEMENT_DOFS + col] = entry;
            }
        }
    }
    mass
}

/// Multiply by the global mass matrix, matrix-free over the element list.
pub(crate) fn apply_mass(
    element_nodes: &[[u32; ELEMENT_NODES]],
    local_mass: &[f64],
    u: &[f64],
    fixed: &[bool],
    out: &mut [f64],
) {
    out.fill(0.0);
    let mut ue = [0.0; ELEMENT_DOFS];
    let mut fe = [0.0; ELEMENT_DOFS];
    for nodes in element_nodes {
        for (local_node, &node) in nodes.iter().enumerate() {
            let base = node as usize * 3;
            for component in 0..3 {
                ue[local_node * 3 + component] = u[base + component];
            }
        }
        for (row, value) in fe.iter_mut().enumerate() {
            let row_start = row * ELEMENT_DOFS;
            let mut sum = 0.0;
            for (column, u_val) in ue.iter().enumerate() {
                sum += local_mass[row_start + column] * u_val;
            }
            *value = sum;
        }
        for (local_node, &node) in nodes.iter().enumerate() {
            let base = node as usize * 3;
            for component in 0..3 {
                out[base + component] += fe[local_node * 3 + component];
            }
        }
    }
    for (value, &is_fixed) in out.iter_mut().zip(fixed.iter()) {
        if is_fixed {
            *value = 0.0;
        }
    }
}

/// Solve `K u = f` with Jacobi-preconditioned conjugate gradient.
#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_system(
    element_nodes: &[[u32; ELEMENT_NODES]],
    local: &[f64],
    ndof: usize,
    fixed: &[bool],
    force: &[f64],
    diag: &[f64],
    tolerance: f64,
    max_iterations: usize,
) -> Result<(Vec<f64>, usize, f64), String> {
    let mut u = vec![0.0; ndof];
    let mut residual = force.to_vec();
    for (value, &is_fixed) in residual.iter_mut().zip(fixed.iter()) {
        if is_fixed {
            *value = 0.0;
        }
    }
    let reference = norm(&residual);
    if reference == 0.0 {
        return Ok((u, 0, 0.0));
    }
    let mut preconditioned: Vec<f64> = residual
        .iter()
        .zip(diag.iter())
        .zip(fixed.iter())
        .map(|((r, d), fixed)| if *fixed || *d <= 0.0 { 0.0 } else { r / d })
        .collect();
    let mut direction = preconditioned.clone();
    let mut rz = dot(&residual, &preconditioned);
    if !rz.is_finite() || rz <= 0.0 {
        return Err("the preconditioner produced a non-positive inner product".into());
    }
    let mut ap = vec![0.0; ndof];
    let mut relative_residual = 1.0;
    for iteration in 1..=max_iterations {
        apply(element_nodes, local, &direction, fixed, &mut ap);
        let curvature = dot(&direction, &ap);
        if !curvature.is_finite() || curvature <= 0.0 {
            return Err(format!(
                "the stiffness matrix is not positive definite in the search direction \
                 (iteration {iteration}); check that the supports remove rigid-body motion"
            ));
        }
        let alpha = rz / curvature;
        for (value, step) in u.iter_mut().zip(direction.iter()) {
            *value += alpha * step;
        }
        for (value, step) in residual.iter_mut().zip(ap.iter()) {
            *value -= alpha * step;
        }
        relative_residual = norm(&residual) / reference;
        if !relative_residual.is_finite() {
            return Err("the residual became non-finite".into());
        }
        if relative_residual <= tolerance {
            return Ok((u, iteration, relative_residual));
        }
        for ((value, r), d) in preconditioned
            .iter_mut()
            .zip(residual.iter())
            .zip(diag.iter())
        {
            *value = if *d > 0.0 { r / d } else { 0.0 };
        }
        let next = dot(&residual, &preconditioned);
        if !next.is_finite() || next <= 0.0 {
            return Err("the conjugate gradient recurrence lost positive definiteness".into());
        }
        let beta = next / rz;
        for (direction_value, z) in direction.iter_mut().zip(preconditioned.iter()) {
            *direction_value = z + beta * *direction_value;
        }
        rz = next;
    }
    Err(format!(
        "the solver did not reach a relative residual of {:.1e} in {} iterations (reached {relative_residual:.3e}); \
         increase --max-iter or coarsen the mesh",
        tolerance, max_iterations
    ))
}

/// Jacobi-preconditioned conjugate gradient.
pub(crate) fn solve(
    element_nodes: &[[u32; ELEMENT_NODES]],
    local: &[f64],
    ndof: usize,
    fixed: &[bool],
    force: &[f64],
    settings: &SolverSettings,
) -> Result<SolveOutcome, String> {
    let diag = diagonal(element_nodes, local, ndof);
    let (u, iterations, relative_residual) = solve_system(
        element_nodes,
        local,
        ndof,
        fixed,
        force,
        &diag,
        settings.relative_tolerance,
        settings.max_iterations,
    )?;
    let unconstrained = vec![false; ndof];
    let mut ap = vec![0.0; ndof];
    apply(element_nodes, local, &u, &unconstrained, &mut ap);
    Ok(SolveOutcome {
        displacement: u,
        iterations,
        relative_residual,
        internal_force: ap,
    })
}

/// One computed dynamic natural mode.
pub(crate) struct ModeOutcome {
    pub mode: usize,
    #[allow(dead_code)]
    pub lambda: f64,
    pub frequency_hz: f64,
    pub angular_frequency_rad_s: f64,
    pub generalized_mass: f64,
    pub eigenvector: Vec<f64>,
}

/// The result of dynamic modal analysis.
pub(crate) struct ModalOutcome {
    pub modes: Vec<ModeOutcome>,
}

fn m_orthogonalize(
    v: &mut [f64],
    previous_modes: &[Vec<f64>],
    element_nodes: &[[u32; ELEMENT_NODES]],
    local_mass: &[f64],
    fixed: &[bool],
    temp_mv: &mut [f64],
) {
    for prev in previous_modes {
        apply_mass(element_nodes, local_mass, v, fixed, temp_mv);
        let coeff = dot(prev, temp_mv);
        for (v_val, &p_val) in v.iter_mut().zip(prev.iter()) {
            *v_val -= coeff * p_val;
        }
    }
}

/// Inverse-iteration subspace modal solver with Gram-Schmidt M-deflation.
pub(crate) fn solve_modal(
    element_nodes: &[[u32; ELEMENT_NODES]],
    local_stiffness: &[f64],
    local_mass: &[f64],
    ndof: usize,
    fixed: &[bool],
    num_modes: usize,
    settings: &SolverSettings,
) -> Result<ModalOutcome, String> {
    let diag_k = diagonal(element_nodes, local_stiffness, ndof);
    let mut converged_modes: Vec<Vec<f64>> = Vec::with_capacity(num_modes);
    let mut modes: Vec<ModeOutcome> = Vec::with_capacity(num_modes);
    let mut mv = vec![0.0; ndof];
    let mut mw = vec![0.0; ndof];
    let mut kw = vec![0.0; ndof];

    for mode_idx in 0..num_modes {
        let mut v = vec![0.0; ndof];
        for (i, val) in v.iter_mut().enumerate() {
            if !fixed[i] {
                let phase = (i as f64 + 1.0) * (mode_idx as f64 + 1.0);
                *val = (phase * 0.71828).sin() + (phase * 0.31415).cos();
            }
        }
        // Orthogonalize against previously found modes
        m_orthogonalize(
            &mut v,
            &converged_modes,
            element_nodes,
            local_mass,
            fixed,
            &mut mv,
        );
        m_orthogonalize(
            &mut v,
            &converged_modes,
            element_nodes,
            local_mass,
            fixed,
            &mut mv,
        );
        apply_mass(element_nodes, local_mass, &v, fixed, &mut mv);
        let mass_norm = dot(&v, &mv).sqrt();
        if mass_norm > 0.0 {
            for x in &mut v {
                *x /= mass_norm;
            }
        }

        let mut lambda = 0.0;
        let max_iter = 30;
        for iter in 0..max_iter {
            // y = M * v
            apply_mass(element_nodes, local_mass, &v, fixed, &mut mv);
            // Solve K * w = y
            let (mut w, _, _) = solve_system(
                element_nodes,
                local_stiffness,
                ndof,
                fixed,
                &mv,
                &diag_k,
                settings.relative_tolerance.min(1.0e-6),
                settings.max_iterations,
            )?;
            // Deflate against prior modes
            m_orthogonalize(
                &mut w,
                &converged_modes,
                element_nodes,
                local_mass,
                fixed,
                &mut mw,
            );
            m_orthogonalize(
                &mut w,
                &converged_modes,
                element_nodes,
                local_mass,
                fixed,
                &mut mw,
            );

            apply_mass(element_nodes, local_mass, &w, fixed, &mut mw);
            let w_m_w = dot(&w, &mw);
            if w_m_w <= 0.0 || !w_m_w.is_finite() {
                return Err("mass normalization encountered non-positive inner product".into());
            }
            let w_norm = w_m_w.sqrt();

            apply(element_nodes, local_stiffness, &w, fixed, &mut kw);
            let w_k_w = dot(&w, &kw);
            let new_lambda = (w_k_w / w_m_w).max(0.0);

            let mut diff = 0.0;
            for (v_val, w_val) in v.iter_mut().zip(w.iter()) {
                let next_v = *w_val / w_norm;
                diff += (next_v - *v_val).powi(2);
                *v_val = next_v;
            }

            let rel_change = (new_lambda - lambda).abs() / new_lambda.max(1.0);
            lambda = new_lambda;
            if iter >= 3 && (rel_change < 1.0e-5 || diff.sqrt() < 1.0e-5) {
                break;
            }
        }

        let omega = lambda.sqrt();
        let freq_hz = omega / std::f64::consts::TAU;
        converged_modes.push(v.clone());
        modes.push(ModeOutcome {
            mode: mode_idx + 1,
            lambda,
            frequency_hz: freq_hz,
            angular_frequency_rad_s: omega,
            generalized_mass: 1.0,
            eigenvector: v,
        });
    }

    modes.sort_by(|a, b| {
        a.frequency_hz
            .partial_cmp(&b.frequency_hz)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (idx, m) in modes.iter_mut().enumerate() {
        m.mode = idx + 1;
    }

    Ok(ModalOutcome { modes })
}

#[cfg(test)]
mod tests {
    use super::{ELEMENT_DOFS, Elasticity, centre_stress, hex_stiffness, von_mises};

    #[test]
    fn a_rigid_body_translation_has_no_internal_force() {
        let elasticity = Elasticity::isotropic(200_000.0, 0.3);
        let stiffness = hex_stiffness(1.0, &elasticity);
        let mut ue = [0.0; ELEMENT_DOFS];
        for node in 0..8 {
            ue[node * 3] = 2.5;
            ue[node * 3 + 1] = -1.5;
            ue[node * 3 + 2] = 0.75;
        }
        for row in 0..ELEMENT_DOFS {
            let mut force = 0.0;
            for column in 0..ELEMENT_DOFS {
                force += stiffness[row * ELEMENT_DOFS + column] * ue[column];
            }
            assert!(
                force.abs() < 1.0e-9,
                "rigid translation produced force {force}"
            );
        }
    }

    #[test]
    fn the_stiffness_matrix_is_symmetric() {
        let elasticity = Elasticity::isotropic(70_000.0, 0.33);
        let stiffness = hex_stiffness(2.0, &elasticity);
        for row in 0..ELEMENT_DOFS {
            for column in 0..ELEMENT_DOFS {
                let a = stiffness[row * ELEMENT_DOFS + column];
                let b = stiffness[column * ELEMENT_DOFS + row];
                assert!((a - b).abs() < 1.0e-6 * (1.0 + a.abs()), "{row},{column}");
            }
        }
    }

    #[test]
    fn uniaxial_strain_recovers_the_analytic_stress() {
        // A unit cube stretched by 1e-4 in x with the lateral faces free has
        // sigma_xx = E * eps / (1 + nu) / (1 - 2 nu) * (1 - nu) ... so instead impose a full
        // uniform strain state and compare the uniaxial one by construction: eps_x = e,
        // eps_y = eps_z = -nu e gives sigma_xx = E e and sigma_yy = sigma_zz = 0.
        let e = 200_000.0;
        let nu = 0.25;
        let elasticity = Elasticity::isotropic(e, nu);
        let strain = 1.0e-4;
        let mut ue = [0.0; ELEMENT_DOFS];
        for (node, &(x, y, z)) in [
            (0.0, 0.0, 0.0),
            (2.0, 0.0, 0.0),
            (2.0, 2.0, 0.0),
            (0.0, 2.0, 0.0),
            (0.0, 0.0, 2.0),
            (2.0, 0.0, 2.0),
            (2.0, 2.0, 2.0),
            (0.0, 2.0, 2.0),
        ]
        .iter()
        .enumerate()
        {
            ue[node * 3] = strain * x;
            ue[node * 3 + 1] = -nu * strain * y;
            ue[node * 3 + 2] = -nu * strain * z;
        }
        let stress = centre_stress(&ue, 2.0, &elasticity);
        let expected = e * strain;
        assert!((stress[0] - expected).abs() < 1.0e-6 * expected);
        assert!(stress[1].abs() < 1.0e-6 * expected);
        assert!(stress[2].abs() < 1.0e-6 * expected);
        assert!((von_mises(&stress) - expected).abs() < 1.0e-6 * expected);
    }
}
