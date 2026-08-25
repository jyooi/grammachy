//! The seam every engine adapter plugs into.
//!
//! `resolve` answers `None` for a slug that has no adapter in this build, and
//! [`crate::check::run`] turns that into the `engine_unavailable` envelope the
//! shell already handles.

use crate::args::{CheckOptions, EngineSlug};
use crate::engines::harper::Harper;
use crate::engines::languagetool::{self, LanguageTool};
use crate::envelope::Issue;

/// Why one Check did not produce Issues. Each variant maps to one error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineFailure {
    Unavailable(String),
    Timeout(String),
    Failed(String),
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
}

/// Build the adapter for one slug, or `None` while the slug has no adapter.
pub fn resolve(slug: EngineSlug) -> Option<Box<dyn Engine>> {
    match slug {
        EngineSlug::Languagetool => Some(Box::new(LanguageTool::new(
            languagetool::Config::from_env(),
        ))),
        EngineSlug::Harper => Some(Box::new(Harper::default())),
        EngineSlug::Openai => None,
    }
}
