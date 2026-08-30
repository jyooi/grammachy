//! The human form of a [`Report`].
//!
//! One line per piece, so a missing piece is one line that already carries the
//! exact command to fix it (spec section 10). Nothing here runs a command.
//! `doctor` installs nothing. A missing system package names Omarchy Install.
//!
//! A piece the machine simply does not have yet reads `optional` rather than
//! `missing` (HUF-237). LanguageTool is the case: a fresh install never fetched
//! it, and calling that missing would tell the reader something is broken.

use super::report::Report;

/// Width of the status column.
const STATUS_WIDTH: usize = 9;

/// Width of the piece name column.
const NAME_WIDTH: usize = 20;

pub fn to_text(report: &Report) -> String {
    let mut out = String::new();

    out.push_str("Grammachy doctor\n\n");
    for check in &report.checks {
        let status = if check.ok {
            "ok"
        } else if check.optional {
            "optional"
        } else {
            "missing"
        };
        out.push_str(&format!(
            "  {status:<STATUS_WIDTH$}{:<NAME_WIDTH$}{}\n",
            check.name,
            check.line()
        ));
    }

    out.push_str("\nDependencies\n\n");
    for dependency in &report.dependencies {
        let status = if dependency.present {
            "ok"
        } else if dependency.required {
            "missing"
        } else {
            "optional"
        };
        out.push_str(&format!(
            "  {status:<STATUS_WIDTH$}{:<NAME_WIDTH$}{}\n",
            dependency.package,
            dependency.line()
        ));
    }
    out.push('\n');

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

    if report.commanded().next().is_some() {
        out.push_str("\nRun the commands above yourself. Doctor installs nothing.\n");
    } else if report.absent_dependencies().next().is_some() {
        out.push_str(
            "\nAdd the named packages through Omarchy Install. Doctor installs nothing.\n",
        );
    }
    out
}
