//! Real ngspice (`ngspice`) process adapter.
//!
//! This module does not guess at results: it writes a deterministic RC step-response netlist, runs
//! the installed `ngspice` binary on it in batch mode, parses the transient waveform ngspice wrote,
//! and compares the capacitor voltage with `1 - exp(-t/RC)`. The circuit is linear, so the only
//! discretisation error is the transient integrator's; the comparison tests the process, the
//! netlist and the parser rather than model accuracy. If `ngspice` is absent the caller gets an
//! error and no metrics.
//!
//! The executable is discovered from `ATLAS_NGSPICE`, then `ngspice` on `PATH`. A rootless
//! installation on Linux is available from conda-forge (`micromamba install -c conda-forge
//! ngspice`) or from the distribution package manager.

use crate::process::{output_with_timeout, provenance};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bench circuit: a 1 V step through a 1 kOhm / 1 uF low-pass filter, so `tau` is 1 ms.
const RESISTANCE_OHMS: f64 = 1_000.0;
const CAPACITANCE_FARADS: f64 = 1e-6;
const STEP_VOLTS: f64 = 1.0;
const STOP_TIME_S: f64 = 5e-3;
const MAX_STEP_S: f64 = 10e-6;
/// Times at which the measured waveform is compared with the closed-form step response.
const PROBE_TIMES_S: [f64; 6] = [1e-4, 5e-4, 1e-3, 2e-3, 3e-3, 5e-3];

/// Verified result of one real `ngspice` run.
#[derive(Debug, Clone, Serialize)]
pub struct NgspiceBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub time_constant_ms: f64,
    pub timepoints: usize,
    pub probe_times_ms: Vec<f64>,
    pub measured_volts: Vec<f64>,
    pub analytic_volts: Vec<f64>,
    /// Largest `|measured - analytic| / analytic` across the probe times, in percent.
    pub max_error_percent: f64,
    /// `1 - max_error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the netlist and the waveform ngspice wrote.
    pub provenance_hash: String,
}

/// Find the solver binary: `ATLAS_NGSPICE` first, then `ngspice` on `PATH`.
#[must_use]
pub fn discover_executable() -> Option<PathBuf> {
    if let Ok(configured) = env::var("ATLAS_NGSPICE") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join("ngspice"))
        .find(|candidate| candidate.is_file())
}

/// Detected `ngspice` version, or an error when the solver is absent or does not answer.
pub fn probe_version() -> Result<String, String> {
    let executable = discover_executable()
        .ok_or("ngspice is not installed: put ngspice on PATH or set ATLAS_NGSPICE")?;
    solver_version(&executable, Duration::from_secs(10))
}

