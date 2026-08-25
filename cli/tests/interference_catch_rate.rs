//! The interference fixture, spec section 13.
//!
//! This prints the catch rate of each engine and never gates. A drop in a
//! number is a release decision recorded in `docs/benchmarks/`, not a red test.
//! The LanguageTool case skips when `127.0.0.1:8081` is silent. The Harper case
//! always runs, because that engine needs no server.
//!
//! Run it with `cargo test --test interference_catch_rate -- --nocapture` to
//! see the report, because cargo hides the output of a passing test.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use grammachy::args::{CheckOptions, EngineSlug};
use grammachy::engine;
use grammachy::envelope::Issue;
use serde::Deserialize;
use serde_json::Value;

const ADDRESS: &str = "127.0.0.1:8081";

/// One fixture sentence, the shape of `tests/fixtures/interference-30.json`.
#[derive(Debug, Deserialize)]
struct Sentence {
    id: String,
    native: String,
    text: String,
    expected_span: Option<Span>,
    /// Null for the correct sentences of the fixture.
    #[serde(default)]
    expected_fix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Span {
    start: usize,
    end: usize,
}

fn fixture() -> Vec<Sentence> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/interference-30.json"
    );
    let text = std::fs::read_to_string(path).expect("the fixture is readable");
    serde_json::from_str(&text).expect("the fixture is a sentence list")
}

fn server_answers() -> bool {
    let address: SocketAddr = ADDRESS.parse().expect("the address is valid");
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

fn check(text: &str, native: &str) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_grammachy"));
    command.arg("check").env(
        "GRAMMACHY_SHELL_JSON",
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-shell.json"),
    );
    if native != "none" {
        command.args(["--native", native]);
    }

    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes())
        .expect("stdin is written");

    let output = child.wait_with_output().expect("the binary exits");
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON object")
}

/// A sentence is caught when one Issue touches the span the fixture expects.
fn caught(issues: &[Value], expected: &Span) -> bool {
    issues.iter().any(|issue| {
        let start = issue["start"].as_u64().unwrap_or_default() as usize;
        let end = issue["end"].as_u64().unwrap_or_default() as usize;
        start < expected.end && expected.start < end
    })
}

/// One line per number, the same shape for every engine.
fn report(engine: &str, tally: &Tally) {
    let rate = 100.0 * tally.caught as f64 / tally.interference as f64;
    println!("{engine} on the interference fixture");
    println!(
        "  caught          {} of {} ({rate:.0} percent)",
        tally.caught, tally.interference
    );
    println!(
        "  exact fix       {} of {}",
        tally.exact_fixes, tally.interference
    );
    println!(
        "  false positives {} of {} correct sentences",
        tally.false_positives, tally.clean
    );
    println!("  missed          {}", tally.misses.join(", "));
}

#[derive(Debug, Default)]
struct Tally {
    interference: usize,
    caught: usize,
    exact_fixes: usize,
    clean: usize,
    false_positives: usize,
    misses: Vec<String>,
}

/// Harper runs in process, so the whole fixture costs one dictionary build
/// rather than one process per sentence. It ignores the Native language.
#[test]
fn the_fixture_prints_the_harper_catch_rate() {
    let harper = engine::resolve(EngineSlug::Harper).expect("this build has the harper adapter");
    let options = CheckOptions {
        engine: EngineSlug::Harper,
        ..CheckOptions::default()
    };

    let mut tally = Tally::default();
    for sentence in &fixture() {
        let issues: Vec<Issue> = harper
            .check(&sentence.text, &options)
            .unwrap_or_else(|failure| panic!("{} answered {failure:?}", sentence.id));

        match &sentence.expected_span {
            None => {
                tally.clean += 1;
                if !issues.is_empty() {
                    tally.false_positives += 1;
                }
            }
            Some(expected) => {
                tally.interference += 1;
                let hit = issues
                    .iter()
                    .any(|issue| issue.start < expected.end && expected.start < issue.end);
                if hit {
                    tally.caught += 1;
                    if issues
                        .iter()
                        .any(|issue| Some(issue.fix.as_str()) == sentence.expected_fix.as_deref())
                    {
                        tally.exact_fixes += 1;
                    }
                } else {
                    tally.misses.push(sentence.id.clone());
                }
            }
        }
    }

    report("Harper", &tally);

    // The fixture reports, it does not gate (spec section 13).
    assert!(
        tally.interference > 0,
        "the fixture holds interference sentences"
    );
}

#[test]
fn the_fixture_prints_the_catch_rate() {
    if !server_answers() {
        eprintln!("skipped: no LanguageTool on {ADDRESS}");
        return;
    }

    let sentences = fixture();
    let mut interference = 0;
    let mut caught_count = 0;
    let mut exact_fixes = 0;
    let mut clean = 0;
    let mut false_positives = 0;
    let mut misses: Vec<String> = Vec::new();

    for sentence in &sentences {
        let envelope = check(&sentence.text, &sentence.native);
        let issues = envelope["issues"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| panic!("{} answered {envelope}", sentence.id));

        match &sentence.expected_span {
            // A correct sentence: any Issue at all is a false positive.
            None => {
                clean += 1;
                if !issues.is_empty() {
                    false_positives += 1;
                }
            }
            Some(expected) => {
                interference += 1;
                if caught(&issues, expected) {
                    caught_count += 1;
                    let exact = issues
                        .iter()
                        .any(|issue| issue["fix"].as_str() == sentence.expected_fix.as_deref());
                    if exact {
                        exact_fixes += 1;
                    }
                } else {
                    misses.push(sentence.id.clone());
                }
            }
        }
    }

    report(
        "LanguageTool",
        &Tally {
            interference,
            caught: caught_count,
            exact_fixes,
            clean,
            false_positives,
            misses,
        },
    );

    // The fixture reports, it does not gate (spec section 13).
    assert!(interference > 0, "the fixture holds interference sentences");
}
