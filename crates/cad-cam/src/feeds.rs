//! Feeds and speeds from material cutting data.
//!
//! The numbers here are **typical starting values** for a small carbide end mill, not a machining
//! datasheet: the source string on every material says so. Surface speed `Vc` and feed per tooth
//! `fz` are scaled to the tool and then held inside the machine's spindle and feed limits.
//!
//! The formulas are the conventional ones:
//!
//! ```text
//! rpm   = 1000 * Vc / (pi * D)      (clamped to the machine spindle limit)
//! feed  = rpm * flutes * fz         (clamped to the machine feed limit)
//! fz    = fz(6 mm) * D / 6 mm       (chip load grows with the tool)
//! ```
//!
//! A limit is reported, never hidden: the caller can see whether the spindle or the feed capped the
//! recommendation, and the actual surface speed and chip load after clamping are returned. The
//! plunge feed is 40 % of the cutting feed, the same ratio the conservative default uses.

use std::f64::consts::PI;
use std::fmt;

/// A workpiece material with typical cutting data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    /// Aluminium 6061, the common free-machining plate.
    Aluminium,
    /// Free-cutting brass.
    Brass,
    /// Low-carbon mild steel 1018.
    MildSteel,
    /// Austenitic stainless steel 304.
    StainlessSteel,
    /// Acetal (POM), a common engineering plastic.
    Acetal,
    /// A medium hardwood.
    Hardwood,
}

/// Typical cutting data at a 6 mm reference diameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuttingData {
    /// Surface speed in metres per minute.
    pub surface_speed_m_min: f64,
    /// Feed per tooth in millimetres at a 6 mm tool.
    pub chip_load_mm_per_tooth: f64,
    /// Where the number comes from, in one sentence.
    pub source: &'static str,
}

impl Material {
    /// Every material this calculator knows.
    pub const ALL: [Material; 6] = [
        Material::Aluminium,
        Material::Brass,
        Material::MildSteel,
        Material::StainlessSteel,
        Material::Acetal,
        Material::Hardwood,
    ];

