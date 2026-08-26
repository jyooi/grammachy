//! The Models list of spec section 5.3 makes promises no node test can reach.
//!
//! `ui/models.test.js` runs the whole route against a stub binary, but it drives
//! the verbs itself: the shared functions it calls are only the right ones if
//! `Overlay.qml` calls the same ones. The overlay cannot be instantiated outside
//! the shell's plugin loader, so this test reads the file the shell ships and
//! holds those calls in place, the way `overlay_chunks.rs` holds the Chunk loop.
//!
//! No test here downloads a model, writes the models directory, or stops a unit.

use grammachy::envelope::ErrorCode;

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The body of one `function <name>(...)` in a QML or JavaScript file.
fn function_body(source: &str, name: &str) -> String {
    let needle = format!("function {name}(");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("the source declares {name}"));
    let open = source[start..]
        .find('{')
        .unwrap_or_else(|| panic!("{name} has a body"))
        + start;

    let mut depth = 0usize;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return source[open..=open + offset].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("{name} has a closing brace")
}

/// The three verbs the CLI answers are the three the overlay runs, spelled the
/// same way. A fourth spelling would be a `bad_arguments` envelope at run time.
#[test]
fn the_overlay_runs_the_three_verbs_of_the_subcommand() {
    let source = read("Overlay.qml");

    assert!(
        function_body(&source, "modelCommand").contains("\"model\""),
        "every verb goes through the model subcommand"
    );
    assert!(
        function_body(&source, "refreshModels").contains("[\"list\"]"),
        "the list is the list verb"
    );
    assert!(
        function_body(&source, "downloadModel").contains("[\"download\", name]"),
        "Download runs the download verb on the row's own name"
    );
    for remover in ["removeModel", "confirmRemoveModel"] {
        assert!(
            function_body(&source, remover).contains("[\"remove\", name]"),
            "{remover} runs the remove verb on the row's own name"
        );
    }
}

/// The acceptance criterion of the progress bar: the CLI prints nothing while
/// curl runs, so the only progress there is comes from polling `model list`.
#[test]
fn a_running_download_is_polled_once_a_second() {
    let source = read("Overlay.qml");

    let timer = source
        .split_once("id: modelPoll")
        .expect("the overlay declares the poll timer")
        .1;
    let body = timer.split_once('}').expect("the timer is closed").0;

    assert!(
        body.contains("interval: 1000"),
        "the poll is one second: {body}"
    );
    assert!(body.contains("repeat: true"), "the poll repeats: {body}");
    assert!(
        body.contains("root.refreshModels()"),
        "the poll runs the list verb: {body}"
    );

    // It runs only while a download does.
    assert!(
        function_body(&source, "downloadModel").contains("modelPoll.start()"),
        "a download starts the poll"
    );
    assert!(
        function_body(&source, "finishModelAction").contains("modelPoll.stop()"),
        "the verb ending stops the poll"
    );
}

/// Spec section 5.3: one download at a time, and Cancel is a signal rather than
/// a kill, because the CLI is what turns a SIGTERM into a kept `.part` file.
#[test]
fn one_download_runs_at_a_time_and_cancel_signals_it() {
    let source = read("Overlay.qml");
    let download = function_body(&source, "downloadModel");

    assert!(
        download.contains("root.modelBusy.length > 0") && download.contains("return"),
        "a second Download while one is in flight is a no-op: {download}"
    );
    assert!(
        download.contains("root.modelBusy = name"),
        "the row in flight is named, which is what turns the other rows off: {download}"
    );

    let cancel = function_body(&source, "cancelModelDownload");
    assert!(
        cancel.contains("modelActionProcess.signal(15)"),
        "Cancel sends SIGTERM: {cancel}"
    );
    for killed in ["running = false", "root.models = []"] {
        assert!(
            !cancel.contains(killed),
            "Cancel must not reach {killed}: {cancel}"
        );
    }
}

/// Spec section 5.3: closing the overlay does not cancel a download, so a
/// summon leaves the list and the process in flight alone.
#[test]
fn a_summon_never_touches_a_download_in_flight() {
    let reset = function_body(&read("Overlay.qml"), "resetRun");

    for kept in [
        "root.models = []",
        "root.modelBusy = \"\"",
        "modelActionProcess.running = false",
        "modelPoll.stop()",
    ] {
        assert!(
            !reset.contains(kept),
            "a summon must not reach {kept}: {reset}"
        );
    }
}

