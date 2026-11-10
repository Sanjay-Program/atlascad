//! Damped least squares (Levenberg-Marquardt style) nonlinear solver
//! and rank-revealing QR degrees-of-freedom analyzer.
//!
//! Pure Rust, zero unsafe code, resilient to singular configurations.

#![forbid(unsafe_code)]

/// Outcome of a nonlinear solve.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub x: Vec<f64>,
    pub converged: bool,
    pub iterations: usize,
    pub final_residual: f64,
}

pub(crate) fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0f64, |m, a| {
        if a.is_finite() {
            m.max(a.abs())
        } else {
            f64::INFINITY
        }
    })
}

pub(crate) fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|a| a * a).sum::<f64>()
}

/// Numerical Jacobian matrix over free parameters (row-major: `rows × free.len()`).
pub fn jacobian(
    f: &dyn Fn(&[f64], &mut Vec<f64>),
    x: &[f64],
    free: &[usize],
    rows: usize,
) -> Vec<f64> {
    let nf = free.len();
    let mut jac = vec![0.0; rows * nf];
    let mut xp = x.to_vec();
    let mut rp = Vec::with_capacity(rows);
    let mut rm = Vec::with_capacity(rows);

    for (k, &j) in free.iter().enumerate() {
        let Some(&xj) = x.get(j) else { continue };
        let h = 1e-7 * xj.abs().max(1.0);
        if let Some(v) = xp.get_mut(j) {
            *v = xj + h;
        }
        rp.clear();
        f(&xp, &mut rp);
        if let Some(v) = xp.get_mut(j) {
            *v = xj - h;
        }
        rm.clear();
        f(&xp, &mut rm);
        if let Some(v) = xp.get_mut(j) {
            *v = xj;
        }
        for i in 0..rows {
            let r_plus = rp.get(i).copied().unwrap_or(0.0);
            let r_minus = rm.get(i).copied().unwrap_or(0.0);
            let d = (r_plus - r_minus) / (2.0 * h);
            if let Some(slot) = jac.get_mut(i * nf + k) {
                *slot = if d.is_finite() { d } else { 0.0 };
            }
        }
    }
    jac
}

/// Solve dense linear system `A y = b` ($n \times n$) via Gaussian elimination with partial pivoting.
pub fn solve_dense(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    let scale = a.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    for col in 0..n {
        let mut piv = col;
        let mut best = 0.0;
        for r in col..n {
            let v = a.get(r * n + col)?.abs();
            if v > best {
                best = v;
                piv = r;
            }
        }
        if best <= scale * 1e-15 {
            return None;
        }
        if piv != col {
            for c in 0..n {
                a.swap(piv * n + c, col * n + c);
            }
            b.swap(piv, col);
        }
        let p = *a.get(col * n + col)?;
        for r in col + 1..n {
            let f = *a.get(r * n + col)? / p;
            if f != 0.0 {
                for c in col..n {
                    let v = *a.get(col * n + c)?;
                    *a.get_mut(r * n + c)? -= f * v;
                }
                let bv = *b.get(col)?;
                *b.get_mut(r)? -= f * bv;
            }
        }
    }
    let mut y = vec![0.0; n];
    for r in (0..n).rev() {
        let mut s = *b.get(r)?;
        for c in r + 1..n {
            s -= a.get(r * n + c)? * y.get(c)?;
        }
        *y.get_mut(r)? = s / a.get(r * n + r)?;
    }
    if y.iter().all(|v| v.is_finite()) {
        Some(y)
    } else {
        None
    }
}

/// Householder QR decomposition with column pivoting.
///
/// Returns (numerical_rank, householder_reflectors).
pub fn qr(mut cols: Vec<Vec<f64>>, rel_tol: f64, abs_tol: f64) -> (usize, Vec<Vec<f64>>) {
    let m = cols.first().map_or(0, Vec::len);
    let n = cols.len();
    let mut refl = Vec::new();
    let mut first_norm = None;

    for k in 0..m.min(n) {
        let mut best = (k, -1.0f64);
        for (j, c) in cols.iter().enumerate().skip(k) {
            let v = c.get(k..).map_or(0.0, norm2);
            if v.is_finite() && v > best.1 {
                best = (j, v);
            }
        }
        let norm = best.1.max(0.0).sqrt();
        let lim = first_norm.map_or(abs_tol, |f: f64| (f * rel_tol).max(abs_tol));
        if norm <= lim {
            break;
        }
        first_norm.get_or_insert(norm);
        cols.swap(k, best.0);
        let Some(col) = cols.get(k) else { break };
        let mut v = vec![0.0; m];
        for i in k..m {
            if let (Some(d), Some(s)) = (v.get_mut(i), col.get(i)) {
                *d = *s;
            }
        }
        let x0 = v.get(k).copied().unwrap_or(0.0);
        let alpha = if x0 >= 0.0 { -norm } else { norm };
        if let Some(d) = v.get_mut(k) {
            *d -= alpha;
        }
        let vv = norm2(&v);
        if vv <= 1e-18 {
            break;
        }
        for c in cols.iter_mut().skip(k) {
            reflect(&v, vv, c);
        }
        refl.push(v);
    }
    (refl.len(), refl)
}

