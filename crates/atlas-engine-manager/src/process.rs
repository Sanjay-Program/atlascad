//! Shared helpers for the process-isolated solver adapters.
//!
//! Each adapter runs a real binary with a timeout and digests its inputs plus outputs, so the
//! mechanics live here instead of being copied per engine.

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Wait for `child` to finish, killing it when `timeout` elapses.
///
/// The child must have been spawned with piped stdout and stderr so `wait_with_output` can collect
/// them. Error messages are engine-neutral; callers prefix their own solver name where useful.
pub(crate) fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<Output, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| format!("cannot read the solver output: {error}"));
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("the solver exceeded {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(format!("cannot wait for the solver: {error}")),
        }
    }
}

/// Run `command` to completion within `timeout`, collecting stdout and stderr.
pub(crate) fn output_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<Output, String> {
    let child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start the solver: {error}"))?;
    wait_with_timeout(child, timeout)
}

/// FNV-1a (64-bit) over the given byte slices, for a stable local content digest.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `fnv1a64-<hex>` digest over the given parts.
pub(crate) fn provenance(parts: &[&[u8]]) -> String {
    let joined: Vec<u8> = parts.concat();
    format!("fnv1a64-{:016x}", fnv1a64(&joined))
}
