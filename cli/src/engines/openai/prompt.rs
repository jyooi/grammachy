//! The prompt and the request body of one chat completion.
//!
//! The wording is the one HUF-181 measured, kept as it stands because the
//! measurement belongs to it. The report's finding was that the "shortest exact
//! substring" rule is what makes the output usable: an earlier, looser prompt
//! caught more but quoted whole sentences, and a whole-sentence Issue does not
//! fit a per-Issue Accept and Skip Panel.
//!
//! HUF-218 added the second rule the wording carries: the answer is compact and
//! the `reason` is at most six words. An indented answer with a full sentence
//! of reason cost about 56 tokens per Issue, which is what stopped a whole
//! Chunk from finishing inside the Check timeout. The same Issues cost about 30
//! tokens compact. One prompt says this to every engine (HUF-219).
//!
//! Asking is not forcing, so a request takes one of two routes to the same
//! shape, and [`super::force_of`] is the one place that picks it. [`GRAMMAR`]
//! is a raw GBNF that admits no whitespace between two tokens, so compactness
//! is decided by the decoder rather than asked for. Only llama-server reads it,
//! and it bounds the whole generation, so it leaves no room for a think. A
//! Check with Local thinking on therefore keeps the `json_schema` response
//! format, and so does every cloud provider, because none of them reads a
//! grammar.
//!
//! Two things differ from the benchmark runner. The native language is named
//! for every value but `none` (spec section 4), where the runner always had
//! one. And the schema carries `category`, because spec section 5.1 makes it a
//! required Issue field and only the model knows whether it saw a misspelling
//! or a grammar mistake.

use serde_json::{json, Value};

use crate::args::{CheckOptions, NativeLanguage};

/// How much room one answer gets, spec section 4: 1,024 tokens for thinking
/// and 1,024 tokens for the answer. The unit caps the thinking half itself
/// with `--reasoning-budget`, so a runaway think never eats the answer. One
/// Check is at most 5,000 UTF-16 units and a compact Issue costs about 30
/// tokens, so the answer half is far past any honest answer and still bounds a
/// model that will not stop.
///
/// This number, and nothing on the unit, is what bounds the grammar route. A
/// probe measured it: gemma-4-E4B-it behind the current unit flags, a 1,991
/// UTF-16 unit error-dense Draft, [`GRAMMAR`], and `enable_thinking` false
/// answered `finish_reason` `stop` after 1,250 completion tokens, with 54
/// well-formed Issues in `content` and an empty `reasoning_content`. So
/// `--reasoning-budget 1024` bounds the think alone, and it never cuts a
/// grammar-forced answer short.
const MAX_TOKENS: u32 = 2_048;

/// The decoding grammar llama-server is given in place of the response format,
/// on the thinking-off route alone.
///
/// It is the array of [`schema`] written as GBNF, with every whitespace rule of
/// the stock `json.gbnf` removed. There is no rule that can emit a space or a
/// newline, so the server cannot indent the answer however the model would like
/// to, and the compactness of HUF-219 is forced rather than asked.
///
/// The field order is fixed here too. A grammar names the keys in one order,
/// which costs the model nothing and keeps every answer the same shape.
pub const GRAMMAR: &str = concat!(
    "root ::= \"[\" (issue (\",\" issue)*)? \"]\"\n",
    "issue ::= \"{\\\"original\\\":\" string \",\\\"fix\\\":\" string ",
    "\",\\\"reason\\\":\" string \",\\\"category\\\":\" category \"}\"\n",
    "category ::= \"\\\"grammar\\\"\" | \"\\\"spelling\\\"\"\n",
    "string ::= \"\\\"\" char* \"\\\"\"\n",
    "char ::= [^\"\\\\\\x7F\\x00-\\x1F] | ",
    "\"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F])\n",
);

/// How a server is made to answer the shape of [`schema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Force {
    /// llama-server with Local thinking off takes a raw GBNF, the one route
    /// that can forbid whitespace between two tokens.
    Grammar,
    /// Local thinking on, or a cloud provider. Both need the whole generation
    /// left free, so both take the OpenAI response format and no grammar.
    JsonSchema,
}

/// The language name the prompt uses, or `None` for `none`.
///
/// `ms` is named here although LanguageTool has no `motherTongue` for it: a
/// model needs no rule pack to know what a Malay speaker writes.
pub fn native_name(native: NativeLanguage) -> Option<&'static str> {
    match native {
        NativeLanguage::None => None,
        NativeLanguage::Zh => Some("Mandarin Chinese"),
        NativeLanguage::Ms => Some("Malay"),
        NativeLanguage::Es => Some("Spanish"),
        NativeLanguage::Fr => Some("French"),
        NativeLanguage::De => Some("German"),
        NativeLanguage::Pt => Some("Portuguese"),
        NativeLanguage::Ja => Some("Japanese"),
    }
}

