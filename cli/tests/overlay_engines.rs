//! The Engines list of spec section 5.4 makes promises no node test can reach.
//!
//! `ui/engines.test.js` runs the whole route against a stub binary, but it
//! drives the verbs itself: the shared functions it calls are only the right
//! ones if `Overlay.qml` calls the same ones. The overlay cannot be
//! instantiated outside the shell's plugin loader, so this test reads the files
//! the shell ships and holds those calls in place, the way `overlay_models.rs`
//! holds the Models list.
//!
//! It also keeps the two halves of HUF-237 in step: the engine slug the Rust
//! catalogue carries, the built-in engine both sides fall back to, and the
//! state words the `doctor` report hands the Settings view.
//!
//! No test here installs a component, writes the engines directory, or stops a
//! unit.

use grammachy::args::{CheckOptions, EngineSlug};
use grammachy::doctor::report;
use grammachy::engines::install;
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
        function_body(&source, "engineCommand").contains("\"engine\""),
        "every verb goes through the engine subcommand"
    );
    assert!(
        function_body(&source, "refreshEngines").contains("[\"list\"]"),
        "the list is the list verb"
    );
    assert!(
        function_body(&source, "installEngine").contains("[\"install\", slug]"),
        "Install is the install verb, named by slug"
    );
    assert!(
        function_body(&source, "removeEngine").contains("[\"remove\", slug]"),
        "Remove is the remove verb, named by slug"
    );
    assert!(
        function_body(&source, "confirmRemoveEngine").contains("[\"remove\", slug]"),
        "the confirmed Remove is the same verb"
    );
}

/// Spec section 5.4: Cancel is a SIGTERM, which the CLI turns into a kept
/// `.part` file and the `cancelled` code. Ending the process instead would
/// orphan curl, which would carry on writing a file nobody is waiting for.
#[test]
fn cancel_signals_the_process_rather_than_ending_it() {
    let body = function_body(&read("Overlay.qml"), "cancelEngineInstall");

    assert!(body.contains("signal(15)"), "{body}");
    assert!(
        !body.contains("running = false"),
        "ending the process would orphan curl: {body}"
    );
}

/// The acceptance criterion: removing the selected engine leaves the Settings
/// consistent. The overlay writes the built-in engine and nothing else.
#[test]
fn removing_the_selected_engine_writes_the_built_in_one() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "fallBackFromRemovedEngine");

    assert!(
        body.contains("Settings.engineAfterRemoval"),
        "the rule lives in ui/settings.js, where a node test owns it: {body}"
    );
    assert!(
        body.contains("persistSetting(\"engine\""),
        "the fallback is stored, so the next Check reads it: {body}"
    );
    assert!(
        body.contains("EnginesJs.isAvailable"),
        "a component the pacman package still supplies moves no setting: {body}"
    );
    assert!(
        function_body(&source, "onEngineActionOutput").contains("fallBackFromRemovedEngine"),
        "the fallback runs on the answer of the remove verb"
    );
}

/// Spec section 7: a question that is off the screen must never still be
/// answerable, the rule the Models Remove confirm already keeps.
#[test]
fn closing_settings_drops_an_open_remove_confirm() {
    let source = read("Overlay.qml");

    assert!(
        source.contains("else if (root.phase === \"confirmEngine\") root.closeEngineConfirm()"),
        "closing Settings answers the open question with Keep"
    );
    assert!(
        source.contains("if (root.phase === \"confirmEngine\") return Keymap.MODE_ENGINE_CONFIRM"),
        "the confirm is a phase with a key mode of its own"
    );
    assert!(
        function_body(&source, "removeEngine").contains("askRemoveEngine"),
        "removing the engine a Check would run on asks once"
    );
}

/// One verb of `grammachy engine` runs at a time, so a second press while one
/// is in flight is a no-op rather than a second transfer. The buttons that
/// would reach these functions are drawn disabled from the same fact.
#[test]
fn one_verb_runs_at_a_time() {
    let source = read("Overlay.qml");

    for name in ["installEngine", "removeEngine"] {
        assert!(
            function_body(&source, name).contains("if (root.enginesBusy) return"),
            "{name} is guarded by the one fact every button reads"
        );
    }
    assert!(
        source.contains("readonly property bool enginesBusy: engineActionProcess.running")
            && source.contains("|| root.engineConfirm.length > 0"),
        "an open confirm counts, because the verb it is waiting on is not run yet"
    );
}

/// Closing the overlay must never cancel an install that is still running: it
/// takes minutes, and a summon that threw it away would be worse than useless.
#[test]
fn closing_the_overlay_leaves_a_running_install_alone() {
    let body = function_body(&read("Overlay.qml"), "resetRun");

    for untouched in ["engines", "engineBusy", "engineActionProcess", "enginePoll"] {
        assert!(
            !body.contains(untouched),
            "resetRun must leave {untouched} alone: {body}"
        );
    }
    // The confirm is a question about a card that is gone, so it goes with it.
    assert!(
        body.contains("root.engineConfirm = \"\""),
        "a summon drops an open Remove confirm: {body}"
    );
}

