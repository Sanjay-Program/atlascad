//! Geometric validation of parametric operations before kernel submission.

use crate::feature::FeatureOperation;
use atlas_core::error::CoreError;

pub struct FeatureValidator;

impl FeatureValidator {
    pub fn validate(op: &FeatureOperation) -> Result<(), CoreError> {
        match op {
            FeatureOperation::Extrude { depth_mm, .. } => {
                if *depth_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Extrude depth must be positive".to_string(),
                    ));
                }
            }
            FeatureOperation::Revolve {
                angle_deg,
                axis_direction,
                ..
            } => {
                if *angle_deg <= 0.0 || *angle_deg > 360.0 {
                    return Err(CoreError::ValidationFailed(
                        "Revolve angle must be in (0, 360] degrees".to_string(),
                    ));
                }
                let mag2 = axis_direction[0] * axis_direction[0]
                    + axis_direction[1] * axis_direction[1]
                    + axis_direction[2] * axis_direction[2];
                if mag2 < 1e-12 {
                    return Err(CoreError::ValidationFailed(
                        "Revolve axis direction vector cannot be zero".to_string(),
                    ));
                }
            }
            FeatureOperation::Fillet {
                radius_mm,
                edge_ids,
            } => {
                if *radius_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Fillet radius must be positive".to_string(),
                    ));
                }
                if edge_ids.is_empty() {
                    return Err(CoreError::ValidationFailed(
                        "Fillet requires at least one target edge".to_string(),
                    ));
                }
            }
            FeatureOperation::Chamfer {
                distance_mm,
                edge_ids,
            } => {
                if *distance_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Chamfer distance must be positive".to_string(),
                    ));
                }
                if edge_ids.is_empty() {
                    return Err(CoreError::ValidationFailed(
                        "Chamfer requires at least one target edge".to_string(),
                    ));
                }
            }
            FeatureOperation::Hole {
                diameter_mm,
                depth_mm,
                through_all,
                ..
            } => {
                if *diameter_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Hole diameter must be positive".to_string(),
                    ));
                }
                if !*through_all && *depth_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Blind hole depth must be positive".to_string(),
                    ));
                }
            }
            FeatureOperation::LinearPattern {
                count, spacing_mm, ..
            } => {
                if *count < 2 {
                    return Err(CoreError::ValidationFailed(
                        "Linear pattern count must be at least 2".to_string(),
                    ));
                }
                if *spacing_mm <= 0.0 {
                    return Err(CoreError::ValidationFailed(
                        "Linear pattern spacing must be positive".to_string(),
                    ));
                }
            }
            FeatureOperation::Mirror { plane_normal, .. } => {
                let mag2 = plane_normal[0] * plane_normal[0]
                    + plane_normal[1] * plane_normal[1]
                    + plane_normal[2] * plane_normal[2];
                if mag2 < 1e-12 {
                    return Err(CoreError::ValidationFailed(
                        "Mirror plane normal vector cannot be zero".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::BooleanOp;

    #[test]
    fn test_feature_validation() {
        let valid_ext = FeatureOperation::Extrude {
            profile_id: "sk1".to_string(),
            depth_mm: 10.0,
            boolean_op: BooleanOp::Join,
        };
        assert!(FeatureValidator::validate(&valid_ext).is_ok());

        let invalid_ext = FeatureOperation::Extrude {
            profile_id: "sk1".to_string(),
            depth_mm: -5.0,
            boolean_op: BooleanOp::Join,
        };
        assert!(FeatureValidator::validate(&invalid_ext).is_err());
    }
}
