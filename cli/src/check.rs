//! `grammachy check`, spec section 5.1.

use std::time::Instant;

use crate::args::CheckOptions;
use crate::engine::{self, EngineFailure};
use crate::envelope::{Envelope, ErrorCode};

/// The size limit of one Check, in UTF-16 code units (spec sections 5.2 and 6).
pub const MAX_UTF16_UNITS: usize = 5_000;

/// Length in UTF-16 code units, the unit the shell indexes with.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Validate the text, run the engine, and answer exactly one envelope.
pub fn run(text: &str, options: &CheckOptions) -> Envelope {
    // A selection of only whitespace has nothing to check, so it is empty.
    if text.trim().is_empty() {
        return Envelope::error(ErrorCode::EmptySelection, "The selection is empty.");
    }

    let length = utf16_len(text);
    if length > MAX_UTF16_UNITS {
        return Envelope::error(
            ErrorCode::TextTooLong,
            format!("The selection is {length} units long, over the limit of {MAX_UTF16_UNITS}."),
        );
    }

    let engine_slug = options.engine.as_str();
    let Some(engine) = engine::resolve(options.engine) else {
        return Envelope::error(
            ErrorCode::EngineUnavailable,
            format!("This build has no {engine_slug} adapter."),
        );
    };

    let started = Instant::now();
    let outcome = engine.check(text, options);
    let elapsed_ms = started.elapsed().as_millis() as u64;

    match outcome {
        Ok(issues) => Envelope::result(engine.slug(), elapsed_ms, issues),
        Err(EngineFailure::Unavailable(message)) => {
            Envelope::error(ErrorCode::EngineUnavailable, message)
        }
        Err(EngineFailure::Timeout(message)) => Envelope::error(ErrorCode::EngineTimeout, message),
        Err(EngineFailure::Failed(message)) => Envelope::error(ErrorCode::EngineError, message),
    }
}
