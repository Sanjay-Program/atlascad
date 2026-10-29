//! Dependency licence policy and SBOM generation.
//!
//! `cargo-deny` and `cargo-audit` are not installed on the development host, so the release gate
//! needs a check that runs offline and answers the one question a commercial sale turns on: *is every
//! third-party crate in the shipped graph under a licence this product may distribute?*
//!
//! The check reads `cargo metadata` (already resolved, no network), classifies each package's
//! licence expression against [`LicencePolicy`], and refuses anything denied or unrecognised. An
//! unrecognised licence is a failure, not a warning: "we did not look" is exactly how a copyleft
//! dependency reaches a proprietary binary.
//!
//! The SBOM is CycloneDX 1.5 with a deterministic component order and no timestamp, so two runs on
//! the same lockfile produce byte-identical output and a diff in CI means the graph changed.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The licence policy a release is checked against.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LicencePolicy {
    pub schema_version: u32,
    /// Licence identifiers this product may distribute.
    pub allow: Vec<String>,
    /// Licence identifiers it may not. Checked before `allow`, so a mistake in one list cannot
    /// make a denied licence pass by also being allowed.
    pub deny: Vec<String>,
    /// Crates that must never appear in the graph, whatever their licence (e.g. a headless
    /// OpenCASCADE wrapper that statically links LGPL code).
    #[serde(default)]
    pub banned_crates: Vec<String>,
    /// The project's own licence, used for workspace members that declare a licence file instead
    /// of an SPDX expression.
    #[serde(default)]
    pub workspace_licence: String,
}

/// The result of classifying one package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Every licence token is on the allow list.
    Allowed,
    /// At least one token is on the deny list, or the crate is banned.
    Denied,
    /// At least one token is not on either list. Treated as a failure by [`Audit::is_ok`].
    Unknown,
}

/// One package's verdict, with the reason a human can check.
#[derive(Debug, Clone, Serialize)]
pub struct PackageVerdict {
    pub name: String,
    pub version: String,
    pub licence: Option<String>,
    pub verdict: Verdict,
    pub reason: String,
    pub workspace: bool,
}

/// The whole audit.
#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub packages: Vec<PackageVerdict>,
}

impl Audit {
    /// Every package whose verdict is not [`Verdict::Allowed`].
    pub fn failures(&self) -> impl Iterator<Item = &PackageVerdict> {
        self.packages
            .iter()
            .filter(|package| package.verdict != Verdict::Allowed)
    }

    /// `true` only when every package is allowed.
    pub fn is_ok(&self) -> bool {
        self.failures().next().is_none()
    }

    /// Counts by verdict, in `allowed, denied, unknown` order.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for package in &self.packages {
            match package.verdict {
                Verdict::Allowed => counts.0 += 1,
                Verdict::Denied => counts.1 += 1,
                Verdict::Unknown => counts.2 += 1,
            }
        }
        counts
    }
}

/// Load the policy JSON.
pub fn load_policy(path: &Path) -> Result<LicencePolicy> {
    let bytes = std::fs::read(path).map_err(|source| Error::io(path, source))?;
    serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: path.display().to_string(),
        source,
    })
}

/// `cargo metadata` for the workspace at `root`, resolved for the host target.
///
/// The platform filter matters: resolving every optional dependency for every target needs crates
/// that are not in the offline cache, and this check must run without a network.
pub fn metadata(root: &Path) -> Result<serde_json::Value> {
    let mut command = Command::new("cargo");
    command
        .arg("metadata")
        .arg("--offline")
        .arg("--format-version")
        .arg("1");
    if let Some(host) = host_triple() {
        command.arg("--filter-platform").arg(host);
    }
    let output = command
        .current_dir(root)
        .output()
        .map_err(|source| Error::io(root, source))?;
    if !output.status.success() {
        return Err(Error::Message(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout).map_err(|source| Error::Json {
        path: "cargo metadata".to_string(),
        source,
    })
}

fn host_triple() -> Option<String> {
    let output = Command::new("rustc").arg("-vV").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|host| host.trim().to_string())
}

/// Split a SPDX licence expression into its identifier tokens.
///
/// Handles the forms Cargo metadata actually contains: `MIT OR Apache-2.0`, the legacy
/// `MIT/Apache-2.0`, parenthesised combinations, and `Apache-2.0 WITH LLVM-exception`.
pub fn licence_tokens(expression: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in expression.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+') {
            current.push(character);
        } else {
            push_token(&mut tokens, &mut current);
        }
    }
    push_token(&mut tokens, &mut current);
    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.is_empty() {
        return;
    }
    if !matches!(current.as_str(), "AND" | "OR" | "WITH") {
        tokens.push(current.clone());
    }
    current.clear();
}

