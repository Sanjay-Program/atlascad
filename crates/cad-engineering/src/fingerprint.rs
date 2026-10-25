//! Deterministic content fingerprints for repository change detection.
//!
//! # Algorithm (documented so anyone can reproduce a digest)
//!
//! * Walk the repository depth-first, **never following symlinks**, skipping any directory whose
//!   name appears in `ignore.directories` (build output, `.git`, caches) and any file whose name
//!   ends with a configured suffix.
//! * Each file contributes `"<relative/path>\0<digest>\n"` to an FNV-1a 64-bit stream, in sorted
//!   path order, so the result is independent of directory iteration order.
//! * A file's digest is FNV-1a over its bytes when it is at most `ignore.max_hash_bytes`; larger
//!   files contribute `"meta:<len>:<mtime_secs>"` instead, which is recorded in the fingerprint's
//!   `metadata_only` count. This bounds scan cost on large binary assets and is an honest
//!   limitation: a same-size, same-mtime edit to such a file is not detected.
//!
//! FNV-1a is chosen for reproducibility across Rust versions (unlike `DefaultHasher`, which is
//! allowed to change) and for speed. It is **not** a cryptographic hash and must not be used for
//! integrity or security decisions.

use crate::model::IgnorePolicy;
use crate::{Error, Result};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ALGORITHM: &str = "fnv1a64-pathv1";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const READ_CHUNK: usize = 64 * 1024;

fn update(state: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *state ^= u64::from(*byte);
        *state = state.wrapping_mul(FNV_PRIME);
    }
}

/// Hex digest of a sequence of strings, each terminated by a NUL. Stable across runs and versions.
pub fn digest_hex(parts: &[&str]) -> String {
    let mut state = FNV_OFFSET;
    for part in parts {
        update(&mut state, part.as_bytes());
        update(&mut state, &[0]);
    }
    format!("{state:016x}")
}

/// One file's contribution to a repository fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Forward-slash relative path; identical on every platform.
    pub relative: String,
    pub digest: String,
    pub bytes: u64,
    /// False when the file was too large and only size + mtime were hashed.
    pub content_hashed: bool,
}

/// Collect every tracked file under `root`, sorted by relative path.
pub fn collect_files(root: &Path, policy: &IgnorePolicy) -> Result<Vec<FileEntry>> {
    let mut entries = Vec::new();
    walk(root, root, policy, &mut entries)?;
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

fn walk(root: &Path, dir: &Path, policy: &IgnorePolicy, out: &mut Vec<FileEntry>) -> Result<()> {
    let read = fs::read_dir(dir).map_err(|source| Error::io(dir, source))?;
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|source| Error::io(dir, source))?;
        children.push(entry.path());
    }
    children.sort();

    for path in children {
        let metadata = fs::symlink_metadata(&path).map_err(|source| Error::io(&path, source))?;
        // Symlinks are skipped, never followed: a scan must not escape the workspace.
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if metadata.is_dir() {
            if policy.directories.iter().any(|skip| skip == &name) {
                continue;
            }
            walk(root, &path, policy, out)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if policy
            .suffixes
            .iter()
            .any(|suffix| name.ends_with(suffix.as_str()))
        {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::Message(format!("path {path:?} is outside {root:?}")))?
            .to_string_lossy()
            .replace('\\', "/");
        let (digest, content_hashed) = hash_file(&path, &metadata, policy.max_hash_bytes)?;
        out.push(FileEntry {
            relative,
            digest,
            bytes: metadata.len(),
            content_hashed,
        });
    }
    Ok(())
}

fn hash_file(path: &Path, metadata: &fs::Metadata, max_hash_bytes: u64) -> Result<(String, bool)> {
    if metadata.len() > max_hash_bytes {
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|delta| delta.as_secs())
            .unwrap_or(0);
        let marker = format!("meta:{}:{mtime}", metadata.len());
        return Ok((digest_hex(&[marker.as_str()]), false));
    }

    let mut file = fs::File::open(path).map_err(|source| Error::io(path, source))?;
    let mut state = FNV_OFFSET;
    let mut buffer = vec![0u8; READ_CHUNK];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| Error::io(path, source))?;
        if read == 0 {
            break;
        }
        update(&mut state, &buffer[..read]);
    }
    Ok((format!("{state:016x}"), true))
}

/// Aggregate fingerprint of a file set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintValue {
    pub digest: String,
    pub files: u64,
    pub bytes: u64,
    pub content_hashed: u64,
    pub metadata_only: u64,
}

pub fn fingerprint_of(files: &[FileEntry]) -> FingerprintValue {
    let mut state = FNV_OFFSET;
    let mut bytes = 0u64;
    let mut content_hashed = 0u64;
    let mut metadata_only = 0u64;

    for file in files {
        update(&mut state, file.relative.as_bytes());
        update(&mut state, &[0]);
        update(&mut state, file.digest.as_bytes());
        update(&mut state, b"\n");
        bytes += file.bytes;
        if file.content_hashed {
            content_hashed += 1;
        } else {
            metadata_only += 1;
        }
    }

    FingerprintValue {
        digest: format!("{state:016x}"),
        files: files.len() as u64,
        bytes,
        content_hashed,
        metadata_only,
    }
}

/// Map of relative path → digest, as persisted in the scan state.
pub fn digest_map(files: &[FileEntry]) -> std::collections::BTreeMap<String, String> {
    files
        .iter()
        .map(|file| (file.relative.clone(), file.digest.clone()))
        .collect()
}
