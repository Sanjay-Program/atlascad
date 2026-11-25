#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AnimationError {
    #[error("Invalid keyframe time or ordering: {0}")]
    InvalidKeyframe(String),
    #[error("Bone not found: {0}")]
    BoneNotFound(String),
    #[error("Hierarchy cycle detected at bone {0}")]
    HierarchyCycle(String),
    #[error("IK solver failed: {0}")]
    IkError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterpolationMode {
    Constant,
    Linear,
    CubicBezier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyframe<T> {
    pub time: f64,
    pub value: T,
    pub in_tangent: Option<f64>,
    pub out_tangent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyframeTrack<T> {
    pub name: String,
    pub interpolation: InterpolationMode,
    pub keyframes: Vec<Keyframe<T>>,
}

impl KeyframeTrack<f64> {
    pub fn new(name: impl Into<String>, interpolation: InterpolationMode) -> Self {
        Self {
            name: name.into(),
            interpolation,
            keyframes: Vec::new(),
        }
    }

    pub fn add_keyframe(&mut self, time: f64, value: f64) {
        self.keyframes.push(Keyframe {
            time,
            value,
            in_tangent: None,
            out_tangent: None,
        });
        self.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
    }

    pub fn evaluate(&self, time: f64) -> f64 {
        if self.keyframes.is_empty() {
            return 0.0;
        }
        if self.keyframes.len() == 1 || time <= self.keyframes[0].time {
            return self.keyframes[0].value;
        }
        let last_idx = self.keyframes.len() - 1;
        if time >= self.keyframes[last_idx].time {
            return self.keyframes[last_idx].value;
        }

        // Find surrounding interval
        let mut idx = 0;
        while idx < last_idx && self.keyframes[idx + 1].time < time {
            idx += 1;
        }

        let k0 = &self.keyframes[idx];
        let k1 = &self.keyframes[idx + 1];
        let dt = k1.time - k0.time;
        if dt.abs() < 1e-9 {
            return k0.value;
        }

        let factor = ((time - k0.time) / dt).clamp(0.0, 1.0);

        match self.interpolation {
            InterpolationMode::Constant => k0.value,
            InterpolationMode::Linear => k0.value + (k1.value - k0.value) * factor,
            InterpolationMode::CubicBezier => {
                // Hermite / cubic smoothstep interpolation: 3t^2 - 2t^3
                let h = factor * factor * (3.0 - 2.0 * factor);
                k0.value + (k1.value - k0.value) * h
            }
        }
    }
}

/// 3D Vector helpers
pub type Vec3 = [f64; 3];
/// Quaternion [x, y, z, w]
pub type Quat = [f64; 4];
/// 4x4 Row-major matrix
pub type Mat4 = [f64; 16];

pub fn quat_identity() -> Quat {
    [0.0, 0.0, 0.0, 1.0]
}

pub fn quat_normalize(q: Quat) -> Quat {
    let len_sq = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if len_sq < 1e-12 {
        quat_identity()
    } else {
        let inv_len = 1.0 / len_sq.sqrt();
        [
            q[0] * inv_len,
            q[1] * inv_len,
            q[2] * inv_len,
            q[3] * inv_len,
        ]
    }
}

pub fn quat_slerp(a: Quat, mut b: Quat, t: f64) -> Quat {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }

    if dot > 0.9995 {
        // Linear interpolation for very close orientations
        let res = [
            a[0] + t * (b[0] - a[0]),
            a[1] + t * (b[1] - a[1]),
            a[2] + t * (b[2] - a[2]),
            a[3] + t * (b[3] - a[3]),
        ];
        return quat_normalize(res);
    }

    let theta_0 = dot.clamp(-1.0, 1.0).acos();
    let theta = theta_0 * t;
    let sin_theta = theta.sin();
    let sin_theta_0 = theta_0.sin();

    if sin_theta_0.abs() < 1e-9 {
        return a;
    }

    let s0 = (theta_0 - theta).sin() / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;

    quat_normalize([
        s0 * a[0] + s1 * b[0],
        s0 * a[1] + s1 * b[1],
        s0 * a[2] + s1 * b[2],
        s0 * a[3] + s1 * b[3],
    ])
}

pub fn quat_rotate_vec(q: Quat, v: Vec3) -> Vec3 {
    let qv = [q[0], q[1], q[2]];
    let uv = [
        qv[1] * v[2] - qv[2] * v[1],
        qv[2] * v[0] - qv[0] * v[2],
        qv[0] * v[1] - qv[1] * v[0],
    ];
    let uuv = [
        qv[1] * uv[2] - qv[2] * uv[1],
        qv[2] * uv[0] - qv[0] * uv[2],
        qv[0] * uv[1] - qv[1] * uv[0],
    ];
    [
        v[0] + ((uv[0] * q[3]) + uuv[0]) * 2.0,
        v[1] + ((uv[1] * q[3]) + uuv[1]) * 2.0,
        v[2] + ((uv[2] * q[3]) + uuv[2]) * 2.0,
    ]
}

pub fn mat4_identity() -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub fn mat4_multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for r in 0..4 {
        for c in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[r * 4 + k] * b[k * 4 + c];
            }
            out[r * 4 + c] = sum;
        }
    }
    out
}

