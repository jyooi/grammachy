//! The Useful fix column and its agreement gate, evals spec section 4.4.
//!
//! A model that finds the right span but writes a different correction scores
//! zero on exact fix, and the pilot found that a good part of those answers
//! still help the writer. So `cli/bench/judge.py` grades every non-exact hit of
//! a recorded run with Claude Fable 5 and writes `judgements.json`. This module
//! is the reading half: it turns that file into the Useful fix column and
//! decides whether the column may count in the ranking.
//!
//! - A **non-exact hit** is an interference item whose valid Check touched at
//!   least one expected span but whose Fixes do not reproduce `expected_text`.
//!   An item nothing touched is a plain miss and is never judged, because the
//!   writer is offered nothing to accept.
//! - Both files are keyed by **item id, then result text**, the sentence the
//!   writer gets after Accept. Nesting the two keys is what keeps a result text
//!   that holds any delimiter readable, and it is what folds the identical
//!   answers of two models onto one judgement.
//! - The **gate** is spec section 4.4: the column counts in the ranking only
//!   when the judge agrees with the committed hand labels on at least 80% of
//!   the labelled items of that run. Below the gate, or with no label matched,
//!   the column still prints and the file says it is excluded.
//!
//! The hand labels are compiled in from `tests/fixtures/judge-labels.json`, so
//! a released binary needs no repository checkout, the same rule the fixture
//! follows. They carry no eval-set text of their own: every label of the file
//! is a fixture item, and section 2.1 forbids committing FCE text.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The agreement a run needs before the Useful fix column may rank, in percent.
pub const AGREEMENT_GATE: f64 = 80.0;

/// The committed hand labels, the truth the judge is measured against.
const LABELS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/judge-labels.json"
));

/// One graded answer: was the writer helped by accepting these edits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Judgement {
    pub useful: bool,
    /// One sentence from the judge. A hand label carries none.
    #[serde(default)]
    pub reason: String,
}

/// A judgements or hand-label file: item id, then result text, then the answer.
pub type Judgements = BTreeMap<String, BTreeMap<String, Judgement>>;

/// Read a judgements file, or say why it is not one.
pub fn read(path: &std::path::Path) -> Result<Judgements, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("--judgements: {} cannot be read: {error}", path.display()))?;
    parse(&text).map_err(|error| {
        format!(
            "--judgements: {} is not a judgements file: {error}",
            path.display()
        )
    })
}

/// Read one judgements file from its text.
pub fn parse(text: &str) -> Result<Judgements, serde_json::Error> {
    serde_json::from_str(text)
}

/// The committed hand labels of spec section 4.4.
pub fn labels() -> Judgements {
    parse(LABELS).expect("the compiled hand labels are a judgements file")
}

/// One non-exact hit of a run, folded onto its judgement key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The engine and model whose row produced it, the key of the table row.
    pub row: (String, String),
    pub id: String,
    /// The sentence the writer gets after Accept.
    pub result: String,
}

/// The Useful fix count of one Models row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowTally {
    /// Non-exact hits the judgements file graded useful.
    pub useful: usize,
    /// Non-exact hits the judgements file graded at all.
    pub judged: usize,
    /// Non-exact hits the row produced, graded or not.
    pub hits: usize,
}

impl RowTally {
    /// The Useful fix cell, `n of m (rate)` over the judged hits.
    ///
    /// A row whose hits are all missing from the file prints what is missing
    /// rather than a rate of nothing, because a silent `0 of 0` reads as a
    /// model that helped nobody.
    pub fn cell(&self) -> String {
        if self.judged == 0 {
            return match self.hits {
                0 => "no non-exact hit".to_string(),
                hits => format!("not judged ({hits} hits)"),
            };
        }
        format!(
            "{} of {} ({:.1}%)",
            self.useful,
            self.judged,
            100.0 * self.useful as f64 / self.judged as f64
        )
    }
}

/// What one judgements file says about one run.
#[derive(Debug, Clone, Default)]
pub struct Assessment {
    /// The Useful fix count of every row that produced a non-exact hit.
    rows: BTreeMap<(String, String), RowTally>,
    /// Folded non-exact hits of the run that a hand label also covers.
    pub labelled: usize,
    /// Labelled hits where the judge and the hand label say the same thing.
    pub agreed: usize,
    /// Folded non-exact hits of the run, and how many the file graded.
    pub hits: usize,
    pub judged: usize,
}

