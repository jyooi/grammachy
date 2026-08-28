//! `grammachy setup` and `grammachy setup --remove`, spec section 10.
//!
//! Setup does the parts of the install that need no password, and it does them
//! idempotently: running it twice leaves exactly one binding block and exactly
//! one menu row, and `--remove` puts both files back as they were.
//!
//! Every path this module touches is injectable, and so is the one side
//! effect, `hyprctl reload`. No test writes a real configuration file or
//! reloads a real compositor.

pub mod bindings;
pub mod block;
pub mod menu;

use std::path::PathBuf;

use serde::Serialize;

use crate::envelope::{CheckError, ErrorBody, ErrorCode, CONTRACT_VERSION};
use crate::settings::StoredSettings;

/// What one step of a run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The step changed a file or fetched something.
    Changed,
    /// The step found what it wanted already in place.
    Unchanged,
    /// The step did not apply to this machine or these Settings.
    Skipped,
}

/// One line of the report a run prints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    pub name: String,
    pub state: State,
    pub detail: String,
}

impl Step {
    fn new(name: &str, state: State, detail: impl Into<String>) -> Self {
        Step {
            name: name.to_string(),
            state,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetupReport {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    /// `install` or `remove`, so the shell need not guess from the flags.
    pub mode: &'static str,
    pub steps: Vec<Step>,
}

/// Exactly one of these is printed on stdout by every `setup` run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum SetupEnvelope {
    Report(SetupReport),
    Error(CheckError),
}

impl SetupEnvelope {
    pub fn report(mode: &'static str, steps: Vec<Step>) -> Self {
        SetupEnvelope::Report(SetupReport {
            contract_version: CONTRACT_VERSION,
            mode,
            steps,
        })
    }

    pub fn error(message: impl Into<String>) -> Self {
        SetupEnvelope::Error(CheckError {
            contract_version: CONTRACT_VERSION,
            error: ErrorBody {
                code: ErrorCode::SetupFailed,
                message: message.into(),
            },
        })
    }

    /// Exit 0 for a report, exit 1 for an error envelope.
    pub fn exit_code(&self) -> i32 {
        match self {
            SetupEnvelope::Report(_) => 0,
            SetupEnvelope::Error(_) => 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialisation cannot fail")
    }
}

/// What one run works on: two paths, the keys of spec section 7, and the one
/// side effect.
pub struct Setup {
    pub bindings_path: PathBuf,
    pub menu_path: PathBuf,
    pub hotkeys: bindings::Hotkeys,
    pub reload: bindings::Reloader,
}

impl Setup {
    /// The run this machine gets, with the test seams applied.
    pub fn from_env() -> Result<Setup, String> {
        Ok(Setup {
            bindings_path: bindings::path().ok_or_else(no_home)?,
            menu_path: menu::path().ok_or_else(no_home)?,
            hotkeys: bindings::Hotkeys::resolve(&StoredSettings::load()),
            reload: bindings::reloader_from_env(),
        })
    }

    /// Hotkeys, then the menu entry.
    pub fn install(&self) -> SetupEnvelope {
        let mut steps = Vec::with_capacity(3);

        match bindings::install(&self.bindings_path, &self.hotkeys) {
            Ok(changed) => steps.push(Step::new(
                "hotkeys",
                state_of(changed),
                self.bindings_path.display().to_string(),
            )),
            Err(message) => return SetupEnvelope::error(message),
        }

        // The reload is what makes the two hotkeys live, but a machine with no
        // running compositor is a fact rather than a failure: the block is on
        // disk either way and the next Hyprland start reads it.
        steps.push(match (self.reload)() {
            Ok(()) => Step::new("reload", State::Changed, "hyprctl reload"),
            Err(message) => Step::new("reload", State::Skipped, message),
        });

        match menu::install(&self.menu_path) {
            Ok(changed) => steps.push(Step::new(
                "menu",
                state_of(changed),
                self.menu_path.display().to_string(),
            )),
            Err(message) => return SetupEnvelope::error(message),
        }

        SetupEnvelope::report("install", steps)
    }

    /// The hotkeys and the menu entry, reversed.
    pub fn remove(&self) -> SetupEnvelope {
        let mut steps = Vec::with_capacity(3);

        match bindings::remove(&self.bindings_path) {
            Ok(changed) => steps.push(Step::new(
                "hotkeys",
                state_of(changed),
                self.bindings_path.display().to_string(),
            )),
            Err(message) => return SetupEnvelope::error(message),
        }

        steps.push(match (self.reload)() {
            Ok(()) => Step::new("reload", State::Changed, "hyprctl reload"),
            Err(message) => Step::new("reload", State::Skipped, message),
        });

        match menu::remove(&self.menu_path) {
            Ok(changed) => steps.push(Step::new(
                "menu",
                state_of(changed),
                self.menu_path.display().to_string(),
            )),
            Err(message) => return SetupEnvelope::error(message),
        }

        SetupEnvelope::report("remove", steps)
    }
}

fn state_of(changed: bool) -> State {
    if changed {
        State::Changed
    } else {
        State::Unchanged
    }
}

fn no_home() -> String {
    "HOME is not set, so setup cannot find the configuration files.".to_string()
}
