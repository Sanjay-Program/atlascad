#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("Asset not found: {0}")]
    NotFound(String),
    #[error("Asset already exists: {0}")]
    AlreadyExists(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    CadGeometry,
    CreativeMesh,
    MaterialDefinition,
    TextureImage,
    AnimationClip,
    AudioTrack,
    VideoClip,
    RealTimeScene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetStatus {
    UpToDate,
    Stale,
    Processing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LodLevel {
    Lod0, // Original fine tessellation
    Lod1, // Medium resolution (50% vertices)
    Lod2, // Low resolution (25% vertices)
    Lod3, // Proxy bounding volume
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    pub name: String,
    pub kind: AssetKind,
    pub status: AssetStatus,
    pub content_hash: String,
    pub byte_size: usize,
    pub upstream_cad_id: Option<String>,
    pub lod_level: LodLevel,
    pub tags: Vec<String>,
}

impl AssetRecord {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: AssetKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            status: AssetStatus::UpToDate,
            content_hash: String::new(),
            byte_size: 0,
            upstream_cad_id: None,
            lod_level: LodLevel::Lod0,
            tags: Vec::new(),
        }
    }
}

/// Content-addressable and dependency-aware asset registry
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetRegistry {
    records: HashMap<String, AssetRecord>,
    // Mapping from upstream CAD feature ID to downstream asset IDs
    cad_dependents: HashMap<String, Vec<String>>,
    // In-memory binary storage payload table
    storage: HashMap<String, Vec<u8>>,
}

impl AssetRegistry {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            cad_dependents: HashMap::new(),
            storage: HashMap::new(),
        }
    }

    /// Register a new asset with binary data
    pub fn store_asset(
        &mut self,
        mut record: AssetRecord,
        data: Vec<u8>,
    ) -> Result<String, AssetError> {
        let hash = compute_fnv_hash(&data);
        record.content_hash = hash;
        record.byte_size = data.len();
        let id = record.id.clone();

        if let Some(ref cad_id) = record.upstream_cad_id {
            self.cad_dependents
                .entry(cad_id.clone())
                .or_default()
                .push(id.clone());
        }

        self.storage.insert(id.clone(), data);
        self.records.insert(id.clone(), record);
        Ok(id)
    }

    pub fn get_record(&self, id: &str) -> Option<&AssetRecord> {
        self.records.get(id)
    }

    pub fn get_data(&self, id: &str) -> Option<&[u8]> {
        self.storage.get(id).map(|v| v.as_slice())
    }

    /// Called by the CAD feature tree whenever a parametric solid or feature is rebuilt.
    /// Flags all downstream creative meshes, LODs, and scenes as Stale!
    pub fn invalidate_cad_feature(&mut self, cad_id: &str) -> usize {
        let mut count = 0;
        if let Some(dependents) = self.cad_dependents.get(cad_id) {
            for dep_id in dependents {
                if let Some(rec) = self.records.get_mut(dep_id)
                    && rec.status != AssetStatus::Stale
                {
                    rec.status = AssetStatus::Stale;
                    count += 1;
                }
            }
        }
        count
    }

    /// Revalidate and mark an asset as fresh after re-tessellation or update
    pub fn mark_rebuilt(&mut self, id: &str, new_data: Vec<u8>) -> Result<(), AssetError> {
        let hash = compute_fnv_hash(&new_data);
        let len = new_data.len();
        if let Some(rec) = self.records.get_mut(id) {
            rec.content_hash = hash;
            rec.byte_size = len;
            rec.status = AssetStatus::UpToDate;
            self.storage.insert(id.to_string(), new_data);
            Ok(())
        } else {
            Err(AssetError::NotFound(id.to_string()))
        }
    }

    /// Retrieve all assets that currently require regeneration
    pub fn list_stale_assets(&self) -> Vec<AssetRecord> {
        self.records
            .values()
            .filter(|r| r.status == AssetStatus::Stale)
            .cloned()
            .collect()
    }
}

/// Simple 64-bit FNV-1a hash formatted as hex string
fn compute_fnv_hash(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_storage_and_cad_invalidation() {
        let mut registry = AssetRegistry::new();

        let mut mesh_asset = AssetRecord::new("fuselage_mesh", "Fuselage High-Res", AssetKind::CreativeMesh);
        mesh_asset.upstream_cad_id = Some("cad_body_loft_01".to_string());

        let store_res = registry.store_asset(mesh_asset, b"VERTICES:12345,FACES:24680".to_vec());
        assert!(store_res.is_ok());

        let initial_rec = registry.get_record("fuselage_mesh");
        assert!(initial_rec.is_some());
        if let Some(r) = initial_rec {
            assert_eq!(r.status, AssetStatus::UpToDate);
        }

        // CAD parametric change triggers invalidation
        let invalidated = registry.invalidate_cad_feature("cad_body_loft_01");
        assert_eq!(invalidated, 1);

        let stale_list = registry.list_stale_assets();
        assert_eq!(stale_list.len(), 1);
        assert_eq!(stale_list[0].id, "fuselage_mesh");

        // Rebuild regenerates data and marks clean
        let rebuild_res = registry.mark_rebuilt("fuselage_mesh", b"VERTICES:12400,FACES:24800".to_vec());
        assert!(rebuild_res.is_ok());

        assert_eq!(registry.list_stale_assets().len(), 0);
    }
}
