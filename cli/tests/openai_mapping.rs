//! Suggestion to Issue mapping for the `openai` engine, spec section 5.1.
//!
//! The three fixtures are chat completions whose content is the output HUF-171
//! recorded from a model on the benchmark test set, plus the added unanchored
//! suggestions each file names in its `_note`. Every case runs without a
//! server.

use grammachy::engines::openai::response::{issues_from, ChatResponse};
use grammachy::envelope::{Category, Issue};

/// Item zh-02 of the benchmark test set.
const BOOK_TEXT: &str = "She bought three book from the store.";

/// Item es-04 of the benchmark test set.
const RAIN_TEXT: &str = "Is raining a lot in Madrid today.";

fn response(name: &str) -> ChatResponse {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} is readable"));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{path} is a chat completion: {error}"))
}

/// The guarantee the shell relies on: `text.slice(start, end) === original`.
fn slice_holds(text: &str, issues: &[Issue]) {
    let units: Vec<u16> = text.encode_utf16().collect();
    for issue in issues {
        let slice = String::from_utf16(&units[issue.start..issue.end]).expect("the span is text");

        assert_eq!(slice, issue.original, "span {}..{}", issue.start, issue.end);
        assert_ne!(issue.fix, issue.original, "the fix is a change");
    }
}

#[test]
fn the_recorded_answer_maps_to_its_two_issues() {
    let issues =
        issues_from(BOOK_TEXT, &response("openai-response.json")).expect("the answer maps");

    assert_eq!(issues.len(), 2);

    assert_eq!(issues[0].start, 17);
    assert_eq!(issues[0].end, 21);
    assert_eq!(issues[0].original, "book");
    assert_eq!(issues[0].fix, "books");
    assert_eq!(issues[0].category, Category::Grammar);
    // A model cites no rule, so the optional field stays out of the envelope.
    assert_eq!(issues[0].rule_id, None);
    // The recorded reason is one word, and the envelope carries one sentence.
    assert_eq!(issues[0].reason, "plural.");

    assert_eq!(issues[1].start, 22);
    assert_eq!(issues[1].end, 26);
    assert_eq!(issues[1].original, "from");
    assert_eq!(issues[1].fix, "at");

    slice_holds(BOOK_TEXT, &issues);
}

#[test]
fn a_suggestion_the_text_does_not_hold_is_dropped() {
    let issues =
        issues_from(BOOK_TEXT, &response("openai-hallucination.json")).expect("the answer maps");

    // Only the anchored suggestion survives. "buyed three books" and "teh" are
    // both quotations of a sentence the model imagined.
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].original, "book");
    slice_holds(BOOK_TEXT, &issues);
}

#[test]
fn overlapping_suggestions_keep_the_earlier_one() {
    let issues = issues_from(RAIN_TEXT, &response("openai-overlap.json")).expect("the answer maps");

    // "a" first appears inside "raining", so the two suggestions address the
    // same code units and only the earlier Issue survives.
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].start, 3);
    assert_eq!(issues[0].end, 10);
    assert_eq!(issues[0].original, "raining");
    assert_eq!(issues[0].fix, "rain");
    slice_holds(RAIN_TEXT, &issues);
}

#[test]
fn issues_come_out_sorted_and_disjoint() {
    let content = r#"[
        {"original": "the store", "fix": "the shop", "reason": "word choice", "category": "grammar"},
        {"original": "book", "fix": "books", "reason": "plural", "category": "grammar"},
        {"original": "bought", "fix": "buys", "reason": "tense", "category": "grammar"}
    ]"#;
    let issues = issues_from(BOOK_TEXT, &completion(content)).expect("the answer maps");

    let starts: Vec<usize> = issues.iter().map(|issue| issue.start).collect();
    assert_eq!(starts, [4, 17, 27]);

    let mut previous_end = 0;
    for issue in &issues {
        assert!(issue.start >= previous_end, "Issues never overlap");
        previous_end = issue.end;
    }
    slice_holds(BOOK_TEXT, &issues);
}

