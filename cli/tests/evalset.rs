//! The eval set of `bench --eval-set`, spec `docs/spec/evals.md` section 2.
//!
//! Two promises are checked here. The licence promise of ADR 0003: the
//! committed sidecar carries no corpus text, and a machine with no corpus
//! cache prints the eval tables as skipped rather than failing. And the
//! conversion promise: the M2 alignment turns into the item shape of HUF-205
//! through the agreed rules, and the draw is reproducible.
//!
//! No case here reaches the network. The conversion and the draw run against a
//! synthetic M2 sample committed beside this file, whose sentences are this
//! project's own. The one case that reads the real corpus skips itself when the
//! cache is empty, the way the live engine cases do.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use grammachy::bench::evalset::{convert, corpus, draw, sidecar};

/// The synthetic M2 sample and the JSON beside it.
fn sample() -> (String, String) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval-m2-sample");
    (
        std::fs::read_to_string(base.join("sample.m2")).expect("the sample M2 is readable"),
        std::fs::read_to_string(base.join("sample.json")).expect("the sample JSON is readable"),
    )
}

fn sample_blocks() -> Vec<corpus::Block> {
    let (m2, json) = sample();
    corpus::blocks_of("sample", &m2, &json, 0).expect("the sample lines up")
}

/// The real corpus cache, or `None` when this machine has not filled it.
fn cache_root() -> Option<PathBuf> {
    let root = grammachy::bench::evalset::cache::directory().join("fce");
    root.join("m2").is_dir().then_some(root)
}

#[test]
fn the_sidecar_carries_ids_offsets_and_codes_and_no_other_string() {
    let selection = sidecar::committed();

    assert_eq!(selection.release, "fce_v2.1.bea19");
    for entry in &selection.items {
        assert!(
            entry.id.starts_with("fce-"),
            "an id names the set, not the sentence: {}",
            entry.id
        );
        assert!(
            entry
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "an id is a slug: {}",
            entry.id
        );
        for edit in &entry.edits {
            assert!(edit.start < edit.end, "{} has a non-empty span", entry.id);
            assert!(
                edit.code
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == ':'),
                "an edit carries an ERRANT code and nothing else: {}",
                edit.code
            );
        }
    }
}

/// Every licensed string of the release: the sentences and the corrections.
///
/// The M2 file also carries the annotators' own metadata, the error codes and
/// the `REQUIRED` markers. Those are not corpus text, and the sidecar's codes
/// come from that vocabulary rather than from anything a writer wrote, so the
/// promise is about the sentences and the fixes.
fn corpus_text(root: &Path) -> String {
    let mut out = String::new();
    for split in corpus::SPLITS {
        let m2 =
            std::fs::read_to_string(root.join("m2").join(format!("fce.{split}.gold.bea19.m2")))
                .expect("the M2 file is readable");
        for line in m2.lines() {
            if let Some(sentence) = line.strip_prefix("S ") {
                out.push_str(sentence);
                out.push('\n');
            } else if let Some(annotation) = line.strip_prefix("A ") {
                if let Some(correction) = annotation.split("|||").nth(2) {
                    out.push_str(correction);
                    out.push('\n');
                }
            }
        }

        let json = std::fs::read_to_string(root.join("json").join(format!("fce.{split}.json")))
            .expect("the JSON file is readable");
        for line in json.lines().filter(|line| !line.trim().is_empty()) {
            let essay: serde_json::Value =
                serde_json::from_str(line).expect("the JSON is one essay per line");
            out.push_str(essay["text"].as_str().expect("an essay carries its text"));
            out.push('\n');
            out.push_str(&essay["edits"].to_string());
            out.push('\n');
        }
    }
    out
}

/// The promise of ADR 0003, checked against the corpus itself.
///
/// The case needs the fetched corpus, so it skips on a machine that has not
/// filled the cache, the way the live engine cases skip on a silent port.
#[test]
fn no_string_of_the_sidecar_appears_in_the_fetched_corpus() {
    let Some(root) = cache_root() else {
        eprintln!("skipped: the corpus cache is empty");
        return;
    };
    let text = corpus_text(&root);

    let selection = sidecar::committed();
    for value in sidecar::strings(&selection) {
        assert!(
            !text.contains(value),
            "the sidecar string {value:?} is corpus text"
        );
    }
}

#[test]
fn the_committed_selection_is_the_composition_the_spec_fixes() {
    let selection = sidecar::committed();

    for language in convert::LANGUAGES {
        let head = format!("fce-{language}-");
        assert_eq!(
            selection
                .items
                .iter()
                .filter(|entry| entry.id.starts_with(&head))
                .count(),
            draw::PER_LANGUAGE,
            "error sentences for {language}"
        );
    }
    let controls = selection
        .items
        .iter()
        .filter(|entry| entry.id.starts_with("fce-ok-"))
        .count();
    assert_eq!(controls, draw::CONTROLS, "error-free controls");
    assert_eq!(
        selection.items.len(),
        convert::LANGUAGES.len() * draw::PER_LANGUAGE + draw::CONTROLS
    );

    let mut places: Vec<(usize, usize)> = selection
        .items
        .iter()
        .map(|entry| (entry.document, entry.sentence))
        .collect();
    places.sort_unstable();
    let drawn = places.len();
    places.dedup();
    assert_eq!(places.len(), drawn, "no sentence is drawn twice");
}

