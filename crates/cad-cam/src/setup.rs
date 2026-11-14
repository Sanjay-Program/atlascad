//! The machining setup: machine, envelope, stock and the work coordinate system.
//!
//! Stock and toolpath coordinates are given in the **work** frame (the part's own frame, origin at
//! the programmed zero). The setup maps them into **machine** coordinates by adding the WCS origin,
//! and that is the frame the machine envelope is checked in.

use cad_contract::mesh::{Mesh, MeshError};

use crate::tool::{Machine, MachineEnvelope, ToolError};

/// The work coordinate system of a setup, expressed in machine coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkCoordinate {
    /// Origin of the work coordinate system, in machine coordinates.
    pub origin: [f64; 3],
    /// Unit direction of the work z axis, in machine coordinates.
    pub z_axis: [f64; 3],
}

impl WorkCoordinate {
    /// The machine origin with +Z up: the default for a 3-axis mill.
    pub const fn standard() -> Self {
        WorkCoordinate {
            origin: [0.0; 3],
            z_axis: [0.0, 0.0, 1.0],
        }
    }

    /// Build a work coordinate system, normalising the z axis.
    pub fn new(origin: [f64; 3], z_axis: [f64; 3]) -> Result<Self, SetupError> {
        if !origin.iter().all(|value| value.is_finite()) {
            return Err(SetupError::Invalid("the WCS origin must be finite".into()));
        }
        let length = (z_axis[0] * z_axis[0] + z_axis[1] * z_axis[1] + z_axis[2] * z_axis[2]).sqrt();
        if !length.is_finite() || length < 1.0e-12 {
            return Err(SetupError::Invalid(
                "the WCS z axis must be non-zero".into(),
            ));
        }
        Ok(WorkCoordinate {
            origin,
            z_axis: [z_axis[0] / length, z_axis[1] / length, z_axis[2] / length],
        })
    }

    /// Map a work point into machine coordinates.
    ///
    /// Supported milling machine families (3-axis, 4-axis, 5-axis) map work coordinates
    /// to machine coordinates relative to the WCS origin.
    pub fn to_machine(&self, work: [f64; 3], machine: Machine) -> Result<[f64; 3], SetupError> {
        if !machine.is_milling() {
            return Err(SetupError::UnsupportedMachine(machine));
        }
        if self.z_axis[0].abs() > 1.0e-9
            || self.z_axis[1].abs() > 1.0e-9
            || (self.z_axis[2] - 1.0).abs() > 1.0e-9
        {
            return Err(SetupError::UnsupportedWorkCoordinate(format!(
                "z axis [{}, {}, {}] is not machine +Z",
                self.z_axis[0], self.z_axis[1], self.z_axis[2]
            )));
        }
        Ok([
            work[0] + self.origin[0],
            work[1] + self.origin[1],
            work[2] + self.origin[2],
        ])
    }

    /// Map a machine point back into the work frame.
    pub fn to_work(&self, machine_point: [f64; 3]) -> [f64; 3] {
        [
            machine_point[0] - self.origin[0],
            machine_point[1] - self.origin[1],
            machine_point[2] - self.origin[2],
        ]
    }
}

impl Default for WorkCoordinate {
    fn default() -> Self {
        WorkCoordinate::standard()
    }
}

