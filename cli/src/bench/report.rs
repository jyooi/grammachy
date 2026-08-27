//! The Markdown report `grammachy bench` prints, `docs/spec/evals.md` section 5.
//!
//! The output of one run is the whole benchmark file: `grammachy bench > docs/
//! benchmarks/<version>.md` is how a release records its numbers. So this
//! module renders the tables, the skipped engines, the regression rule, and the
//! note on how each number was measured, and nothing is added by hand
//! afterwards.
//!
//! An engine or a model the machine cannot reach is a row that says `skipped`
//! plus a reason under the table. It is never an error: a machine without
//! llama.cpp still produces a valid benchmark file for the engines it has.
//!
//! The Models section is three tables (HUF-205): Quality, Cost, and Recall by
//! native language. The recommendation is decided here, from the rows: the
//! best eligible local row by exact fix rate, then F0.5, then lower p50, above
//! the validity floor and never with more false positives than the default
//! engine. Cloud rows compete only for the separate cloud line.

use crate::bench::judge::{Assessment, RowKey, AGREEMENT_GATE, MINIMUM_LABELLED};
use crate::bench::machine::Machine;
use crate::bench::memory::Reading;
use crate::bench::metrics::Tally;
use crate::bench::mode_word;
use crate::bench::weights::{Terms, Weights};

/// What one row prints in every measured column when the row did not run.
const SKIPPED: &str = "skipped";

/// The validity a row needs to be recommended, in percent.
const VALIDITY_FLOOR: f64 = 95.0;

/// The numbers of one row that ran.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub tally: Tally,
    /// Resident memory, and where the run read it.
    pub memory: Reading,
    /// How long the whole row took, including any server start it paid for.
    pub wall_ms: u64,
}

/// Whether one row ran, and why it did not.
#[derive(Debug, Clone)]
pub enum Outcome {
    Measured(Box<Measurement>),
    /// The one sentence that says why the row was skipped.
    Skipped(String),
}

impl Outcome {
    fn tally(&self) -> Option<&Tally> {
        match self {
            Outcome::Measured(measurement) => Some(&measurement.tally),
            Outcome::Skipped(_) => None,
        }
    }
}

/// One row of the Engines table.
#[derive(Debug, Clone)]
pub struct EngineRow {
    pub engine: String,
    pub outcome: Outcome,
}

/// What one Models row paid for the llama.cpp server behind its numbers.
///
/// Two rows of one model share one start, so only the first of them carries
/// the weight load in its wall time. A run that starts no unit at all pays
/// for neither, so neither row may claim a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerStart {
    /// This row stopped the unit, so its wall time carries the weight load.
    Paid,
    /// An earlier row of this model paid for the start this row measured on.
    Reused,
    /// No start happened, because this is a cloud row or unit starts are off.
    None,
}

impl ServerStart {
    /// What the wall time sentence adds about the server behind the row.
    fn wall_time_note(self) -> &'static str {
        match self {
            ServerStart::Paid => ", server start included",
            ServerStart::Reused => ", on the server an earlier row of this model started",
            ServerStart::None => "",
        }
    }
}

/// One row of the Models tables.
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub model: String,
    /// The engine the row ran on, `openai` or `openrouter`.
    pub engine: String,
    /// The local thinking mode the row ran in, `None` for a cloud row.
    pub thinking: Option<bool>,
    /// What this row paid for the llama.cpp server it measured on.
    pub server_start: ServerStart,
    pub weights: Weights,
    pub outcome: Outcome,
}

impl ModelRow {
    fn is_cloud(&self) -> bool {
        self.weights.terms == Terms::Hosted
    }

    /// The Thinking cell of the Cost table (evals spec section 3).
    fn thinking_cell(&self) -> String {
        match self.thinking {
            Some(on) => mode_word(on).to_string(),
            None => "-".to_string(),
        }
    }

    /// The key this row's Useful fix count is filed under.
    fn key(&self) -> RowKey {
        RowKey {
            engine: self.engine.clone(),
            model: self.model.clone(),
            thinking: self.thinking,
        }
    }

    /// Whether the judge left this row out of its sample by rule rather than
    /// by chance: `judge.py` grades the product default alone (section 4.4).
    fn out_of_the_judged_sample(&self) -> bool {
        self.thinking == Some(false)
    }
}

/// Everything one run of the benchmark found.
#[derive(Debug, Clone)]
pub struct Report {
    pub version: String,
    pub machine: Machine,
    /// The command that produced this file, so it can be run again.
    pub command: String,
    pub interference: usize,
    pub clean: usize,
    /// The native languages of the interference sentences, in fixture order.
    pub languages: Vec<String>,
    /// The engine a release is measured against (spec section 7).
    pub default_engine: String,
    pub max_cost: Option<f64>,
    /// What the run paid the cloud engine, summed over the answers that
    /// reported a cost.
    ///
    /// An answer with no `usage.cost` ends the cloud rows and stays out of this
    /// sum, so the figure is a lower bound on what the run paid.
    pub cloud_spend_usd: f64,
    pub engines: Vec<EngineRow>,
    pub models: Vec<ModelRow>,
    /// What `--judgements` said about this run, absent without the flag.
    pub judge: Option<Assessment>,
}

impl Report {
    /// The whole benchmark file, as Markdown.
    pub fn render(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!("# Grammachy benchmark {}\n\n", self.version));
        out.push_str(&format!(
            "Fixture: {} interference sentences and {} correct ones, `cli/tests/fixtures/interference-30.json`.\n",
            self.interference, self.clean
        ));
        out.push_str(&format!("Machine: {}.\n", self.machine.line()));
        out.push_str(&format!("Command: `{}`.\n\n", self.command));

        out.push_str(&self.engines_table());
        out.push_str(&self.models_section());
        out.push_str(&self.skipped_section());
        out.push_str(&self.regression_rule());
        out.push_str(MEASUREMENT_NOTE);

