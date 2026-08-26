//! `grammachy bench`, spec section 13.1.
//!
//! One run sends the interference fixture through every engine this machine can
//! reach and prints one Markdown document: an Engines table, the Models tables,
//! the rows that were skipped, and the regression rule a release is held to.
//! Redirecting that output into `docs/benchmarks/<version>.md` is how a release
//! records its numbers, so nothing is added to the file by hand.
//!
//! This is the one subcommand that does not print a JSON envelope on success.
//! It is a developer and release command, not a shell surface: the shell calls
//! `check` and `chunk` only. A failure still prints the error envelope, so the
//! exit-1 contract of spec section 5.1 holds.
//!
//! `--engine` names the engine that the `--model` rows are evaluated with,
//! `openai` by default or `openrouter`; `--cloud-model` names a row that runs
//! through `openrouter` whatever `--engine` says, so one run holds local and
//! cloud rows side by side. Neither narrows the Engines table, because a
//! benchmark file holds every table and one run must produce the whole file.
//!
//! Reachability is decided by running, not by probing: an engine that answers
//! `engine_unavailable` for the first sentence is a skipped row with that
//! sentence as its reason. A later failure is an invalid Check, counted as a
//! miss (HUF-205). Nothing here treats a missing engine as an error.
//!
//! `--max-cost` caps what a run may spend through `openrouter`, summed from
//! `usage.cost`. A row that would pass the cap ends as skipped, and so does
//! every cloud row after it. `--record <dir>` writes every Check's answer to
//! `checks.json`, the input of the judge script (HUF-205).

pub mod fixture;
pub mod machine;
pub mod memory;
pub mod metrics;
pub mod report;
pub mod weights;

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde::Serialize;

use crate::args::{BenchArgs, CheckOptions, EngineSlug};
use crate::engine::{self, EngineFailure};
use crate::engines::{languagetool, openai};
use crate::envelope::Issue;
use crate::settings::StoredSettings;

use fixture::Sentence;
use machine::Machine;
use metrics::{Recorded, Tally};
use report::{EngineRow, Measurement, ModelRow, Outcome, Report};

/// The engines of the Engines table, in the order the table prints them.
///
/// `openrouter` is not here: it has no meaning without a model id, so it
/// appears in the Models tables only.
const ENGINES: [EngineSlug; 3] = [
    EngineSlug::Languagetool,
    EngineSlug::Harper,
    EngineSlug::Openai,
];

/// Build the report of one run, or say why the arguments do not describe a run.
pub fn run(args: &BenchArgs, stored: &StoredSettings) -> Result<String, String> {
    let plan = Plan::of(args)?;
    let base = base_options(stored);
    let sentences = fixture::sentences();
    let mut spend = Spend::new(args.max_cost);
    let mut checks: Vec<RecordedCheck> = Vec::new();

    let engines = ENGINES
        .iter()
        .map(|slug| engine_row(*slug, &base, &sentences, &mut spend, &mut checks))
        .collect();
    let models = plan
        .rows
        .iter()
        .map(|(slug, model)| model_row(*slug, model, &base, &sentences, &mut spend, &mut checks))
        .collect();

    if let Some(directory) = &args.record {
        record(directory, &checks)?;
    }

    let (interference, clean) = counts(&sentences);
    Ok(Report {
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine: Machine::here(),
        command: command_line(args),
        interference,
        clean,
        languages: languages(&sentences),
        default_engine: CheckOptions::default().engine.as_str().to_string(),
        max_cost: args.max_cost,
        engines,
        models,
    }
    .render())
}

/// The Models rows one run evaluates, each on its engine, local rows first.
#[derive(Debug, PartialEq)]
struct Plan {
    rows: Vec<(EngineSlug, String)>,
}

impl Plan {
    /// Read the flags, or say why they do not describe a run.
    fn of(args: &BenchArgs) -> Result<Plan, String> {
        let engine = match args.engine {
            None => EngineSlug::Openai,
            Some(EngineSlug::Openai) | Some(EngineSlug::Openrouter) => args.engine.unwrap(),
            Some(other) if args.models.is_empty() => {
                return Err(format!(
                    "The {} engine takes no model, so --engine {} benchmarks nothing.",
                    other.as_str(),
                    other.as_str()
                ))
            }
            Some(other) => {
                return Err(format!(
                    "Only the openai and openrouter engines take a model, not {}.",
                    other.as_str()
                ))
            }
        };

        let mut rows: Vec<(EngineSlug, String)> = Vec::new();
        let mut cloud: Vec<(EngineSlug, String)> = Vec::new();
        for model in &args.models {
            if engine.is_cloud() {
                cloud.push((engine, model.clone()));
            } else {
                rows.push((engine, model.clone()));
            }
        }
        for model in &args.cloud_models {
            cloud.push((EngineSlug::Openrouter, model.clone()));
        }
        rows.extend(cloud);

        let any_cloud = rows.iter().any(|(slug, _)| slug.is_cloud());
        match (any_cloud, args.max_cost) {
            (true, None) => {
                return Err(
                    "A run through openrouter needs --max-cost <usd>, the most the whole run may spend."
                        .to_string(),
                )
            }
            (true, Some(cap)) if cap.is_nan() || cap <= 0.0 => {
                return Err("--max-cost must be a positive number of USD.".to_string())
            }
            (false, Some(_)) => {
                return Err(
                    "--max-cost applies to openrouter rows only, and this run has none.".to_string(),
                )
            }
            _ => {}
        }

        Ok(Plan { rows })
    }
}

