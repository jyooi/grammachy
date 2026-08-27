//! The interference fixture the benchmark runs, `docs/spec/evals.md` section 1.
//!
//! The file is `tests/fixtures/interference-30.json`, the test set of HUF-171
//! in the item shape HUF-205 settled: every item carries the edits that turn
//! its text into `expected_text`, and a correct sentence carries no edit. It is
//! compiled into the binary, so a released `grammachy bench` needs no
//! repository checkout. The fixture grows only through real user sentences
//! (`docs/spec/evals.md` section 1), so the two counts below are read from the
//! file rather than fixed here.

use serde::{Deserialize, Serialize};

use crate::args::NativeLanguage;

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/interference-30.json"
));

/// One fixture item.
///
/// `edits` is empty on the correct sentences, which is what makes a sentence a
/// false positive probe rather than an interference probe.
#[derive(Debug, Clone, Deserialize)]
pub struct Sentence {
    pub id: String,
    pub native: String,
    pub text: String,
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// The text after every edit, the Corrected text a perfect engine produces.
    pub expected_text: String,
}

/// One mistake of an item and the replacement that corrects it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    /// The text the span quotes, kept so the file can be read by eye.
    #[serde(default)]
    pub text: String,
    /// The replacement. Empty deletes the span.
    pub fix: String,
    #[serde(default, rename = "type")]
    pub kind: String,
}

impl Edit {
    pub fn span(&self) -> Span {
        Span {
            start: self.start,
            end: self.end,
        }
    }
}

/// A half-open span in UTF-16 code units, the unit of spec section 5.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Whether two spans share at least one code unit.
    pub fn overlaps(self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

impl Sentence {
    /// The Native language this sentence is checked with.
    ///
    /// A code the spec does not list reads as `none`, the same rule the stored
    /// Settings follow (spec section 7).
    pub fn native_language(&self) -> NativeLanguage {
        NativeLanguage::from_stored(&self.native).unwrap_or(NativeLanguage::None)
    }

    /// Whether the item carries a mistake to catch.
    pub fn is_interference(&self) -> bool {
        !self.edits.is_empty()
    }
}

/// Every sentence of the fixture, in file order.
pub fn sentences() -> Vec<Sentence> {
    serde_json::from_str(FIXTURE).expect("the compiled fixture is a sentence list")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::utf16_slice;

    #[test]
    fn the_fixture_holds_interference_sentences_and_correct_ones() {
        let sentences = sentences();
        let interference = sentences
            .iter()
            .filter(|sentence| sentence.is_interference())
            .count();
        let clean = sentences.len() - interference;

        assert_eq!(interference, 30, "the interference half of the fixture");
        assert!(clean > 0, "the fixture holds correct sentences too");
    }

    #[test]
    fn every_edit_quotes_the_sentence_it_belongs_to() {
        for sentence in sentences() {
            for edit in &sentence.edits {
                assert!(
                    edit.start < edit.end,
                    "{} has a non-empty span",
                    sentence.id
                );
                assert_eq!(
                    utf16_slice(&sentence.text, edit.start, edit.end),
                    Some(edit.text.as_str()),
                    "{} quotes its own text",
                    sentence.id
                );
            }
        }
    }

    #[test]
    fn a_correct_sentence_expects_itself() {
        for sentence in sentences() {
            if !sentence.is_interference() {
                assert_eq!(sentence.text, sentence.expected_text, "{}", sentence.id);
            }
        }
    }

    #[test]
    fn a_span_overlaps_only_when_it_shares_a_code_unit() {
        let span = Span { start: 4, end: 8 };

        assert!(span.overlaps(Span { start: 7, end: 20 }));
        assert!(span.overlaps(Span { start: 0, end: 5 }));
        assert!(!span.overlaps(Span { start: 8, end: 12 }));
        assert!(!span.overlaps(Span { start: 0, end: 4 }));
    }

    #[test]
    fn an_unknown_native_code_reads_as_none() {
        let sentence = Sentence {
            id: "x-01".to_string(),
            native: "kl".to_string(),
            text: "text".to_string(),
            edits: Vec::new(),
            expected_text: "text".to_string(),
        };

        assert_eq!(sentence.native_language(), NativeLanguage::None);
    }
}