        out
    }

    fn engines_table(&self) -> String {
        let mut out = String::from("## Engines\n\n");
        out.push_str("| Engine | Catch rate | False positives | p50 latency | Resident memory |\n");
        out.push_str("|---|---|---|---|---|\n");
        for row in &self.engines {
            out.push_str(&format!(
                "| `{}` | {} |\n",
                row.engine,
                engine_cells(&row.outcome).join(" | ")
            ));
        }
        out.push('\n');
        for row in &self.engines {
            out.push_str(&memory_source_line(
                &format!("`{}`", row.engine),
                &row.outcome,
            ));
        }
        out.push('\n');
        out
    }

    fn models_section(&self) -> String {
        let mut out = String::from("## Models\n\n");
        if self.models.is_empty() {
            out.push_str(
                "No model was named. Run `grammachy bench --engine openai --model <name>` (repeatable, plus `--cloud-model <id> --max-cost <usd>` for a cloud row) to fill these tables.\n\n",
            );
            return out;
        }

        let verdicts = self.verdicts();

        out.push_str("### Quality\n\n");
        let useful_column = self.judge.is_some();
        out.push_str("| Model | Catch rate | Precision | Recall | F0.5 | Exact fix |");
        if useful_column {
            out.push_str(" Useful fix |");
        }
        out.push_str(" False positives | Style creep | Valid |\n");
        out.push_str(&format!(
            "|---|{}\n",
            "---|".repeat(if useful_column { 9 } else { 8 })
        ));
        for row in &self.models {
            out.push_str(&format!(
                "| `{}` | {} |\n",
                row.model,
                self.quality_cells(row).join(" | ")
            ));
        }
        out.push('\n');
        if self.holds_both_modes() {
            out.push_str("Every Models table prints the rows in one order. A model that appears twice is its two Thinking modes, named in the Cost table below.\n\n");
        }
        if let Some(judge) = &self.judge {
            out.push_str(&judge.lines());
            out.push_str(&self.ranking_sentence());
            out.push('\n');
        }

        out.push_str("### Cost\n\n");
        out.push_str("| Model | Thinking | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |\n");
        out.push_str("|---|---|---|---|---|---|---|---|\n");
        for (row, verdict) in self.models.iter().zip(&verdicts) {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                row.model,
                row.thinking_cell(),
                cost_cells(row).join(" | "),
                row.weights.license,
                verdict
            ));
        }
        out.push('\n');
        out.push_str("Thinking is the local mode the row ran in, from `--thinking`. A cloud row prints `-`: the mode is a llama.cpp chat-template argument and never reaches a provider.\n");
        for row in &self.models {
            out.push_str(&memory_source_line(&self.row_label(row), &row.outcome));
        }
        for row in &self.models {
            if let Outcome::Measured(measurement) = &row.outcome {
                out.push_str(&format!(
                    "Wall time of {}: {} s for the whole fixture{}.\n",
                    self.row_label(row),
                    measurement.wall_ms / 1_000,
                    row.server_start.wall_time_note()
                ));
            }
        }
        if let Some(cap) = self.max_cost {
            out.push_str(&format!(
                "Cloud spend of this run: {:.4} USD of the {cap} USD cap, summed over the answers that reported a cost.\n",
                self.cloud_spend_usd
            ));
            out.push_str(
                "An answer that reported no cost stays out of that sum, so the figure is a lower bound.\n",
            );
        }
        out.push('\n');

        out.push_str("### Throughput\n\n");
        out.push_str("| Model | Time to first token (p50) | Output tokens per second | Output tokens per Check (p50) | Output tokens per Issue |\n");
        out.push_str("|---|---|---|---|---|\n");
        for row in &self.models {
            out.push_str(&format!(
                "| `{}` | {} |\n",
                row.model,
                throughput_cells(&row.outcome).join(" | ")
            ));
        }
        out.push_str("\nTime to first token and the token rate come from the model server's own timings. A rate marked `whole request` is output tokens over the request time as seen from this machine, network included, because the provider reports no timings.\n");
        out.push_str("Output tokens per Issue is the output tokens of the row over the Issues the same Checks answered, so it prices one Issue rather than one Check.\n\n");

        out.push_str("### Recall by native language\n\n");
        out.push_str(&format!("| Model | {} |\n", self.languages.join(" | ")));
        out.push_str(&format!("|---|{}\n", "---|".repeat(self.languages.len())));
        for row in &self.models {
            let cells: Vec<String> = match row.outcome.tally() {
                Some(tally) => self
                    .languages
                    .iter()
                    .map(|language| tally.language_cell(language))
                    .collect(),
                None => vec![SKIPPED.to_string(); self.languages.len()],
            };
            out.push_str(&format!("| `{}` | {} |\n", row.model, cells.join(" | ")));
        }
        out.push('\n');

        out.push_str(&self.recommendation_lines(&verdicts));
        out
    }

    /// The name the prose under the tables gives one row.
    ///
    /// `--thinking both` prints one model twice, and the Model column of every
    /// table is the bare name (evals spec section 4.2), so a line that names a
    /// row on its own has to say which of the two it means.
    fn row_label(&self, row: &ModelRow) -> String {
        let shared = self
            .models
            .iter()
            .filter(|other| other.model == row.model)
            .count()
            > 1;
        match (shared, row.thinking) {
            (true, Some(on)) => format!("`{}` with thinking {}", row.model, mode_word(on)),
            _ => format!("`{}`", row.model),
        }
    }

    /// The measured cells of one Quality row, in table order.
    ///
    /// The Useful fix cell is present only with `--judgements`, and it prints
    /// whether or not the gate passed: below the gate the column is still the
    /// measurement, it just does not rank. A thinking-off row says so instead
    /// of a count, because the judge grades the product default alone and a
    /// bare `no non-exact hit` would read as a row that produced none.
    fn quality_cells(&self, row: &ModelRow) -> Vec<String> {
        let width = if self.judge.is_some() { 9 } else { 8 };
        let Outcome::Measured(measurement) = &row.outcome else {
            return vec![SKIPPED.to_string(); width];
        };
        let tally = &measurement.tally;
        let mut cells = vec![
            tally.catch_rate_cell(),
            tally.precision_cell(),
            tally.recall_cell(),
            tally.f05_cell(),
            tally.exact_cell(),
        ];
        if let Some(judge) = &self.judge {
            cells.push(match row.out_of_the_judged_sample() {
                true => "not the product default".to_string(),
                false => judge.row(&row.key()).unwrap_or_default().cell(),
            });
        }
        cells.push(tally.false_positive_cell());
        cells.push(tally.creep_cell());
        cells.push(tally.validity_cell());
        cells
    }

    /// The measured rows that produced a non-exact hit the file grades none of.
    ///
    /// The swapped measure adds useful non-exact hits to exact fixes, so a row
    /// the file grades no hit of competes on a strictly smaller measure than a
    /// graded row. A judgements file comes from an earlier run, whose answers
    /// this run need not have repeated, so that gap is coverage rather than
    /// quality and the whole table has to fall back rather than one row.
    ///
    /// A row that produced no non-exact hit at all was offered nothing to
    /// grade, so it is not unjudged and it never blocks the swap.
    ///
    /// A skipped row is not one of these either. A cloud row the cost cap ended
    /// keeps the Checks it already ran, so the judge input carries its hits,
    /// but the row has no tally, prints `skipped` in every column, and can
    /// never be recommended. Letting it drop the swap would decide the
    /// recommendation on a row the report did not measure.
    fn unjudged_rows(&self) -> Vec<String> {
        let Some(judge) = self.judge.as_ref() else {
            return Vec::new();
        };
        self.models
            .iter()
            .filter(|row| row.outcome.tally().is_some())
            .filter(|row| {
                judge
                    .row(&row.key())
                    .is_some_and(|graded| graded.hits > 0 && graded.judged == 0)
            })
            .map(|row| self.row_label(row))
            .collect()
    }

    /// How many measured Models rows the judgements file graded a hit of.
    fn judged_rows(&self) -> usize {
        let Some(judge) = self.judge.as_ref() else {
            return 0;
        };
        self.models
            .iter()
            .filter(|row| row.outcome.tally().is_some())
            .filter(|row| {
                judge
                    .row(&row.key())
                    .is_some_and(|graded| graded.judged > 0)
            })
            .count()
    }

    /// Whether the judgements file covers the measured rows well enough to
    /// rank on, the second condition of the ranking swap.
    ///
    /// It covers them when no measured row is unjudged and at least one
    /// measured row carries a judged hit. Without the second half the swap is
    /// vacuously true for a run whose every Models row was skipped: the
    /// Engines rows still feed the judge, so the gate can pass, and the file
    /// would name a ranking measure that ranked nothing at all.
    fn judge_covers_measured_rows(&self) -> bool {
        self.unjudged_rows().is_empty() && self.judged_rows() > 0
    }

    /// What one row is ranked on, in percent of its interference sentences.
    ///
    /// Exact fix rate, unless the judge cleared the gate of spec section 4.4
    /// and the file covers the measured rows. Then it is exact fix plus the
    /// non-exact hits the judge called useful, because both are answers the
    /// writer can accept and keep.
    fn rank_score(&self, row: &ModelRow) -> f64 {
        let Some(tally) = row.outcome.tally() else {
            return 0.0;
        };
        let exact = tally.exact_rate_percent();
        let Some(judge) = self
            .judge
            .as_ref()
            .filter(|judge| judge.ranks() && self.judge_covers_measured_rows())
        else {
            return exact;
        };
        if tally.interference == 0 {
            return exact;
        }
        let useful = judge.row(&row.key()).unwrap_or_default().useful;
        100.0 * (tally.exact + useful) as f64 / tally.interference as f64
    }

    /// The one sentence that says whether the Useful fix column ranks.
    ///
    /// `Assessment::lines` reports what the judge measured and stops there, so
    /// this is the only claim the file makes about the ranking. It reads the
    /// same two conditions `rank_score` and `ranking_measure` do, which is what
    /// keeps the three paragraphs from disagreeing.
    fn ranking_sentence(&self) -> String {
        let Some(judge) = self.judge.as_ref() else {
            return String::new();
        };
        let mut out = if judge.ranks() && self.judge_covers_measured_rows() {
            String::from("The Useful fix column counts in the ranking.\n")
        } else {
            self.no_ranking_sentence(judge)
        };
        if self.has_unjudged_mode() {
            out.push_str(
                "A thinking-off row is never judged, so no Useful fix count reaches its ranking.\n",
            );
        }
        out
    }

    /// Why the Useful fix column stays out of the ranking of this run.
    fn no_ranking_sentence(&self, judge: &Assessment) -> String {
        let reason = if judge.labelled == 0 {
            "no hand label covers a hit of this run".to_string()
        } else if judge.labelled < MINIMUM_LABELLED {
            format!("this run matched under the {MINIMUM_LABELLED} hand labels the gate needs")
        } else if !judge.ranks() {
            format!("the judge is under the {AGREEMENT_GATE:.0}% gate")
        } else if !self.unjudged_rows().is_empty() {
            format!(
                "the judgements file covers no non-exact hit of {}",
                self.unjudged_rows().join(", ")
            )
        } else {
            "the judgements file covers no measured model row".to_string()
        };
        format!("The Useful fix column does not count in the ranking, because {reason}.\n")
    }

    /// Whether the run measured a row the judge leaves out of its sample.
    ///
    /// Under the swapped measure such a row competes on exact fix while a
    /// graded row adds its useful hits, so the file says so rather than
    /// letting the reader assume the two rows were ranked alike.
    fn has_unjudged_mode(&self) -> bool {
        self.models
            .iter()
            .any(|row| row.outcome.tally().is_some() && row.out_of_the_judged_sample())
    }

    /// Whether the run holds both modes of at least one local model.
    fn holds_both_modes(&self) -> bool {
        self.models.iter().any(|row| {
            self.models
                .iter()
                .any(|other| other.model == row.model && other.thinking != row.thinking)
        })
    }

    /// The Recommended cell of every row, in row order.
    fn verdicts(&self) -> Vec<String> {
        let default_fp = self.default_engine_false_positives();
        let local_winner = self.winner(false, default_fp);
        let cloud_winner = self.winner(true, default_fp);

        self.models
            .iter()
            .enumerate()
            .map(|(index, row)| {
                if Some(index) == local_winner {
                    return "recommended".to_string();
                }
                if Some(index) == cloud_winner {
                    return "recommended cloud model".to_string();
                }
                match self.objection(row, default_fp) {
                    Some(why) => why,
                    None => "eligible".to_string(),
                }
            })
            .collect()
    }

    /// Why one row cannot be recommended, or `None` when it can.
    fn objection(&self, row: &ModelRow, default_fp: Option<usize>) -> Option<String> {
        if let Some(why) = row.weights.objection() {
            return Some(why.to_string());
        }
        let Some(tally) = row.outcome.tally() else {
            return Some("no, the row was skipped".to_string());
        };
        if tally.validity_percent() < VALIDITY_FLOOR {
            return Some(format!("no, validity under {VALIDITY_FLOOR:.0}%"));
        }
        if default_fp.is_some_and(|floor| tally.false_positives > floor) {
            return Some(format!(
                "no, more false positives than `{}`",
                self.default_engine
            ));
        }
        None
    }

    /// The index of the best eligible row of one kind, local or cloud.
    fn winner(&self, cloud: bool, default_fp: Option<usize>) -> Option<usize> {
        self.models
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_cloud() == cloud)
            .filter(|(_, row)| self.objection(row, default_fp).is_none())
            .filter_map(|(index, row)| row.outcome.tally().map(|tally| (index, row, tally)))
            .max_by(|(_, a_row, a), (_, b_row, b)| {
                self.rank_score(a_row)
                    .total_cmp(&self.rank_score(b_row))
                    .then(a.f05_percent().total_cmp(&b.f05_percent()))
                    .then(b.p50_ms.cmp(&a.p50_ms))
            })
            .map(|(index, _, _)| index)
    }

    /// The false positives of the default engine, when it was measured.
    fn default_engine_false_positives(&self) -> Option<usize> {
        self.engines
            .iter()
            .find(|row| row.engine == self.default_engine)
            .and_then(|row| row.outcome.tally())
            .map(|tally| tally.false_positives)
    }

    fn recommendation_lines(&self, verdicts: &[String]) -> String {
        let mut out = String::new();
        let default_fp = self.default_engine_false_positives();
        let named = |verdict: &str| -> Option<&ModelRow> {
            self.models
                .iter()
                .zip(verdicts)
                .find(|(_, cell)| cell.as_str() == verdict)
                .map(|(row, _)| row)
        };
        // The README names the mode the winning row ran under (evals spec
        // section 5), so the local line always carries it.
        match named("recommended") {
            Some(row) => out.push_str(&format!(
                "Recommended local model, the Settings default and the README line: `{}`, with thinking {}.\n",
                row.model,
                match row.thinking {
                    Some(on) => mode_word(on),
                    None => "on",
                }
            )),
            None => out.push_str("No local row is eligible for the recommendation.\n"),
        }
        match named("recommended cloud model") {
            Some(row) => out.push_str(&format!(
                "Recommended cloud model, the `openrouterModel` line of the README: `{}`. Cloud is never the default engine.\n",
                row.model
            )),
            None if self.models.iter().any(ModelRow::is_cloud) => {
                out.push_str("No cloud row is eligible for the cloud recommendation.\n")
            }
            None => {}
        }
        out.push_str(&format!(
            "Ranking: {}, then F0.5, then lower p50 (HUF-205). Floors: validity at least {VALIDITY_FLOOR:.0}% and no more false positives than the default engine, `{}`{}. A recommended local model must also fit the machine tier above (`docs/spec/evals.md` section 5).\n\n",
            self.ranking_measure(),
            self.default_engine,
            match default_fp {
                Some(fp) => format!(", which earned {fp}"),
                None => ", which was skipped in this run, so that floor was not applied".to_string(),
            }
        ));
        out
    }

    /// What the ranking is measured on, named in the file.
    fn ranking_measure(&self) -> &'static str {
        match self.judge.as_ref().is_some_and(Assessment::ranks) && self.judge_covers_measured_rows() {
            true => "exact fix rate plus the non-exact hits the judge called useful, over the interference sentences",
            false => "exact fix rate",
        }
    }

    fn skipped_section(&self) -> String {
        let mut reasons: Vec<String> = Vec::new();
        for row in &self.engines {
            if let Outcome::Skipped(why) = &row.outcome {
                reasons.push(format!("- Engine `{}`: {why}\n", row.engine));
            }
        }
        for row in &self.models {
            if let Outcome::Skipped(why) = &row.outcome {
                reasons.push(format!("- Model {}: {why}\n", self.row_label(row)));
            }
        }

        let mut out = String::from("## Skipped\n\n");
        if reasons.is_empty() {
            out.push_str("Every engine and model of this run was reachable.\n\n");
            return out;
        }
        out.push_str("These rows did not run on this machine. A row that is not reachable is skipped, never an error.\n\n");
        out.extend(reasons);
        out.push('\n');
        out
    }

    fn regression_rule(&self) -> String {
        format!(
            "## Regression rule\n\nA release must not drop the catch rate of the default engine, `{}`, and must not raise its false positives, against the previous file in `docs/benchmarks/`. A row that was skipped in one file and measured in the next is a new measurement, not a regression.\n\n",
            self.default_engine
        )
    }
}

