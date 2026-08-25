//! The interference fixture the benchmark runs, spec section 13.1.
//!
//! The file is `tests/fixtures/interference-30.json`, the test set of HUF-171.
//! It is compiled into the binary, so a released `grammachy bench` needs no
//! repository checkout. The fixture grows only through real user sentences
//! (spec section 13.1), so the two counts below are read from the file rather
//! than fixed here.

use serde::Deserialize;

use crate::args::NativeLanguage;

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/interference-30.json"
));

/// One fixture sentence.
///
/// `expected_span` is absent on the correct sentences, which is what makes a
/// sentence a false positive probe rather than an interference probe.
#[derive(Debug, Clone, Deserialize)]
pub struct Sentence {
    pub id: String,
    pub native: String,
    pub text: String,
    pub expected_span: Option<Span>,
    #[serde(default)]
    pub expected_fix: Option<String>,
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
}

/// Every sentence of the fixture, in file order.
pub fn sentences() -> Vec<Sentence> {
    serde_json::from_str(FIXTURE).expect("the compiled fixture is a sentence list")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_holds_interference_sentences_and_correct_ones() {
        let sentences = sentences();
        let interference = sentences
            .iter()
            .filter(|sentence| sentence.expected_span.is_some())
            .count();
        let clean = sentences.len() - interference;

        assert_eq!(interference, 30, "the interference half of the fixture");
        assert!(clean > 0, "the fixture holds correct sentences too");
    }

    #[test]
    fn every_expected_span_quotes_the_sentence_it_belongs_to() {
        for sentence in sentences() {
            let Some(span) = sentence.expected_span else {
                continue;
            };
            let units: Vec<u16> = sentence.text.encode_utf16().collect();
            assert!(
                span.start < span.end && span.end <= units.len(),
                "{} has a span inside its text",
                sentence.id
            );
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
            expected_span: None,
            expected_fix: None,
        };

        assert_eq!(sentence.native_language(), NativeLanguage::None);
    }
}
