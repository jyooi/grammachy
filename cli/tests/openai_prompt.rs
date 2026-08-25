//! The prompt of the `openai` engine, spec section 4.
//!
//! "The `openai` prompt names the native language for every value but `none`."
//! The wording itself is the one HUF-181 measured, so these cases pin the two
//! things the spec fixes rather than the prose.

use grammachy::args::{CheckOptions, NativeLanguage, TargetEnglish};
use grammachy::engines::openai::prompt::{build, native_name, request_body};

fn options(native: NativeLanguage) -> CheckOptions {
    CheckOptions {
        native,
        target: TargetEnglish::EnUs,
        ..CheckOptions::default()
    }
}

const TEXT: &str = "She bought three book from the store.";

#[test]
fn the_native_language_sentence_is_there_for_zh() {
    let prompt = build(TEXT, &options(NativeLanguage::Zh));

    assert!(
        prompt.contains("The writer's native language is Mandarin Chinese."),
        "{prompt}"
    );
}

#[test]
fn the_native_language_sentence_is_absent_for_none() {
    let prompt = build(TEXT, &options(NativeLanguage::None));

    assert!(!prompt.contains("native language"), "{prompt}");
    assert!(native_name(NativeLanguage::None).is_none());
}

#[test]
fn every_other_value_names_a_language() {
    // `ms` is named too, although LanguageTool has no `motherTongue` for it.
    for native in [
        NativeLanguage::Zh,
        NativeLanguage::Ms,
        NativeLanguage::Es,
        NativeLanguage::Fr,
        NativeLanguage::De,
        NativeLanguage::Pt,
        NativeLanguage::Ja,
    ] {
        let name = native_name(native).unwrap_or_else(|| panic!("{native:?} has a name"));
        assert!(
            build(TEXT, &options(native)).contains(&format!("native language is {name}.")),
            "{native:?} is named in the prompt"
        );
    }
}

#[test]
fn the_prompt_asks_for_grammar_and_spelling_only() {
    let prompt = build(TEXT, &options(NativeLanguage::None));

    assert!(prompt.contains("grammar and spelling checker for en-US English."));
    assert!(prompt.contains("Report only grammar and spelling mistakes."));
    // Depth in v1 is grammar and spelling; style is never reported.
    assert!(prompt.contains("Do not report style"));
}

#[test]
fn the_prompt_asks_for_the_shortest_substring() {
    // HUF-181: a looser prompt caught more and quoted whole sentences, which
    // does not fit a per-Issue Accept and Skip Panel.
    let prompt = build(TEXT, &options(NativeLanguage::None));

    assert!(prompt.contains("shortest exact substring"), "{prompt}");
    assert!(prompt.contains("never the whole sentence"), "{prompt}");
}

#[test]
fn the_text_reaches_the_model_verbatim() {
    let text = "He say \"hi\".\r\n\r\nShe go \u{1F600}.";
    let body = request_body(text, &options(NativeLanguage::None));

    let content = body["messages"][0]["content"]
        .as_str()
        .expect("the message is a string");
    // The prompt carries the text as a JSON string, so every newline, quote,
    // and astral character survives the round trip.
    let quoted = content
        .rsplit_once("Text: ")
        .expect("the prompt ends with the text")
        .1;
    assert_eq!(
        serde_json::from_str::<String>(quoted).expect("the text is a JSON string"),
        text
    );
}

#[test]
fn the_answer_is_pinned_to_the_issue_shape() {
    let body = request_body(TEXT, &options(NativeLanguage::None));
    let schema = &body["response_format"]["json_schema"]["schema"];

    assert_eq!(schema["type"], "array");
    assert_eq!(
        schema["items"]["properties"]["category"]["enum"][0],
        "grammar"
    );
    assert_eq!(
        schema["items"]["properties"]["category"]["enum"][1],
        "spelling"
    );
    assert_eq!(schema["items"]["additionalProperties"], false);
    // A Check is a classification, so nothing about it is sampled.
    assert_eq!(body["temperature"], 0);
}
