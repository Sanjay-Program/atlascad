use std::io;
use std::path::{Path, PathBuf};

fn backend_dir_name() -> Option<&'static str> {
    match std::env::var("MT_BOOL_SSI_BACKEND").ok().as_deref() {
        None | Some("") => None,
        Some(other) => Some(Box::leak(other.to_owned().into_boxed_str())),
    }
}

/// Returns the shared example artifact directory under `target`.
pub fn artifact_dir() -> io::Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_dir = manifest_dir.parent().unwrap_or(manifest_dir.as_path());
    let dir = match backend_dir_name() {
        None => workspace_dir.join("target").join("examples"),
        Some(backend) => workspace_dir.join("target").join(backend).join("examples"),
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the artifact path for a relative file name under `target/examples`.
pub fn artifact_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    artifact_dir().map(|dir| dir.join(path))
}

/// Resolves a command-line path.
///
/// Bare file names are placed under `target/examples`.
/// Paths with directory components and absolute paths are used as-is.
pub fn resolve_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() || path.parent().is_some_and(|parent| parent != Path::new("")) {
        Ok(path.to_path_buf())
    } else {
        artifact_path(path)
    }
}
