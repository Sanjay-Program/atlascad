//! Tools, their geometry and cutting conditions, and the machine envelope they run in.
//!
//! A tool is described by what it physically is (a diameter, a flute length, a shape) and how it is
//! driven (spindle speed, feed, plunge feed, machine limits). Nothing here is a program: a tool
//! becomes motion only through [`crate::strategy::generate`].

use std::fmt;

/// The kinematic family of a machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Machine {
    /// A 3-axis mill, z axis up.
    Milling3Axis,
    /// A 4-axis mill, indexed.
    Milling4Axis,
    /// A 5-axis mill, simultaneous.
    Milling5Axis,
    /// A lathe.
    Turning,
}

impl Machine {
    /// The number of controlled axes this machine has.
    pub const fn axis_count(self) -> u32 {
        match self {
            Machine::Milling3Axis => 3,
            Machine::Milling4Axis => 4,
            Machine::Milling5Axis => 5,
            Machine::Turning => 2,
        }
    }

    /// Does this machine move a tool along z?
    pub const fn has_z(self) -> bool {
        matches!(
            self,
            Machine::Milling3Axis | Machine::Milling4Axis | Machine::Milling5Axis
        )
    }

    /// Is this a milling machine (as opposed to a lathe)?
    pub const fn is_milling(self) -> bool {
        self.has_z()
    }
}

impl fmt::Display for Machine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Machine::Milling3Axis => "3-axis milling",
            Machine::Milling4Axis => "4-axis indexing",
            Machine::Milling5Axis => "5-axis simultaneous",
            Machine::Turning => "turning",
        };
        formatter.write_str(name)
    }
}

/// A rectangular machine working volume in machine coordinates, in millimetres.
///
/// Every generated move is checked against it, so a program that leaves the envelope is refused
/// before a control ever sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MachineEnvelope {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl MachineEnvelope {
    /// A symmetric 500 mm cube around the origin, the default for the headless workflow.
    pub const fn default_mill() -> Self {
        MachineEnvelope {
            min: [-500.0; 3],
            max: [500.0; 3],
        }
    }

    /// Build an envelope, refusing a non-finite or inverted box.
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Result<Self, ToolError> {
        for axis in 0..3 {
            if !min[axis].is_finite() || !max[axis].is_finite() {
                return Err(ToolError::InvalidEnvelope(format!(
                    "envelope axis {axis} must be finite"
                )));
            }
            if max[axis] <= min[axis] {
                return Err(ToolError::InvalidEnvelope(format!(
                    "envelope axis {axis} must have max > min, got {} .. {}",
                    min[axis], max[axis]
                )));
            }
        }
        Ok(MachineEnvelope { min, max })
    }

    /// Is `point` inside the envelope, allowing `tolerance` millimetres of slack?
    pub fn contains(&self, point: [f64; 3], tolerance: f64) -> bool {
        (0..3).all(|axis| {
            point[axis] >= self.min[axis] - tolerance && point[axis] <= self.max[axis] + tolerance
        })
    }

    /// The side lengths of the working volume.
    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

impl Default for MachineEnvelope {
    fn default() -> Self {
        MachineEnvelope::default_mill()
    }
}

/// An opaque tool identifier. Zero is reserved and is never issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(u64);

impl ToolId {
    /// Issue an id, refusing the reserved zero.
    pub const fn new(raw: u64) -> Option<Self> {
        match raw {
            0 => None,
            value => Some(ToolId(value)),
        }
    }

    /// The raw number, for G-code `T` words and reporting.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "T{}", self.0)
    }
}

