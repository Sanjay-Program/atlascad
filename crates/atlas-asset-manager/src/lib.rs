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
    #[error("Access denied: {0}")]
    AccessDenied(String),
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
    AssemblyComponent,
    DrawingBlock,
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetStatus {
    UpToDate,
    Stale,
    Processing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetVisibility {
    Public,  // Open community marketplace / universal access
    Team,    // Shared within the user's organization / workspace
    Private, // Restricted strictly to creator / owner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LodLevel {
    Lod0, // Original fine tessellation
    Lod1, // Medium resolution (50% vertices)
    Lod2, // Low resolution (25% vertices)
    Lod3, // Proxy bounding volume
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAuthor {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub team_id: Option<String>,
    pub avatar_color: String,
}

impl Default for AssetAuthor {
    fn default() -> Self {
        Self {
            user_id: "usr_system".to_string(),
            name: "Atlas System Library".to_string(),
            email: "library@atlascad.internal".to_string(),
            team_id: None,
            avatar_color: "#22d3ee".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetParameter {
    pub name: String,
    pub param_type: String, // "number", "boolean", "string", "select"
    pub default_value: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub unit: Option<String>,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: AssetKind,
    pub category: String, // "Aerospace", "Hardware", "Robotics", "Materials", "Electrical", "Structural"
    pub version: String,  // SemVer e.g. "1.0.0"
    pub visibility: AssetVisibility,
    pub author: AssetAuthor,
    pub status: AssetStatus,
    pub content_hash: String,
    pub byte_size: usize,
    pub upstream_cad_id: Option<String>,
    pub lod_level: LodLevel,
    pub tags: Vec<String>,
    pub parameters: Vec<AssetParameter>,
    pub downloads: usize,
    pub stars: usize,
    pub forked_from: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl AssetRecord {
    pub fn new(id: impl Into<String>, name: impl Into<String>, kind: AssetKind) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            kind,
            category: "General".to_string(),
            version: "1.0.0".to_string(),
            visibility: AssetVisibility::Public,
            author: AssetAuthor::default(),
            status: AssetStatus::UpToDate,
            content_hash: String::new(),
            byte_size: 0,
            upstream_cad_id: None,
            lod_level: LodLevel::Lod0,
            tags: Vec::new(),
            parameters: Vec::new(),
            downloads: 0,
            stars: 0,
            forked_from: None,
            created_at: 1732280000,
            updated_at: 1732280000,
        }
    }

    pub fn with_author(mut self, author: AssetAuthor) -> Self {
        self.author = author;
        self
    }

    pub fn with_visibility(mut self, visibility: AssetVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_parameters(mut self, params: Vec<AssetParameter>) -> Self {
        self.parameters = params;
        self
    }
}

/// Content-addressable and multi-user dependency-aware asset registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRegistry {
    records: HashMap<String, AssetRecord>,
    // Mapping from upstream CAD feature ID to downstream asset IDs
    cad_dependents: HashMap<String, Vec<String>>,
    // In-memory binary storage payload table
    storage: HashMap<String, Vec<u8>>,
}

impl Default for AssetRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.seed_standard_catalog();
        registry
    }
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

    pub fn get_record_mut(&mut self, id: &str) -> Option<&mut AssetRecord> {
        self.records.get_mut(id)
    }

    pub fn get_data(&self, id: &str) -> Option<&[u8]> {
        self.storage.get(id).map(|v| v.as_slice())
    }

    /// Fork an existing asset into the caller's private or team workspace
    pub fn fork_asset(
        &mut self,
        source_id: &str,
        new_id: &str,
        caller: AssetAuthor,
        new_visibility: AssetVisibility,
    ) -> Result<String, AssetError> {
        let source_rec = self
            .records
            .get(source_id)
            .ok_or_else(|| AssetError::NotFound(source_id.to_string()))?
            .clone();

        // Check if caller is permitted to read source asset
        if source_rec.visibility == AssetVisibility::Private
            && source_rec.author.user_id != caller.user_id
        {
            return Err(AssetError::AccessDenied(
                "Cannot fork private asset belonging to another user".to_string(),
            ));
        }

        if source_rec.visibility == AssetVisibility::Team {
            let caller_team = caller.team_id.as_deref();
            let source_team = source_rec.author.team_id.as_deref();
            if caller_team.is_none() || caller_team != source_team {
                return Err(AssetError::AccessDenied(
                    "Cannot fork team asset outside your organization".to_string(),
                ));
            }
        }

        let data = self
            .storage
            .get(source_id)
            .cloned()
            .unwrap_or_else(Vec::new);

        let mut forked_rec = source_rec.clone();
        forked_rec.id = new_id.to_string();
        forked_rec.name = format!("{} (Fork)", source_rec.name);
        forked_rec.author = caller;
        forked_rec.visibility = new_visibility;
        forked_rec.forked_from = Some(source_id.to_string());
        forked_rec.downloads = 0;
        forked_rec.stars = 0;

        self.store_asset(forked_rec, data)
    }

    /// Multi-user permission-filtered asset query
    pub fn filter_assets(
        &self,
        viewer_user_id: &str,
        viewer_team_id: Option<&str>,
        visibility_filter: Option<AssetVisibility>,
        category_filter: Option<&str>,
        search_query: Option<&str>,
    ) -> Vec<AssetRecord> {
        let q = search_query.map(|s| s.to_lowercase());

        self.records
            .values()
            .filter(|r| {
                // 1. Permissions / Access Control
                let permitted = match r.visibility {
                    AssetVisibility::Public => true,
                    AssetVisibility::Team => {
                        let matches_team = viewer_team_id.is_some()
                            && r.author.team_id.as_deref() == viewer_team_id;
                        let matches_user = r.author.user_id == viewer_user_id;
                        matches_team || matches_user
                    }
                    AssetVisibility::Private => r.author.user_id == viewer_user_id,
                };
                if !permitted {
                    return false;
                }

                // 2. Visibility filter
                if let Some(vis) = visibility_filter
                    && r.visibility != vis
                {
                    return false;
                }

                // 3. Category filter
                if let Some(cat) = category_filter
                    && !cat.eq_ignore_ascii_case("all")
                    && !r.category.eq_ignore_ascii_case(cat)
                {
                    return false;
                }

                // 4. Search query
                if let Some(ref query) = q {
                    let in_name = r.name.to_lowercase().contains(query);
                    let in_desc = r.description.to_lowercase().contains(query);
                    let in_tags = r.tags.iter().any(|t| t.to_lowercase().contains(query));
                    let in_author = r.author.name.to_lowercase().contains(query);
                    if !in_name && !in_desc && !in_tags && !in_author {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect()
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

    /// Pre-populate the asset registry with standard world-class engineering assets
    pub fn seed_standard_catalog(&mut self) {
        let seed_assets = vec![
            (
                AssetRecord::new("ast_aero_waverider", "Aerospace Supersonic Waverider", AssetKind::CadGeometry)
                    .with_description("Parametric Mach 1.5–6.0 hypersonic waverider airframe with integrated scramjet inlet and twin canted stabilizers.")
                    .with_category("Aerospace")
                    .with_tags(vec!["aircraft".into(), "supersonic".into(), "waverider".into(), "aerodynamics".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "mach_target".into(),
                            param_type: "number".into(),
                            default_value: "3.5".into(),
                            min: Some(1.2),
                            max: Some(7.0),
                            unit: Some("M".into()),
                            options: vec![],
                        },
                        AssetParameter {
                            name: "wingspan_m".into(),
                            param_type: "number".into(),
                            default_value: "12.4".into(),
                            min: Some(6.0),
                            max: Some(30.0),
                            unit: Some("m".into()),
                            options: vec![],
                        },
                    ]),
                b"ATLASCAD_ASSET:AERO_WAVERIDER:M3.5:SWEPT_DELTA".to_vec()
            ),
            (
                AssetRecord::new("ast_hw_m8_titanium", "ISO 4762 Titanium M8x30 Bolt", AssetKind::AssemblyComponent)
                    .with_description("High-strength Grade 5 Ti-6Al-4V socket head cap screw for aerospace assemblies with analytic helical threads.")
                    .with_category("Hardware")
                    .with_tags(vec!["fastener".into(), "titanium".into(), "iso4762".into(), "bolt".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "thread_pitch".into(),
                            param_type: "number".into(),
                            default_value: "1.25".into(),
                            min: Some(0.75),
                            max: Some(2.0),
                            unit: Some("mm".into()),
                            options: vec![],
                        },
                        AssetParameter {
                            name: "nominal_length".into(),
                            param_type: "number".into(),
                            default_value: "30.0".into(),
                            min: Some(10.0),
                            max: Some(120.0),
                            unit: Some("mm".into()),
                            options: vec![],
                        },
                    ]),
                b"ATLASCAD_ASSET:ISO4762:M8x30:TI6AL4V".to_vec()
            ),
            (
                AssetRecord::new("ast_bearing_6205", "Deep Groove Ball Bearing 6205", AssetKind::AssemblyComponent)
                    .with_description("Standard ISO radial ball bearing 25x52x15 mm with dynamic load rating Cr 14.8 kN and C0r 7.8 kN.")
                    .with_category("Hardware")
                    .with_tags(vec!["bearing".into(), "deep_groove".into(), "rotary".into(), "iso".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "seal_type".into(),
                            param_type: "select".into(),
                            default_value: "2RS".into(),
                            min: None,
                            max: None,
                            unit: None,
                            options: vec!["Open".into(), "ZZ".into(), "2RS".into()],
                        },
                    ]),
                b"ATLASCAD_ASSET:BEARING_6205:25x52x15".to_vec()
            ),
            (
                AssetRecord::new("ast_mat_carbon_peek", "Carbon Fiber / PEEK Aero Composite", AssetKind::MaterialDefinition)
                    .with_description("Aerospace high-temperature thermoplastic prepreg composite (E11 = 138 GPa, Density = 1.58 g/cm³, Yield = 1,820 MPa).")
                    .with_category("Materials")
                    .with_tags(vec!["composite".into(), "carbon_fiber".into(), "peek".into(), "aerospace".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "fiber_volume_fraction".into(),
                            param_type: "number".into(),
                            default_value: "0.60".into(),
                            min: Some(0.40),
                            max: Some(0.70),
                            unit: Some("%".into()),
                            options: vec![],
                        },
                    ]),
                b"ATLASCAD_ASSET:MAT:CARBON_PEEK:Vf0.6".to_vec()
            ),
            (
                AssetRecord::new("ast_motor_nema23", "NEMA 23 High-Torque Stepper Motor", AssetKind::AssemblyComponent)
                    .with_description("Dual-shaft bipolar stepper motor (1.8 deg step angle, 2.8 A/phase, 2.2 Nm holding torque, 57x57mm faceplate).")
                    .with_category("Robotics")
                    .with_tags(vec!["motor".into(), "stepper".into(), "nema23".into(), "robotics".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "shaft_diameter".into(),
                            param_type: "number".into(),
                            default_value: "6.35".into(),
                            min: Some(5.0),
                            max: Some(8.0),
                            unit: Some("mm".into()),
                            options: vec![],
                        },
                    ]),
                b"ATLASCAD_ASSET:MOTOR:NEMA23:2.2NM".to_vec()
            ),
            (
                AssetRecord::new("ast_gearbox_planetary", "Planetary Gearbox 10:1 Drive", AssetKind::AssemblyComponent)
                    .with_description("High-precision low-backlash (<5 arcmin) coaxial planetary reduction gearbox for robotic joints and automation.")
                    .with_category("Robotics")
                    .with_tags(vec!["gearbox".into(), "planetary".into(), "transmission".into(), "robotics".into()])
                    .with_visibility(AssetVisibility::Public)
                    .with_parameters(vec![
                        AssetParameter {
                            name: "gear_ratio".into(),
                            param_type: "number".into(),
                            default_value: "10.0".into(),
                            min: Some(3.0),
                            max: Some(100.0),
                            unit: Some(":1".into()),
                            options: vec![],
                        },
                    ]),
                b"ATLASCAD_ASSET:GEARBOX:PLANETARY:10:1".to_vec()
            ),
            (
                AssetRecord::new("ast_team_internal_bracket", "Avionics Mounting Bracket MkIV", AssetKind::CadGeometry)
                    .with_description("Company proprietary avionics bay shock-isolated CNC bracket certified for FAA DO-160G vibration profiles.")
                    .with_category("Aerospace")
                    .with_tags(vec!["bracket".into(), "avionics".into(), "internal".into(), "team".into()])
                    .with_visibility(AssetVisibility::Team)
                    .with_author(AssetAuthor {
                        user_id: "usr_lead_eng".into(),
                        name: "Sarah Chen (Structural Lead)".into(),
                        email: "sarah.chen@aerocorp.internal".into(),
                        team_id: Some("team_aero_structures".into()),
                        avatar_color: "#10b981".into(),
                    }),
                b"ATLASCAD_ASSET:TEAM:AVIONICS_BRACKET_MKIV".to_vec()
            ),
            (
                AssetRecord::new("ast_private_stealth_nozzle", "Experimental Stealth Fluidic Thrust Nozzle", AssetKind::CadGeometry)
                    .with_description("Confidential patent-pending fluidic vectoring supersonic nozzle with serrated low-RCS radar chevron tiles.")
                    .with_category("Aerospace")
                    .with_tags(vec!["stealth".into(), "fluidic".into(), "nozzle".into(), "patent".into()])
                    .with_visibility(AssetVisibility::Private)
                    .with_author(AssetAuthor {
                        user_id: "usr_sanjay".into(),
                        name: "Sanjay (Chief Architect)".into(),
                        email: "sanjay@atlascad.internal".into(),
                        team_id: Some("team_aero_structures".into()),
                        avatar_color: "#6366f1".into(),
                    }),
                b"ATLASCAD_ASSET:CONFIDENTIAL:STEALTH_NOZZLE_V2".to_vec()
            ),
        ];

        for (rec, data) in seed_assets {
            let _ = self.store_asset(rec, data);
        }
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

    #[test]
    fn test_asset_visibility_filtering_and_forking() {
        let mut registry = AssetRegistry::new();
        registry.seed_standard_catalog();

        // 1. Unauthenticated or public viewer sees public assets, but NOT team or private
        let public_view = registry.filter_assets("usr_guest", None, None, None, None);
        assert!(!public_view.is_empty());
        assert!(public_view.iter().all(|a| a.visibility == AssetVisibility::Public));

        // 2. Team member sees Public + their Team's assets, but NOT another user's private assets
        let team_view = registry.filter_assets("usr_other", Some("team_aero_structures"), None, None, None);
        let has_team_bracket = team_view.iter().any(|a| a.id == "ast_team_internal_bracket");
        let has_private_nozzle = team_view.iter().any(|a| a.id == "ast_private_stealth_nozzle");
        assert!(has_team_bracket);
        assert!(!has_private_nozzle);

        // 3. Asset Owner sees Public + Team + their own Private assets
        let owner_view = registry.filter_assets("usr_sanjay", Some("team_aero_structures"), None, None, None);
        let owner_has_private = owner_view.iter().any(|a| a.id == "ast_private_stealth_nozzle");
        assert!(owner_has_private);

        // 4. Forking public asset works for another user
        let caller = AssetAuthor {
            user_id: "usr_elena".into(),
            name: "Elena (Robotics Lead)".into(),
            email: "elena@robotics.internal".into(),
            team_id: Some("team_robotics".into()),
            avatar_color: "#ec4899".into(),
        };
        let fork_res = registry.fork_asset("ast_motor_nema23", "ast_elena_nema23_custom", caller.clone(), AssetVisibility::Private);
        assert!(fork_res.is_ok());

        let maybe_forked = registry.get_record("ast_elena_nema23_custom");
        assert!(maybe_forked.is_some());
        if let Some(forked_rec) = maybe_forked {
            assert_eq!(forked_rec.author.user_id, "usr_elena");
            assert_eq!(forked_rec.visibility, AssetVisibility::Private);
            assert_eq!(forked_rec.forked_from, Some("ast_motor_nema23".into()));
        }

        // 5. Trying to fork another user's private asset fails with AccessDenied
        let illegal_fork = registry.fork_asset("ast_private_stealth_nozzle", "ast_stolen_nozzle", caller, AssetVisibility::Private);
        assert!(matches!(illegal_fork, Err(AssetError::AccessDenied(_))));
    }
}
