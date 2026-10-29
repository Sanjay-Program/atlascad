//! Capability discovery.
//!
//! A kernel that cannot do fillets should say so, not fail with a string after the user has drawn
//! the geometry. Callers ask [`KernelInfo::capabilities`] up-front and gate UI and command
//! validation on [`CapabilitySet::contains`].
//!
//! Capabilities describe *this* kernel, not the vendor's roadmap. A capability is only listed when
//! the adapter has an operation behind it that is exercised by a test.

use serde::{Deserialize, Serialize};

/// An operation a geometry kernel may provide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// A closed planar profile swept linearly into a solid.
    Extrude,
    /// A profile revolved about an axis.
    Revolve,
    /// Sweep along a path.
    Sweep,
    /// Loft between profiles.
    Loft,
    BooleanUnion,
    BooleanDifference,
    BooleanIntersection,
    /// Constant-radius edge/face blend.
    Fillet,
    /// Edge chamfer.
    Chamfer,
    /// Hollowing / offset of a solid.
    Shell,
    /// Rigid translation of a solid. Needed to place a tool that must overshoot its target, which
    /// is how an exact through-cut is built without coincident faces.
    Transform,
    /// Triangle tessellation of a solid.
    Tessellate,
    StepImport,
    StepExport,
    StlImport,
    StlExport,
    IgfesImport,
    IgfesExport,
    /// Surface/volume properties from exact geometry rather than a mesh.
    MassProperties,
    /// The kernel builds for `wasm32-unknown-unknown`.
    Wasm,
}

impl Capability {
    /// Every capability, in a stable order (useful for reports and UI lists).
    pub const ALL: &'static [Capability] = &[
        Capability::Extrude,
        Capability::Revolve,
        Capability::Sweep,
        Capability::Loft,
        Capability::BooleanUnion,
        Capability::BooleanDifference,
        Capability::BooleanIntersection,
        Capability::Fillet,
        Capability::Chamfer,
        Capability::Shell,
        Capability::Transform,
        Capability::Tessellate,
        Capability::StepImport,
        Capability::StepExport,
        Capability::StlImport,
        Capability::StlExport,
        Capability::IgfesImport,
        Capability::IgfesExport,
        Capability::MassProperties,
        Capability::Wasm,
    ];

    /// The stable bit for this capability.
    const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }

    /// A human-readable name for messages.
    pub const fn label(self) -> &'static str {
        match self {
            Capability::Extrude => "extrude",
            Capability::Revolve => "revolve",
            Capability::Sweep => "sweep",
            Capability::Loft => "loft",
            Capability::BooleanUnion => "boolean union",
            Capability::BooleanDifference => "boolean difference",
            Capability::BooleanIntersection => "boolean intersection",
            Capability::Fillet => "fillet",
            Capability::Chamfer => "chamfer",
            Capability::Shell => "shell",
            Capability::Transform => "transform",
            Capability::Tessellate => "tessellate",
            Capability::StepImport => "STEP import",
            Capability::StepExport => "STEP export",
            Capability::StlImport => "STL import",
            Capability::StlExport => "STL export",
            Capability::IgfesImport => "IGES import",
            Capability::IgfesExport => "IGES export",
            Capability::MassProperties => "mass properties",
            Capability::Wasm => "WebAssembly build",
        }
    }
}

impl core::fmt::Display for Capability {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

/// A set of [`Capability`] values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySet(u32);

impl CapabilitySet {
    /// An empty set: a kernel that advertises nothing.
    pub const fn empty() -> Self {
        CapabilitySet(0)
    }

    /// A set containing every known capability (use only when each one is truly tested).
    pub fn all() -> Self {
        let mut set = CapabilitySet::empty();
        for c in Capability::ALL {
            set.insert(*c);
        }
        set
    }

    /// Add a capability.
    pub fn insert(&mut self, capability: Capability) {
        self.0 |= capability.bit();
    }

    /// Does the set contain `capability`?
    pub const fn contains(self, capability: Capability) -> bool {
        self.0 & capability.bit() != 0
    }

    /// The capabilities present, in [`Capability::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .iter()
            .copied()
            .filter(move |c| self.contains(*c))
    }

    /// Capabilities the caller requires but the set lacks.
    pub fn missing(self, required: &[Capability]) -> Vec<Capability> {
        required
            .iter()
            .copied()
            .filter(|c| !self.contains(*c))
            .collect()
    }

    /// Number of capabilities present.
    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// No capabilities at all.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        let mut set = CapabilitySet::empty();
        for c in iter {
            set.insert(c);
        }
        set
    }
}

impl<const N: usize> From<[Capability; N]> for CapabilitySet {
    fn from(v: [Capability; N]) -> Self {
        v.into_iter().collect()
    }
}

/// What a kernel says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelInfo {
    /// Kernel name, e.g. `"truck"`.
    pub name: String,
    /// The crate version this adapter was built against, e.g. `"0.6"`.
    pub version: String,
    /// What it can actually do.
    pub capabilities: CapabilitySet,
}

impl KernelInfo {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        capabilities: CapabilitySet,
    ) -> Self {
        KernelInfo {
            name: name.into(),
            version: version.into(),
            capabilities,
        }
    }

    /// Is `capability` available?
    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(capability)
    }
}

#[cfg(test)]
mod tests {
    use super::{Capability, CapabilitySet, KernelInfo};

    #[test]
    fn bits_are_distinct_for_every_capability() {
        // If two capabilities shared a bit, `contains` would lie. Check the count, not the values.
        let all = CapabilitySet::all();
        assert_eq!(all.len(), Capability::ALL.len());
        for c in Capability::ALL {
            assert!(all.contains(*c), "{} missing from all()", c.label());
        }
    }

    #[test]
    fn empty_contains_nothing() {
        let empty = CapabilitySet::empty();
        assert!(empty.is_empty());
        for c in Capability::ALL {
            assert!(!empty.contains(*c));
        }
    }

    #[test]
    fn insert_and_contains() {
        let mut set = CapabilitySet::empty();
        set.insert(Capability::Extrude);
        set.insert(Capability::Tessellate);
        assert!(set.contains(Capability::Extrude));
        assert!(set.contains(Capability::Tessellate));
        assert!(!set.contains(Capability::Fillet));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn missing_reports_the_gap() {
        let set = CapabilitySet::from([Capability::Extrude]);
        let missing = set.missing(&[Capability::Extrude, Capability::Fillet, Capability::Shell]);
        assert_eq!(missing, vec![Capability::Fillet, Capability::Shell]);
    }

    #[test]
    fn iter_is_stable_and_complete() {
        let set = CapabilitySet::from([Capability::Tessellate, Capability::Extrude]);
        let listed: Vec<_> = set.iter().collect();
        assert_eq!(listed, vec![Capability::Extrude, Capability::Tessellate]);
    }

    #[test]
    fn json_round_trip() {
        let info = KernelInfo::new(
            "truck",
            "0.6",
            CapabilitySet::from([Capability::Extrude, Capability::Tessellate]),
        );
        let json = serde_json::to_string(&info).unwrap_or_default();
        assert_eq!(serde_json::from_str::<KernelInfo>(&json).ok(), Some(info));
    }
}