#[test]
fn a_fix_that_changes_nothing_is_not_an_issue() {
    let content = r#"[
        {"original": "book", "fix": "book", "reason": "looks fine", "category": "grammar"},
        {"original": "from", "fix": "at", "reason": "preposition", "category": "grammar"}
    ]"#;
    let issues = issues_from(BOOK_TEXT, &completion(content)).expect("the answer maps");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].original, "from");
}

#[test]
fn a_suggestion_with_no_fix_is_not_an_issue() {
    let content = r#"[{"original": "book", "reason": "plural", "category": "grammar"}]"#;

    assert!(issues_from(BOOK_TEXT, &completion(content))
        .expect("the answer maps")
        .is_empty());
}

#[test]
fn an_empty_fix_deletes_the_span() {
    // Item ms-04 of the test set: the fix for "discussed about" is a deletion.
    let text = "We discussed about the new project for hours.";
    let content = r#"[{"original": " about", "fix": "", "reason": "Redundant preposition", "category": "grammar"}]"#;

    let issues = issues_from(text, &completion(content)).expect("the answer maps");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].start, 12);
    assert_eq!(issues[0].end, 18);
    assert_eq!(issues[0].fix, "");
}

#[test]
fn the_category_comes_from_the_model_and_falls_back_to_grammar() {
    let content = r#"[
        {"original": "book", "fix": "books", "reason": "plural", "category": "spelling"},
        {"original": "from", "fix": "at", "reason": "preposition", "category": "made up"}
    ]"#;
    let issues = issues_from(BOOK_TEXT, &completion(content)).expect("the answer maps");

    assert_eq!(issues[0].category, Category::Spelling);
    assert_eq!(issues[1].category, Category::Grammar);
}

#[test]
fn spans_count_utf16_units_across_a_surrogate_pair() {
    let text = "\u{1F600} She bought three book from teh store.";
    let content = r#"[
        {"original": "book", "fix": "books", "reason": "plural", "category": "grammar"},
        {"original": "teh", "fix": "the", "reason": "misspelling", "category": "spelling"}
    ]"#;
    let issues = issues_from(text, &completion(content)).expect("the answer maps");

    assert_eq!(issues.len(), 2);
    // The emoji is two UTF-16 code units, so every span sits two units later
    // than its byte offset would suggest.
    assert_eq!(issues[0].start, 20);
    assert_eq!(issues[1].category, Category::Spelling);
    slice_holds(text, &issues);
}

#[test]
fn a_wrapped_array_is_still_read() {
    let content = "Here is what I found:\n```json\n[{\"original\": \"book\", \"fix\": \"books\", \"reason\": \"plural\", \"category\": \"grammar\"}]\n```";

    let issues = issues_from(BOOK_TEXT, &completion(content)).expect("the answer maps");

    assert_eq!(issues.len(), 1);
}

#[test]
fn a_correct_sentence_maps_to_no_issues() {
    let issues = issues_from(BOOK_TEXT, &completion("[]")).expect("the answer maps");

    assert!(issues.is_empty());
}

#[test]
fn an_answer_with_no_message_is_an_engine_error() {
    let empty: ChatResponse = serde_json::from_str(r#"{"choices": []}"#).expect("the shape parses");
    assert!(issues_from(BOOK_TEXT, &empty).is_err());

    let failed: ChatResponse =
        serde_json::from_str(r#"{"error": {"message": "no model loaded"}}"#).expect("it parses");
    let message = issues_from(BOOK_TEXT, &failed).expect_err("the server failed");
    assert!(message.contains("no model loaded"), "{message}");
}

/// One chat completion carrying `content` as the model's answer.
fn completion(content: &str) -> ChatResponse {
    let document = serde_json::json!({
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content } }]
    });
    serde_json::from_value(document).expect("the shape parses")
}
