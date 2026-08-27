//! Filling the corpus cache, spec `docs/spec/evals.md` section 2.1.
//!
//! ADR 0003 is the licence stance, so the fetch is the step that has to be
//! right: the tarball is pinned by sha256, it lands in a gitignored cache, and
//! the first fill puts the reader on the licence.
//!
//! No case here reaches `cl.cam.ac.uk`. Every one of them builds its own small
//! tarball in the same layout as the release and serves it over a `file://`
//! URL, which is the same `curl` route the real fetch takes.

use std::path::{Path, PathBuf};
use std::process::Command;

use grammachy::bench::evalset::cache::{self, Source};
use grammachy::model::digest::sha256_path;
use grammachy::model::Downloader;

/// The licence text of the release, standing in for the real one.
const LICENCE: &str = "The CLC FCE Dataset Licence.\n";

/// The synthetic split this project committed for its own conversion cases.
///
/// It is this project's own sentences, so a tarball built from it carries no
/// corpus text (ADR 0003). It is also the sample `evalset.rs` reads, so it
/// pairs the way the release pairs.
fn split_files() -> (String, String) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval-m2-sample");
    (
        std::fs::read_to_string(base.join("sample.m2")).expect("the sample M2 is readable"),
        std::fs::read_to_string(base.join("sample.json")).expect("the sample JSON is readable"),
    )
}

/// A directory of this test binary alone.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

/// A tarball in the layout of the release, and the digest it pins to.
///
/// `splits` names the splits it carries, so a case can build one that unpacks
/// half a release.
fn tarball(directory: &Path, splits: &[&str]) -> Source {
    let tree = directory.join("tree");
    let root = tree.join("fce");
    std::fs::create_dir_all(root.join("m2")).expect("the m2 directory is created");
    std::fs::create_dir_all(root.join("json")).expect("the json directory is created");
    std::fs::write(root.join("licence.txt"), LICENCE).expect("the licence is written");
    let (m2, json) = split_files();
    for split in splits {
        std::fs::write(root.join(format!("m2/fce.{split}.gold.bea19.m2")), &m2)
            .expect("the M2 file is written");
        std::fs::write(root.join(format!("json/fce.{split}.json")), &json)
            .expect("the JSON file is written");
    }

    let path = directory.join("fce_v2.1.bea19.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&path)
        .arg("-C")
        .arg(&tree)
        .arg("fce")
        .status()
        .expect("tar runs");
    assert!(status.success(), "the sample tarball is packed");

    Source {
        url: format!("file://{}", path.display()),
        sha256: sha256_path(&path).expect("the sample tarball hashes"),
    }
}

/// The real transfer, `curl`, which reads a `file://` URL like any other.
fn curl() -> Downloader {
    Box::new(grammachy::model::curl)
}

/// A cache that fills once, from the pinned tarball and nowhere else.
#[test]
fn the_first_run_unpacks_the_pinned_tarball_and_no_later_run_fetches_again() {
    let directory = scratch("eval-cache-fill");
    let source = tarball(&directory, &["train", "dev", "test"]);
    let cache_directory = directory.join("cache");

    let cache = cache::ensure_in(&cache_directory, &source, &curl(), true)
        .expect("the pinned tarball fills the cache");

    assert!(cache.m2("train").is_file(), "the M2 tree is unpacked");
    assert!(cache.json("train").is_file(), "the JSON tree is unpacked");
    assert!(cache.licence().is_file(), "the licence is unpacked");
    assert!(
        !cache_directory.join("fce_v2.1.bea19.tar.gz.part").exists(),
        "the part file is renamed rather than left behind"
    );

    let refuse: Downloader = Box::new(|_, _| panic!("a filled cache never fetches again"));
    let again = cache::ensure_in(&cache_directory, &source, &refuse, true)
        .expect("the filled cache is read rather than fetched");

    assert_eq!(again, cache, "the second run answers the same cache");

    // A release the machine holds only half of is not the release. An unpack
    // that stopped part way must fetch the rest rather than answer as filled.
    std::fs::remove_file(cache.json("test")).expect("one split file is removed");
    let error = cache::ensure_in(&cache_directory, &source, &refuse, false).unwrap_err();

    assert!(
        error.contains("forbids the fetch"),
        "half a release fetches again: {error}"
    );
}

/// The digest is what makes the fetch safe, so a tarball that does not match
/// it is refused before anything is unpacked.
#[test]
fn a_tarball_that_misses_the_pinned_digest_is_refused_and_leaves_nothing() {
    let directory = scratch("eval-cache-digest");
    let source = tarball(&directory, &["train", "dev", "test"]);
    let wrong = Source {
        url: source.url.clone(),
        sha256: "0".repeat(64),
    };
    let cache_directory = directory.join("cache");

    let error = cache::ensure_in(&cache_directory, &wrong, &curl(), true).unwrap_err();

    assert!(
        error.contains("does not match the pinned digest"),
        "{error}"
    );
    assert!(
        error.contains(&source.sha256),
        "the error names what it got: {error}"
    );
    assert!(
        !cache_directory.join("fce_v2.1.bea19.tar.gz.part").exists(),
        "the rejected download is removed"
    );
    assert!(
        !cache_directory.join("fce").exists(),
        "nothing is unpacked from a tarball that was refused"
    );
}

/// Half a release is not the release.
///
/// The run reads every split, so a tarball that carries the train split alone
/// would read later as a corpus that cannot be parsed. The fill answers it
/// instead, and it names the file it lacks.
#[test]
fn a_tarball_that_unpacks_half_the_release_names_the_file_it_lacks() {
    let directory = scratch("eval-cache-half");
    let source = tarball(&directory, &["train"]);
    let cache_directory = directory.join("cache");

    let error = cache::ensure_in(&cache_directory, &source, &curl(), true).unwrap_err();

    assert!(error.contains("unpacked without"), "{error}");
    assert!(error.ends_with("fce.dev.gold.bea19.m2"), "{error}");
}

/// An address on the loopback interface with nothing listening on it.
fn silent_address() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    drop(listener);
    address
}

