//! Workspace observation.
//!
//! The scanner is read-only with respect to source repositories. It reads Git metadata through the
//! `git` binary (no libgit2 dependency), fingerprints tracked files, and asks Cargo for the
//! workspace package list with `cargo metadata --no-deps --offline` (no network, no build).
//!
//! Every external command is bounded: a hung `git` or `cargo` cannot stall an engineering cycle.
//! Timeouts are recorded in the observation notes rather than silently producing an empty result.

use crate::fingerprint;
use crate::model::*;
use crate::{Error, Result, SCANNER_VERSION};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default bound for a single external command (`git rev-parse`, `cargo metadata`).
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Scanner<'a> {
    root: &'a Path,
    policy: WorkspacePolicy,
    command_timeout: Duration,
}

impl<'a> Scanner<'a> {
    /// The policy is owned so a scanner can be built from a temporary without lifetime gymnastics.
    pub fn new(root: &'a Path, policy: WorkspacePolicy) -> Self {
        Scanner {
            root,
            policy,
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }

    pub fn with_command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = timeout;
        self
    }

    /// Observe the workspace. `repositories` supplies the declared ids, so an observation keeps a
    /// stable id instead of deriving one from a directory name.
    pub fn scan(&self, repositories: &[RepositoryEntry]) -> Result<ScanState> {
        let discovered = self.discover()?;
        let mut observations: Vec<RepoObservation> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        for repo in repositories {
            if repo.kind == RepoKind::External {
                continue;
            }
            let normalized = crate::manifest::normalize_path(&repo.path);
            seen.insert(normalized.clone());
            // Presence is a filesystem fact, not a discovery-list fact: a declared repository that
            // has no Git metadata and no Cargo.toml is still present and must not look missing.
            let exists = self.root.join(&normalized).is_dir();
            observations.push(self.observe(repo.id.clone(), &normalized, true, exists));
        }

        for path in discovered {
            if seen.contains(&path) {
                continue;
            }
            // An unregistered checkout still gets a stable, content-independent id so repeated scans
            // do not rename it (renaming would create a new task every scan).
            let id = format!("repo.unregistered.{}", slug(&path.replace('/', "-")));
            observations.push(self.observe(id, &path, false, true));
        }

        observations.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(ScanState {
            schema_version: SCHEMA_VERSION,
            observed_at_unix: now_unix(),
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                cpus: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(0),
                scanner_version: SCANNER_VERSION.to_string(),
            },
            repos: observations,
        })
    }

    /// Candidate repositories: a directory directly under a scan root that looks like a checkout.
    fn discover(&self) -> Result<Vec<String>> {
        let mut found = BTreeSet::new();
        for root_name in &self.policy.scan_roots {
            let dir = self.root.join(root_name);
            if !dir.is_dir() {
                continue;
            }
            let read = std::fs::read_dir(&dir).map_err(|source| Error::io(&dir, source))?;
            for entry in read {
                let entry = entry.map_err(|source| Error::io(&dir, source))?;
                let path: PathBuf = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let looks_like_checkout = path.join(".git").exists()
                    || path.join("Cargo.toml").is_file()
                    || path.join("package.json").is_file();
                if !looks_like_checkout {
                    continue;
                }
                let name = match path.file_name() {
                    Some(name) => name.to_string_lossy().to_string(),
                    None => continue,
                };
                found.insert(crate::manifest::normalize_path(&format!(
                    "{root_name}/{name}"
                )));
            }
        }
        Ok(found.into_iter().collect())
    }

    fn observe(&self, id: String, relative: &str, declared: bool, exists: bool) -> RepoObservation {
        let absolute = self.root.join(relative);
        let mut notes = Vec::new();
        let mut git = None;
        let mut fingerprint_value = None;
        let mut files = std::collections::BTreeMap::new();
        let mut packages = Vec::new();

        if !exists || !absolute.is_dir() {
            notes.push("path is missing on disk".to_string());
            return RepoObservation {
                id,
                path: relative.to_string(),
                declared,
                exists: false,
                git: None,
                fingerprint: None,
                files,
                packages,
                notes,
            };
        }

        if absolute.join(".git").exists() {
            git = Some(self.git_state(&absolute, &mut notes));
        } else {
            notes.push("no Git metadata; identity relies on the content fingerprint".to_string());
        }

        match fingerprint::collect_files(&absolute, &self.policy.ignore) {
            Ok(entries) => {
                let value = fingerprint::fingerprint_of(&entries);
                files = fingerprint::digest_map(&entries);
                fingerprint_value = Some(Fingerprint {
                    algorithm: fingerprint::ALGORITHM.to_string(),
                    digest: value.digest,
                    files: value.files,
                    bytes: value.bytes,
                    content_hashed: value.content_hashed,
                    metadata_only: value.metadata_only,
                });
            }
            Err(error) => notes.push(format!("fingerprint failed: {error}")),
        }

        if absolute.join("Cargo.toml").is_file() {
            if absolute.join("rust-toolchain.toml").is_file()
                || absolute.join("rust-toolchain").is_file()
            {
                notes.push(
                    "pins a toolchain with rust-toolchain(.toml); cargo is invoked from the \
                     workspace root so rustup cannot start a toolchain download"
                        .to_string(),
                );
            }
            match self.cargo_packages(&absolute) {
                Ok(found) => packages = found,
                Err(message) => notes.push(message),
            }
        }

        RepoObservation {
            id,
            path: relative.to_string(),
            declared,
            exists: true,
            git,
            fingerprint: fingerprint_value,
            files,
            packages,
            notes,
        }
    }

    fn git_state(&self, dir: &Path, notes: &mut Vec<String>) -> GitState {
        let commit = self.git(dir, &["rev-parse", "HEAD"], notes);
        let branch = self.git(dir, &["rev-parse", "--abbrev-ref", "HEAD"], notes);
        let origin = self.git(dir, &["remote", "get-url", "origin"], notes);
        let dirty_paths = match self.git(dir, &["status", "--porcelain"], notes) {
            Some(output) if !output.is_empty() => output.lines().count() as u32,
            Some(_) => 0,
            None => 0,
        };
        GitState {
            commit,
            branch,
            origin,
            dirty_paths,
        }
    }

    fn git(&self, dir: &Path, args: &[&str], notes: &mut Vec<String>) -> Option<String> {
        let mut command = Command::new("git");
        command.arg("-C").arg(dir).args(args);
        let output = run_bounded(command, self.command_timeout);
        if output.timed_out {
            notes.push(format!("git {args:?} timed out"));
            return None;
        }
        if !output.success {
            return None;
        }
        Some(output.stdout.trim().to_string())
    }

    fn cargo_packages(&self, dir: &Path) -> std::result::Result<Vec<String>, String> {
        // Invoke cargo from the workspace root with an absolute --manifest-path. Running it with
        // the repository as the working directory would let a `rust-toolchain.toml` in that
        // repository make rustup download (or attempt to download) a different toolchain: measured
        // as a 25 s hang plus network access in kernels/hagane, kernels/monstertruck and
        // references/cadrs, all of which pin Rust 1.99.0.
        let manifest = dir.join("Cargo.toml");
        let mut command = Command::new("cargo");
        command
            .arg("metadata")
            .arg("--no-deps")
            .arg("--offline")
            .arg("--format-version")
            .arg("1")
            .arg("--manifest-path")
            .arg(&manifest)
            .current_dir(self.root)
            .env("CARGO_NET_OFFLINE", "true");
        let output = run_bounded(command, self.command_timeout);
        if output.timed_out {
            return Err("cargo metadata timed out".to_string());
        }
        if !output.success {
            let first = output.stderr.lines().next().unwrap_or("unknown error");
            return Err(format!("cargo metadata failed: {}", first.trim()));
        }
        #[derive(Deserialize)]
        struct Metadata {
            packages: Vec<Package>,
        }
        #[derive(Deserialize)]
        struct Package {
            name: String,
            version: String,
            manifest_path: PathBuf,
        }
        let metadata: Metadata = serde_json::from_str(&output.stdout)
            .map_err(|error| format!("cargo metadata returned unparseable JSON: {error}"))?;
        let mut names: Vec<String> = metadata
            .packages
            .into_iter()
            // `--manifest-path` can return every package in the containing workspace. Keep the
            // inventory scoped to this checkout so a root workspace addition is not attributed
            // to every repository scanned beneath it.
            .filter(|package| package.manifest_path.starts_with(dir))
            .map(|package| format!("{}@{}", package.name, package.version))
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|delta| delta.as_secs())
        .unwrap_or(0)
}

struct CommandOutput {
    success: bool,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

/// Run a command with a wall-clock bound, draining both pipes concurrently so a large
/// `cargo metadata` payload cannot deadlock against a full pipe buffer.
fn run_bounded(mut command: Command, timeout: Duration) -> CommandOutput {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandOutput {
                success: false,
                timed_out: false,
                stdout: String::new(),
                stderr: format!("failed to start command: {error}"),
            };
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_all(stdout));
    let stderr_reader = std::thread::spawn(move || read_all(stderr));

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                let _ = child.kill();
                break None;
            }
        }
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    CommandOutput {
        success: status.map(|s| s.success()).unwrap_or(false),
        timed_out,
        stdout: String::from_utf8_lossy(&stdout).to_string(),
        stderr: String::from_utf8_lossy(&stderr).to_string(),
    }
}

fn read_all<R: Read>(reader: Option<R>) -> Vec<u8> {
    let mut buffer = Vec::new();
    if let Some(mut reader) = reader {
        let _ = reader.read_to_end(&mut buffer);
    }
    buffer
}
