//! Shared runner for the Python-based solver adapters.
//!
//! Pandapower and Cantera are libraries rather than command-line solvers, so their adapters run a
//! short, fixed script in a separate interpreter and parse the JSON it prints. Keeping the
//! interpreter discovery, timeout and output handling here means each adapter only owns its
//! benchmark and its closed-form check.
//!
//! The interpreter is discovered from `ATLAS_PYTHON`, then `python3` and `python` on `PATH`.
//! Nothing is installed by this crate: a missing module is reported as such.

use crate::process::output_with_timeout;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Find the interpreter: `ATLAS_PYTHON` first, then `python3`, then `python` on `PATH`.
#[must_use]
pub fn discover_python() -> Option<PathBuf> {
    if let Ok(configured) = env::var("ATLAS_PYTHON") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = env::var_os("PATH")?;
    for name in ["python3", "python"] {
        let found = env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file());
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Run `code` in the interpreter and return its stdout.
pub(crate) fn run_code(code: &str, timeout: Duration) -> Result<String, String> {
    let python = discover_python()
        .ok_or("Python is not installed: put python3 on PATH or set ATLAS_PYTHON")?;
    let mut command = Command::new(python);
    command.arg("-c").arg(code);
    let outcome = output_with_timeout(command, timeout)?;
    if !outcome.status.success() {
        return Err(format!(
            "the Python solver failed with {}: {}",
            outcome.status,
            String::from_utf8_lossy(&outcome.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&outcome.stdout).to_string())
}

/// Run `code`, which must print one JSON object, and parse the last such line.
pub(crate) fn run_json(
    code: &str,
    timeout: Duration,
) -> Result<(serde_json::Value, String), String> {
    let stdout = run_code(code, timeout)?;
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.trim_start().starts_with('{'))
        .ok_or("the Python solver printed no JSON result")?;
    let value = serde_json::from_str(line.trim())
        .map_err(|error| format!("the Python solver printed unreadable JSON: {error}"))?;
    Ok((value, stdout))
}

/// Version reported by an importable module, or an error naming the module.
///
/// The package metadata is preferred because some distributions (OpenSeesPy) do not expose
/// `__version__`; the module attribute is the fallback.
pub(crate) fn module_version(module: &str, timeout: Duration) -> Result<String, String> {
    module_version_or_attribute(module, "__version__", timeout)
}

/// Version reported by an importable module, falling back to `attribute`.
///
/// Some distributions carry no package metadata and no `__version__`, but do export the version
/// under their own name: pythonocc-core ships `OCC.VERSION` and nothing importlib can read.
pub(crate) fn module_version_or_attribute(
    module: &str,
    attribute: &str,
    timeout: Duration,
) -> Result<String, String> {
    let code = format!(
        "import importlib.metadata as md\n\
         import {module}\n\
         try:\n\
         \x20   print(md.version('{module}'))\n\
         except md.PackageNotFoundError:\n\
         \x20   print(getattr({module}, '{attribute}', 'unknown'))"
    );
    let stdout = run_code(&code, timeout)
        .map_err(|error| format!("`{module}` is not importable: {error}"))?;
    let version = stdout.trim().to_string();
    if version.is_empty() {
        return Err(format!("`{module}` reported no version"));
    }
    Ok(version)
}

/// Read a required floating-point field from a solver's JSON object.
pub(crate) fn number(value: &serde_json::Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("the solver output has no `{key}` number"))
}

/// Read a required boolean field from a solver's JSON object.
pub(crate) fn boolean(value: &serde_json::Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("the solver output has no `{key}` flag"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interpreter_that_exists_is_found() {
        // Any developer or CI machine running the tests has a Rust toolchain but not necessarily
        // Python; both outcomes are valid, so only a found path is checked.
        if let Some(python) = discover_python() {
            assert!(python.is_file(), "{} is not a file", python.display());
        }
    }

    #[test]
    fn json_is_read_from_the_last_printed_object() -> Result<(), Box<dyn std::error::Error>> {
        if discover_python().is_none() {
            eprintln!("SKIP: no Python interpreter is installed here");
            return Ok(());
        }
        let (value, stdout) = run_json(
            "print('noise')\nprint('{\"a\": 2.5}')\n",
            Duration::from_secs(30),
        )
        .map_err(std::io::Error::other)?;
        assert_eq!(number(&value, "a")?, 2.5);
        assert!(stdout.contains("noise"));
        assert!(run_json("print('nothing')", Duration::from_secs(30)).is_err());
        Ok(())
    }
}
