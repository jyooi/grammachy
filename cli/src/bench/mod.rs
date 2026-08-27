//! `grammachy bench`, `docs/spec/evals.md` section 4.
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
//! every cloud row after it. A cloud answer that carries no `usage.cost` ends
//! its row the same way, because a run that cannot measure its spend cannot
//! hold the cap. `--record <dir>` writes every Check's answer to `checks.json`,
//! the input of the judge script (HUF-205). The run proves the directory holds
//! that file before the first row, so a directory it cannot write never
//! discards a report it already paid for. The last row writes a pending file
//! and renames it, so the record of an earlier run stays whole until this run
//! has a whole one of its own.
//!
//! A run is watchable and does not wait on the network (section 4.1). Every
//! sentence prints one line on stderr naming its row, its item, and its time.
//! Cloud rows run one thread each, beside each other and beside the local
//! rows, because a cloud row waits on a provider while a local row uses the
//! machine. Local rows stay on this thread and in order, because they share
//! one llama.cpp server and one in-process Harper. Every row is placed back at
//! its own index, so the tables print in plan order whatever order they end in.
//!
//! `--thinking off|on|both` decides the mode of every local row, and `both`
//! runs each local model twice so one file holds both modes. The flag rather
//! than the stored `localThinking` is what the rows carry, so a benchmark file
//! is reproducible from the Command line it prints. The Engines table has no
//! Thinking column, so its `openai` row runs once, in the mode the flag names.
//! Only `both` leaves that row on the product default.
//!
//! `--max-cost` is read under one lock before each cloud Check, so a run may
//! pass the cap by at most one Check for each cloud row in flight. The report
//! prints what the run actually paid rather than the cap.

pub mod fixture;
pub mod judge;
pub mod machine;
pub mod memory;
pub mod metrics;
pub mod report;
pub mod weights;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::args::{BenchArgs, BenchThinking, CheckOptions, EngineSlug};
use crate::engine::{self, Engine, EngineFailure};
use crate::engines::{languagetool, openai, openrouter};
use crate::envelope::Issue;
use crate::settings::StoredSettings;

use fixture::Sentence;
use machine::Machine;
use memory::{Reading, Source};
use metrics::{Recorded, Tally};
use report::{EngineRow, Measurement, ModelRow, Outcome, Report, ServerUse};

/// The engines of the Engines table, in the order the table prints them.
///
/// `openrouter` is not here: it has no meaning without a model id, so it
/// appears in the Models tables only.
const ENGINES: [EngineSlug; 3] = [
    EngineSlug::Languagetool,
    EngineSlug::Harper,
    EngineSlug::Openai,
];

/// The file `--record` writes inside the directory it is given.
const RECORD_FILE: &str = "checks.json";

/// What one run produced.
///
/// The report and the record file are two separate promises. A `--record`
/// write that fails after the rows ran keeps the report, because the run
/// already paid for those numbers, and carries the failure beside it.
pub struct Run {
    /// The whole benchmark file, as Markdown.
    pub report: String,
    /// Why `--record` did not land, when the run reached the write.
    pub record_failure: Option<String>,
}

/// Build the report of one run, or say why the arguments do not describe a run.
pub fn run(args: &BenchArgs, stored: &StoredSettings) -> Result<Run, String> {
    let plan = Plan::of(args)?;
    let base = base_options(stored, args.thinking);
    let sentences = fixture::sentences();
    let spend = Mutex::new(Spend::new(args.max_cost));

    let (engines, models) = std::thread::scope(|scope| {
        // The cloud rows start first, so they wait on their provider while
        // this thread runs the rows that need the machine.
        let cloud: Vec<(usize, _)> = plan
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.slug.is_cloud())
            .map(|(index, row)| {
                let (base, sentences, spend) = (&base, &sentences, &spend);
                let handle =
                    scope.spawn(move || model_row(row, ServerUse::Fresh, base, sentences, spend));
                (index, handle)
            })
            .collect();

        let engines: Vec<(EngineRow, Vec<RecordedCheck>)> = ENGINES
            .iter()
            .map(|slug| engine_row(*slug, &base, &sentences, &spend))
            .collect();

        let mut models: Vec<Option<(ModelRow, Vec<RecordedCheck>)>> =
            plan.rows.iter().map(|_| None).collect();
        // The model the llama.cpp unit currently serves, so two rows that
        // differ only by thinking mode share one server start.
        let mut serving: Option<&str> = None;
        for (index, row) in plan.rows.iter().enumerate() {
            if row.slug.is_cloud() {
                continue;
            }
            let server_use = server_use(serving, &row.model);
            if server_use == ServerUse::Fresh {
                restart_model_server();
                serving = Some(row.model.as_str());
            }
            models[index] = Some(model_row(row, server_use, &base, &sentences, &spend));
        }
        for (index, handle) in cloud {
            models[index] = Some(handle.join().expect("a benchmark row does not panic"));
        }

        let models: Vec<(ModelRow, Vec<RecordedCheck>)> = models
            .into_iter()
            .map(|row| row.expect("every planned row ran"))
            .collect();
        (engines, models)
    });

    // The record file keeps plan order however the rows finished, so two runs
    // of one command produce a file that can be compared line by line.
    let checks: Vec<RecordedCheck> = engines
        .iter()
        .map(|(_, checks)| checks)
        .chain(models.iter().map(|(_, checks)| checks))
        .flat_map(|checks| checks.iter().cloned())
        .collect();
    let engines: Vec<EngineRow> = engines.into_iter().map(|(row, _)| row).collect();
    let models: Vec<ModelRow> = models.into_iter().map(|(row, _)| row).collect();

    let spend = spend.into_inner().expect("the spend is not poisoned");
    let record_failure = plan
        .record
        .as_ref()
        .and_then(|path| record(path, &checks).err());

    // The judgement is read from the record of this run rather than from the
    // rows, so one folded answer is graded once however many rows wrote it.
    let judge = plan.judgements.as_ref().map(|judgements| {
        let hits: Vec<judge::Hit> = one_per_item(&checks)
            .iter()
            .filter_map(|check| check.hit())
            .collect();
        judge::Assessment::of(&hits, judgements, &judge::labels())
    });

    let (interference, clean) = counts(&sentences);
    let report = Report {
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine: Machine::here(),
        command: command_line(args),
        interference,
        clean,
        languages: languages(&sentences),
        default_engine: CheckOptions::default().engine.as_str().to_string(),
        max_cost: args.max_cost,
        cloud_spend_usd: spend.spent_usd(),
        engines,
        models,
        judge,
    }
    .render();

    Ok(Run {
        report,
        record_failure,
    })
}

