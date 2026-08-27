//! Turning one M2 block into one eval-set item, spec `evals.md` section 2.
//!
//! The M2 file is token offsets over tokenised text, and the item shape of
//! HUF-205 is UTF-16 offsets over the sentence a writer would actually type.
//! So the tokens are joined back into a sentence here, and the character span
//! of every token is recorded while that join happens. An offset can therefore
//! never drift from the text it points into: both come out of the same walk.
//!
//! The conversion rules are the spec's:
//!
//! - A block is kept when it is one sentence and carries exactly one edit,
//!   and that edit is neither a spelling nor a punctuation edit. Dropping the
//!   edit but keeping the sentence would leave a mistake in the text that no
//!   metric expects, which reads as a false positive against every engine that
//!   finds it.
//! - A zero-width missing-word edit is widened to the next word, because the
//!   contract of spec section 5.1 has no zero-width span: an Issue quotes the
//!   text it replaces.
//! - The ERRANT code becomes the item's `type`.
//!
//! FCE has no astral characters, so a character offset is a UTF-16 offset.
//! Nothing here trusts that: the walk counts UTF-16 code units.

use super::corpus::{Block, Edit};

/// The languages the eval set draws from, spec `evals.md` section 2.
///
/// `ms` is absent from FCE and stays on the real-user route of the fixture.
pub const LANGUAGES: [&str; 6] = ["zh", "es", "fr", "de", "pt", "ja"];

/// How long an error sentence may be, in tokens.
const ERROR_TOKENS: std::ops::RangeInclusive<usize> = 6..=32;

/// How long an error-free control may be, in tokens.
const CONTROL_TOKENS: std::ops::RangeInclusive<usize> = 8..=30;

/// Tokens that end a sentence, so a block with one before its last token is
/// more than one sentence and is not drawn.
const SENTENCE_END: [&str; 4] = [".", "!", "?", "..."];

/// Tokens that join to the word on their left, so no space is written first.
const ATTACH_LEFT: [&str; 12] = [".", ",", "!", "?", ";", ":", ")", "]", "}", "%", "'", "..."];

/// Tokens that join to the word on their right, so no space follows them.
const ATTACH_RIGHT: [&str; 6] = ["(", "[", "{", "$", "\u{a3}", "\u{20ac}"];

/// Whether an ERRANT code names a mistake this eval set measures.
///
/// Spelling and punctuation are out by the spec. `UNK` is an edit the
/// annotator could not classify, so it says nothing about grammar.
pub fn is_measurable(code: &str) -> bool {
    code != "UNK" && code != "R:SPELL" && code != "R:ORTH" && !code.contains("PUNCT")
}

/// Whether a block is one sentence rather than several.
fn is_single_sentence(tokens: &[String]) -> bool {
    !tokens
        .iter()
        .rev()
        .skip(1)
        .any(|token| SENTENCE_END.contains(&token.as_str()))
}

/// One eval-set item, before it is given its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub document: usize,
    pub sentence: usize,
    pub native: String,
    pub text: String,
    pub edits: Vec<ItemEdit>,
    pub expected_text: String,
}

impl Item {
    /// Whether the item carries a mistake to catch.
    pub fn is_interference(&self) -> bool {
        !self.edits.is_empty()
    }
}

/// One expected mistake of an item, in UTF-16 offsets into `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemEdit {
    pub start: usize,
    pub end: usize,
    /// The text the span quotes.
    pub text: String,
    /// The replacement.
    pub fix: String,
    /// The ERRANT code, the item's `type`.
    pub code: String,
}

/// The item one block yields, or `None` when the rules do not keep it.
pub fn item(block: &Block) -> Option<Item> {
    if !LANGUAGES.contains(&block.native.as_str()) {
        return None;
    }
    if block.tokens.iter().any(String::is_empty) || !is_single_sentence(&block.tokens) {
        return None;
    }

    let count = block.tokens.len();
    match block.edits.len() {
        0 if CONTROL_TOKENS.contains(&count) => Some(build(block, None)),
        1 if ERROR_TOKENS.contains(&count) && is_measurable(&block.edits[0].code) => {
            build_error(block, &block.edits[0])
        }
        _ => None,
    }?
    .into()
}

/// The item of a block whose one edit was kept, or `None` when the edit does
/// not point into the block.
fn build_error(block: &Block, edit: &Edit) -> Option<Item> {
    if edit.end > block.tokens.len() || edit.start > edit.end {
        return None;
    }
    if edit.is_insertion() && block.tokens.is_empty() {
        return None;
    }
    if !edit.is_insertion() && edit.correction.trim().is_empty() && edit.end - edit.start > 3 {
        // A long deletion leaves nothing to quote that reads as one mistake.
        return None;
    }
    Some(build(block, Some(edit)))
}

