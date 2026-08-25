//! Lint to Issue mapping for the `harper` engine, spec section 5.1.
//!
//! `harper-lints.json` holds verbatim recordings of `harper-core` 2.8 linting
//! each case text. The cases run without a dictionary and without a linter,
//! because [`harper_core::linting::Lint`] deserialises.
//!
//! To re-record a case, lint its text with `LintGroup::new_curated` over
//! `Document::new_curated` and serialise the returned lints, as
//! `harper::lint` does.

use std::collections::HashMap;

use grammachy::engines::harper::lints::issues_from;
use grammachy::envelope::{Category, Issue};
use harper_core::linting::Lint;
use serde::Deserialize;

/// One recorded case: the text that was linted and the lints it earned.
#[derive(Debug, Deserialize)]
struct Case {
    text: String,
    lints: Vec<Lint>,
}

fn case(name: &str) -> Case {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/harper-lints.json"
    );
    let text = std::fs::read_to_string(path).expect("the fixture is readable");
    let mut cases: HashMap<String, Case> =
        serde_json::from_str(&text).expect("the fixture is a case map");
    cases
        .remove(name)
        .unwrap_or_else(|| panic!("the fixture holds the case {name}"))
}

fn issues_of(name: &str) -> (String, Vec<Issue>) {
    let case = case(name);
    let issues = issues_from(&case.text, &case.lints);
    (case.text, issues)
}

/// Every Issue addresses the text it came from, in UTF-16 code units.
fn slice_holds(text: &str, issues: &[Issue]) {
    let units: Vec<u16> = text.encode_utf16().collect();
    for issue in issues {
        let slice = String::from_utf16(&units[issue.start..issue.end]).expect("the span is whole");
        assert_eq!(slice, issue.original, "the span does not hold the original");
    }
}

#[test]
fn a_spelling_lint_and_a_grammar_lint_map_to_their_issues() {
    let (text, issues) = issues_of("ordering");

    assert_eq!(issues.len(), 2);

    assert_eq!(issues[0].start, 27);
    assert_eq!(issues[0].end, 30);
    assert_eq!(issues[0].original, "teh");
    // Three suggestions, and only the first one is the fix.
    assert_eq!(issues[0].fix, "the");
    assert_eq!(issues[0].category, Category::Spelling);
    assert_eq!(issues[0].rule_id.as_deref(), Some("Spelling"));
    assert_eq!(issues[0].reason, "Did you mean to spell `teh` this way?");

    assert_eq!(issues[1].start, 45);
    assert_eq!(issues[1].end, 47);
    assert_eq!(issues[1].original, "go");
    assert_eq!(issues[1].fix, "goes");
    assert_eq!(issues[1].category, Category::Grammar);
    assert_eq!(issues[1].rule_id.as_deref(), Some("Agreement"));

    slice_holds(&text, &issues);
}

/// The recording carries a second lint on `teh`, so the earlier and tighter
/// Issue wins and the Issues come out sorted by `start`.
#[test]
fn overlapping_lints_leave_one_issue_and_the_order_holds() {
    let case = case("ordering");
    assert_eq!(case.lints.len(), 3);

    let issues = issues_from(&case.text, &case.lints);
    let starts: Vec<usize> = issues.iter().map(|issue| issue.start).collect();

    assert_eq!(starts, vec![27, 45]);
}

/// A lint with no suggestion has no `fix` to offer, so it is dropped.
#[test]
fn a_lint_without_a_suggestion_is_dropped() {
    let case = case("no_suggestion");
    assert_eq!(case.lints.len(), 1);
    assert!(case.lints[0].suggestions.is_empty());

    assert!(issues_from(&case.text, &case.lints).is_empty());
}

/// Harper counts `char`s and the contract counts UTF-16 code units, so eight
/// astral characters move every later span eight units along.
#[test]
fn an_astral_character_moves_the_span() {
    let (text, issues) = issues_of("astral");

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].original, "teh");
    // The lint sits at char 17 and the eight astral characters in front of it
    // are two UTF-16 code units each.
    assert_eq!(issues[0].start, 25);
    assert_eq!(issues[0].end, 28);
    assert_eq!(issues[1].original, "go");
    assert_eq!(issues[1].start, 42);

    slice_holds(&text, &issues);
}

/// Spec section 4 gives Harper a 10 s timeout, which the adapter applies by
/// linting on a worker thread. A timeout of nothing proves the path.
#[test]
fn a_check_that_outruns_the_timeout_is_an_engine_timeout() {
    use grammachy::args::{CheckOptions, EngineSlug};
    use grammachy::engine::{Engine, EngineFailure};
    use grammachy::engines::harper::Harper;
    use std::time::Duration;

    let options = CheckOptions {
        engine: EngineSlug::Harper,
        ..CheckOptions::default()
    };
    let failure = Harper::new(Duration::ZERO)
        .check("He go home.", &options)
        .expect_err("nothing finishes within no time at all");

    assert!(matches!(failure, EngineFailure::Timeout(_)), "{failure:?}");
}
