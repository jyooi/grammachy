//! The JSON contract between the CLI and the shell, spec section 5.1.

use serde::Serialize;

/// The contract version every envelope carries.
pub const CONTRACT_VERSION: u32 = 1;

/// One mistake found by a Check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    /// Start of the span in UTF-16 code units, indexed into the stdin text.
    pub start: usize,
    /// End of the span in UTF-16 code units, half open.
    pub end: usize,
    pub original: String,
    pub fix: String,
    pub reason: String,
    pub category: Category,
    #[serde(rename = "ruleId", skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Grammar,
    Spelling,
}

/// Put an Issue list into the order the contract promises.
///
/// Spec section 5.1: sorted by `start` and never overlapping, where the earlier
/// Issue wins. Every engine adapter owes the shell this, so it lives beside the
/// contract rather than inside one adapter.
pub fn sorted_disjoint(mut issues: Vec<Issue>) -> Vec<Issue> {
    // A shorter span first keeps the tighter of two Issues that start together.
    issues.sort_by_key(|issue| (issue.start, issue.end));

    let mut kept: Vec<Issue> = Vec::with_capacity(issues.len());
    for issue in issues {
        match kept.last() {
            Some(previous) if issue.start < previous.end => continue,
            _ => kept.push(issue),
        }
    }
    kept
}

/// The codes the shell knows, spec section 5.1.
///
/// Three codes a Check never answers sit beside them. `SetupFailed` belongs to
/// `grammachy setup` (spec sections 10 and 12), and `Cancelled` and
/// `DownloadFailed` belong to `grammachy model` (spec section 5.3). Each owns a
/// code of its own instead of borrowing an engine code that would tell the user
/// the wrong thing: a cancelled download is the user's own decision, and a
/// digest that did not match is not an engine failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    EmptySelection,
    TextTooLong,
    EngineUnavailable,
    EngineTimeout,
    EngineError,
    BadArguments,
    SetupFailed,
    Cancelled,
    DownloadFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckResult {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    pub engine: String,
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckError {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    pub error: ErrorBody,
}

/// The check result or error envelope printed on stdout (spec section 5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Envelope {
    Result(CheckResult),
    Error(CheckError),
}

impl Envelope {
    pub fn result(engine: impl Into<String>, elapsed_ms: u64, issues: Vec<Issue>) -> Self {
        Envelope::Result(CheckResult {
            contract_version: CONTRACT_VERSION,
            engine: engine.into(),
            elapsed_ms,
            issues,
        })
    }

    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Envelope::Error(CheckError {
            contract_version: CONTRACT_VERSION,
            error: ErrorBody {
                code,
                message: message.into(),
            },
        })
    }

    /// Exit 0 for any result, exit 1 for an error envelope.
    pub fn exit_code(&self) -> i32 {
        match self {
            Envelope::Result(_) => 0,
            Envelope::Error(_) => 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialisation cannot fail")
    }
}