/// What a tool physically is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolGeometry {
    /// A flat end mill: a cylinder with a square end.
    FlatEndMill {
        /// Cutting diameter in millimetres.
        diameter_mm: f64,
        /// Usable flute length in millimetres.
        flute_length_mm: f64,
    },
    /// A ball nose cutter: a cylinder capped by a hemisphere.
    BallEndMill {
        /// Ball diameter in millimetres.
        diameter_mm: f64,
        /// Usable flute length in millimetres.
        flute_length_mm: f64,
    },
    /// A drill, identified by its point angle.
    Drill {
        /// Hole diameter in millimetres.
        diameter_mm: f64,
        /// Usable flute length in millimetres.
        flute_length_mm: f64,
        /// Included point angle in degrees (118° is a standard twist drill).
        point_angle_deg: f64,
    },
    /// A face mill: a wide, shallow disc.
    FaceMill {
        /// Cutting diameter in millimetres.
        diameter_mm: f64,
        /// Usable flute length in millimetres.
        flute_length_mm: f64,
    },
}

impl ToolGeometry {
    /// The cutting diameter in millimetres.
    pub const fn diameter_mm(&self) -> f64 {
        match self {
            ToolGeometry::FlatEndMill { diameter_mm, .. }
            | ToolGeometry::BallEndMill { diameter_mm, .. }
            | ToolGeometry::Drill { diameter_mm, .. }
            | ToolGeometry::FaceMill { diameter_mm, .. } => *diameter_mm,
        }
    }

    /// The usable flute length in millimetres.
    pub const fn flute_length_mm(&self) -> f64 {
        match self {
            ToolGeometry::FlatEndMill {
                flute_length_mm, ..
            }
            | ToolGeometry::BallEndMill {
                flute_length_mm, ..
            }
            | ToolGeometry::Drill {
                flute_length_mm, ..
            }
            | ToolGeometry::FaceMill {
                flute_length_mm, ..
            } => *flute_length_mm,
        }
    }

    /// A human-readable family name.
    pub const fn family(&self) -> &'static str {
        match self {
            ToolGeometry::FlatEndMill { .. } => "flat end mill",
            ToolGeometry::BallEndMill { .. } => "ball end mill",
            ToolGeometry::Drill { .. } => "drill",
            ToolGeometry::FaceMill { .. } => "face mill",
        }
    }
}

/// How a tool is driven: spindle speed and feed rates, held inside the machine's limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CuttingCondition {
    /// Spindle speed in revolutions per minute.
    pub spindle_rpm: f64,
    /// Cutting feed rate in millimetres per minute.
    pub feed_mm_min: f64,
    /// Plunge feed rate in millimetres per minute.
    pub plunge_feed_mm_min: f64,
    /// The machine's maximum spindle speed in rpm.
    pub max_spindle_rpm: f64,
    /// The machine's maximum feed rate in mm/min.
    pub max_feed_mm_min: f64,
}

impl CuttingCondition {
    /// A conservative aluminium/plastic condition for a small mill.
    pub const fn conservative_default() -> Self {
        CuttingCondition {
            spindle_rpm: 12_000.0,
            feed_mm_min: 600.0,
            plunge_feed_mm_min: 200.0,
            max_spindle_rpm: 24_000.0,
            max_feed_mm_min: 5_000.0,
        }
    }

    /// Build a condition, refusing values the machine cannot honour.
    pub fn new(
        spindle_rpm: f64,
        feed_mm_min: f64,
        plunge_feed_mm_min: f64,
        max_spindle_rpm: f64,
        max_feed_mm_min: f64,
    ) -> Result<Self, ToolError> {
        let condition = CuttingCondition {
            spindle_rpm,
            feed_mm_min,
            plunge_feed_mm_min,
            max_spindle_rpm,
            max_feed_mm_min,
        };
        condition.validate()?;
        Ok(condition)
    }

