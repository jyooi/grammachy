//! `grammachy chunk`, spec section 5.2.
//!
//! One Draft becomes a list of Chunks that tile the whole text: contiguous,
//! non-overlapping, and indexed in UTF-16 code units. Whole paragraphs pack
//! greedily up to the Check size limit, an oversize paragraph splits at
//! sentence ends, and an oversize sentence takes a hard cut at the limit.
//!
//! The limit is the selected Engine's, not one fixed number, so the Chunks a
//! Draft yields depend on the engine the Check will run on (spec section 4).

use serde::Serialize;

use crate::check::utf16_len;
use crate::envelope::{CheckError, ErrorBody, ErrorCode, CONTRACT_VERSION};

/// The size limit of one Draft, in UTF-16 code units (spec section 5.2).
pub const MAX_DRAFT_UTF16_UNITS: usize = 50_000;

/// One slice of the Draft, half open, in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Chunk {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChunkList {
    #[serde(rename = "contractVersion")]
    pub contract_version: u32,
    pub chunks: Vec<Chunk>,
}

/// Exactly one of these is printed on stdout by every `chunk` run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ChunkEnvelope {
    Chunks(ChunkList),
    Error(CheckError),
}

impl ChunkEnvelope {
    pub fn chunks(chunks: Vec<Chunk>) -> Self {
        ChunkEnvelope::Chunks(ChunkList {
            contract_version: CONTRACT_VERSION,
            chunks,
        })
    }

    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        ChunkEnvelope::Error(CheckError {
            contract_version: CONTRACT_VERSION,
            error: ErrorBody {
                code,
                message: message.into(),
            },
        })
    }

    /// Exit 0 for a Chunk list, exit 1 for an error envelope.
    pub fn exit_code(&self) -> i32 {
        match self {
            ChunkEnvelope::Chunks(_) => 0,
            ChunkEnvelope::Error(_) => 1,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialisation cannot fail")
    }
}

/// Validate the Draft and answer exactly one envelope.
///
/// `limit` is the Check size limit of the selected Engine, which
/// [`crate::args::EngineSlug::check_limit_utf16`] owns.
pub fn run(text: &str, limit: usize) -> ChunkEnvelope {
    if text.trim().is_empty() {
        return ChunkEnvelope::error(ErrorCode::EmptySelection, "The Draft is empty.");
    }

    let length = utf16_len(text);
    if length > MAX_DRAFT_UTF16_UNITS {
        return ChunkEnvelope::error(
            ErrorCode::TextTooLong,
            format!("The Draft is {length} units long, over the limit of {MAX_DRAFT_UTF16_UNITS}."),
        );
    }

    ChunkEnvelope::chunks(chunks_of(text, limit))
}

/// The Chunks of `text`, in UTF-16 code units, each at most `limit` long.
pub fn chunks_of(text: &str, limit: usize) -> Vec<Chunk> {
    let ranges = pack(text, units(text, limit), limit);

    let mut chunks = Vec::with_capacity(ranges.len());
    let mut start = 0;
    for (from, to) in ranges {
        let end = start + utf16_len(&text[from..to]);
        chunks.push(Chunk { start, end });
        start = end;
    }
    chunks
}

/// The atomic byte ranges the packer may not split further.
///
/// Every unit that fits stays whole. A paragraph over the limit becomes its
/// sentences, and a sentence over the limit becomes hard cuts at the limit.
fn units(text: &str, limit: usize) -> Vec<(usize, usize)> {
    let mut units = Vec::new();
    for paragraph in paragraphs(text) {
        if fits(text, paragraph, limit) {
            units.push(paragraph);
            continue;
        }
        for sentence in sentences(text, paragraph) {
            if fits(text, sentence, limit) {
                units.push(sentence);
            } else {
                units.extend(hard_cuts(text, sentence, limit));
            }
        }
    }
    units
}

/// Pack the units greedily, up to the limit per Chunk.
fn pack(text: &str, units: Vec<(usize, usize)>, limit: usize) -> Vec<(usize, usize)> {
    let mut packed: Vec<(usize, usize)> = Vec::new();
    let mut filled = 0;

    for unit in units {
        let length = utf16_len(&text[unit.0..unit.1]);
        match packed.last_mut() {
            Some(open) if filled + length <= limit => {
                open.1 = unit.1;
                filled += length;
            }
            _ => {
                packed.push(unit);
                filled = length;
            }
        }
    }
    packed
}