/// The one user message of the Check.
pub fn build(text: &str, options: &CheckOptions) -> String {
    let mut lines = vec![format!(
        "You are a grammar and spelling checker for {} English.",
        options.target.as_str()
    )];

    if let Some(name) = native_name(options.native) {
        lines.push(format!(
            "The writer's native language is {name}. Look for mistakes such native speakers make when writing English (articles, tense, plural, false friends, word order, prepositions, agreement)."
        ));
    }

    lines.push(
        "Report only grammar and spelling mistakes. Do not report style or word choice that is already correct."
            .to_string(),
    );
    lines.push(
        "Return ONLY a JSON array. Each element is {\"original\": <the shortest exact substring of the text that contains the mistake, usually one to three words, never the whole sentence>, \"fix\": <replacement for that substring only>, \"reason\": <why it is wrong, at most six words>, \"category\": <\"spelling\" for a misspelled word, otherwise \"grammar\">}. Return [] if the text is correct. No prose, no markdown."
            .to_string(),
    );
    lines.push("Write the JSON compact: no spaces and no newlines between tokens.".to_string());
    lines.push(String::new());
    lines.push(format!("Text: {}", Value::String(text.to_string())));

    lines.join("\n")
}

/// The JSON schema a provider turns into a decoding grammar, so the answer is
/// always a well-formed array.
pub fn schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "original": { "type": "string" },
                "fix": { "type": "string" },
                "reason": { "type": "string" },
                "category": { "type": "string", "enum": ["grammar", "spelling"] }
            },
            "required": ["original", "fix", "reason", "category"],
            "additionalProperties": false
        }
    })
}

/// The whole POST body of one `/v1/chat/completions` request.
pub fn request_body(text: &str, options: &CheckOptions, force: Force) -> Value {
    let mut body = json!({
        "model": options.openai_model,
        "messages": [{ "role": "user", "content": build(text, options) }],
        // A Check is a classification, not a piece of writing.
        "temperature": 0,
        "max_tokens": MAX_TOKENS,
        "stream": false,
        // Spec section 4. llama.cpp reads this through the chat template of
        // the model file, so a change of the Setting needs no unit restart.
        // The unit caps the think with --reasoning-budget, so a runaway think
        // cannot spend the whole answer budget before the first bracket.
        "chat_template_kwargs": { "enable_thinking": options.local_thinking }
    });

    let fields = body.as_object_mut().expect("the body is an object");
    match force {
        Force::Grammar => {
            fields.insert("grammar".to_string(), json!(GRAMMAR));
        }
        Force::JsonSchema => {
            fields.insert(
                "response_format".to_string(),
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": "issues", "schema": schema() }
                }),
            );
        }
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{EngineSlug, TargetEnglish};

    fn options(native: NativeLanguage) -> CheckOptions {
        CheckOptions {
            native,
            target: TargetEnglish::EnUs,
            engine: EngineSlug::Openai,
            ..CheckOptions::default()
        }
    }

    #[test]
    fn the_text_is_carried_as_a_json_string() {
        let prompt = build("He say \"hi\".\nShe go.", &options(NativeLanguage::None));

        assert!(
            prompt.ends_with("Text: \"He say \\\"hi\\\".\\nShe go.\""),
            "{prompt}"
        );
    }

    #[test]
    fn thinking_on_is_what_the_default_options_send() {
        let body = request_body(
            "He go home.",
            &options(NativeLanguage::None),
            Force::Grammar,
        );

        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
        assert_eq!(body["max_tokens"], 2048);
    }

    #[test]
    fn thinking_off_is_carried_on_the_same_key() {
        let mut off = options(NativeLanguage::None);
        off.local_thinking = false;
        let body = request_body("He go home.", &off, Force::Grammar);

        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(body["max_tokens"], 2048);
    }

    #[test]
    fn the_body_names_the_model_and_carries_the_grammar() {
        let body = request_body(
            "He go home.",
            &options(NativeLanguage::None),
            Force::Grammar,
        );

        assert_eq!(body["model"], "gemma-4-e4b-it");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["grammar"], GRAMMAR);
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn the_json_schema_route_carries_the_schema_and_no_grammar() {
        let body = request_body(
            "He go home.",
            &options(NativeLanguage::None),
            Force::JsonSchema,
        );

        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["items"]["required"],
            json!(["original", "fix", "reason", "category"])
        );
        assert!(body.get("grammar").is_none());
    }
}
