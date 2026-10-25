//! Geometric tolerances.
//!
//! A tolerance is a *policy* value, not a magic constant. Kernel calls that compare geometry need
//! one, and passing an unvalidated `f64` lets `NaN`, `0` or a negative value reach the kernel where
//! it can produce silently wrong results instead of an error. [`Tolerance`] can only be built from
//! finite, positive values.

use serde::{Deserialize, Deserializer, Serialize};

/// The default chord/linear tolerance in millimetres (the value SolidCraft's tessellator falls
/// back to for a missing or invalid tolerance).
pub const DEFAULT_LINEAR_MM: f64 = 0.05;

/// The default angular tolerance in radians (~0.29°).
pub const DEFAULT_ANGULAR_RAD: f64 = 0.005;

/// The largest linear tolerance a model can meaningfully use, in millimetres (1 metre).
///
/// Beyond this, "coincident" would mean "a metre apart"; such a value is a units mistake rather
/// than a modelling choice, so it is rejected.
pub const MAX_LINEAR_MM: f64 = 1_000.0;

/// Why a tolerance was rejected.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum ToleranceError {
    #[error("linear tolerance {0} is not a finite positive number")]
    LinearNotFinitePositive(f64),
    #[error("linear tolerance {value} mm exceeds the {max} mm limit; check the model's units")]
    LinearTooLarge { value: f64, max: f64 },
    #[error("angular tolerance {0} is not a finite positive number")]
    AngularNotFinitePositive(f64),
    #[error("angular tolerance {value} rad exceeds π/2")]
    AngularTooLarge { value: f64 },
}

/// A validated pair of linear (mm) and angular (rad) tolerances.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Tolerance {
    linear_mm: f64,
    angular_rad: f64,
}

impl Tolerance {
    /// Build a tolerance, rejecting values that cannot describe geometry.
    pub fn new(linear_mm: f64, angular_rad: f64) -> Result<Self, ToleranceError> {
        if !linear_mm.is_finite() || linear_mm <= 0.0 {
            return Err(ToleranceError::LinearNotFinitePositive(linear_mm));
        }
        if linear_mm > MAX_LINEAR_MM {
            return Err(ToleranceError::LinearTooLarge {
                value: linear_mm,
                max: MAX_LINEAR_MM,
            });
        }
        if !angular_rad.is_finite() || angular_rad <= 0.0 {
            return Err(ToleranceError::AngularNotFinitePositive(angular_rad));
        }
        if angular_rad > core::f64::consts::FRAC_PI_2 {
            return Err(ToleranceError::AngularTooLarge { value: angular_rad });
        }
        Ok(Tolerance {
            linear_mm,
            angular_rad,
        })
    }

    /// Linear tolerance in millimetres.
    pub const fn linear_mm(self) -> f64 {
        self.linear_mm
    }

    /// Angular tolerance in radians.
    pub const fn angular_rad(self) -> f64 {
        self.angular_rad
    }

    /// The defaults used when a caller has no reason to choose otherwise.
    pub fn default_pair() -> Self {
        Tolerance {
            linear_mm: DEFAULT_LINEAR_MM,
            angular_rad: DEFAULT_ANGULAR_RAD,
        }
    }
}

impl Default for Tolerance {
    fn default() -> Self {
        Tolerance::default_pair()
    }
}

impl<'de> Deserialize<'de> for Tolerance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            linear_mm: f64,
            angular_rad: f64,
        }
        let raw = Raw::deserialize(deserializer)?;
        Tolerance::new(raw.linear_mm, raw.angular_rad).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::Tolerance;

    #[test]
    fn default_is_valid() {
        let t = Tolerance::default();
        assert_eq!(t.linear_mm(), super::DEFAULT_LINEAR_MM);
        assert!(t.angular_rad() > 0.0);
    }

    #[test]
    fn non_finite_and_non_positive_are_rejected() {
        assert!(Tolerance::new(f64::NAN, 0.01).is_err());
        assert!(Tolerance::new(f64::INFINITY, 0.01).is_err());
        assert!(Tolerance::new(0.0, 0.01).is_err());
        assert!(Tolerance::new(-1.0, 0.01).is_err());
        assert!(Tolerance::new(0.01, f64::NAN).is_err());
        assert!(Tolerance::new(0.01, 0.0).is_err());
    }

    #[test]
    fn absurd_linear_tolerance_is_rejected_with_a_units_hint() {
        // A model authored in metres but read as millimetres is the realistic cause.
        let err = Tolerance::new(5_000.0, 0.01);
        assert!(err.is_err());
        let msg = err.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.contains("units"),
            "message should mention units, got {msg}"
        );
    }

    #[test]
    fn json_round_trip_revalidates() {
        let t = Tolerance::new(0.01, 0.001).unwrap_or_default();
        let json = serde_json::to_string(&t).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Tolerance>(&json).ok(), Some(t));
        // A tampered file with a zero tolerance must not load.
        assert!(
            serde_json::from_str::<Tolerance>(r#"{"linear_mm":0.0,"angular_rad":0.001}"#).is_err()
        );
    }
}