fn fits(text: &str, range: (usize, usize), limit: usize) -> bool {
    utf16_len(&text[range.0..range.1]) <= limit
}

/// The paragraphs of `text` as byte ranges, each carrying its own trailing
/// blank-line separator so that the ranges tile the whole text.
fn paragraphs(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut paragraphs = Vec::new();
    let mut start = 0;
    let mut at = 0;

    while at < bytes.len() {
        if bytes[at] != b'\n' {
            at += 1;
            continue;
        }
        match blank_line_break(bytes, at) {
            Some(end) => {
                paragraphs.push((start, end));
                start = end;
                at = end;
            }
            None => at += 1,
        }
    }

    if start < bytes.len() {
        paragraphs.push((start, bytes.len()));
    }
    paragraphs
}

/// The end of the paragraph break that starts at the newline `at`, or `None`
/// when that newline only ends a line. A break is one newline, then optional
/// blank space, then another newline; further blank lines join the same break.
fn blank_line_break(bytes: &[u8], at: usize) -> Option<usize> {
    let mut probe = at + 1;
    while probe < bytes.len() && matches!(bytes[probe], b' ' | b'\t' | b'\r') {
        probe += 1;
    }
    if probe >= bytes.len() || bytes[probe] != b'\n' {
        return None;
    }

    // Keep the indentation of the next paragraph with it, so the break ends
    // just after the last newline of the run.
    let mut end = probe + 1;
    let mut scan = end;
    while scan < bytes.len() {
        match bytes[scan] {
            b' ' | b'\t' | b'\r' => scan += 1,
            b'\n' => {
                scan += 1;
                end = scan;
            }
            _ => break,
        }
    }
    Some(end)
}

/// Split one paragraph into sentences at `.`, `?`, `!`, or a closing quote
/// after one of them, followed by whitespace. Each sentence keeps the
/// whitespace that follows it, so the ranges tile the paragraph.
fn sentences(text: &str, range: (usize, usize)) -> Vec<(usize, usize)> {
    let slice = &text[range.0..range.1];
    let chars: Vec<(usize, char)> = slice.char_indices().collect();

    let mut sentences = Vec::new();
    let mut cut = 0;
    let mut at = 0;

    while at < chars.len() {
        if !matches!(chars[at].1, '.' | '?' | '!') {
            at += 1;
            continue;
        }

        let mut probe = at + 1;
        while probe < chars.len() && is_closing_quote(chars[probe].1) {
            probe += 1;
        }
        if probe >= chars.len() || !chars[probe].1.is_whitespace() {
            at += 1;
            continue;
        }
        while probe < chars.len() && chars[probe].1.is_whitespace() {
            probe += 1;
        }

        let end = chars.get(probe).map_or(slice.len(), |(byte, _)| *byte);
        sentences.push((range.0 + cut, range.0 + end));
        cut = end;
        at = probe;
    }

    if cut < slice.len() {
        sentences.push((range.0 + cut, range.0 + slice.len()));
    }
    sentences
}

fn is_closing_quote(c: char) -> bool {
    matches!(
        c,
        '"' | '\'' | '\u{2019}' | '\u{201D}' | '\u{00BB}' | '\u{203A}'
    )
}