/// One Models row: a model, on an engine, in one thinking mode.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    slug: EngineSlug,
    model: String,
    /// The local thinking mode, `None` for a cloud row. A provider never reads
    /// `chat_template_kwargs`, so a cloud row has no mode of its own to name.
    thinking: Option<bool>,
}

impl Row {
    /// The rows one planned pair expands into, one per thinking mode.
    ///
    /// A cloud row is one row whatever the flag says: `--thinking` is a local
    /// setting, and running a provider twice would bill the run twice for the
    /// same answers.
    fn of(slug: EngineSlug, model: String, thinking: BenchThinking) -> Vec<Row> {
        if slug.is_cloud() {
            return vec![Row {
                slug,
                model,
                thinking: None,
            }];
        }
        thinking
            .modes()
            .iter()
            .map(|mode| Row {
                slug,
                model: model.clone(),
                thinking: Some(*mode),
            })
            .collect()
    }

    /// The Settings this row runs its whole fixture with.
    fn options(&self, base: &CheckOptions) -> CheckOptions {
        match self.slug {
            EngineSlug::Openrouter => CheckOptions {
                engine: self.slug,
                openrouter_model: self.model.clone(),
                ..base.clone()
            },
            _ => CheckOptions {
                engine: self.slug,
                openai_model: self.model.clone(),
                local_thinking: self.thinking.unwrap_or(base.local_thinking),
                ..base.clone()
            },
        }
    }
}

/// The Models rows one run evaluates, each on its engine, local rows first.
#[derive(Debug, PartialEq)]
struct Plan {
    rows: Vec<Row>,
    /// Where `record` writes, in a directory already proved writable.
    record: Option<PathBuf>,
    /// The judgements `--judgements` named, read before the first row so a
    /// file the run cannot read never costs it a fixture pass.
    judgements: Option<judge::Judgements>,
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

        let mut pairs: Vec<(EngineSlug, String)> = Vec::new();
        let mut cloud: Vec<(EngineSlug, String)> = Vec::new();
        for model in &args.models {
            if engine.is_cloud() {
                cloud.push((engine, model.clone()));
            } else {
                pairs.push((engine, model.clone()));
            }
        }
        for model in &args.cloud_models {
            cloud.push((EngineSlug::Openrouter, model.clone()));
        }
        pairs.extend(cloud);
        // The same pair twice is one fixture run twice, and a cloud row billed
        // twice, so only its first place in the order stays.
        let mut planned: Vec<(EngineSlug, String)> = Vec::new();
        pairs.retain(|pair| {
            let fresh = !planned.contains(pair);
            if fresh {
                planned.push(pair.clone());
            }
            fresh
        });

        // A local pair becomes one row per mode, and the two modes of one
        // model stay next to each other so they share a server start.
        let rows: Vec<Row> = pairs
            .into_iter()
            .flat_map(|(slug, model)| Row::of(slug, model, args.thinking))
            .collect();

        let any_cloud = rows.iter().any(|row| row.slug.is_cloud());
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

        let record = match &args.record {
            Some(directory) => Some(open_record(directory)?),
            None => None,
        };
        let judgements = match &args.judgements {
            Some(path) => Some(judge::read(path)?),
            None => None,
        };

        Ok(Plan {
            rows,
            record,
            judgements,
        })
    }
}

