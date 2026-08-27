//! `grammachy setup`, `grammachy setup --openrouter-key`, and
//! `grammachy setup --remove`, spec section 10.
//!
//! Setup does the parts of the install that need no password, and it does them
//! idempotently: running it twice leaves exactly one binding block and exactly
//! one menu row, and `--remove` puts both files back as they were. The model
//! stays, because it is a slow download and the user may well install again.
//! `--openrouter-key` is a run of its own: it writes the key file of section 4
//! from stdin and touches neither configuration file.
//!
//! Every path this module touches is injectable, and so are the two side
//! effects, `hyprctl reload` and the download. No test writes a real
//! configuration file, reloads a real compositor, fetches a real model, or
//! writes the real key.

pub mod bindings;
pub mod block;
pub mod key;
pub mod menu;

use std::path::PathBuf;

use serde::Serialize;

use crate::args::EngineSlug;
use crate::envelope::{CheckError, ErrorBody, ErrorCode, CONTRACT_VERSION};
use crate::model;

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
    /// `install`, `key`, or `remove`, so the shell need not guess from the
    /// flags.
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

/// What one run works on: four paths and the two side effects.
pub struct Setup {
    pub bindings_path: PathBuf,
    pub menu_path: PathBuf,
    pub models_directory: PathBuf,
    /// The OpenRouter key file of spec section 4.
    pub key_path: PathBuf,
    pub reload: bindings::Reloader,
    pub download: model::Downloader,
}

impl Setup {
    /// The run this machine gets, with the test seams applied.
    pub fn from_env() -> Result<Setup, String> {
        Ok(Setup {
            bindings_path: bindings::path().ok_or_else(no_home)?,
            menu_path: menu::path().ok_or_else(no_home)?,
            models_directory: model::directory().ok_or_else(no_home)?,
            key_path: key::path().ok_or_else(no_home)?,
            reload: bindings::reloader_from_env(),
            download: model::downloader(),
        })
    }

    /// Hotkeys and the menu first, then the model. A failed download still
    /// leaves the bindings and the compose row in place.
    pub fn install(&self, engine: EngineSlug, model_name: &str) -> SetupEnvelope {
        let mut steps = Vec::with_capacity(4);

        match bindings::install(&self.bindings_path) {
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

        match self.model_step(engine, model_name) {
            Ok(step) => steps.push(step),
            Err(message) => return SetupEnvelope::error(message),
        }

        SetupEnvelope::report("install", steps)
    }

    /// The hotkeys, the menu entry, and the key, reversed. The model stays.
    pub fn remove(&self) -> SetupEnvelope {
        let mut steps = Vec::with_capacity(5);

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

        // Spec section 10: the key goes with the install, because it is the
        // one thing here that a later user of this machine could spend.
        match key::remove(&self.key_path) {
            Ok(true) => steps.push(Step::new(
                "key",
                State::Changed,
                format!("{} was deleted.", self.key_path.display()),
            )),
            Ok(false) => steps.push(Step::new(
                "key",
                State::Unchanged,
                format!("There is no key at {}.", self.key_path.display()),
            )),
            Err(message) => return SetupEnvelope::error(message),
        }

        steps.push(Step::new(
            "model",
            State::Skipped,
            format!(
                "The weights in {} are kept.",
                self.models_directory.display()
            ),
        ));

        SetupEnvelope::report("remove", steps)
    }

    /// The OpenRouter key of spec section 4, read from stdin.
    ///
    /// This run writes nothing else, and the report never carries the key: a
    /// reader of the envelope learns the path and the mode and no more.
    pub fn write_key(&self, stdin: &str) -> SetupEnvelope {
        let parsed = match key::parse(stdin) {
            Ok(parsed) => parsed,
            Err(message) => return SetupEnvelope::error(message),
        };

        match key::write(&self.key_path, &parsed) {
            Ok(changed) => SetupEnvelope::report(
                "key",
                vec![Step::new(
                    "key",
                    state_of(changed),
                    format!(
                        "{} is mode 0{:o} in a mode 0{:o} directory.",
                        self.key_path.display(),
                        key::FILE_MODE,
                        key::DIRECTORY_MODE
                    ),
                )],
            ),
            Err(message) => SetupEnvelope::error(message),
        }
    }

    /// The weights, and only when the engine setting asks for them.
    fn model_step(&self, engine: EngineSlug, model_name: &str) -> Result<Step, String> {
        if engine != EngineSlug::Openai {
            return Ok(Step::new(
                "model",
                State::Skipped,
                format!("The engine is {}, which needs no weights.", engine.as_str()),
            ));
        }

        let backend = model::tier().backend_packages().join(" and ");
        let outcome = model::ensure(model_name, &self.models_directory, &self.download)?;
        Ok(match outcome {
            model::Outcome::Present(path) => Step::new(
                "model",
                State::Unchanged,
                format!("{} is already here. Backend: {backend}.", path.display()),
            ),
            model::Outcome::Downloaded(path) => Step::new(
                "model",
                State::Changed,
                format!("{} was downloaded. Backend: {backend}.", path.display()),
            ),
        })
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
