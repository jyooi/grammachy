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
//!   text it replaces. Punctuation is not a word, so an edit in front of a
//!   stop or a comma widens onto the word before it instead. A span quoting a
//!   stop alone is one no Issue overlaps, because an engine writes its Issue
//!   over the word, which sits beside that span rather than inside it, and
//!   every metric of section 4 asks for an overlap.
//! - A word added after the last token of a sentence that ends in a stop is
//!   not kept. The corrected sentence would read as a fragment behind its own
//!   full stop, so no engine could write it and the item would measure
//!   nothing.
//! - The ERRANT code becomes the item's `type`.
//!
//! The Corrected text is the same walk over the corrected token stream, never
//! a second rule that splices strings together. The edit is then the one span
//! that turns the sentence into that Corrected text, which is how the span,
//! the fix, and `expected_text` can never disagree. A correction no single
//! span can write is a block this module does not keep.
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
        0 if CONTROL_TOKENS.contains(&count) => Some(control(block)),
        1 if ERROR_TOKENS.contains(&count) && is_measurable(&block.edits[0].code) => {
            build_error(block, &block.edits[0])
        }
        _ => None,
    }
}

/// The item of an error-free block, which expects itself.
fn control(block: &Block) -> Item {
    let text = detokenise(&block.tokens).0;
    Item {
        document: block.document,
        sentence: block.sentence,
        native: block.native.clone(),
        expected_text: text.clone(),
        text,
        edits: Vec::new(),
    }
}

/// The item of a block whose one edit was kept, or `None` when the edit does
/// not point into the block or does not read as one mistake.
fn build_error(block: &Block, edit: &Edit) -> Option<Item> {
    if edit.end > block.tokens.len() || edit.start > edit.end {
        return None;
    }
    if edit.is_insertion() && block.tokens.is_empty() {
        return None;
    }
    if edit.is_insertion() && edit.start == block.tokens.len() && ends_the_sentence(&block.tokens) {
        // A word added behind the full stop reads as a fragment rather than
        // as a sentence, so no engine could write the Corrected text.
        return None;
    }
    if !edit.is_insertion() && edit.correction.trim().is_empty() && edit.end - edit.start > 3 {
        // A long deletion leaves nothing to quote that reads as one mistake.
        return None;
    }
    build(block, edit)
}

/// Whether the last token of a block is the stop that ends its sentence.
fn ends_the_sentence(tokens: &[String]) -> bool {
    tokens
        .last()
        .is_some_and(|token| SENTENCE_END.contains(&token.as_str()))
}

/// Join the tokens back into a sentence and place the edit inside it.
///
/// Both sentences come out of [`detokenise`], the tokens as the writer typed
/// them and the tokens the annotator corrected, so the Corrected text is never
/// spliced together from a second separator rule.
fn build(block: &Block, edit: &Edit) -> Option<Item> {
    let (text, spans) = detokenise(&block.tokens);
    let expected_text = detokenise(&corrected(&block.tokens, edit)).0;
    let placed = place(&text, &spans, &block.tokens, edit, &expected_text)?;

    Some(Item {
        document: block.document,
        sentence: block.sentence,
        native: block.native.clone(),
        text,
        edits: vec![placed],
        expected_text,
    })
}

/// The token stream the annotator's correction leaves behind.
fn corrected(tokens: &[String], edit: &Edit) -> Vec<String> {
    let mut out = tokens[..edit.start].to_vec();
    out.extend(edit.correction.split_whitespace().map(str::to_string));
    out.extend_from_slice(&tokens[edit.end..]);
    out
}

/// The UTF-16 span of one edit and the replacement it carries, or `None` when
/// no single span writes the Corrected text.
///
/// The span quotes the words the edit is about: the words it replaces, or the
/// word a missing word is widened onto, because the contract of spec section
/// 5.1 has no zero-width span. [`fit`] then takes the separator on either side
/// of those words when the correction moved it.
fn place(
    text: &str,
    spans: &[(usize, usize)],
    tokens: &[String],
    edit: &Edit,
    expected_text: &str,
) -> Option<ItemEdit> {
    let (start, end) = match edit.is_insertion() {
        false => (spans[edit.start].0, spans[edit.end - 1].1),
        true => widened(spans, tokens, edit.start)?,
    };
    let (start, end, fix) = fit(text, expected_text, start, end)?;

    Some(ItemEdit {
        start,
        end,
        text: crate::text::utf16_slice(text, start, end)
            .unwrap_or_default()
            .to_string(),
        fix,
        code: edit.code.clone(),
    })
}

