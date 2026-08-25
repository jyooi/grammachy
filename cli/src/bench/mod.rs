//! `grammachy bench`, spec section 13.1.
//!
//! One run sends the interference fixture through every engine this machine can
//! reach and prints one Markdown document: an Engines table, a Models table,
//! the rows that were skipped, and the regression rule a release is held to.
//! Redirecting that output into `docs/benchmarks/<version>.md` is how a release
//! records its numbers, so nothing is added to the file by hand.
//!
//! This is the one subcommand that does not print a JSON envelope on success.
//! It is a developer and release command, not a shell surface: the shell calls
//! `check` and `chunk` only. A failure still prints the error envelope, so the
//! exit-1 contract of spec section 5.1 holds.
//!
//! `--engine` names the engine that the `--model` rows are evaluated with, and
//! v1 accepts `openai` there. It does not narrow the Engines table, because a
//! benchmark file holds both tables and one run must produce the whole file.
//!
//! Reachability is decided by running, not by probing: an engine that answers
//! `engine_unavailable` for the first sentence is a skipped row with that
//! sentence as its reason. Nothing here treats a missing engine as an error.

pub mod fixture;
pub mod machine;
pub mod memory;
pub mod metrics;
pub mod report;
pub mod weights;

use std::process::Command;
use std::time::Instant;

use crate::args::{BenchArgs, CheckOptions, EngineSlug};
use crate::engine::{self, EngineFailure};
use crate::engines::{languagetool, openai};
use crate::settings::StoredSettings;

use fixture::{Sentence, Span};
use machine::Machine;
use metrics::{Recorded, Tally};
use report::{EngineRow, Measurement, ModelRow, Outcome, Report};

/// The engines of the Engines table, in the order the table prints them.
const ENGINES: [EngineSlug; 3] = [
    EngineSlug::Languagetool,
    EngineSlug::Harper,
    EngineSlug::Openai,
];

/// Build the report of one run, or say why the arguments do not describe a run.
pub fn run(args: &BenchArgs, stored: &StoredSettings) -> Result<String, String> {
    let model_engine = model_engine(args)?;
    let base = base_options(stored);
    let sentences = fixture::sentences();

    let engines = ENGINES
        .iter()
        .map(|slug| engine_row(*slug, &base, &sentences))
        .collect();
    let models = args
        .models
        .iter()
        .map(|model| model_row(model_engine, model, &base, &sentences))
        .collect();

    let (interference, clean) = counts(&sentences);
    Ok(Report {
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine: Machine::here(),
        command: command_line(args),
        interference,
        clean,
        default_engine: CheckOptions::default().engine.as_str().to_string(),
        engines,
        models,
    }
    .render())
}

/// The engine the `--model` rows run on.
///
/// Only `openai` takes a model in v1, so anything else is `bad_arguments`
/// before a single sentence is sent.
fn model_engine(args: &BenchArgs) -> Result<EngineSlug, String> {
    match args.engine {
        None | Some(EngineSlug::Openai) => Ok(EngineSlug::Openai),
        Some(other) if args.models.is_empty() => Err(format!(
            "The {} engine takes no model, so --engine {} benchmarks nothing.",
            other.as_str(),
            other.as_str()
        )),
        Some(other) => Err(format!(
            "Only the openai engine takes a model, not {}.",
            other.as_str()
        )),
    }
}

/// The Settings every row starts from, before the row sets its own engine.
///
/// The stored `shell.json` still applies, because the OpenAI base URL lives
/// there and a benchmark must talk to the server the user's Checks talk to.
fn base_options(stored: &StoredSettings) -> CheckOptions {
    let defaults = CheckOptions::default();
    CheckOptions {
        openai_base_url: stored
            .openai_base_url
            .clone()
            .unwrap_or(defaults.openai_base_url),
        openai_model: stored.openai_model.clone().unwrap_or(defaults.openai_model),
        openai_api_key: stored
            .openai_api_key
            .clone()
            .unwrap_or(defaults.openai_api_key),
        ..defaults
    }
}

fn counts(sentences: &[Sentence]) -> (usize, usize) {
    let interference = sentences
        .iter()
        .filter(|sentence| sentence.expected_span.is_some())
        .count();
    (interference, sentences.len() - interference)
}

/// The command that produced this file, so the file can be reproduced.
fn command_line(args: &BenchArgs) -> String {
    let mut line = String::from("grammachy bench");
    if !args.models.is_empty() {
        line.push_str(" --engine openai");
        for model in &args.models {
            line.push_str(&format!(" --model {model}"));
        }
    }
    line
}

fn engine_row(slug: EngineSlug, base: &CheckOptions, sentences: &[Sentence]) -> EngineRow {
    let options = CheckOptions {
        engine: slug,
        ..base.clone()
    };
    EngineRow {
        engine: slug.as_str().to_string(),
        memory_kind: memory_kind(slug),
        outcome: measure(slug, &options, sentences),
    }
}

fn model_row(
    slug: EngineSlug,
    model: &str,
    base: &CheckOptions,
    sentences: &[Sentence],
) -> ModelRow {
    // llama.cpp serves one model per process, so each row needs its own server.
    restart_model_server();

    let options = CheckOptions {
        engine: slug,
        openai_model: model.to_string(),
        ..base.clone()
    };
    ModelRow {
        model: model.to_string(),
        weights: weights::of(model),
        outcome: measure(slug, &options, sentences),
    }
}

