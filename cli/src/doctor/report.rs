//! What `doctor` concludes from [`Facts`], and the envelope it hands the shell.
//!
//! The report is a pure function of the facts, so a test writes the machine it
//! wants and reads the exact lines back. Spec section 10 fixes what is
//! checked: the binary, LanguageTool, and its transient unit. Spec section 8
//! asks for the one-line diagnosis the `engine_unavailable` card shows under
//! its body, which is `diagnosis` here.

use serde::Serialize;

use crate::args::EngineSlug;
use crate::envelope::CONTRACT_VERSION;

use super::deps::{self, Dependency};
use super::facts::{Facts, UnitState};

/// One thing `doctor` looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Stable name for the shell, never shown to a user.
    pub id: &'static str,
    /// The display name of the piece.
    pub name: &'static str,
    pub ok: bool,
    /// Whether a piece that is not `ok` is one the machine simply does not
    /// have yet rather than one it is missing.
    ///
    /// LanguageTool is the case HUF-237 made: it is an engine the user adds
    /// from Settings, so a fresh install that never asked for it is not a
    /// broken install. `ok` still answers the engine question, so
    /// `doctor --engine languagetool` on such a machine still refuses; only
    /// the word beside the line changes, from `missing` to `optional`.
    pub optional: bool,
    /// One sentence saying what was found, or what is missing.
    pub detail: String,
    /// The exact command that installs the missing piece, when one exists.
    /// `doctor` never runs this itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
    /// The stable word for which state this piece is in, for a check whose
    /// states the shell must tell apart. Only the `languagetool` check
    /// carries one. `detail` is prose and no contract, so nothing may read
    /// that instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<&'static str>,
    /// The engine slugs that need this piece.
    pub engines: Vec<&'static str>,
}

impl Check {
    /// The whole finding as one line, which is what both the human report and
    /// the engine diagnosis print.
    pub fn line(&self) -> String {
        match &self.remedy {
            Some(remedy) if remedy.starts_with("grammachy ") => {
                format!("{} Run: {remedy}", self.detail)
            }
            Some(remedy) => format!("{} {remedy}", self.detail),
            None => self.detail.clone(),
        }
    }

    fn needed_by(&self, engine: EngineSlug) -> bool {
        self.engines.contains(&engine.as_str())
    }
}

/// The whole `doctor` answer for one machine and one engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    /// The engine the diagnosis is about.
    pub engine: String,
    /// Whether every piece that engine needs is in place.
    pub ready: bool,
    /// The one line the `engine_unavailable` card shows under its body.
    pub diagnosis: String,
    pub checks: Vec<Check>,
    /// Every system package Grammachy leans on, spec section 10. A missing
    /// required package never moves `ready`, which answers the engine
    /// question alone; the setup card is what refuses a bootstrap without one.
    pub dependencies: Vec<Dependency>,
}

impl Report {
    /// Read one machine for one engine.
    pub fn new(facts: &Facts, engine: EngineSlug) -> Self {
        let checks = build_checks(facts);
        let ready = checks
            .iter()
            .filter(|check| check.needed_by(engine))
            .all(|check| check.ok);
        let diagnosis = diagnose(&checks, facts, engine);

        Report {
            contract_version: CONTRACT_VERSION,
            engine: engine.as_str().to_string(),
            ready,
            diagnosis,
            checks,
            dependencies: deps::table(facts),
        }
    }

    /// The pieces that are missing, in the order they were checked.
    pub fn missing(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(|check| !check.ok)
    }

    /// The checks that carry a runnable command, missing or merely improvable.
    ///
    /// The manual-step footer reads this rather than [`Report::missing`],
    /// because an advisory line prints a command the user may still run.
    /// A system-package hint is not a command.
    pub fn commanded(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(|check| {
            check
                .remedy
                .as_deref()
                .is_some_and(|remedy| remedy.starts_with("grammachy "))
        })
    }

    /// The packages that are not on this machine, in table order.
    pub fn absent_dependencies(&self) -> impl Iterator<Item = &Dependency> {
        self.dependencies
            .iter()
            .filter(|dependency| !dependency.present)
    }

