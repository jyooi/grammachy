//! The arithmetic of one benchmark row, `docs/spec/evals.md` section 5.
//!
//! Every number in the tables comes from this module, computed from the
//! per-sentence results the run recorded. Nothing here talks to an engine, so
//! the arithmetic is testable without a server.
//!
//! The definitions, each computed from the Issues of one Check and the edits of
//! one fixture item so two implementations agree to the digit:
//!
//! - **caught**: at least one Issue overlaps an expected edit. A right span with
//!   a wrong Fix still counts, because the Panel shows the span and lets the
//!   user Skip the Fix. This is the regression gate of v1 13.1, untouched.
//! - **false positive**: a correct sentence that earned at least one Issue.
//! - **pair**: an Issue pairs with the first unpaired edit it overlaps, provided
//!   the Issue extends no more than three words past the edit on either side.
//!   Precision, recall, and F0.5 are micro-averaged over the whole set.
//! - **exact fix**: applying every Fix of the Check yields `expected_text`,
//!   after collapsing runs of whitespace.
//! - **style creep**: unpaired Issues on interference sentences, per 100 such
//!   sentences.
//! - **valid**: the Check returned a result envelope. An invalid Check counts
//!   as zero Issues, so a miss, and stays out of precision, exact fix, and
//!   latency.
//! - **p50 and p95**: nearest rank over the valid latencies, no interpolation.

use std::collections::BTreeMap;

use crate::bench::fixture::Span;
use crate::engine::Usage;
use crate::envelope::Issue;
use crate::text::{byte_index_of_utf16, utf16_slice};

/// How far an Issue may reach past the edit it pairs with, in words.
const PAIR_SLACK_WORDS: usize = 3;

/// What one sentence cost and what the engine answered for it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub id: String,
    pub native: String,
    pub text: String,
    /// The spans the fixture expects, empty for a correct sentence.
    pub edits: Vec<Span>,
    pub expected_text: String,
    /// The Issues the engine answered, empty when the Check was invalid.
    pub issues: Vec<Issue>,
    /// Whether the Check returned a result envelope.
    pub valid: bool,
    pub latency_ms: u64,
    /// `usage.cost` in USD when the engine reported one.
    pub cost: Option<f64>,
    /// Token counts and server timings when the engine reported them.
    pub usage: Option<Usage>,
}

impl Recorded {
    pub fn is_interference(&self) -> bool {
        !self.edits.is_empty()
    }
}

/// Recall restricted to one native language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LanguageRecall {
    pub pairs: usize,
    pub edits: usize,
}

/// Every number of one table row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tally {
    /// Interference sentences seen.
    pub interference: usize,
    pub caught: usize,
    /// Correct sentences seen.
    pub clean: usize,
    pub false_positives: usize,
    /// Issues of every valid Check, correct sentences included.
    pub issues: usize,
    pub pairs: usize,
    /// Expected edits of every sentence, invalid Checks included.
    pub edits: usize,
    pub exact: usize,
    /// Unpaired Issues on interference sentences.
    pub creep_issues: usize,
    pub checks: usize,
    pub valid: usize,
    pub p50_ms: u64,
    pub p95_ms: u64,
    /// The sum of every reported cost, and how many Checks reported one.
    pub cost_usd: f64,
    pub priced: usize,
    /// The ids of the interference sentences no Issue touched.
    pub misses: Vec<String>,
    pub by_language: BTreeMap<String, LanguageRecall>,
    pub throughput: Throughput,
}

/// How fast the model produced its answers, from what the server reported.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Throughput {
    /// Median time before the first output token, from server timings.
    pub ttft_p50_ms: Option<u64>,
    /// Output tokens per second over the row.
    pub tokens_per_second: Option<f64>,
    /// Whether the rate was measured around the whole request rather than
    /// the server's own generation time, so it includes the network.
    pub whole_request: bool,
    /// Median output tokens of one Check.
    pub output_tokens_p50: Option<u64>,
    /// Output tokens of the row divided by the Issues those same Checks
    /// answered. This is the number HUF-218 measured at about 56 and HUF-219
    /// set out to halve, so it is what says whether the compact answer landed.
    pub tokens_per_issue: Option<f64>,
}

