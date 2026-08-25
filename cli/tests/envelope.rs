//! The envelopes must match spec section 5.1 field for field.

use grammachy::envelope::{Category, Envelope, ErrorCode, Issue};
use serde_json::{json, Value};

fn parse(envelope: &Envelope) -> Value {
    serde_json::from_str(&envelope.to_json()).expect("the envelope is JSON")
}

fn sample_issue() -> Issue {
    Issue {
        start: 17,
        end: 21,
        original: "book".to_string(),
        fix: "books".to_string(),
        reason: "Possible agreement error. The noun 'book' seems to be countable.".to_string(),
        category: Category::Grammar,
        rule_id: Some("CD_NN".to_string()),
    }
}

#[test]
fn result_envelope_matches_the_spec() {
    let envelope = Envelope::result("languagetool", 23, vec![sample_issue()]);

    // Comparing parsed values, so key order in the JSON text does not matter.
    assert_eq!(
        parse(&envelope),
        json!({
            "contractVersion": 1,
            "engine": "languagetool",
            "elapsedMs": 23,
            "issues": [{
                "start": 17,
                "end": 21,
                "original": "book",
                "fix": "books",
                "reason": "Possible agreement error. The noun 'book' seems to be countable.",
                "category": "grammar",
                "ruleId": "CD_NN"
            }]
        })
    );
}

#[test]
fn result_envelope_has_no_extra_fields() {
    let envelope = Envelope::result("harper", 4, vec![sample_issue()]);
    let value = parse(&envelope);

    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["contractVersion", "elapsedMs", "engine", "issues"]);

    let issue = &value["issues"][0];
    let mut issue_keys: Vec<&str> = issue
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    issue_keys.sort_unstable();
    assert_eq!(
        issue_keys,
        ["category", "end", "fix", "original", "reason", "ruleId", "start"]
    );
}

#[test]
fn an_issue_without_a_rule_id_omits_the_field() {
    let issue = Issue {
        rule_id: None,
        category: Category::Spelling,
        ..sample_issue()
    };
    let value = parse(&Envelope::result("harper", 1, vec![issue]));

    assert_eq!(value["issues"][0].get("ruleId"), None);
    assert_eq!(value["issues"][0]["category"], "spelling");
}

#[test]
fn an_empty_result_is_a_success_envelope() {
    let envelope = Envelope::result("languagetool", 7, Vec::new());

    assert_eq!(envelope.exit_code(), 0);
    assert_eq!(
        parse(&envelope),
        json!({
            "contractVersion": 1,
            "engine": "languagetool",
            "elapsedMs": 7,
            "issues": []
        })
    );
}

#[test]
fn error_envelope_matches_the_spec() {
    let envelope = Envelope::error(
        ErrorCode::EngineUnavailable,
        "LanguageTool did not answer on 127.0.0.1:8081",
    );

    assert_eq!(envelope.exit_code(), 1);
    assert_eq!(
        parse(&envelope),
        json!({
            "contractVersion": 1,
            "error": {
                "code": "engine_unavailable",
                "message": "LanguageTool did not answer on 127.0.0.1:8081"
            }
        })
    );
}

#[test]
fn error_envelope_has_no_extra_fields() {
    let value = parse(&Envelope::error(ErrorCode::BadArguments, "bad flag"));

    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["contractVersion", "error"]);

    let mut error_keys: Vec<&str> = value["error"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    error_keys.sort_unstable();
    assert_eq!(error_keys, ["code", "message"]);
}

#[test]
fn every_error_code_serialises_as_the_shell_expects() {
    let codes = [
        (ErrorCode::EmptySelection, "empty_selection"),
        (ErrorCode::TextTooLong, "text_too_long"),
        (ErrorCode::EngineUnavailable, "engine_unavailable"),
        (ErrorCode::EngineTimeout, "engine_timeout"),
        (ErrorCode::EngineError, "engine_error"),
        (ErrorCode::BadArguments, "bad_arguments"),
    ];

    for (code, wire) in codes {
        let value = parse(&Envelope::error(code, "message"));
        assert_eq!(value["error"]["code"], wire);
    }
}