/// An axis-aligned block of stock in work coordinates, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stock {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Stock {
    /// Build a stock block, refusing a non-finite or empty volume.
    pub fn new(min: [f64; 3], max: [f64; 3]) -> Result<Self, SetupError> {
        for axis in 0..3 {
            if !min[axis].is_finite() || !max[axis].is_finite() {
                return Err(SetupError::Invalid(format!(
                    "stock axis {axis} must be finite"
                )));
            }
            if max[axis] <= min[axis] {
                return Err(SetupError::Invalid(format!(
                    "stock axis {axis} must have max > min, got {} .. {}",
                    min[axis], max[axis]
                )));
            }
        }
        Ok(Stock { min, max })
    }

    /// Derive the stock block from a closed mesh's bounding box.
    ///
    /// The mesh is validated first, so a non-finite vertex fails here rather than producing a
    /// nonsensical block.
    pub fn from_mesh(mesh: &Mesh) -> Result<Self, SetupError> {
        mesh.validate()?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for position in &mesh.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        Stock::new(min, max)
    }

    /// The stock block as a mesh, for a caller that wants to display or measure it.
    pub fn to_mesh(&self) -> Mesh {
        let [x0, y0, z0] = self.min;
        let [x1, y1, z1] = self.max;
        let positions = vec![
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    /// The side lengths of the stock.
    pub fn size(&self) -> [f64; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    /// Is an xy point inside the stock footprint, grown by `margin`?
    ///
    /// A negative margin shrinks the footprint, which is how a strict "inside the body" test is
    /// expressed.
    pub fn contains_xy(&self, point: [f64; 2], margin: f64) -> bool {
        point[0] >= self.min[0] - margin
            && point[0] <= self.max[0] + margin
            && point[1] >= self.min[1] - margin
            && point[1] <= self.max[1] + margin
    }

    /// Translate the block by an offset.
    pub fn translated(&self, offset: [f64; 3]) -> Stock {
        Stock {
            min: [
                self.min[0] + offset[0],
                self.min[1] + offset[1],
                self.min[2] + offset[2],
            ],
            max: [
                self.max[0] + offset[0],
                self.max[1] + offset[1],
                self.max[2] + offset[2],
            ],
        }
    }
}

/// One machining setup: machine, envelope, stock, WCS and safe clearance plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Setup {
    /// The machine this program is for.
    pub machine: Machine,
    /// The machine working volume, in machine coordinates.
    pub envelope: MachineEnvelope,
    /// The stock block, in work coordinates.
    pub stock: Stock,
    /// The work coordinate system.
    pub wcs: WorkCoordinate,
    /// The safe z plane, in work coordinates, above the stock top.
    pub clearance_mm: f64,
}

impl Setup {
    /// Build a setup, checking the stock fits the machine and the clearance plane is above it.
    pub fn new(
        machine: Machine,
        envelope: MachineEnvelope,
        stock: Stock,
        wcs: WorkCoordinate,
        clearance_mm: f64,
    ) -> Result<Self, SetupError> {
        if !machine.is_milling() {
            return Err(SetupError::UnsupportedMachine(machine));
        }
        if !clearance_mm.is_finite() || clearance_mm <= stock.max[2] {
            return Err(SetupError::Invalid(format!(
                "the clearance plane ({clearance_mm} mm) must be above the stock top ({} mm)",
                stock.max[2]
            )));
        }
        for x in [stock.min[0], stock.max[0]] {
            for y in [stock.min[1], stock.max[1]] {
                for z in [stock.min[2], stock.max[2]] {
                    let machine_point = wcs.to_machine([x, y, z], machine)?;
                    if !envelope.contains(machine_point, 0.0) {
                        return Err(SetupError::StockOutsideEnvelope(format!(
                            "stock corner ({x}, {y}, {z}) maps to ({}, {}, {}) in machine coordinates, outside the envelope",
                            machine_point[0], machine_point[1], machine_point[2]
                        )));
                    }
                }
            }
        }
        Ok(Setup {
            machine,
            envelope,
            stock,
            wcs,
            clearance_mm,
        })
    }

    /// A default setup for a size-given block on a 3-axis mill with a 5 mm clearance plane.
    pub fn for_block(size: [f64; 3]) -> Result<Self, SetupError> {
        let stock = Stock::new([0.0; 3], size)?;
        Setup::new(
            Machine::Milling3Axis,
            MachineEnvelope::default_mill(),
            stock,
            WorkCoordinate::standard(),
            size[2] + 5.0,
        )
    }

    /// Map a work point to machine coordinates.
    pub fn machine_point(&self, work: [f64; 3]) -> Result<[f64; 3], SetupError> {
        self.wcs.to_machine(work, self.machine)
    }

    /// The stock block in machine coordinates.
    pub fn stock_in_machine(&self) -> Result<Stock, SetupError> {
        let min = self.machine_point(self.stock.min)?;
        let max = self.machine_point(self.stock.max)?;
        Stock::new(min, max)
    }
}

/// Why a setup was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SetupError {
    #[error("invalid setup: {0}")]
    Invalid(String),
    #[error("this crate drives z-up 3-axis mills only, not {0}")]
    UnsupportedMachine(Machine),
    #[error("unsupported work coordinate system: {0}")]
    UnsupportedWorkCoordinate(String),
    #[error("stock does not fit the machine: {0}")]
    StockOutsideEnvelope(String),
    #[error("stock mesh: {0}")]
    Mesh(#[from] MeshError),
    #[error("machine envelope: {0}")]
    Envelope(#[from] ToolError),
}
