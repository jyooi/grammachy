//! The chat completion answer, and the mapping from its suggestions to the
//! Issues of spec section 5.1.
//!
//! A model quotes text; it does not count code units. So every suggestion is
//! placed by looking its `original` up in the text the CLI sent, and a
//! suggestion that quotes something the text does not hold is dropped. HUF-181
//! measured why that matters: small models emit both no-op suggestions, where
//! the fix equals the original, and unanchored ones. Neither is an Issue.

use serde::Deserialize;
use serde_json::{Map, Value};

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
    /// Optional start, offset, or position the model emitted. Placement only.
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Suggestion {
    /// A claimed UTF-16 start, when the model gave one.
    fn position_hint(&self) -> Option<usize> {
        for key in ["start", "offset", "position"] {
            let Some(value) = self.extra.get(key) else {
                continue;
            };
            let parsed = value
                .as_u64()
                .map(|number| number as usize)
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()));
            if parsed.is_some() {
                return parsed;
            }
        }
        None
    }
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
        let used: Vec<(usize, usize)> = placed
            .iter()
            .filter(|issue| issue.original == suggestion.original)
            .map(|issue| (issue.start, issue.end))
            .collect();
        if let Some(issue) = issue_of(text, suggestion, &used) {
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
/// `used` is the spans already claimed by earlier Issues that quoted the same
/// text, so a repeated quotation lands on the next unused occurrence.
fn issue_of(text: &str, suggestion: &Suggestion, used: &[(usize, usize)]) -> Option<Issue> {
    let fix = suggestion.fix.as_ref()?;
    if suggestion.original.is_empty() {
        return None;
    }
    // A fix that changes nothing is noise, and spec section 5.1 forbids it.
    if *fix == suggestion.original {
        return None;
    }

    let (start, end) = place(text, &suggestion.original, used, suggestion.position_hint())?;
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

/// The UTF-16 span of one whole-token occurrence of `original`.
///
/// A match must not sit inside another word. `used` holds spans already claimed,
/// so a repeated quotation lands on the next unused occurrence. When the model
/// gives a position hint, the unused match nearest that hint wins. The function
/// returns none when no whole-token match exists.
fn place(
    text: &str,
    original: &str,
    used: &[(usize, usize)],
    hint: Option<usize>,
) -> Option<(usize, usize)> {
    let mut chosen: Option<(usize, usize)> = None;
    let mut chosen_distance = usize::MAX;

    for (byte_index, _) in text.match_indices(original) {
        let end_byte = byte_index + original.len();
        if !is_whole_token(text, byte_index, end_byte) {
            continue;
        }
        let start = utf16_len(&text[..byte_index]);
        let end = start + utf16_len(original);
        if used.iter().any(|span| *span == (start, end)) {
            continue;
        }
        match hint {
            None => return Some((start, end)),
            Some(hint) => {
                let distance = start.abs_diff(hint);
                if chosen.is_none() || distance < chosen_distance {
                    chosen = Some((start, end));
                    chosen_distance = distance;
                }
            }
        }
    }
    chosen
}

/// True when this match does not start or end inside another word.
fn is_whole_token(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text.get(end..).and_then(|rest| rest.chars().next());
    let first = text.get(start..end).and_then(|span| span.chars().next());
    let last = text
        .get(start..end)
        .and_then(|span| span.chars().next_back());
    let glued_left =
        first.is_some_and(char::is_alphanumeric) && before.is_some_and(char::is_alphanumeric);
    let glued_right =
        last.is_some_and(char::is_alphanumeric) && after.is_some_and(char::is_alphanumeric);
    !glued_left && !glued_right
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

        assert_eq!(place(text, "book", &[], None), Some((11, 15)));
        assert_eq!(place(text, "book", &[(11, 15)], None), Some((30, 34)));
        assert_eq!(place(text, "book", &[(11, 15), (30, 34)], None), None);
    }

    #[test]
    fn a_span_after_an_astral_character_counts_utf16_units() {
        let text = "\u{1F600} He go home.";

        // The emoji is two UTF-16 code units, so "go" starts at 6, not at 5.
        assert_eq!(place(text, "go", &[], None), Some((6, 8)));
    }

    #[test]
    fn an_article_does_not_land_inside_another_word() {
        assert_eq!(place("Is raining a lot.", "a", &[], None), Some((11, 12)));
        assert_eq!(place("Is raining today.", "a", &[], None), None);
    }

    #[test]
    fn a_position_hint_picks_the_nearer_unused_token() {
        let text = "She read a book and he read a book.";

        assert_eq!(place(text, "book", &[], Some(30)), Some((30, 34)));
        assert_eq!(place(text, "book", &[(30, 34)], Some(30)), Some((11, 15)));
    }

    #[test]
    fn an_answer_with_no_array_is_no_issues() {
        assert!(issues_from_content("He go home.", "I found nothing.").is_empty());
    }
}
