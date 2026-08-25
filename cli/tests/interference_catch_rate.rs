//! The interference fixture, spec section 13.
//!
//! This prints the catch rate of the default engine and never gates. A drop in
//! the number is a release decision recorded in `docs/benchmarks/`, not a red
//! test. The case skips when `127.0.0.1:8081` is silent.
//!
//! Run it with `cargo test --test interference_catch_rate -- --nocapture` to
//! see the report, because cargo hides the output of a passing test.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

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
    command.arg("check");
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

    let rate = 100.0 * caught_count as f64 / interference as f64;
    println!("LanguageTool on the interference fixture");
    println!("  caught          {caught_count} of {interference} ({rate:.0} percent)");
    println!("  exact fix       {exact_fixes} of {interference}");
    println!("  false positives {false_positives} of {clean} correct sentences");
    println!("  missed          {}", misses.join(", "));

    // The fixture reports, it does not gate (spec section 13).
    assert!(interference > 0, "the fixture holds interference sentences");
}
