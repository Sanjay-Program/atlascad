use crate::mesh::PolyMesh;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SculptBrushType {
    Draw,
    Clay,
    Smooth,
    Flatten,
    Inflate,
    Pinch,
    Crease,
    Grab,
    Elastic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FalloffCurve {
    Smooth,
    Sphere,
    Linear,
    Constant,
}

impl FalloffCurve {
    pub fn evaluate(&self, normalized_dist: f64) -> f64 {
        let d = normalized_dist.clamp(0.0, 1.0);
        match self {
            Self::Smooth => {
                let x = 1.0 - d;
                x * x * (3.0 - 2.0 * x)
            }
            Self::Sphere => (1.0 - d * d).max(0.0).sqrt(),
            Self::Linear => 1.0 - d,
            Self::Constant => {
                if d <= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SculptBrush {
    pub brush_type: SculptBrushType,
    pub radius: f64,
    pub strength: f64,
    pub falloff: FalloffCurve,
    pub invert: bool,
}

impl Default for SculptBrush {
    fn default() -> Self {
        Self {
            brush_type: SculptBrushType::Draw,
            radius: 1.0,
            strength: 0.5,
            falloff: FalloffCurve::Smooth,
            invert: false,
        }
    }
}

impl SculptBrush {
    pub fn new(
        brush_type: SculptBrushType,
        radius: f64,
        strength: f64,
        falloff: FalloffCurve,
    ) -> Self {
        Self {
            brush_type,
            radius,
            strength,
            falloff,
            invert: false,
        }
    }

    pub fn apply(
        &self,
        mesh: &mut PolyMesh,
        stroke_center: [f64; 3],
        stroke_normal: [f64; 3],
        motion_vector: [f64; 3],
    ) -> usize {
        let radius_sq = self.radius * self.radius;
        if radius_sq <= 1e-12 {
            return 0;
        }

        let sign = if self.invert { -1.0 } else { 1.0 };
        let mut affected = 0;

        match self.brush_type {
            SculptBrushType::Draw | SculptBrushType::Clay => {
                for (pos, norm) in mesh.positions.iter_mut().zip(&mesh.normals) {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength * sign * 0.1;
                        let n = if self.brush_type == SculptBrushType::Clay {
                            stroke_normal
                        } else {
                            *norm
                        };
                        pos[0] += n[0] * weight;
                        pos[1] += n[1] * weight;
                        pos[2] += n[2] * weight;
                        affected += 1;
                    }
                }
            }
            SculptBrushType::Inflate => {
                for (pos, norm) in mesh.positions.iter_mut().zip(&mesh.normals) {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength * sign * 0.1;
                        pos[0] += norm[0] * weight;
                        pos[1] += norm[1] * weight;
                        pos[2] += norm[2] * weight;
                        affected += 1;
                    }
                }
            }
            SculptBrushType::Smooth => {
                let orig_positions = mesh.positions.clone();
                for (idx, pos) in mesh.positions.iter_mut().enumerate() {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength;
                        let mut avg = [0.0, 0.0, 0.0];
                        let mut count = 0.0;
                        for face in &mesh.faces {
                            if face.contains(&(idx as u32)) {
                                for &v in face {
                                    if v != idx as u32
                                        && let Some(other) = orig_positions.get(v as usize)
                                    {
                                        avg[0] += other[0];
                                        avg[1] += other[1];
                                        avg[2] += other[2];
                                        count += 1.0;
                                    }
                                }
                            }
                        }
                        if count > 0.0 {
                            let target = [avg[0] / count, avg[1] / count, avg[2] / count];
                            pos[0] += (target[0] - pos[0]) * weight * 0.5;
                            pos[1] += (target[1] - pos[1]) * weight * 0.5;
                            pos[2] += (target[2] - pos[2]) * weight * 0.5;
                            affected += 1;
                        }
                    }
                }
            }
            SculptBrushType::Grab | SculptBrushType::Elastic => {
                for pos in &mut mesh.positions {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength;
                        pos[0] += motion_vector[0] * weight;
                        pos[1] += motion_vector[1] * weight;
                        pos[2] += motion_vector[2] * weight;
                        affected += 1;
                    }
                }
            }
            SculptBrushType::Pinch => {
                for pos in &mut mesh.positions {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength * sign * 0.1;
                        pos[0] -= dx * weight;
                        pos[1] -= dy * weight;
                        pos[2] -= dz * weight;
                        affected += 1;
                    }
                }
            }
            SculptBrushType::Flatten => {
                for pos in &mut mesh.positions {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength;
                        let dist_to_plane =
                            dx * stroke_normal[0] + dy * stroke_normal[1] + dz * stroke_normal[2];
                        pos[0] -= stroke_normal[0] * dist_to_plane * weight;
                        pos[1] -= stroke_normal[1] * dist_to_plane * weight;
                        pos[2] -= stroke_normal[2] * dist_to_plane * weight;
                        affected += 1;
                    }
                }
            }
            SculptBrushType::Crease => {
                for (pos, norm) in mesh.positions.iter_mut().zip(&mesh.normals) {
                    let dx = pos[0] - stroke_center[0];
                    let dy = pos[1] - stroke_center[1];
                    let dz = pos[2] - stroke_center[2];
                    let dist_sq = dx * dx + dy * dy + dz * dz;
                    if dist_sq <= radius_sq {
                        let d = dist_sq.sqrt() / self.radius;
                        let weight = self.falloff.evaluate(d) * self.strength * sign * 0.1;
                        pos[0] -= norm[0] * weight;
                        pos[1] -= norm[1] * weight;
                        pos[2] -= norm[2] * weight;
                        pos[0] -= dx * weight * 0.5;
                        pos[1] -= dy * weight * 0.5;
                        pos[2] -= dz * weight * 0.5;
                        affected += 1;
                    }
                }
            }
        }

        if affected > 0 {
            mesh.recompute_normals();
        }
        affected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sculpt_draw_brush_modifies_vertices() {
        let mut cube = PolyMesh::cube(2.0);
        let brush = SculptBrush {
            brush_type: SculptBrushType::Draw,
            radius: 1.5,
            strength: 0.8,
            falloff: FalloffCurve::Smooth,
            invert: false,
        };
        let affected = brush.apply(&mut cube, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);
        assert!(affected > 0, "brush should deform top vertices of cube");
    }
}
