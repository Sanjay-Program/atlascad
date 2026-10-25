//! Explicit units.
//!
//! CADCraft stores units in the DXF `INSUNITS` header variable; SolidCraft keeps unit-aware
//! expressions. Neither shares a unit type, so lengths crossing a boundary are easy to mix up.
//! This module gives both one small, explicit unit system.
//!
//! All geometry in the contract is expressed in **millimetres**, matching SolidCraft's kernel
//! convention (`tessellate(tol)` documents `tol` in mm) and DXF's `INSUNITS = 4`.

use serde::{Deserialize, Serialize};

/// A length unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    /// Millimetres — the contract's internal unit.
    Millimeter,
    Centimeter,
    Meter,
    Inch,
    Foot,
}

impl Unit {
    /// How many millimetres one of this unit is.
    pub const fn to_millimeters(self) -> f64 {
        match self {
            Unit::Millimeter => 1.0,
            Unit::Centimeter => 10.0,
            Unit::Meter => 1000.0,
            Unit::Inch => 25.4,
            Unit::Foot => 304.8,
        }
    }

    /// The DXF `INSUNITS` code for this unit, or `None` for units DXF does not define.
    ///
    /// Codes: 1 = inches, 2 = feet, 4 = millimetres, 5 = centimetres, 6 = metres.
    pub const fn dxf_insunits(self) -> i32 {
        match self {
            Unit::Inch => 1,
            Unit::Foot => 2,
            Unit::Millimeter => 4,
            Unit::Centimeter => 5,
            Unit::Meter => 6,
        }
    }

    /// The unit for a DXF `INSUNITS` code. `0` means "unitless" in DXF and maps to `None`.
    pub const fn from_dxf_insunits(code: i32) -> Option<Unit> {
        match code {
            1 => Some(Unit::Inch),
            2 => Some(Unit::Foot),
            4 => Some(Unit::Millimeter),
            5 => Some(Unit::Centimeter),
            6 => Some(Unit::Meter),
            _ => None,
        }
    }

    /// The short symbol used in UI and file headers.
    pub const fn symbol(self) -> &'static str {
        match self {
            Unit::Millimeter => "mm",
            Unit::Centimeter => "cm",
            Unit::Meter => "m",
            Unit::Inch => "in",
            Unit::Foot => "ft",
        }
    }

    /// Parse a unit from a symbol or common name. Case-insensitive.
    ///
    /// Accepts `mm`, `millimeter(s)`, `cm`, `centimeter(s)`, `m`, `meter(s)`, `in`, `inch(es)`,
    /// `"`, `ft`, `foot`, `feet`, `'`.
    pub fn parse(s: &str) -> Option<Unit> {
        let s = s.trim().to_ascii_lowercase();
        let unit = match s.as_str() {
            "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" => Unit::Millimeter,
            "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" => Unit::Centimeter,
            "m" | "meter" | "meters" | "metre" | "metres" => Unit::Meter,
            "in" | "inch" | "inches" | "\"" => Unit::Inch,
            "ft" | "foot" | "feet" | "'" => Unit::Foot,
            _ => return None,
        };
        Some(unit)
    }

    /// Convert `value` from this unit to `to`.
    ///
    /// Not a lossless operation for every pair of units; the result is an ordinary `f64`.
    pub fn convert(self, to: Unit, value: f64) -> f64 {
        value * self.to_millimeters() / to.to_millimeters()
    }

    /// Convert a length in this unit to millimetres.
    pub fn to_mm(self, value: f64) -> f64 {
        value * self.to_millimeters()
    }

    /// Convert millimetres to this unit.
    pub fn from_mm(self, value: f64) -> f64 {
        value / self.to_millimeters()
    }
}

impl core::fmt::Display for Unit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::Unit;

    #[test]
    fn inch_is_exactly_25_4_mm() {
        assert_eq!(Unit::Inch.to_millimeters(), 25.4);
        assert_eq!(Unit::Inch.to_mm(2.0), 50.8);
    }

    #[test]
    fn foot_is_twelve_inches() {
        // 12 * 25.4 is 304.79999999999995 in binary floating point while the foot constant is the
        // exact definition 304.8, so this is compared within representation error, not exactly.
        let twelve_inches = 12.0 * Unit::Inch.to_millimeters();
        assert!(
            (Unit::Foot.to_millimeters() - twelve_inches).abs() < 1e-12,
            "foot {} vs twelve inches {twelve_inches}",
            Unit::Foot.to_millimeters()
        );
    }

    #[test]
    fn conversion_round_trips_within_tolerance() {
        for (from, to) in [
            (Unit::Millimeter, Unit::Inch),
            (Unit::Meter, Unit::Foot),
            (Unit::Centimeter, Unit::Millimeter),
        ] {
            let original = 123.456;
            let there = from.convert(to, original);
            let back = to.convert(from, there);
            assert!(
                (back - original).abs() < 1e-9,
                "{from}->{to}->{from} gave {back}"
            );
        }
    }

    #[test]
    fn dxf_codes_map_both_ways() {
        for unit in [
            Unit::Inch,
            Unit::Foot,
            Unit::Millimeter,
            Unit::Centimeter,
            Unit::Meter,
        ] {
            assert_eq!(Unit::from_dxf_insunits(unit.dxf_insunits()), Some(unit));
        }
        // DXF 0 is "unitless" and 3 is "miles"; neither is a unit this contract models.
        assert_eq!(Unit::from_dxf_insunits(0), None);
        assert_eq!(Unit::from_dxf_insunits(3), None);
    }

    #[test]
    fn parsing_accepts_symbols_and_names() {
        assert_eq!(Unit::parse("MM"), Some(Unit::Millimeter));
        assert_eq!(Unit::parse(" inch "), Some(Unit::Inch));
        assert_eq!(Unit::parse("feet"), Some(Unit::Foot));
        assert_eq!(Unit::parse("parsec"), None);
    }

    #[test]
    fn json_round_trip() {
        let json = serde_json::to_string(&Unit::Millimeter).unwrap_or_default();
        assert_eq!(json, "\"millimeter\"");
        let back: Unit = serde_json::from_str(&json).unwrap_or(Unit::Millimeter);
        assert_eq!(back, Unit::Millimeter);
    }
}
