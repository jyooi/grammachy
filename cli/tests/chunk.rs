//! `grammachy chunk` end to end, plus the tiling property over varied Drafts.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use grammachy::args::EngineSlug;
use grammachy::chunk::{chunks_of, MAX_DRAFT_UTF16_UNITS};
use serde_json::Value;

/// The size limit of one Chunk on the default engine, in UTF-16 code units
/// (spec sections 4 and 5.2).
const MAX_CHUNK_UTF16_UNITS: usize = EngineSlug::Languagetool.check_limit_utf16();

/// The same limit on the local LLM engine, which reads less per Check.
const LOCAL_CHUNK_UTF16_UNITS: usize = EngineSlug::Openai.check_limit_utf16();

struct Run {
    status: i32,
    stdout: String,
}

/// A directory of this test binary, removed with the target directory.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("chunk-settings");
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

fn run(args: &[&str], stdin: &str) -> Run {
    // `chunk` resolves the engine the way `check` does, so it reads the
    // Settings entry. The path below does not exist, which keeps every run on
    // the built-in defaults and off the developer's real file (spec section 7).
    let mut child = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .env(
            "GRAMMACHY_SHELL_JSON",
            scratch_dir().join("no-such-shell.json"),
        )
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");

    write_stdin(&mut child, stdin.as_bytes());

    let output = child.wait_with_output().expect("the binary exits");
    Run {
        status: output.status.code().expect("the binary was not signalled"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
    }
}

fn write_stdin(child: &mut Child, stdin: &[u8]) {
    let mut pipe = child.stdin.take().expect("stdin is piped");
    match pipe.write_all(stdin) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::BrokenPipe => {}
        Err(error) => panic!("stdin is written: {error}"),
    }
}

fn envelope(run: &Run) -> Value {
    assert_eq!(
        run.stdout.lines().count(),
        1,
        "stdout holds exactly one line: {}",
        run.stdout
    );
    serde_json::from_str(run.stdout.trim()).expect("stdout is one JSON object")
}

#[test]
fn a_draft_prints_one_chunk_list() {
    let result = run(&["chunk"], "One paragraph.\n\nAnother paragraph.");
    let value = envelope(&result);

    assert_eq!(result.status, 0);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(
        value["chunks"],
        serde_json::json!([{"start": 0, "end": 34}])
    );
}

#[test]
fn empty_stdin_prints_the_empty_selection_envelope() {
    let result = run(&["chunk"], "");
    let value = envelope(&result);

    assert_eq!(result.status, 1);
    assert_eq!(value["error"]["code"], "empty_selection");
}

#[test]
fn a_draft_at_the_limit_succeeds_and_one_unit_over_is_text_too_long() {
    let at_limit = run(&["chunk"], &"a".repeat(MAX_DRAFT_UTF16_UNITS));
    assert_eq!(at_limit.status, 0);
    assert_eq!(
        envelope(&at_limit)["chunks"]
            .as_array()
            .expect("chunks is an array")
            .len(),
        10
    );

    let over = run(&["chunk"], &"a".repeat(MAX_DRAFT_UTF16_UNITS + 1));
    assert_eq!(over.status, 1);
    assert_eq!(envelope(&over)["error"]["code"], "text_too_long");
}

/// Spec section 4: the local engine packs to 2,000 units, so a 20,000-unit
/// Draft is ten Chunks rather than four.
#[test]
fn the_local_engine_packs_a_twenty_thousand_unit_draft_into_ten_chunks() {
    let draft = "a".repeat(20_000);

    let local = run(&["chunk", "--engine", "openai"], &draft);
    assert_eq!(local.status, 0);
    let chunks = envelope(&local)["chunks"]
        .as_array()
        .expect("chunks is an array")
        .clone();
    assert_eq!(chunks.len(), 10);
    for chunk in &chunks {
        let span = chunk["end"].as_u64().unwrap() - chunk["start"].as_u64().unwrap();
        assert!(
            span <= LOCAL_CHUNK_UTF16_UNITS as u64,
            "a Chunk fits one Check"
        );
    }

    for slug in ["languagetool", "harper"] {
        let wide = run(&["chunk", "--engine", slug], &draft);
        assert_eq!(wide.status, 0);
        assert_eq!(
            envelope(&wide)["chunks"]
                .as_array()
                .expect("chunks is an array")
                .len(),
            4,
            "{slug} packs to its own wider limit"
        );
    }
}