fn reflect(v: &[f64], vv: f64, c: &mut [f64]) {
    let d: f64 = v.iter().zip(c.iter()).map(|(a, b)| a * b).sum();
    let f = 2.0 * d / vv;
    if f != 0.0 {
        for (ci, vi) in c.iter_mut().zip(v) {
            *ci -= f * vi;
        }
    }
}

/// Balanced row extraction for Jacobian rank analysis.
fn balanced_rows(m: &[f64], rows: usize, cols: usize) -> Vec<Vec<f64>> {
    let mut r: Vec<Vec<f64>> = (0..rows)
        .map(|i| {
            m.get(i * cols..(i + 1) * cols)
                .map(<[f64]>::to_vec)
                .unwrap_or_else(|| vec![0.0; cols])
        })
        .collect();

    for row in &mut r {
        let n = norm2(row).sqrt();
        let f = if n > 1e-12 && n.is_finite() {
            1.0 / n
        } else {
            0.0
        };
        for v in row.iter_mut() {
            *v *= f;
        }
    }

    for c in 0..cols {
        let n = r
            .iter()
            .map(|row| row.get(c).map_or(0.0, |v| v * v))
            .sum::<f64>()
            .sqrt();
        let f = if n > 1e-6 && n.is_finite() {
            1.0 / n
        } else {
            0.0
        };
        for row in &mut r {
            if let Some(v) = row.get_mut(c) {
                *v *= f;
            }
        }
    }
    r
}

/// Compute numerical rank of row-major `rows × cols` matrix.
pub fn rank(m: &[f64], rows: usize, cols: usize) -> usize {
    if rows == 0 || cols == 0 {
        return 0;
    }
    qr(balanced_rows(m, rows, cols), 1e-7, 1e-12).0
}

/// Solve nonlinear constraints using damped Gauss-Newton / Levenberg-Marquardt.
pub fn solve(
    f: &dyn Fn(&[f64], &mut Vec<f64>),
    x0: &[f64],
    free: &[usize],
    weights: &[f64],
    tol: f64,
    max_iter: usize,
) -> Outcome {
    let mut x = x0.to_vec();
    let mut r = Vec::new();
    f(&x, &mut r);
    let rows = r.len();

    let initial_res = max_abs(&r);
    if rows == 0 || initial_res <= tol {
        return Outcome {
            x,
            converged: true,
            iterations: 0,
            final_residual: initial_res,
        };
    }
    if free.is_empty() {
        return Outcome {
            x,
            converged: false,
            iterations: 0,
            final_residual: initial_res,
        };
    }

    let nf = free.len();
    let winv: Vec<f64> = free
        .iter()
        .map(|&j| 1.0 / weights.get(j).copied().unwrap_or(1.0).max(1e-12))
        .collect();

    let mut mu = 1e-8;
    let mut cost = norm2(&r);
    let mut iter = 0;
    let mut rn = Vec::with_capacity(rows);

    while iter < max_iter {
        iter += 1;
        let jac = jacobian(f, &x, free, rows);

        // Form normal equations in dual space: A = J W^-1 J^T (rows x rows)
        let mut a = vec![0.0; rows * rows];
        for i in 0..rows {
            for k in i..rows {
                let mut s = 0.0;
                for (c, &w) in winv.iter().enumerate() {
                    let j_ic = jac.get(i * nf + c).copied().unwrap_or(0.0);
                    let j_kc = jac.get(k * nf + c).copied().unwrap_or(0.0);
                    s += j_ic * j_kc * w;
                }
                if let Some(v) = a.get_mut(i * rows + k) {
                    *v = s;
                }
                if let Some(v) = a.get_mut(k * rows + i) {
                    *v = s;
                }
            }
        }

        let diag_max = (0..rows)
            .map(|i| a.get(i * rows + i).copied().unwrap_or(0.0))
            .fold(0.0f64, f64::max)
            .max(1e-12);

        let mut improved = false;
        for _ in 0..12 {
            let mut ad = a.clone();
            for i in 0..rows {
                if let Some(v) = ad.get_mut(i * rows + i) {
                    *v += mu * diag_max;
                }
            }
            let Some(y) = solve_dense(ad, r.clone(), rows) else {
                mu *= 10.0;
                continue;
            };

            let mut xn = x.clone();
            for (c, &j) in free.iter().enumerate() {
                let mut s = 0.0;
                for (i, &yi) in y.iter().enumerate() {
                    let j_ic = jac.get(i * nf + c).copied().unwrap_or(0.0);
                    s += j_ic * yi;
                }
                if let Some(v) = xn.get_mut(j) {
                    *v -= winv.get(c).copied().unwrap_or(1.0) * s;
                }
            }

            rn.clear();
            f(&xn, &mut rn);
            let cn = norm2(&rn);
            if cn.is_finite() && cn < cost {
                x = xn;
                std::mem::swap(&mut r, &mut rn);
                cost = cn;
                mu = (mu * 0.1).max(1e-15);
                improved = true;
                break;
            }
            mu *= 10.0;
        }

        let cur_res = max_abs(&r);
        if cur_res <= tol {
            return Outcome {
                x,
                converged: true,
                iterations: iter,
                final_residual: cur_res,
            };
        }

        if !improved && mu > 1e12 {
            break;
        }
    }

    let final_res = max_abs(&r);
    Outcome {
        x,
        converged: final_res <= tol,
        iterations: iter,
        final_residual: final_res,
    }
}
