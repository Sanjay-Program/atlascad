#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use atlas_animation_core::{
    mat4_from_transform, mat4_identity, mat4_multiply, mat4_transform_point, quat_identity, Mat4,
    Quat, Vec3,
};

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Parent node not found: {0}")]
    ParentNotFound(String),
    #[error("Cycle detected inserting node {0}")]
    CycleDetected(String),
}

/// Axis-Aligned Bounding Box
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn empty() -> Self {
        Self {
            min: [f64::INFINITY, f64::INFINITY, f64::INFINITY],
            max: [f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min[0] > self.max[0] || self.min[1] > self.max[1] || self.min[2] > self.max[2]
    }

    pub fn include_point(&mut self, p: Vec3) {
        self.min[0] = self.min[0].min(p[0]);
        self.min[1] = self.min[1].min(p[1]);
        self.min[2] = self.min[2].min(p[2]);

        self.max[0] = self.max[0].max(p[0]);
        self.max[1] = self.max[1].max(p[1]);
        self.max[2] = self.max[2].max(p[2]);
    }

    pub fn merge(&self, other: &Aabb) -> Aabb {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Aabb {
            min: [
                self.min[0].min(other.min[0]),
                self.min[1].min(other.min[1]),
                self.min[2].min(other.min[2]),
            ],
            max: [
                self.max[0].max(other.max[0]),
                self.max[1].max(other.max[1]),
                self.max[2].max(other.max[2]),
            ],
        }
    }

    pub fn center(&self) -> Vec3 {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    pub fn extents(&self) -> Vec3 {
        [
            (self.max[0] - self.min[0]) * 0.5,
            (self.max[1] - self.min[1]) * 0.5,
            (self.max[2] - self.min[2]) * 0.5,
        ]
    }

    pub fn ray_intersects(&self, origin: Vec3, dir: Vec3, t_min: f64, t_max: f64) -> bool {
        let mut t0 = t_min;
        let mut t1 = t_max;

        for i in 0..3 {
            let inv_d = 1.0 / dir[i];
            let mut t_near = (self.min[i] - origin[i]) * inv_d;
            let mut t_far = (self.max[i] - origin[i]) * inv_d;

            if inv_d < 0.0 {
                std::mem::swap(&mut t_near, &mut t_far);
            }

            t0 = t0.max(t_near);
            t1 = t1.min(t_far);

            if t1 <= t0 {
                return false;
            }
        }
        true
    }
}

/// Transform representing 3D spatial pose
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: quat_identity(),
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Transform {
    pub fn new(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            position,
            rotation,
            scale,
        }
    }

    pub fn from_translation(position: Vec3) -> Self {
        Self {
            position,
            rotation: quat_identity(),
            scale: [1.0, 1.0, 1.0],
        }
    }

    pub fn to_matrix(&self) -> Mat4 {
        mat4_from_transform(self.position, self.rotation, self.scale)
    }
}

/// Mesh component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshComponent {
    pub mesh_id: String,
    pub material_id: Option<String>,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
    pub bounds: Aabb,
}

/// Light type
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LightType {
    Sun,
    Point,
    Spot { inner_angle: f64, outer_angle: f64 },
    Area { width: f64, height: f64 },
}

/// Light component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightComponent {
    pub light_type: LightType,
    pub color: [f64; 3],
    pub intensity: f64, // Lumens or Lux
    pub range: f64,
}

/// Camera projection
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProjectionType {
    Perspective { fov_y_rad: f64 },
    Orthographic { ortho_size: f64 },
}

/// Camera component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraComponent {
    pub projection: ProjectionType,
    pub near_clip: f64,
    pub far_clip: f64,
    pub aspect_ratio: f64,
}

/// Physics body type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsBodyType {
    Static,
    Dynamic,
    Kinematic,
}

/// Physics body component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicsBodyComponent {
    pub body_type: PhysicsBodyType,
    pub mass: f64,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub friction: f64,
    pub restitution: f64,
}

/// Live digital twin telemetry component
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryComponent {
    pub channels: HashMap<String, f64>,
    pub last_updated_sec: f64,
}

/// Generic node in the scene graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneNode {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub local_transform: Transform,
    pub world_matrix: Mat4,
    pub mesh: Option<MeshComponent>,
    pub light: Option<LightComponent>,
    pub camera: Option<CameraComponent>,
    pub physics: Option<PhysicsBodyComponent>,
    pub telemetry: Option<TelemetryComponent>,
}

impl SceneNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parent_id: None,
            children_ids: Vec::new(),
            local_transform: Transform::default(),
            world_matrix: mat4_identity(),
            mesh: None,
            light: None,
            camera: None,
            physics: None,
            telemetry: None,
        }
    }
}