/// What the run has spent so far through the cloud engine.
struct Spend {
    cap: Option<f64>,
    spent_usd: f64,
    /// The last cost seen, the estimate of what the next Check will cost.
    last_usd: f64,
    /// Set once a row hit the cap, so every later cloud row is skipped too.
    exhausted: Option<String>,
}

impl Spend {
    fn new(cap: Option<f64>) -> Spend {
        Spend {
            cap,
            spent_usd: 0.0,
            last_usd: 0.0,
            exhausted: None,
        }
    }

    /// Whether the next Check would pass the cap.
    fn would_exceed(&self) -> bool {
        self.cap
            .is_some_and(|cap| self.spent_usd + self.last_usd > cap)
    }

    fn add(&mut self, cost: Option<f64>) {
        if let Some(cost) = cost {
            self.spent_usd += cost;
            self.last_usd = cost;
        }
    }
}

/// One Check as `--record` writes it.
#[derive(Debug, Clone, Serialize)]
struct RecordedCheck {
    engine: String,
    model: String,
    id: String,
    valid: bool,
    latency_ms: u64,
    cost: Option<f64>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    prompt_ms: Option<f64>,
    generation_ms: Option<f64>,
    issues: Vec<Issue>,
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
        .filter(|sentence| sentence.is_interference())
        .count();
    (interference, sentences.len() - interference)
}

/// The native languages with at least one interference sentence, in file order.
fn languages(sentences: &[Sentence]) -> Vec<String> {
    let mut languages: Vec<String> = Vec::new();
    for sentence in sentences
        .iter()
        .filter(|sentence| sentence.is_interference())
    {
        if !languages.contains(&sentence.native) {
            languages.push(sentence.native.clone());
        }
    }
    languages
}

/// The command that produced this file, so the file can be reproduced.
fn command_line(args: &BenchArgs) -> String {
    let mut line = String::from("grammachy bench");
    if !args.models.is_empty() {
        line.push_str(&format!(
            " --engine {}",
            args.engine.unwrap_or(EngineSlug::Openai).as_str()
        ));
        for model in &args.models {
            line.push_str(&format!(" --model {model}"));
        }
    }
    for model in &args.cloud_models {
        line.push_str(&format!(" --cloud-model {model}"));
    }
    if let Some(cap) = args.max_cost {
        line.push_str(&format!(" --max-cost {cap}"));
    }
    line
}

fn engine_row(
    slug: EngineSlug,
    base: &CheckOptions,
    sentences: &[Sentence],
    spend: &mut Spend,
    checks: &mut Vec<RecordedCheck>,
) -> EngineRow {
    let options = CheckOptions {
        engine: slug,
        ..base.clone()
    };
    EngineRow {
        engine: slug.as_str().to_string(),
        memory_kind: memory_kind(slug),
        outcome: measure(slug, &options, sentences, spend, checks),
    }
}

fn model_row(
    slug: EngineSlug,
    model: &str,
    base: &CheckOptions,
    sentences: &[Sentence],
    spend: &mut Spend,
    checks: &mut Vec<RecordedCheck>,
) -> ModelRow {
    let options = match slug {
        EngineSlug::Openrouter => CheckOptions {
            engine: slug,
            openrouter_model: model.to_string(),
            ..base.clone()
        },
        _ => {
            // llama.cpp serves one model per process, so each row needs its
            // own server.
            restart_model_server();
            CheckOptions {
                engine: slug,
                openai_model: model.to_string(),
                ..base.clone()
            }
        }
    };
    let weights = if slug.is_cloud() {
        weights::HOSTED
    } else {
        weights::of(model)
    };
    ModelRow {
        model: model.to_string(),
        engine: slug.as_str().to_string(),
        weights,
        outcome: measure(slug, &options, sentences, spend, checks),
    }
}

