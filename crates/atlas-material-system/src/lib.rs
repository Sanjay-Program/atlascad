//! PBR Material System and MaterialX shader node graph engine for AtlasCAD.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MaterialError {
    #[error("shader node error: {0}")]
    ShaderNode(String),
    #[error("materialx serialization error: {0}")]
    Serialization(String),
}

/// Physically-Based Rendering (PBR) metallic-roughness material definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PbrMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f64; 4],
    pub metallic: f64,
    pub roughness: f64,
    pub normal_scale: f64,
    pub occlusion_strength: f64,
    pub emissive_factor: [f64; 3],
    pub subsurface_scale: f64,
    pub subsurface_color: [f64; 3],
    pub transmission: f64,
    pub ior: f64,
    pub clearcoat: f64,
    pub clearcoat_roughness: f64,
    pub sheen_color: [f64; 3],
    pub textures: Vec<TextureBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureSemantic {
    BaseColor,
    Normal,
    MetallicRoughness,
    Occlusion,
    Emissive,
    HeightDisplacement,
    Subsurface,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureBinding {
    pub semantic: TextureSemantic,
    pub asset_id: String,
    pub uv_set: u32,
}

impl Default for PbrMaterial {
    fn default() -> Self {
        Self {
            id: "mat_default".into(),
            name: "Default_PBR".into(),
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            normal_scale: 1.0,
            occlusion_strength: 1.0,
            emissive_factor: [0.0, 0.0, 0.0],
            subsurface_scale: 0.0,
            subsurface_color: [1.0, 1.0, 1.0],
            transmission: 0.0,
            ior: 1.5,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            sheen_color: [0.0, 0.0, 0.0],
            textures: Vec::new(),
        }
    }
}

impl PbrMaterial {
    pub fn new(name: impl Into<String>) -> Self {
        let n = name.into();
        Self {
            id: format!("mat_{}", n.to_lowercase().replace(' ', "_")),
            name: n,
            ..Default::default()
        }
    }

    pub fn to_materialx_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<materialx version="1.38">
  <nodegraph name="NG_{name}">
    <standard_surface name="SRF_{name}" type="surfaceshader">
      <input name="base" type="float" value="1.0" />
      <input name="base_color" type="color3" value="{r:.4}, {g:.4}, {b:.4}" />
      <input name="metalness" type="float" value="{metallic:.4}" />
      <input name="specular_roughness" type="float" value="{roughness:.4}" />
      <input name="emission" type="float" value="1.0" />
      <input name="emission_color" type="color3" value="{er:.4}, {eg:.4}, {eb:.4}" />
      <input name="subsurface" type="float" value="{subsurface:.4}" />
      <input name="transmission" type="float" value="{transmission:.4}" />
      <input name="coat" type="float" value="{coat:.4}" />
      <input name="coat_roughness" type="float" value="{coat_roughness:.4}" />
    </standard_surface>
    <output name="surface" type="surfaceshader" nodename="SRF_{name}" />
  </nodegraph>
  <surfacematerial name="{name}" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="NG_{name}" output="surface" />
  </surfacematerial>
</materialx>"#,
            name = self.name,
            r = self.base_color[0],
            g = self.base_color[1],
            b = self.base_color[2],
            metallic = self.metallic,
            roughness = self.roughness,
            er = self.emissive_factor[0],
            eg = self.emissive_factor[1],
            eb = self.emissive_factor[2],
            subsurface = self.subsurface_scale,
            transmission = self.transmission,
            coat = self.clearcoat,
            coat_roughness = self.clearcoat_roughness
        )
    }
}

/// Node-based procedural shader graph (MaterialX / Blender Shader Node model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShaderNode {
    ColorConstant {
        color: [f64; 4],
    },
    FloatConstant {
        value: f64,
    },
    TextureSample {
        asset_id: String,
        uv_set: u32,
    },
    MathAdd,
    MathMultiply,
    MixBlend {
        factor: f64,
    },
    Fresnel {
        ior: f64,
    },
    NormalMap {
        scale: f64,
    },
    PbrSurfaceOutput {
        base_color_input: u32,
        metallic_input: u32,
        roughness_input: u32,
        normal_input: Option<u32>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShaderNodeGraph {
    pub name: String,
    pub nodes: Vec<ShaderNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pbr_material_to_materialx_xml() {
        let mut mat = PbrMaterial::new("CarbonFiber");
        mat.metallic = 0.85;
        mat.roughness = 0.25;
        mat.base_color = [0.1, 0.1, 0.12, 1.0];

        let xml = mat.to_materialx_xml();
        assert!(xml.contains("<materialx version=\"1.38\">"));
        assert!(xml.contains("CarbonFiber"));
        assert!(xml.contains("specular_roughness"));
    }
}
