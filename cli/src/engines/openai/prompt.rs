//! The prompt and the request body of one chat completion.
//!
//! The wording is the one HUF-181 measured, kept as it stands because the
//! measurement belongs to it. The report's finding was that the "shortest exact
//! substring" rule is what makes the output usable: an earlier, looser prompt
//! caught more but quoted whole sentences, and a whole-sentence Issue does not
//! fit a per-Issue Accept and Skip Panel.
//!
//! Two things differ from the benchmark runner. The native language is named
//! for every value but `none` (spec section 4), where the runner always had
//! one. And the schema carries `category`, because spec section 5.1 makes it a
//! required Issue field and only the model knows whether it saw a misspelling
//! or a grammar mistake.

use serde_json::{json, Value};

use crate::args::{CheckOptions, NativeLanguage};

/// How much room the answer gets. One Check is at most 5,000 UTF-16 units, and
/// an Issue costs about 40 tokens, so this is far past any honest answer and
/// still bounds a model that will not stop.
const MAX_TOKENS: u32 = 1_024;

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
        "Return ONLY a JSON array. Each element is {\"original\": <the shortest exact substring of the text that contains the mistake, usually one to three words, never the whole sentence>, \"fix\": <replacement for that substring only>, \"reason\": <short reason, one sentence>, \"category\": <\"spelling\" for a misspelled word, otherwise \"grammar\">}. Return [] if the text is correct. No prose, no markdown."
            .to_string(),
    );
    lines.push(String::new());
    lines.push(format!("Text: {}", Value::String(text.to_string())));

    lines.join("\n")
}

/// The JSON schema the server turns into a decoding grammar, so the answer is
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
pub fn request_body(text: &str, options: &CheckOptions) -> Value {
    json!({
        "model": options.openai_model,
        "messages": [{ "role": "user", "content": build(text, options) }],
        // A Check is a classification, not a piece of writing.
        "temperature": 0,
        "max_tokens": MAX_TOKENS,
        "stream": false,
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": "issues", "schema": schema() }
        }
    })
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
    fn the_body_names_the_model_and_asks_for_the_schema() {
        let body = request_body("He go home.", &options(NativeLanguage::None));

        assert_eq!(body["model"], "gemma-4-e4b-it");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["items"]["required"],
            json!(["original", "fix", "reason", "category"])
        );
    }
}