/// Run the whole fixture through one engine and read its resident memory.
///
/// An engine that cannot answer the first sentence ends the row there: a
/// benchmark of half a fixture is worse than no benchmark, and the message the
/// engine gave is exactly what the reader of the file needs. A failure after
/// that is one invalid Check, because a model that times out on one sentence
/// in forty is a fact the row should carry rather than hide.
fn measure(
    slug: EngineSlug,
    options: &CheckOptions,
    sentences: &[Sentence],
    spend: &mut Spend,
    checks: &mut Vec<RecordedCheck>,
) -> Outcome {
    let Some(adapter) = engine::resolve(slug) else {
        return Outcome::Skipped(format!("This build has no {} adapter.", slug.as_str()));
    };
    if slug.is_cloud() {
        if let Some(why) = &spend.exhausted {
            return Outcome::Skipped(why.clone());
        }
    }

    let started_row = Instant::now();
    let before = memory::peak_resident_bytes();
    let mut recorded = Vec::with_capacity(sentences.len());
    let model = row_model(slug, options);

    for (index, sentence) in sentences.iter().enumerate() {
        if slug.is_cloud() && spend.would_exceed() {
            let why = format!(
                "cost cap {} USD reached after {index} sentences",
                spend.cap.unwrap_or_default()
            );
            spend.exhausted = Some(why.clone());
            return Outcome::Skipped(why);
        }

        let options = CheckOptions {
            native: sentence.native_language(),
            ..options.clone()
        };
        let started = Instant::now();
        let answer = adapter.answer(&sentence.text, &options);
        let latency_ms = started.elapsed().as_millis() as u64;

        let (issues, cost, usage, valid) = match answer {
            Ok(answer) => (answer.issues, answer.cost, answer.usage, true),
            Err(failure) if index == 0 && ends_the_row(&failure) => {
                return Outcome::Skipped(reason(&sentence.id, failure));
            }
            Err(failure) => {
                eprintln!(
                    "grammachy bench: {} on {}: {}",
                    model,
                    sentence.id,
                    reason(&sentence.id, failure)
                );
                (Vec::new(), None, None, false)
            }
        };
        if slug.is_cloud() && valid && cost.is_none() {
            eprintln!(
                "grammachy bench: {model} on {} answered without usage.cost",
                sentence.id
            );
        }
        spend.add(cost);

        checks.push(RecordedCheck {
            engine: slug.as_str().to_string(),
            model: model.clone(),
            id: sentence.id.clone(),
            valid,
            latency_ms,
            cost,
            prompt_tokens: usage.and_then(|usage| usage.prompt_tokens),
            completion_tokens: usage.and_then(|usage| usage.completion_tokens),
            prompt_ms: usage.and_then(|usage| usage.prompt_ms),
            generation_ms: usage.and_then(|usage| usage.generation_ms),
            issues: issues.clone(),
        });
        recorded.push(Recorded {
            id: sentence.id.clone(),
            native: sentence.native.clone(),
            text: sentence.text.clone(),
            edits: sentence.edits.iter().map(fixture::Edit::span).collect(),
            expected_text: sentence.expected_text.clone(),
            issues,
            valid,
            latency_ms,
            cost,
            usage,
        });
    }

    Outcome::Measured(Box::new(Measurement {
        tally: Tally::of(&recorded),
        memory_bytes: memory_bytes(slug, before),
        wall_ms: started_row.elapsed().as_millis() as u64,
    }))
}

/// The name the record file carries for one row.
fn row_model(slug: EngineSlug, options: &CheckOptions) -> String {
    match slug {
        EngineSlug::Openrouter => options.openrouter_model.clone(),
        EngineSlug::Openai => options.openai_model.clone(),
        other => other.as_str().to_string(),
    }
}

/// Whether a first-sentence failure means the engine is not there at all.
///
/// A timeout or a server error on the first sentence is a Check that ran and
/// failed, so it is an invalid Check rather than a skipped row.
fn ends_the_row(failure: &EngineFailure) -> bool {
    matches!(
        failure,
        EngineFailure::Unavailable(_) | EngineFailure::BadArguments(_)
    )
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

/// Write every Check of the run to `<directory>/checks.json`.
fn record(directory: &Path, checks: &[RecordedCheck]) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| {
        format!(
            "--record: {} cannot be created: {error}",
            directory.display()
        )
    })?;
    let path = directory.join("checks.json");
    let text = serde_json::to_string_pretty(checks).expect("checks serialise");
    std::fs::write(&path, text)
        .map_err(|error| format!("--record: {} cannot be written: {error}", path.display()))
}