/// One `grammachy bench --eval-set` run against one cache, with every engine
/// pointed at a dead port so no server this machine happens to run answers.
fn bench(cache_directory: &Path, source: &Source, shell_json: &Path) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .args(["bench", "--eval-set"])
        .env("GRAMMACHY_EVAL_CACHE", cache_directory)
        .env(
            "GRAMMACHY_EVAL_BASE_URL",
            source.url.trim_end_matches("/fce_v2.1.bea19.tar.gz"),
        )
        .env("GRAMMACHY_EVAL_SHA256", &source.sha256)
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_LLAMA_START", "never")
        .env("GRAMMACHY_SHELL_JSON", shell_json)
        .output()
        .expect("the binary runs");

    assert_eq!(
        output.status.code(),
        Some(0),
        "a bench run is never an error"
    );
    (
        String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    )
}

/// Spec section 2.1: the licence notice prints on the first fill only.
#[test]
fn the_licence_notice_prints_on_the_first_fill_and_never_again() {
    let directory = scratch("eval-cache-notice");
    let source = tarball(&directory, &["train", "dev", "test"]);
    let cache_directory = directory.join("cache");
    let shell_json = directory.join("shell.json");
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

    let (report, stderr) = bench(&cache_directory, &source, &shell_json);

    assert!(
        stderr.contains(&format!(
            "Its licence is at {}.",
            cache_directory.join("fce/licence.txt").display()
        )),
        "the first fill names where the licence sits: {stderr}"
    );
    assert!(
        stderr.contains("for non-commercial research and educational purposes."),
        "the first fill quotes the non-commercial line: {stderr}"
    );
    assert!(
        cache_directory
            .join("fce/m2/fce.train.gold.bea19.m2")
            .is_file(),
        "the fetch landed in the cache the run was pointed at"
    );

    // This tarball is not the release the selection was drawn from, so the
    // eval tables skip with a reason rather than reporting other sentences.
    assert!(
        report.contains("- The eval set: the cached corpus has no sentence for fce-"),
        "a corpus that is not the release is a skipped table: {report}"
    );

    let (_, second) = bench(&cache_directory, &source, &shell_json);

    assert!(
        !second.contains("Its licence is at"),
        "a later run reads the cache and prints no notice: {second}"
    );
    assert!(
        !second.contains("non-commercial"),
        "a later run quotes no licence line: {second}"
    );
}