/// The sentence that names where one row's memory number came from.
///
/// A row that ran names its own source rather than a rule read off the engine,
/// because a llama.cpp row on a graphics device and one on the CPU are the same
/// engine and two different numbers. A skipped row measured nothing, so it has
/// no source to name and prints no line. `name` arrives quoted, because a
/// Models row may name its thinking mode outside the backticks.
fn memory_source_line(name: &str, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Skipped(_) => String::new(),
        Outcome::Measured(measurement) => format!(
            "Resident memory of {name} is {}.\n",
            measurement.memory.source.line()
        ),
    }
}

/// The four measured cells of one Engines row, in table order.
fn engine_cells(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Skipped(_) => vec![SKIPPED.to_string(); 4],
        Outcome::Measured(measurement) => vec![
            measurement.tally.catch_rate_cell(),
            measurement.tally.false_positive_cell(),
            format!("{} ms", measurement.tally.p50_ms),
            measurement.memory.cell(),
        ],
    }
}

/// The four measured cells of one Throughput row.
fn throughput_cells(outcome: &Outcome) -> Vec<String> {
    let Outcome::Measured(measurement) = outcome else {
        return vec![SKIPPED.to_string(); 4];
    };
    let throughput = &measurement.tally.throughput;
    let unmeasured = || "not measured".to_string();
    vec![
        throughput
            .ttft_p50_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(unmeasured),
        throughput
            .tokens_per_second
            .map(|rate| {
                if throughput.whole_request {
                    format!("{rate:.1} (whole request)")
                } else {
                    format!("{rate:.1}")
                }
            })
            .unwrap_or_else(unmeasured),
        throughput
            .output_tokens_p50
            .map(|tokens| tokens.to_string())
            .unwrap_or_else(unmeasured),
        throughput
            .tokens_per_issue
            .map(|tokens| format!("{tokens:.1}"))
            .unwrap_or_else(unmeasured),
    ]
}