/// The flag is the caller's choice of limit, and the limits it names are the
/// ones the CLI enforces for a Check.
#[test]
fn the_engine_flag_packs_to_that_engine_limit() {
    for slug in [
        EngineSlug::Openai,
        EngineSlug::Languagetool,
        EngineSlug::Harper,
    ] {
        let limit = slug.check_limit_utf16();
        let draft = "a".repeat(limit + 1);
        let result = run(&["chunk", "--engine", slug.as_str()], &draft);

        assert_eq!(result.status, 0);
        assert_eq!(
            envelope(&result)["chunks"],
            serde_json::json!([
                {"start": 0, "end": limit},
                {"start": limit, "end": limit + 1},
            ]),
            "{} cuts at its own limit",
            slug.as_str()
        );
    }
}

#[test]
fn an_unknown_engine_prints_bad_arguments() {
    let result = run(&["chunk", "--engine", "gector"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

#[test]
fn an_unknown_flag_prints_bad_arguments() {
    let result = run(&["chunk", "--size", "100"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

/// The Chunks of every Draft tile it: contiguous, non-overlapping, covering
/// every UTF-16 unit, each under the Check limit, and never cut inside a
/// character.
#[test]
fn chunks_tile_every_draft() {
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;

    for case in 0..400 {
        let draft = random_draft(&mut seed, 1 + case * 40);
        assert_tiles(&draft);
    }
}

#[test]
fn chunks_tile_the_hard_edges() {
    for draft in [
        "a",
        "\u{1F600}",
        "a\n\nb",
        "a\r\n\r\nb",
        "\n\n\n",
        "  \n \n  a",
        "a\n\n",
        &"\u{1F600}".repeat(24_999),
        &format!("a{}", "\u{1F600}".repeat(24_000)),
        &"a\n\n".repeat(10_000),
        &"Hi. ".repeat(10_000),
        &format!("\"{}!\" ", "a".repeat(20_000)),
    ] {
        assert_tiles(draft);
    }
}

fn assert_tiles(draft: &str) {
    let units: Vec<u16> = draft.encode_utf16().collect();
    let chunks = chunks_of(draft, MAX_CHUNK_UTF16_UNITS);

    assert!(!chunks.is_empty(), "a non-empty Draft has Chunks");

    let mut expected_start = 0;
    for chunk in &chunks {
        assert_eq!(chunk.start, expected_start, "Chunks are contiguous");
        assert!(chunk.end > chunk.start, "a Chunk is never empty");
        assert!(
            chunk.end - chunk.start <= MAX_CHUNK_UTF16_UNITS,
            "a Chunk fits one Check"
        );
        String::from_utf16(&units[chunk.start..chunk.end])
            .expect("a Chunk never cuts inside a character");
        expected_start = chunk.end;
    }
    assert_eq!(expected_start, units.len(), "the Chunks cover the Draft");
}

/// A Draft of at most `budget` UTF-16 units, built from fragments that stress
/// the splitter: astral characters, CRLF, blank lines, quotes, and long runs.
fn random_draft(seed: &mut u64, budget: usize) -> String {
    const FRAGMENTS: [&str; 14] = [
        "a",
        "word ",
        "A short sentence. ",
        "Is it? ",
        "\"Quoted!\" ",
        "'Quoted.' ",
        "\u{201C}Curly.\u{201D} ",
        "\u{1F600}",
        "\u{1F600}\u{1F600} ",
        "\n",
        "\r\n",
        "\n\n",
        "\r\n\r\n",
        "  \t",
    ];

    let mut draft = String::new();
    let mut units = 0;
    while units < budget {
        let fragment = FRAGMENTS[(next(seed) % FRAGMENTS.len() as u64) as usize];
        let length: usize = fragment.chars().map(char::len_utf16).sum();
        if units + length > budget {
            // The budget is nearly spent, so fill it with single units.
            draft.push('a');
            units += 1;
            continue;
        }

        let repeat = 1 + (next(seed) % 400) as usize;
        for _ in 0..repeat {
            if units + length > budget {
                break;
            }
            draft.push_str(fragment);
            units += length;
        }
    }

    if draft.trim().is_empty() {
        draft.push('a');
    }
    draft
}

/// xorshift64, so the cases are varied but the run is the same every time.
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