/// What resident memory means for one engine, named under the table.
fn memory_kind(slug: EngineSlug) -> &'static str {
    match slug {
        EngineSlug::Harper => {
            "the growth of this process's own peak RSS, because it runs in process"
        }
        EngineSlug::Languagetool | EngineSlug::Openai => "the RSS of its server process",
        EngineSlug::Openrouter => "not measured, because the model runs on the provider's machine",
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
        EngineSlug::Openrouter => None,
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
            cloud_models: Vec::new(),
            max_cost: None,
            record: None,
        }
    }

    fn cloud(models: &[&str], cloud_models: &[&str], max_cost: Option<f64>) -> BenchArgs {
        BenchArgs {
            cloud_models: cloud_models.iter().map(|model| model.to_string()).collect(),
            max_cost,
            ..args(None, models)
        }
    }

    #[test]
    fn a_run_with_no_engine_flag_evaluates_models_on_openai() {
        assert_eq!(
            Plan::of(&args(None, &["qwen2.5-7b-instruct"]))
                .unwrap()
                .rows,
            [(EngineSlug::Openai, "qwen2.5-7b-instruct".to_string())]
        );
    }

    #[test]
    fn an_engine_that_takes_no_model_is_refused_before_anything_runs() {
        let message = Plan::of(&args(Some(EngineSlug::Harper), &["qwen2.5-7b-instruct"]))
            .expect_err("harper takes no model");

        assert!(
            message.contains("Only the openai and openrouter engines"),
            "{message}"
        );
    }

    #[test]
    fn an_engine_flag_with_no_model_says_it_benchmarks_nothing() {
        let message =
            Plan::of(&args(Some(EngineSlug::Languagetool), &[])).expect_err("nothing to run");

        assert!(message.contains("takes no model"), "{message}");
    }

    #[test]
    fn cloud_rows_come_after_local_rows_and_need_a_cost_cap() {
        let plan = Plan::of(&cloud(
            &["gemma-4-e4b-it"],
            &["deepseek/deepseek-v4-flash-0731"],
            Some(10.0),
        ))
        .expect("a capped cloud run");
        assert_eq!(
            plan.rows,
            [
                (EngineSlug::Openai, "gemma-4-e4b-it".to_string()),
                (
                    EngineSlug::Openrouter,
                    "deepseek/deepseek-v4-flash-0731".to_string()
                ),
            ]
        );

        let message = Plan::of(&cloud(&[], &["deepseek/deepseek-v4-flash-0731"], None))
            .expect_err("no cap, no cloud run");
        assert!(message.contains("--max-cost"), "{message}");

        let message = Plan::of(&cloud(&["gemma-4-e4b-it"], &[], Some(10.0)))
            .expect_err("a cap without cloud rows");
        assert!(message.contains("openrouter rows only"), "{message}");
    }

    #[test]
    fn engine_openrouter_makes_every_model_a_cloud_row() {
        let plan = Plan::of(&BenchArgs {
            max_cost: Some(1.0),
            ..args(Some(EngineSlug::Openrouter), &["google/gemini-3.7-flash"])
        })
        .expect("a capped cloud run");

        assert_eq!(plan.rows[0].0, EngineSlug::Openrouter);
    }

    #[test]
    fn the_command_line_repeats_every_flag_so_the_file_can_be_reproduced() {
        assert_eq!(command_line(&args(None, &[])), "grammachy bench");
        assert_eq!(
            command_line(&args(Some(EngineSlug::Openai), &["qwen2.5-7b-instruct", "qwen2.5-3b-instruct"])),
            "grammachy bench --engine openai --model qwen2.5-7b-instruct --model qwen2.5-3b-instruct"
        );
        assert_eq!(
            command_line(&cloud(&["gemma-4-e4b-it"], &["google/gemini-3.7-flash"], Some(10.0))),
            "grammachy bench --engine openai --model gemma-4-e4b-it --cloud-model google/gemini-3.7-flash --max-cost 10"
        );
    }

    #[test]
    fn the_cap_stops_before_the_check_that_would_pass_it() {
        let mut spend = Spend::new(Some(0.05));
        assert!(!spend.would_exceed());
        spend.add(Some(0.03));
        assert!(
            spend.would_exceed(),
            "0.03 spent plus 0.03 next passes 0.05"
        );

        let mut open = Spend::new(None);
        open.add(Some(100.0));
        assert!(!open.would_exceed());
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
    fn only_an_absent_engine_ends_the_row_on_the_first_sentence() {
        assert!(ends_the_row(&EngineFailure::Unavailable(String::new())));
        assert!(ends_the_row(&EngineFailure::BadArguments(String::new())));
        assert!(!ends_the_row(&EngineFailure::Timeout(String::new())));
        assert!(!ends_the_row(&EngineFailure::Failed(String::new())));
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