/// Join the tokens back into a sentence and place the edit inside it.
fn build(block: &Block, edit: Option<&Edit>) -> Item {
    let (text, spans) = detokenise(&block.tokens);
    let edits = edit
        .map(|edit| vec![place(&text, &spans, &block.tokens, edit)])
        .unwrap_or_default();
    let expected_text = apply(&text, &edits);

    Item {
        document: block.document,
        sentence: block.sentence,
        native: block.native.clone(),
        text,
        edits,
        expected_text,
    }
}

/// The UTF-16 span of one edit and the replacement it carries.
///
/// A missing word is a zero-width edit in M2. It is widened to the next word,
/// or to the last word when it belongs after the end, so the span always
/// quotes text the way spec section 5.1 requires.
fn place(text: &str, spans: &[(usize, usize)], tokens: &[String], edit: &Edit) -> ItemEdit {
    let (start, end, fix) = if !edit.is_insertion() {
        (
            spans[edit.start].0,
            spans[edit.end - 1].1,
            edit.correction.clone(),
        )
    } else if edit.start < tokens.len() {
        let span = spans[edit.start];
        (
            span.0,
            span.1,
            format!("{} {}", edit.correction, tokens[edit.start]),
        )
    } else {
        let span = spans[tokens.len() - 1];
        (
            span.0,
            span.1,
            format!("{} {}", tokens[tokens.len() - 1], edit.correction),
        )
    };

    let fix = fix.trim().to_string();
    // A deletion takes the space in front of the word with it, the way a
    // Fix that removes a word has to, or the Corrected text keeps a gap.
    let start = match fix.is_empty() && start > 0 {
        true if crate::text::utf16_slice(text, start - 1, start) == Some(" ") => start - 1,
        _ => start,
    };

    ItemEdit {
        start,
        end,
        text: crate::text::utf16_slice(text, start, end)
            .unwrap_or_default()
            .to_string(),
        fix,
        code: edit.code.clone(),
    }
}

/// The text with every edit applied, the Corrected text a perfect engine gives.
fn apply(text: &str, edits: &[ItemEdit]) -> String {
    let mut out = text.to_string();
    for edit in edits.iter().rev() {
        let Some(start) = crate::text::byte_index_of_utf16(&out, edit.start) else {
            continue;
        };
        let Some(end) = crate::text::byte_index_of_utf16(&out, edit.end) else {
            continue;
        };
        out.replace_range(start..end, &edit.fix);
    }
    out
}

/// Join tokens into a sentence and record each token's UTF-16 span in it.
///
/// The separator is decided per token, so the spans are exact by construction
/// rather than by a second pass that could disagree with the join.
pub fn detokenise(tokens: &[String]) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut spans = Vec::with_capacity(tokens.len());
    let mut units = 0;
    let mut quotes = 0;

    for (index, token) in tokens.iter().enumerate() {
        let opening_quote = token == "\"" && quotes % 2 == 0;
        if token == "\"" {
            quotes += 1;
        }
        let previous = index.checked_sub(1).map(|before| tokens[before].as_str());
        if index > 0 && separated(previous, token, opening_quote, tokens, index) {
            text.push(' ');
            units += 1;
        }
        let start = units;
        text.push_str(token);
        units += crate::text::utf16_len(token);
        spans.push((start, units));
    }

    (text, spans)
}

/// Whether a space is written before this token.
fn separated(
    previous: Option<&str>,
    token: &str,
    opening_quote: bool,
    tokens: &[String],
    index: usize,
) -> bool {
    if ATTACH_LEFT.contains(&token) || token.starts_with('\'') || token == "n't" {
        return false;
    }
    if token == "\"" && !opening_quote {
        return false;
    }
    match previous {
        Some(previous) => {
            if ATTACH_RIGHT.contains(&previous) {
                return false;
            }
            // The quote that opened stays against the word it opened.
            !(previous == "\"" && opened_before(tokens, index - 1))
        }
        None => true,
    }
}

