//! The prompt of the `openai` engine, spec section 4.
//!
//! "The `openai` prompt names the native language for every value but `none`."
//! The wording itself is the one HUF-181 measured, so these cases pin the two
//! things the spec fixes rather than the prose.

use grammachy::args::{CheckOptions, NativeLanguage, TargetEnglish};
use grammachy::engines::openai::force_of;
use grammachy::engines::openai::prompt::{build, native_name, request_body, Force, GRAMMAR};

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
    let body = request_body(text, &options(NativeLanguage::None), Force::Grammar);

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
    let body = request_body(TEXT, &options(NativeLanguage::None), Force::JsonSchema);
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

#[test]
fn the_prompt_caps_the_reason_and_asks_for_compact_json() {
    // HUF-219: one wording for every engine, because a cloud provider takes no
    // grammar and has the prompt alone to go on.
    let prompt = build(TEXT, &options(NativeLanguage::None));

    assert!(prompt.contains("at most six words"), "{prompt}");
    assert!(
        prompt.contains("no spaces and no newlines between tokens"),
        "{prompt}"
    );
}

/// The Local thinking Setting of spec section 4 picks the forcing route, so
/// both Toggle positions stay live. A grammar bounds the whole generation, so
/// thinking on has to keep the response format instead.
#[test]
fn the_thinking_setting_picks_the_forcing_route() {
    let thinking_on = CheckOptions {
        local_thinking: true,
        ..options(NativeLanguage::Fr)
    };
    let thinking_off = CheckOptions {
        local_thinking: false,
        ..options(NativeLanguage::Fr)
    };

    let on = request_body(TEXT, &thinking_on, force_of(&thinking_on));
    assert_eq!(on["response_format"]["type"], "json_schema");
    assert!(on.get("grammar").is_none(), "{on}");
    assert_eq!(on["chat_template_kwargs"]["enable_thinking"], true);

    let off = request_body(TEXT, &thinking_off, force_of(&thinking_off));
    assert_eq!(off["grammar"], serde_json::json!(GRAMMAR));
    assert!(off.get("response_format").is_none(), "{off}");
    assert_eq!(off["chat_template_kwargs"]["enable_thinking"], false);

    // HUF-219: the wording is one prompt, whatever forces the shape.
    assert_eq!(on["messages"], off["messages"]);
}

#[test]
fn the_two_engines_get_the_same_prompt() {
    let local = request_body(TEXT, &options(NativeLanguage::Fr), Force::Grammar);
    let cloud = request_body(TEXT, &options(NativeLanguage::Fr), Force::JsonSchema);

    assert_eq!(
        local["messages"], cloud["messages"],
        "the wording is one prompt for every engine"
    );
}

#[test]
fn the_grammar_is_the_one_llama_server_is_given() {
    // Pinned whole, because a decoding grammar is a contract with the server
    // and a stray character in it is an HTTP 400 rather than a worse answer.
    assert_eq!(
        GRAMMAR,
        concat!(
            "root ::= \"[\" (issue (\",\" issue)*)? \"]\"\n",
            "issue ::= \"{\\\"original\\\":\" string \",\\\"fix\\\":\" string ",
            "\",\\\"reason\\\":\" string \",\\\"category\\\":\" category \"}\"\n",
            "category ::= \"\\\"grammar\\\"\" | \"\\\"spelling\\\"\"\n",
            "string ::= \"\\\"\" char* \"\\\"\"\n",
            "char ::= [^\"\\\\\\x7F\\x00-\\x1F] | ",
            "\"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F])\n",
        )
    );
}

#[test]
fn no_structural_rule_of_the_grammar_can_emit_whitespace() {
    // The point of the grammar (HUF-219): the server cannot indent the answer,
    // so an Issue costs about 30 tokens rather than 56. Only `char` may carry a
    // space, because a reason is words.
    for line in GRAMMAR.lines() {
        let (name, body) = line.split_once("::=").expect("every line is a rule");
        if name.trim() == "char" {
            continue;
        }
        for literal in quoted_literals(body) {
            assert!(
                !literal.contains(' ') && !literal.contains('\t') && !literal.contains('\n'),
                "rule {} may emit whitespace: {literal:?}",
                name.trim()
            );
        }
    }
}

#[test]
fn the_grammar_names_the_four_issue_fields_in_the_schema_order() {
    let issue = GRAMMAR
        .lines()
        .find(|line| line.starts_with("issue ::="))
        .expect("the issue rule is there");
    let keys: Vec<&str> = ["original", "fix", "reason", "category"]
        .into_iter()
        .filter(|key| issue.contains(&format!("\\\"{key}\\\":")))
        .collect();

    assert_eq!(keys, ["original", "fix", "reason", "category"]);
}

/// Every double-quoted literal of one GBNF rule body.
///
/// GBNF escapes a quote inside a literal as `\"`, so a backslash guards the
/// character after it.
fn quoted_literals(body: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current: Option<String> = None;
    let mut escaped = false;
    for character in body.chars() {
        match current.as_mut() {
            Some(literal) if escaped => {
                literal.push(character);
                escaped = false;
            }
            Some(literal) if character == '\\' => {
                literal.push(character);
                escaped = true;
            }
            Some(_) if character == '"' => literals.push(current.take().expect("open literal")),
            Some(literal) => literal.push(character),
            None if character == '"' => current = Some(String::new()),
            None => {}
        }
    }
    literals
}