    /// Check every rate is finite, positive and inside the machine limits.
    pub fn validate(&self) -> Result<(), ToolError> {
        for (name, value) in [
            ("spindle speed", self.spindle_rpm),
            ("feed rate", self.feed_mm_min),
            ("plunge feed rate", self.plunge_feed_mm_min),
            ("maximum spindle speed", self.max_spindle_rpm),
            ("maximum feed rate", self.max_feed_mm_min),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(ToolError::InvalidCutting(format!(
                    "the {name} must be finite and positive, got {value}"
                )));
            }
        }
        if self.spindle_rpm > self.max_spindle_rpm {
            return Err(ToolError::InvalidCutting(format!(
                "spindle speed {} rpm exceeds the machine limit of {} rpm",
                self.spindle_rpm, self.max_spindle_rpm
            )));
        }
        if self.feed_mm_min > self.max_feed_mm_min {
            return Err(ToolError::InvalidCutting(format!(
                "feed rate {} mm/min exceeds the machine limit of {} mm/min",
                self.feed_mm_min, self.max_feed_mm_min
            )));
        }
        if self.plunge_feed_mm_min > self.feed_mm_min {
            return Err(ToolError::InvalidCutting(format!(
                "plunge feed {} mm/min exceeds the cutting feed {} mm/min",
                self.plunge_feed_mm_min, self.feed_mm_min
            )));
        }
        Ok(())
    }
}

impl Default for CuttingCondition {
    fn default() -> Self {
        CuttingCondition::conservative_default()
    }
}

/// A tool holder (chuck or collet) above the flutes, as it enters the machine envelope.
///
/// The holder is tested as a vertical column on the tool axis: if its lower face would be below
/// the material at the tool's xy position, the toolpath is refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Holder {
    /// Holder body diameter in millimetres.
    pub diameter_mm: f64,
    /// Holder length above the flutes in millimetres.
    pub length_mm: f64,
}

impl Holder {
    /// Build a holder, refusing impossible dimensions.
    pub fn new(diameter_mm: f64, length_mm: f64) -> Result<Self, ToolError> {
        if !diameter_mm.is_finite() || diameter_mm <= 0.0 {
            return Err(ToolError::InvalidHolder(format!(
                "the holder diameter must be finite and positive, got {diameter_mm} mm"
            )));
        }
        if !length_mm.is_finite() || length_mm <= 0.0 {
            return Err(ToolError::InvalidHolder(format!(
                "the holder length must be finite and positive, got {length_mm} mm"
            )));
        }
        Ok(Holder {
            diameter_mm,
            length_mm,
        })
    }
}

/// A tool as held in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    /// The id other records reference it by.
    pub id: ToolId,
    /// A shop-floor name, e.g. `6 mm flat end mill`.
    pub name: String,
    /// The cutting geometry.
    pub geometry: ToolGeometry,
    /// How it is driven.
    pub cutting: CuttingCondition,
    /// The deepest cut a single axial pass may take, in millimetres.
    ///
    /// Strategies split a deeper operation into passes no deeper than this.
    pub max_depth_of_cut_mm: f64,
    /// The holder above the flutes, when one is modelled.
    pub holder: Option<Holder>,
}

impl Tool {
    /// Build a tool, refusing impossible geometry or cutting conditions.
    pub fn new(
        id: ToolId,
        name: impl Into<String>,
        geometry: ToolGeometry,
        cutting: CuttingCondition,
        max_depth_of_cut_mm: f64,
    ) -> Result<Self, ToolError> {
        let tool = Tool {
            id,
            name: name.into(),
            geometry,
            cutting,
            max_depth_of_cut_mm,
            holder: None,
        };
        tool.validate()?;
        Ok(tool)
    }