/// Whether the quote token at this place opened rather than closed.
fn opened_before(tokens: &[String], place: usize) -> bool {
    tokens[..place]
        .iter()
        .filter(|token| *token == "\"")
        .count()
        % 2
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(native: &str, sentence: &str, edits: Vec<Edit>) -> Block {
        Block {
            document: 7,
            sentence: 2,
            native: native.to_string(),
            tokens: sentence.split(' ').map(str::to_string).collect(),
            edits,
        }
    }

    fn edit(start: usize, end: usize, code: &str, correction: &str) -> Edit {
        Edit {
            start,
            end,
            code: code.to_string(),
            correction: correction.to_string(),
        }
    }

    #[test]
    fn the_tokens_join_back_into_a_sentence_a_writer_would_type() {
        let tokens: Vec<String> = "I do n't like the show 's ending , \" Over the Rainbow \" ."
            .split(' ')
            .map(str::to_string)
            .collect();

        let (text, spans) = detokenise(&tokens);

        assert_eq!(
            text,
            "I don't like the show's ending, \"Over the Rainbow\"."
        );
        assert_eq!(spans.len(), tokens.len());
        for (token, (start, end)) in tokens.iter().zip(&spans) {
            assert_eq!(
                crate::text::utf16_slice(&text, *start, *end),
                Some(token.as_str())
            );
        }
    }

    #[test]
    fn a_replacement_edit_becomes_a_span_that_quotes_its_own_text() {
        let item = item(&block(
            "zh",
            "I saws the show on the wall .",
            vec![edit(1, 2, "R:VERB:TENSE", "saw")],
        ))
        .expect("the block is kept");

        assert_eq!(item.text, "I saws the show on the wall.");
        assert_eq!(item.edits[0].text, "saws");
        assert_eq!(item.edits[0].fix, "saw");
        assert_eq!(item.edits[0].code, "R:VERB:TENSE");
        assert_eq!(item.expected_text, "I saw the show on the wall.");
        assert_eq!(
            crate::text::utf16_slice(&item.text, item.edits[0].start, item.edits[0].end),
            Some("saws")
        );
    }

    #[test]
    fn a_missing_word_widens_onto_the_word_that_follows_it() {
        let item = item(&block(
            "es",
            "I went to cinema with my friends .",
            vec![edit(3, 3, "M:DET", "the")],
        ))
        .expect("the block is kept");

        assert_eq!(item.edits[0].text, "cinema");
        assert_eq!(item.edits[0].fix, "the cinema");
        assert_eq!(item.expected_text, "I went to the cinema with my friends.");
        assert!(
            item.edits[0].start < item.edits[0].end,
            "no zero-width span"
        );
    }

    #[test]
    fn a_deletion_edit_removes_the_span_it_quotes() {
        let item = item(&block(
            "fr",
            "If you do not agree , I will act consequently .",
            vec![edit(9, 10, "U:ADV", "")],
        ))
        .expect("the block is kept");

        assert_eq!(
            item.edits[0].text, " consequently",
            "a deletion takes its own space with it"
        );
        assert_eq!(item.edits[0].fix, "");
        assert_eq!(item.expected_text, "If you do not agree, I will act.");
    }

    #[test]
    fn an_error_free_block_expects_itself() {
        let item = item(&block(
            "ja",
            "I look forward to hearing from you soon .",
            Vec::new(),
        ))
        .expect("the block is kept");

        assert!(!item.is_interference());
        assert_eq!(item.text, item.expected_text);
    }

    #[test]
    fn the_rules_drop_what_the_eval_set_does_not_measure() {
        let long = "I go to the school every single day of the week .";

        assert!(
            item(&block("ko", long, vec![edit(1, 2, "R:VERB", "went")])).is_none(),
            "a language outside the six"
        );
        assert!(
            item(&block("zh", long, vec![edit(1, 2, "R:SPELL", "went")])).is_none(),
            "a spelling edit"
        );
        assert!(
            item(&block("zh", long, vec![edit(1, 2, "M:PUNCT", ",")])).is_none(),
            "a punctuation edit"
        );
        assert!(
            item(&block("zh", long, vec![edit(1, 2, "UNK", "went")])).is_none(),
            "an unclassified edit"
        );
        assert!(
            item(&block(
                "zh",
                long,
                vec![
                    edit(1, 2, "R:VERB", "went"),
                    edit(4, 5, "R:NOUN", "schools")
                ]
            ))
            .is_none(),
            "two edits in one sentence"
        );
        assert!(
            item(&block(
                "zh",
                "I go there . She stays here every day of the week .",
                vec![edit(1, 2, "R:VERB", "went")]
            ))
            .is_none(),
            "more than one sentence"
        );
        assert!(
            item(&block("zh", "I go .", vec![edit(1, 2, "R:VERB", "went")])).is_none(),
            "too short to read as a sentence"
        );
    }
}
