//! The Atlas CAD engineering control plane.
//!
//! This crate is the machine-readable memory of the project. It answers, without a chat transcript:
//! *what repositories exist, what they are licensed under, what is integrated, what is proven, and
//! what work is blocked.* It is deliberately dependency-light (`serde` + `serde_json` + `thiserror`)
//! so it can run on the memory-constrained development machine described in
//! `docs/build-and-test.md`.
//!
//! # What it does
//!
//! * [`manifest`] — loads and cross-validates the manifests under `engineering/manifest/`.
//! * [`fingerprint`] — deterministic, documented content fingerprints (FNV-1a 64, path-sorted).
//! * [`scan`] — observes the workspace: Git state, file digests, Cargo packages.
//! * [`diff`] — turns observations into [`model::Change`] records, including changed-file lists.
//! * [`tasks`] — synthesises scanner tasks with ids derived from the observed value, and merges them
//!   idempotently (an unchanged scan cannot duplicate a task).
//! * [`report`] — renders the integration report from manifests + observations.
//! * [`supply`] — checks every dependency's licence against the release policy and emits a
//!   deterministic CycloneDX SBOM, offline.
//!
//! # What it deliberately does not do
//!
//! It does not modify source repositories, does not run builds or tests, and never rewrites a
//! curated manifest. A scan writes only [`paths::SCAN_STATE`]; everything else is evidence to be
//! reviewed by a human or an engineering session. This is what stops a source change from silently
//! overwriting a known-good integration record.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod diff;
pub mod fingerprint;
pub mod manifest;
pub mod model;
pub mod paths;
pub mod report;
pub mod scan;
pub mod supply;
pub mod tasks;

pub use manifest::{Diagnostics, ManifestSet, TaskStores};
pub use model::*;

/// Errors produced by the control plane.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("manifest validation failed:\n{}", errors.join("\n"))]
    Validation { errors: Vec<String> },
    #[error("{0}")]
    Message(String),
}

impl Error {
    /// Build an [`Error::Io`] carrying the path that failed.
    pub fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Error::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Version string of the scanner, recorded in every scan state.
pub const SCANNER_VERSION: &str = concat!("cad-engineering/", env!("CARGO_PKG_VERSION"));
