//! Numerical validation against closed-form analytical solutions.

use serde::{Deserialize, Serialize};

/// Status of analytical engineering validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// Numerical result matches analytical closed-form solution within tolerance (e.g. < 1%).
    ValidationPassed,
    /// Numerical result deviates from analytical solution with moderate error (e.g. 1% - 5%).
    ValidationWarning,
    /// Significant deviation from theoretical solution (> 5%).
    ValidationFailed,
}

/// Comparison between computed numerical output and theoretical reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyticalComparison {
    pub metric_name: String,
    pub numerical_value: f64,
    pub analytical_value: f64,
    pub error_percent: f64,
    pub status: ValidationStatus,
}

impl AnalyticalComparison {
    /// Compute relative error and assess validation status.
    pub fn assess(metric_name: impl Into<String>, numerical: f64, analytical: f64) -> Self {
        let diff = (numerical - analytical).abs();
        let denom = if analytical.abs() > 1e-12 {
            analytical.abs()
        } else {
            1.0
        };
        let error_percent = (diff / denom) * 100.0;

        let status = if error_percent <= 1.0 {
            ValidationStatus::ValidationPassed
        } else if error_percent <= 5.0 {
            ValidationStatus::ValidationWarning
        } else {
            ValidationStatus::ValidationFailed
        };

        Self {
            metric_name: metric_name.into(),
            numerical_value: numerical,
            analytical_value: analytical,
            error_percent,
            status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_assessment() {
        // Tip deflection: analytical 0.420 mm, numerical 0.422 mm -> ~0.47% error -> Passed
        let comp = AnalyticalComparison::assess("Tip Deflection", 0.422, 0.420);
        assert_eq!(comp.status, ValidationStatus::ValidationPassed);
        assert!(comp.error_percent < 0.5);

        // Huge error -> Failed
        let failed = AnalyticalComparison::assess("Stress", 250.0, 180.0);
        assert_eq!(failed.status, ValidationStatus::ValidationFailed);
    }
}
