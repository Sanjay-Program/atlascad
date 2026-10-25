//! One error taxonomy for every kernel adapter.
//!
//! The variants follow SolidCraft's shipped `KernelError` (`Invalid`, `Failed`, `TooLarge`,
//! `Internal`), because those four categories have already survived contact with a real B-rep
//! kernel. Two things are added that a UI and a retry policy actually need:
//!
//! * [`KernelError::Unsupported`] — a capability the kernel does not advertise, so the caller can
//!   disable the command instead of showing "the operation failed".
//! * [`KernelError::Cancelled`] and [`KernelError::Timeout`] — long operations must be
//!   interruptible (brief §10), and a cancelled operation is not a failure.
//!
//! [`ErrorKind`] exists so the UI can branch on a small enum without matching on strings, and so
//! that "retryable" is a property of the kind rather than a guess at the message.

use crate::capability::Capability;
use crate::kernel::SolidId;

/// A kernel operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KernelError {
    /// The kernel does not provide this capability.
    #[error("{capability} is not supported by this kernel")]
    Unsupported { capability: Capability },

    /// The caller supplied geometry or parameters the kernel cannot accept.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// The operation was well-formed but produced no valid result.
    #[error("the operation failed: {0}")]
    Failed(String),

    /// A size (fillet radius, shell thickness, …) is too large for the geometry around it.
    /// A smaller value may succeed, so this is reported separately from a plain failure.
    #[error("not supported at this size: {0}")]
    TooLarge(String),

    /// The underlying kernel panicked or hit an internal invariant. The model is unchanged.
    #[error("kernel internal error: {0}")]
    Internal(String),

    /// A handle from a different kernel session, or one already released.
    #[error("unknown solid handle {0:?}")]
    UnknownSolid(SolidId),

    /// The operation was cancelled by the caller.
    #[error("cancelled")]
    Cancelled,

    /// The operation exceeded its deadline.
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
}

impl KernelError {
    /// The category, for UI branching.
    pub fn kind(&self) -> ErrorKind {
        match self {
            KernelError::Unsupported { .. } => ErrorKind::Unsupported,
            KernelError::Invalid(_) => ErrorKind::InvalidInput,
            KernelError::Failed(_) => ErrorKind::OperationFailed,
            KernelError::TooLarge(_) => ErrorKind::SizeTooLarge,
            KernelError::Internal(_) => ErrorKind::Internal,
            KernelError::UnknownSolid(_) => ErrorKind::InvalidHandle,
            KernelError::Cancelled => ErrorKind::Cancelled,
            KernelError::Timeout(_) => ErrorKind::Timeout,
        }
    }

    /// Should the caller offer to retry the same operation unchanged?
    ///
    /// Only a panic-class internal error and a timeout are worth retrying as-is. A size problem
    /// needs user input, and an invalid input will fail identically.
    pub fn is_retryable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Internal | ErrorKind::Timeout)
    }

    /// Does this error mean the document is untouched?
    ///
    /// Every error in this taxonomy is fail-closed: the contract requires an adapter to leave the
    /// document unchanged when it returns `Err`. This helper exists so callers read as intent
    /// rather than as an assumption.
    pub const fn document_unchanged(&self) -> bool {
        true
    }
}

/// The coarse category of a [`KernelError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Unsupported,
    InvalidInput,
    OperationFailed,
    SizeTooLarge,
    Internal,
    InvalidHandle,
    Cancelled,
    Timeout,
}

/// The result type used by every kernel operation in the contract.
pub type Result<T> = core::result::Result<T, KernelError>;

#[cfg(test)]
mod tests {
    use super::{ErrorKind, KernelError};
    use crate::capability::Capability;

    #[test]
    fn unsupported_names_the_capability() {
        let e = KernelError::Unsupported {
            capability: Capability::Fillet,
        };
        let msg = e.to_string();
        assert!(msg.contains("fillet"), "message was {msg}");
        assert_eq!(e.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn only_internal_and_timeout_are_retryable() {
        assert!(KernelError::Internal("boom".into()).is_retryable());
        assert!(KernelError::Timeout(std::time::Duration::from_secs(1)).is_retryable());
        assert!(!KernelError::Invalid("bad".into()).is_retryable());
        assert!(!KernelError::TooLarge("radius".into()).is_retryable());
        assert!(!KernelError::Cancelled.is_retryable());
    }

    #[test]
    fn every_error_means_the_document_is_unchanged() {
        let all = [
            KernelError::Unsupported {
                capability: Capability::Loft,
            },
            KernelError::Invalid("x".into()),
            KernelError::Failed("x".into()),
            KernelError::TooLarge("x".into()),
            KernelError::Internal("x".into()),
            KernelError::Cancelled,
        ];
        for e in all {
            assert!(e.document_unchanged());
        }
    }
}
