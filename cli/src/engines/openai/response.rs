//! The chat completion answer, and the mapping from its suggestions to the
//! Issues of spec section 5.1.
//!
//! A model quotes text; it does not count code units. So every suggestion is
//! placed by looking its `original` up in the text the CLI sent, and a
//! suggestion that quotes something the text does not hold is dropped. HUF-181
//! measured why that matters: small models emit both no-op suggestions, where
//! the fix equals the original, and unanchored ones. Neither is an Issue.

use serde::Deserialize;
use serde_json::Value;

use crate::envelope::{sorted_disjoint, Category, Issue};
use crate::text::{utf16_len, utf16_slice};

/// Only the fields the adapter reads of an OpenAI chat completion.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    #[serde(default)]
    pub choices: Vec<Choice>,
    /// Some servers answer a refusal or a load failure here instead.
    #[serde(default)]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub message: Option<Message>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub content: Option<String>,
}

/// One element of the array the model returns.
#[derive(Debug, Clone, Deserialize)]
struct Suggestion {
    #[serde(default)]
    original: String,
    /// Absent means the model named no replacement, so there is no Issue. The
    /// empty string is a replacement: it deletes the span.
    fix: Option<String>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    category: String,
}

/// The Issues of one answer, or why the answer is not one.
pub fn issues_from(text: &str, response: &ChatResponse) -> Result<Vec<Issue>, String> {
    if let Some(error) = &response.error {
        return Err(format!("The model server answered with an error: {error}"));
    }

    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.as_ref())
        .and_then(|message| message.content.as_deref())
        .ok_or_else(|| "The model server answered with no message.".to_string())?;

    Ok(issues_from_content(text, content))
}

/// Map the message content, whatever it is wrapped in, to Issues.
pub fn issues_from_content(text: &str, content: &str) -> Vec<Issue> {
    let Some(suggestions) = parse_array(content) else {
        // The schema makes this unreachable on llama.cpp, and a server that
        // ignores the schema is a server that found nothing usable.
        return Vec::new();
    };

    let mut placed: Vec<Issue> = Vec::with_capacity(suggestions.len());
    for suggestion in &suggestions {
        // The same quotation twice means two occurrences, not one twice.
        let seen = placed
            .iter()
            .filter(|issue| issue.original == suggestion.original)
            .count();
        if let Some(issue) = issue_of(text, suggestion, seen) {
            placed.push(issue);
        }
    }

    sorted_disjoint(placed)
}

/// The array inside the content, tolerating prose or a fence around it.
fn parse_array(content: &str) -> Option<Vec<Suggestion>> {
    let start = content.find('[')?;
    let end = content.rfind(']')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&content[start..=end]).ok()
}

/// One suggestion as an Issue, or nothing when it does not earn one.
///
/// `skip` is how many earlier Issues already quoted the same text, so a
/// repeated quotation lands on the next occurrence rather than on the first.
fn issue_of(text: &str, suggestion: &Suggestion, skip: usize) -> Option<Issue> {
    let fix = suggestion.fix.as_ref()?;
    if suggestion.original.is_empty() {
        return None;
    }
    // A fix that changes nothing is noise, and spec section 5.1 forbids it.
    if *fix == suggestion.original {
        return None;
    }

    let (start, end) = place(text, &suggestion.original, skip)?;
    // The text is what the shell will slice, so the quotation must be exact.
    if utf16_slice(text, start, end)? != suggestion.original {
        return None;
    }

    let category = match suggestion.category.as_str() {
        "spelling" => Category::Spelling,
        _ => Category::Grammar,
    };

    Some(Issue {
        start,
        end,
        original: suggestion.original.clone(),
        fix: fix.clone(),
        reason: reason_of(&suggestion.reason, category),
        category,
        // A model has no rule to cite, so the optional field stays out.
        rule_id: None,
    })
}

/// The UTF-16 span of the `skip`-th occurrence of `original`, or nothing when
/// the model quoted something the text does not hold.
fn place(text: &str, original: &str, skip: usize) -> Option<(usize, usize)> {
    let (byte_index, _) = text.match_indices(original).nth(skip)?;
    let start = utf16_len(&text[..byte_index]);
    Some((start, start + utf16_len(original)))
}

/// One English sentence with no rule id (spec section 5.1).
fn reason_of(reason: &str, category: Category) -> String {
    let collapsed = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return match category {
            Category::Grammar => "Possible grammar mistake.".to_string(),
            Category::Spelling => "Possible spelling mistake.".to_string(),
        };
    }
    if collapsed.ends_with(['.', '!', '?']) {
        collapsed
    } else {
        format!("{collapsed}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reason_becomes_one_sentence() {
        assert_eq!(
            reason_of("Redundant\n preposition", Category::Grammar),
            "Redundant preposition."
        );
        assert_eq!(
            reason_of("The noun is countable.", Category::Grammar),
            "The noun is countable."
        );
        assert_eq!(
            reason_of("  ", Category::Spelling),
            "Possible spelling mistake."
        );
    }

    #[test]
    fn a_repeated_quotation_lands_on_the_next_occurrence() {
        let text = "She read a book and he read a book.";

        assert_eq!(place(text, "book", 0), Some((11, 15)));
        assert_eq!(place(text, "book", 1), Some((30, 34)));
        assert_eq!(place(text, "book", 2), None);
    }

    #[test]
    fn a_span_after_an_astral_character_counts_utf16_units() {
        let text = "\u{1F600} He go home.";

        // The emoji is two UTF-16 code units, so "go" starts at 6, not at 5.
        assert_eq!(place(text, "go", 0), Some((6, 8)));
    }

    #[test]
    fn an_answer_with_no_array_is_no_issues() {
        assert!(issues_from_content("He go home.", "I found nothing.").is_empty());
    }
}
