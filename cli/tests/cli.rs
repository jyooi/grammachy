//! End to end runs of the binary: stdout carries one envelope, stderr carries logs.

use std::io::{ErrorKind, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

struct Run {
    status: i32,
    stdout: String,
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

/// Point every run at a dead address and forbid the unit start, so the binary
/// tests exercise argument handling only and never touch systemd. The settings
/// file points at a path that does not exist, so no run reads the developer's
/// real `~/.config/omarchy/shell.json` (spec section 7).
fn no_engine(command: &mut Command) -> &mut Command {
    command
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env(
            "GRAMMACHY_SHELL_JSON",
            scratch_dir().join("no-such-shell.json"),
        )
}

/// A directory of this test binary, removed with the target directory.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-settings");
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

/// Write one temporary `shell.json` holding the plugin entry.
fn settings_file(name: &str, entry_body: &str) -> PathBuf {
    let path = scratch_dir().join(name);
    let document = format!(
        r#"{{ "bar": {{ "layout": {{ "left": [], "center": [
            {{ "id": "io.github.jyooi.grammachy", {entry_body} }}
        ], "right": [] }} }}, "plugins": [] }}"#
    );
    std::fs::write(&path, document).expect("the settings file is written");
    path
}

/// Run the binary against one temporary settings file.
fn run_with_settings(args: &[&str], stdin: &str, settings: &Path) -> Run {
    let mut child = no_engine(&mut Command::new(env!("CARGO_BIN_EXE_grammachy")))
        .env("GRAMMACHY_SHELL_JSON", settings)
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

fn run(args: &[&str], stdin: &str) -> Run {
    let mut child = no_engine(&mut Command::new(env!("CARGO_BIN_EXE_grammachy")))
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

/// Clap may reject flags before it reads stdin. The child then closes the pipe.
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
fn empty_stdin_prints_the_empty_selection_envelope() {
    let result = run(&["check"], "");
    let value = envelope(&result);

    assert_eq!(result.status, 1);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["error"]["code"], "empty_selection");
}

#[test]
fn text_over_the_limit_prints_text_too_long() {
    let result = run(&["check"], &"a".repeat(5_001));

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "text_too_long");
}

#[test]
fn an_unknown_native_language_prints_bad_arguments() {
    let result = run(&["check", "--native", "xx"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

#[test]
fn an_unknown_engine_prints_bad_arguments() {
    let result = run(&["check", "--engine", "xx"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

#[test]
fn an_unknown_flag_prints_bad_arguments() {
    let result = run(&["check", "--depth", "style"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

#[test]
fn every_valid_flag_value_is_accepted() {
    for native in ["none", "zh", "ms", "es", "fr", "de", "pt", "ja"] {
        let result = run(&["check", "--native", native], "Some text.");
        let code = envelope(&result)["error"]["code"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(code, "bad_arguments", "--native {native} is valid");
    }

    for engine in ["languagetool", "openai", "harper"] {
        let result = run(&["check", "--engine", engine], "Some text.");
        let code = envelope(&result)["error"]["code"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(code, "bad_arguments", "--engine {engine} is valid");
    }

    let result = run(&["check", "--target", "en-US"], "Some text.");
    assert_ne!(envelope(&result)["error"]["code"], "bad_arguments");
}

#[test]
fn invalid_utf8_on_stdin_prints_bad_arguments() {
    let mut child = no_engine(&mut Command::new(env!("CARGO_BIN_EXE_grammachy")))
        .arg("check")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    write_stdin(&mut child, &[0xff, 0xfe, 0x00]);
    let output = child.wait_with_output().expect("the binary exits");

    let value: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(value["error"]["code"], "bad_arguments");
}

#[test]
fn an_engine_with_no_adapter_yet_is_a_clean_engine_unavailable() {
    for engine in ["harper", "openai"] {
        let result = run(&["check", "--engine", engine], "He go home.");
        let value = envelope(&result);

        assert_eq!(result.status, 1);
        assert_eq!(value["contractVersion"], 1);
        assert_eq!(
            value["error"]["code"], "engine_unavailable",
            "--engine {engine}"
        );
        assert!(
            value["error"]["message"]
                .as_str()
                .expect("the message is a string")
                .contains(engine),
            "the message names the engine: {value}"
        );
    }
}

#[test]
fn the_stored_engine_applies_when_no_flag_gives_one() {
    let settings = settings_file("stored-engine.json", r#""engine": "harper""#);
    let result = run_with_settings(&["check"], "He go home.", &settings);

    assert_eq!(envelope(&result)["error"]["code"], "engine_unavailable");
}

#[test]
fn the_engine_flag_wins_over_the_stored_engine() {
    let settings = settings_file("flag-over-file.json", r#""engine": "harper""#);
    let result = run_with_settings(
        &["check", "--engine", "languagetool"],
        "He go home.",
        &settings,
    );

    // The address is dead, so LanguageTool answers unavailable and names
    // itself. Harper would have named itself instead.
    let value = envelope(&result);
    assert_eq!(value["error"]["code"], "engine_unavailable");
    assert!(
        value["error"]["message"]
            .as_str()
            .expect("the message is a string")
            .contains("LanguageTool"),
        "the flag chose LanguageTool: {value}"
    );
}

#[test]
fn an_unknown_stored_engine_falls_back_to_the_default() {
    let settings = settings_file("unknown-engine.json", r#""engine": "gpt""#);
    let before = std::fs::read_to_string(&settings).expect("the file is readable");

    let result = run_with_settings(&["check"], "He go home.", &settings);

    let value = envelope(&result);
    assert_eq!(value["error"]["code"], "engine_unavailable");
    assert!(
        value["error"]["message"]
            .as_str()
            .expect("the message is a string")
            .contains("LanguageTool"),
        "the default engine ran: {value}"
    );
    assert_eq!(
        std::fs::read_to_string(&settings).expect("the file is readable"),
        before,
        "the CLI never rewrites the settings file"
    );
}

#[test]
fn a_missing_settings_file_is_fine() {
    let missing = scratch_dir().join("absent-shell.json");
    let _ = std::fs::remove_file(&missing);

    let result = run_with_settings(&["check"], "He go home.", &missing);

    assert_eq!(envelope(&result)["error"]["code"], "engine_unavailable");
    assert!(!missing.exists(), "the CLI never creates the settings file");
}
