//! Drawing the 365 items of the eval set, spec `evals.md` section 2.
//!
//! The draw runs once, against the fetched release, and its answer is the
//! committed sidecar. It is kept in the binary and tested because the sidecar
//! has to be reproducible: a reader who fetches the same release and runs the
//! same draw must get the same 365 items, or the selection is a fact nobody
//! can check.
//!
//! The generator is seeded, so the shuffle is a fixed permutation rather than
//! a run-to-run one. At most one item comes from any one essay, so the set
//! spreads over writers rather than sampling one writer many times.

use std::collections::{HashMap, HashSet};

use super::convert::{self, Item, LANGUAGES};
use super::corpus::Block;

/// The seed of the draw, fixed so the sidecar can be reproduced.
pub const SEED: u64 = 0x6772_616d_6d61_6368;

/// Error sentences drawn per native language.
pub const PER_LANGUAGE: usize = 50;

/// Error-free sentences drawn as false-positive controls.
pub const CONTROLS: usize = 25;

/// One drawn item and the id the eval set knows it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawn {
    pub id: String,
    pub item: Item,
}

/// The eval set this corpus yields, in the order the sidecar records it.
///
/// Errors come first, language by language in [`LANGUAGES`] order, then the
/// controls. A language the corpus cannot fill yields what it has, so a
/// smaller corpus draws a smaller set rather than failing.
pub fn draw(blocks: &[Block]) -> Vec<Drawn> {
    let candidates: Vec<Item> = blocks.iter().filter_map(convert::item).collect();
    let order = shuffled(candidates.len());

    let mut used: HashSet<usize> = HashSet::new();
    let mut errors: HashMap<&str, Vec<&Item>> = HashMap::new();
    for &index in &order {
        let item = &candidates[index];
        if !item.is_interference() || used.contains(&item.document) {
            continue;
        }
        let taken = errors.entry(language_of(item)).or_default();
        if taken.len() == PER_LANGUAGE {
            continue;
        }
        taken.push(item);
        used.insert(item.document);
    }

    let mut drawn: Vec<Drawn> = Vec::new();
    for language in LANGUAGES {
        for (place, item) in errors
            .remove(language)
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            drawn.push(Drawn {
                id: format!("fce-{language}-{:02}", place + 1),
                item: (*item).clone(),
            });
        }
    }

    let mut controls = 0;
    for &index in &order {
        let item = &candidates[index];
        if controls == CONTROLS || item.is_interference() || used.contains(&item.document) {
            continue;
        }
        controls += 1;
        used.insert(item.document);
        drawn.push(Drawn {
            id: format!("fce-ok-{controls:02}"),
            item: item.clone(),
        });
    }

    drawn
}

/// The language key of one candidate, borrowed from [`LANGUAGES`].
///
/// [`convert::item`] keeps nothing outside that list, so the lookup always
/// answers.
fn language_of(item: &Item) -> &'static str {
    LANGUAGES
        .iter()
        .copied()
        .find(|language| *language == item.native)
        .expect("a kept candidate is one of the six languages")
}

/// A fixed permutation of `0..count`.
fn shuffled(count: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    let mut rng = Rng(SEED);
    for index in (1..count).rev() {
        order.swap(index, rng.below(index + 1));
    }
    order
}

/// SplitMix64, a generator small enough to read and fixed enough to commit.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::evalset::corpus::Edit;

    /// A corpus of one error sentence and one control per language, plus a
    /// second error sentence in the same essay as the first.
    fn sample() -> Vec<Block> {
        let mut blocks = Vec::new();
        for (index, language) in LANGUAGES.iter().enumerate() {
            let document = index * 2;
            blocks.push(block(
                document,
                0,
                language,
                "I go to the cinema with my friends yesterday .",
                vec![Edit {
                    start: 1,
                    end: 2,
                    code: "R:VERB:TENSE".to_string(),
                    correction: "went".to_string(),
                }],
            ));
            blocks.push(block(
                document,
                1,
                language,
                "She take the bus to work every single morning .",
                vec![Edit {
                    start: 1,
                    end: 2,
                    code: "R:VERB:SVA".to_string(),
                    correction: "takes".to_string(),
                }],
            ));
            blocks.push(block(
                document + 1,
                0,
                language,
                "My cousin paints small wooden boats in her garage .",
                Vec::new(),
            ));
        }
        blocks
    }

    fn block(
        document: usize,
        sentence: usize,
        native: &str,
        text: &str,
        edits: Vec<Edit>,
    ) -> Block {
        Block {
            document,
            sentence,
            native: native.to_string(),
            tokens: text.split(' ').map(str::to_string).collect(),
            edits,
        }
    }

    #[test]
    fn the_draw_takes_one_item_per_essay_and_names_it_by_language() {
        let drawn = draw(&sample());

        assert_eq!(drawn.len(), 12, "six errors and six controls");
        let ids: Vec<&str> = drawn.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            &ids[..6],
            [
                "fce-zh-01",
                "fce-es-01",
                "fce-fr-01",
                "fce-de-01",
                "fce-pt-01",
                "fce-ja-01"
            ]
        );
        assert_eq!(
            &ids[6..],
            [
                "fce-ok-01",
                "fce-ok-02",
                "fce-ok-03",
                "fce-ok-04",
                "fce-ok-05",
                "fce-ok-06"
            ]
        );

        let documents: HashSet<usize> = drawn.iter().map(|item| item.item.document).collect();
        assert_eq!(documents.len(), drawn.len(), "no essay is drawn twice");
    }

    #[test]
    fn the_same_corpus_draws_the_same_set_every_time() {
        assert_eq!(draw(&sample()), draw(&sample()));
    }

    #[test]
    fn an_error_item_carries_its_one_edit_and_a_control_carries_none() {
        let drawn = draw(&sample());
        let error = &drawn[0].item;
        let control = &drawn[6].item;

        assert_eq!(error.edits.len(), 1);
        assert!(control.edits.is_empty());
        assert_eq!(control.text, control.expected_text);
    }

    #[test]
    fn the_permutation_is_fixed_rather_than_run_to_run() {
        assert_eq!(shuffled(8), shuffled(8));
        assert_ne!(shuffled(8), (0..8).collect::<Vec<usize>>());
    }
}