/// Classify one licence expression.
///
/// A denied token wins over an allowed one; an unrecognised token makes the whole expression
/// unknown. A package with no licence expression at all is unknown, because its terms are then
/// whatever the file it ships says, which this check cannot read.
pub fn classify(expression: Option<&str>, policy: &LicencePolicy) -> (Verdict, String) {
    let Some(expression) = expression.filter(|value| !value.trim().is_empty()) else {
        return (
            Verdict::Unknown,
            "no licence expression declared".to_string(),
        );
    };
    let tokens = licence_tokens(expression);
    if tokens.is_empty() {
        return (
            Verdict::Unknown,
            format!("licence `{expression}` has no identifiers"),
        );
    }
    let denied: Vec<&String> = tokens
        .iter()
        .filter(|token| policy.deny.iter().any(|denied| denied == *token))
        .collect();
    if !denied.is_empty() {
        let names: Vec<&str> = denied.iter().map(|token| token.as_str()).collect();
        return (
            Verdict::Denied,
            format!("denied licence(s): {}", names.join(", ")),
        );
    }
    let unknown: Vec<&String> = tokens
        .iter()
        .filter(|token| !policy.allow.iter().any(|allowed| allowed == *token))
        .collect();
    if !unknown.is_empty() {
        let names: Vec<&str> = unknown.iter().map(|token| token.as_str()).collect();
        return (
            Verdict::Unknown,
            format!(
                "licence(s) not in the allow list: {} (add them deliberately if they are safe)",
                names.join(", ")
            ),
        );
    }
    (Verdict::Allowed, format!("allow-listed: {expression}"))
}

/// Audit every package `cargo metadata` reports.
pub fn audit(metadata: &serde_json::Value, policy: &LicencePolicy) -> Result<Audit> {
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::Message("cargo metadata has no `packages` array".into()))?;
    let members: BTreeSet<&str> = metadata
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<BTreeSet<&str>>()
        })
        .unwrap_or_default();

    let mut verdicts = Vec::with_capacity(packages.len());
    for package in packages {
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| Error::Message("cargo metadata package has no name".into()))?;
        let version = package
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let id = package
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let workspace = members.contains(id);
        let licence = package
            .get("license")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);

        // A workspace member is this product, so its declared terms are the product's own; the
        // commercial agreement governs them, not the third-party allow list.
        let (verdict, reason) = if workspace {
            match &licence {
                Some(expression) => (
                    Verdict::Allowed,
                    format!("workspace member under its own licence: {expression}"),
                ),
                None => (
                    Verdict::Allowed,
                    format!(
                        "workspace member under its own licence: {}",
                        if policy.workspace_licence.is_empty() {
                            "declared licence file"
                        } else {
                            policy.workspace_licence.as_str()
                        }
                    ),
                ),
            }
        } else if policy.banned_crates.iter().any(|banned| banned == name) {
            (
                Verdict::Denied,
                format!("crate `{name}` is banned by policy whatever its licence"),
            )
        } else {
            classify(licence.as_deref(), policy)
        };

        verdicts.push(PackageVerdict {
            name: name.to_string(),
            version: version.to_string(),
            licence,
            verdict,
            reason,
            workspace,
        });
    }
    verdicts.sort_by(|left, right| {
        (left.name.as_str(), left.version.as_str())
            .cmp(&(right.name.as_str(), right.version.as_str()))
    });
    Ok(Audit { packages: verdicts })
}

