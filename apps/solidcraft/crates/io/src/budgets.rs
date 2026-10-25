//! Import budgets and cancellation.
//!
//! Untrusted CAD files are the primary attack surface (docs/security-review.md S-3): a 3MF can
//! declare ten thousand zip entries of a gigabyte each, an STL can claim a billion triangles, and
//! an import can arrive for a document the user has already navigated away from. The caps that
//! make those cases fail cleanly are values here rather than scattered constants, so a host can
//! tighten them for a server or loosen them for a workstation without editing the readers.
//!
//! Every reader checks its budget *before* spending the memory, and [`Cancel`] lets a host stop a
//! long import at the next checkpoint instead of waiting for it to finish.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{IoError, Result};

/// One bounded resource an import consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Budget {
    /// The file on disk.
    FileBytes,
    /// Points or model records: mesh vertices, 3MF XML elements, STEP entities.
    Entities,
    /// Mesh triangles, after every placement.
    Triangles,
    /// Entries in a zip package (3MF).
    ZipEntries,
    /// The size one zip entry may expand to.
    UncompressedBytes,
}

impl Budget {
    /// Every budget, in the order a reader checks them.
    pub const ALL: [Budget; 5] = [Budget::FileBytes, Budget::Entities, Budget::Triangles, Budget::ZipEntries, Budget::UncompressedBytes];

    /// The resource this budget bounds, for messages.
    pub fn name(self) -> &'static str {
        match self {
            Budget::FileBytes => "file size",
            Budget::Entities => "entity count",
            Budget::Triangles => "triangle count",
            Budget::ZipEntries => "zip entry count",
            Budget::UncompressedBytes => "uncompressed size",
        }
    }

    /// The limit `budgets` sets for this budget.
    pub fn limit(self, budgets: &ImportBudgets) -> u64 {
        match self {
            Budget::FileBytes => budgets.max_file_bytes,
            Budget::Entities => budgets.max_entities,
            Budget::Triangles => budgets.max_triangles,
            Budget::ZipEntries => budgets.max_zip_entries,
            Budget::UncompressedBytes => budgets.max_uncompressed_bytes,
        }
    }
}

impl fmt::Display for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An import that would spend more of `budget` than its budget allows. The numbers are kept, so a
/// caller can tell the user exactly which limit was met instead of parsing a message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{budget} {actual} exceeds the import budget of {limit}")]
pub struct BudgetExceeded {
    /// Which resource ran out.
    pub budget: Budget,
    /// How much the file asked for.
    pub actual: u64,
    /// The limit the host set.
    pub limit: u64,
}

/// What an import may spend. [`Default`] is what a desktop session uses; set any field lower for
/// a server or a batch importer.
///
/// ```ignore
/// let budgets = ImportBudgets { max_file_bytes: 64 << 20, max_zip_entries: 500, ..Default::default() };
/// let feature = mesh_import_feature_with(bytes, path, &budgets, &cancel)?;
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBudgets {
    /// The largest file read at all, in bytes.
    pub max_file_bytes: u64,
    /// The most points or model records one import may hold.
    pub max_entities: u64,
    /// The most triangles one import may hold, after components are placed.
    pub max_triangles: u64,
    /// The most entries a 3MF package may declare.
    pub max_zip_entries: u64,
    /// The size one zip entry may expand to.
    pub max_uncompressed_bytes: u64,
}

impl Default for ImportBudgets {
    fn default() -> Self {
        ImportBudgets {
            max_file_bytes: crate::MAX_3MF_BYTES as u64,
            max_entities: crate::threemf::MAX_ELEMENTS as u64,
            max_triangles: crate::threemf::MAX_ELEMENTS as u64,
            max_zip_entries: crate::threemf::MAX_ENTRIES as u64,
            max_uncompressed_bytes: crate::threemf::MAX_PART,
        }
    }
}

impl ImportBudgets {
    /// `Err` when `actual` would spend more of `budget` than these budgets allow.
    pub fn check(&self, budget: Budget, actual: u64) -> std::result::Result<(), BudgetExceeded> {
        let limit = budget.limit(self);
        if actual > limit { Err(BudgetExceeded { budget, actual, limit }) } else { Ok(()) }
    }

    /// [`check`](Self::check) as the import error type.
    pub(crate) fn enforce(&self, budget: Budget, actual: u64) -> Result<()> {
        self.check(budget, actual).map_err(IoError::Budget)
    }
}

/// A stop signal an import checks as it goes. Cloning shares the flag, so the thread that owns
/// the import can poll [`is_cancelled`](Self::is_cancelled) while another cancels it (a host
/// closing the document, a user pressing Cancel).
#[derive(Clone, Debug, Default)]
pub struct Cancel {
    flag: Arc<AtomicBool>,
}

impl Cancel {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the running import to stop at its next checkpoint. Idempotent.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Was [`cancel`](Self::cancel) called?
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// `Err(IoError::Cancelled)` once cancelled, so a reader can `?` at a checkpoint.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(IoError::Cancelled) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_bound_every_budget() {
        let b = ImportBudgets::default();
        for budget in Budget::ALL {
            assert!(budget.limit(&b) > 0, "{budget} has no default");
        }
        // The defaults are the caps the readers already documented.
        assert_eq!(b.max_file_bytes, 512 << 20);
        assert_eq!(b.max_zip_entries, 10_000);
    }

    #[test]
    fn a_limit_is_inclusive_and_a_budget_names_itself() {
        let b = ImportBudgets { max_file_bytes: 8, ..Default::default() };
        assert!(b.check(Budget::FileBytes, 8).is_ok());
        let over = b.check(Budget::FileBytes, 9).unwrap_err();
        assert_eq!(over, BudgetExceeded { budget: Budget::FileBytes, actual: 9, limit: 8 });
        assert_eq!(over.to_string(), "file size 9 exceeds the import budget of 8");
        assert!(matches!(b.enforce(Budget::FileBytes, 9), Err(IoError::Budget(e)) if e.actual == 9));
    }

    #[test]
    fn a_cancelled_token_is_shared_and_sticky() {
        let a = Cancel::new();
        let b = a.clone();
        assert!(!a.is_cancelled());
        assert!(a.check().is_ok());
        b.cancel();
        assert!(a.is_cancelled());
        assert!(matches!(a.check(), Err(IoError::Cancelled)));
        a.cancel();
        assert!(b.is_cancelled());
    }
}
