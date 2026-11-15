//! Project-scoped structured engineering knowledge, decision history, and agent memory.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignDecisionRecord {
    pub decision_id: String,
    pub title: String,
    pub rationale: String,
    pub proposing_agent: String,
    pub numerical_evidence: serde_json::Value,
    pub timestamp_epoch_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintStatus {
    Satisfied,
    Violated,
    PendingEvaluation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintRecord {
    pub name: String,
    pub target_value: String,
    pub achieved_value: Option<String>,
    pub status: ConstraintStatus,
    pub verification_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignRevisionRecord {
    pub revision_index: u32,
    pub solid_volume_mm3: f64,
    pub mass_grams: f64,
    pub peak_stress_mpa: f64,
    pub factor_of_safety: f64,
    pub compliance_score: f64,
    pub status: String,
    pub evidence_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectContext {
    pub project_name: String,
    pub active_material: String,
    pub coordinate_units: String,
    pub facts: BTreeMap<String, String>,
    pub decisions: Vec<DesignDecisionRecord>,
    pub constraints: BTreeMap<String, ConstraintRecord>,
    pub revisions: Vec<DesignRevisionRecord>,
    pub audit_hashes: Vec<String>,
}

impl ProjectContext {
    #[must_use]
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            project_name: project_name.into(),
            active_material: "steel.1018".to_string(),
            coordinate_units: "mm".to_string(),
            facts: BTreeMap::new(),
            decisions: Vec::new(),
            constraints: BTreeMap::new(),
            revisions: Vec::new(),
            audit_hashes: Vec::new(),
        }
    }

    pub fn set_fact(&mut self, key: impl Into<String>, val: impl Into<String>) {
        self.facts.insert(key.into(), val.into());
    }

    #[must_use]
    pub fn get_fact(&self, key: &str) -> Option<&str> {
        self.facts.get(key).map(String::as_str)
    }

    pub fn record_decision(&mut self, decision: DesignDecisionRecord) {
        self.decisions.push(decision);
    }

    pub fn track_constraint(&mut self, constraint: ConstraintRecord) {
        self.constraints.insert(constraint.name.clone(), constraint);
    }

    pub fn add_revision(&mut self, rev: DesignRevisionRecord) {
        self.revisions.push(rev);
    }
}