/// The four measured cells of one Cost row, before the license and verdict.
fn cost_cells(row: &ModelRow) -> Vec<String> {
    match &row.outcome {
        Outcome::Skipped(_) => vec![SKIPPED.to_string(); 4],
        Outcome::Measured(measurement) => {
            let tally = &measurement.tally;
            let cost = if row.is_cloud() {
                match tally.cost_per_1000() {
                    Some(usd) => format!("{usd:.2} USD"),
                    None => "n/a".to_string(),
                }
            } else {
                "0.00 (local)".to_string()
            };
            vec![
                format!("{} ms", tally.p50_ms),
                format!("{} ms", tally.p95_ms),
                measurement.memory.cell(),
                cost,
            ]
        }
    }
}

const MEASUREMENT_NOTE: &str = "\
## How the numbers are measured

- Catch rate: an interference sentence is caught when at least one Issue overlaps a span the fixture expects. A right span with a wrong Fix still counts, because the Panel shows the span and lets the user Skip the Fix.
- Precision, recall, F0.5: an Issue pairs with the first unpaired expected edit it overlaps, provided it reaches no more than three words past the edit on either side. Precision is pairs over Issues, recall is pairs over expected edits, both over the whole fixture.
- Exact fix: every Fix of the Check applied to the sentence equals the corrected sentence the fixture holds, after collapsing runs of whitespace.
- False positives: correct sentences that earned at least one Issue. One sentence counts once, however many Issues it earned.
- Style creep: unpaired Issues on interference sentences, per 100 interference sentences.
- Valid: Checks that returned a result. An invalid Check counts as zero Issues, so a miss, and stays out of precision, exact fix, and latency.
- p50 and p95 latency: nearest rank over the valid Checks of the fixture, correct sentences included, measured in process around one Check.
- Cost per 1,000 Checks: the sum of `usage.cost` over the row divided by the number of Checks that reported a cost, times 1,000. A cloud answer that reports no cost ends its row as skipped, because the run cannot then measure what it spends. A cloud row where no Check answered prints `n/a`. Local rows cost nothing per Check.
- Every sentence is checked with the Native language the fixture records for it, which is what the shell passes on a real Check.
- Thinking: the mode `--thinking` gave the local rows. `both` runs every local model twice, once in each mode. The Engines table's `openai` row runs once, in the mode the flag names, and under `both` in the product default. A cloud row prints `-`.
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::memory::Source;
    use crate::bench::weights;

    fn tally(caught: usize, exact: usize, false_positives: usize, valid: usize) -> Tally {
        Tally {
            interference: 30,
            caught,
            clean: 10,
            false_positives,
            issues: 30,
            pairs: caught,
            edits: 30,
            exact,
            creep_issues: 2,
            checks: 40,
            valid,
            p50_ms: 20,
            p95_ms: 50,
            cost_usd: 0.0008,
            priced: 40,
            misses: vec!["zh-01".to_string()],
            ..Tally::default()
        }
    }

    fn measured(tally: Tally, memory_bytes: Option<u64>) -> Outcome {
        on_device(tally, memory_bytes, Source::ServerRss)
    }

    fn on_device(tally: Tally, memory_bytes: Option<u64>, source: Source) -> Outcome {
        Outcome::Measured(Box::new(Measurement {
            tally,
            memory: Reading::new(memory_bytes, source),
            wall_ms: 12_000,
        }))
    }

    fn model(name: &str, engine: &str, weights: Weights, outcome: Outcome) -> ModelRow {
        let thinking = match engine {
            "openai" => Some(true),
            _ => None,
        };
        thinking_model(name, engine, thinking, weights, outcome)
    }

    fn thinking_model(
        name: &str,
        engine: &str,
        thinking: Option<bool>,
        weights: Weights,
        outcome: Outcome,
    ) -> ModelRow {
        ModelRow {
            model: name.to_string(),
            engine: engine.to_string(),
            thinking,
            server_start: if engine == "openai" {
                ServerStart::Paid
            } else {
                ServerStart::None
            },
            weights,
            outcome,
        }
    }

    /// The judge key of one Models row, the way the tests name them.
    fn key(engine: &str, model: &str) -> RowKey {
        RowKey {
            engine: engine.to_string(),
            model: model.to_string(),
            thinking: match engine {
                "openai" => Some(true),
                _ => None,
            },
        }
    }

    fn report() -> Report {
        Report {
            version: "0.1.0".to_string(),
            machine: Machine {
                cpus: 24,
                ram_gb: 27,
            },
            command: "grammachy bench".to_string(),
            judge: None,
            interference: 30,
            clean: 10,
            languages: vec!["zh".to_string(), "es".to_string()],
            default_engine: "languagetool".to_string(),
            max_cost: None,
            cloud_spend_usd: 0.0,
            engines: vec![
                EngineRow {
                    engine: "languagetool".to_string(),
                    outcome: measured(tally(10, 5, 0, 40), Some(731_000_000)),
                },
                EngineRow {
                    engine: "openai".to_string(),
                    outcome: Outcome::Skipped("llama.cpp is not installed.".to_string()),
                },
            ],
            models: vec![model(
                "qwen2.5-3b-instruct",
                "openai",
                weights::of("qwen2.5-3b-instruct"),
                Outcome::Skipped("llama.cpp is not installed.".to_string()),
            )],
        }
    }

    #[test]
    fn a_measured_engine_carries_every_number_of_the_row() {
        let rendered = report().render();

        assert!(
            rendered.contains("| `languagetool` | 10 of 30 (33.3%) | 0 of 10 | 20 ms | 731 MB |"),
            "{rendered}"
        );
    }

    #[test]
    fn an_unreachable_engine_is_skipped_rather_than_an_error() {
        let rendered = report().render();

        assert!(
            rendered.contains("| `openai` | skipped | skipped | skipped | skipped |"),
            "{rendered}"
        );
        assert!(
            rendered.contains("- Engine `openai`: llama.cpp is not installed."),
            "{rendered}"
        );
    }

    /// Evals spec sections 3 and 4.1: the Cost table is where a local row names
    /// its mode, and a run that holds both says which of two like-named rows a
    /// line under the table means.
    #[test]
    fn both_modes_of_one_model_are_two_rows_the_file_can_tell_apart() {
        let mut report = report();
        report.models = vec![
            thinking_model(
                "gemma-4-e4b-it",
                "openai",
                Some(true),
                weights::of("gemma-4-e4b-it"),
                measured(tally(20, 12, 0, 40), Some(5_000_000_000)),
            ),
            thinking_model(
                "gemma-4-e4b-it",
                "openai",
                Some(false),
                weights::of("gemma-4-e4b-it"),
                measured(tally(18, 8, 0, 40), Some(5_000_000_000)),
            ),
            thinking_model(
                "google/gemini-3.7-flash",
                "openrouter",
                None,
                weights::HOSTED,
                measured(tally(25, 20, 0, 40), None),
            ),
        ];

        report.models[1].server_start = ServerStart::Reused;

        let rendered = report.render();

        assert!(
            rendered.contains("| `gemma-4-e4b-it` | on | 20 ms |"),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `gemma-4-e4b-it` | off | 20 ms |"),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `google/gemini-3.7-flash` | - | 20 ms |"),
            "a cloud row names no mode: {rendered}"
        );
        assert!(
            rendered.contains("A model that appears twice is its two Thinking modes"),
            "the Quality table says why one name is on two rows: {rendered}"
        );
        assert!(
            rendered.contains(
                "Wall time of `gemma-4-e4b-it` with thinking on: 12 s for the whole fixture, server start included.\n"
            ),
            "the row that started the server says so: {rendered}"
        );
        assert!(
            rendered.contains(
                "Wall time of `gemma-4-e4b-it` with thinking off: 12 s for the whole fixture, on the server an earlier row of this model started.\n"
            ),
            "the row that reused the server never claims the start: {rendered}"
        );
        assert!(
            rendered
                .contains("Wall time of `google/gemini-3.7-flash`: 12 s for the whole fixture.\n"),
            "one row of a name needs no mode and a cloud row starts no server: {rendered}"
        );
        assert!(
            rendered.contains(
                "Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`, with thinking on."
            ),
            "the README line names the winning row's mode: {rendered}"
        );
    }

    /// The judge grades the product default alone, so a thinking-off row says
    /// so rather than printing a count no file could hold.
    #[test]
    fn a_thinking_off_row_says_it_is_out_of_the_judged_sample() {
        let mut report = judged_report(4);
        report.models.push(thinking_model(
            "phi-4-mini-instruct",
            "openai",
            Some(false),
            weights::of("phi-4-mini-instruct"),
            measured(tally(20, 12, 0, 40), Some(3_000_000_000)),
        ));

        let rendered = report.render();

        assert!(
            rendered.contains("| not the product default |"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "A thinking-off row is never judged, so no Useful fix count reaches its ranking.\n"
            ),
            "the file says the two rows were not ranked alike: {rendered}"
        );
    }

    /// The disclosure belongs to the row, not to the swap: a run whose judge is
    /// under the gate still prints a Useful fix column that skips one row.
    #[test]
    fn a_thinking_off_row_is_disclosed_even_when_the_judge_does_not_rank() {
        let row = |report: &mut Report| {
            report.models.push(thinking_model(
                "phi-4-mini-instruct",
                "openai",
                Some(false),
                weights::of("phi-4-mini-instruct"),
                measured(tally(20, 12, 0, 40), Some(3_000_000_000)),
            ));
        };
        let disclosure =
            "A thinking-off row is never judged, so no Useful fix count reaches its ranking.\n";

        let mut under_the_gate = judged_report(0);
        row(&mut under_the_gate);
        let rendered = under_the_gate.render();

        assert!(
            rendered.contains("The Useful fix column does not count in the ranking, because"),
            "this run's judge does not rank: {rendered}"
        );
        assert!(
            rendered.contains(disclosure),
            "the disclosure prints whether or not the swap is active: {rendered}"
        );

        let mut every_row_judged = judged_report(4);
        let rendered = every_row_judged.render();

        assert!(
            !rendered.contains(disclosure),
            "a run with no thinking-off row discloses nothing: {rendered}"
        );
        row(&mut every_row_judged);

        assert!(
            every_row_judged.render().contains(disclosure),
            "the swapped measure discloses the row too"
        );
    }

    #[test]
    fn a_model_with_non_commercial_weights_is_shown_and_marked_never_recommended() {
        let rendered = report().render();

        assert!(
            rendered.contains(
                "| `qwen2.5-3b-instruct` | on | skipped | skipped | skipped | skipped | Qwen Research License | never, the weights are non-commercial |"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `qwen2.5-3b-instruct` | skipped | skipped | skipped | skipped | skipped | skipped | skipped | skipped |"),
            "the Quality row is skipped too: {rendered}"
        );
    }

    #[test]
    fn the_file_names_the_machine_tier_and_the_regression_rule() {
        let rendered = report().render();

        assert!(
            rendered.contains("Machine: 16 GB tier, 24 CPUs, 27 GB RAM."),
            "{rendered}"
        );
        assert!(
            rendered.contains("must not drop the catch rate of the default engine, `languagetool`"),
            "{rendered}"
        );
    }

    #[test]
    fn a_run_with_no_model_says_how_to_fill_the_models_table() {
        let mut report = report();
        report.models.clear();

        let rendered = report.render();

        assert!(rendered.contains("## Models"), "{rendered}");
        assert!(
            rendered.contains("--engine openai --model <name>"),
            "{rendered}"
        );
    }

    #[test]
    fn a_measured_engine_names_the_pool_its_memory_number_came_from() {
        let mut report = report();
        report.engines[0].outcome = Outcome::Skipped("LanguageTool is not installed.".to_string());
        report.engines[1].outcome =
            on_device(tally(22, 18, 1, 40), Some(1_800_000_000), Source::Device);

        let rendered = report.render();

        assert!(
            rendered.contains("| `openai` | 22 of 30 (73.3%) | 1 of 10 | 20 ms | 1.8 GB |"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "Resident memory of `openai` is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.\n"
            ),
            "the row names the pool it was measured in: {rendered}"
        );
        assert!(
            !rendered.contains("Resident memory of `languagetool`"),
            "a skipped row measured nothing, so it names no source: {rendered}"
        );
    }

    #[test]
    fn an_engine_on_an_integrated_processor_names_the_shared_pool_instead() {
        let mut report = report();
        report.engines[1].outcome = on_device(
            tally(22, 18, 1, 40),
            Some(1_800_000_000),
            Source::DeviceShared,
        );

        let rendered = report.render();

        assert!(
            rendered.contains("Resident memory of `openai` is the system memory its server process maps onto an integrated graphics processor, read from the DRM fdinfo of that process rather than from its RSS.\n"),
            "{rendered}"
        );
    }

    #[test]
    fn every_measured_model_names_its_own_pool_under_the_cost_table() {
        let mut report = report();
        report.models = vec![
            model(
                "gemma-3n-e4b-it",
                "openai",
                weights::of("gemma-3n-e4b-it"),
                on_device(tally(24, 20, 0, 40), Some(1_800_000_000), Source::Device),
            ),
            model(
                "qwen2.5-3b-instruct",
                "openai",
                weights::of("qwen2.5-3b-instruct"),
                Outcome::Skipped("llama.cpp is not installed.".to_string()),
            ),
        ];

        let rendered = report.render();

        assert!(
            rendered.contains(
                "Resident memory of `gemma-3n-e4b-it` is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.\n"
            ),
            "a measured model names its pool under the Cost table: {rendered}"
        );
        assert!(
            !rendered.contains("Resident memory of `qwen2.5-3b-instruct`"),
            "a skipped model measured nothing, so it names no source: {rendered}"
        );
    }

    #[test]
    fn a_run_that_reached_everything_says_nothing_was_skipped() {
        let mut report = report();
        report.engines[1].outcome = measured(tally(10, 5, 0, 40), None);
        report.models.clear();

        let rendered = report.render();

        assert!(
            rendered.contains("Every engine and model of this run was reachable."),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `openai` | 10 of 30 (33.3%) | 0 of 10 | 20 ms | not measured |"),
            "{rendered}"
        );
    }

    #[test]
    fn the_best_eligible_local_row_is_recommended_and_cloud_rows_compete_apart() {
        let mut report = report();
        report.max_cost = Some(10.0);
        report.cloud_spend_usd = 0.0016;
        report.models = vec![
            model(
                "gemma-4-e4b-it",
                "openai",
                weights::of("gemma-4-e4b-it"),
                measured(tally(28, 20, 0, 40), Some(5_000_000_000)),
            ),
            model(
                "qwen3.5-4b",
                "openai",
                weights::of("qwen3.5-4b"),
                measured(tally(29, 25, 0, 40), Some(3_000_000_000)),
            ),
            model(
                "phi-4-mini",
                "openai",
                weights::of("phi-4-mini"),
                measured(tally(29, 27, 3, 40), Some(3_000_000_000)),
            ),
            model(
                "deepseek/deepseek-v4-flash-0731",
                "openrouter",
                weights::HOSTED,
                measured(tally(30, 29, 0, 40), None),
            ),
            model(
                "google/gemini-3.7-flash",
                "openrouter",
                weights::HOSTED,
                measured(tally(30, 28, 0, 37), None),
            ),
        ];

        let rendered = report.render();

        assert!(rendered.contains("| `qwen3.5-4b` | on | 20 ms | 50 ms | 3.0 GB | 0.00 (local) | Apache-2.0 | recommended |"), "{rendered}");
        assert!(rendered.contains("| `gemma-4-e4b-it` | on | 20 ms | 50 ms | 5.0 GB | 0.00 (local) | Apache-2.0 | eligible |"), "{rendered}");
        assert!(rendered.contains("| `phi-4-mini` | on | 20 ms | 50 ms | 3.0 GB | 0.00 (local) | MIT | no, more false positives than `languagetool` |"), "{rendered}");
        assert!(rendered.contains("| `deepseek/deepseek-v4-flash-0731` | - | 20 ms | 50 ms | not measured | 0.02 USD | hosted | recommended cloud model |"), "{rendered}");
        assert!(rendered.contains("| `google/gemini-3.7-flash` | - | 20 ms | 50 ms | not measured | 0.02 USD | hosted | no, validity under 95% |"), "{rendered}");
        assert!(
            rendered.contains(
                "Recommended local model, the Settings default and the README line: `qwen3.5-4b`, with thinking on."
            ),
            "{rendered}"
        );
        assert!(rendered.contains("Recommended cloud model, the `openrouterModel` line of the README: `deepseek/deepseek-v4-flash-0731`."), "{rendered}");
        assert!(
            rendered.contains("Cloud spend of this run: 0.0016 USD of the 10 USD cap, summed over the answers that reported a cost."),
            "{rendered}"
        );
        assert!(rendered.contains("| `qwen3.5-4b` | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 96.7% | 25 of 30 (83.3%) | 0 of 10 | 6.7 | 40 of 40 (100.0%) |"), "{rendered}");
    }

    #[test]
    fn the_cloud_spend_line_holds_what_a_row_the_cap_ended_already_paid() {
        let mut report = report();
        report.max_cost = Some(0.05);
        report.cloud_spend_usd = 0.049;
        report.models = vec![model(
            "deepseek/deepseek-v4-flash-0731",
            "openrouter",
            weights::HOSTED,
            Outcome::Skipped("cost cap 0.05 USD reached after 20 sentences".to_string()),
        )];

        let rendered = report.render();

        assert!(
            rendered.contains("Cloud spend of this run: 0.0490 USD of the 0.05 USD cap, summed over the answers that reported a cost."),
            "a row the cap ended carries no tally, and its spend still happened: {rendered}"
        );
        assert!(
            rendered.contains(
                "An answer that reported no cost stays out of that sum, so the figure is a lower bound."
            ),
            "{rendered}"
        );
    }

    #[test]
    fn the_throughput_table_marks_a_rate_measured_around_the_whole_request() {
        let mut report = report();
        let mut local = tally(28, 20, 0, 40);
        local.throughput = crate::bench::metrics::Throughput {
            ttft_p50_ms: Some(510),
            tokens_per_second: Some(25.3),
            whole_request: false,
            output_tokens_p50: Some(480),
            tokens_per_issue: Some(30.4),
        };
        let mut cloud = tally(30, 29, 0, 40);
        cloud.throughput = crate::bench::metrics::Throughput {
            ttft_p50_ms: None,
            tokens_per_second: Some(31.0),
            whole_request: true,
            output_tokens_p50: Some(120),
            tokens_per_issue: None,
        };
        report.models = vec![
            model(
                "gemma-4-e4b-it",
                "openai",
                weights::of("gemma-4-e4b-it"),
                measured(local, None),
            ),
            model(
                "deepseek/deepseek-v4-flash-0731",
                "openrouter",
                weights::HOSTED,
                measured(cloud, None),
            ),
        ];

        let rendered = report.render();

        assert!(
            rendered.contains("| `gemma-4-e4b-it` | 510 ms | 25.3 | 480 | 30.4 |"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "| `deepseek/deepseek-v4-flash-0731` | not measured | 31.0 (whole request) | 120 | not measured |"
            ),
            "{rendered}"
        );
    }

    #[test]
    fn a_skipped_default_engine_drops_the_false_positive_floor_and_says_so() {
        let mut report = report();
        report.engines[0].outcome = Outcome::Skipped("LanguageTool did not answer".to_string());
        report.models = vec![model(
            "gemma-4-e4b-it",
            "openai",
            weights::of("gemma-4-e4b-it"),
            measured(tally(28, 20, 4, 40), None),
        )];

        let rendered = report.render();

        assert!(
            rendered.contains("| Apache-2.0 | recommended |"),
            "{rendered}"
        );
        assert!(
            rendered.contains("which was skipped in this run, so that floor was not applied"),
            "{rendered}"
        );
    }

    #[test]
    fn the_language_table_has_one_column_per_language() {
        let mut report = report();
        let mut tally = tally(28, 20, 0, 40);
        tally.by_language.insert(
            "zh".to_string(),
            crate::bench::metrics::LanguageRecall { pairs: 7, edits: 8 },
        );
        report.models = vec![model(
            "gemma-4-e4b-it",
            "openai",
            weights::of("gemma-4-e4b-it"),
            measured(tally, None),
        )];

        let rendered = report.render();

        assert!(
            rendered.contains(
                "| Model | zh | es |\n|---|---|---|\n| `gemma-4-e4b-it` | 7 of 8 | 0 of 0 |"
            ),
            "{rendered}"
        );
    }
    /// Two rows and a judgements file, so the column, the gate line, and the
    /// ranking can be read out of one rendered file.
    fn judged_report(judge_labels_agree: usize) -> Report {
        use crate::bench::judge::{Assessment, Hit, Judgement, Judgements};

        let mut report = report();
        // `gemma` wins on exact fix, `qwen` wins once useful fixes count.
        report.models = vec![
            model(
                "gemma-4-e4b-it",
                "openai",
                weights::of("gemma-4-e4b-it"),
                measured(tally(30, 12, 0, 40), None),
            ),
            model(
                "phi-4-mini-instruct",
                "openai",
                weights::of("phi-4-mini-instruct"),
                measured(tally(30, 10, 0, 40), None),
            ),
        ];

        let entry = |useful: bool| Judgement {
            useful,
            reason: "a recorded reason".to_string(),
        };
        let mut judgements = Judgements::new();
        let mut labels = Judgements::new();
        let mut hits: Vec<Hit> = Vec::new();
        // Five hits for `phi`, four of them useful, and one for `gemma`.
        for index in 0..5 {
            let id = format!("zh-0{index}");
            let result = format!("answer {index}");
            let useful = index < 4;
            judgements
                .entry(id.clone())
                .or_default()
                .insert(result.clone(), entry(useful));
            // A hand label that agrees for the first `judge_labels_agree` of
            // them and disagrees for the rest.
            labels.entry(id.clone()).or_default().insert(
                result.clone(),
                entry(if index < judge_labels_agree {
                    useful
                } else {
                    !useful
                }),
            );
            hits.push(Hit {
                row: key("openai", "phi-4-mini-instruct"),
                id,
                result,
            });
        }
        judgements
            .entry("es-01".to_string())
            .or_default()
            .insert("one gemma answer".to_string(), entry(true));
        hits.push(Hit {
            row: key("openai", "gemma-4-e4b-it"),
            id: "es-01".to_string(),
            result: "one gemma answer".to_string(),
        });

        report.judge = Some(Assessment::of(&hits, &judgements, &labels));
        report
    }

    #[test]
    fn judgements_add_the_useful_fix_column_to_the_quality_table() {
        let rendered = judged_report(5).render();

        assert!(
            rendered.contains("| F0.5 | Exact fix | Useful fix | False positives |"),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("| `phi-4-mini-instruct` | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 100.0% | 10 of 30 (33.3%) | 4 of 5 (80.0%) |"),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `gemma-4-e4b-it` | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 100.0% | 12 of 30 (40.0%) | 1 of 1 (100.0%) |"),
            "{rendered}"
        );
    }

    /// Without the flag the table keeps its eight columns, so an old benchmark
    /// file and a new one stay comparable.
    #[test]
    fn a_run_without_judgements_prints_no_useful_fix_column() {
        let rendered = report().render();

        assert!(!rendered.contains("Useful fix"), "{rendered}");
        assert!(
            rendered.contains("| F0.5 | Exact fix | False positives |"),
            "{rendered}"
        );
    }

    #[test]
    fn a_judge_at_the_gate_ranks_and_the_file_says_so() {
        let rendered = judged_report(4).render();

        assert!(
            rendered.contains("The judge agreed with the hand labels on 4 of 5 (80.0%), at or above the 80% gate.\nThe Useful fix column counts in the ranking.\n"),
            "{rendered}"
        );
        // 10 exact plus 4 useful beats 12 exact plus 1 useful.
        assert!(
            rendered.contains("Recommended local model, the Settings default and the README line: `phi-4-mini-instruct`, with thinking on."),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate plus the non-exact hits the judge called useful, over the interference sentences, then F0.5"),
            "{rendered}"
        );
    }

    #[test]
    fn a_judge_under_the_gate_still_prints_the_column_but_never_ranks() {
        let rendered = judged_report(3).render();

        assert!(
            rendered.contains("The judge agreed with the hand labels on 3 of 5 (60.0%), under the 80% gate.\nThe Useful fix column does not count in the ranking, because the judge is under the 80% gate.\n"),
            "{rendered}"
        );
        assert!(rendered.contains("| 4 of 5 (80.0%) |"), "{rendered}");
        // The ranking falls back to exact fix rate, so `gemma` wins again.
        assert!(
            rendered.contains("Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`, with thinking on."),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate, then F0.5"),
            "{rendered}"
        );
    }

    /// A judged run, one entry per model: the name, its exact fixes, its
    /// non-exact hits, and how many of those hits the file graded useful.
    ///
    /// A row whose grade is `None` produced hits the judgements file covers
    /// none of, the row a later run earns when its answers drift from the
    /// recorded ones. Every graded hit carries an agreeing hand label, so a
    /// run of five or more graded hits clears the gate.
    fn coverage_report(rows: &[(&str, usize, usize, Option<usize>)]) -> Report {
        use crate::bench::judge::{Assessment, Hit, Judgement, Judgements};

        let entry = |useful: bool| Judgement {
            useful,
            reason: "a recorded reason".to_string(),
        };
        let mut report = report();
        let mut judgements = Judgements::new();
        let mut labels = Judgements::new();
        let mut hits: Vec<Hit> = Vec::new();

        report.models.clear();
        for (name, exact, row_hits, useful) in rows {
            report.models.push(model(
                name,
                "openai",
                weights::of(name),
                measured(tally(30, *exact, 0, 40), None),
            ));
            for index in 0..*row_hits {
                let id = format!("{name}-{index}");
                let result = format!("{name} answer {index}");
                if let Some(useful) = useful {
                    let helped = index < *useful;
                    judgements
                        .entry(id.clone())
                        .or_default()
                        .insert(result.clone(), entry(helped));
                    labels
                        .entry(id.clone())
                        .or_default()
                        .insert(result.clone(), entry(helped));
                }
                hits.push(Hit {
                    row: key("openai", name),
                    id,
                    result,
                });
            }
        }

        report.judge = Some(Assessment::of(&hits, &judgements, &labels));
        report
    }

    /// A judgements file comes from an earlier run, so a row of this run may
    /// have answered in words that file never graded. Those hits are unknown
    /// rather than useless, and adding a graded row's useful hits while the
    /// unjudged row gets none would decide the recommendation on coverage. So
    /// one uncovered row drops the swapped measure for the whole table.
    #[test]
    fn one_row_the_file_covers_no_hit_of_drops_the_swapped_measure_for_every_row() {
        let rendered = coverage_report(&[
            ("gemma-4-e4b-it", 15, 10, None),
            ("phi-4-mini-instruct", 12, 10, Some(8)),
        ])
        .render();

        assert!(
            rendered.contains("at or above the 80% gate.\n"),
            "the gate itself passed: {rendered}"
        );
        assert!(rendered.contains("| not judged (10 hits) |"), "{rendered}");
        assert!(
            rendered.contains(
                "The Useful fix column does not count in the ranking, because the judgements file covers no non-exact hit of `gemma-4-e4b-it`.\n"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate, then F0.5"),
            "{rendered}"
        );
        // 15 exact beats 12 exact. The swapped measure would have made it 12
        // plus 8 useful against 15 plus nothing, and handed it to `phi`.
        assert!(
            rendered.contains("Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`, with thinking on."),
            "{rendered}"
        );
    }

    /// A row that produced no non-exact hit was offered nothing to grade, so
    /// it is covered rather than uncovered and the swap still applies.
    #[test]
    fn a_row_with_no_non_exact_hit_does_not_drop_the_swapped_measure() {
        let rendered = coverage_report(&[
            ("gemma-4-e4b-it", 12, 0, None),
            ("phi-4-mini-instruct", 10, 5, Some(5)),
        ])
        .render();

        assert!(rendered.contains("| no non-exact hit |"), "{rendered}");
        assert!(
            rendered.contains("The Useful fix column counts in the ranking.\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate plus the non-exact hits the judge called useful, over the interference sentences, then F0.5"),
            "{rendered}"
        );
        // 10 exact plus 5 useful beats 12 exact and nothing to add to it.
        assert!(
            rendered.contains("Recommended local model, the Settings default and the README line: `phi-4-mini-instruct`, with thinking on."),
            "{rendered}"
        );
    }
    /// A cloud row the cost cap ended keeps the Checks it already ran, so the
    /// judge input carries its hits. The report never measured that row, so it
    /// must not decide what the measured rows are ranked on.
    #[test]
    fn a_skipped_row_that_ran_some_checks_does_not_drop_the_swapped_measure() {
        let mut report = coverage_report(&[
            ("gemma-4-e4b-it", 12, 0, None),
            ("phi-4-mini-instruct", 10, 5, Some(5)),
            ("deepseek/deepseek-v4-flash-0731", 0, 2, None),
        ]);
        report.models[2].outcome = Outcome::Skipped("the cost cap ended this row.".to_string());
        let rendered = report.render();

        assert!(
            !rendered.contains("covers no non-exact hit of"),
            "a row the report did not measure is never named unjudged: {rendered}"
        );
        assert!(
            rendered.contains("The Useful fix column counts in the ranking.\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate plus the non-exact hits the judge called useful, over the interference sentences, then F0.5"),
            "{rendered}"
        );
        // 10 exact plus 5 useful beats 12 exact, which is only true while the
        // skipped row leaves the swapped measure in place.
        assert!(
            rendered.contains("Recommended local model, the Settings default and the README line: `phi-4-mini-instruct`, with thinking on."),
            "{rendered}"
        );
    }
    /// Every Models row skipped is not a table the judge may claim to rank.
    ///
    /// The Engines rows still feed the judge, so the gate can pass on a run
    /// whose whole Models table says `skipped`. Nothing was ranked, so naming
    /// the swapped measure would be an untrue sentence in a released file.
    #[test]
    fn a_run_whose_every_model_row_is_skipped_never_claims_the_swapped_measure() {
        use crate::bench::judge::{Assessment, Hit, Judgement, Judgements};

        let entry = |useful: bool| Judgement {
            useful,
            reason: "a recorded reason".to_string(),
        };
        let mut report = report();
        let mut judgements = Judgements::new();
        let mut labels = Judgements::new();
        let mut hits: Vec<Hit> = Vec::new();

        report.models = vec![model(
            "gemma-4-e4b-it",
            "openai",
            weights::of("gemma-4-e4b-it"),
            Outcome::Skipped("llama.cpp is not installed.".to_string()),
        )];
        // Five graded hits of the `harper` Engines row, every one agreed with,
        // so the gate itself passes on this run.
        for index in 0..5 {
            let id = format!("zh-0{index}");
            let result = format!("answer {index}");
            judgements
                .entry(id.clone())
                .or_default()
                .insert(result.clone(), entry(true));
            labels
                .entry(id.clone())
                .or_default()
                .insert(result.clone(), entry(true));
            hits.push(Hit {
                row: key("harper", "harper"),
                id,
                result,
            });
        }

        report.judge = Some(Assessment::of(&hits, &judgements, &labels));
        let rendered = report.render();

        assert!(
            rendered.contains("at or above the 80% gate.\n"),
            "the gate itself passed: {rendered}"
        );
        assert!(
            rendered.contains(
                "The Useful fix column does not count in the ranking, because the judgements file covers no measured model row.\n"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("Ranking: exact fix rate, then F0.5"),
            "{rendered}"
        );
    }
}
