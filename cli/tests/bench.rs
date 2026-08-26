//! End to end runs of `grammachy bench`, spec section 13.1.
//!
//! No case touches systemd and no case reaches a real server. LanguageTool is
//! pointed at a dead address, unit starts are forbidden, and the Models table
//! runs against a stub chat endpoint in this test binary. Harper runs for real,
//! because it needs no server.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

/// A chat completion that finds the plural mistake of the fixture and nothing
/// else, so every sentence gets a well-formed answer.
const ANSWER: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant",
    "content":"[{\"original\": \"three book\", \"fix\": \"three books\", \"reason\": \"Plural after a number.\", \"category\": \"grammar\"}]"}}]}"#;

struct Run {
    status: i32,
    stdout: String,
}

/// A stub chat endpoint on a port the operating system picks.
fn stub_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            read_request(&mut stream);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ANSWER}",
                ANSWER.len()
            );
            let _ = stream.flush();
        }
    });

    address
}

/// Read one whole request, headers and body.
///
/// The body must be drained too. A stub that answers and closes on an unread
/// body resets the connection, and the adapter then reports the server as
/// unreachable rather than reading the answer.
fn read_request(stream: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    let _ = stream.read_exact(&mut body);
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

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench");
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

/// Write one temporary `shell.json`, so no run reads the real one.
///
/// An entry that names no `openaiBaseUrl` gets a silent one. The default is a
/// fixed loopback port, so a machine that already runs llama.cpp there would
/// otherwise answer a run that is meant to find nothing.
fn settings_file(name: &str, entry_body: &str) -> PathBuf {
    let path = scratch_dir().join(name);
    let entry = if entry_body.contains("openaiBaseUrl") {
        entry_body.to_string()
    } else {
        format!(
            r#""openaiBaseUrl": "http://{}", {entry_body}"#,
            silent_address()
        )
    };
    let document = format!(
        r#"{{ "bar": {{ "layout": {{ "left": [], "center": [
            {{ "id": "io.github.jyooi.grammachy", {entry} }}
        ], "right": [] }} }}, "plugins": [] }}"#
    );
    std::fs::write(&path, document).expect("the settings file is written");
    path
}

/// Run `grammachy bench` with the seams that keep the suite off this machine.
fn bench(settings: &Path, arguments: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .arg("bench")
        .args(arguments)
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_LLAMA_START", "never")
        .env("GRAMMACHY_SHELL_JSON", settings)
        .output()
        .expect("the binary runs");

    Run {
        status: output.status.code().expect("the binary exits with a code"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
    }
}

/// The first row of one engine or model: the Engines row, or the Quality row
/// of a model.
fn row<'a>(report: &'a str, name: &str) -> &'a str {
    nth_row(report, name, 0)
}

/// The Cost row of one model, the second table it appears in.
fn cost_row<'a>(report: &'a str, name: &str) -> &'a str {
    nth_row(report, name, 1)
}

fn nth_row<'a>(report: &'a str, name: &str, index: usize) -> &'a str {
    let head = format!("| `{name}` |");
    report
        .lines()
        .filter(|line| line.starts_with(&head))
        .nth(index)
        .unwrap_or_else(|| panic!("the report holds row {index} for {name}:\n{report}"))
}