    /// Check the invariants a generator relies on.
    pub fn validate(&self) -> Result<(), ToolError> {
        if !self.geometry.diameter_mm().is_finite() || self.geometry.diameter_mm() <= 0.0 {
            return Err(ToolError::InvalidGeometry(format!(
                "{} must have a positive diameter, got {} mm",
                self.geometry.family(),
                self.geometry.diameter_mm()
            )));
        }
        if !self.geometry.flute_length_mm().is_finite() || self.geometry.flute_length_mm() <= 0.0 {
            return Err(ToolError::InvalidGeometry(format!(
                "{} must have a positive flute length, got {} mm",
                self.geometry.family(),
                self.geometry.flute_length_mm()
            )));
        }
        if let ToolGeometry::Drill {
            point_angle_deg, ..
        } = self.geometry
            && !(point_angle_deg > 0.0 && point_angle_deg < 180.0)
        {
            return Err(ToolError::InvalidGeometry(format!(
                "a drill point angle must be between 0 and 180 degrees, got {point_angle_deg}"
            )));
        }
        if !self.max_depth_of_cut_mm.is_finite() || self.max_depth_of_cut_mm <= 0.0 {
            return Err(ToolError::InvalidGeometry(format!(
                "the maximum depth of cut must be finite and positive, got {} mm",
                self.max_depth_of_cut_mm
            )));
        }
        if self.name.trim().is_empty() {
            return Err(ToolError::InvalidGeometry(
                "a tool needs a name for the shop floor".into(),
            ));
        }
        if let Some(holder) = self.holder {
            if holder.diameter_mm < self.geometry.diameter_mm() {
                return Err(ToolError::InvalidHolder(format!(
                    "a {} mm holder cannot hold a {} mm tool",
                    holder.diameter_mm,
                    self.geometry.diameter_mm()
                )));
            }
            Holder::new(holder.diameter_mm, holder.length_mm)?;
        }
        self.cutting.validate()
    }

    /// The cutting diameter in millimetres.
    pub fn diameter_mm(&self) -> f64 {
        self.geometry.diameter_mm()
    }

    /// The cutting radius in millimetres.
    pub fn radius_mm(&self) -> f64 {
        self.diameter_mm() * 0.5
    }

    /// The usable flute length in millimetres.
    pub fn flute_length_mm(&self) -> f64 {
        self.geometry.flute_length_mm()
    }

    /// Attach a holder, refusing one narrower than the tool.
    pub fn with_holder(mut self, holder: Holder) -> Result<Self, ToolError> {
        self.holder = Some(holder);
        self.validate()?;
        Ok(self)
    }
}

/// Why a tool or a machine envelope was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ToolError {
    #[error("invalid tool geometry: {0}")]
    InvalidGeometry(String),
    #[error("invalid cutting condition: {0}")]
    InvalidCutting(String),
    #[error("invalid machine envelope: {0}")]
    InvalidEnvelope(String),
    #[error("invalid tool holder: {0}")]
    InvalidHolder(String),
    #[error("tool {0} is already in the library")]
    DuplicateTool(u64),
    #[error("tool {0} is not in the library")]
    UnknownTool(u64),
    #[error("a {tool} tool cannot run on a {machine} machine")]
    IncompatibleMachine { tool: Machine, machine: Machine },
}

/// A set of tools a job may use, keyed by id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolLibrary {
    tools: Vec<Tool>,
}

impl ToolLibrary {
    /// An empty library.
    pub fn new() -> Self {
        ToolLibrary { tools: Vec::new() }
    }

    /// Add a tool, refusing a duplicate id rather than silently replacing one.
    pub fn insert(&mut self, tool: Tool) -> Result<(), ToolError> {
        tool.validate()?;
        if self.get(tool.id).is_some() {
            return Err(ToolError::DuplicateTool(tool.id.get()));
        }
        self.tools.push(tool);
        Ok(())
    }

    /// Look up a tool by id.
    pub fn get(&self, id: ToolId) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.id == id)
    }

    /// Look up a tool by id, or report which id is missing.
    pub fn require(&self, id: ToolId) -> Result<&Tool, ToolError> {
        self.get(id).ok_or(ToolError::UnknownTool(id.get()))
    }

    /// How many tools the library holds.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Is the library empty?
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Iterate the tools in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Tool> {
        self.tools.iter()
    }
}
