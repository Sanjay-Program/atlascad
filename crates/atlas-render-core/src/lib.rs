#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Invalid image dimensions: {0}x{1}")]
    InvalidDimensions(u32, u32),
    #[error("Render computation failed: {0}")]
    Computation(String),
}

pub type Vec3 = [f64; 3];

pub fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn normalize(v: Vec3) -> Vec3 {
    let len = dot(v, v).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn scale(v: Vec3, s: f64) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}

/// Ray in 3D space
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self {
            origin,
            direction: normalize(direction),
        }
    }

    pub fn at(&self, t: f64) -> Vec3 {
        add(self.origin, scale(self.direction, t))
    }
}

/// Ray-surface intersection result
#[derive(Debug, Clone)]
pub struct HitRecord {
    pub t: f64,
    pub point: Vec3,
    pub normal: Vec3,
    pub albedo: [f64; 3],
    pub metallic: f64,
    pub roughness: f64,
}

/// Sphere primitive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpherePrimitive {
    pub center: Vec3,
    pub radius: f64,
    pub albedo: [f64; 3],
    pub metallic: f64,
    pub roughness: f64,
}

impl SpherePrimitive {
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<HitRecord> {
        let oc = sub(ray.origin, self.center);
        let a = dot(ray.direction, ray.direction);
        let b = 2.0 * dot(oc, ray.direction);
        let c = dot(oc, oc) - self.radius * self.radius;
        let discriminant = b * b - 4.0 * a * c;

        if discriminant < 0.0 {
            return None;
        }

        let sqrtd = discriminant.sqrt();
        let mut root = (-b - sqrtd) / (2.0 * a);
        if root < t_min || root > t_max {
            root = (-b + sqrtd) / (2.0 * a);
            if root < t_min || root > t_max {
                return None;
            }
        }

        let point = ray.at(root);
        let normal = normalize(scale(sub(point, self.center), 1.0 / self.radius));

        Some(HitRecord {
            t: root,
            point,
            normal,
            albedo: self.albedo,
            metallic: self.metallic,
            roughness: self.roughness,
        })
    }
}

/// Triangle primitive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrianglePrimitive {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub albedo: [f64; 3],
}

impl TrianglePrimitive {
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<HitRecord> {
        let edge1 = sub(self.v1, self.v0);
        let edge2 = sub(self.v2, self.v0);
        let h = cross(ray.direction, edge2);
        let a = dot(edge1, h);

        if a.abs() < 1e-9 {
            return None;
        }

        let f = 1.0 / a;
        let s = sub(ray.origin, self.v0);
        let u = f * dot(s, h);

        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = cross(s, edge1);
        let v = f * dot(ray.direction, q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * dot(edge2, q);
        if t < t_min || t > t_max {
            return None;
        }

        let point = ray.at(t);
        let normal = normalize(cross(edge1, edge2));

        Some(HitRecord {
            t,
            point,
            normal,
            albedo: self.albedo,
            metallic: 0.0,
            roughness: 0.5,
        })
    }
}

/// Direct light source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectionalLight {
    pub direction: Vec3, // Towards the light
    pub color: [f64; 3],
    pub intensity: f64,
}

/// Perspective ray tracer camera
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerspectiveCamera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_deg: f64,
    pub aspect: f64,
}

impl PerspectiveCamera {
    pub fn new(position: Vec3, target: Vec3, up: Vec3, fov_deg: f64, aspect: f64) -> Self {
        Self {
            position,
            target,
            up,
            fov_deg,
            aspect,
        }
    }

    pub fn generate_ray(&self, u: f64, v: f64) -> Ray {
        let theta = self.fov_deg.to_radians();
        let h = (theta * 0.5).tan();
        let viewport_height = 2.0 * h;
        let viewport_width = self.aspect * viewport_height;

        let w = normalize(sub(self.position, self.target));
        let u_vec = normalize(cross(self.up, w));
        let v_vec = cross(w, u_vec);

        let horizontal = scale(u_vec, viewport_width);
        let vertical = scale(v_vec, viewport_height);

        let lower_left_corner = sub(
            sub(
                sub(self.position, scale(horizontal, 0.5)),
                scale(vertical, 0.5),
            ),
            w,
        );

        let screen_point = add(
            add(lower_left_corner, scale(horizontal, u)),
            scale(vertical, v),
        );
        let dir = sub(screen_point, self.position);
        Ray::new(self.position, dir)
    }
}

