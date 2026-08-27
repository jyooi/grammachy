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

/// Point every run at a dead address and forbid every unit start, so the binary
/// tests exercise argument handling only and never touch systemd. The settings
/// file points at a path that does not exist, so no run reads the developer's
/// real `~/.config/omarchy/shell.json` (spec section 7).
fn no_engine(command: &mut Command) -> &mut Command {
    command
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_LLAMA_START", "never")
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
        // `harper` runs in process and answers a result, so there is no code.
        let result = run(&["check", "--engine", engine], "Some text.");
        let value = envelope(&result);
        assert_ne!(
            value["error"]["code"], "bad_arguments",
            "--engine {engine} is valid"
        );
    }

    for thinking in ["on", "off"] {
        let result = run(&["check", "--thinking", thinking], "Some text.");
        assert_ne!(
            envelope(&result)["error"]["code"],
            "bad_arguments",
            "--thinking {thinking} is valid"
        );
    }

    let result = run(&["check", "--target", "en-US"], "Some text.");
    assert_ne!(envelope(&result)["error"]["code"], "bad_arguments");
}

/// Spec section 4 gives `--thinking` two values, so anything else is refused
/// before a Check runs.
#[test]
fn an_unknown_thinking_value_prints_bad_arguments() {
    let result = run(&["check", "--thinking", "maybe"], "Some text.");

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
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
fn a_silent_model_server_is_a_clean_engine_unavailable() {
    let settings = settings_file(
        "openai-silent.json",
        &format!(r#""openaiBaseUrl": "http://{}""#, silent_address()),
    );

    let result = run_with_settings(&["check", "--engine", "openai"], "He go home.", &settings);
    let value = envelope(&result);

    assert_eq!(result.status, 1);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["error"]["code"], "engine_unavailable");
}

#[test]
fn a_remote_openai_base_url_is_bad_arguments() {
    for base_url in ["https://api.openai.com/v1", "http://example.com:8080"] {
        let settings = settings_file(
            "openai-remote.json",
            &format!(r#""openaiBaseUrl": "{base_url}""#),
        );

        let result = run_with_settings(&["check", "--engine", "openai"], "He go home.", &settings);
        let value = envelope(&result);

        assert_eq!(result.status, 1);
        assert_eq!(value["contractVersion"], 1);
        assert_eq!(value["error"]["code"], "bad_arguments", "{base_url}");
        assert!(
            value["error"]["message"]
                .as_str()
                .expect("the message is a string")
                .contains("only localhost"),
            "the message names the rule: {value}"
        );
    }
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

/// One temporary home for a `setup` run: copies of both configuration files,
/// no compositor, and a models directory nothing writes to. The real files and
/// the real `~/.local/share/grammachy/` are never in reach.
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
        .env("GRAMMACHY_MODELS_DIR", home.join("models"))
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
    // The default engine is `languagetool`, so nothing is downloaded.
    assert!(!home.join("models").exists());

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

/// One `grammachy model` run against a scratch models directory. The unit stop
/// is forbidden, so no run here reaches the llama.cpp unit the live shell uses.
fn run_model(args: &[&str], models_directory: &Path) -> Run {
    let output = no_engine(&mut Command::new(env!("CARGO_BIN_EXE_grammachy")))
        .env("GRAMMACHY_MODELS_DIR", models_directory)
        .env("GRAMMACHY_LLAMA_STOP", "never")
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

fn models_home(name: &str) -> PathBuf {
    let directory = scratch_dir().join(format!("models-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the models directory is created");
    directory
}

/// Spec section 5.3: one JSON envelope on stdout, exit 0, one row per
/// catalogue model.
#[test]
fn model_list_prints_one_envelope_with_every_catalogue_row() {
    let directory = models_home("list");
    std::fs::write(directory.join("gemma-4-E4B-it-Q4_K_M.gguf"), b"whole")
        .expect("the ready file is written");

    let result = run_model(&["model", "list"], &directory);
    let value = envelope(&result);

    assert_eq!(result.status, 0);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["verb"], "list");
    assert_eq!(value["directory"], directory.display().to_string());
    assert!(value["freeBytes"].as_u64().is_some());
    assert_eq!(value["models"].as_array().unwrap().len(), 5);
    assert_eq!(value["models"][0]["state"], "ready");
    assert_eq!(value["models"][1]["state"], "absent");
    assert_eq!(value["models"][3]["name"], "qwen3.8-4b");
    assert_eq!(value["models"][3]["licence"], "Apache-2.0");
    assert_eq!(value["models"][4]["name"], "granite-4.2-3b");
    assert_eq!(value["models"][4]["licence"], "Apache-2.0");
}

#[test]
fn model_remove_deletes_the_file_and_answers_absent() {
    let directory = models_home("remove");
    let weights = directory.join("Phi-4-mini-instruct-Q4_K_M.gguf");
    std::fs::write(&weights, b"whole").expect("the ready file is written");

    let result = run_model(&["model", "remove", "phi-4-mini-instruct"], &directory);
    let value = envelope(&result);

    assert_eq!(result.status, 0);
    assert_eq!(value["verb"], "remove");
    assert_eq!(value["models"].as_array().unwrap().len(), 1);
    assert_eq!(value["models"][0]["name"], "phi-4-mini-instruct");
    assert_eq!(value["models"][0]["state"], "absent");
    assert!(!weights.exists());
}

/// A name the catalogue does not carry never reaches the network: it is one
/// error envelope and exit 1.
#[test]
fn model_download_of_an_unknown_name_is_bad_arguments() {
    let directory = models_home("unknown");

    let result = run_model(&["model", "download", "no-such-model"], &directory);
    let value = envelope(&result);

    assert_eq!(result.status, 1);
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["error"]["code"], "bad_arguments");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
}

#[test]
fn model_needs_a_verb() {
    let result = run_model(&["model"], &models_home("no-verb"));

    assert_eq!(result.status, 1);
    assert_eq!(envelope(&result)["error"]["code"], "bad_arguments");
}

// Spec section 7: `openrouterModel` has no built-in default, so the cloud
// engine with nothing stored refuses before it opens a socket. No test here
// may reach openrouter.ai, and this one never does: the refusal comes first.
#[test]
fn the_cloud_engine_with_no_model_prints_bad_arguments() {
    let settings = settings_file(
        "openrouter-blank-model.json",
        r#""engine": "openrouter", "openrouterModel": """#,
    );
    let result = run_with_settings(&["check"], "He go home.", &settings);
    let value = envelope(&result);

    assert_eq!(result.status, 1);
    assert_eq!(value["error"]["code"], "bad_arguments");
    assert_eq!(
        value["error"]["message"],
        "The cloud model is not set. Type one in Settings. (reason: no_model)"
    );
}
