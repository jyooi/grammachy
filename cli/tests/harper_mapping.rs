use std::collections::HashMap;

use grammachy::engines::harper::lints::issues_from;
use grammachy::envelope::{Category, Issue};
use harper_core::linting::Lint;
use serde::Deserialize;

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

#[test]
fn overlapping_lints_leave_one_issue_and_the_order_holds() {
    let case = case("ordering");
    assert_eq!(case.lints.len(), 3);

    let issues = issues_from(&case.text, &case.lints);
    let starts: Vec<usize> = issues.iter().map(|issue| issue.start).collect();

    assert_eq!(starts, vec![27, 45]);
}

#[test]
fn a_lint_without_a_suggestion_is_dropped() {
    let case = case("no_suggestion");
    assert_eq!(case.lints.len(), 1);
    assert!(case.lints[0].suggestions.is_empty());

    assert!(issues_from(&case.text, &case.lints).is_empty());
}

#[test]
fn an_astral_character_moves_the_span() {
    let (text, issues) = issues_of("astral");

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].original, "teh");
    assert_eq!(issues[0].start, 25);
    assert_eq!(issues[0].end, 28);
    assert_eq!(issues[1].original, "go");
    assert_eq!(issues[1].start, 42);

    slice_holds(&text, &issues);
}

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
