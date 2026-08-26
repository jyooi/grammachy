//! The eval set of `bench --eval-set`, spec `docs/spec/evals.md` section 2.
//!
//! The fixture answers "did a release regress"; this set answers "which model
//! is recommended". It is 365 items: 300 FCE error sentences, 50 per native
//! language, 25 error-free FCE controls, and the 40-item fixture.
//!
//! The FCE half is never committed. ADR 0003 is the stance: the corpus is
//! licensed for non-commercial research, so the tarball is fetched into a
//! gitignored cache at run time ([`cache`]), the repository commits a
//! text-free selection ([`sidecar`]), and the sentences are rebuilt from the
//! cache on every run ([`corpus`] and [`convert`]).
//!
//! A machine with no cache is not an error. [`load`] answers one sentence that
//! says why, and the runner prints the eval tables as skipped with that reason,
//! so a clean clone still produces a whole benchmark file.

pub mod cache;
pub mod convert;
pub mod corpus;
pub mod draw;
pub mod sidecar;

use std::collections::HashMap;

use crate::bench::fixture::{self, Edit, Sentence};

use corpus::Block;
use sidecar::{Entry, Sidecar};

/// The whole eval set, or the one sentence that says why this machine has none.
pub fn load() -> Result<Vec<Sentence>, String> {
    let cache = cache::ensure()?;
    let blocks = corpus::blocks(&cache)?;
    let mut items = resolve(&blocks, &sidecar::committed())?;
    items.extend(fixture::sentences());
    Ok(items)
}

/// Rebuild the selected sentences from this corpus.
///
/// A sentence the cache does not hold, or one that converts to other offsets,
/// ends the whole set: half an eval set would rank models on a different
/// question than the file claims.
pub fn resolve(blocks: &[Block], selection: &Sidecar) -> Result<Vec<Sentence>, String> {
    let mut index: HashMap<(usize, usize), &Block> = HashMap::with_capacity(blocks.len());
    for block in blocks {
        index.insert((block.document, block.sentence), block);
    }

    selection
        .items
        .iter()
        .map(|entry| rebuild(&index, entry, &selection.release))
        .collect()
}

/// One selected sentence, checked against what the sidecar recorded of it.
fn rebuild(
    index: &HashMap<(usize, usize), &Block>,
    entry: &Entry,
    release: &str,
) -> Result<Sentence, String> {
    let block = index
        .get(&(entry.document, entry.sentence))
        .ok_or_else(|| format!("the cached corpus has no sentence for {} of {release}", entry.id))?;
    let item = convert::item(block)
        .ok_or_else(|| format!("the cached corpus no longer yields {} of {release}", entry.id))?;

    if item.edits.len() != entry.edits.len()
        || item
            .edits
            .iter()
            .zip(&entry.edits)
            .any(|(built, recorded)| {
                built.start != recorded.start
                    || built.end != recorded.end
                    || built.code != recorded.code
            })
    {
        return Err(format!(
            "the cached corpus places the edits of {} elsewhere than the sidecar records",
            entry.id
        ));
    }

    Ok(Sentence {
        id: entry.id.clone(),
        native: item.native,
        text: item.text,
        edits: item
            .edits
            .into_iter()
            .map(|edit| Edit {
                start: edit.start,
                end: edit.end,
                text: edit.text,
                fix: edit.fix,
                kind: edit.code,
            })
            .collect(),
        expected_text: item.expected_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use corpus::Edit as M2Edit;
    use sidecar::{Entry, EntryEdit};

    fn block(document: usize, text: &str, edits: Vec<M2Edit>) -> Block {
        Block {
            document,
            sentence: 0,
            native: "zh".to_string(),
            tokens: text.split(' ').map(str::to_string).collect(),
            edits,
        }
    }

    fn selection(edits: Vec<EntryEdit>) -> Sidecar {
        Sidecar {
            release: "test".to_string(),
            seed: 1,
            items: vec![Entry {
                id: "fce-zh-01".to_string(),
                document: 3,
                sentence: 0,
                edits,
            }],
        }
    }

    #[test]
    fn a_selected_sentence_is_rebuilt_from_the_corpus() {
        let blocks = vec![block(
            3,
            "I saws the show on the wall .",
            vec![M2Edit {
                start: 1,
                end: 2,
                code: "R:VERB:TENSE".to_string(),
                correction: "saw".to_string(),
            }],
        )];

        let items = resolve(
            &blocks,
            &selection(vec![EntryEdit {
                start: 2,
                end: 6,
                code: "R:VERB:TENSE".to_string(),
            }]),
        )
        .unwrap();

        assert_eq!(items[0].id, "fce-zh-01");
        assert_eq!(items[0].text, "I saws the show on the wall.");
        assert_eq!(items[0].edits[0].text, "saws");
        assert_eq!(items[0].edits[0].kind, "R:VERB:TENSE");
        assert_eq!(items[0].expected_text, "I saw the show on the wall.");
    }

    #[test]
    fn a_corpus_that_places_the_edit_elsewhere_ends_the_set() {
        let blocks = vec![block(
            3,
            "I saws the show on the wall .",
            vec![M2Edit {
                start: 1,
                end: 2,
                code: "R:VERB:TENSE".to_string(),
                correction: "saw".to_string(),
            }],
        )];

        let error = resolve(
            &blocks,
            &selection(vec![EntryEdit {
                start: 9,
                end: 13,
                code: "R:VERB:TENSE".to_string(),
            }]),
        )
        .unwrap_err();

        assert!(error.contains("elsewhere than the sidecar records"), "{error}");
    }

    #[test]
    fn a_cache_without_the_sentence_ends_the_set() {
        let error = resolve(&[], &selection(Vec::new())).unwrap_err();

        assert!(error.contains("has no sentence for fce-zh-01"), "{error}");
    }
}