impl Tally {
    /// Count one run of the fixture.
    pub fn of(recorded: &[Recorded]) -> Tally {
        let mut tally = Tally::default();

        for sentence in recorded {
            tally.checks += 1;
            tally.edits += sentence.edits.len();
            if sentence.valid {
                tally.valid += 1;
                tally.issues += sentence.issues.len();
            }
            if let Some(cost) = sentence.cost {
                tally.cost_usd += cost;
                tally.priced += 1;
            }

            let pairs = pair(&sentence.text, &sentence.issues, &sentence.edits);
            tally.pairs += pairs;
            if sentence.is_interference() {
                tally
                    .by_language
                    .entry(sentence.native.clone())
                    .or_default()
                    .add(pairs, sentence.edits.len());
            }

            if !sentence.is_interference() {
                tally.clean += 1;
                if !sentence.issues.is_empty() {
                    tally.false_positives += 1;
                }
                continue;
            }

            tally.interference += 1;
            if is_caught(&sentence.issues, &sentence.edits) {
                tally.caught += 1;
            } else {
                tally.misses.push(sentence.id.clone());
            }
            tally.creep_issues += sentence.issues.len() - pairs;
            if sentence.valid && is_exact(&sentence.text, &sentence.issues, &sentence.expected_text)
            {
                tally.exact += 1;
            }
        }

        let mut latencies: Vec<u64> = recorded
            .iter()
            .filter(|sentence| sentence.valid)
            .map(|sentence| sentence.latency_ms)
            .collect();
        latencies.sort_unstable();
        tally.p50_ms = nearest_rank(&latencies, 0.5);
        tally.p95_ms = nearest_rank(&latencies, 0.95);
        tally.throughput = Throughput::of(recorded);
        tally
    }

    pub fn catch_rate_percent(&self) -> f64 {
        percent(self.caught, self.interference)
    }

    pub fn precision_percent(&self) -> f64 {
        percent(self.pairs, self.issues)
    }

    pub fn recall_percent(&self) -> f64 {
        percent(self.pairs, self.edits)
    }

    pub fn f05_percent(&self) -> f64 {
        let p = self.precision_percent() / 100.0;
        let r = self.recall_percent() / 100.0;
        if p + r == 0.0 {
            return 0.0;
        }
        100.0 * 1.25 * p * r / (0.25 * p + r)
    }

    pub fn exact_rate_percent(&self) -> f64 {
        percent(self.exact, self.interference)
    }

    /// Unpaired Issues per 100 interference sentences.
    pub fn creep_per_100(&self) -> f64 {
        if self.interference == 0 {
            return 0.0;
        }
        100.0 * self.creep_issues as f64 / self.interference as f64
    }

    pub fn validity_percent(&self) -> f64 {
        percent(self.valid, self.checks)
    }

    /// The catch rate as one table cell, such as `10 of 30 (33.3%)`.
    pub fn catch_rate_cell(&self) -> String {
        count_cell(self.caught, self.interference)
    }

    pub fn precision_cell(&self) -> String {
        count_cell(self.pairs, self.issues)
    }

    pub fn recall_cell(&self) -> String {
        count_cell(self.pairs, self.edits)
    }

    pub fn f05_cell(&self) -> String {
        format!("{:.1}%", self.f05_percent())
    }

    pub fn exact_cell(&self) -> String {
        count_cell(self.exact, self.interference)
    }

    /// The false positives as one table cell, such as `0 of 10`.
    pub fn false_positive_cell(&self) -> String {
        format!("{} of {}", self.false_positives, self.clean)
    }

    pub fn creep_cell(&self) -> String {
        format!("{:.1}", self.creep_per_100())
    }

    pub fn validity_cell(&self) -> String {
        count_cell(self.valid, self.checks)
    }

