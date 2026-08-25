//! The human form of a [`Report`].
//!
//! One line per piece, so a missing piece is one line that already carries the
//! exact command to fix it (spec section 10). Nothing here runs a command:
//! pacman steps stay manual and `doctor` installs nothing.

use super::report::Report;

/// Width of the status column.
const STATUS_WIDTH: usize = 9;

/// Width of the piece name column.
const NAME_WIDTH: usize = 20;

pub fn to_text(report: &Report) -> String {
    let mut out = String::new();

    out.push_str("Grammachy doctor\n\n");
    for check in &report.checks {
        let status = if check.ok { "ok" } else { "missing" };
        out.push_str(&format!(
            "  {status:<STATUS_WIDTH$}{:<NAME_WIDTH$}{}\n",
            check.name,
            check.line()
        ));
    }

    let tier_name = report.hardware_tier;
    out.push_str(&format!(
        "\nHardware tier {tier_name}, so llama.cpp wants {}.\n",
        report.backend_package
    ));

    let engine = &report.engine;
    let state = if report.ready {
        "is ready"
    } else {
        "cannot run"
    };
    out.push_str(&format!(
        "Engine {engine} {state}.\n  {}\n",
        report.diagnosis
    ));

    if report.missing().next().is_some() {
        out.push_str("\nRun the commands above yourself. Doctor installs nothing.\n");
    }
    out
}