/// Run the RC step-response benchmark through the installed solver.
///
/// Returns an error with the solver's log when it does not start, times out or writes no waveform.
pub fn run_rc_step_benchmark(timeout: Duration) -> Result<NgspiceBenchmark, String> {
    let executable = discover_executable()
        .ok_or("ngspice is not installed: put ngspice on PATH or set ATLAS_NGSPICE")?;
    let version = solver_version(&executable, timeout)?;
    let netlist = rc_netlist();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-ngspice-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;

    let started = Instant::now();
    let result = run_in_workdir(&executable, &workdir, &netlist, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let parsed = result.and_then(|()| {
        let waveform = fs::read_to_string(workdir.join("rc.dat")).map_err(|error| {
            let log = fs::read_to_string(workdir.join("rc.log")).unwrap_or_default();
            format!(
                "ngspice wrote no waveform: {error}; log: {}",
                log.lines()
                    .rev()
                    .take(6)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })?;
        parse_waveform(&waveform)
            .ok_or_else(|| "ngspice waveform has no numeric samples".to_string())
    });
    let output = fs::read(workdir.join("rc.dat")).unwrap_or_default();
    let _ = fs::remove_dir_all(&workdir);
    let waveform = parsed?;

    let time_constant = RESISTANCE_OHMS * CAPACITANCE_FARADS;
    let probe_times_ms: Vec<f64> = PROBE_TIMES_S.iter().map(|t| t * 1e3).collect();
    let analytic_volts: Vec<f64> = PROBE_TIMES_S.iter().map(|t| analytic_volts(*t)).collect();
    let measured_volts: Vec<f64> = PROBE_TIMES_S
        .iter()
        .map(|t| interpolate(&waveform, *t))
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(|| "the waveform does not cover every probe time".to_string())?;

    let max_error_percent = measured_volts
        .iter()
        .zip(&analytic_volts)
        .map(|(measured, analytic)| (measured - analytic).abs() / analytic * 100.0)
        .fold(0.0_f64, f64::max);
    let provenance_hash = provenance(&[netlist.as_bytes(), output.as_slice()]);

    Ok(NgspiceBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        time_constant_ms: time_constant * 1e3,
        timepoints: waveform.len(),
        probe_times_ms,
        measured_volts,
        analytic_volts,
        max_error_percent,
        accuracy_score: (1.0 - max_error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash,
    })
}

fn run_in_workdir(
    executable: &PathBuf,
    workdir: &PathBuf,
    netlist: &str,
    timeout: Duration,
) -> Result<(), String> {
    fs::write(workdir.join("rc.cir"), netlist)
        .map_err(|error| format!("cannot write the solver netlist: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("-b")
        .arg("-o")
        .arg("rc.log")
        .arg("rc.cir")
        .current_dir(workdir);
    let outcome = output_with_timeout(command, Duration::from_secs(2) + timeout)?;
    if !outcome.status.success() {
        return Err(format!(
            "ngspice failed with {}: {} | {}",
            outcome.status,
            String::from_utf8_lossy(&outcome.stderr).trim(),
            String::from_utf8_lossy(&outcome.stdout).trim()
        ));
    }
    Ok(())
}

fn solver_version(executable: &PathBuf, timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(executable);
    command.arg("--version");
    let outcome = output_with_timeout(command, timeout)?;
    let text = String::from_utf8_lossy(&outcome.stdout);
    // ngspice prints e.g. "** ngspice-41 : Circuit level simulation program".
    let version = text
        .split_whitespace()
        .find_map(|token| token.strip_prefix("ngspice-"))
        .map(|token| token.trim_end_matches(':').to_string())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| format!("cannot read the ngspice version from: {}", text.trim()))?;
    Ok(version)
}

/// Parse `wrdata` output: a `time v(out)` header followed by two numeric columns. Frames are
/// single-scale, so every line is one `(time, volts)` sample.
fn parse_waveform(dat: &str) -> Option<Vec<(f64, f64)>> {
    let samples: Vec<(f64, f64)> = dat
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let time = columns.next()?.parse::<f64>().ok()?;
            let volts = columns.next()?.parse::<f64>().ok()?;
            Some((time, volts))
        })
        .collect();
    (!samples.is_empty()).then_some(samples)
}

/// Linear interpolation between the solver's own timepoints; `None` outside the simulated range.
fn interpolate(samples: &[(f64, f64)], at: f64) -> Option<f64> {
    let last = samples.last()?;
    if at > last.0 {
        return None;
    }
    let mut previous = *samples.first()?;
    for &current in samples {
        if current.0 >= at {
            if current.0 <= previous.0 {
                return Some(current.1);
            }
            let fraction = (at - previous.0) / (current.0 - previous.0);
            return Some(previous.1 + fraction * (current.1 - previous.1));
        }
        previous = current;
    }
    Some(last.1)
}

fn analytic_volts(time_s: f64) -> f64 {
    let tau = RESISTANCE_OHMS * CAPACITANCE_FARADS;
    STEP_VOLTS * (1.0 - (-time_s / tau).exp())
}

fn rc_netlist() -> String {
    format!(
        "* AtlasCAD ngspice RC step-response benchmark\n\
         V1 in 0 DC 0 PULSE(0 {STEP_VOLTS} 0 1n 1n 10 20)\n\
         R1 in out {RESISTANCE_OHMS}\n\
         C1 out 0 {CAPACITANCE_FARADS}\n\
         .tran {}u {}m\n\
         .control\n\
         run\n\
         set wr_singlescale\n\
         set wr_vecnames\n\
         option numdgt=10\n\
         wrdata rc.dat v(out)\n\
         .endc\n\
         .end\n",
        MAX_STEP_S * 1e6,
        STOP_TIME_S * 1e3
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_runs_when_ngspice_is_installed() -> Result<(), Box<dyn std::error::Error>> {
        if discover_executable().is_none() {
            eprintln!("SKIP: ngspice is not installed in this environment");
            return Ok(());
        }
        let result =
            run_rc_step_benchmark(Duration::from_secs(60)).map_err(std::io::Error::other)?;
        assert!(!result.solver_version.is_empty());
        assert_eq!(result.probe_times_ms.len(), PROBE_TIMES_S.len());
        assert_eq!(result.measured_volts.len(), PROBE_TIMES_S.len());
        assert!(
            result.timepoints > 100,
            "only {} samples",
            result.timepoints
        );
        assert!(
            (result.time_constant_ms - 1.0).abs() < 1e-9,
            "tau = {} ms",
            result.time_constant_ms
        );
        assert!(
            result.max_error_percent < 0.1,
            "solver deviates from 1 - exp(-t/RC) by {:.4}%",
            result.max_error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }

    #[test]
    fn parser_and_interpolation_read_the_waveform() -> Result<(), Box<dyn std::error::Error>> {
        let dat = " time              v(out)           \n\
                    0.0000000000e+00  0.0000000000e+00 \n\
                    1.0000000000e-03  6.3212055883e-01 \n\
                    2.0000000000e-03  8.6466471676e-01 \n";
        let samples = parse_waveform(dat).ok_or("the wrdata waveform was not parsed")?;
        assert_eq!(samples.len(), 3);
        let middle = interpolate(&samples, 1.5e-3).ok_or("no interpolation")?;
        assert!((middle - (0.63212055883 + 0.86466471676) / 2.0).abs() < 1e-9);
        assert!(interpolate(&samples, 3e-3).is_none());
        Ok(())
    }
}
