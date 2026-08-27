//! The committed selection, `cli/tests/fixtures/eval-set.sidecar.json`.
//!
//! ADR 0003 draws a hard line here: this file is the one part of the eval set
//! the repository carries, and it must hold no corpus text. So an entry names
//! the item's id, the essay and the sentence inside it, the UTF-16 offsets of
//! the expected edit, and the ERRANT code. Sentence, fix, and every other
//! string are read from the fetched cache at run time.
//!
//! The offsets are not needed to find the sentence, which the essay and
//! sentence index already do. They are the check: a cache that converts to
//! other offsets is not the release this selection was drawn from, and the
//! eval tables skip rather than report numbers about the wrong sentences.

use serde::{Deserialize, Serialize};

use super::draw::Drawn;

/// The committed sidecar, compiled in so a released binary needs no checkout.
const FILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/eval-set.sidecar.json"
));

/// The whole selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    /// The release the selection was drawn from.
    pub release: String,
    /// The seed of the draw, so the selection can be reproduced.
    pub seed: u64,
    pub items: Vec<Entry>,
}

/// One selected sentence, named without quoting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// The essay, counted over the splits in corpus order.
    pub document: usize,
    /// The place of the sentence inside that essay.
    pub sentence: usize,
    /// Empty on an error-free control.
    #[serde(default)]
    pub edits: Vec<EntryEdit>,
}

impl Entry {
    /// The writer's first language, read from the id.
    ///
    /// The entry carries no language of its own, because ADR 0003 holds this
    /// file to ids, indices, offsets, and codes. The id already names one, as
    /// in `fce-zh-01`, so it is what a rebuilt sentence is checked against. A
    /// control is drawn from every language and names none.
    pub fn native(&self) -> Option<&str> {
        let segment = self.id.split('-').nth(1)?;
        super::convert::LANGUAGES
            .into_iter()
            .find(|language| *language == segment)
    }
}

/// One expected mistake, in UTF-16 offsets and an ERRANT code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryEdit {
    pub start: usize,
    pub end: usize,
    /// The ERRANT code, such as `R:PREP`. It names a class of mistake, so it
    /// is the one string of an edit and it quotes no corpus text.
    pub code: String,
}

/// The committed selection.
pub fn committed() -> Sidecar {
    serde_json::from_str(FILE).expect("the compiled sidecar is a selection")
}

/// The sidecar one draw produces.
pub fn of(drawn: &[Drawn]) -> Sidecar {
    Sidecar {
        release: super::cache::RELEASE.to_string(),
        seed: super::draw::SEED,
        items: drawn
            .iter()
            .map(|item| Entry {
                id: item.id.clone(),
                document: item.item.document,
                sentence: item.item.sentence,
                edits: item
                    .item
                    .edits
                    .iter()
                    .map(|edit| EntryEdit {
                        start: edit.start,
                        end: edit.end,
                        code: edit.code.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The file text of one selection, the bytes the repository commits.
pub fn render(sidecar: &Sidecar) -> String {
    let mut text = serde_json::to_string_pretty(sidecar).expect("a selection serialises");
    text.push('\n');
    text
}

/// Every string this selection carries, the set the licence test checks.
///
/// Field names are not here: they are this project's own words. Values are,
/// because a value is where corpus text could hide.
pub fn strings(sidecar: &Sidecar) -> Vec<&str> {
    let mut out = vec![sidecar.release.as_str()];
    for entry in &sidecar.items {
        out.push(entry.id.as_str());
        out.extend(entry.edits.iter().map(|edit| edit.code.as_str()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_sidecar_reads_back() {
        let sidecar = committed();

        assert_eq!(sidecar.release, super::super::cache::RELEASE);
        assert_eq!(sidecar.seed, super::super::draw::SEED);
    }

    #[test]
    fn a_rendered_selection_reads_back_as_itself() {
        let sidecar = committed();

        let read: Sidecar = serde_json::from_str(&render(&sidecar)).expect("it reads back");

        assert_eq!(read, sidecar);
    }
}