    /// Cost per 1,000 Checks in USD, or `None` when a valid answer lacks a
    /// cost. An invalid Check bought nothing, so it neither counts nor blocks.
    pub fn cost_per_1000(&self) -> Option<f64> {
        if self.priced == 0 || self.priced < self.valid {
            return None;
        }
        Some(self.cost_usd / self.priced as f64 * 1_000.0)
    }

    /// One cell of the "Recall by native language" table.
    pub fn language_cell(&self, language: &str) -> String {
        let recall = self.by_language.get(language).copied().unwrap_or_default();
        if recall.edits < 10 {
            format!("{} of {}", recall.pairs, recall.edits)
        } else {
            count_cell(recall.pairs, recall.edits)
        }
    }
}

impl Throughput {
    fn of(recorded: &[Recorded]) -> Throughput {
        let valid: Vec<&Recorded> = recorded.iter().filter(|sentence| sentence.valid).collect();
        let usages: Vec<Usage> = valid.iter().filter_map(|sentence| sentence.usage).collect();

        let mut ttft: Vec<u64> = usages
            .iter()
            .filter_map(|usage| usage.prompt_ms)
            .map(|ms| ms.round() as u64)
            .collect();
        ttft.sort_unstable();

        let mut outputs: Vec<u64> = usages
            .iter()
            .filter_map(|usage| usage.completion_tokens)
            .collect();
        outputs.sort_unstable();

        // The rate comes from the server's own generation time whenever any
        // valid answer reports both its output tokens and that time.
        // It comes from the whole request time around the answers when no
        // valid answer reports the server's own time.
        let timed: Vec<(u64, f64)> = valid
            .iter()
            .filter_map(|sentence| {
                let usage = sentence.usage?;
                Some((usage.completion_tokens?, usage.generation_ms?))
            })
            .collect();
        let (pairs, whole_request) = if timed.is_empty() {
            let around: Vec<(u64, f64)> = valid
                .iter()
                .filter_map(|sentence| {
                    Some((
                        sentence.usage?.completion_tokens?,
                        sentence.latency_ms as f64,
                    ))
                })
                .collect();
            (around, true)
        } else {
            (timed, false)
        };
        let tokens: u64 = pairs.iter().map(|(tokens, _)| tokens).sum();
        let ms: f64 = pairs.iter().map(|(_, ms)| ms).sum();
        let tokens_per_second = (ms > 0.0).then(|| tokens as f64 / ms * 1_000.0);

        // Both sides of the ratio come from the same Checks: a Check whose
        // server reported no token count would otherwise put its Issues in the
        // denominator with nothing in the numerator.
        let counted: Vec<(u64, usize)> = valid
            .iter()
            .filter_map(|sentence| {
                Some((sentence.usage?.completion_tokens?, sentence.issues.len()))
            })
            .collect();
        let counted_tokens: u64 = counted.iter().map(|(tokens, _)| tokens).sum();
        let counted_issues: usize = counted.iter().map(|(_, issues)| issues).sum();

        Throughput {
            ttft_p50_ms: (!ttft.is_empty()).then(|| nearest_rank(&ttft, 0.5)),
            tokens_per_second,
            whole_request: whole_request && tokens_per_second.is_some(),
            output_tokens_p50: (!outputs.is_empty()).then(|| nearest_rank(&outputs, 0.5)),
            tokens_per_issue: (counted_issues > 0)
                .then(|| counted_tokens as f64 / counted_issues as f64),
        }
    }
}

