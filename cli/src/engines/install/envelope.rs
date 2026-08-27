//! The JSON contract of `grammachy engine`, spec section 5.4.
//!
//! Every verb prints exactly one envelope, a report carries the whole list the
//! Settings view draws, and the error envelope is the shared one of section
//! 5.1 with the two codes only a transfer can answer.

use serde::Serialize;

use crate::envelope::{CheckError, ErrorBody, ErrorCode, CONTRACT_VERSION};

use super::transfer::Failure;

/// What one optional component has on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Nothing of it is here.
    Absent,
    /// A `.part` file is here, so an Install resumes rather than restarts.
    Partial,
    /// The whole archive is here, unpacked, and its digest matched the pin.
    Ready,
}

/// One row of the optional engines list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineRow {
    /// The engine slug, which is also what the `engine` setting holds.
    pub slug: String,
    /// The display name, the same one the Settings dropdown draws.
    pub name: String,
    /// The upstream release this row installs.
    pub version: String,
    pub state: State,
    /// Bytes of the `.part` file, and `0` unless the state is `partial`. The
    /// shell polls `engine list` while an install runs and reads this.
    #[serde(rename = "partialBytes")]
    pub partial_bytes: u64,
    /// The pinned byte size of the archive.
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
    /// The upstream licence of the component.
    pub licence: String,
    /// Whether the component needs a Java runtime beside it, which is a
    /// package this install cannot put in place (spec section 4).
    #[serde(rename = "needsJava")]
    pub needs_java: bool,
    /// Where the component is on this machine, or `""` when it is nowhere.
    ///
    /// A row installed by this verb answers its own directory; a row the
    /// pacman package supplies answers that launcher, so the Settings view can
    /// say the component is there without claiming this verb put it there.
    pub path: String,
    /// Whether the pacman package supplies this component, so `remove` here
    /// would not take it off the machine.
    #[serde(rename = "fromPackage")]
    pub from_package: bool,
}

/// What one verb answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineReport {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    /// `list`, `install`, or `remove`, so the shell need not guess.
    pub verb: &'static str,
    /// Where the components live on this machine.
    pub directory: String,
    /// Free bytes on the file system that directory sits on.
    #[serde(rename = "freeBytes")]
    pub free_bytes: u64,
    /// Every catalogue row for `list`, and the one row acted on otherwise.
    pub engines: Vec<EngineRow>,
}

/// Exactly one of these is printed on stdout by every `engine` run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum EngineEnvelope {
    Report(EngineReport),
    Error(CheckError),
}

impl EngineEnvelope {
    pub fn report(report: EngineReport) -> Self {
        EngineEnvelope::Report(report)
    }

    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        EngineEnvelope::Error(CheckError {
            contract_version: CONTRACT_VERSION,
            error: ErrorBody {
                code,
                message: message.into(),
            },
        })
    }

    pub fn bad_arguments(message: impl Into<String>) -> Self {
        EngineEnvelope::error(ErrorCode::BadArguments, message)
    }

    /// The envelope one [`Failure`] prints.
    ///
    /// An install is a transfer with a second step: it can be refused, it can
    /// fail part way, and it can be cancelled.
    pub fn failure(failure: Failure) -> Self {
        match failure {
            Failure::BadArguments(message) => {
                EngineEnvelope::error(ErrorCode::BadArguments, message)
            }
            Failure::DownloadFailed(message) => {
                EngineEnvelope::error(ErrorCode::DownloadFailed, message)
            }
            Failure::Cancelled(message) => EngineEnvelope::error(ErrorCode::Cancelled, message),
        }
    }

    /// Exit 0 for a report, exit 1 for an error envelope.
    pub fn exit_code(&self) -> i32 {
        match self {
            EngineEnvelope::Report(_) => 0,
            EngineEnvelope::Error(_) => 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialisation cannot fail")
    }
}
