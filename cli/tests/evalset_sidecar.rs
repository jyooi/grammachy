//! Redrawing the committed eval-set selection, spec `evals.md` section 2.
//!
//! The selection is drawn once and committed as
//! `tests/fixtures/eval-set.sidecar.json`. This case is how it is drawn again:
//! it needs the fetched corpus, so it is ignored by default and no CI run ever
//! reaches `cl.cam.ac.uk`.
//!
//! Fill the cache and redraw with:
//!
//! ```text
//! cargo test --test evalset_sidecar -- --ignored --nocapture
//! ```
//!
//! The draw is seeded, so the same release redraws the same 325 items. A
//! different answer means the conversion rules changed, and the sidecar is
//! then a new selection rather than a correction of the old one. The run adds
//! the 40 fixture items to them, which the sidecar never holds.

use std::path::Path;

use grammachy::bench::evalset::{cache, corpus, draw, sidecar};

#[test]
#[ignore = "needs the fetched corpus; run with --ignored to redraw the sidecar"]
fn redraw_the_committed_selection() {
    let cache = cache::ensure().expect("the corpus cache is filled");
    let blocks = corpus::blocks(&cache).expect("the corpus reads");
    let drawn = draw::draw(&blocks);

    let by_language = |language: &str| {
        drawn
            .iter()
            .filter(|item| item.id.starts_with(&format!("fce-{language}-")))
            .count()
    };
    for language in grammachy::bench::evalset::convert::LANGUAGES {
        assert_eq!(
            by_language(language),
            draw::PER_LANGUAGE,
            "the corpus fills {language}"
        );
    }
    let controls = drawn
        .iter()
        .filter(|item| item.id.starts_with("fce-ok-"))
        .count();
    assert_eq!(controls, draw::CONTROLS, "the controls are filled");

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval-set.sidecar.json");
    std::fs::write(&path, sidecar::render(&sidecar::of(&drawn))).expect("the sidecar is written");
    println!("redrew {} items into {}", drawn.len(), path.display());
}