/// Prove the record directory holds the file before the first row runs.
///
/// A run through the cloud engine spends real money, and its report is the
/// whole point of the run. So a directory that cannot hold the record ends the
/// run here, rather than after the last row has already been paid for. The
/// probe is the pending file, so the record of an earlier run stays whole.
fn open_record(directory: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory).map_err(|error| {
        format!(
            "--record: {} cannot be created: {error}",
            directory.display()
        )
    })?;
    let path = directory.join(RECORD_FILE);
    let pending = pending_path(&path);
    std::fs::write(&pending, "")
        .map_err(|error| format!("--record: {} cannot be written: {error}", pending.display()))?;
    let _ = std::fs::remove_file(&pending);
    Ok(path)
}

/// The file one run writes first and renames onto the record file.
///
/// The rename keeps the record of an earlier run whole until this run has a
/// whole one of its own, because a run can end at any sentence.
fn pending_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".pending");
    PathBuf::from(name)
}

/// What the run has spent so far through the cloud engine.
struct Spend {
    cap: Option<f64>,
    spent: f64,
    /// The last cost seen, the estimate of what the next Check will cost.
    last_usd: f64,
    /// Set once a row ended the cloud rows, so every later one is skipped too.
    exhausted: Option<String>,
}

impl Spend {
    fn new(cap: Option<f64>) -> Spend {
        Spend {
            cap,
            spent: 0.0,
            last_usd: 0.0,
            exhausted: None,
        }
    }

    /// Whether the next Check would pass the cap.
    fn would_exceed(&self) -> bool {
        self.cap.is_some_and(|cap| self.spent + self.last_usd > cap)
    }

    fn add(&mut self, cost: Option<f64>) {
        if let Some(cost) = cost {
            self.spent += cost;
            self.last_usd = cost;
        }
    }

    /// What the run has already paid the cloud engine, over the answers that
    /// reported a cost.
    ///
    /// The report reads this rather than the row tallies, because a row the cap
    /// or an unpriced answer ended carries no tally and was still billed.
    fn spent_usd(&self) -> f64 {
        self.spent
    }

    /// End the cloud rows of this run and say why.
    ///
    /// Every later cloud row carries the same sentence, because the reason a
    /// row stopped is a fact about the run rather than about one model.
    fn exhaust(&mut self, why: String) -> Outcome {
        self.exhausted = Some(why.clone());
        Outcome::Skipped(why)
    }
}

/// The one sentence a row skipped for an unpriced answer carries.
///
/// `--max-cost` is a hard bound on what a run may spend. An answer with no
/// `usage.cost` leaves the spend unmeasurable, so the row ends there rather
/// than billing on blind.
fn unpriced(id: &str) -> String {
    format!(
        "the answer for fixture sentence {id} carried no usage.cost, so this run cannot measure its spend"
    )
}

/// One Check as `--record` writes it.
///
/// The item travels with the answer, because the record file is the whole
/// input of the judge script (spec section 4.4) and the only place eval-set
/// text may land (section 4.3). `result_text` is the sentence the writer gets
/// after Accept: it is what the judge grades and the second half of the
/// judgement key, so it is written rather than left to be derived twice.
#[derive(Debug, Clone, Serialize)]
struct RecordedCheck {
    engine: String,
    model: String,
    /// The thinking mode of the row, `None` for every engine but the local
    /// LLM. The judge selects on it, so it travels with the answer (evals spec
    /// section 4.3); see [`row_thinking`].
    thinking: Option<bool>,
    id: String,
    native: String,
    text: String,
    /// The item's own edits, the reference correction the judge is shown.
    edits: Vec<fixture::Edit>,
    expected_text: String,
    valid: bool,
    latency_ms: u64,
    cost: Option<f64>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    prompt_ms: Option<f64>,
    generation_ms: Option<f64>,
    issues: Vec<Issue>,
    /// The text after every Fix, or `None` when a span does not index the text.
    result_text: Option<String>,
}

impl RecordedCheck {
    /// Whether this Check is a non-exact hit, the sample of spec section 4.4.
    ///
    /// The item carries a mistake, the Check answered, at least one Issue
    /// touched a span the item expects, and applying every Fix does not
    /// reproduce the expected sentence. An item nothing touched is a plain
    /// miss: the writer is offered nothing to accept, so there is nothing to
    /// grade.
    fn non_exact_hit(&self) -> bool {
        let spans: Vec<fixture::Span> = self.edits.iter().map(fixture::Edit::span).collect();
        self.valid
            && !spans.is_empty()
            && metrics::is_caught(&self.issues, &spans)
            && !metrics::is_exact(&self.text, &self.issues, &self.expected_text)
    }

    /// The row this Check belongs to, the key of its Models row.
    fn row_key(&self) -> judge::RowKey {
        judge::RowKey {
            engine: self.engine.clone(),
            model: self.model.clone(),
            thinking: self.thinking,
        }
    }