#[test]
fn the_conversion_reads_the_m2_alignment_through_the_agreed_rules() {
    let items: Vec<convert::Item> = sample_blocks().iter().filter_map(convert::item).collect();
    let texts: Vec<&str> = items.iter().map(|item| item.text.as_str()).collect();

    assert!(
        !texts.iter().any(|text| text.contains("recieve")),
        "a spelling edit is not measured: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("She stays here")),
        "a block of two sentences is not drawn: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("a advice")),
        "a sentence with two edits is not drawn: {texts:?}"
    );

    let replacement = items
        .iter()
        .find(|item| item.text.starts_with("We arrived"))
        .expect("the preposition sentence is drawn");
    assert_eq!(replacement.edits[0].text, "to");
    assert_eq!(replacement.edits[0].fix, "at");
    assert_eq!(replacement.edits[0].code, "R:PREP");
    assert_eq!(
        replacement.expected_text,
        "We arrived at the station at eight in the morning."
    );

    let missing = items
        .iter()
        .find(|item| item.text.starts_with("I went to"))
        .expect("the missing determiner sentence is drawn");
    assert!(
        missing.edits[0].start < missing.edits[0].end,
        "a missing word is widened onto the word that follows it"
    );
    assert_eq!(missing.edits[0].fix, "the cinema");
    assert_eq!(
        missing.expected_text,
        "I went to the cinema with my brother last night."
    );

    let control = items
        .iter()
        .find(|item| !item.is_interference())
        .expect("a control is drawn");
    assert_eq!(control.text, control.expected_text);
}

#[test]
fn the_draw_is_reproducible_and_rebuilds_from_its_own_sidecar() {
    let blocks = sample_blocks();
    let drawn = draw::draw(&blocks);

    assert_eq!(
        drawn,
        draw::draw(&blocks),
        "the same corpus draws the same set"
    );
    let ids: Vec<&str> = drawn.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["fce-zh-01", "fce-es-01", "fce-ok-01", "fce-ok-02"]);

    let selection = sidecar::of(&drawn);
    let rebuilt = grammachy::bench::evalset::resolve(&blocks, &selection)
        .expect("the sidecar rebuilds against the corpus it was drawn from");

    assert_eq!(rebuilt.len(), drawn.len());
    for (item, drawn) in rebuilt.iter().zip(&drawn) {
        assert_eq!(item.id, drawn.id);
        assert_eq!(item.text, drawn.item.text);
        assert_eq!(item.expected_text, drawn.item.expected_text);
    }
}

/// An address on the loopback interface with nothing listening on it.
fn silent_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    drop(listener);
    address
}

/// Spec `evals.md` section 2.1: no cache means a skipped table with a reason.
#[test]
fn a_run_without_the_cache_skips_the_eval_tables_rather_than_failing() {
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("eval-set-absent");
    let _ = std::fs::remove_dir_all(&empty);
    std::fs::create_dir_all(&empty).expect("the scratch directory is created");

    // Every engine is pointed at a dead port, so the run reaches no server
    // this machine happens to be running.
    let shell_json = empty.join("shell.json");
    std::fs::write(
        &shell_json,
        format!(
            r#"{{ "bar": {{ "layout": {{ "left": [], "center": [
                {{ "id": "io.github.jyooi.grammachy", "openaiBaseUrl": "http://{}" }}
            ], "right": [] }} }}, "plugins": [] }}"#,
            silent_address()
        ),
    )
    .expect("the settings file is written");

    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .args(["bench", "--eval-set"])
        .env("GRAMMACHY_EVAL_CACHE", &empty)
        .env("GRAMMACHY_EVAL_FETCH", "never")
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_LLAMA_START", "never")
        .env("GRAMMACHY_SHELL_JSON", &shell_json)
        .output()
        .expect("the binary runs");
    let report = String::from_utf8(output.stdout).expect("stdout is UTF-8");

    assert_eq!(
        output.status.code(),
        Some(0),
        "a missing corpus is no error"
    );
    assert!(
        report.contains("- The eval set: "),
        "the reason is under Skipped: {report}"
    );
    assert!(
        report.contains("forbids the fetch"),
        "the reason says what stopped it: {report}"
    );
    assert!(
        !report.contains("## Models (eval set)"),
        "no eval tables were printed: {report}"
    );
    assert!(
        !report.contains("CLC FCE Dataset Licence"),
        "a run with no eval set claims no corpus: {report}"
    );
}