/// Render a deterministic CycloneDX 1.5 SBOM from the metadata.
pub fn sbom(metadata: &serde_json::Value, policy: &LicencePolicy) -> Result<serde_json::Value> {
    let audit = audit(metadata, policy)?;
    let mut components = Vec::with_capacity(audit.packages.len());
    for package in &audit.packages {
        let mut component = serde_json::json!({
            "type": if package.workspace { "application" } else { "library" },
            "name": package.name,
            "version": package.version,
            "purl": format!("pkg:cargo/{}@{}", package.name, package.version),
            "properties": [
                { "name": "atlas:licence_verdict", "value": format!("{:?}", package.verdict).to_lowercase() },
                { "name": "atlas:licence_reason", "value": package.reason }
            ]
        });
        let licence = package
            .licence
            .clone()
            .unwrap_or_else(|| policy.workspace_licence.clone());
        if !licence.is_empty() {
            component["licenses"] = if package.licence.is_some() {
                serde_json::json!([{ "expression": licence }])
            } else {
                serde_json::json!([{ "license": { "name": licence } }])
            };
        }
        components.push(component);
    }
    Ok(serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "tools": [{ "vendor": "AtlasCAD", "name": "atlas-eng", "version": env!("CARGO_PKG_VERSION") }],
            "component": {
                "type": "application",
                "name": "atlascad",
                "version": env!("CARGO_PKG_VERSION")
            }
        },
        "components": components
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn policy() -> LicencePolicy {
        LicencePolicy {
            schema_version: 1,
            allow: vec![
                "MIT".into(),
                "Apache-2.0".into(),
                "Unicode-3.0".into(),
                "Zlib".into(),
            ],
            deny: vec!["GPL-3.0-only".into(), "AGPL-3.0-only".into()],
            banned_crates: vec!["cadrum".into(), "opencascade".into()],
            workspace_licence: "LicenseRef-Example".into(),
        }
    }

    #[test]
    fn expressions_are_split_into_identifiers() {
        assert_eq!(licence_tokens("MIT OR Apache-2.0"), ["MIT", "Apache-2.0"]);
        assert_eq!(licence_tokens("MIT/Apache-2.0"), ["MIT", "Apache-2.0"]);
        assert_eq!(
            licence_tokens("(MIT OR Apache-2.0) AND Unicode-3.0"),
            ["MIT", "Apache-2.0", "Unicode-3.0"]
        );
        assert_eq!(
            licence_tokens("Apache-2.0 WITH LLVM-exception"),
            ["Apache-2.0", "LLVM-exception"]
        );
    }

    #[test]
    fn an_allowed_expression_passes_and_a_denied_one_fails() {
        let policy = policy();
        assert_eq!(
            classify(Some("MIT OR Apache-2.0"), &policy).0,
            Verdict::Allowed
        );
        assert_eq!(
            classify(Some("MIT/Apache-2.0"), &policy).0,
            Verdict::Allowed
        );
        assert_eq!(
            classify(Some("MIT AND GPL-3.0-only"), &policy).0,
            Verdict::Denied,
            "a denied token must win over an allowed one"
        );
    }

    #[test]
    fn an_unknown_or_missing_licence_fails_rather_than_passing() {
        let policy = policy();
        assert_eq!(classify(Some("MIT OR WTFPL"), &policy).0, Verdict::Unknown);
        assert_eq!(classify(None, &policy).0, Verdict::Unknown);
        assert_eq!(classify(Some("   "), &policy).0, Verdict::Unknown);
    }

    fn fixture() -> serde_json::Value {
        serde_json::json!({
            "packages": [
                { "id": "member 0.1.0", "name": "cad-document", "version": "0.1.0", "license": null, "license_file": "LICENSE" },
                { "id": "mit 1.0.0", "name": "permissive", "version": "1.0.0", "license": "MIT OR Apache-2.0" },
                { "id": "gpl 1.0.0", "name": "copyleft", "version": "2.0.0", "license": "GPL-3.0-only" },
                { "id": "banned 1.0.0", "name": "cadrum", "version": "0.1.0", "license": "MIT" }
            ],
            "workspace_members": ["member 0.1.0"]
        })
    }

    #[test]
    fn the_audit_separates_members_allowed_denied_and_banned() -> TestResult {
        let audit = audit(&fixture(), &policy())?;
        let by_name = |name: &str| {
            audit
                .packages
                .iter()
                .find(|package| package.name == name)
                .map(|package| package.verdict)
        };
        assert_eq!(by_name("cad-document"), Some(Verdict::Allowed));
        assert_eq!(by_name("permissive"), Some(Verdict::Allowed));
        assert_eq!(by_name("copyleft"), Some(Verdict::Denied));
        assert_eq!(
            by_name("cadrum"),
            Some(Verdict::Denied),
            "a banned crate is denied whatever its licence says"
        );
        assert_eq!(audit.counts(), (2, 2, 0));
        assert!(!audit.is_ok());
        Ok(())
    }

    #[test]
    fn the_sbom_is_deterministic_and_records_every_package() -> TestResult {
        let policy = policy();
        let first = sbom(&fixture(), &policy)?;
        let second = sbom(&fixture(), &policy)?;
        assert_eq!(
            serde_json::to_string(&first)?,
            serde_json::to_string(&second)?,
            "two runs on the same metadata must be byte-identical"
        );
        assert_eq!(first["bomFormat"], "CycloneDX");
        let components = first["components"]
            .as_array()
            .ok_or("components must be an array")?;
        assert_eq!(components.len(), 4);
        let names: Vec<&str> = components
            .iter()
            .filter_map(|component| component["name"].as_str())
            .collect();
        assert_eq!(names, ["cad-document", "cadrum", "copyleft", "permissive"]);
        assert_eq!(components[0]["purl"], "pkg:cargo/cad-document@0.1.0");
        Ok(())
    }
}
