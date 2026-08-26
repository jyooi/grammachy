//! The JSON contract of `grammachy model`, spec section 5.3.
//!
//! Every verb prints exactly one of these on stdout. A report carries the whole
//! Models list the shell draws, so `list`, `download`, and `remove` all leave
//! the shell able to redraw without a second run. The error envelope is the
//! shared one of section 5.1, with the two codes only a download can answer.

use serde::Serialize;

use crate::envelope::{CheckError, ErrorBody, ErrorCode, CONTRACT_VERSION};
use crate::model::Failure;

/// What one catalogue model has on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Nothing of it is here.
    Absent,
    /// A `.part` file is here, so a Download resumes rather than restarts.
    Partial,
    /// The whole file is here and its digest matched the pin.
    Ready,
}

/// One row of the Models list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelRow {
    /// The catalogue name, which is also what the `openaiModel` setting holds.
    pub name: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub state: State,
    /// Bytes of the `.part` file, and `0` unless the state is `partial`. The
    /// shell polls `model list` while a download runs and reads this.
    #[serde(rename = "partialBytes")]
    pub partial_bytes: u64,
    /// The pinned byte size of the whole file.
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
    /// The weights licence, from the one table spec section 13.1 fixes.
    pub licence: String,
}

/// What one verb answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelReport {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    /// `list`, `download`, or `remove`, so the shell need not guess.
    pub verb: &'static str,
    /// Where the weights live on this machine.
    pub directory: String,
    /// Free bytes on the file system that directory sits on.
    #[serde(rename = "freeBytes")]
    pub free_bytes: u64,
    /// Every catalogue row for `list`, and the one row acted on otherwise.
    pub models: Vec<ModelRow>,
}

/// Exactly one of these is printed on stdout by every `model` run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ModelEnvelope {
    Report(ModelReport),
    Error(CheckError),
}

impl ModelEnvelope {
    pub fn report(report: ModelReport) -> Self {
        ModelEnvelope::Report(report)
    }

    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        ModelEnvelope::Error(CheckError {
            contract_version: CONTRACT_VERSION,
            error: ErrorBody {
                code,
                message: message.into(),
            },
        })
    }

    pub fn bad_arguments(message: impl Into<String>) -> Self {
        ModelEnvelope::error(ErrorCode::BadArguments, message)
    }

    /// The envelope one [`Failure`] prints.
    pub fn failure(failure: Failure) -> Self {
        match failure {
            Failure::BadArguments(message) => {
                ModelEnvelope::error(ErrorCode::BadArguments, message)
            }
            Failure::DownloadFailed(message) => {
                ModelEnvelope::error(ErrorCode::DownloadFailed, message)
            }
            Failure::Cancelled(message) => ModelEnvelope::error(ErrorCode::Cancelled, message),
        }
    }

    /// Exit 0 for a report, exit 1 for an error envelope.
    pub fn exit_code(&self) -> i32 {
        match self {
            ModelEnvelope::Report(_) => 0,
            ModelEnvelope::Error(_) => 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialisation cannot fail")
    }
}
