//! # Dimensional Units & Physical Quantities
//!
//! Provides type-safe SI & Imperial unit conversions, dimensionality verification,
//! and physical boundary validation for engineering calculations across AtlasCAD.

use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// Standard SI and Imperial length units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Meter,
    Kilometer,
    Inch,
    Foot,
    Yard,
    Mil, // 1/1000 inch
}

impl LengthUnit {
    /// Conversion factor to SI base unit (meter).
    pub fn to_meters_factor(self) -> f64 {
        match self {
            Self::Millimeter => 1.0e-3,
            Self::Centimeter => 1.0e-2,
            Self::Meter => 1.0,
            Self::Kilometer => 1.0e3,
            Self::Inch => 0.0254,
            Self::Foot => 0.3048,
            Self::Yard => 0.9144,
            Self::Mil => 0.0000254,
        }
    }

    /// Converts a value from this unit to another length unit.
    pub fn convert_to(self, value: f64, target: LengthUnit) -> f64 {
        let in_meters = value * self.to_meters_factor();
        in_meters / target.to_meters_factor()
    }
}

/// A physical length measurement with type-safe unit conversion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Length {
    value_meters: f64,
}

impl Length {
    pub fn from_meters(meters: f64) -> Result<Self, CoreError> {
        if !meters.is_finite() || meters < 0.0 {
            return Err(CoreError::ValidationFailed(
                "Length must be a non-negative finite value".to_string(),
            ));
        }
        Ok(Self {
            value_meters: meters,
        })
    }

    pub fn from_mm(mm: f64) -> Result<Self, CoreError> {
        Self::from_meters(mm * 1.0e-3)
    }

    pub fn from_inches(inches: f64) -> Result<Self, CoreError> {
        Self::from_meters(inches * 0.0254)
    }

    pub fn from_feet(feet: f64) -> Result<Self, CoreError> {
        Self::from_meters(feet * 0.3048)
    }

    pub fn as_meters(self) -> f64 {
        self.value_meters
    }

    pub fn as_mm(self) -> f64 {
        self.value_meters * 1.0e3
    }

    pub fn as_inches(self) -> f64 {
        self.value_meters / 0.0254
    }

    pub fn as_feet(self) -> f64 {
        self.value_meters / 0.3048
    }
}

/// Standard pressure and mechanical stress units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressureUnit {
    Pascal,
    Kilopascal,
    Megapascal,
    Gigapascal,
    Bar,
    Psi,
    Ksi,
    Atmosphere,
}

impl PressureUnit {
    /// Conversion factor to SI base unit (Pascal = N/m²).
    pub fn to_pascals_factor(self) -> f64 {
        match self {
            Self::Pascal => 1.0,
            Self::Kilopascal => 1.0e3,
            Self::Megapascal => 1.0e6,
            Self::Gigapascal => 1.0e9,
            Self::Bar => 1.0e5,
            Self::Psi => 6894.757293,
            Self::Ksi => 6894757.293,
            Self::Atmosphere => 101325.0,
        }
    }

    pub fn convert_to(self, value: f64, target: PressureUnit) -> f64 {
        let in_pa = value * self.to_pascals_factor();
        in_pa / target.to_pascals_factor()
    }
}

/// A physical pressure or stress measurement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pressure {
    value_pa: f64,
}

impl Pressure {
    pub fn from_pascals(pa: f64) -> Result<Self, CoreError> {
        if !pa.is_finite() {
            return Err(CoreError::ValidationFailed(
                "Pressure must be a finite real number".to_string(),
            ));
        }
        Ok(Self { value_pa: pa })
    }

    pub fn from_mpa(mpa: f64) -> Result<Self, CoreError> {
        Self::from_pascals(mpa * 1.0e6)
    }

    pub fn from_bar(bar: f64) -> Result<Self, CoreError> {
        Self::from_pascals(bar * 1.0e5)
    }

    pub fn from_psi(psi: f64) -> Result<Self, CoreError> {
        Self::from_pascals(psi * 6894.757293)
    }

    pub fn as_pa(self) -> f64 {
        self.value_pa
    }

    pub fn as_mpa(self) -> f64 {
        self.value_pa / 1.0e6
    }

    pub fn as_gpa(self) -> f64 {
        self.value_pa / 1.0e9
    }

