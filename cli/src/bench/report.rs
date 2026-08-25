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

use crate::bench::machine::Machine;
use crate::bench::memory;
use crate::bench::metrics::Tally;
use crate::bench::weights::Weights;

/// What one row prints in every measured column when the row did not run.
const SKIPPED: &str = "skipped";

/// The numbers of one row that ran.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub tally: Tally,
    /// Resident memory in bytes, or `None` when it could not be read.
    pub memory_bytes: Option<u64>,
}

/// Whether one row ran, and why it did not.
#[derive(Debug, Clone)]
pub enum Outcome {
    Measured(Box<Measurement>),
    /// The one sentence that says why the row was skipped.
    Skipped(String),
}

/// One row of the Engines table.
#[derive(Debug, Clone)]
pub struct EngineRow {
    pub engine: String,
    /// What resident memory means for this engine, named under the table.
    pub memory_kind: &'static str,
    pub outcome: Outcome,
}

/// One row of the Models table.
#[derive(Debug, Clone)]
pub struct ModelRow {
    pub model: String,
    pub weights: Weights,
    pub outcome: Outcome,
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
    /// The engine a release is measured against (spec section 7).
    pub default_engine: String,
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
        out.push_str(&self.models_table());
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
                measured_cells(&row.outcome).join(" | ")
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

    fn models_table(&self) -> String {
        let mut out = String::from("## Models\n\n");
        if self.models.is_empty() {
            out.push_str(
                "No model was named. Run `grammachy bench --engine openai --model <name>`, once per model, to fill this table.\n\n",
            );
            return out;
        }

        out.push_str("| Model | Catch rate | False positives | p50 latency | Resident memory | Weights license | Recommended |\n");
        out.push_str("|---|---|---|---|---|---|---|\n");
        for row in &self.models {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                row.model,
                measured_cells(&row.outcome).join(" | "),
                row.weights.license,
                row.weights.recommendation()
            ));
        }
        out.push_str(
            "\nThe recommended model of the Settings defaults and the README is the best row marked `eligible` that fits the machine tier above (spec section 13.1).\n\n",
        );
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

/// The four measured cells of one row, in table order.
fn measured_cells(outcome: &Outcome) -> Vec<String> {
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

const MEASUREMENT_NOTE: &str = "\
## How the numbers are measured

- Catch rate: an interference sentence is caught when at least one Issue overlaps the span the fixture expects. A right span with a wrong Fix still counts, because the Panel shows the span and lets the user Skip the Fix.
- False positives: correct sentences that earned at least one Issue. One sentence counts once, however many Issues it earned.
- p50 latency: the median over every sentence of the fixture, correct ones included, measured in process around one Check.
- Every sentence is checked with the Native language the fixture records for it, which is what the shell passes on a real Check.
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::weights;

    fn tally() -> Tally {
        Tally {
            interference: 30,
            caught: 10,
            clean: 10,
            false_positives: 0,
            p50_ms: 20,
            misses: vec!["zh-01".to_string()],
        }
    }

    fn measured(memory_bytes: Option<u64>) -> Outcome {
        Outcome::Measured(Box::new(Measurement {
            tally: tally(),
            memory_bytes,
        }))
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
            default_engine: "languagetool".to_string(),
            engines: vec![
                EngineRow {
                    engine: "languagetool".to_string(),
                    memory_kind: "the RSS of its server process",
                    outcome: measured(Some(731_000_000)),
                },
                EngineRow {
                    engine: "openai".to_string(),
                    memory_kind: "the RSS of its server process",
                    outcome: Outcome::Skipped("llama.cpp is not installed.".to_string()),
                },
            ],
            models: vec![ModelRow {
                model: "qwen2.5-3b-instruct".to_string(),
                weights: weights::of("qwen2.5-3b-instruct"),
                outcome: Outcome::Skipped("llama.cpp is not installed.".to_string()),
            }],
        }
    }

    #[test]
    fn a_measured_engine_carries_every_number_of_the_row() {
        let rendered = report().render();

        assert!(
            rendered.contains("| `languagetool` | 10 of 30 (33%) | 0 of 10 | 20 ms | 731 MB |"),
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
        assert!(
            !rendered.contains("| `openai` | error"),
            "a skipped engine never renders as a failed row: {rendered}"
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
        report.engines[1].outcome = measured(None);
        report.models.clear();

        let rendered = report.render();

        assert!(
            rendered.contains("Every engine and model of this run was reachable."),
            "{rendered}"
        );
        assert!(
            rendered.contains("| `openai` | 10 of 30 (33%) | 0 of 10 | 20 ms | not measured |"),
            "{rendered}"
        );
    }
}
