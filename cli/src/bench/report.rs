//! The Markdown report `grammachy bench` prints, spec section 13.1.
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

use crate::bench::machine::Machine;
use crate::bench::memory;
use crate::bench::metrics::Tally;
use crate::bench::weights::{Terms, Weights};

/// What one row prints in every measured column when the row did not run.
const SKIPPED: &str = "skipped";

/// The validity a row needs to be recommended, in percent.
const VALIDITY_FLOOR: f64 = 95.0;

/// The numbers of one row that ran.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub tally: Tally,
    /// Resident memory in bytes, or `None` when it could not be read.
    pub memory_bytes: Option<u64>,
    /// How long the whole row took, server start included.
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
    /// What resident memory means for this engine, named under the table.
    pub memory_kind: &'static str,
    pub outcome: Outcome,
}

/// One row of the Models tables.
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub model: String,
    /// The engine the row ran on, `openai` or `openrouter`.
    pub engine: String,
    pub weights: Weights,
    pub outcome: Outcome,
}

impl ModelRow {
    fn is_cloud(&self) -> bool {
        self.weights.terms == Terms::Hosted
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
            out.push_str(&format!(
                "Resident memory of `{}` is {}.\n",
                row.engine, row.memory_kind
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
        out.push_str("| Model | Catch rate | Precision | Recall | F0.5 | Exact fix | False positives | Style creep | Valid |\n");
        out.push_str("|---|---|---|---|---|---|---|---|---|\n");
        for row in &self.models {
            out.push_str(&format!(
                "| `{}` | {} |\n",
                row.model,
                quality_cells(&row.outcome).join(" | ")
            ));
        }
        out.push('\n');

        out.push_str("### Cost\n\n");
        out.push_str("| Model | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |\n");
        out.push_str("|---|---|---|---|---|---|---|\n");
        for (row, verdict) in self.models.iter().zip(&verdicts) {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                row.model,
                cost_cells(row).join(" | "),
                row.weights.license,
                verdict
            ));
        }
        out.push('\n');
        for row in &self.models {
            if let Outcome::Measured(measurement) = &row.outcome {
                out.push_str(&format!(
                    "Wall time of `{}`: {} s for the whole fixture{}.\n",
                    row.model,
                    measurement.wall_ms / 1_000,
                    if row.is_cloud() {
                        ""
                    } else {
                        ", server start included"
                    }
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
        out.push_str("| Model | Time to first token (p50) | Output tokens per second | Output tokens per Check (p50) |\n");
        out.push_str("|---|---|---|---|\n");
        for row in &self.models {
            out.push_str(&format!(
                "| `{}` | {} |\n",
                row.model,
                throughput_cells(&row.outcome).join(" | ")
            ));
        }
        out.push_str("\nTime to first token and the token rate come from the model server's own timings. A rate marked `whole request` is output tokens over the request time as seen from this machine, network included, because the provider reports no timings.\n\n");

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
            .filter_map(|(index, row)| row.outcome.tally().map(|tally| (index, tally)))
            .max_by(|(_, a), (_, b)| {
                a.exact_rate_percent()
                    .total_cmp(&b.exact_rate_percent())
                    .then(a.f05_percent().total_cmp(&b.f05_percent()))
                    .then(b.p50_ms.cmp(&a.p50_ms))
            })
            .map(|(index, _)| index)
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
        let named = |verdict: &str| -> Option<String> {
            self.models
                .iter()
                .zip(verdicts)
                .find(|(_, cell)| cell.as_str() == verdict)
                .map(|(row, _)| row.model.clone())
        };
        match named("recommended") {
            Some(model) => out.push_str(&format!(
                "Recommended local model, the Settings default and the README line: `{model}`.\n"
            )),
            None => out.push_str("No local row is eligible for the recommendation.\n"),
        }
        match named("recommended cloud model") {
            Some(model) => out.push_str(&format!(
                "Recommended cloud model, the `openrouterModel` line of the README: `{model}`. Cloud is never the default engine.\n"
            )),
            None if self.models.iter().any(ModelRow::is_cloud) => {
                out.push_str("No cloud row is eligible for the cloud recommendation.\n")
            }
            None => {}
        }
        out.push_str(&format!(
            "Ranking: exact fix rate, then F0.5, then lower p50 (HUF-205). Floors: validity at least {VALIDITY_FLOOR:.0}% and no more false positives than the default engine, `{}`{}. A recommended local model must also fit the machine tier above (spec section 13.1).\n\n",
            self.default_engine,
            match default_fp {
                Some(fp) => format!(", which earned {fp}"),
                None => ", which was skipped in this run, so that floor was not applied".to_string(),
            }
        ));
        out
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
                reasons.push(format!("- Model `{}`: {why}\n", row.model));
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

/// The four measured cells of one Engines row, in table order.
fn engine_cells(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Skipped(_) => vec![SKIPPED.to_string(); 4],
        Outcome::Measured(measurement) => vec![
            measurement.tally.catch_rate_cell(),
            measurement.tally.false_positive_cell(),
            format!("{} ms", measurement.tally.p50_ms),
            memory::cell(measurement.memory_bytes),
        ],
    }
}

/// The eight measured cells of one Quality row.
fn quality_cells(outcome: &Outcome) -> Vec<String> {
    match outcome {
        Outcome::Skipped(_) => vec![SKIPPED.to_string(); 8],
        Outcome::Measured(measurement) => {
            let tally = &measurement.tally;
            vec![
                tally.catch_rate_cell(),
                tally.precision_cell(),
                tally.recall_cell(),
                tally.f05_cell(),
                tally.exact_cell(),
                tally.false_positive_cell(),
                tally.creep_cell(),
                tally.validity_cell(),
            ]
        }
    }
}

/// The three measured cells of one Throughput row.
fn throughput_cells(outcome: &Outcome) -> Vec<String> {
    let Outcome::Measured(measurement) = outcome else {
        return vec![SKIPPED.to_string(); 3];
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
                memory::cell(measurement.memory_bytes),
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
";

#[cfg(test)]
mod tests {
    use super::*;
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
        Outcome::Measured(Box::new(Measurement {
            tally,
            memory_bytes,
            wall_ms: 12_000,
        }))
    }

    fn model(name: &str, engine: &str, weights: Weights, outcome: Outcome) -> ModelRow {
        ModelRow {
            model: name.to_string(),
            engine: engine.to_string(),
            weights,
            outcome,
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
            interference: 30,
            clean: 10,
            languages: vec!["zh".to_string(), "es".to_string()],
            default_engine: "languagetool".to_string(),
            max_cost: None,
            cloud_spend_usd: 0.0,
            engines: vec![
                EngineRow {
                    engine: "languagetool".to_string(),
                    memory_kind: "the RSS of its server process",
                    outcome: measured(tally(10, 5, 0, 40), Some(731_000_000)),
                },
                EngineRow {
                    engine: "openai".to_string(),
                    memory_kind: "the RSS of its server process",
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

    #[test]
    fn a_model_with_non_commercial_weights_is_shown_and_marked_never_recommended() {
        let rendered = report().render();

        assert!(
            rendered.contains(
                "| `qwen2.5-3b-instruct` | skipped | skipped | skipped | skipped | Qwen Research License | never, the weights are non-commercial |"
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

        assert!(rendered.contains("| `qwen3.5-4b` | 20 ms | 50 ms | 3.0 GB | 0.00 (local) | Apache-2.0 | recommended |"), "{rendered}");
        assert!(rendered.contains("| `gemma-4-e4b-it` | 20 ms | 50 ms | 5.0 GB | 0.00 (local) | Apache-2.0 | eligible |"), "{rendered}");
        assert!(rendered.contains("| `phi-4-mini` | 20 ms | 50 ms | 3.0 GB | 0.00 (local) | MIT | no, more false positives than `languagetool` |"), "{rendered}");
        assert!(rendered.contains("| `deepseek/deepseek-v4-flash-0731` | 20 ms | 50 ms | not measured | 0.02 USD | hosted | recommended cloud model |"), "{rendered}");
        assert!(rendered.contains("| `google/gemini-3.7-flash` | 20 ms | 50 ms | not measured | 0.02 USD | hosted | no, validity under 95% |"), "{rendered}");
        assert!(
            rendered.contains(
                "Recommended local model, the Settings default and the README line: `qwen3.5-4b`."
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
        };
        let mut cloud = tally(30, 29, 0, 40);
        cloud.throughput = crate::bench::metrics::Throughput {
            ttft_p50_ms: None,
            tokens_per_second: Some(31.0),
            whole_request: true,
            output_tokens_p50: Some(120),
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
            rendered.contains("| `gemma-4-e4b-it` | 510 ms | 25.3 | 480 |"),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "| `deepseek/deepseek-v4-flash-0731` | not measured | 31.0 (whole request) | 120 |"
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
}