/// The Settings view draws the list whatever engine is selected, because the
/// whole point is to add one the dropdown cannot offer yet.
#[test]
fn the_list_is_drawn_for_every_engine_and_the_dropdown_is_filtered() {
    let settings_view = read("ui/SettingsView.qml");
    let block = block_of(&settings_view, "EnginesView {");

    assert!(
        !block.contains("visible:"),
        "the list is never hidden behind the engine that is selected: {block}"
    );
    assert!(
        block.contains("engines: root.engines"),
        "the list draws what the overlay read: {block}"
    );
    assert!(
        settings_view.contains("Settings.engineOptions(EnginesJs.unavailable(root.engines)"),
        "the dropdown offers only the engines this machine has"
    );

    let overlay = read("Overlay.qml");
    assert!(
        overlay.contains("if (root.settingsOpen) {\n      root.refreshEngines()"),
        "opening Settings reads the list"
    );

    // Both surfaces carry the same Settings view, so the state and the five
    // signals reach it whichever one the reader summoned.
    for card in ["ui/QuickCard.qml", "ui/ComposeCard.qml"] {
        let source = read(card);
        for wired in [
            "engines: root.engines",
            "onEngineInstallRequested",
            "onEngineCancelRequested",
            "onEngineRemoveRequested",
            "onEngineRemoveConfirmed",
            "onEngineKeepRequested",
        ] {
            assert!(source.contains(wired), "{card} carries {wired}");
        }
    }
    assert_eq!(
        overlay.matches("onEngineInstallRequested").count(),
        2,
        "the overlay wires both cards, so neither surface has a dead button"
    );
}

/// The body of one `<Type> {` block, from its opening brace to its match.
fn block_of(source: &str, opening: &str) -> String {
    let start = source
        .find(opening)
        .unwrap_or_else(|| panic!("the source declares {opening}"));
    let open = source[start..].find('{').expect("the block has a body") + start;

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
    panic!("{opening} has a closing brace")
}

/// The one engine slug the catalogue carries is the one the shell names, and
/// it is a slug the CLI accepts as an engine at all.
#[test]
fn the_catalogue_slug_is_an_engine_slug_the_shell_knows() {
    assert_eq!(install::slugs(), ["languagetool"]);
    assert_eq!(
        EngineSlug::from_stored("languagetool"),
        Some(EngineSlug::Languagetool)
    );
    assert!(install::is_component("languagetool"));
    // Every other engine has nothing to install, so the dropdown always offers
    // it and no row is ever drawn for it.
    assert!(!install::is_component("harper"));

    let engines_js = read("ui/engines.js");
    assert!(
        engines_js.contains("From the languagetool package"),
        "the hint names the package by the slug the CLI uses"
    );
}

/// Spec section 4: a fresh install checks with Harper. The default lives in
/// three files and this is what keeps them equal.
#[test]
fn the_built_in_engine_is_the_default_on_both_sides() {
    assert_eq!(CheckOptions::default().engine, EngineSlug::Harper);

    let settings_js = read("ui/settings.js");
    assert!(
        settings_js.contains(r#"var BUILT_IN_ENGINE = "harper""#),
        "the shell falls back to the engine that cannot go away"
    );
    assert!(
        settings_js.contains(
            r#"engine: { type: "enum", values: ["languagetool", "harper"], fallback: "harper" }"#
        ),
        "the stored default is the same engine the CLI resolves to"
    );
    assert!(
        read("ui/SettingsView.qml").contains(r#"property string engine: "harper""#),
        "the view's own starting value is that engine too"
    );
}

/// `doctor` carries a state word per route onto the machine, and the Settings
/// row reads that word rather than the prose of `detail`. This keeps the two
/// lists of words equal.
#[test]
fn the_doctor_state_words_are_the_ones_the_shell_reads() {
    assert_eq!(report::LANGUAGETOOL_INSTALLED, "installed");
    assert_eq!(report::LANGUAGETOOL_PACKAGE, "package");
    assert_eq!(report::LANGUAGETOOL_ABSENT, "absent");
    // The install command `doctor` names is the one this subcommand answers to,
    // and it needs no password.
    assert_eq!(
        report::LANGUAGETOOL_INSTALL_COMMAND,
        "grammachy engine install languagetool"
    );
    assert!(!report::LANGUAGETOOL_INSTALL_COMMAND.contains("sudo"));
}

/// The row states the CLI prints are the three the shell knows, so no answer
/// ever reads as a state the view has no drawing for.
#[test]
fn the_row_states_are_the_ones_the_shell_draws() {
    let engines_js = read("ui/engines.js");

    for word in ["absent", "partial", "ready"] {
        assert!(
            engines_js.contains(&format!("\"{word}\"")),
            "the shell knows the {word} state"
        );
    }
    // The two codes only a transfer answers, plus the shared one of 5.1. Each
    // is read off the enum rather than spelled here, so a rename of a variant
    // fails this test rather than a card at run time.
    for code in [
        ErrorCode::Cancelled,
        ErrorCode::DownloadFailed,
        ErrorCode::BadArguments,
    ] {
        let word = serde_json::to_value(code)
            .expect("the code serialises")
            .as_str()
            .expect("the code is one word")
            .to_string();
        assert!(
            engines_js.contains(&format!("\"{word}\"")),
            "the shell reads the {word} code"
        );
    }
}
