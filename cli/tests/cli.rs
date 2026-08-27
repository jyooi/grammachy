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

/// Point every run at a dead address, so the binary tests exercise argument
/// handling only and never touch systemd. No run reads the developer's real
/// `~/.config/omarchy/shell.json` either (spec section 7).
///
/// The engines directory is a path that does not exist, so no run reads the
/// components this machine has installed (spec section 5.4).
fn no_engine(command: &mut Command) -> &mut Command {
    command
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_ENGINES_DIR", scratch_dir().join("no-engines"))
        .env("GRAMMACHY_SHELL_JSON", silent_settings())
}

/// The settings file every run that names no other one reads: nothing at all,
/// so every key is the built-in default of spec section 7.
fn silent_settings() -> PathBuf {
    settings_file("silent-shell.json", "")
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
            {{ "id": "io.github.jyooi.grammachy"{}{entry_body} }}
        ], "right": [] }} }}, "plugins": [] }}"#,
        if entry_body.is_empty() { "" } else { ", " }
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
        // The default engine runs in process and answers a result, so there
        // is no code at all on this path (spec section 4, HUF-237).
        let result = run(&["check", "--native", native], "Some text.");
        assert_ne!(
            envelope(&result)["error"]["code"],
            "bad_arguments",
            "--native {native} is valid"
        );
    }

    for engine in ["languagetool", "harper"] {
        // `harper` runs in process and answers a result, so there is no code.
        let result = run(&["check", "--engine", engine], "Some text.");
        let value = envelope(&result);
        assert_ne!(
            value["error"]["code"], "bad_arguments",
            "--engine {engine} is valid"
        );
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

/// The in-process engine of spec section 4 needs no unit and no port, so it
/// answers real Issues in the run that forbids both.
#[test]
fn the_harper_engine_answers_issues_with_no_unit_running() {
    let result = run(&["check", "--engine", "harper"], "He go home.");
    let value = envelope(&result);

    assert_eq!(result.status, 0, "harper Check failed: {value}");
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["engine"], "harper");

    let issues = value["issues"].as_array().expect("issues is an array");
    assert!(!issues.is_empty(), "harper found nothing in {value}");
    assert_eq!(issues[0]["original"], "go");
    assert_eq!(issues[0]["fix"], "goes");
    assert_eq!(issues[0]["category"], "grammar");
}

#[test]
fn the_stored_engine_applies_when_no_flag_gives_one() {
    let settings = settings_file("stored-engine.json", r#""engine": "harper""#);
    let result = run_with_settings(&["check"], "He go home.", &settings);

    // The dead address would have made the default engine answer unavailable.
    assert_eq!(envelope(&result)["engine"], "harper");
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

/// The acceptance criterion of HUF-240: a stored engine of a removed slug
/// falls back to the default engine rather than erroring.
#[test]
fn a_stored_engine_of_a_removed_slug_falls_back_to_the_default() {
    for removed in ["openai", "openrouter"] {
        let settings = settings_file(
            &format!("removed-engine-{removed}.json"),
            &format!(r#""engine": "{removed}""#),
        );

        let result = run_with_settings(&["check"], "He go home.", &settings);

        let value = envelope(&result);
        assert_eq!(result.status, 0, "{removed}: {value}");
        assert_eq!(value["engine"], "harper", "{removed}: {value}");
    }
}

#[test]
fn an_unknown_stored_engine_falls_back_to_the_default() {
    let settings = settings_file("unknown-engine.json", r#""engine": "gpt""#);
    let before = std::fs::read_to_string(&settings).expect("the file is readable");

    let result = run_with_settings(&["check"], "He go home.", &settings);

    let value = envelope(&result);
    // HUF-237: the default is Harper, which is compiled into the binary, so
    // the fallback answers a result on a machine with nothing installed.
    assert_eq!(result.status, 0, "{value}");
    assert_eq!(value["engine"], "harper", "the default engine ran: {value}");
    assert_eq!(
        std::fs::read_to_string(&settings).expect("the file is readable"),
        before,
        "the CLI never rewrites the settings file"
    );
}

/// Unknown stored keys are ignored without error, so a settings file left over
/// from a removed feature (or a newer version of Grammachy) never breaks a run.
#[test]
fn unknown_stored_keys_are_ignored_without_error() {
    let settings = settings_file(
        "unknown-keys.json",
        r#""engine": "harper", "openaiModel": "qwen3.8-4b", "somethingElse": true"#,
    );

    let result = run_with_settings(&["check"], "He go home.", &settings);

    let value = envelope(&result);
    assert_eq!(result.status, 0, "{value}");
    assert_eq!(value["engine"], "harper", "{value}");
}

/// The acceptance criterion of HUF-237: a fresh install runs no download and
/// no pacman command, and the first Check answers.
#[test]
fn a_missing_settings_file_is_fine() {
    let missing = scratch_dir().join("absent-shell.json");
    let _ = std::fs::remove_file(&missing);

    let result = run_with_settings(&["check"], "He go home.", &missing);

    let value = envelope(&result);
    assert_eq!(result.status, 0, "{value}");
    assert_eq!(value["engine"], "harper", "{value}");
    assert!(value["issues"].is_array(), "{value}");
    assert!(!missing.exists(), "the CLI never creates the settings file");
}

/// One temporary home for a `setup` run: copies of both configuration files
/// and no compositor. The real files are never in reach.
fn setup_home(name: &str) -> PathBuf {
    let directory = scratch_dir().join(format!("home-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the temporary home is created");
    std::fs::write(
        directory.join("bindings.lua"),
        include_str!("fixtures/config/bindings.lua"),
    )
    .expect("the bindings copy is written");
    std::fs::write(
        directory.join("omarchy-menu.jsonc"),
        include_str!("fixtures/config/omarchy-menu.jsonc"),
    )
    .expect("the menu copy is written");
    directory
}

fn run_setup(args: &[&str], home: &Path) -> Run {
    let output = no_engine(&mut Command::new(env!("CARGO_BIN_EXE_grammachy")))
        .env("GRAMMACHY_BINDINGS_LUA", home.join("bindings.lua"))
        .env("GRAMMACHY_MENU_JSONC", home.join("omarchy-menu.jsonc"))
        .env("GRAMMACHY_HYPRCTL_RELOAD", "never")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary runs");

    Run {
        status: output.status.code().expect("the binary was not signalled"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
    }
}

#[test]
fn setup_writes_the_block_and_the_entry_and_remove_takes_them_out() {
    let home = setup_home("setup");
    let bindings = home.join("bindings.lua");
    let menu = home.join("omarchy-menu.jsonc");
    let before = (
        std::fs::read_to_string(&bindings).unwrap(),
        std::fs::read_to_string(&menu).unwrap(),
    );

    let installed = run_setup(&["setup"], &home);
    let value = envelope(&installed);

    assert_eq!(installed.status, 0);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["mode"], "install");
    assert!(std::fs::read_to_string(&bindings)
        .unwrap()
        .contains("-- grammachy begin"));
    assert!(std::fs::read_to_string(&menu)
        .unwrap()
        .contains("grammachy.compose"));

    let removed = run_setup(&["setup", "--remove"], &home);

    assert_eq!(removed.status, 0);
    assert_eq!(envelope(&removed)["mode"], "remove");
    assert_eq!(
        (
            std::fs::read_to_string(&bindings).unwrap(),
            std::fs::read_to_string(&menu).unwrap()
        ),
        before
    );
}
