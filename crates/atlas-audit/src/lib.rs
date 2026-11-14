//! # atlas-audit
//!
//! Cryptographic, immutable, hash-chained engineering audit trail
//! compliant with ISO 9001, AS9100, and FDA 21 CFR Part 11.

pub mod action;
pub mod chain;
pub mod entry;
pub mod standards;

pub mod prelude {
    pub use crate::action::AuditAction;
    pub use crate::chain::{AuditChain, AuditError};
    pub use crate::entry::AuditEntry;
    pub use crate::standards::{ComplianceEvaluation, ComplianceEvaluator};
}
