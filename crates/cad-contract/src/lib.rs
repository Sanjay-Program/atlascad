//! The stable contract between a Atlas CAD application and a geometry kernel.
//!
//! This crate deliberately contains **no geometry implementation**. It owns the vocabulary two
//! applications (CADCraft, a 2D drafting app, and SolidCraft, a parametric 3D app) can agree on
//! without either giving up its own types:
//!
//! * [`units`] — an explicit unit system instead of an implicit one.
//! * [`ids`] — stable, application-level identifiers that are safe to persist.
//! * [`tol`] — validated geometric tolerances.
//! * [`capability`] — capability discovery, so a caller can ask what a kernel can do before using it.
//! * [`error`] — one error taxonomy shared by every kernel adapter.
//! * [`mesh`] — a plain triangle-mesh interchange type with invariants that can be checked.
//! * [`kernel`] — the minimal operations a 3D kernel must provide.
//! * [`document`] — a versioned document header with tested migrations.
//!
//! # Design rules
//!
//! 1. **No kernel types leak.** The contract uses plain arrays and opaque handles, so a truck
//!    adapter, an OCCT adapter, or a future in-house kernel can implement it without exposing
//!    their own topology types (the mistake that makes a kernel swap impossible).
//! 2. **No lowest-common-denominator geometry.** Kernel-specific abilities are *advertised* through
//!    [`capability::Capability`] and reached through the adapter's own API, rather than being
//!    flattened into a trait that cannot express them.
//! 3. **Nothing here is persisted except what is explicitly marked.** [`kernel::SolidId`] is a
//!    per-session handle and must never be written to a project file; [`ids::EntityId`] is the
//!    persistent identifier.
//!
//! # Example
//!
//! ```
//! use cad_contract::{capability::{Capability, CapabilitySet}, units::Unit};
//!
//! let caps = CapabilitySet::from([Capability::Extrude, Capability::Tessellate]);
//! assert!(caps.contains(Capability::Extrude));
//! assert!(!caps.contains(Capability::Fillet));
//! assert_eq!(Unit::Inch.to_millimeters(), 25.4);
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod capability;
pub mod document;
pub mod error;
pub mod ids;
pub mod kernel;
pub mod mesh;
pub mod tol;
pub mod units;

pub use capability::{Capability, CapabilitySet, KernelInfo};
pub use error::{ErrorKind, KernelError, Result};
pub use ids::{DocumentId, EntityId};
pub use kernel::{BooleanOp, Kernel, SolidId};
pub use mesh::{Aabb3, Mesh};
pub use tol::Tolerance;
pub use units::Unit;