/// Run the whole fixture through one engine and read its resident memory.
///
/// The first sentence that the engine cannot answer ends the row: a benchmark
/// of half a fixture is worse than no benchmark, and the message the engine
/// gave is exactly what the reader of the file needs.
fn measure(slug: EngineSlug, options: &CheckOptions, sentences: &[Sentence]) -> Outcome {
    let Some(adapter) = engine::resolve(slug) else {
        return Outcome::Skipped(format!("This build has no {} adapter.", slug.as_str()));
    };

    let before = memory::peak_resident_bytes();
    let mut recorded = Vec::with_capacity(sentences.len());

    for sentence in sentences {
        let options = CheckOptions {
            native: sentence.native_language(),
            ..options.clone()
        };
        let started = Instant::now();
        let answer = adapter.check(&sentence.text, &options);
        let latency_ms = started.elapsed().as_millis() as u64;

        match answer {
            Ok(issues) => recorded.push(Recorded {
                id: sentence.id.clone(),
                expected: sentence.expected_span,
                spans: issues
                    .iter()
                    .map(|issue| Span {
                        start: issue.start,
                        end: issue.end,
                    })
                    .collect(),
                latency_ms,
            }),
            Err(failure) => return Outcome::Skipped(reason(&sentence.id, failure)),
        }
    }

    Outcome::Measured(Box::new(Measurement {
        tally: Tally::of(&recorded),
        memory_bytes: memory_bytes(slug, before),
    }))
}

/// The one sentence a skipped row carries.
fn reason(id: &str, failure: EngineFailure) -> String {
    let message = match failure {
        EngineFailure::Unavailable(message)
        | EngineFailure::Timeout(message)
        | EngineFailure::Failed(message)
        | EngineFailure::BadArguments(message) => message,
    };
    format!("{message} (at fixture sentence {id})")
}

/// What resident memory means for one engine, named under the table.
fn memory_kind(slug: EngineSlug) -> &'static str {
    match slug {
        EngineSlug::Harper => {
            "the growth of this process's own peak RSS, because it runs in process"
        }
        EngineSlug::Languagetool | EngineSlug::Openai => "the RSS of its server process",
    }
}

/// The resident memory of one engine after its run.
fn memory_bytes(slug: EngineSlug, before: Option<u64>) -> Option<u64> {
    match slug {
        EngineSlug::Harper => {
            let after = memory::peak_resident_bytes()?;
            Some(after.saturating_sub(before?))
        }
        EngineSlug::Languagetool => {
            memory::resident_bytes(memory::unit_main_pid(languagetool::unit::UNIT_NAME)?)
        }
        EngineSlug::Openai => {
            memory::resident_bytes(memory::unit_main_pid(openai::unit::UNIT_NAME)?)
        }
    }
}

/// Stop the llama.cpp unit so the next model row starts its own server.
///
/// This is skipped whenever unit starts are forbidden, which is the seam every
/// test sets, so no test ever reaches systemd.
fn restart_model_server() {
    if !openai::Config::from_env().start_unit {
        return;
    }
    let _ = Command::new("systemctl")
        .args(["--user", "stop", openai::unit::UNIT_NAME])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(engine: Option<EngineSlug>, models: &[&str]) -> BenchArgs {
        BenchArgs {
            engine,
            models: models.iter().map(|model| model.to_string()).collect(),
        }
    }

    #[test]
    fn a_run_with_no_engine_flag_evaluates_models_on_openai() {
        assert_eq!(
            model_engine(&args(None, &["qwen2.5-7b-instruct"])),
            Ok(EngineSlug::Openai)
        );
    }

    #[test]
    fn an_engine_that_takes_no_model_is_refused_before_anything_runs() {
        let message = model_engine(&args(Some(EngineSlug::Harper), &["qwen2.5-7b-instruct"]))
            .expect_err("harper takes no model");

        assert!(message.contains("Only the openai engine"), "{message}");
    }

    #[test]
    fn an_engine_flag_with_no_model_says_it_benchmarks_nothing() {
        let message =
            model_engine(&args(Some(EngineSlug::Languagetool), &[])).expect_err("nothing to run");

        assert!(message.contains("takes no model"), "{message}");
    }

    #[test]
    fn the_command_line_repeats_every_model_so_the_file_can_be_reproduced() {
        assert_eq!(command_line(&args(None, &[])), "grammachy bench");
        assert_eq!(
            command_line(&args(Some(EngineSlug::Openai), &["qwen2.5-7b-instruct", "qwen2.5-3b-instruct"])),
            "grammachy bench --engine openai --model qwen2.5-7b-instruct --model qwen2.5-3b-instruct"
        );
    }

    #[test]
    fn a_failed_engine_names_the_sentence_it_stopped_at() {
        let skipped = reason(
            "zh-01",
            EngineFailure::Unavailable("No LanguageTool answered on 127.0.0.1:8081".to_string()),
        );

        assert_eq!(
            skipped,
            "No LanguageTool answered on 127.0.0.1:8081 (at fixture sentence zh-01)"
        );
    }

    #[test]
    fn the_base_options_keep_the_openai_endpoint_of_the_stored_settings() {
        let stored = StoredSettings {
            openai_base_url: Some("http://127.0.0.1:9999".to_string()),
            ..StoredSettings::default()
        };

        let options = base_options(&stored);

        assert_eq!(options.openai_base_url, "http://127.0.0.1:9999");
        assert_eq!(options.engine, CheckOptions::default().engine);
    }
}