impl LanguageRecall {
    fn add(&mut self, pairs: usize, edits: usize) {
        self.pairs += pairs;
        self.edits += edits;
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    100.0 * part as f64 / whole as f64
}

fn count_cell(part: usize, whole: usize) -> String {
    format!("{part} of {whole} ({:.1}%)", percent(part, whole))
}

fn span_of(issue: &Issue) -> Span {
    Span {
        start: issue.start,
        end: issue.end,
    }
}

/// Nearest-rank percentile: element `ceil(p x n)` of the sorted list.
fn nearest_rank(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// How many Issues pair with an edit.
///
/// Both lists are walked by `start`. An Issue takes the first unpaired edit it
/// overlaps, provided it reaches no more than three words past the edit on
/// either side. Each Issue and each edit pairs at most once.
pub fn pair(text: &str, issues: &[Issue], edits: &[Span]) -> usize {
    let mut edits: Vec<Span> = edits.to_vec();
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut taken = vec![false; edits.len()];
    let mut issues: Vec<Span> = issues.iter().map(span_of).collect();
    issues.sort_by_key(|issue| (issue.start, issue.end));

    let mut pairs = 0;
    for issue in issues {
        let found = edits.iter().enumerate().find(|(index, edit)| {
            !taken[*index] && issue.overlaps(**edit) && within_slack(text, issue, **edit)
        });
        if let Some((index, _)) = found {
            taken[index] = true;
            pairs += 1;
        }
    }
    pairs
}

/// Whether the Issue extends at most three words past the edit on each side.
fn within_slack(text: &str, issue: Span, edit: Span) -> bool {
    let left = words_between(text, issue.start, edit.start);
    let right = words_between(text, edit.end, issue.end);
    left <= PAIR_SLACK_WORDS && right <= PAIR_SLACK_WORDS
}

/// The whitespace-delimited words of `text` between two UTF-16 offsets, zero
/// when the range is empty or reversed.
fn words_between(text: &str, from: usize, to: usize) -> usize {
    if from >= to {
        return 0;
    }
    utf16_slice(text, from, to)
        .map(|slice| slice.split_whitespace().count())
        .unwrap_or(0)
}

/// Apply every Fix of the Check and compare with the expected text.
pub fn is_exact(text: &str, issues: &[Issue], expected: &str) -> bool {
    let Some(corrected) = corrected(text, issues) else {
        return false;
    };
    collapse(&corrected) == collapse(expected)
}

/// Whether at least one Issue overlaps a span the item expects, the "caught"
/// rule of spec section 3.
pub fn is_caught(issues: &[Issue], edits: &[Span]) -> bool {
    issues
        .iter()
        .any(|issue| edits.iter().any(|edit| span_of(issue).overlaps(*edit)))
}

/// The Corrected text of the product: every Fix applied, later spans first so
/// earlier offsets stay valid. `None` when a span does not index the text.
///
/// This is the sentence the writer gets after Accept, so it is both the second
/// half of the judgement key of spec section 4.4 and what the judge grades.
pub fn corrected(text: &str, issues: &[Issue]) -> Option<String> {
    let mut sorted: Vec<&Issue> = issues.iter().collect();
    sorted.sort_by_key(|issue| (issue.start, issue.end));
    let mut corrected = text.to_string();
    for issue in sorted.iter().rev() {
        let from = byte_index_of_utf16(text, issue.start)?;
        let to = byte_index_of_utf16(text, issue.end)?;
        corrected.replace_range(from..to, &issue.fix);
    }
    Some(corrected)
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Category;

    fn span(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    fn issue(text: &str, start: usize, end: usize, fix: &str) -> Issue {
        Issue {
            start,
            end,
            original: utf16_slice(text, start, end).unwrap_or("").to_string(),
            fix: fix.to_string(),
            reason: "test".to_string(),
            category: Category::Grammar,
            rule_id: None,
        }
    }

    fn recorded(
        id: &str,
        text: &str,
        edits: &[(usize, usize, &str)],
        issues: &[(usize, usize, &str)],
        ms: u64,
    ) -> Recorded {
        let mut expected = text.to_string();
        for (start, end, fix) in edits.iter().rev() {
            let from = byte_index_of_utf16(text, *start).unwrap();
            let to = byte_index_of_utf16(text, *end).unwrap();
            expected.replace_range(from..to, fix);
        }
        Recorded {
            id: id.to_string(),
            native: id.split('-').next().unwrap_or("none").to_string(),
            text: text.to_string(),
            edits: edits.iter().map(|(a, b, _)| span(*a, *b)).collect(),
            expected_text: collapse(&expected),
            issues: issues
                .iter()
                .map(|(a, b, fix)| issue(text, *a, *b, fix))
                .collect(),
            valid: true,
            latency_ms: ms,
            cost: None,
            usage: None,
        }
    }

    const BOOK: &str = "She bought three book from the store.";

    #[test]
    fn an_issue_that_touches_the_expected_span_is_a_catch() {
        let recorded = vec![
            // Exactly the expected span.
            recorded(
                "zh-02",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "books")],
                10,
            ),
            // Wider than the expected span, which still localizes the mistake.
            recorded(
                "zh-03",
                BOOK,
                &[(17, 21, "books")],
                &[(11, 21, "three books")],
                10,
            ),
            // Beside the expected span, which is a miss.
            recorded("es-07", BOOK, &[(17, 21, "books")], &[(27, 30, "a")], 10),
            // No Issue at all, which is a miss.
            recorded("zh-07", BOOK, &[(17, 21, "books")], &[], 10),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.interference, 4);
        assert_eq!(tally.caught, 2);
        assert_eq!(tally.misses, ["es-07", "zh-07"]);
        assert_eq!(tally.catch_rate_cell(), "2 of 4 (50.0%)");
    }

    #[test]
    fn a_span_that_ends_where_the_expected_span_starts_is_a_miss() {
        let tally = Tally::of(&[recorded(
            "zh-01",
            BOOK,
            &[(17, 21, "books")],
            &[(11, 17, "3 ")],
            5,
        )]);

        assert_eq!(tally.caught, 0);
    }

    #[test]
    fn one_correct_sentence_counts_as_one_false_positive_however_many_issues() {
        let recorded = vec![
            recorded("ok-01", BOOK, &[], &[], 4),
            recorded("ok-02", BOOK, &[], &[(0, 3, "He")], 4),
            recorded(
                "ok-03",
                BOOK,
                &[],
                &[(0, 3, "He"), (11, 16, "four"), (27, 30, "a")],
                4,
            ),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.clean, 3);
        assert_eq!(tally.false_positives, 2);
        assert_eq!(tally.false_positive_cell(), "2 of 3");
        // Every Issue on a correct sentence is unpaired, so precision sees them.
        assert_eq!(tally.issues, 4);
        assert_eq!(tally.pairs, 0);
    }

    #[test]
    fn pairing_takes_the_first_unpaired_overlapping_edit_once() {
        let text = "I very like this song and she go home.";
        let edits = [span(2, 11), span(26, 32)];
        // Two Issues on the same edit pair once; the second Issue is creep.
        let issues = [
            issue(text, 2, 6, "really"),
            issue(text, 2, 11, "really like"),
        ];

        assert_eq!(pair(text, &issues, &edits), 1);

        let both = [
            issue(text, 2, 11, "really like"),
            issue(text, 30, 32, "goes"),
        ];
        assert_eq!(pair(text, &both, &edits), 2);
    }

    #[test]
    fn an_issue_more_than_three_words_wider_than_the_edit_does_not_pair() {
        let text = "Yesterday I go to the library with my friend.";
        let edit = [span(12, 14)];
        // "Yesterday I go to the library": three words to the right of "go".
        assert_eq!(pair(text, &[issue(text, 0, 29, "x")], &edit), 1);
        // Four words to the right.
        assert_eq!(pair(text, &[issue(text, 0, 34, "x")], &edit), 0);
        // The whole sentence, which the Panel cannot use.
        assert_eq!(pair(text, &[issue(text, 0, 45, "x")], &edit), 0);
    }

    #[test]
    fn precision_recall_and_f05_are_micro_averaged() {
        let recorded = vec![
            recorded(
                "zh-02",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "books"), (27, 30, "a")],
                10,
            ),
            recorded("zh-07", BOOK, &[(17, 21, "books")], &[], 10),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.pairs, 1);
        assert_eq!(tally.issues, 2);
        assert_eq!(tally.edits, 2);
        assert_eq!(tally.precision_cell(), "1 of 2 (50.0%)");
        assert_eq!(tally.recall_cell(), "1 of 2 (50.0%)");
        assert_eq!(tally.f05_cell(), "50.0%");
        assert_eq!(tally.creep_issues, 1);
        assert_eq!(tally.creep_cell(), "50.0");
    }

    #[test]
    fn an_exact_fix_needs_the_whole_corrected_text_to_match() {
        let recorded = vec![
            recorded(
                "zh-02",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "books")],
                10,
            ),
            // Right span, wrong fix: caught but not exact.
            recorded(
                "zh-03",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "book's")],
                10,
            ),
            // A wider span with the right words is still exact.
            recorded(
                "zh-04",
                BOOK,
                &[(17, 21, "books")],
                &[(11, 21, "three books")],
                10,
            ),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.caught, 3);
        assert_eq!(tally.exact, 2);
        assert_eq!(tally.exact_cell(), "2 of 3 (66.7%)");
    }

    #[test]
    fn a_deletion_is_exact_after_collapsing_whitespace() {
        let text = "Although it was raining, but we still went hiking.";
        assert!(is_exact(
            text,
            &[issue(text, 25, 28, "")],
            "Although it was raining, we still went hiking."
        ));
        assert!(is_exact(
            text,
            &[issue(text, 24, 28, "")],
            "Although it was raining, we still went hiking."
        ));
    }

    #[test]
    fn an_invalid_check_is_a_miss_and_stays_out_of_precision_and_latency() {
        let mut invalid = recorded("zh-02", BOOK, &[(17, 21, "books")], &[], 900);
        invalid.valid = false;
        let recorded = vec![
            recorded(
                "zh-03",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "books")],
                10,
            ),
            invalid,
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.checks, 2);
        assert_eq!(tally.valid, 1);
        assert_eq!(tally.validity_cell(), "1 of 2 (50.0%)");
        assert_eq!(tally.misses, ["zh-02"]);
        assert_eq!(tally.recall_cell(), "1 of 2 (50.0%)");
        assert_eq!(tally.precision_cell(), "1 of 1 (100.0%)");
        assert_eq!(tally.p50_ms, 10);
        assert_eq!(tally.p95_ms, 10);
    }

    #[test]
    fn latency_is_the_nearest_rank_without_interpolation() {
        let recorded: Vec<Recorded> = [90, 10, 20, 1_000]
            .iter()
            .enumerate()
            .map(|(index, ms)| recorded(&format!("ok-{index}"), BOOK, &[], &[], *ms))
            .collect();

        let tally = Tally::of(&recorded);

        // ceil(0.5 x 4) = 2nd of [10, 20, 90, 1000]; ceil(0.95 x 4) = 4th.
        assert_eq!(tally.p50_ms, 20);
        assert_eq!(tally.p95_ms, 1_000);
    }

    #[test]
    fn cost_per_thousand_needs_every_check_priced() {
        let mut priced = recorded("ok-01", BOOK, &[], &[], 1);
        priced.cost = Some(0.00002);
        let mut also = recorded("ok-02", BOOK, &[], &[], 1);
        also.cost = Some(0.00004);
        let unpriced = recorded("ok-03", BOOK, &[], &[], 1);

        let full = Tally::of(&[priced.clone(), also.clone()]);
        assert!((full.cost_per_1000().unwrap() - 0.03).abs() < 1e-9);

        let partial = Tally::of(&[priced.clone(), also.clone(), unpriced]);
        assert_eq!(partial.cost_per_1000(), None);

        let mut failed = recorded("ok-05", BOOK, &[], &[], 1);
        failed.valid = false;
        let with_failure = Tally::of(&[priced, also, failed]);
        assert!((with_failure.cost_per_1000().unwrap() - 0.03).abs() < 1e-9);

        assert_eq!(
            Tally::of(&[recorded("ok-04", BOOK, &[], &[], 1)]).cost_per_1000(),
            None
        );
    }

    #[test]
    fn recall_by_language_prints_a_count_under_ten_edits() {
        let recorded = vec![
            recorded(
                "zh-02",
                BOOK,
                &[(17, 21, "books")],
                &[(17, 21, "books")],
                10,
            ),
            recorded("es-01", BOOK, &[(17, 21, "books")], &[], 10),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.language_cell("zh"), "1 of 1");
        assert_eq!(tally.language_cell("es"), "0 of 1");
        assert_eq!(tally.language_cell("fr"), "0 of 0");
    }

    #[test]
    fn throughput_prefers_server_timings_and_falls_back_to_the_whole_request() {
        let mut local = recorded("ok-01", BOOK, &[], &[], 20_000);
        local.usage = Some(Usage {
            prompt_tokens: Some(180),
            completion_tokens: Some(500),
            prompt_ms: Some(500.0),
            generation_ms: Some(19_500.0),
        });
        let tally = Tally::of(&[local]);
        assert_eq!(tally.throughput.ttft_p50_ms, Some(500));
        assert!((tally.throughput.tokens_per_second.unwrap() - 500.0 / 19.5).abs() < 1e-6);
        assert!(!tally.throughput.whole_request);
        assert_eq!(tally.throughput.output_tokens_p50, Some(500));

        let mut cloud = recorded("ok-02", BOOK, &[], &[], 4_000);
        cloud.usage = Some(Usage {
            prompt_tokens: Some(180),
            completion_tokens: Some(100),
            ..Usage::default()
        });
        let tally = Tally::of(&[cloud]);
        assert_eq!(tally.throughput.ttft_p50_ms, None);
        assert!((tally.throughput.tokens_per_second.unwrap() - 25.0).abs() < 1e-9);
        assert!(tally.throughput.whole_request);

        let none = Tally::of(&[recorded("ok-03", BOOK, &[], &[], 10)]);
        assert_eq!(none.throughput, Throughput::default());
    }

    /// HUF-219 reads this number, so both sides of the ratio have to come from
    /// the same Checks.
    #[test]
    fn output_tokens_per_issue_counts_only_the_checks_that_reported_tokens() {
        let counted = |id: &str, issues: &[(usize, usize, &str)], tokens: u64| {
            let mut sentence = recorded(id, BOOK, &[(17, 21, "books")], issues, 10);
            sentence.usage = Some(Usage {
                completion_tokens: Some(tokens),
                ..Usage::default()
            });
            sentence
        };

        // A Check whose server reported no token count keeps its two Issues
        // out of the denominator, so the ratio stays 40 over 1 rather than
        // 40 over 3.
        let mut silent = recorded(
            "zh-02",
            BOOK,
            &[(17, 21, "books")],
            &[(17, 21, "books"), (27, 30, "a")],
            10,
        );
        silent.usage = None;
        let tally = Tally::of(&[counted("zh-01", &[(17, 21, "books")], 40), silent]);
        assert_eq!(tally.throughput.tokens_per_issue, Some(40.0));

        // An invalid Check is out of the ratio on both sides.
        let mut invalid = counted("zh-03", &[(17, 21, "books")], 1_000);
        invalid.valid = false;
        let tally = Tally::of(&[counted("zh-01", &[(17, 21, "books")], 40), invalid]);
        assert_eq!(tally.throughput.tokens_per_issue, Some(40.0));

        // Counted Checks that answered nothing divide by zero, so there is no
        // number to print.
        let tally = Tally::of(&[counted("zh-01", &[], 40), counted("zh-02", &[], 20)]);
        assert_eq!(tally.throughput.tokens_per_issue, None);
        assert_eq!(tally.throughput.output_tokens_p50, Some(20));
    }

    #[test]
    fn an_empty_run_reports_zero_rather_than_dividing_by_zero() {
        let tally = Tally::of(&[]);

        assert_eq!(tally.p50_ms, 0);
        assert_eq!(tally.catch_rate_percent(), 0.0);
        assert_eq!(tally.f05_percent(), 0.0);
        assert_eq!(tally.catch_rate_cell(), "0 of 0 (0.0%)");
    }
}