    /// Exit 0 when the chosen engine can run, exit 1 when it cannot.
    ///
    /// A piece another engine needs never fails the run, because a user who
    /// checks with Harper owes nothing to LanguageTool.
    pub fn exit_code(&self) -> i32 {
        if self.ready {
            0
        } else {
            1
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("report serialisation cannot fail")
    }
}

/// Every piece spec section 10 names, in the order the report prints them.
fn build_checks(facts: &Facts) -> Vec<Check> {
    vec![
        binary_check(facts),
        languagetool_check(facts),
        java_check(facts),
        unit_check(
            "unit:languagetool",
            "LanguageTool unit",
            "grammachy-languagetool",
            facts.languagetool_unit,
            vec!["languagetool"],
        ),
    ]
}

fn binary_check(facts: &Facts) -> Check {
    let version = &facts.version;
    match &facts.binary {
        Some(path) => Check {
            id: "binary",
            name: "Grammachy CLI",
            ok: true,
            optional: false,
            detail: format!("grammachy {version} at {}", path.display()),
            remedy: None,
            state: None,
            engines: vec!["languagetool", "harper"],
        },
        None => Check {
            id: "binary",
            name: "Grammachy CLI",
            ok: false,
            optional: false,
            detail: format!("grammachy {version} runs, but its own path is not readable."),
            remedy: None,
            state: None,
            engines: vec!["languagetool", "harper"],
        },
    }
}

/// LanguageTool, spec section 4 and HUF-237.
///
/// It is an opt-in component, so a machine that does not have it is not a
/// broken install: the line says so and names the verb that adds it without a
/// password. The pacman package is the alternative, and the report says which
/// of the two answered, because only the installed tree is one
/// `grammachy engine remove languagetool` can take away again.
///
/// The state word is the contract the Settings row reads: `detail` is prose
/// and nothing may parse it.
fn languagetool_check(facts: &Facts) -> Check {
    match (&facts.languagetool_tree, &facts.languagetool_launcher) {
        (Some(path), _) => Check {
            id: "languagetool",
            name: "LanguageTool",
            ok: true,
            optional: false,
            detail: path.display().to_string(),
            remedy: None,
            state: Some(LANGUAGETOOL_INSTALLED),
            engines: vec!["languagetool"],
        },
        // The package is a LanguageTool this project did not put there, so the
        // line says where it came from: Remove in Settings would not take it
        // off the machine.
        (None, Some(path)) => Check {
            id: "languagetool",
            name: "LanguageTool",
            ok: true,
            optional: false,
            detail: format!("{} from the languagetool package.", path.display()),
            remedy: None,
            state: Some(LANGUAGETOOL_PACKAGE),
            engines: vec!["languagetool"],
        },
        (None, None) => Check {
            id: "languagetool",
            name: "LanguageTool",
            ok: false,
            // Nobody asked for this engine, so nothing about this machine is
            // wrong. Only a Check on `languagetool` turns the line into a
            // refusal, which `ok` still does.
            optional: true,
            detail: "LanguageTool is optional and is not installed. Add it in Settings, Engines."
                .to_string(),
            // No sudo: the install writes one directory under HOME.
            remedy: Some(LANGUAGETOOL_INSTALL_COMMAND.to_string()),
            state: Some(LANGUAGETOOL_ABSENT),
            engines: vec!["languagetool"],
        },
    }
}

/// The state words the `languagetool` check carries, one per route onto the
/// machine.
///
/// `docs/doctor.md` documents them and `cli/tests/overlay_engines.rs` holds
/// them, because `detail` is prose and no contract: a reader that needs to
/// tell the two routes apart reads this word and never that sentence.
pub const LANGUAGETOOL_INSTALLED: &str = "installed";
pub const LANGUAGETOOL_PACKAGE: &str = "package";
pub const LANGUAGETOOL_ABSENT: &str = "absent";

/// The one command that adds LanguageTool, named by every line that offers it.
pub const LANGUAGETOOL_INSTALL_COMMAND: &str = "grammachy engine install languagetool";

/// The Java runtime, which only LanguageTool needs.
///
/// A machine that has no LanguageTool has no use for Java either, so a missing
/// runtime there is optional for the same reason the component is. Once
/// LanguageTool is on the machine a missing runtime is a real fault, because
/// the server cannot start without one.
fn java_check(facts: &Facts) -> Check {
    match &facts.java {
        Some(path) => Check {
            id: "java",
            name: "Java runtime",
            ok: true,
            optional: false,
            detail: path.display().to_string(),
            remedy: None,
            state: None,
            engines: vec!["languagetool"],
        },
        // The launcher runs "$JAVA_HOME/bin/java" and Arch never exports
        // JAVA_HOME, so a machine with no default JVM cannot start the unit.
        None => Check {
            id: "java",
            name: "Java runtime",
            ok: false,
            optional: facts.languagetool().is_none(),
            detail: "No Java runtime: JAVA_HOME is not set and no default JVM is installed."
                .to_string(),
            remedy: Some(deps::install_hint(&["jre-openjdk"])),
            state: None,
            engines: vec!["languagetool"],
        },
    }
}

/// A stopped unit is not a fault: the next Check starts it (spec section 4).
fn unit_check(
    id: &'static str,
    name: &'static str,
    unit: &'static str,
    state: UnitState,
    engines: Vec<&'static str>,
) -> Check {
    match state {
        UnitState::Running => Check {
            id,
            name,
            ok: true,
            optional: false,
            detail: format!("{unit} is running."),
            remedy: None,
            state: None,
            engines,
        },
        UnitState::Stopped => Check {
            id,
            name,
            ok: true,
            optional: false,
            detail: format!("{unit} is not running. The next Check starts it."),
            remedy: None,
            state: None,
            engines,
        },
        UnitState::Unknown => Check {
            id,
            name,
            ok: false,
            optional: false,
            detail: format!("systemctl --user did not answer, so nothing can start {unit}."),
            remedy: None,
            state: None,
            engines,
        },
    }
}

/// The one line the `engine_unavailable` card shows under its body.
fn diagnose(checks: &[Check], facts: &Facts, engine: EngineSlug) -> String {
    if let Some(failed) = checks
        .iter()
        .filter(|check| check.needed_by(engine))
        .find(|check| !check.ok)
    {
        return failed.line();
    }
    ready_line(facts, engine)
}

/// What to say when nothing is missing.
fn ready_line(facts: &Facts, engine: EngineSlug) -> String {
    match engine {
        EngineSlug::Harper => {
            "Harper runs inside the companion binary and needs nothing installed.".to_string()
        }
        EngineSlug::Languagetool => {
            match (facts.languagetool_unit, &facts.languagetool_address) {
                (UnitState::Running, Some(address)) => {
                    format!("LanguageTool is installed and its unit runs on {address}.")
                }
                (UnitState::Running, None) => {
                    "LanguageTool is installed and its unit is opening its loopback port.".to_string()
                }
                _ => "LanguageTool is installed. The next Check starts it on a private loopback port, which takes a moment.".to_string(),
            }
        }
    }
}
