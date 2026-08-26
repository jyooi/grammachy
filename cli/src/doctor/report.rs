//! What `doctor` concludes from [`Facts`], and the envelope it hands the shell.
//!
//! The report is a pure function of the facts, so a test writes the machine it
//! wants and reads the exact lines back. Spec section 10 fixes what is
//! checked: the binary, LanguageTool, llama.cpp, the model file, and the two
//! transient units. Spec section 8 asks for the one-line diagnosis the
//! `engine_unavailable` card shows under its body, which is `diagnosis` here.

use serde::Serialize;

use crate::args::EngineSlug;
use crate::envelope::CONTRACT_VERSION;

use super::facts::{Facts, HardwareTier, UnitState};

/// One thing `doctor` looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Stable name for the shell, never shown to a user.
    pub id: &'static str,
    /// The display name of the piece.
    pub name: &'static str,
    pub ok: bool,
    /// One sentence saying what was found, or what is missing.
    pub detail: String,
    /// The exact command that installs the missing piece, when one exists.
    /// pacman steps stay manual: `doctor` never runs this itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
    /// The engine slugs that need this piece.
    pub engines: Vec<&'static str>,
}

impl Check {
    /// The whole finding as one line, which is what both the human report and
    /// the engine diagnosis print.
    pub fn line(&self) -> String {
        match &self.remedy {
            Some(remedy) => format!("{} Run: {remedy}", self.detail),
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
    #[serde(rename = "hardwareTier")]
    pub hardware_tier: &'static str,
    /// The ggml package that tier wants beside `llama-cpp`.
    #[serde(rename = "backendPackage")]
    pub backend_package: &'static str,
    pub checks: Vec<Check>,
}

impl Report {
    /// Read one machine for one engine.
    pub fn new(facts: &Facts, engine: EngineSlug) -> Self {
        let tier = facts.tier();
        let checks = build_checks(facts, tier);
        let ready = unchecked(engine).is_none()
            && checks
                .iter()
                .filter(|check| check.needed_by(engine))
                .all(|check| check.ok);
        let diagnosis = diagnose(&checks, facts, engine);

        Report {
            contract_version: CONTRACT_VERSION,
            engine: engine.as_str().to_string(),
            ready,
            diagnosis,
            hardware_tier: tier.as_str(),
            backend_package: tier.backend_package(),
            checks,
        }
    }

    /// The pieces that are missing, in the order they were checked.
    pub fn missing(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(|check| !check.ok)
    }

    /// Exit 0 when the chosen engine can run, exit 1 when it cannot.
    ///
    /// A piece another engine needs never fails the run, because a user who
    /// checks with LanguageTool owes nothing to llama.cpp.
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
fn build_checks(facts: &Facts, tier: HardwareTier) -> Vec<Check> {
    vec![
        binary_check(facts),
        languagetool_check(facts),
        java_check(facts),
        llama_check(facts, tier),
        model_check(facts),
        endpoint_check(facts),
        unit_check(
            "unit:languagetool",
            "LanguageTool unit",
            "grammachy-languagetool",
            facts.languagetool_unit,
            vec!["languagetool"],
        ),
        unit_check(
            "unit:llama",
            "llama.cpp unit",
            "grammachy-llama",
            facts.llama_unit,
            vec!["openai"],
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
            detail: format!("grammachy {version} at {}", path.display()),
            remedy: None,
            engines: vec!["languagetool", "openai", "harper"],
        },
        None => Check {
            id: "binary",
            name: "Grammachy CLI",
            ok: false,
            detail: format!("grammachy {version} runs, but its own path is not readable."),
            remedy: None,
            engines: vec!["languagetool", "openai", "harper"],
        },
    }
}

fn languagetool_check(facts: &Facts) -> Check {
    match &facts.languagetool_launcher {
        Some(path) => Check {
            id: "languagetool",
            name: "LanguageTool",
            ok: true,
            detail: path.display().to_string(),
            remedy: None,
            engines: vec!["languagetool"],
        },
        None => Check {
            id: "languagetool",
            name: "LanguageTool",
            ok: false,
            detail: format!(
                "LanguageTool is not installed: {} does not exist.",
                crate::engines::languagetool::unit::PACKAGE_LAUNCHER
            ),
            remedy: Some("sudo pacman -S languagetool".to_string()),
            engines: vec!["languagetool"],
        },
    }
}

fn java_check(facts: &Facts) -> Check {
    match &facts.java {
        Some(path) => Check {
            id: "java",
            name: "Java runtime",
            ok: true,
            detail: path.display().to_string(),
            remedy: None,
            engines: vec!["languagetool"],
        },
        // The launcher runs "$JAVA_HOME/bin/java" and Arch never exports
        // JAVA_HOME, so a machine with no default JVM cannot start the unit.
        None => Check {
            id: "java",
            name: "Java runtime",
            ok: false,
            detail: "No Java runtime: JAVA_HOME is not set and no default JVM is installed."
                .to_string(),
            remedy: Some("sudo pacman -S jre-openjdk".to_string()),
            engines: vec!["languagetool"],
        },
    }
}

fn llama_check(facts: &Facts, tier: HardwareTier) -> Check {
    match &facts.llama_server {
        Some(path) => Check {
            id: "llama.cpp",
            name: "llama.cpp server",
            ok: true,
            detail: path.display().to_string(),
            remedy: None,
            engines: vec!["openai"],
        },
        // The llama-cpp package carries no compute backend of its own, so the
        // tier of this machine decides the second package on the line.
        None => Check {
            id: "llama.cpp",
            name: "llama.cpp server",
            ok: false,
            detail: format!(
                "llama.cpp is not installed: {} does not exist.",
                crate::engines::openai::unit::PACKAGE_SERVER
            ),
            remedy: Some(format!(
                "sudo pacman -S llama-cpp {}",
                tier.backend_package()
            )),
            engines: vec!["openai"],
        },
    }
}

fn model_check(facts: &Facts) -> Check {
    let model = &facts.model;
    match (&facts.model_file, &facts.models_directory) {
        (Some(path), _) => Check {
            id: "model",
            name: "Model weights",
            ok: true,
            detail: path.display().to_string(),
            remedy: None,
            engines: vec!["openai"],
        },
        (None, Some(directory)) => Check {
            id: "model",
            name: "Model weights",
            ok: false,
            detail: format!("No weights for {model} in {}.", directory.display()),
            remedy: Some("grammachy setup".to_string()),
            engines: vec!["openai"],
        },
        (None, None) => Check {
            id: "model",
            name: "Model weights",
            ok: false,
            detail: "No model directory: HOME is not set.".to_string(),
            remedy: None,
            engines: vec!["openai"],
        },
    }
}

fn endpoint_check(facts: &Facts) -> Check {
    match &facts.openai_endpoint {
        Ok(address) => Check {
            id: "endpoint",
            name: "Local LLM endpoint",
            ok: true,
            detail: address.clone(),
            remedy: None,
            engines: vec!["openai"],
        },
        // Spec section 4: a base URL off this machine is bad_arguments and no
        // Check is ever sent, so it is a broken setting, not a missing piece.
        Err(message) => Check {
            id: "endpoint",
            name: "Local LLM endpoint",
            ok: false,
            detail: message.clone(),
            remedy: None,
            engines: vec!["openai"],
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
            detail: format!("{unit} is running."),
            remedy: None,
            engines,
        },
        UnitState::Stopped => Check {
            id,
            name,
            ok: true,
            detail: format!("{unit} is not running. The next Check starts it."),
            remedy: None,
            engines,
        },
        UnitState::Unknown => Check {
            id,
            name,
            ok: false,
            detail: format!("systemctl --user did not answer, so nothing can start {unit}."),
            remedy: None,
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
    match unchecked(engine) {
        Some(why) => why.to_string(),
        None => ready_line(facts, engine),
    }
}

/// Why `doctor` cannot say the cloud engine is ready.
///
/// The engine needs one piece, the key file, and no check reads it yet.
const CLOUD_UNCHECKED: &str =
    "Grammachy cannot check the cloud key yet. Put the OpenRouter key in ~/.config/grammachy/openrouter-key.";

/// Why `doctor` cannot decide one engine's readiness, when it cannot.
///
/// An engine whose pieces no check reads is never reported ready, because a
/// ready answer on no evidence is worse than no answer.
fn unchecked(engine: EngineSlug) -> Option<&'static str> {
    match engine {
        EngineSlug::Openrouter => Some(CLOUD_UNCHECKED),
        _ => None,
    }
}

/// What to say when nothing is missing.
fn ready_line(facts: &Facts, engine: EngineSlug) -> String {
    match engine {
        EngineSlug::Harper => {
            "Harper runs inside the companion binary and needs nothing installed.".to_string()
        }
        EngineSlug::Languagetool => {
            let address = &facts.languagetool_address;
            match facts.languagetool_unit {
                UnitState::Running => {
                    format!("LanguageTool is installed and its unit runs on {address}.")
                }
                _ => format!(
                    "LanguageTool is installed. The next Check starts it on {address}, which takes a moment."
                ),
            }
        }
        EngineSlug::Openai => {
            let address = facts
                .openai_endpoint
                .as_deref()
                .unwrap_or("the configured address");
            match facts.llama_unit {
                UnitState::Running => {
                    format!("llama.cpp is installed and its unit runs on {address}.")
                }
                _ => format!(
                    "llama.cpp and the weights are installed. The next Check starts the server on {address}, which takes a moment."
                ),
            }
        }
        EngineSlug::Openrouter => CLOUD_UNCHECKED.to_string(),
    }
}