    pub fn as_bar(self) -> f64 {
        self.value_pa / 1.0e5
    }

    pub fn as_psi(self) -> f64 {
        self.value_pa / 6894.757293
    }
}

/// Thermodynamic temperature representations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Temperature {
    kelvin: f64,
}

impl Temperature {
    pub fn from_kelvin(k: f64) -> Result<Self, CoreError> {
        if !k.is_finite() || k < 0.0 {
            return Err(CoreError::ValidationFailed(
                "Temperature cannot drop below absolute zero (0 K)".to_string(),
            ));
        }
        Ok(Self { kelvin: k })
    }

    pub fn from_celsius(c: f64) -> Result<Self, CoreError> {
        Self::from_kelvin(c + 273.15)
    }

    pub fn from_fahrenheit(f: f64) -> Result<Self, CoreError> {
        Self::from_kelvin((f - 32.0) * (5.0 / 9.0) + 273.15)
    }

    pub fn as_kelvin(self) -> f64 {
        self.kelvin
    }

    pub fn as_celsius(self) -> f64 {
        self.kelvin - 273.15
    }

    pub fn as_fahrenheit(self) -> f64 {
        (self.kelvin - 273.15) * (9.0 / 5.0) + 32.0
    }
}

/// Force measurement units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Force {
    value_newtons: f64,
}

impl Force {
    pub fn from_newtons(n: f64) -> Result<Self, CoreError> {
        if !n.is_finite() {
            return Err(CoreError::ValidationFailed(
                "Force must be a finite real number".to_string(),
            ));
        }
        Ok(Self { value_newtons: n })
    }

    pub fn from_kilonewtons(kn: f64) -> Result<Self, CoreError> {
        Self::from_newtons(kn * 1.0e3)
    }

    pub fn from_lbf(lbf: f64) -> Result<Self, CoreError> {
        Self::from_newtons(lbf * 4.4482216152605)
    }

    pub fn as_newtons(self) -> f64 {
        self.value_newtons
    }

    pub fn as_kn(self) -> f64 {
        self.value_newtons / 1.0e3
    }

    pub fn as_lbf(self) -> f64 {
        self.value_newtons / 4.4482216152605
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_conversion_exactness() -> Result<(), CoreError> {
        let l = Length::from_inches(1.0)?;
        assert!((l.as_mm() - 25.4).abs() < 1e-12);
        assert!((l.as_meters() - 0.0254).abs() < 1e-12);

        let l_foot = Length::from_feet(1.0)?;
        assert!((l_foot.as_inches() - 12.0).abs() < 1e-12);
        assert!((l_foot.as_mm() - 304.8).abs() < 1e-12);

        // Negative length rejected
        assert!(Length::from_meters(-5.0).is_err());
        Ok(())
    }

    #[test]
    fn test_pressure_conversion() -> Result<(), CoreError> {
        let p = Pressure::from_bar(1.0)?;
        assert!((p.as_pa() - 100_000.0).abs() < 1e-6);
        assert!((p.as_mpa() - 0.1).abs() < 1e-6);
        assert!((p.as_psi() - 14.50377).abs() < 1e-3);

        let p_ksi = PressureUnit::Ksi.convert_to(1.0, PressureUnit::Megapascal);
        assert!((p_ksi - 6.894757).abs() < 1e-4);
        Ok(())
    }

    #[test]
    fn test_temperature_conversion_and_bounds() -> Result<(), CoreError> {
        let t0 = Temperature::from_celsius(0.0)?;
        assert!((t0.as_kelvin() - 273.15).abs() < 1e-9);
        assert!((t0.as_fahrenheit() - 32.0).abs() < 1e-9);

        let boiling = Temperature::from_fahrenheit(212.0)?;
        assert!((boiling.as_celsius() - 100.0).abs() < 1e-9);

        // Absolute zero boundary check
        assert!(Temperature::from_kelvin(-0.01).is_err());
        assert!(Temperature::from_celsius(-273.16).is_err());
        Ok(())
    }

    #[test]
    fn test_force_conversion() -> Result<(), CoreError> {
        let f = Force::from_kilonewtons(1.0)?;
        assert_eq!(f.as_newtons(), 1000.0);
        assert!((f.as_lbf() - 224.8089).abs() < 1e-3);
        Ok(())
    }
}