impl Assessment {
    /// Grade one run: the hits it produced against the judgements it was given.
    ///
    /// The row counts are per row, because the column is a row column. The
    /// agreement is over the folded hits of the whole run, because the gate is
    /// a fact about the judge on this run rather than about one model.
    pub fn of(hits: &[Hit], judgements: &Judgements, labels: &Judgements) -> Assessment {
        let mut assessment = Assessment::default();
        let mut folded: BTreeMap<(&str, &str), ()> = BTreeMap::new();

        for hit in hits {
            let judgement = look_up(judgements, &hit.id, &hit.result);
            let row = assessment.rows.entry(hit.row.clone()).or_default();
            row.hits += 1;
            if let Some(judgement) = judgement {
                row.judged += 1;
                if judgement.useful {
                    row.useful += 1;
                }
            }

            // The agreement counts one folded hit once, however many rows
            // produced the same answer for the same item.
            if folded
                .insert((hit.id.as_str(), hit.result.as_str()), ())
                .is_some()
            {
                continue;
            }
            assessment.hits += 1;
            let Some(judgement) = judgement else { continue };
            assessment.judged += 1;
            let Some(label) = look_up(labels, &hit.id, &hit.result) else {
                continue;
            };
            assessment.labelled += 1;
            if judgement.useful == label.useful {
                assessment.agreed += 1;
            }
        }

        assessment
    }

    pub fn row(&self, engine: &str, model: &str) -> Option<RowTally> {
        self.rows
            .get(&(engine.to_string(), model.to_string()))
            .copied()
    }

    /// How often the judge and the hand labels said the same thing, in percent.
    ///
    /// A run no hand label covers has no agreement to report, which is not the
    /// same as an agreement of zero.
    pub fn agreement_percent(&self) -> Option<f64> {
        if self.labelled == 0 {
            return None;
        }
        Some(100.0 * self.agreed as f64 / self.labelled as f64)
    }

    /// Whether the Useful fix column may count in the ranking.
    pub fn ranks(&self) -> bool {
        self.agreement_percent()
            .is_some_and(|percent| percent >= AGREEMENT_GATE)
    }

    /// The sentences the benchmark file carries under the Quality table.
    pub fn lines(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Useful fix: {} of the {} folded non-exact hits of this run carry a judgement in the judgements file.\n",
            self.judged, self.hits
        ));
        match self.agreement_percent() {
            Some(percent) if self.ranks() => out.push_str(&format!(
                "The judge agreed with the hand labels on {} of {} ({percent:.1}%), at or above the {AGREEMENT_GATE:.0}% gate, so the Useful fix column counts in the ranking.\n",
                self.agreed, self.labelled
            )),
            Some(percent) => out.push_str(&format!(
                "The judge agreed with the hand labels on {} of {} ({percent:.1}%), under the {AGREEMENT_GATE:.0}% gate, so the Useful fix column is printed but excluded from the ranking.\n",
                self.agreed, self.labelled
            )),
            None => out.push_str(&format!(
                "No hand label of `cli/tests/fixtures/judge-labels.json` covers a hit of this run, so the {AGREEMENT_GATE:.0}% gate could not be measured and the Useful fix column is excluded from the ranking.\n",
            )),
        }
        out
    }
}

