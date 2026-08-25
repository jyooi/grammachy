//! The mapping from `harper-core` lints to the Issues of spec section 5.1.
//!
//! [`harper_core::linting::Lint`] is `Deserialize`, so this module maps a
//! recorded lint list as faithfully as a live one and the mapping tests need no
//! dictionary and no linter.

use harper_core::linting::{Lint, LintKind, Suggestion};
use harper_core::Span;

use crate::envelope::{Category, Issue};
use crate::issues::normalise;
use crate::text::utf16_offsets;

/// Lint kinds that report style rather than a mistake.
///
/// Depth in v1 is grammar and spelling only and the user's voice is kept, so
/// these are reported to nobody (spec section 1). The list mirrors the style
/// filter of the LanguageTool adapter.
const STYLE_KINDS: [LintKind; 4] = [
    LintKind::Enhancement,
    LintKind::Readability,
    LintKind::Redundancy,
    LintKind::Style,
];

/// The `category` of spec section 5.1.
///
/// Harper splits a misspelling from a typo by what the writer knew, which the
/// contract does not, so both are spelling. Everything the adapter still
/// reports after the style filter is grammar.
fn category_of(kind: LintKind) -> Option<Category> {
    if STYLE_KINDS.contains(&kind) {
        return None;
    }
    match kind {
        LintKind::Spelling | LintKind::Typo => Some(Category::Spelling),
        _ => Some(Category::Grammar),
    }
}

/// The replacement text of the first suggestion, given the text it replaces.
///
/// Harper says what to do rather than what the span becomes, so a removal is
/// the empty replacement and an insertion keeps the original in front of the
/// added characters.
fn fix_of(suggestion: &Suggestion, original: &str) -> String {
    match suggestion {
        Suggestion::ReplaceWith(characters) => characters.iter().collect(),
        Suggestion::InsertAfter(characters) => {
            let mut fix = original.to_string();
            fix.extend(characters.iter());
            fix
        }
        Suggestion::Remove => String::new(),
    }
}

/// One English sentence with no rule id (spec section 5.1).
///
/// Harper messages are already written as prose. The whitespace is collapsed to
/// one space so a wrapped message stays one line in the inspector.
fn reason_of(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The half-open UTF-16 span of a lint, whose own span counts `char`s.
fn utf16_span(offsets: &[usize], span: Span<char>) -> Option<(usize, usize)> {
    if span.start > span.end {
        return None;
    }
    Some((*offsets.get(span.start)?, *offsets.get(span.end)?))
}

/// Turn one lint into an Issue, or drop it.
///
/// A lint is dropped when it is style, when its span does not address the
/// checked text, or when it carries no suggestion. A suggestion that changes
/// nothing is dropped by [`normalise`].
fn issue_of(offsets: &[usize], characters: &[char], lint: &Lint) -> Option<Issue> {
    let category = category_of(lint.lint_kind)?;
    let (start, end) = utf16_span(offsets, lint.span)?;
    let original: String = characters
        .get(lint.span.start..lint.span.end)?
        .iter()
        .collect();

    let fix = fix_of(lint.suggestions.first()?, &original);

    Some(Issue {
        start,
        end,
        original,
        fix,
        reason: reason_of(&lint.message),
        category,
        // Harper names no rule on a lint, so the kind is the closest thing a
        // bug report can carry.
        rule_id: Some(lint.lint_kind.to_string_key()),
    })
}

/// Map a whole lint list to the Issue list the envelope carries.
///
/// [`normalise`] keeps every guarantee of spec section 5.1: sorted by `start`,
/// never overlapping because the earlier Issue wins, and never carrying a `fix`
/// equal to the `original`.
pub fn issues_from(text: &str, lints: &[Lint]) -> Vec<Issue> {
    let offsets = utf16_offsets(text);
    let characters: Vec<char> = text.chars().collect();

    normalise(
        lints
            .iter()
            .filter_map(|lint| issue_of(&offsets, &characters, lint))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_lint_is_dropped_and_a_typo_is_spelling() {
        assert_eq!(category_of(LintKind::Style), None);
        assert_eq!(category_of(LintKind::Enhancement), None);
        assert_eq!(category_of(LintKind::Spelling), Some(Category::Spelling));
        assert_eq!(category_of(LintKind::Typo), Some(Category::Spelling));
        assert_eq!(category_of(LintKind::Agreement), Some(Category::Grammar));
        assert_eq!(category_of(LintKind::WordOrder), Some(Category::Grammar));
    }

    #[test]
    fn every_suggestion_becomes_the_text_the_span_turns_into() {
        let replace = Suggestion::ReplaceWith("goes".chars().collect());
        let insert = Suggestion::InsertAfter("s".chars().collect());

        assert_eq!(fix_of(&replace, "go"), "goes");
        assert_eq!(fix_of(&insert, "book"), "books");
        assert_eq!(fix_of(&Suggestion::Remove, "but"), "");
    }

    #[test]
    fn a_message_keeps_its_prose_on_one_line() {
        assert_eq!(
            reason_of("Did you mean\n  the definite  article?"),
            "Did you mean the definite article?"
        );
    }

    #[test]
    fn a_span_past_the_end_of_the_text_is_dropped() {
        let text = "He go home.";
        let lint = Lint {
            span: Span::new(4, 400),
            lint_kind: LintKind::Agreement,
            suggestions: vec![Suggestion::ReplaceWith("goes".chars().collect())],
            message: "Out of range.".to_string(),
            priority: 127,
        };

        assert!(issues_from(text, &[lint]).is_empty());
    }
}