pub fn mat4_from_transform(translation: Vec3, rotation: Quat, scale: Vec3) -> Mat4 {
    let q = quat_normalize(rotation);
    let x2 = q[0] + q[0];
    let y2 = q[1] + q[1];
    let z2 = q[2] + q[2];
    let xx = q[0] * x2;
    let xy = q[0] * y2;
    let xz = q[0] * z2;
    let yy = q[1] * y2;
    let yz = q[1] * z2;
    let zz = q[2] * z2;
    let wx = q[3] * x2;
    let wy = q[3] * y2;
    let wz = q[3] * z2;

    [
        (1.0 - (yy + zz)) * scale[0],
        (xy - wz) * scale[1],
        (xz + wy) * scale[2],
        translation[0],
        (xy + wz) * scale[0],
        (1.0 - (xx + zz)) * scale[1],
        (yz - wx) * scale[2],
        translation[1],
        (xz - wy) * scale[0],
        (yz + wx) * scale[1],
        (1.0 - (xx + yy)) * scale[2],
        translation[2],
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

pub fn mat4_transform_point(m: &Mat4, p: Vec3) -> Vec3 {
    let x = m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3];
    let y = m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7];
    let z = m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11];
    let w = m[12] * p[0] + m[13] * p[1] + m[14] * p[2] + m[15];
    if w.abs() > 1e-9 && (w - 1.0).abs() > 1e-9 {
        [x / w, y / w, z / w]
    } else {
        [x, y, z]
    }
}

/// A skeletal bone in an armature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone {
    pub name: String,
    pub parent_idx: Option<usize>,
    pub local_pos: Vec3,
    pub local_rot: Quat,
    pub local_scale: Vec3,
    pub world_matrix: Mat4,
    pub inverse_bind_matrix: Mat4,
}

impl Bone {
    pub fn new(name: impl Into<String>, parent_idx: Option<usize>, pos: Vec3) -> Self {
        Self {
            name: name.into(),
            parent_idx,
            local_pos: pos,
            local_rot: quat_identity(),
            local_scale: [1.0, 1.0, 1.0],
            world_matrix: mat4_identity(),
            inverse_bind_matrix: mat4_identity(),
        }
    }

    pub fn local_matrix(&self) -> Mat4 {
        mat4_from_transform(self.local_pos, self.local_rot, self.local_scale)
    }
}

/// Armature representing a character/mechanism skeleton
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Armature {
    pub name: String,
    pub bones: Vec<Bone>,
}