#[test]
fn a_run_prints_the_engines_table_and_skips_what_it_cannot_reach() {
    let settings = settings_file("engines.json", r#""engine": "languagetool""#);

    let run = bench(&settings, &[]);

    assert_eq!(run.status, 0, "a skipped engine is not a failure");
    assert!(
        run.stdout.starts_with("# Grammachy benchmark "),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("## Engines"), "{}", run.stdout);

    // Harper needs no server, so it is measured on every machine.
    let harper = row(&run.stdout, "harper");
    assert!(harper.contains(" of 30 ("), "{harper}");
    assert!(harper.contains(" of 10 |"), "{harper}");
    assert!(harper.contains(" ms |"), "{harper}");

    // The two server engines have nothing to talk to in this test.
    assert_eq!(
        row(&run.stdout, "languagetool"),
        "| `languagetool` | skipped | skipped | skipped | skipped |"
    );
    assert!(
        run.stdout
            .contains("- Engine `languagetool`: LanguageTool did not answer"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("- Engine `openai`: No model server answered"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_run_names_the_machine_tier_and_the_regression_rule() {
    let settings = settings_file("tier.json", r#""engine": "harper""#);

    let run = bench(&settings, &[]);

    assert!(run.stdout.contains(" tier, "), "{}", run.stdout);
    assert!(run.stdout.contains("## Regression rule"), "{}", run.stdout);
    assert!(
        run.stdout
            .contains("must not drop the catch rate of the default engine, `languagetool`"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_named_model_is_evaluated_against_the_endpoint_of_the_settings() {
    let settings = settings_file(
        "models.json",
        &format!(r#""openaiBaseUrl": "http://{}""#, stub_server()),
    );

    let run = bench(
        &settings,
        &["--engine", "openai", "--model", "qwen2.5-7b-instruct"],
    );

    assert_eq!(run.status, 0);
    assert!(run.stdout.contains("## Models"), "{}", run.stdout);

    // The stub answers the plural mistake of zh-02 for every sentence, so the
    // row carries one catch and a false positive on every correct sentence.
    let model = row(&run.stdout, "qwen2.5-7b-instruct");
    assert!(model.contains("| 1 of 30 (3.3%) |"), "{model}");
    assert!(model.contains("| 0 of 10 |"), "{model}");
    let cost = cost_row(&run.stdout, "qwen2.5-7b-instruct");
    assert!(
        cost.contains("| 0.00 (local) | Apache-2.0 | recommended |"),
        "{cost}"
    );
    assert!(
        run.stdout
            .contains("grammachy bench --engine openai --model qwen2.5-7b-instruct"),
        "the file names the command that produced it:\n{}",
        run.stdout
    );
}

#[test]
fn a_model_with_non_commercial_weights_is_shown_but_never_recommended() {
    let settings = settings_file(
        "non-commercial.json",
        &format!(r#""openaiBaseUrl": "http://{}""#, stub_server()),
    );

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen2.5-3b-instruct",
            "--model",
            "qwen2.5-7b-instruct",
        ],
    );

    assert_eq!(run.status, 0);
    let restricted = cost_row(&run.stdout, "qwen2.5-3b-instruct");
    assert!(
        restricted.contains("| Qwen Research License | never, the weights are non-commercial |"),
        "{restricted}"
    );
    // It is still a full row: the table shows it for reference.
    assert!(
        row(&run.stdout, "qwen2.5-3b-instruct").contains("| 1 of 30 (3.3%) |"),
        "{}",
        run.stdout
    );
    assert!(
        cost_row(&run.stdout, "qwen2.5-7b-instruct").contains("| recommended |"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_model_on_an_engine_that_takes_none_is_a_bad_arguments_envelope() {
    let settings = settings_file("bad.json", r#""engine": "harper""#);

    let run = bench(
        &settings,
        &["--engine", "harper", "--model", "qwen2.5-7b-instruct"],
    );

    assert_eq!(run.status, 1);
    let envelope: serde_json::Value =
        serde_json::from_str(&run.stdout).expect("a failure prints one JSON envelope");
    assert_eq!(envelope["error"]["code"], "bad_arguments");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("the envelope carries a message")
            .contains("Only the openai and openrouter engines"),
        "{envelope}"
    );
}

#[test]
fn an_unreachable_model_still_carries_its_license_and_recommendation() {
    let settings = settings_file(
        "unreachable.json",
        &format!(r#""openaiBaseUrl": "http://{}""#, silent_address()),
    );

    let run = bench(
        &settings,
        &["--engine", "openai", "--model", "qwen2.5-3b-instruct"],
    );

    assert_eq!(
        run.status, 0,
        "an unreachable model is skipped, not an error"
    );
    assert_eq!(
        cost_row(&run.stdout, "qwen2.5-3b-instruct"),
        "| `qwen2.5-3b-instruct` | skipped | skipped | skipped | skipped | Qwen Research License | never, the weights are non-commercial |"
    );
    assert!(
        run.stdout
            .contains("- Model `qwen2.5-3b-instruct`: No model server answered"),
        "{}",
        run.stdout
    );
}

/// The release habit of spec section 13.1: one file per version, produced by
/// the command above and committed as it was printed.
#[test]
fn the_committed_benchmark_file_of_this_version_is_the_output_of_the_command() {
    let version = env!("CARGO_PKG_VERSION");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/benchmarks")
        .join(format!("{version}.md"));

    let file = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is committed for version {version}: {error}",
            path.display()
        )
    });

    assert!(
        file.starts_with(&format!("# Grammachy benchmark {version}\n")),
        "the file is the whole output of the command, title first"
    );
    for heading in [
        "## Engines",
        "## Models",
        "## Skipped",
        "## Regression rule",
    ] {
        assert!(file.contains(heading), "the file holds {heading}");
    }
    assert!(file.contains(" tier, "), "the file names the machine tier");
    // The number itself is a release decision, not a gate (spec section 13),
    // so this only holds that the default engine was measured rather than
    // skipped in the file a release ships with.
    let default_engine = file
        .lines()
        .find(|line| line.starts_with("| `languagetool` |"))
        .expect("the file holds the row of the default engine");
    assert!(
        default_engine.contains(" of 30 ("),
        "the default engine is measured, not skipped: {default_engine}"
    );
}
