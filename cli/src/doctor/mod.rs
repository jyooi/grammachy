//! `grammachy doctor`, spec sections 4, 8, 10, and 12.
//!
//! `doctor` looks at the binary, LanguageTool, and its transient unit, and
//! prints one line per piece. A missing engine piece names the command that
//! adds it. A missing system package names the package for Omarchy Install.
//! Nothing here installs anything (spec section 10). [`deps`] is that table.
//!
//! The same run also answers the one-line diagnosis the `engine_unavailable`
//! card of spec section 8 shows under its body. `--json` prints the whole
//! report as one envelope, so the shell reads that line instead of parsing
//! text. `docs/doctor.md` documents the envelope.
//!
//! Detection is injectable: [`facts::Facts`] is a plain value, the report is a
//! pure function of it, and only [`facts::Facts::collect`] reads the machine.

pub mod deps;
pub mod facts;
pub mod render;
pub mod report;

use crate::args::EngineSlug;

pub use deps::Dependency;
pub use facts::{Facts, UnitState};
pub use report::{Check, Report};

/// What one `doctor` run prints and exits with.
pub struct DoctorOutput {
    pub text: String,
    pub exit_code: i32,
}

/// Read the machine and render the answer.
pub fn run(facts: &Facts, engine: EngineSlug, json: bool) -> DoctorOutput {
    let report = Report::new(facts, engine);
    let text = if json {
        report.to_json()
    } else {
        render::to_text(&report)
    };

    DoctorOutput {
        text,
        exit_code: report.exit_code(),
    }
}