    /// A stable, lowercase name for reports and job files.
    pub const fn name(self) -> &'static str {
        match self {
            Material::Aluminium => "aluminium",
            Material::Brass => "brass",
            Material::MildSteel => "mild_steel",
            Material::StainlessSteel => "stainless_steel",
            Material::Acetal => "acetal",
            Material::Hardwood => "hardwood",
        }
    }

    /// A shop-floor label.
    pub const fn label(self) -> &'static str {
        match self {
            Material::Aluminium => "aluminium 6061",
            Material::Brass => "free-cutting brass",
            Material::MildSteel => "mild steel 1018",
            Material::StainlessSteel => "stainless steel 304",
            Material::Acetal => "acetal (POM)",
            Material::Hardwood => "medium hardwood",
        }
    }

    /// Typical cutting data. The source string is part of the value on purpose.
    pub const fn cutting_data(self) -> CuttingData {
        const SOURCE: &str = "typical starting values for a small solid-carbide end mill; verify \
                              against the tool supplier and the machine before cutting";
        match self {
            Material::Aluminium => CuttingData {
                surface_speed_m_min: 250.0,
                chip_load_mm_per_tooth: 0.020,
                source: SOURCE,
            },
            Material::Brass => CuttingData {
                surface_speed_m_min: 150.0,
                chip_load_mm_per_tooth: 0.018,
                source: SOURCE,
            },
            Material::MildSteel => CuttingData {
                surface_speed_m_min: 90.0,
                chip_load_mm_per_tooth: 0.015,
                source: SOURCE,
            },
            Material::StainlessSteel => CuttingData {
                surface_speed_m_min: 50.0,
                chip_load_mm_per_tooth: 0.012,
                source: SOURCE,
            },
            Material::Acetal => CuttingData {
                surface_speed_m_min: 300.0,
                chip_load_mm_per_tooth: 0.025,
                source: SOURCE,
            },
            Material::Hardwood => CuttingData {
                surface_speed_m_min: 180.0,
                chip_load_mm_per_tooth: 0.030,
                source: SOURCE,
            },
        }
    }

    /// Parse a material name, accepting the common spellings a shop floor or a job file uses.
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        match name.as_str() {
            "aluminium" | "aluminum" | "alu" | "6061" => Some(Material::Aluminium),
            "brass" => Some(Material::Brass),
            "mild_steel" | "steel" | "1018" => Some(Material::MildSteel),
            "stainless" | "stainless_steel" | "304" | "ss" => Some(Material::StainlessSteel),
            "acetal" | "pom" | "plastic" => Some(Material::Acetal),
            "hardwood" | "wood" | "oak" => Some(Material::Hardwood),
            _ => None,
        }
    }

    /// The names a user can pass, for an error message.
    pub fn known_names() -> String {
        Material::ALL
            .iter()
            .map(|material| material.name())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Taylor tool life equation parameters ($V_c \cdot T^n = C$) for standard solid carbide end mills.
    pub const fn taylor_parameters(self) -> TaylorParameters {
        match self {
            Material::Aluminium => TaylorParameters { n: 0.35, c: 800.0 },
            Material::Brass => TaylorParameters { n: 0.30, c: 500.0 },
            Material::MildSteel => TaylorParameters { n: 0.25, c: 260.0 },
            Material::StainlessSteel => TaylorParameters { n: 0.20, c: 180.0 },
            Material::Acetal => TaylorParameters { n: 0.40, c: 1000.0 },
            Material::Hardwood => TaylorParameters { n: 0.40, c: 1200.0 },
        }
    }

    /// Estimate tool wear and expected life for cutting this material at `surface_speed_m_min` for `cutting_time_minutes`.
    pub fn estimate_tool_wear(
        self,
        surface_speed_m_min: f64,
        cutting_time_minutes: f64,
    ) -> ToolWearEstimate {
        let params = self.taylor_parameters();
        // V_c * T^n = C => T = (C / V_c)^(1 / n)
        let tool_life_minutes = if surface_speed_m_min > 0.0 && params.n > 0.0 && params.c > 0.0 {
            (params.c / surface_speed_m_min)
                .powf(1.0 / params.n)
                .max(1.0e-3)
        } else {
            f64::INFINITY
        };
        let cutting_time = cutting_time_minutes.max(0.0);
        let wear_fraction = if tool_life_minutes.is_finite() && tool_life_minutes > 0.0 {
            (cutting_time / tool_life_minutes).max(0.0)
        } else {
            0.0
        };
        let wear_percent = (wear_fraction * 100.0).clamp(0.0, 1000.0);
        let remaining_life_minutes = if tool_life_minutes.is_finite() {
            (tool_life_minutes - cutting_time).max(0.0)
        } else {
            f64::INFINITY
        };
        ToolWearEstimate {
            surface_speed_m_min,
            cutting_time_minutes: cutting_time,
            tool_life_minutes,
            wear_fraction,
            wear_percent,
            remaining_life_minutes,
        }
    }
}

/// Taylor tool life equation constants ($V_c \cdot T^n = C$).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaylorParameters {
    /// Taylor tool life exponent $n$.
    pub n: f64,
    /// Cutting speed constant $C$ (m/min) for 1-minute tool life.
    pub c: f64,
}

/// Estimate of tool wear and life based on Taylor's tool life equation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolWearEstimate {
    /// Surface cutting speed in metres per minute.
    pub surface_speed_m_min: f64,
    /// Active cutting time in minutes.
    pub cutting_time_minutes: f64,
    /// Theoretical tool life in minutes from Taylor's equation.
    pub tool_life_minutes: f64,
    /// Tool wear fraction (cutting_time / tool_life). 1.0 = 100% worn.
    pub wear_fraction: f64,
    /// Tool wear as a percentage (wear_fraction * 100%).
    pub wear_percent: f64,
    /// Estimated remaining useful life in minutes.
    pub remaining_life_minutes: f64,
}