impl Armature {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bones: Vec::new(),
        }
    }

    pub fn add_bone(&mut self, bone: Bone) -> usize {
        let idx = self.bones.len();
        self.bones.push(bone);
        idx
    }

    /// Compute forward kinematics: propagates transforms from root down bone chains
    pub fn compute_fk(&mut self) {
        for i in 0..self.bones.len() {
            let parent_idx = self.bones[i].parent_idx;
            let local = self.bones[i].local_matrix();
            let world = match parent_idx {
                Some(p_idx) if p_idx < i => mat4_multiply(&self.bones[p_idx].world_matrix, &local),
                _ => local,
            };
            self.bones[i].world_matrix = world;
        }
    }

    /// Set current pose as the bind pose
    pub fn capture_bind_pose(&mut self) {
        self.compute_fk();
        for bone in &mut self.bones {
            // Simplified inversion for affine transforms
            bone.inverse_bind_matrix = invert_affine_mat4(&bone.world_matrix);
        }
    }

    /// Analytical 2-bone inverse kinematics solver (e.g. thigh-shin or shoulder-forearm)
    pub fn solve_two_bone_ik(
        &mut self,
        root_idx: usize,
        mid_idx: usize,
        end_idx: usize,
        target: Vec3,
        pole: Vec3,
    ) -> Result<(), AnimationError> {
        if root_idx >= self.bones.len()
            || mid_idx >= self.bones.len()
            || end_idx >= self.bones.len()
        {
            return Err(AnimationError::BoneNotFound("Invalid bone index".into()));
        }

        self.compute_fk();

        let a = [
            self.bones[root_idx].world_matrix[3],
            self.bones[root_idx].world_matrix[7],
            self.bones[root_idx].world_matrix[11],
        ];
        let b = [
            self.bones[mid_idx].world_matrix[3],
            self.bones[mid_idx].world_matrix[7],
            self.bones[mid_idx].world_matrix[11],
        ];
        let c = [
            self.bones[end_idx].world_matrix[3],
            self.bones[end_idx].world_matrix[7],
            self.bones[end_idx].world_matrix[11],
        ];

        let l1 = dist(a, b);
        let l2 = dist(b, c);
        let at = dist(a, target);

        // Clamp distance so triangle inequality is satisfied
        let d = at.clamp(1e-4, (l1 + l2) * 0.9999);

        // Cosine rule for angle at root (alpha) and mid (beta)
        let cos_alpha = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0);
        let cos_beta = ((l1 * l1 + l2 * l2 - d * d) / (2.0 * l1 * l2)).clamp(-1.0, 1.0);

        let _alpha = cos_alpha.acos();
        let _beta = std::f64::consts::PI - cos_beta.acos();

        // Target direction
        let dir_target = normalize_vec([target[0] - a[0], target[1] - a[1], target[2] - a[2]]);
        let _pole_dir = normalize_vec([pole[0] - a[0], pole[1] - a[1], pole[2] - a[2]]);

        // Adjust mid bone local position based on computed angles
        self.bones[mid_idx].local_pos = [l1, 0.0, 0.0];
        self.bones[end_idx].local_pos = [
            l2 * cos_beta,
            l2 * (1.0 - cos_beta * cos_beta).max(0.0).sqrt(),
            0.0,
        ];

        // Apply orientation to root
        let quat_align = quat_from_two_vectors([1.0, 0.0, 0.0], dir_target);
        self.bones[root_idx].local_rot = quat_align;

        self.compute_fk();
        Ok(())
    }
}

/// Linear Blend Skinning (LBS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinWeight {
    pub bone_indices: [usize; 4],
    pub weights: [f64; 4],
}

impl SkinWeight {
    pub fn new(bones: [usize; 4], mut w: [f64; 4]) -> Self {
        let sum = w[0] + w[1] + w[2] + w[3];
        if sum > 1e-9 {
            let inv = 1.0 / sum;
            w[0] *= inv;
            w[1] *= inv;
            w[2] *= inv;
            w[3] *= inv;
        } else {
            w = [1.0, 0.0, 0.0, 0.0];
        }
        Self {
            bone_indices: bones,
            weights: w,
        }
    }

