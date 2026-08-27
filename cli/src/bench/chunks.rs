//! The Chunk fixture the benchmark runs, `docs/spec/evals.md` section 1.
//!
//! The fixture and the eval set both grade one sentence at a time. Neither of
//! them proves the thing a Compose user meets first: a whole Chunk, at the
//! Check size limit of the local engine, answered inside the timeout. That is
//! what these Drafts are for, and why the Chunk table reports wall time,
//! validity, and recall rather than a ranking (HUF-219).
//!
//! One Draft per native language, in the item shape of the evals spec, so the
//! one loader and the one metrics module read it unchanged. Every Draft is the
//! project's own writing: no corpus text is copied, so these files carry no
//! licence of their own and are committed like the interference fixture.
//!
//! The Drafts are compiled into the binary, so a released `grammachy bench`
//! needs no repository checkout.

use super::fixture::Sentence;

/// The Drafts, one per native language, in the order the Chunk table prints.
///
/// The list is the spec's language list. A language with no file here would
/// leave a gap the table could not show, so the array is what a new language
/// is added to.
const DRAFTS: [&str; 7] = [
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/zh.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/ms.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/es.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/fr.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/de.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/pt.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/chunks/ja.json"
    )),
];

/// The path the report names, so a reader finds the Drafts from the file.
pub const DIRECTORY: &str = "cli/tests/fixtures/chunks/";

/// Every Draft of the Chunk fixture, in file order.
pub fn drafts() -> Vec<Sentence> {
    DRAFTS
        .iter()
        .map(|draft| serde_json::from_str(draft).expect("a compiled Draft is one item"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::EngineSlug;
    use crate::text::{utf16_len, utf16_slice};

    /// Apply every edit of one Draft, the Corrected text a perfect Check gives.
    fn corrected(draft: &Sentence) -> String {
        let mut out = String::new();
        let mut at = 0;
        for edit in &draft.edits {
            out.push_str(utf16_slice(&draft.text, at, edit.start).expect("the span is inside"));
            out.push_str(&edit.fix);
            at = edit.end;
        }
        out.push_str(utf16_slice(&draft.text, at, utf16_len(&draft.text)).expect("the tail"));
        out
    }

    #[test]
    fn there_is_one_draft_per_native_language_of_the_spec() {
        let natives: Vec<String> = drafts().into_iter().map(|draft| draft.native).collect();

        assert_eq!(natives, ["zh", "ms", "es", "fr", "de", "pt", "ja"]);
    }

    #[test]
    fn every_draft_has_an_id_of_its_own() {
        let mut ids: Vec<String> = drafts().into_iter().map(|draft| draft.id).collect();
        let planned = ids.len();
        ids.sort();
        ids.dedup();

        assert_eq!(ids.len(), planned, "two Drafts share an id");
    }

    /// A Draft over the limit would be refused by `check` rather than measured,
    /// so the Chunk row would carry no number at all.
    #[test]
    fn every_draft_fits_one_check_of_the_local_engine() {
        let limit = EngineSlug::Openai.check_limit_utf16();

        for draft in drafts() {
            let units = utf16_len(&draft.text);
            assert!(units <= limit, "{} is {units} units", draft.id);
            // A Draft far under the limit would not prove the heavy case the
            // Compose surface meets, which is the whole point of this fixture.
            assert!(units > limit * 3 / 4, "{} is only {units} units", draft.id);
        }
    }

    #[test]
    fn every_edit_quotes_the_draft_it_belongs_to() {
        for draft in drafts() {
            let mut previous_end = 0;
            for edit in &draft.edits {
                assert!(edit.start < edit.end, "{} has a non-empty span", draft.id);
                assert!(
                    edit.start >= previous_end,
                    "{} has edits in order and never overlapping",
                    draft.id
                );
                assert_eq!(
                    utf16_slice(&draft.text, edit.start, edit.end),
                    Some(edit.text.as_str()),
                    "{} quotes its own text at {}",
                    draft.id,
                    edit.start
                );
                assert_ne!(
                    edit.text, edit.fix,
                    "{} has an edit that fixes nothing",
                    draft.id
                );
                previous_end = edit.end;
            }
        }
    }

    #[test]
    fn applying_every_edit_of_a_draft_gives_its_expected_text() {
        for draft in drafts() {
            assert_eq!(corrected(&draft), draft.expected_text, "{}", draft.id);
        }
    }

    /// A Draft with no edit would measure a recall of nothing.
    #[test]
    fn every_draft_carries_the_errors_the_table_measures_recall_against() {
        for draft in drafts() {
            assert!(draft.is_interference(), "{} has edits", draft.id);
            assert!(
                draft.edits.len() >= 15,
                "{} has {} edits",
                draft.id,
                draft.edits.len()
            );
        }
    }
}