impl fmt::Display for Material {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// What the calculator needs: the material, the tool and the machine's limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedsRequest {
    /// The workpiece material.
    pub material: Material,
    /// Cutting diameter in millimetres.
    pub diameter_mm: f64,
    /// Number of cutting teeth.
    pub flutes: u32,
    /// The machine's maximum spindle speed in rpm.
    pub max_spindle_rpm: f64,
    /// The machine's maximum feed rate in millimetres per minute.
    pub max_feed_mm_min: f64,
}

/// The recommended cutting conditions, after the machine's limits are applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecommendedFeeds {
    /// Spindle speed in revolutions per minute.
    pub spindle_rpm: f64,
    /// Cutting feed in millimetres per minute.
    pub feed_mm_min: f64,
    /// Plunge feed in millimetres per minute (40 % of the cutting feed).
    pub plunge_feed_mm_min: f64,
    /// The surface speed actually achieved after clamping, in metres per minute.
    pub surface_speed_m_min: f64,
    /// The chip load actually achieved after clamping, in millimetres per tooth.
    pub chip_load_mm_per_tooth: f64,
    /// The chip load the material table asked for, before clamping.
    pub target_chip_load_mm_per_tooth: f64,
    /// The spindle limit lowered the speed the material asked for.
    pub spindle_limited: bool,
    /// The feed limit lowered the feed the spindle and chip load asked for.
    pub feed_limited: bool,
}

/// Recommend a spindle speed and feed, clamped to the machine and reporting every clamp.
pub fn recommend(request: &FeedsRequest) -> Result<RecommendedFeeds, FeedsError> {
    let diameter = request.diameter_mm;
    if !diameter.is_finite() || diameter <= 0.0 || diameter > 50.0 {
        return Err(FeedsError::InvalidDiameter(diameter));
    }
    if request.flutes == 0 || request.flutes > 12 {
        return Err(FeedsError::InvalidFlutes(request.flutes));
    }
    for (name, value) in [
        ("maximum spindle speed", request.max_spindle_rpm),
        ("maximum feed rate", request.max_feed_mm_min),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(FeedsError::InvalidLimit(format!(
                "the {name} must be finite and positive, got {value}"
            )));
        }
    }

    let data = request.material.cutting_data();
    let target_rpm = 1000.0 * data.surface_speed_m_min / (PI * diameter);
    let spindle_limited = target_rpm > request.max_spindle_rpm;
    let spindle_rpm = target_rpm.min(request.max_spindle_rpm).floor().max(1.0);

    let target_chip_load = (data.chip_load_mm_per_tooth * diameter / 6.0).clamp(0.002, 0.150);
    let target_feed = spindle_rpm * f64::from(request.flutes) * target_chip_load;
    let feed_limited = target_feed > request.max_feed_mm_min;
    let feed_mm_min = target_feed.min(request.max_feed_mm_min).floor().max(1.0);
    let chip_load = feed_mm_min / (spindle_rpm * f64::from(request.flutes));

    Ok(RecommendedFeeds {
        spindle_rpm,
        feed_mm_min,
        plunge_feed_mm_min: (feed_mm_min * 0.4).floor().max(1.0),
        surface_speed_m_min: PI * diameter * spindle_rpm / 1000.0,
        chip_load_mm_per_tooth: chip_load,
        target_chip_load_mm_per_tooth: target_chip_load,
        spindle_limited,
        feed_limited,
    })
}

/// Why a recommendation could not be made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FeedsError {
    #[error("the cutting diameter must be finite, positive and at most 50 mm, got {0} mm")]
    InvalidDiameter(f64),
    #[error("the flute count must be between 1 and 12, got {0}")]
    InvalidFlutes(u32),
    #[error("invalid machine limit: {0}")]
    InvalidLimit(String),
}