/// The judgement of one item and result text, when the file carries it.
fn look_up<'a>(judgements: &'a Judgements, id: &str, result: &str) -> Option<&'a Judgement> {
    judgements.get(id)?.get(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(engine: &str, model: &str, id: &str, result: &str) -> Hit {
        Hit {
            row: (engine.to_string(), model.to_string()),
            id: id.to_string(),
            result: result.to_string(),
        }
    }

    /// A judgements file in the shape `judge.py` writes.
    fn file(entries: &[(&str, &str, bool)]) -> Judgements {
        let mut out = Judgements::new();
        for (id, result, useful) in entries {
            out.entry(id.to_string()).or_default().insert(
                result.to_string(),
                Judgement {
                    useful: *useful,
                    reason: "a recorded reason".to_string(),
                },
            );
        }
        out
    }

    #[test]
    fn a_row_counts_the_useful_share_of_the_hits_that_were_judged() {
        let hits = [
            hit("openai", "gemma", "zh-03", "kept one"),
            hit("openai", "gemma", "zh-04", "kept two"),
            hit("openai", "gemma", "zh-06", "never judged"),
        ];
        let judgements = file(&[("zh-03", "kept one", true), ("zh-04", "kept two", false)]);

        let row = Assessment::of(&hits, &judgements, &Judgements::new())
            .row("openai", "gemma")
            .expect("the row produced hits");

        assert_eq!(
            row,
            RowTally {
                useful: 1,
                judged: 2,
                hits: 3
            }
        );
        assert_eq!(row.cell(), "1 of 2 (50.0%)");
    }

    #[test]
    fn a_row_with_no_judged_hit_says_so_rather_than_printing_a_rate() {
        assert_eq!(
            RowTally {
                useful: 0,
                judged: 0,
                hits: 2
            }
            .cell(),
            "not judged (2 hits)"
        );
        assert_eq!(RowTally::default().cell(), "no non-exact hit");
    }

    #[test]
    fn agreement_counts_one_folded_hit_once_however_many_rows_produced_it() {
        let hits = [
            hit("openai", "gemma", "zh-03", "same answer"),
            hit("openrouter", "deepseek", "zh-03", "same answer"),
        ];
        let judgements = file(&[("zh-03", "same answer", true)]);
        let labels = file(&[("zh-03", "same answer", true)]);

        let assessment = Assessment::of(&hits, &judgements, &labels);

        assert_eq!((assessment.hits, assessment.judged), (1, 1));
        assert_eq!((assessment.agreed, assessment.labelled), (1, 1));
        // Both rows still carry the hit, because the column is a row column.
        assert_eq!(assessment.row("openai", "gemma").unwrap().hits, 1);
        assert_eq!(assessment.row("openrouter", "deepseek").unwrap().hits, 1);
    }

    #[test]
    fn a_judge_that_agrees_with_four_of_five_labels_clears_the_gate() {
        let entries: Vec<(String, String, bool)> = (0..5)
            .map(|n| (format!("zh-0{n}"), format!("answer {n}"), true))
            .collect();
        let borrowed: Vec<(&str, &str, bool)> = entries
            .iter()
            .map(|(id, result, useful)| (id.as_str(), result.as_str(), *useful))
            .collect();
        let hits: Vec<Hit> = borrowed
            .iter()
            .map(|(id, result, _)| hit("openai", "gemma", id, result))
            .collect();
        let judgements = file(&borrowed);
        // One label disagrees, so agreement is four of five.
        let mut flipped = borrowed.clone();
        flipped[0].2 = false;
        let labels = file(&flipped);

        let assessment = Assessment::of(&hits, &judgements, &labels);

        assert_eq!((assessment.agreed, assessment.labelled), (4, 5));
        assert_eq!(assessment.agreement_percent(), Some(80.0));
        assert!(assessment.ranks(), "80% is at the gate, not under it");
    }

    #[test]
    fn a_judge_that_agrees_with_three_of_five_labels_is_under_the_gate() {
        let hits = [
            hit("openai", "gemma", "a", "one"),
            hit("openai", "gemma", "b", "two"),
        ];
        let judgements = file(&[("a", "one", true), ("b", "two", true)]);
        let labels = file(&[("a", "one", true), ("b", "two", false)]);

        let assessment = Assessment::of(&hits, &judgements, &labels);

        assert_eq!(assessment.agreement_percent(), Some(50.0));
        assert!(!assessment.ranks());
        assert!(
            assessment.lines().contains("under the 80% gate"),
            "{}",
            assessment.lines()
        );
    }

    #[test]
    fn a_run_no_hand_label_covers_reports_no_agreement_and_never_ranks() {
        let hits = [hit("openai", "gemma", "zh-03", "an answer")];
        let judgements = file(&[("zh-03", "an answer", true)]);

        let assessment = Assessment::of(&hits, &judgements, &Judgements::new());

        assert_eq!(assessment.agreement_percent(), None);
        assert!(!assessment.ranks());
        assert!(
            assessment.lines().contains("could not be measured"),
            "{}",
            assessment.lines()
        );
    }

    /// A hand label keyed by the same item but a different result text is a
    /// different answer, so it must not be read as this one's truth.
    #[test]
    fn a_label_on_another_result_of_the_same_item_is_not_this_hit_s_label() {
        let hits = [hit("openai", "gemma", "zh-03", "what this model wrote")];
        let judgements = file(&[("zh-03", "what this model wrote", true)]);
        let labels = file(&[("zh-03", "what another model wrote", false)]);

        let assessment = Assessment::of(&hits, &judgements, &labels);

        assert_eq!(assessment.labelled, 0);
    }

    #[test]
    fn the_committed_hand_labels_are_the_seventeen_pilot_items() {
        let labels = labels();
        let items: usize = labels.values().map(BTreeMap::len).sum();
        let useful: usize = labels
            .values()
            .flat_map(BTreeMap::values)
            .filter(|label| label.useful)
            .count();

        assert_eq!(items, 17, "the pilot sample of HUF-210");
        assert_eq!(useful, 8, "eight useful and nine not useful");
    }

    /// The committed labels carry no eval-set text, the rule of section 2.1.
    ///
    /// Every label of the file belongs to an item of the fixture, which is
    /// committed text already. A label on a fetched eval-set item would put
    /// FCE-derived text in the repository, so it never belongs here.
    #[test]
    fn every_committed_hand_label_names_a_fixture_item() {
        let ids: Vec<String> = crate::bench::fixture::sentences()
            .into_iter()
            .map(|sentence| sentence.id)
            .collect();

        for id in labels().keys() {
            assert!(ids.contains(id), "{id} is not a fixture item");
        }
    }
}