/// Spec section 7: Use is the setting and nothing else, and Remove never
/// touches it, so the two cannot be confused.
#[test]
fn use_writes_the_setting_and_remove_leaves_it_alone() {
    let source = read("Overlay.qml");
    let use_model = function_body(&source, "useModel");

    assert!(
        use_model.contains("root.persistSetting(\"openaiModel\", name)"),
        "Use writes the openaiModel setting: {use_model}"
    );

    for remover in ["removeModel", "confirmRemoveModel"] {
        let body = function_body(&source, remover);
        assert!(
            !body.contains("persistSetting"),
            "{remover} must not touch the setting: {body}"
        );
    }
}

/// Spec section 7: removing the model a Check would run on asks once, and that
/// confirm is a phase with a key mode of its own.
#[test]
fn removing_the_model_in_use_asks_once_through_its_own_phase() {
    let source = read("Overlay.qml");
    let remove = function_body(&source, "removeModel");

    assert!(
        remove.contains("root.setting(\"openaiModel\")"),
        "the question is asked only about the model the setting names: {remove}"
    );
    assert!(
        remove.contains("root.askRemoveModel(name)"),
        "the model in use goes through the confirm: {remove}"
    );
    assert!(
        function_body(&source, "askRemoveModel").contains("root.phase = \"confirmModel\""),
        "the confirm is a phase of its own"
    );

    // A new phase that is not named in the key map silently inherits the review
    // keys, which would make Enter accept an Issue that is not on screen.
    let key_mode = function_body(&source, "keyMode");
    assert!(
        key_mode.contains("root.phase === \"confirmModel\"")
            && key_mode.contains("Keymap.MODE_MODEL_CONFIRM"),
        "the confirm phase has its own key mode: {key_mode}"
    );

    let keymap = read("ui/keymap.js");
    assert!(
        keymap.contains("MODE_MODEL_CONFIRM"),
        "ui/keymap.js knows the mode"
    );
    let handler = function_body(&source, "handleKey");
    assert!(
        handler.contains("Keymap.REMOVE_MODEL") && handler.contains("Keymap.KEEP_MODEL"),
        "both answers to the question are routed: {handler}"
    );
}

/// The two codes only `grammachy model` can answer have to reach a card, or a
/// cancelled download would read as the companion tool being out of date.
#[test]
fn the_shell_knows_the_two_codes_only_the_model_verbs_answer() {
    let source = read("ui/models.js");

    for code in [ErrorCode::Cancelled, ErrorCode::DownloadFailed] {
        let snake = serde_json::to_string(&code).expect("an error code serialises");
        assert!(
            source.contains(&snake),
            "ui/models.js has a note for {snake}"
        );
    }
}

/// The three states of spec section 5.3 are the three the shell draws.
#[test]
fn the_shell_knows_every_state_a_row_can_be_in() {
    let source = read("ui/models.js");
    let states = read("cli/src/model/envelope.rs");
    let block = states
        .split_once("pub enum State {")
        .expect("envelope.rs declares State")
        .1
        .split_once('}')
        .expect("the State enum is closed")
        .0;

    let names: Vec<String> = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with("///"))
        .map(|line| line.trim_end_matches(',').to_ascii_lowercase())
        .collect();

    assert_eq!(names, ["absent", "partial", "ready"]);
    for state in names {
        assert!(
            source.contains(&format!("\"{state}\"")),
            "ui/models.js knows the {state} state"
        );
    }
}

/// The Models list is drawn only for the engine that has weights, spec
/// section 7, and it is read only when it is drawn.
#[test]
fn the_list_belongs_to_the_local_llm_engine_alone() {
    let source = read("Overlay.qml");

    assert!(
        source.contains("readonly property bool showsModels: root.settingsOpen")
            && source.contains("String(root.setting(\"engine\")) === \"openai\""),
        "the list is the Local LLM engine's alone"
    );
    assert!(
        source.contains("onShowsModelsChanged: if (root.showsModels) root.refreshModels()"),
        "opening it is what reads it"
    );
    assert!(
        read("ui/SettingsView.qml").contains("visible: root.showsOpenai"),
        "the block it sits in is shown for the openai engine only"
    );
}
