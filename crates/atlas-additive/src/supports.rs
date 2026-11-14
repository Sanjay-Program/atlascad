//! Additive manufacturing support generation, overhang angle detection, and support volume estimation.

use serde::{Deserialize, Serialize};

/// Type of 3D printing support structure geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportPattern {
    LinearPillars,
    TreeBranches,
    GridTowers,
}

/// Parameters governing support generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportSettings {
    pub critical_overhang_angle_deg: f64, // e.g. 45.0 degrees
    pub support_density: f64,             // 0.05 to 0.50 (default 0.15 = 15% infill)
    pub pattern: SupportPattern,
    pub z_contact_distance_mm: f64, // e.g. 0.2 mm air gap for breakaway
    pub xy_separation_distance_mm: f64, // e.g. 0.5 mm
}

impl Default for SupportSettings {
    fn default() -> Self {
        Self {
            critical_overhang_angle_deg: 45.0,
            support_density: 0.15,
            pattern: SupportPattern::TreeBranches,
            z_contact_distance_mm: 0.20,
            xy_separation_distance_mm: 0.50,
        }
    }
}

/// Analysis result of overhang features requiring support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportAnalysis {
    pub requires_support: bool,
    pub overhang_area_mm2: f64,
    pub estimated_support_volume_mm3: f64,
    pub estimated_support_mass_grams: f64,
    pub critical_facet_count: usize,
}

/// Tests whether a downward-facing normal requires support under the specified threshold.
pub fn is_overhang_requiring_support(normal: [f64; 3], critical_angle_deg: f64) -> bool {
    let norm = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if norm < 1e-6 {
        return false;
    }

    let nz = normal[2] / norm;
    // Only downward-facing facets (nz < -0.01) can require support
    if nz >= -0.01 {
        return false;
    }

    // Angle of the surface plane with the horizontal build plate:
    // For a horizontal plane, normal is vertical (n_xy = 0), so plane inclination theta = 0 deg.
    // For a vertical plane, normal is horizontal (n_xy = 1), so plane inclination theta = 90 deg.
    // A surface plane requires support if its inclination with the horizontal is less than critical_angle_deg.
    let n_xy = (normal[0] * normal[0] + normal[1] * normal[1]).sqrt();
    let theta_plane_horizontal_deg = (n_xy / norm).clamp(0.0, 1.0).asin().to_degrees();
    theta_plane_horizontal_deg < critical_angle_deg
}

/// Analyzes a set of triangular facets for support generation and volume estimation.
pub fn analyze_overhang_supports(
    facets: &[([f64; 3], [f64; 3], [f64; 3])], // Array of (v0, v1, v2) triangles in mm
    settings: &SupportSettings,
    material_density_g_cm3: f64,
) -> SupportAnalysis {
    let mut overhang_area = 0.0;
    let mut support_volume = 0.0;
    let mut critical_count = 0;

    for (v0, v1, v2) in facets {
        // Compute outward face normal via cross product
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];

        if is_overhang_requiring_support(normal, settings.critical_overhang_angle_deg) {
            critical_count += 1;
            let normal_len =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            let area = 0.5 * normal_len; // Triangle 3D area
            overhang_area += area;

            // Height above build plate (z = 0)
            let avg_z = ((v0[2] + v1[2] + v2[2]) / 3.0).max(0.0);
            // Projected support volume down to build plate scaled by support density
            support_volume += area * avg_z * settings.support_density;
        }
    }

    // Mass: volume (mm3) * density (g/cm3) / 1000 mm3 per cm3
    let mass_grams = (support_volume * material_density_g_cm3) / 1000.0;

    SupportAnalysis {
        requires_support: critical_count > 0,
        overhang_area_mm2: overhang_area,
        estimated_support_volume_mm3: support_volume,
        estimated_support_mass_grams: mass_grams,
        critical_facet_count: critical_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overhang_support_detection() {
        // Vertical wall normal: [1, 0, 0] -> no support
        assert!(!is_overhang_requiring_support([1.0, 0.0, 0.0], 45.0));

        // Upward facing normal: [0, 0, 1] -> no support
        assert!(!is_overhang_requiring_support([0.0, 0.0, 1.0], 45.0));

        // Flat horizontal bottom ceiling: [0, 0, -1] -> severe overhang, requires support!
        assert!(is_overhang_requiring_support([0.0, 0.0, -1.0], 45.0));

        // Downward 30-degree shallow plane (n_xy = 0.5, nz = -0.866): theta = 30 deg < 45 deg -> requires support
        assert!(is_overhang_requiring_support([0.5, 0.0, -0.866], 45.0));

        // Downward 60-degree steep plane (n_xy = 0.866, nz = -0.5): theta = 60 deg > 45 deg -> self-supporting!
        assert!(!is_overhang_requiring_support([0.866, 0.0, -0.5], 45.0));
    }

    #[test]
    fn test_facet_support_volume_estimation() {
        let settings = SupportSettings::default();
        // A horizontal flat bridge 20 mm x 20 mm at z = 30 mm facing downward (-Z)
        let tri_d1 = ([0.0, 0.0, 30.0], [20.0, 20.0, 30.0], [20.0, 0.0, 30.0]);
        let tri_d2 = ([0.0, 0.0, 30.0], [0.0, 20.0, 30.0], [20.0, 20.0, 30.0]);

        let analysis = analyze_overhang_supports(&[tri_d1, tri_d2], &settings, 1.24); // PLA 1.24 g/cm3
        assert!(analysis.requires_support);
        assert_eq!(analysis.critical_facet_count, 2);
        assert!((analysis.overhang_area_mm2 - 400.0).abs() < 1e-3);
        assert!(analysis.estimated_support_volume_mm3 > 0.0);
        assert!(analysis.estimated_support_mass_grams > 0.0);
    }
}