    /// This Check as one folded hit of the run, when it is one.
    ///
    /// A local row that ran with thinking off is never one. `judge.py` grades
    /// the product default alone (evals spec section 4.4), so grading a
    /// thinking-off answer here would count a judgement the file never holds.
    fn hit(&self) -> Option<judge::Hit> {
        if self.thinking == Some(false) {
            return None;
        }
        if !self.non_exact_hit() {
            return None;
        }
        Some(judge::Hit {
            row: self.row_key(),
            id: self.id.clone(),
            result: self.result_text.clone()?,
        })
    }
}

/// The Settings every row starts from, before the row sets its own engine.
///
/// The stored `shell.json` still applies, because the OpenAI base URL lives
/// there and a benchmark must talk to the server the user's Checks talk to.
/// `localThinking` is the one exception: `--thinking` owns it, so the file a
/// run prints is the output of its own Command line rather than of a Setting
/// the reader cannot see. This value is the mode the Engines `openai` row
/// runs in, and every Models row names its own.
fn base_options(stored: &StoredSettings, thinking: BenchThinking) -> CheckOptions {
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
        local_thinking: thinking.engine_mode(),
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
    // Always named, because it always decides what the local rows measured.
    line.push_str(&format!(" --thinking {}", args.thinking.as_str()));
    line
}

fn engine_row(
    slug: EngineSlug,
    base: &CheckOptions,
    sentences: &[Sentence],
    spend: &Mutex<Spend>,
) -> (EngineRow, Vec<RecordedCheck>) {
    let options = CheckOptions {
        engine: slug,
        ..base.clone()
    };
    let (outcome, checks) = measure(slug, &options, sentences, spend);
    (
        EngineRow {
            engine: slug.as_str().to_string(),
            outcome,
        },
        checks,
    )
}

/// Run the whole fixture for one Models row.
///
/// The caller owns the llama.cpp server, because two rows of one model differ
/// only by a request field and must not pay for two server starts.
/// `server_use` is the caller's answer for whether an earlier row of this
/// model already ran, which is what the report's wall time sentence reads
/// rather than inferring the restart rule again.
fn model_row(
    row: &Row,
    server_use: ServerUse,
    base: &CheckOptions,
    sentences: &[Sentence],
    spend: &Mutex<Spend>,
) -> (ModelRow, Vec<RecordedCheck>) {
    let options = row.options(base);
    let weights = if row.slug.is_cloud() {
        weights::HOSTED
    } else {
        weights::of(&row.model)
    };
    let (outcome, checks) = measure(row.slug, &options, sentences, spend);
    (
        ModelRow {
            model: row.model.clone(),
            engine: row.slug.as_str().to_string(),
            thinking: row.thinking,
            server_use,
            weights,
            outcome,
        },
        checks,
    )
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
    spend: &Mutex<Spend>,
) -> (Outcome, Vec<RecordedCheck>) {
    let mut checks: Vec<RecordedCheck> = Vec::with_capacity(sentences.len());
    let Some(adapter) = adapter(slug) else {
        let why = format!("This build has no {} adapter.", slug.as_str());
        return (Outcome::Skipped(why), checks);
    };

    let started_row = Instant::now();
    let before = memory::peak_resident_bytes();
    let mut recorded = Vec::with_capacity(sentences.len());
    let model = row_model(slug, options);
    let thinking = row_thinking(slug, options);
    let label = row_label(slug, &model, thinking);

    for (index, sentence) in sentences.iter().enumerate() {
        // The cap and the reason another row ended are read together, so a row
        // in flight stops as soon as any cloud row has ended the run's spend.
        if slug.is_cloud() {
            let mut spend = spend.lock().expect("the spend is not poisoned");
            if let Some(why) = &spend.exhausted {
                return (Outcome::Skipped(why.clone()), checks);
            }
            if spend.would_exceed() {
                let why = format!(
                    "cost cap {} USD reached after {index} sentences",
                    spend.cap.unwrap_or_default()
                );
                return (spend.exhaust(why), checks);
            }
        }

        let options = CheckOptions {
            native: sentence.native_language(),
            ..options.clone()
        };
        let started = Instant::now();
        let answer = adapter.answer(&sentence.text, &options);
        let check = started.elapsed();
        let latency_ms = check.as_millis() as u64;
        progress(
            &label,
            &sentence.id,
            index,
            sentences.len(),
            check,
            started_row.elapsed(),
        );

        let (issues, cost, usage, valid) = match answer {
            Ok(answer) => (answer.issues, answer.cost, answer.usage, true),
            Err(failure) if index == 0 && ends_the_row(&failure) => {
                return (Outcome::Skipped(reason(&sentence.id, failure)), checks);
            }
            Err(failure) => {
                eprintln!(
                    "grammachy bench: {label} on {}: {}",
                    sentence.id,
                    reason(&sentence.id, failure)
                );
                (Vec::new(), None, None, false)
            }
        };

        checks.push(RecordedCheck {
            engine: slug.as_str().to_string(),
            model: model.clone(),
            thinking,
            id: sentence.id.clone(),
            native: sentence.native.clone(),
            text: sentence.text.clone(),
            edits: sentence.edits.clone(),
            expected_text: sentence.expected_text.clone(),
            valid,
            latency_ms,
            cost,
            prompt_tokens: usage.and_then(|usage| usage.prompt_tokens),
            completion_tokens: usage.and_then(|usage| usage.completion_tokens),
            prompt_ms: usage.and_then(|usage| usage.prompt_ms),
            generation_ms: usage.and_then(|usage| usage.generation_ms),
            issues: issues.clone(),
            result_text: metrics::corrected(&sentence.text, &issues),
        });

        if slug.is_cloud() {
            let mut spend = spend.lock().expect("the spend is not poisoned");
            spend.add(cost);
            if valid && cost.is_none() {
                return (spend.exhaust(unpriced(&sentence.id)), checks);
            }
        }

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

    let measurement = Measurement {
        tally: Tally::of(&recorded),
        memory: memory_reading(slug, before),
        wall_ms: started_row.elapsed().as_millis() as u64,
    };
    (Outcome::Measured(Box::new(measurement)), checks)
}

/// The adapter one row runs on.
///
/// A cloud row retries one transient answer, the rule of section 4.1: a rate
/// limit or a provider fault must not cost the row a Check. Nothing else is
/// changed, so every row still runs the adapter the product runs.
fn adapter(slug: EngineSlug) -> Option<Box<dyn Engine>> {
    if slug != EngineSlug::Openrouter {
        return engine::resolve(slug);
    }
    Some(Box::new(openrouter::Openrouter::new(openrouter::Config {
        retry_after: Some(openrouter::RETRY_DELAY),
        ..openrouter::Config::from_env()
    })))
}

/// One stderr line per sentence, so a run of tens of minutes is watchable.
///
/// It names the row, the item, and both times a watcher needs: the Check that
/// just ended and the row so far. Rows run beside each other, so every line
/// names its own row rather than relying on the order they arrive in.
fn progress(label: &str, id: &str, index: usize, total: usize, check: Duration, row: Duration) {
    eprintln!(
        "grammachy bench: {label} {id} ({}/{total}) {}, row {}",
        index + 1,
        seconds(check),
        seconds(row),
    );
}

/// A duration as one progress cell, such as `1.2 s`.
fn seconds(elapsed: Duration) -> String {
    format!("{:.1} s", elapsed.as_secs_f64())
}

/// The name one row carries on stderr: the engine, plus its model when the
/// engine takes one, plus its mode when the engine has one.
///
/// `--thinking both` runs one model twice, so a progress line that named the
/// model alone would not say which of the two rows it belongs to.
fn row_label(slug: EngineSlug, model: &str, thinking: Option<bool>) -> String {
    let mut label = if model == slug.as_str() {
        model.to_string()
    } else {
        format!("{} {model}", slug.as_str())
    };
    if let Some(on) = thinking {
        label.push_str(&format!(" (thinking {})", mode_word(on)));
    }
    label
}

/// How a thinking mode is written, everywhere a run names one.
pub fn mode_word(on: bool) -> &'static str {
    match on {
        true => "on",
        false => "off",
    }
}