/// Complete scene graph container
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub root_nodes: Vec<String>,
    pub nodes: HashMap<String, SceneNode>,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            root_nodes: Vec::new(),
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, mut node: SceneNode, parent_id: Option<&str>) -> Result<String, SceneError> {
        let id = node.id.clone();
        if let Some(p_id) = parent_id {
            if let Some(parent) = self.nodes.get_mut(p_id) {
                node.parent_id = Some(p_id.to_string());
                parent.children_ids.push(id.clone());
            } else {
                return Err(SceneError::ParentNotFound(p_id.to_string()));
            }
        } else {
            self.root_nodes.push(id.clone());
        }

        self.nodes.insert(id.clone(), node);
        self.update_transforms();
        Ok(id)
    }

    pub fn get_node(&self, id: &str) -> Option<&SceneNode> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut SceneNode> {
        self.nodes.get_mut(id)
    }

    /// Recursively update world transformation matrices from roots downward
    pub fn update_transforms(&mut self) {
        let root_ids = self.root_nodes.clone();
        for root_id in root_ids {
            self.update_subgraph_transform(&root_id, &mat4_identity());
        }
    }

    fn update_subgraph_transform(&mut self, node_id: &str, parent_world: &Mat4) {
        let (world_mat, children) = {
            if let Some(node) = self.nodes.get_mut(node_id) {
                let local = node.local_transform.to_matrix();
                node.world_matrix = mat4_multiply(parent_world, &local);
                (node.world_matrix, node.children_ids.clone())
            } else {
                return;
            }
        };

        for child_id in children {
            self.update_subgraph_transform(&child_id, &world_mat);
        }
    }

    /// Compute composite AABB for a node and all of its descendants
    pub fn compute_world_bounds(&self, node_id: &str) -> Aabb {
        let mut bounds = Aabb::empty();
        if let Some(node) = self.nodes.get(node_id) {
            if let Some(mesh) = node.mesh.as_ref().filter(|m| !m.bounds.is_empty()) {
                // Transform 8 corners by world matrix
                let corners = [
                        [mesh.bounds.min[0], mesh.bounds.min[1], mesh.bounds.min[2]],
                        [mesh.bounds.max[0], mesh.bounds.min[1], mesh.bounds.min[2]],
                        [mesh.bounds.min[0], mesh.bounds.max[1], mesh.bounds.min[2]],
                        [mesh.bounds.max[0], mesh.bounds.max[1], mesh.bounds.min[2]],
                        [mesh.bounds.min[0], mesh.bounds.min[1], mesh.bounds.max[2]],
                        [mesh.bounds.max[0], mesh.bounds.min[1], mesh.bounds.max[2]],
                        [mesh.bounds.min[0], mesh.bounds.max[1], mesh.bounds.max[2]],
                        [mesh.bounds.max[0], mesh.bounds.max[1], mesh.bounds.max[2]],
                    ];
                    for c in corners {
                        let tc = mat4_transform_point(&node.world_matrix, c);
                        bounds.include_point(tc);
                    }
            }
            for child_id in &node.children_ids {
                let cb = self.compute_world_bounds(child_id);
                bounds = bounds.merge(&cb);
            }
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb_merging_and_ray() {
        let mut b1 = Aabb::empty();
        b1.include_point([0.0, 0.0, 0.0]);
        b1.include_point([2.0, 2.0, 2.0]);

        assert_eq!(b1.center(), [1.0, 1.0, 1.0]);

        let ray_orig = [-1.0, 1.0, 1.0];
        let ray_dir = [1.0, 0.0, 0.0];
        assert!(b1.ray_intersects(ray_orig, ray_dir, 0.0, 10.0));

        let miss_dir = [0.0, 1.0, 0.0];
        assert!(!b1.ray_intersects(ray_orig, miss_dir, 0.0, 10.0));
    }

    #[test]
    fn test_scene_hierarchy_transform() {
        let mut scene = Scene::new("TestWorld");
        let mut root = SceneNode::new("root", "RootNode");
        root.local_transform.position = [0.0, 10.0, 0.0];
        let root_res = scene.add_node(root, None);
        assert!(root_res.is_ok());

        let mut child = SceneNode::new("child", "ChildNode");
        child.local_transform.position = [5.0, 0.0, 0.0];
        let child_res = scene.add_node(child, Some("root"));
        assert!(child_res.is_ok());

        assert!(scene.get_node("child").is_some());
        if let Some(child_node) = scene.get_node("child") {
            assert_eq!(child_node.world_matrix[3], 5.0);
            assert_eq!(child_node.world_matrix[7], 10.0);
        }
    }
}
