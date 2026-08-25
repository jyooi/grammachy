//! The arithmetic of one benchmark row, spec section 13.1.
//!
//! Every number in the tables comes from this module, computed from the
//! per-sentence results the run recorded. Nothing here talks to an engine, so
//! the arithmetic is testable without a server.
//!
//! The three definitions are the ones HUF-171 measured with:
//!
//! - **caught**: at least one Issue of the answer overlaps the span the fixture
//!   expects. A right span with a wrong Fix still counts, because the Panel
//!   shows the user the span and lets them Skip the Fix.
//! - **false positive**: a correct sentence that earned at least one Issue.
//!   One sentence counts once, however many Issues it earned.
//! - **p50 latency**: the median over every sentence of the fixture, correct
//!   ones included, because the user pays that cost on every Check.

use crate::bench::fixture::Span;

/// What one sentence cost and what the engine answered for it.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub id: String,
    /// The span the fixture expects, or `None` for a correct sentence.
    pub expected: Option<Span>,
    /// The span of every Issue the engine answered, in UTF-16 code units.
    pub spans: Vec<Span>,
    pub latency_ms: u64,
}

/// Every number of one table row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tally {
    /// Interference sentences seen.
    pub interference: usize,
    pub caught: usize,
    /// Correct sentences seen.
    pub clean: usize,
    pub false_positives: usize,
    pub p50_ms: u64,
    /// The ids of the interference sentences no Issue touched.
    pub misses: Vec<String>,
}

impl Tally {
    /// Count one run of the fixture.
    pub fn of(recorded: &[Recorded]) -> Tally {
        let mut tally = Tally::default();

        for sentence in recorded {
            match sentence.expected {
                None => {
                    tally.clean += 1;
                    if !sentence.spans.is_empty() {
                        tally.false_positives += 1;
                    }
                }
                Some(expected) => {
                    tally.interference += 1;
                    if sentence.spans.iter().any(|span| span.overlaps(expected)) {
                        tally.caught += 1;
                    } else {
                        tally.misses.push(sentence.id.clone());
                    }
                }
            }
        }

        tally.p50_ms = p50_ms(recorded);
        tally
    }

    /// The catch rate as a percentage, or zero when nothing was measured.
    pub fn catch_rate_percent(&self) -> f64 {
        if self.interference == 0 {
            return 0.0;
        }
        100.0 * self.caught as f64 / self.interference as f64
    }

    /// The catch rate as one table cell, such as `10 of 30 (33%)`.
    pub fn catch_rate_cell(&self) -> String {
        format!(
            "{} of {} ({:.0}%)",
            self.caught,
            self.interference,
            self.catch_rate_percent()
        )
    }

    /// The false positives as one table cell, such as `0 of 10`.
    pub fn false_positive_cell(&self) -> String {
        format!("{} of {}", self.false_positives, self.clean)
    }
}

/// The median latency in milliseconds.
///
/// An even count averages the two middle values, so adding one slow sentence to
/// an even fixture cannot move the median by a whole sentence.
fn p50_ms(recorded: &[Recorded]) -> u64 {
    if recorded.is_empty() {
        return 0;
    }

    let mut latencies: Vec<u64> = recorded
        .iter()
        .map(|sentence| sentence.latency_ms)
        .collect();
    latencies.sort_unstable();

    let middle = latencies.len() / 2;
    if latencies.len() % 2 == 1 {
        latencies[middle]
    } else {
        (latencies[middle - 1] + latencies[middle]) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// One interference sentence with the Issues an engine answered.
    fn interference(
        id: &str,
        expected: (usize, usize),
        spans: &[(usize, usize)],
        ms: u64,
    ) -> Recorded {
        Recorded {
            id: id.to_string(),
            expected: Some(span(expected.0, expected.1)),
            spans: spans.iter().map(|&(a, b)| span(a, b)).collect(),
            latency_ms: ms,
        }
    }

    /// One correct sentence with the Issues an engine answered.
    fn clean(id: &str, spans: &[(usize, usize)], ms: u64) -> Recorded {
        Recorded {
            id: id.to_string(),
            expected: None,
            spans: spans.iter().map(|&(a, b)| span(a, b)).collect(),
            latency_ms: ms,
        }
    }

    #[test]
    fn an_issue_that_touches_the_expected_span_is_a_catch() {
        let recorded = vec![
            // Exactly the expected span.
            interference("zh-02", (17, 21), &[(17, 21)], 10),
            // Wider than the expected span, which still localizes the mistake.
            interference("ms-04", (14, 19), &[(3, 22)], 10),
            // Touching by one code unit at the end.
            interference("fr-05", (10, 14), &[(13, 30)], 10),
            // Beside the expected span, which is a miss.
            interference("es-07", (4, 9), &[(20, 25)], 10),
            // No Issue at all, which is a miss.
            interference("zh-07", (0, 5), &[], 10),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.interference, 5);
        assert_eq!(tally.caught, 3);
        assert_eq!(tally.misses, ["es-07", "zh-07"]);
        assert_eq!(tally.catch_rate_cell(), "3 of 5 (60%)");
        assert!((tally.catch_rate_percent() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_span_that_ends_where_the_expected_span_starts_is_a_miss() {
        let tally = Tally::of(&[interference("zh-01", (12, 14), &[(0, 12)], 5)]);

        assert_eq!(tally.caught, 0);
    }

    #[test]
    fn one_correct_sentence_counts_as_one_false_positive_however_many_issues() {
        let recorded = vec![
            clean("ok-01", &[], 4),
            clean("ok-02", &[(0, 3)], 4),
            clean("ok-03", &[(0, 3), (9, 12), (20, 24)], 4),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.clean, 3);
        assert_eq!(tally.false_positives, 2);
        assert_eq!(tally.false_positive_cell(), "2 of 3");
    }

    #[test]
    fn a_correct_sentence_never_reaches_the_catch_rate() {
        let recorded = vec![
            interference("zh-02", (17, 21), &[(17, 21)], 10),
            clean("ok-01", &[(0, 4)], 10),
        ];

        let tally = Tally::of(&recorded);

        assert_eq!(tally.catch_rate_cell(), "1 of 1 (100%)");
        assert_eq!(tally.false_positive_cell(), "1 of 1");
    }

    #[test]
    fn the_median_is_the_middle_of_an_odd_count() {
        let recorded = vec![
            clean("a", &[], 90),
            clean("b", &[], 10),
            clean("c", &[], 20),
        ];

        assert_eq!(Tally::of(&recorded).p50_ms, 20);
    }

    #[test]
    fn the_median_of_an_even_count_averages_the_two_middle_values() {
        let recorded = vec![
            clean("a", &[], 10),
            clean("b", &[], 20),
            clean("c", &[], 30),
            clean("d", &[], 1_000),
        ];

        assert_eq!(Tally::of(&recorded).p50_ms, 25);
    }

    #[test]
    fn an_empty_run_reports_zero_rather_than_dividing_by_zero() {
        let tally = Tally::of(&[]);

        assert_eq!(tally.p50_ms, 0);
        assert_eq!(tally.catch_rate_percent(), 0.0);
        assert_eq!(tally.catch_rate_cell(), "0 of 0 (0%)");
    }
}