/// The thinking mode one row's record entry and label carry.
///
/// Only the local LLM engine has one. `harper` and `languagetool` never read
/// the Setting, so recording a mode for them would drop their answers out of
/// the judge's sample on a `--thinking off` run.
fn row_thinking(slug: EngineSlug, options: &CheckOptions) -> Option<bool> {
    match slug {
        EngineSlug::Openai => Some(options.local_thinking),
        _ => None,
    }
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

/// Write every Check of the run to the record file the plan opened.
fn record(path: &Path, checks: &[RecordedCheck]) -> Result<(), String> {
    let pending = pending_path(path);
    let text = serde_json::to_string_pretty(&one_per_item(checks)).expect("checks serialise");
    std::fs::write(&pending, text)
        .map_err(|error| format!("--record: {} cannot be written: {error}", pending.display()))?;
    std::fs::rename(&pending, path)
        .map_err(|error| format!("--record: {} cannot be written: {error}", path.display()))
}

/// One entry per engine, model, thinking mode, and item, the promise of evals
/// section 4.3.
///
/// The Engines `openai` row runs the model the Settings name, so a `--model`
/// row that names that model is the same fixture run twice. A repeated key
/// keeps the last entry, the later pass, and holds the first entry's place in
/// the file. The decision waits until here rather than reading the plan,
/// because a row that never ran pushed nothing, and a pair the report measured
/// must never leave the record empty.
fn one_per_item(checks: &[RecordedCheck]) -> Vec<&RecordedCheck> {
    let mut kept: Vec<&RecordedCheck> = Vec::with_capacity(checks.len());
    let mut place: HashMap<(&str, &str, Option<bool>, &str), usize> = HashMap::new();
    for check in checks {
        let key = (
            check.engine.as_str(),
            check.model.as_str(),
            check.thinking,
            check.id.as_str(),
        );
        match place.get(&key) {
            Some(&index) => kept[index] = check,
            None => {
                place.insert(key, kept.len());
                kept.push(check);
            }
        }
    }
    kept
}

/// The resident memory of one engine after its run, and where it was read.
///
/// LanguageTool is a JVM on the CPU, so RSS is the whole of what it holds. The
/// llama.cpp server may hold its weights on a graphics device instead, where
/// RSS cannot see them, so [`memory::server_reading`] asks the device first.
fn memory_reading(slug: EngineSlug, before: Option<u64>) -> Reading {
    match slug {
        EngineSlug::Harper => {
            let growth = memory::peak_resident_bytes()
                .zip(before)
                .map(|(after, before)| after.saturating_sub(before));
            Reading::new(growth, Source::Growth)
        }
        EngineSlug::Languagetool => {
            let pid = memory::unit_main_pid(languagetool::unit::UNIT_NAME);
            Reading::new(pid.and_then(memory::resident_bytes), Source::ServerRss)
        }
        EngineSlug::Openai => {
            memory::server_reading(memory::unit_main_pid(openai::unit::UNIT_NAME))
        }
        EngineSlug::Openrouter => Reading::new(None, Source::Provider),
    }
}

/// Whether one local row shares the server an earlier row of its model ran on.
///
/// The run leaves the server up between the two rows of one model, so the
/// second of them measures on whatever the first measured on. That is a fact
/// about the run and holds however the server got there.
fn server_use(serving: Option<&str>, model: &str) -> ServerUse {
    if serving == Some(model) {
        ServerUse::Reused
    } else {
        ServerUse::Fresh
    }
}

/// Stop the llama.cpp unit so the next model row gets a server of its own.
///
/// This is skipped whenever unit starts are forbidden, which is the seam every
/// test sets, so no test ever reaches systemd. The stop frees no port that a
/// hand-run server holds, which is why no row claims a start it may not have
/// paid for.
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
            judgements: None,
            thinking: BenchThinking::On,
        }
    }

    /// Evals spec section 4.1: the run leaves the server up between the two
    /// rows of one model, and restarts it for every other row.
    #[test]
    fn only_a_later_row_of_the_same_model_reuses_a_server() {
        assert_eq!(server_use(Some("gemma"), "gemma"), ServerUse::Reused);
        assert_eq!(server_use(None, "gemma"), ServerUse::Fresh);
        assert_eq!(server_use(Some("qwen"), "gemma"), ServerUse::Fresh);
    }

    /// One planned row, the way the tests name them.
    fn row(slug: EngineSlug, model: &str, thinking: Option<bool>) -> Row {
        Row {
            slug,
            model: model.to_string(),
            thinking,
        }
    }

    fn local(model: &str) -> Row {
        row(EngineSlug::Openai, model, Some(true))
    }

    fn cloud_row(model: &str) -> Row {
        row(EngineSlug::Openrouter, model, None)
    }

    /// The record one earlier run left in the directory.
    const EARLIER_RUN: &str = r#"[{"id":"zh-01"}]"#;

    fn recorded_check(id: &str) -> RecordedCheck {
        RecordedCheck {
            engine: "openrouter".to_string(),
            model: "deepseek/deepseek-v4-flash-0731".to_string(),
            thinking: None,
            id: id.to_string(),
            native: "zh".to_string(),
            text: "She has twenty years.".to_string(),
            edits: Vec::new(),
            expected_text: "She is twenty years old.".to_string(),
            valid: true,
            latency_ms: 120,
            cost: Some(0.0001),
            prompt_tokens: Some(31),
            completion_tokens: Some(18),
            prompt_ms: Some(12.5),
            generation_ms: Some(240.0),
            issues: Vec::new(),
            result_text: Some("She has twenty years.".to_string()),
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
            [local("qwen2.5-7b-instruct")]
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
                local("gemma-4-e4b-it"),
                cloud_row("deepseek/deepseek-v4-flash-0731"),
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
    fn the_same_pair_twice_is_planned_once_so_it_runs_and_bills_once() {
        let both_flags = Plan::of(&BenchArgs {
            cloud_models: vec!["deepseek/deepseek-v4-flash-0731".to_string()],
            max_cost: Some(1.0),
            ..args(
                Some(EngineSlug::Openrouter),
                &["deepseek/deepseek-v4-flash-0731"],
            )
        })
        .expect("a capped cloud run");
        assert_eq!(
            both_flags.rows,
            [cloud_row("deepseek/deepseek-v4-flash-0731")]
        );

        let repeated = Plan::of(&cloud(
            &[],
            &[
                "deepseek/deepseek-v4-flash-0731",
                "deepseek/deepseek-v4-flash-0731",
            ],
            Some(1.0),
        ))
        .expect("a capped cloud run");
        assert_eq!(repeated.rows, both_flags.rows);

        let mixed = Plan::of(&cloud(
            &["gemma-4-e4b-it", "gemma-4-e4b-it"],
            &["google/gemini-3.7-flash"],
            Some(1.0),
        ))
        .expect("a capped cloud run");
        assert_eq!(
            mixed.rows,
            [
                local("gemma-4-e4b-it"),
                cloud_row("google/gemini-3.7-flash"),
            ],
            "local rows still come before cloud rows"
        );
    }

    #[test]
    fn the_record_keeps_the_later_pass_of_a_pair_two_rows_both_ran() {
        let mut first = recorded_check("zh-01");
        first.engine = "openai".to_string();
        first.model = "gemma-4-e4b-it".to_string();
        first.latency_ms = 3451;
        let mut later = first.clone();
        later.latency_ms = 3230;
        let other = recorded_check("zh-02");

        let both_passes = [first, other.clone(), later.clone()];
        let kept = one_per_item(&both_passes);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].latency_ms, 3230, "the later pass wins the key");
        assert_eq!(kept[0].id, "zh-01", "and keeps the first entry's place");
        assert_eq!(kept[1].id, other.id);
    }

    #[test]
    fn engine_openrouter_makes_every_model_a_cloud_row() {
        let plan = Plan::of(&BenchArgs {
            max_cost: Some(1.0),
            ..args(Some(EngineSlug::Openrouter), &["google/gemini-3.7-flash"])
        })
        .expect("a capped cloud run");

        assert_eq!(plan.rows[0].slug, EngineSlug::Openrouter);
    }

    #[test]
    fn the_command_line_repeats_every_flag_so_the_file_can_be_reproduced() {
        assert_eq!(
            command_line(&args(None, &[])),
            "grammachy bench --thinking on"
        );
        assert_eq!(
            command_line(&args(Some(EngineSlug::Openai), &["qwen2.5-7b-instruct", "qwen2.5-3b-instruct"])),
            "grammachy bench --engine openai --model qwen2.5-7b-instruct --model qwen2.5-3b-instruct --thinking on"
        );
        assert_eq!(
            command_line(&cloud(&["gemma-4-e4b-it"], &["google/gemini-3.7-flash"], Some(10.0))),
            "grammachy bench --engine openai --model gemma-4-e4b-it --cloud-model google/gemini-3.7-flash --max-cost 10 --thinking on"
        );
        assert_eq!(
            command_line(&BenchArgs {
                thinking: BenchThinking::Both,
                ..args(None, &["gemma-4-e4b-it"])
            }),
            "grammachy bench --engine openai --model gemma-4-e4b-it --thinking both"
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
    fn an_unpriced_cloud_answer_ends_the_cloud_rows_and_keeps_what_they_spent() {
        let mut spend = Spend::new(Some(10.0));
        spend.add(Some(0.03));
        spend.add(None);

        assert!(
            !spend.would_exceed(),
            "an unpriced answer cannot move the cap, which is why the row must end"
        );
        let outcome = spend.exhaust(unpriced("zh-01"));

        let Outcome::Skipped(why) = outcome else {
            panic!("an unpriced answer ends the row");
        };
        assert_eq!(
            why,
            "the answer for fixture sentence zh-01 carried no usage.cost, so this run cannot measure its spend"
        );
        assert_eq!(spend.exhausted.as_deref(), Some(why.as_str()));
        assert!((spend.spent_usd() - 0.03).abs() < 1e-9);
    }

    #[test]
    fn a_record_directory_that_cannot_hold_the_file_is_refused_before_any_row_runs() {
        let taken = std::env::temp_dir().join(format!(
            "grammachy-bench-record-{}-{}",
            std::process::id(),
            "file"
        ));
        std::fs::write(&taken, "not a directory").expect("the scratch file is written");

        let message = Plan::of(&BenchArgs {
            record: Some(taken.clone()),
            ..args(None, &[])
        })
        .expect_err("a file cannot hold the record");

        let _ = std::fs::remove_file(&taken);
        assert!(message.starts_with("--record: "), "{message}");
    }

    #[test]
    fn a_record_directory_is_proved_writable_without_discarding_the_last_run() {
        let directory = std::env::temp_dir().join(format!(
            "grammachy-bench-record-{}-{}",
            std::process::id(),
            "dir"
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the scratch directory is made");
        let earlier = directory.join(RECORD_FILE);
        std::fs::write(&earlier, EARLIER_RUN).expect("an earlier run left its record");

        let plan = Plan::of(&BenchArgs {
            record: Some(directory.clone()),
            ..args(None, &[])
        })
        .expect("the directory holds the record");

        assert_eq!(plan.record, Some(earlier.clone()));
        assert_eq!(
            std::fs::read_to_string(&earlier).expect("the earlier record is still there"),
            EARLIER_RUN,
            "a run that never reaches a row must not discard the last one's answers"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_record_file_is_replaced_only_once_the_new_one_is_whole() {
        let directory = std::env::temp_dir().join(format!(
            "grammachy-bench-record-{}-{}",
            std::process::id(),
            "replace"
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let path = open_record(&directory).expect("the directory holds the record");
        std::fs::write(&path, EARLIER_RUN).expect("an earlier run left its record");

        record(&path, &[recorded_check("zh-02")]).expect("the record is written");

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("the record is there"))
                .expect("the record is JSON");
        assert_eq!(written[0]["id"], "zh-02");
        assert!(
            !pending_path(&path).exists(),
            "the pending file is renamed, not left behind"
        );
        let _ = std::fs::remove_dir_all(&directory);
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

        let options = base_options(&stored, BenchThinking::On);

        assert_eq!(options.openai_base_url, "http://127.0.0.1:9999");
        assert_eq!(options.engine, CheckOptions::default().engine);
    }

    /// Evals spec section 4.1: `both` prints one local model twice, and the
    /// two modes stay adjacent so they share one llama.cpp server start.
    #[test]
    fn thinking_both_runs_every_local_row_twice_and_leaves_cloud_rows_alone() {
        let plan = Plan::of(&BenchArgs {
            thinking: BenchThinking::Both,
            ..cloud(
                &["gemma-4-e4b-it", "qwen3.5-4b"],
                &["google/gemini-3.7-flash"],
                Some(1.0),
            )
        })
        .expect("a capped cloud run");

        assert_eq!(
            plan.rows,
            [
                row(EngineSlug::Openai, "gemma-4-e4b-it", Some(true)),
                row(EngineSlug::Openai, "gemma-4-e4b-it", Some(false)),
                row(EngineSlug::Openai, "qwen3.5-4b", Some(true)),
                row(EngineSlug::Openai, "qwen3.5-4b", Some(false)),
                cloud_row("google/gemini-3.7-flash"),
            ]
        );
    }

    #[test]
    fn thinking_off_runs_every_local_row_once_in_that_mode() {
        let plan = Plan::of(&BenchArgs {
            thinking: BenchThinking::Off,
            ..args(None, &["gemma-4-e4b-it"])
        })
        .expect("a local run");

        assert_eq!(
            plan.rows,
            [row(EngineSlug::Openai, "gemma-4-e4b-it", Some(false))]
        );
    }

    /// The flag decides the request, so the file a run prints is the output of
    /// its own Command line rather than of a Setting the reader cannot see.
    #[test]
    fn the_flag_and_not_the_stored_setting_decides_what_a_row_thinks() {
        let stored = StoredSettings {
            local_thinking: Some(false),
            ..StoredSettings::default()
        };
        let base = base_options(&stored, BenchThinking::Both);

        assert!(
            base.local_thinking,
            "the Engines openai row runs in the product default under --thinking both"
        );
        assert!(local("gemma-4-e4b-it").options(&base).local_thinking);
        assert!(
            !row(EngineSlug::Openai, "gemma-4-e4b-it", Some(false))
                .options(&base)
                .local_thinking
        );
    }

    /// Evals spec section 4.4: `judge.py` grades the product default alone, so
    /// the Rust half must not offer it a thinking-off answer.
    #[test]
    fn a_thinking_off_check_is_never_a_judged_hit() {
        let mut check = recorded_check("zh-01");
        check.engine = "openai".to_string();
        check.model = "gemma-4-e4b-it".to_string();
        check.edits = vec![fixture::Edit {
            start: 4,
            end: 7,
            text: "has".to_string(),
            fix: "is".to_string(),
            kind: String::new(),
        }];
        check.issues = vec![Issue {
            start: 4,
            end: 7,
            original: "has".to_string(),
            fix: "is".to_string(),
            reason: "Be for age.".to_string(),
            category: crate::envelope::Category::Grammar,
            rule_id: None,
        }];

        check.thinking = Some(true);
        assert!(check.hit().is_some(), "the product default is judged");

        check.thinking = Some(false);
        assert!(check.non_exact_hit(), "the answer is still a non-exact hit");
        assert!(check.hit().is_none(), "but it is out of the judge's sample");
    }

    /// A record entry per mode, so `--thinking both` never folds two rows onto
    /// one key (evals spec section 4.3).
    #[test]
    fn the_record_keeps_one_entry_per_thinking_mode() {
        let mut on = recorded_check("zh-01");
        on.engine = "openai".to_string();
        on.model = "gemma-4-e4b-it".to_string();
        on.thinking = Some(true);
        let mut off = on.clone();
        off.thinking = Some(false);

        let both = [on, off];
        let kept = one_per_item(&both);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].thinking, Some(true));
        assert_eq!(kept[1].thinking, Some(false));
    }

    #[test]
    fn only_the_local_llm_engine_records_a_thinking_mode() {
        let options = CheckOptions {
            local_thinking: false,
            ..CheckOptions::default()
        };

        assert_eq!(row_thinking(EngineSlug::Openai, &options), Some(false));
        assert_eq!(row_thinking(EngineSlug::Harper, &options), None);
        assert_eq!(row_thinking(EngineSlug::Languagetool, &options), None);
        assert_eq!(row_thinking(EngineSlug::Openrouter, &options), None);
    }

    #[test]
    fn a_progress_line_names_the_mode_of_the_row_it_belongs_to() {
        assert_eq!(
            row_label(EngineSlug::Openai, "gemma-4-e4b-it", Some(true)),
            "openai gemma-4-e4b-it (thinking on)"
        );
        assert_eq!(row_label(EngineSlug::Harper, "harper", None), "harper");
    }
}
