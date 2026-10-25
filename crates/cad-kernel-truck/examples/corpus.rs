//! The corpus table, as a binary: `cargo run -p cad-kernel-truck --example corpus`.
//!
//! Prints one row per fixture per kernel, then exits **non-zero when a previously passing case has
//! regressed** (or when the kernel crashed internally), so CI can gate on it. `known-failure` rows
//! and `unsupported` rows are printed and keep the gate green; an unexpected pass prints a note
//! telling the operator to promote the baseline.
//!
//! A second kernel joins the table by adding another column to `run_all` — the report is
//! structured per kernel precisely so the comparison in `docs/kernel-comparison.md` can eventually
//! be generated from measurements instead of written by hand.

use std::process::ExitCode;

use cad_contract::kernel::Kernel;
use cad_kernel_truck::TruckKernel;
use cad_kernel_truck::corpus::run_all;

fn main() -> ExitCode {
    let mut truck = TruckKernel::new();
    let mut columns: [(&str, &mut dyn Kernel); 1] = [("truck", &mut truck)];

    let report = run_all(&mut columns);
    print!("{}", report.render());
    ExitCode::from(report.exit_code())
}