/// Simple render scene
#[derive(Debug, Clone, Default)]
pub struct RenderScene {
    pub spheres: Vec<SpherePrimitive>,
    pub triangles: Vec<TrianglePrimitive>,
    pub lights: Vec<DirectionalLight>,
    pub ambient_light: [f64; 3],
}

impl RenderScene {
    pub fn new() -> Self {
        Self {
            spheres: Vec::new(),
            triangles: Vec::new(),
            lights: Vec::new(),
            ambient_light: [0.05, 0.05, 0.08],
        }
    }

    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<HitRecord> {
        let mut closest_hit: Option<HitRecord> = None;
        let mut closest_so_far = t_max;

        for s in &self.spheres {
            if let Some(hit) = s.intersect(ray, t_min, closest_so_far) {
                closest_so_far = hit.t;
                closest_hit = Some(hit);
            }
        }

        for t in &self.triangles {
            if let Some(hit) = t.intersect(ray, t_min, closest_so_far) {
                closest_so_far = hit.t;
                closest_hit = Some(hit);
            }
        }

        closest_hit
    }

    pub fn trace_ray(&self, ray: &Ray, depth: u32) -> [f64; 3] {
        if depth == 0 {
            return [0.0, 0.0, 0.0];
        }

        if let Some(hit) = self.intersect(ray, 0.001, 1e9) {
            let mut final_color = [
                hit.albedo[0] * self.ambient_light[0],
                hit.albedo[1] * self.ambient_light[1],
                hit.albedo[2] * self.ambient_light[2],
            ];

            // Directional lights contribution with shadow rays
            for light in &self.lights {
                let light_dir = normalize(light.direction);
                let shadow_ray = Ray::new(hit.point, light_dir);
                let in_shadow = self.intersect(&shadow_ray, 0.001, 1e9).is_some();

                if !in_shadow {
                    let n_dot_l = dot(hit.normal, light_dir).max(0.0);
                    let diffuse = [
                        hit.albedo[0] * light.color[0] * n_dot_l * light.intensity,
                        hit.albedo[1] * light.color[1] * n_dot_l * light.intensity,
                        hit.albedo[2] * light.color[2] * n_dot_l * light.intensity,
                    ];
                    final_color = [
                        final_color[0] + diffuse[0],
                        final_color[1] + diffuse[1],
                        final_color[2] + diffuse[2],
                    ];
                }
            }

            final_color
        } else {
            // Sky gradient
            let unit_dir = normalize(ray.direction);
            let t = 0.5 * (unit_dir[1] + 1.0);
            [
                (1.0 - t) * 0.1 + t * 0.4,
                (1.0 - t) * 0.1 + t * 0.6,
                (1.0 - t) * 0.1 + t * 0.9,
            ]
        }
    }

    /// Render scene into an RGB image buffer (3 bytes per pixel)
    pub fn render_image(&self, camera: &PerspectiveCamera, width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidDimensions(width, height));
        }

        let mut buffer = vec![0u8; (width * height * 3) as usize];

        for y in 0..height {
            for x in 0..width {
                let u = (x as f64 + 0.5) / (width as f64);
                let v = ((height - 1 - y) as f64 + 0.5) / (height as f64);

                let ray = camera.generate_ray(u, v);
                let color = self.trace_ray(&ray, 3);

                // Gamma 2.2 tone mapping
                let r = (color[0].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                let g = (color[1].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;
                let b = (color[2].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8;

                let idx = ((y * width + x) * 3) as usize;
                buffer[idx] = r;
                buffer[idx + 1] = g;
                buffer[idx + 2] = b;
            }
        }

        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_intersection_and_render() {
        let mut scene = RenderScene::new();
        scene.spheres.push(SpherePrimitive {
            center: [0.0, 0.0, -5.0],
            radius: 1.0,
            albedo: [0.8, 0.2, 0.2],
            metallic: 0.1,
            roughness: 0.4,
        });

        scene.lights.push(DirectionalLight {
            direction: [1.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
        });

        let cam = PerspectiveCamera::new(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            60.0,
            1.0,
        );

        let img_res = scene.render_image(&cam, 16, 16);
        assert!(img_res.is_ok());
        if let Ok(buf) = img_res {
            assert_eq!(buf.len(), 16 * 16 * 3);
            // Center pixel hits the red sphere
            let center_idx = (8 * 16 + 8) * 3;
            assert!(buf[center_idx] > 50, "Red channel should be illuminated");
        }
    }
}