/// The span a missing word is widened onto, or `None` when the sentence holds
/// no word to widen onto.
///
/// The next word is the answer wherever there is one, which is the rule of
/// spec section 5.1. A missing word in front of punctuation, or one that
/// belongs after the last token, widens back onto the word before it instead,
/// and takes the punctuation between them, so the span always quotes a word an
/// Issue can overlap.
fn widened(spans: &[(usize, usize)], tokens: &[String], at: usize) -> Option<(usize, usize)> {
    if tokens.get(at).is_some_and(|token| is_word(token)) {
        return Some(spans[at]);
    }
    let word = tokens[..at].iter().rposition(|token| is_word(token))?;
    Some((spans[word].0, spans[at - 1].1))
}

/// Whether a token is a word rather than punctuation.
fn is_word(token: &str) -> bool {
    token.chars().any(char::is_alphanumeric)
}

/// The span and replacement that turn `text` into `expected_text`, from a span
/// that quotes the words the edit is about.
///
/// The correction may need a separator the sentence did not write, or leave
/// one the sentence did: a possessive joins onto the word in front of it, and
/// a deleted word takes the space in front of it with it. So the span may take
/// one unit more on either side, and the first pair whose untouched text is
/// the untouched text of the Corrected sentence is the answer. `None` says no
/// one replacement writes this correction, which is a block that is not kept.
///
/// The pairs are tried tightest first, and the space in front of the words
/// before the space behind them, which is the rule of spec `evals.md` section
/// 2 for a deletion.
fn fit(
    text: &str,
    expected_text: &str,
    start: usize,
    end: usize,
) -> Option<(usize, usize, String)> {
    let units = crate::text::utf16_len(text);
    let expected_units = crate::text::utf16_len(expected_text);
    let before = start.saturating_sub(1);

    for (start, end) in [
        (start, end),
        (before, end),
        (start, end + 1),
        (before, end + 1),
    ] {
        if end > units || end < start {
            continue;
        }
        let tail = units - end;
        if expected_units < start + tail {
            continue;
        }
        if crate::text::utf16_slice(text, 0, start)
            != crate::text::utf16_slice(expected_text, 0, start)
        {
            continue;
        }
        if crate::text::utf16_slice(text, end, units)
            != crate::text::utf16_slice(expected_text, expected_units - tail, expected_units)
        {
            continue;
        }
        let Some(fix) = crate::text::utf16_slice(expected_text, start, expected_units - tail)
        else {
            continue;
        };
        return Some((start, end, fix.to_string()));
    }

    None
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

    /// The sentence a writer gets after accepting the item's own edit.
    fn applied(item: &Item) -> String {
        let mut out = item.text.clone();
        for edit in item.edits.iter().rev() {
            let start = crate::text::byte_index_of_utf16(&out, edit.start).expect("a real index");
            let end = crate::text::byte_index_of_utf16(&out, edit.end).expect("a real index");
            out.replace_range(start..end, &edit.fix);
        }
        out
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

    /// A widened insertion joins by the rule that wrote the sentence.
    ///
    /// The next token may attach to the word in front of it, such as `.` or
    /// `n't`, and a separator written without that rule lands inside that word.
    #[test]
    fn a_missing_word_before_an_attached_token_joins_the_way_the_sentence_does() {
        let cases = [
            (
                "I go to the cinema every single day .",
                8,
                "M:ADV",
                "quickly",
                "I go to the cinema every single day quickly.",
            ),
            (
                "In my opinion , the film was very good .",
                3,
                "M:ADV",
                "however",
                "In my opinion however, the film was very good.",
            ),
            (
                "I like the show 's ending very much .",
                4,
                "M:NOUN",
                "friend",
                "I like the show friend's ending very much.",
            ),
            (
                "I do n't like the show at all .",
                2,
                "M:ADV",
                "really",
                "I do reallyn't like the show at all.",
            ),
        ];

        for (sentence, index, code, correction, expected) in cases {
            let item = item(&block(
                "zh",
                sentence,
                vec![edit(index, index, code, correction)],
            ))
            .expect("the block is kept");

            assert_eq!(item.expected_text, expected);
            assert!(
                item.edits[0].start < item.edits[0].end,
                "no zero-width span"
            );

            let mut stream: Vec<String> = sentence.split(' ').map(str::to_string).collect();
            stream.insert(index, correction.to_string());
            assert_eq!(
                item.expected_text,
                detokenise(&stream).0,
                "the fix reads as the same words read inside a sentence"
            );
            assert_eq!(
                applied(&item),
                item.expected_text,
                "the one span writes the Corrected text"
            );
        }
    }

    /// A correction is tokenised too, so it is joined rather than pasted in.
    #[test]
    fn a_tokenised_correction_is_joined_back_into_words() {
        let item = item(&block(
            "de",
            "I did not liked the film at all .",
            vec![edit(1, 3, "R:VERB:TENSE", "did n't")],
        ))
        .expect("the block is kept");

        assert_eq!(item.edits[0].fix, "didn't");
        assert_eq!(item.expected_text, "I didn't liked the film at all.");
    }

    /// A word added behind the full stop is not a sentence a writer types.
    #[test]
    fn a_word_added_behind_the_final_stop_is_not_kept() {
        assert!(
            item(&block(
                "pt",
                "Our neighbour repaired the broken fence last Tuesday .",
                vec![edit(9, 9, "M:ADV", "quickly")],
            ))
            .is_none(),
            "the corrected sentence would read as a fragment"
        );
    }

    /// A block that carries no final stop still takes a word at its end.
    #[test]
    fn a_missing_word_at_the_end_widens_onto_the_word_before_it() {
        let item = item(&block(
            "pt",
            "Our neighbour repaired the broken fence last Tuesday",
            vec![edit(8, 8, "M:ADV", "quickly")],
        ))
        .expect("the block is kept");

        assert_eq!(item.edits[0].text, "Tuesday");
        assert_eq!(item.edits[0].fix, "Tuesday quickly");
        assert_eq!(
            item.expected_text,
            "Our neighbour repaired the broken fence last Tuesday quickly"
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

    /// A missing word in front of punctuation quotes the word before it.
    ///
    /// A span on the stop alone is a span no Issue overlaps: an engine writes
    /// its Issue over the word beside it, and every metric of the evals spec
    /// asks for an overlap, so the item would score every model a miss.
    #[test]
    fn a_missing_word_before_punctuation_quotes_a_word_rather_than_the_mark() {
        let cases = [
            (
                "I saw him at the station this morning .",
                8,
                "morning",
                "morning again",
                "I saw him at the station this morning again.",
            ),
            (
                "In my opinion , the film was very good .",
                3,
                "opinion",
                "opinion however",
                "In my opinion however, the film was very good.",
            ),
        ];

        for (sentence, index, quoted, fix, expected) in cases {
            let correction = fix.split(' ').next_back().expect("a correction");
            let item = item(&block(
                "de",
                sentence,
                vec![edit(index, index, "M:ADV", correction)],
            ))
            .expect("the block is kept");

            assert_eq!(item.edits[0].text, quoted);
            assert_eq!(item.edits[0].fix, fix);
            assert_eq!(item.expected_text, expected);
            assert!(
                item.edits[0].text.chars().any(char::is_alphanumeric),
                "the span quotes a word"
            );
            assert_eq!(applied(&item), item.expected_text);
        }
    }

    /// A correction that joins onto the word in front of it takes the space
    /// the sentence wrote between them.
    #[test]
    fn a_correction_that_attaches_leftward_takes_the_space_with_it() {
        let cases = [
            (
                "My brother car is red .",
                2,
                2,
                "M:NOUN:POSS",
                "'s",
                "My brother's car is red.",
            ),
            (
                "I do not like it .",
                2,
                3,
                "R:CONTR",
                "n't",
                "I don't like it.",
            ),
        ];

        for (sentence, start, end, code, correction, expected) in cases {
            let block = block("zh", sentence, vec![edit(start, end, code, correction)]);
            let item = item(&block).expect("the block is kept");

            assert_eq!(item.expected_text, expected);
            assert_eq!(
                item.expected_text,
                detokenise(&corrected(&block.tokens, &block.edits[0])).0,
                "the Corrected text is the sentence the corrected tokens write"
            );
            assert_eq!(
                applied(&item),
                item.expected_text,
                "the one span writes the Corrected text"
            );
        }
    }

    #[test]
    fn an_error_free_block_expects_itself() {
        let item = item(&block(
            "ja",
            "The rain stopped before we reached the harbour wall .",
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
