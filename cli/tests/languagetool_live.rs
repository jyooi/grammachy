//! End to end runs against a real LanguageTool server.
//!
//! Every case skips when `127.0.0.1:8081` is silent, so CI stays green on a
//! machine without the `languagetool` package (spec section 13).
//!
//! The cold start case is `#[ignore]` because it stops and starts a systemd
//! unit. Run it by hand with
//! `cargo test --test languagetool_live -- --ignored --nocapture`.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

const ADDRESS: &str = "127.0.0.1:8081";

/// Whether a LanguageTool server answers on the port the spec fixes.
fn server_answers() -> bool {
    let address: SocketAddr = ADDRESS.parse().expect("the address is valid");
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

/// Run the binary with `text` on stdin and answer the parsed envelope.
fn check(text: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .arg("check")
        // The default engine is `harper` since HUF-237, so this engine is now
        // named rather than assumed.
        .args(["--engine", "languagetool"])
        .env(
            "GRAMMACHY_SHELL_JSON",
            Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such-shell.json"),
        )
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

/// The guarantee the shell relies on: `text.slice(start, end) === original`.
fn assert_spans_slice_back(text: &str, envelope: &Value) {
    let units: Vec<u16> = text.encode_utf16().collect();
    for issue in envelope["issues"].as_array().expect("issues is an array") {
        let start = issue["start"].as_u64().expect("start is a number") as usize;
        let end = issue["end"].as_u64().expect("end is a number") as usize;
        let slice = String::from_utf16(&units[start..end]).expect("the span is text");

        assert_eq!(slice, issue["original"], "span {start}..{end}");
        assert_ne!(issue["fix"], issue["original"], "the fix is a change");
        assert!(
            issue["category"] == "grammar" || issue["category"] == "spelling",
            "category is grammar or spelling: {}",
            issue["category"]
        );
    }
}

fn assert_sorted_and_disjoint(envelope: &Value) {
    let mut previous_end = 0;
    for issue in envelope["issues"].as_array().expect("issues is an array") {
        let start = issue["start"].as_u64().expect("start is a number") as usize;
        let end = issue["end"].as_u64().expect("end is a number") as usize;

        assert!(start >= previous_end, "Issues are sorted and disjoint");
        previous_end = end;
    }
}

#[test]
fn a_real_check_finds_the_tense_mistake() {
    if !server_answers() {
        eprintln!("skipped: no LanguageTool on {ADDRESS}");
        return;
    }

    let text = "He go to school yesterday.";
    let envelope = check(text);

    assert_eq!(envelope["contractVersion"], 1);
    assert_eq!(envelope["engine"], "languagetool");
    assert!(envelope["elapsedMs"].is_number());
    assert!(
        !envelope["issues"].as_array().expect("issues").is_empty(),
        "the sentence has a mistake: {envelope}"
    );
    assert_spans_slice_back(text, &envelope);
    assert_sorted_and_disjoint(&envelope);
}

#[test]
fn a_correct_sentence_finds_nothing() {
    if !server_answers() {
        eprintln!("skipped: no LanguageTool on {ADDRESS}");
        return;
    }

    let envelope = check("She walked to the library yesterday afternoon.");

    assert_eq!(envelope["issues"], serde_json::json!([]));
}

#[test]
fn spans_survive_surrogate_pairs_and_crlf() {
    if !server_answers() {
        eprintln!("skipped: no LanguageTool on {ADDRESS}");
        return;
    }

    let text = "\u{1F600} He go to school yesterday.\r\n\r\nShe have three book.";
    let envelope = check(text);

    assert_spans_slice_back(text, &envelope);
    assert_sorted_and_disjoint(&envelope);
}

#[test]
fn the_second_check_reuses_the_running_unit() {
    if !server_answers() {
        eprintln!("skipped: no LanguageTool on {ADDRESS}");
        return;
    }

    let first = check("He go to school yesterday.");
    let second = check("He go to school yesterday.");

    assert_eq!(first["issues"], second["issues"]);
    // A reused server answers well inside the 10 s Check timeout.
    let elapsed = second["elapsedMs"].as_u64().expect("elapsedMs is a number");
    assert!(elapsed < 10_000, "the reused unit answered in {elapsed} ms");
}

#[test]
#[ignore = "stops and starts the grammachy-languagetool unit"]
fn a_cold_start_brings_the_unit_up() {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "grammachy-languagetool"])
        .status();
    assert!(!server_answers(), "the unit is stopped before the Check");

    let text = "He go to school yesterday.";
    let envelope = check(text);

    assert!(
        !envelope["issues"].as_array().expect("issues").is_empty(),
        "the cold Check found Issues: {envelope}"
    );
    assert_spans_slice_back(text, &envelope);

    let active = Command::new("systemctl")
        .args(["--user", "is-active", "grammachy-languagetool"])
        .output()
        .expect("systemctl runs");
    assert_eq!(String::from_utf8_lossy(&active.stdout).trim(), "active");

    // The second Check reuses what the first one started.
    assert!(!check(text)["issues"].as_array().expect("issues").is_empty());
}
