//! End to end runs of the binary against a real local model server.
//!
//! Every case skips when `127.0.0.1:8080` is silent, so CI stays green on a
//! machine without llama.cpp and without the weights (spec section 13). To run
//! them, install the server and the model:
//!
//! ```text
//! sudo pacman -S llama-cpp ggml-cpu     # add ggml-vulkan for a GPU or an iGPU
//! grammachy setup                       # downloads the model (HUF-196)
//! ```
//!
//! The cold start case is `#[ignore]` because it stops and starts a systemd
//! unit. Run it by hand with
//! `cargo test --test openai_live -- --ignored --nocapture`.

use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

const ADDRESS: &str = "127.0.0.1:8080";

/// One Check at a time, because the server has one slot.
///
/// The unit runs llama-server with `--parallel 1`, so two Checks queue rather
/// than run. Cargo runs these cases side by side, and a queued Check spends
/// the wait inside the adapter's own 90 s timeout, which turns a slow model
/// into a failure that has nothing to do with the answer. Taking this lock
/// around the run is what keeps the cases measuring the model rather than
/// each other.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Whether a model server answers on the default base URL of spec section 7.
fn server_answers() -> bool {
    let address: SocketAddr = ADDRESS.parse().expect("the address is valid");
    TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
}

/// Run `grammachy check --engine openai` with `text` on stdin.
///
/// `base_url` overrides the Settings entry, which is how the remote-host case
/// reaches the adapter without touching the developer's real `shell.json`.
/// `name` keeps that file to one test, because tests run side by side.
fn check(name: &str, text: &str, base_url: Option<&str>) -> Value {
    let settings = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("openai-live-{name}.json"));
    match base_url {
        Some(url) => std::fs::write(
            &settings,
            format!(
                r#"{{ "plugins": [ {{ "id": "io.github.jyooi.grammachy", "openaiBaseUrl": "{url}" }} ] }}"#
            ),
        )
        .expect("the settings file is written"),
        None => {
            let _ = std::fs::remove_file(&settings);
        }
    }

    let _slot = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut child = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .args(["check", "--engine", "openai"])
        .env("GRAMMACHY_SHELL_JSON", &settings)
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
fn a_real_check_prints_issues() {
    if !server_answers() {
        eprintln!("skipped: no model server on {ADDRESS}");
        return;
    }

    let text = "Yesterday I go to the library with my friend.";
    let envelope = check("issues", text, None);

    assert_eq!(envelope["contractVersion"], 1);
    assert_eq!(envelope["engine"], "openai");
    assert!(envelope["elapsedMs"].is_number());
    assert!(
        !envelope["issues"].as_array().expect("issues").is_empty(),
        "the sentence has a mistake: {envelope}"
    );
    assert_spans_slice_back(text, &envelope);
    assert_sorted_and_disjoint(&envelope);
}

#[test]
fn a_correct_sentence_finds_nothing_it_can_anchor() {
    if !server_answers() {
        eprintln!("skipped: no model server on {ADDRESS}");
        return;
    }

    let text = "She walked to the library yesterday afternoon.";
    let envelope = check("clean", text, None);

    assert_eq!(envelope["contractVersion"], 1);
    assert_spans_slice_back(text, &envelope);
    assert_sorted_and_disjoint(&envelope);
}

#[test]
fn spans_survive_surrogate_pairs_and_crlf() {
    if !server_answers() {
        eprintln!("skipped: no model server on {ADDRESS}");
        return;
    }

    let text = "\u{1F600} He go to school yesterday.\r\n\r\nShe have three book.";
    let envelope = check("spans", text, None);

    assert_spans_slice_back(text, &envelope);
    assert_sorted_and_disjoint(&envelope);
}

/// This one needs no server: nothing is sent, so nothing has to answer.
#[test]
fn a_remote_base_url_is_refused() {
    let envelope = check("remote", "He go home.", Some("https://api.openai.com/v1"));

    assert_eq!(envelope["contractVersion"], 1);
    assert_eq!(envelope["error"]["code"], "bad_arguments");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("the message is a string")
            .contains("only localhost"),
        "the message names the rule: {envelope}"
    );
}

#[test]
#[ignore = "stops and starts the grammachy-llama unit"]
fn a_cold_start_brings_the_unit_up() {
    let _ = Command::new("systemctl")
        .args(["--user", "stop", "grammachy-llama"])
        .status();
    assert!(!server_answers(), "the unit is stopped before the Check");

    let text = "Yesterday I go to the library with my friend.";
    let envelope = check("cold", text, None);

    assert!(
        !envelope["issues"].as_array().expect("issues").is_empty(),
        "the cold Check found Issues: {envelope}"
    );
    assert_spans_slice_back(text, &envelope);

    let active = Command::new("systemctl")
        .args(["--user", "is-active", "grammachy-llama"])
        .output()
        .expect("systemctl runs");
    assert_eq!(String::from_utf8_lossy(&active.stdout).trim(), "active");

    // The second Check reuses what the first one started.
    assert!(!check("cold", text, None)["issues"]
        .as_array()
        .expect("issues")
        .is_empty());
}
