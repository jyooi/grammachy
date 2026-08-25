//! Match to Issue mapping, spec section 5.1.
//!
//! Every case runs against a stored LanguageTool `/v2/check` answer, so the
//! rules are checked without a server.

use grammachy::engines::languagetool::response::{issues_from, CheckResponse};
use grammachy::envelope::{Category, Issue};

const ORDERING_TEXT: &str = "She bought three book from teh store, and he go home.";

fn response(name: &str) -> CheckResponse {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{path} is a /v2/check answer: {error}"))
}

fn ordering_issues() -> Vec<Issue> {
    issues_from(ORDERING_TEXT, &response("languagetool-ordering.json"))
}

#[test]
fn issues_come_out_sorted_by_start() {
    let issues = ordering_issues();
    let starts: Vec<usize> = issues.iter().map(|issue| issue.start).collect();

    assert_eq!(starts, [17, 27, 45]);
}

#[test]
fn an_overlapping_match_loses_to_the_earlier_issue() {
    let issues = ordering_issues();
    let rule_ids: Vec<&str> = issues
        .iter()
        .filter_map(|issue| issue.rule_id.as_deref())
        .collect();

    assert!(!rule_ids.contains(&"OVERLAPPING_SAME_START"));
    assert!(!rule_ids.contains(&"OVERLAPPING_LATER_START"));
    // No Issue may reach into the next one.
    for pair in issues.windows(2) {
        assert!(pair[0].end <= pair[1].start, "{pair:?} overlap");
    }
}

#[test]
fn a_no_op_fix_and_a_match_without_a_suggestion_are_dropped() {
    let rule_ids: Vec<String> = ordering_issues()
        .into_iter()
        .filter_map(|issue| issue.rule_id)
        .collect();

    assert!(!rule_ids.contains(&"NO_OP_FIX".to_string()));
    assert!(!rule_ids.contains(&"NO_SUGGESTION".to_string()));
    for issue in ordering_issues() {
        assert_ne!(issue.fix, issue.original);
    }
}

#[test]
fn a_style_match_is_never_reported() {
    let rule_ids: Vec<String> = ordering_issues()
        .into_iter()
        .filter_map(|issue| issue.rule_id)
        .collect();

    assert!(!rule_ids.contains(&"TOO_WORDY".to_string()));
}

#[test]
fn the_first_suggestion_is_the_fix_and_the_slice_is_the_original() {
    let issues = ordering_issues();

    assert_eq!(issues[0].original, "book");
    assert_eq!(issues[0].fix, "books");
    assert_eq!(issues[0].category, Category::Grammar);
    assert_eq!(issues[0].rule_id.as_deref(), Some("CD_NN"));

    // The typo offers "the" and "ten"; the first suggestion wins.
    assert_eq!(issues[1].original, "teh");
    assert_eq!(issues[1].fix, "the");
    assert_eq!(issues[1].category, Category::Spelling);
}

#[test]
fn the_reason_is_prose_without_the_rule_id() {
    for issue in ordering_issues() {
        let rule_id = issue.rule_id.clone().unwrap_or_default();
        assert!(
            !issue.reason.contains(&rule_id),
            "{} leaks its rule id",
            issue.reason
        );
        assert!(!issue.reason.is_empty());
    }
}

/// The guarantee the shell relies on: `text.slice(start, end) === original`.
fn slice_holds(text: &str, issues: &[Issue]) {
    let units: Vec<u16> = text.encode_utf16().collect();
    for issue in issues {
        let slice = String::from_utf16(&units[issue.start..issue.end]).expect("the span is text");
        assert_eq!(slice, issue.original, "span {}..{}", issue.start, issue.end);
    }
}

#[test]
fn every_span_slices_back_to_its_original() {
    slice_holds(ORDERING_TEXT, &ordering_issues());
}

#[test]
fn a_span_after_a_surrogate_pair_counts_two_units_for_it() {
    // The astral character is two UTF-16 units but four bytes, so "go" sits at
    // UTF-16 offset 6 and byte offset 8.
    let text = "\u{1F600} He go home.";
    let response: CheckResponse = serde_json::from_str(
        r#"{"matches":[{"message":"Agreement error.","offset":6,"length":2,
            "replacements":[{"value":"goes"}],
            "rule":{"id":"AGREEMENT","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    )
    .expect("the answer parses");

    let issues = issues_from(text, &response);

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].original, "go");
    slice_holds(text, &issues);
}

#[test]
fn a_span_that_splits_a_surrogate_pair_is_dropped() {
    let text = "\u{1F600} He go home.";
    let response: CheckResponse = serde_json::from_str(
        r#"{"matches":[{"message":"Broken span.","offset":1,"length":2,
            "replacements":[{"value":"x"}],
            "rule":{"id":"BROKEN","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    )
    .expect("the answer parses");

    assert!(issues_from(text, &response).is_empty());
}

#[test]
fn a_span_past_the_end_is_dropped() {
    let response: CheckResponse = serde_json::from_str(
        r#"{"matches":[{"message":"Out of range.","offset":900,"length":4,
            "replacements":[{"value":"x"}],
            "rule":{"id":"OUT_OF_RANGE","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    )
    .expect("the answer parses");

    assert!(issues_from("Short text.", &response).is_empty());
}

#[test]
fn crlf_line_endings_keep_the_spans_honest() {
    // Each "\r\n" is two UTF-16 units, so the second line starts at 15.
    let text = "He go home.\r\n\r\nShe have a cat.";
    assert_eq!(text.encode_utf16().count(), 30);

    let response: CheckResponse = serde_json::from_str(
        r#"{"matches":[
            {"message":"Agreement error.","offset":3,"length":2,
             "replacements":[{"value":"goes"}],
             "rule":{"id":"A","issueType":"grammar","category":{"id":"GRAMMAR"}}},
            {"message":"Agreement error.","offset":19,"length":4,
             "replacements":[{"value":"has"}],
             "rule":{"id":"B","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    )
    .expect("the answer parses");

    let issues = issues_from(text, &response);

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].original, "go");
    assert_eq!(issues[1].original, "have");
    slice_holds(text, &issues);
}

#[test]
fn multi_paragraph_text_keeps_one_offset_space() {
    let text =
        "First paragraph have a mistake.\n\nSecond paragraph go wrong too.\n\nThird one is fine.";
    assert_eq!(text.find("have"), Some(16));
    assert_eq!(text.find("go"), Some(50));

    let response: CheckResponse = serde_json::from_str(
        r#"{"matches":[
            {"message":"Agreement error.","offset":16,"length":4,
             "replacements":[{"value":"has"}],
             "rule":{"id":"A","issueType":"grammar","category":{"id":"GRAMMAR"}}},
            {"message":"Agreement error.","offset":50,"length":2,
             "replacements":[{"value":"goes"}],
             "rule":{"id":"B","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    )
    .expect("the answer parses");

    let issues = issues_from(text, &response);

    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].original, "have");
    assert_eq!(issues[1].original, "go");
    slice_holds(text, &issues);
}

#[test]
fn an_answer_with_no_matches_is_an_empty_issue_list() {
    let response: CheckResponse = serde_json::from_str(r#"{"matches":[]}"#).expect("it parses");

    assert!(issues_from("All fine here.", &response).is_empty());
}