/// Cut one oversize sentence at the limit, never inside a character.
fn hard_cuts(text: &str, range: (usize, usize), limit: usize) -> Vec<(usize, usize)> {
    let mut cuts = Vec::new();
    let mut start = range.0;

    while !fits(text, (start, range.1), limit) {
        let mut units = 0;
        let mut end = start;
        for (offset, c) in text[start..range.1].char_indices() {
            if units + c.len_utf16() > limit {
                break;
            }
            units += c.len_utf16();
            end = start + offset + c.len_utf8();
        }
        cuts.push((start, end));
        start = end;
    }

    if start < range.1 {
        cuts.push((start, range.1));
    }
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::EngineSlug;

    /// The limit every engine packs to (spec section 4): Harper and
    /// LanguageTool share one Check size limit.
    const LIMIT: usize = EngineSlug::Languagetool.check_limit_utf16();

    fn text_of(text: &str, chunk: Chunk) -> String {
        let units: Vec<u16> = text.encode_utf16().collect();
        String::from_utf16(&units[chunk.start..chunk.end]).expect("the cut is on a character")
    }

    #[test]
    fn short_text_is_one_chunk() {
        let text = "One paragraph.\n\nAnother paragraph.";
        assert_eq!(
            chunks_of(text, LIMIT),
            vec![Chunk {
                start: 0,
                end: utf16_len(text)
            }]
        );
    }

    #[test]
    fn whole_paragraphs_pack_greedily_up_to_the_limit() {
        // Three paragraphs of 2,000 units each, separator included. Two fit,
        // the third opens a second Chunk.
        let paragraph = "a".repeat(1_998);
        let text = format!("{paragraph}\n\n{paragraph}\n\n{paragraph}");

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0],
            Chunk {
                start: 0,
                end: 4_000
            }
        );
        assert_eq!(chunks[1].end, utf16_len(&text));
        assert!(text_of(&text, chunks[1]).starts_with('a'));
    }

    #[test]
    fn a_paragraph_over_the_limit_splits_at_sentence_ends() {
        let sentence = format!("{}. ", "a".repeat(1_998));
        let text = sentence.repeat(4);

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0],
            Chunk {
                start: 0,
                end: 4_000
            }
        );
        assert_eq!(text_of(&text, chunks[0]), sentence.repeat(2));
    }

    #[test]
    fn a_sentence_end_may_carry_a_closing_quote() {
        let quoted = format!("\"{}?\" ", "a".repeat(1_996));
        let text = quoted.repeat(4);

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(chunks.len(), 2);
        assert_eq!(text_of(&text, chunks[0]), quoted.repeat(2));
        assert!(text_of(&text, chunks[1]).starts_with('"'));
    }

    #[test]
    fn a_period_inside_a_word_is_not_a_sentence_end() {
        let text = format!("{}.{} ", "a".repeat(3_000), "b".repeat(3_000));

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(
            chunks[0],
            Chunk {
                start: 0,
                end: 5_000
            }
        );
    }

    #[test]
    fn a_sentence_over_the_limit_is_a_hard_cut_at_the_limit() {
        let text = format!("{}. ", "a".repeat(11_000));

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(
            chunks,
            vec![
                Chunk {
                    start: 0,
                    end: 5_000
                },
                Chunk {
                    start: 5_000,
                    end: 10_000
                },
                Chunk {
                    start: 10_000,
                    end: utf16_len(&text)
                },
            ]
        );
    }

    #[test]
    fn a_hard_cut_never_splits_a_surrogate_pair() {
        // Every astral character is two units, so the limit lands mid pair
        // when one single-unit character leads the text.
        let text = format!("a{}", "\u{1F600}".repeat(4_000));

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(
            chunks[0],
            Chunk {
                start: 0,
                end: 4_999
            }
        );
        assert_eq!(text_of(&text, chunks[0]).chars().count(), 2_500);
    }

    #[test]
    fn crlf_blank_lines_separate_paragraphs() {
        let paragraph = "a".repeat(1_998);
        let text = format!("{paragraph}\r\n\r\n{paragraph}\r\n\r\n{paragraph}");

        let chunks = chunks_of(&text, LIMIT);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0],
            Chunk {
                start: 0,
                end: 4_004
            }
        );
    }

    #[test]
    fn empty_stdin_answers_empty_selection() {
        assert_eq!(
            run("", LIMIT).to_json(),
            r#"{"contractVersion":1,"error":{"code":"empty_selection","message":"The Draft is empty."}}"#
        );
    }

    #[test]
    fn a_draft_over_the_draft_limit_answers_text_too_long() {
        let over = "a".repeat(MAX_DRAFT_UTF16_UNITS + 1);
        assert!(matches!(run(&over, LIMIT), ChunkEnvelope::Error(_)));
        assert_eq!(run(&over, LIMIT).exit_code(), 1);

        let at_limit = "a".repeat(MAX_DRAFT_UTF16_UNITS);
        assert!(matches!(run(&at_limit, LIMIT), ChunkEnvelope::Chunks(_)));
        assert_eq!(run(&at_limit, LIMIT).exit_code(), 0);
    }

    /// Harper and LanguageTool are the only two engines left, and both read
    /// the same Check size limit (spec section 4), so `chunk --engine` packs
    /// every Draft the same way regardless of which one it names.
    #[test]
    fn every_remaining_engine_packs_to_the_same_limit() {
        assert_eq!(
            EngineSlug::Harper.check_limit_utf16(),
            EngineSlug::Languagetool.check_limit_utf16()
        );
    }
}
