//! A height field sampled from a part mesh: what a 3-axis roughing pass can see.
//!
//! A vertical ray is cast down through every grid cell of the stock footprint and the highest
//! triangle hit becomes the cell's height. That is exactly the information a 3-axis Z-level
//! roughing strategy needs, and it is why undercuts are invisible to it: a ray sees the top
//! surface, never a cavity opening sideways.

use cad_contract::mesh::{Mesh, MeshError};

use crate::setup::Stock;

/// The height field of a part over the stock footprint, in work coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightField {
    min: [f64; 2],
    cell: [f64; 2],
    columns: usize,
    rows: usize,
    heights: Vec<f64>,
}

impl HeightField {
    /// Sample `mesh` over the stock footprint at roughly `resolution_mm`.
    ///
    /// The mesh must be valid (finite, in-range indices) and contain at least one non-vertical
    /// triangle. Vertical triangles are skipped: they project to a line in xy and a vertical ray
    /// cannot distinguish their height.
    pub fn from_mesh(mesh: &Mesh, stock: Stock, resolution_mm: f64) -> Result<Self, SurfaceError> {
        mesh.validate()?;
        if !resolution_mm.is_finite() || resolution_mm <= 0.0 {
            return Err(SurfaceError::InvalidResolution(resolution_mm));
        }
        let size = stock.size();
        let columns = ((size[0] / resolution_mm).ceil() as usize).max(1);
        let rows = ((size[1] / resolution_mm).ceil() as usize).max(1);
        let cell = [size[0] / columns as f64, size[1] / rows as f64];

        let mut facets = Vec::new();
        for triangle in &mesh.triangles {
            let (Some(a), Some(b), Some(c)) = (
                mesh.positions.get(triangle[0] as usize),
                mesh.positions.get(triangle[1] as usize),
                mesh.positions.get(triangle[2] as usize),
            ) else {
                continue;
            };
            let (a, b, c) = (*a, *b, *c);
            let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if determinant.abs() < 1.0e-12 {
                continue;
            }
            facets.push(Facet {
                a,
                b,
                c,
                determinant,
                min: [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])],
                max: [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])],
            });
        }
        if facets.is_empty() {
            return Err(SurfaceError::NoSurface);
        }

        let mut heights = vec![f64::NEG_INFINITY; columns * rows];
        for row in 0..rows {
            let y = stock.min[1] + (row as f64 + 0.5) * cell[1];
            for column in 0..columns {
                let x = stock.min[0] + (column as f64 + 0.5) * cell[0];
                let mut best = f64::NEG_INFINITY;
                for facet in &facets {
                    if x < facet.min[0] || x > facet.max[0] || y < facet.min[1] || y > facet.max[1]
                    {
                        continue;
                    }
                    if let Some(height) = facet.height_at(x, y)
                        && height > best
                    {
                        best = height;
                    }
                }
                heights[row * columns + column] = best;
            }
        }
        Ok(HeightField {
            min: [stock.min[0], stock.min[1]],
            cell,
            columns,
            rows,
            heights,
        })
    }

    /// The grid cell size in millimetres.
    pub fn cell_size(&self) -> [f64; 2] {
        self.cell
    }

    /// The number of columns (x) and rows (y).
    pub fn dims(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }

    /// The stock origin the field is sampled over.
    pub fn min(&self) -> [f64; 2] {
        self.min
    }

    /// The part height at `(x, y)`, or negative infinity where the part has no material.
    /// The lower xy corner of the sampling grid.
    pub fn origin(&self) -> [f64; 2] {
        self.min
    }

    /// The sampled heights in row-major order: row 0 is the lowest y.
    pub fn heights(&self) -> &[f64] {
        &self.heights
    }

    pub fn height_at(&self, x: f64, y: f64) -> f64 {
        let column = (((x - self.min[0]) / self.cell[0]).floor())
            .clamp(0.0, (self.columns - 1) as f64) as usize;
        let row = (((y - self.min[1]) / self.cell[1]).floor()).clamp(0.0, (self.rows - 1) as f64)
            as usize;
        self.heights[row * self.columns + column]
    }

    /// The lowest part height, if the part has any material in the stock footprint.
    pub fn lowest(&self) -> Option<f64> {
        self.heights
            .iter()
            .copied()
            .filter(|height| height.is_finite())
            .fold(None, |lowest: Option<f64>, height| {
                Some(lowest.map_or(height, |value| value.min(height)))
            })
    }

    /// The highest part height, if the part has any material in the stock footprint.
    pub fn highest(&self) -> Option<f64> {
        self.heights
            .iter()
            .copied()
            .filter(|height| height.is_finite())
            .fold(None, |highest: Option<f64>, height| {
                Some(highest.map_or(height, |value| value.max(height)))
            })
    }
}

/// One triangle, with the barycentric denominator precomputed.
#[derive(Debug, Clone, Copy)]
struct Facet {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    determinant: f64,
    min: [f64; 2],
    max: [f64; 2],
}

impl Facet {
    /// The plane height at an xy point inside the triangle.
    fn height_at(&self, x: f64, y: f64) -> Option<f64> {
        let determinant = self.determinant;
        let wa = ((self.b[1] - self.c[1]) * (x - self.c[0])
            + (self.c[0] - self.b[0]) * (y - self.c[1]))
            / determinant;
        let wb = ((self.c[1] - self.a[1]) * (x - self.c[0])
            + (self.a[0] - self.c[0]) * (y - self.c[1]))
            / determinant;
        let wc = 1.0 - wa - wb;
        if wa < -1.0e-9 || wb < -1.0e-9 || wc < -1.0e-9 {
            return None;
        }
        Some(wa * self.a[2] + wb * self.b[2] + wc * self.c[2])
    }
}

/// Why a height field could not be sampled.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SurfaceError {
    #[error("the sampling resolution must be finite and positive, got {0} mm")]
    InvalidResolution(f64),
    #[error("the part mesh has no non-vertical triangle to sample")]
    NoSurface,
    #[error("part mesh: {0}")]
    Mesh(#[from] MeshError),
}