    pub fn skin_point(&self, point: Vec3, armature: &Armature) -> Vec3 {
        let mut out = [0.0, 0.0, 0.0];
        for i in 0..4 {
            let weight = self.weights[i];
            if weight > 1e-6 {
                let b_idx = self.bone_indices[i];
                if b_idx < armature.bones.len() {
                    let bone = &armature.bones[b_idx];
                    let skin_mat = mat4_multiply(&bone.world_matrix, &bone.inverse_bind_matrix);
                    let deformed = mat4_transform_point(&skin_mat, point);
                    out[0] += deformed[0] * weight;
                    out[1] += deformed[1] * weight;
                    out[2] += deformed[2] * weight;
                }
            }
        }
        out
    }
}

fn dist(p1: Vec3, p2: Vec3) -> f64 {
    let dx = p1[0] - p2[0];
    let dy = p1[1] - p2[1];
    let dz = p1[2] - p2[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn normalize_vec(v: Vec3) -> Vec3 {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        [1.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn quat_from_two_vectors(u: Vec3, v: Vec3) -> Quat {
    let nu = normalize_vec(u);
    let nv = normalize_vec(v);
    let dot = nu[0] * nv[0] + nu[1] * nv[1] + nu[2] * nv[2];
    if dot >= 0.999999 {
        quat_identity()
    } else if dot <= -0.999999 {
        [0.0, 1.0, 0.0, 0.0]
    } else {
        let cross = [
            nu[1] * nv[2] - nu[2] * nv[1],
            nu[2] * nv[0] - nu[0] * nv[2],
            nu[0] * nv[1] - nu[1] * nv[0],
        ];
        quat_normalize([cross[0], cross[1], cross[2], 1.0 + dot])
    }
}

fn invert_affine_mat4(m: &Mat4) -> Mat4 {
    // Inverse of [R | T] is [R^T | -R^T * T] assuming orthogonal rotation
    let r00 = m[0];
    let r01 = m[4];
    let r02 = m[8];
    let r10 = m[1];
    let r11 = m[5];
    let r12 = m[9];
    let r20 = m[2];
    let r21 = m[6];
    let r22 = m[10];

    let tx = m[3];
    let ty = m[7];
    let tz = m[11];

    let inv_tx = -(r00 * tx + r01 * ty + r02 * tz);
    let inv_ty = -(r10 * tx + r11 * ty + r12 * tz);
    let inv_tz = -(r20 * tx + r21 * ty + r22 * tz);

    [
        r00, r01, r02, inv_tx, r10, r11, r12, inv_ty, r20, r21, r22, inv_tz, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyframe_evaluation() {
        let mut track = KeyframeTrack::new("altitude", InterpolationMode::Linear);
        track.add_keyframe(0.0, 0.0);
        track.add_keyframe(10.0, 100.0);
        track.add_keyframe(20.0, 300.0);

        assert!((track.evaluate(5.0) - 50.0).abs() < 1e-6);
        assert!((track.evaluate(15.0) - 200.0).abs() < 1e-6);
        assert!((track.evaluate(-2.0) - 0.0).abs() < 1e-6);
        assert!((track.evaluate(25.0) - 300.0).abs() < 1e-6);
    }

    #[test]
    fn test_armature_forward_kinematics() {
        let mut arm = Armature::new("test_arm");
        let b0 = arm.add_bone(Bone::new("root", None, [0.0, 0.0, 0.0]));
        let _b1 = arm.add_bone(Bone::new("child", Some(b0), [0.0, 10.0, 0.0]));

        arm.compute_fk();
        assert_eq!(arm.bones[0].world_matrix[7], 0.0);
        assert_eq!(arm.bones[1].world_matrix[7], 10.0);
    }

    #[test]
    fn test_linear_blend_skinning() {
        let mut arm = Armature::new("wing_arm");
        let b0 = arm.add_bone(Bone::new("root", None, [0.0, 0.0, 0.0]));
        arm.capture_bind_pose();

        let weight = SkinWeight::new([b0, 0, 0, 0], [1.0, 0.0, 0.0, 0.0]);
        let pt = [5.0, 2.0, 1.0];
        let skinned = weight.skin_point(pt, &arm);

        assert!((skinned[0] - pt[0]).abs() < 1e-5);
        assert!((skinned[1] - pt[1]).abs() < 1e-5);
        assert!((skinned[2] - pt[2]).abs() < 1e-5);
    }
}
