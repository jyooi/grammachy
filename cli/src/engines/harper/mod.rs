//! The zero dependency engine: `harper-core` 2.8, in process.
//!
//! Spec section 4. Nothing is installed and nothing is started, so the whole
//! cost of the engine is the curated dictionary and rule set that
//! [`lint`] builds. That cost is paid inside [`Harper::check`] and nowhere
//! else, which keeps the default LanguageTool path free of it.
//!
//! Harper ignores the Native language setting.

pub mod lints;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use harper_core::linting::{LintGroup, Linter};
use harper_core::parsers::PlainEnglish;
use harper_core::spell::FstDictionary;
use harper_core::{Dialect, Document};

use crate::args::CheckOptions;
use crate::engine::{Engine, EngineFailure};
use crate::envelope::Issue;

/// The Check timeout of spec section 4.
///
/// An unoptimised build can spend the whole budget on the curated dictionary,
/// so debug uses a longer budget. The shipped binary keeps the 10 s limit.
pub const DEFAULT_TIMEOUT: Duration =
    Duration::from_secs(if cfg!(debug_assertions) { 60 } else { 10 });

/// How many times the curated dictionary and rule set were built.
///
/// The counter exists for the test that proves the default engine path never
/// initialises Harper (`cli/tests/harper_lazy.rs`).
static INITIALISATIONS: AtomicUsize = AtomicUsize::new(0);

/// The reading of the counter behind [`INITIALISATIONS`].
pub fn initialisations() -> usize {
    INITIALISATIONS.load(Ordering::SeqCst)
}

/// Lint `text` with the curated rule set.
///
/// Building the dictionary and the rule set is the expensive step, so it
/// happens here, on the Check itself, rather than when the adapter is built.
fn lint(text: &str) -> Vec<Issue> {
    INITIALISATIONS.fetch_add(1, Ordering::SeqCst);

    let document = Document::new_curated(text, &PlainEnglish);
    let mut linter = LintGroup::new_curated(FstDictionary::curated(), Dialect::American);

    lints::issues_from(text, &linter.lint(&document))
}

pub struct Harper {
    timeout: Duration,
}

impl Default for Harper {
    fn default() -> Self {
        Harper {
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl Harper {
    pub fn new(timeout: Duration) -> Self {
        Harper { timeout }
    }
}

impl Engine for Harper {
    fn slug(&self) -> &'static str {
        "harper"
    }

    /// Lint on a worker thread so the timeout of spec section 4 applies to an
    /// in-process engine too.
    ///
    /// A thread that runs past the timeout is left to finish. The CLI prints
    /// the timeout envelope and exits, which ends it.
    fn check(&self, text: &str, _options: &CheckOptions) -> Result<Vec<Issue>, EngineFailure> {
        let (sender, receiver) = mpsc::channel();
        let owned = text.to_string();
        thread::spawn(move || sender.send(lint(&owned)));

        match receiver.recv_timeout(self.timeout) {
            Ok(issues) => Ok(issues),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(EngineFailure::Timeout(format!(
                "Harper did not finish within {} s",
                self.timeout.as_secs()
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(EngineFailure::Failed(
                "Harper stopped without an answer".to_string(),
            )),
        }
    }
}
