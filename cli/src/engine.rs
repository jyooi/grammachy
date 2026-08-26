//! The seam every engine adapter plugs into.
//!
//! `resolve` answers `None` for a slug that has no adapter in this build, and
//! [`crate::check::run`] turns that into the `engine_unavailable` envelope the
//! shell already handles.

use crate::args::{CheckOptions, EngineSlug};
use crate::engines::harper::Harper;
use crate::engines::languagetool::{self, LanguageTool};
use crate::engines::openai::{self, Openai};
use crate::engines::openrouter::{self, Openrouter};
use crate::envelope::Issue;

/// Why one Check did not produce Issues. Each variant maps to one error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineFailure {
    Unavailable(String),
    Timeout(String),
    Failed(String),
    /// The Check cannot run as configured, so nothing was sent. The `openai`
    /// base URL host rule of spec section 4 is the one case in v1.
    BadArguments(String),
}

/// The Issues of one Check plus what the engine charged for it.
///
/// The cost stays inside Rust: the 5.1 envelope never carries it. Only the
/// benchmark reads it, for the cost column and the `--max-cost` cap.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub issues: Vec<Issue>,
    /// `usage.cost` in USD, present only on a cloud engine that reported it.
    pub cost: Option<f64>,
}

pub trait Engine {
    /// The slug the result envelope reports.
    fn slug(&self) -> &'static str;

    /// Find the Issues in `text`.
    ///
    /// The returned Issues must be sorted by `start`, must not overlap, must
    /// carry a `fix` that differs from `original`, and index in UTF-16 code
    /// units into the exact text given (spec section 5.1).
    fn check(&self, text: &str, options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure>;

    /// The Issues and the cost of one Check. A local engine has no cost, so
    /// only a cloud adapter overrides this.
    fn answer(&self, text: &str, options: &CheckOptions) -> Result<Answer, EngineFailure> {
        self.check(text, options)
            .map(|issues| Answer { issues, cost: None })
    }
}

/// Build the adapter for one slug, or `None` while the slug has no adapter.
pub fn resolve(slug: EngineSlug) -> Option<Box<dyn Engine>> {
    match slug {
        EngineSlug::Languagetool => Some(Box::new(LanguageTool::new(
            languagetool::Config::from_env(),
        ))),
        EngineSlug::Harper => Some(Box::new(Harper::default())),
        EngineSlug::Openai => Some(Box::new(Openai::new(openai::Config::from_env()))),
        EngineSlug::Openrouter => Some(Box::new(Openrouter::new(openrouter::Config::from_env()))),
    }
}
