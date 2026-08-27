//! The cloud engine on the surfaces, `docs/spec/evals.md` section 7.
//!
//! `ui/settings.test.js` runs the Settings rules and the consent gate, but it
//! drives them itself: the rules it calls are only the right ones if
//! `Overlay.qml`, the two cards, and the bar widget call the same ones.
//! `Overlay.qml` cannot be instantiated outside the shell's plugin loader, so
//! this test reads the files the shell ships and holds those calls in place,
//! the way `overlay_models.rs` holds the Models verbs.
//!
//! No test here reaches openrouter.ai or reads the key file.

use grammachy::args::EngineSlug;
use grammachy::settings::{DEFAULT_OPENROUTER_MODEL, OPENROUTER_MODEL_PLACEHOLDER};

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

/// Where the block holding `marker` ends, by matching its braces. Braces inside
/// a double-quoted string do not count, because a description may carry one.
fn block_end(source: &str, marker: usize) -> usize {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, character) in source[marker..].char_indices() {
        if in_string {
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return marker + offset;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    panic!("the block that starts at {marker} is closed");
}

/// The slug is one word and both sides speak it.
#[test]
fn the_shell_and_the_cli_name_the_same_cloud_engine() {
    let source = read("ui/settings.js");

    assert!(
        source.contains(r#"var CLOUD_ENGINE = "openrouter""#),
        "ui/settings.js names the cloud engine"
    );
    assert_eq!(EngineSlug::Openrouter.as_str(), "openrouter");
    assert!(
        source.contains(r#"{ value: "openrouter", label: "Cloud LLM (OpenRouter)" }"#),
        "the dropdown carries the label spec section 7 fixes"
    );
    assert!(
        source.contains(r#""languagetool", "openai", "harper", "openrouter""#),
        "the engine descriptor accepts the slug the dropdown offers"
    );
}

/// Spec section 7: `openrouterModel` has no built-in default, and the
/// placeholder is what the empty field shows. The two live twice, so the two
/// copies have to agree.
#[test]
fn the_overlay_placeholder_and_default_equal_the_cli_ones() {
    let source = read("ui/settings.js");

    assert_eq!(DEFAULT_OPENROUTER_MODEL, "");
    assert!(
        source.contains(r#"openrouterModel: { type: "string", fallback: "" }"#),
        "the shell copy of the default is the empty string too"
    );
    assert!(
        source.contains(&format!(
            r#"var OPENROUTER_MODEL_PLACEHOLDER = "{OPENROUTER_MODEL_PLACEHOLDER}""#
        )),
        "the shell placeholder is {OPENROUTER_MODEL_PLACEHOLDER}"
    );
    assert!(
        source.contains(r#"cloudConsent: { type: "boolean", fallback: false }"#),
        "cloudConsent is the boolean of spec section 7, default false"
    );
}

/// Spec section 7 shows the cloud field for the cloud engine only, so it belongs
/// inside the group the view hides. Hoisting it into the root layout is the
/// regression this catches, because it shows the field for every engine.
#[test]
fn the_cloud_field_sits_in_the_group_the_engine_hides() {
    let source = read("ui/SettingsView.qml");

    let group = source
        .find("visible: root.showsOpenrouter")
        .expect("the view hides one group for the cloud engine");
    let group_end = block_end(&source, group);

    for needle in [
        "placeholderText: Settings.OPENROUTER_MODEL_PLACEHOLDER",
        r#"root.commitAndSettle("openrouterModel""#,
        "text: root.cloudKeyHint",
    ] {
        let at = source
            .find(needle)
            .unwrap_or_else(|| panic!("the view carries {needle}"));
        assert!(
            at > group && at < group_end,
            "{needle} is inside the group the engine hides: it sits at {at}, and the group runs to {group_end}"
        );
    }
}

/// Spec section 7: only the consent card writes `cloudConsent`, so the Settings
/// view draws no control for it.
#[test]
fn the_settings_view_draws_no_control_for_the_consent() {
    let source = read("ui/SettingsView.qml");

    assert!(
        !source.contains(r#"settingChanged("cloudConsent""#),
        "the Settings view never writes cloudConsent"
    );
    assert!(
        source.contains("Settings.keyHint(root.cloudKey)"),
        "the key hint comes from ui/settings.js, so a node test owns its wording"
    );
}

/// The gate sits on `launchCheck`, the one place a Check leaves for the CLI.
/// A gate on `runCheck` alone would let every Chunk of a Draft past it.
#[test]
fn the_gate_stands_on_the_one_route_out() {
    let source = read("Overlay.qml");
    let launch = function_body(&source, "launchCheck");

    assert!(
        launch.contains("root.needsCloudConsent(engineSlug)"),
        "launchCheck asks the question: {launch}"
    );
    assert!(
        launch.contains("root.askCloudConsent(text, engineSlug)"),
        "launchCheck holds the pending Check: {launch}"
    );
    let question = launch
        .find("needsCloudConsent")
        .expect("launchCheck asks the question");
    let started = launch
        .find("checkProcess.running = true")
        .expect("launchCheck starts the process");
    assert!(
        question < started,
        "the question comes before the process starts: {launch}"
    );

    assert!(
        function_body(&source, "needsCloudConsent").contains("Settings.needsCloudConsent"),
        "the rule lives in ui/settings.js, so a node test owns it"
    );
}

/// Continue keeps the answer and runs the Check that waited on it. It must not
/// go back through the gate, because the stored value takes a moment to come
/// back through the shell and the Check goes out now.
#[test]
fn continue_stores_the_answer_and_runs_the_pending_check() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "continueCloudCheck");

    assert!(
        body.contains(r#"root.persistSetting("cloudConsent", true)"#),
        "Continue stores the answer: {body}"
    );
    assert!(
        body.contains("root.cloudConsentGiven = true"),
        "Continue also answers for this session, so the write is never raced: {body}"
    );
    assert!(
        body.contains("root.launchCheck(text, engineSlug)"),
        "Continue runs the Check that waited: {body}"
    );
    let session = body
        .find("cloudConsentGiven = true")
        .expect("Continue answers for this session");
    let launch = body
        .find("root.launchCheck")
        .expect("Continue runs the Check");
    assert!(
        session < launch,
        "the session answer is set before the Check goes out: {body}"
    );
}

/// Cancel sends nothing. The engine setting stays as it is, so the next Check
/// asks again rather than quietly running somewhere else.
#[test]
fn cancel_sends_nothing_and_keeps_the_engine() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "cancelCloudCheck");

    assert!(
        !body.contains("launchCheck"),
        "Cancel starts no Check: {body}"
    );
    assert!(
        !body.contains("checkProcess"),
        "Cancel touches no process: {body}"
    );
    assert!(
        !body.contains("persistSetting"),
        "Cancel stores nothing, the engine setting included: {body}"
    );
    assert!(
        body.contains("root.clearCloudConsent()"),
        "Cancel drops the text it was holding: {body}"
    );

    // The notice names the Engine, and Cancel runs when the reader opens
    // Settings, so the name is read where the card is drawn and never baked in.
    assert!(
        !body.contains("engineLabel"),
        "Cancel bakes no Engine name into the notice: {body}"
    );
    assert!(
        source.contains(
            r#"readonly property string noticeBody: root.noticeNamesEngine
    ? root.noticeBodyText + root.engineLabel(root.setting("engine")) + "."
    : root.noticeBodyText"#
        ),
        "the drawn notice reads the Engine setting at draw time"
    );
}

/// A phase that is not in the key map inherits the review keys, which would let
/// a bare Enter mean Accept on a card that has nothing to accept.
#[test]
fn the_consent_phase_has_a_key_mode_of_its_own() {
    let source = read("Overlay.qml");
    let mode = function_body(&source, "keyMode");

    assert!(
        mode.contains(r#"root.phase === "cloudConsent""#)
            && mode.contains("Keymap.MODE_CLOUD_CONSENT"),
        "the consent phase names its own mode: {mode}"
    );
    assert!(
        source.contains("Keymap.CLOUD_CONTINUE) root.continueCloudCheck()"),
        "the key map dispatch carries Continue"
    );
    assert!(
        source.contains("Keymap.CLOUD_CANCEL) root.cancelCloudCheck()"),
        "the key map dispatch carries Cancel"
    );
}

/// A question that is off the screen must never still be answerable. The hero
/// gear is reachable from every phase and neither card draws the consent over
/// the Settings view, so opening Settings has to end the question.
#[test]
fn opening_settings_cancels_the_pending_consent() {
    let source = read("Overlay.qml");

    assert!(
        source.contains(
            r#"if (root.settingsOpen && root.phase === "cloudConsent") root.cancelCloudCheck()"#
        ),
        "opening Settings cancels the Check the consent card was holding"
    );

    let mode = function_body(&source, "keyMode");
    let settings = mode
        .find("root.settingsOpen) return Keymap.MODE_IDLE")
        .expect("keyMode answers for the Settings view");
    let consent = mode
        .find(r#"root.phase === "cloudConsent""#)
        .expect("keyMode answers for the consent card");
    assert!(
        settings < consent,
        "Settings hides the consent card, so Settings takes the keyboard first: {mode}"
    );
}

/// Neither card draws the consent over the Settings view, which is what makes
/// the cancel above the whole of the answer rather than half of it.
///
/// The wording and both buttons are three drawn parts, and every one of them
/// has to hang off the flag the Settings view clears: `showsCheck` on the quick
/// card and `showsCard` on the compose card. The gate may sit on `consenting`
/// itself or on each `visible` line, and nowhere else.
#[test]
fn neither_card_draws_the_consent_over_the_settings_view() {
    for (card, gate) in [
        ("ui/QuickCard.qml", "root.showsCheck"),
        ("ui/ComposeCard.qml", "root.showsCard"),
    ] {
        let source = read(card);
        let declaration = source
            .lines()
            .find(|line| line.contains("property bool consenting:"))
            .unwrap_or_else(|| panic!("{card} declares consenting"));
        let gated_at_the_source = declaration.contains(gate);

        let drawn: Vec<&str> = source
            .lines()
            .filter(|line| line.contains("visible:"))
            .filter(|line| line.contains("root.consenting") && !line.contains("!root.consenting"))
            .collect();
        assert_eq!(
            drawn.len(),
            3,
            "{card} draws the consent wording and both buttons: {drawn:?}"
        );
        for line in drawn {
            assert!(
                gated_at_the_source || line.contains(gate),
                "{card} draws `{}` while the Settings view is up",
                line.trim()
            );
        }
    }
}

/// Spec section 9: a chunked run that stops keeps what the engine already
/// answered. A `Retry remaining` after a failed Chunk can reach the consent
/// card with the Issues of the finished Chunks in hand, so Cancel puts the
/// reader back where the run was rather than in a blank Draft.
#[test]
fn cancel_keeps_a_partial_chunked_review() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "cancelCloudCheck");

    let keep = body
        .find(r#"root.surface === "compose" && root.issues.length > 0"#)
        .expect("Cancel tells a run that found Issues from one that found none");
    let clear = body
        .find("root.clearChunkRun()")
        .expect("a run that found nothing still ends");
    assert!(
        keep < clear,
        "the Issues in hand are answered for before the run is cleared: {body}"
    );
    assert!(
        body.contains("root.stopChunkRun()") && body.contains("root.backToChunkStop()"),
        "Cancel stops the run where it stands and draws the card it stood in front of: {body}"
    );

    // `Retry remaining` resumes at the Chunk that stopped the run, so the list
    // and the index have to outlive the Cancel.
    let stop = function_body(&source, "stopChunkRun");
    for gone in [
        "root.chunks = []",
        "root.chunkIndex = 0",
        "root.chunkEngine",
    ] {
        assert!(
            !stop.contains(gone),
            "stopChunkRun keeps {gone} for the retry: {stop}"
        );
    }

    // The retry clears the error card before the first Chunk goes out, so it
    // records what it cleared and the Cancel puts that back.
    assert!(
        function_body(&source, "retryRemaining").contains("root.chunkResume = root.errorCard"),
        "the retry records the failure it is leaving"
    );
    let back = function_body(&source, "backToChunkStop");
    assert!(
        back.contains(r#"root.phase = "error""#) && back.contains(r#"root.phase = "result""#),
        "Cancel lands on the failure it came from, or on the partial result: {back}"
    );

    // A pending failure never outlives the run it belongs to.
    assert!(
        function_body(&source, "clearChunkRun").contains("root.chunkResume = null"),
        "a cleared run drops the failure it was resuming from"
    );
    assert!(
        function_body(&source, "continueCloudCheck").contains("root.chunkResume = null"),
        "a Check that goes out drops the failure it was resuming from"
    );
}

/// The reader's decision time is not engine time. A chunked Check reaches the
/// card with the compose progress clock already running, so the card stops it
/// and Continue starts it again.
#[test]
fn the_consent_card_stops_the_compose_progress_clock() {
    let source = read("Overlay.qml");

    assert!(
        function_body(&source, "askCloudConsent").contains("root.pauseChunkClock()"),
        "the card stops the clock before it goes up"
    );
    assert!(
        function_body(&source, "pauseChunkClock").contains("chunkTicker.stop()"),
        "the pause stops the ticker"
    );

    let resume = function_body(&source, "resumeChunkClock");
    assert!(
        resume.contains("root.chunkStartedAt = Date.now() - root.chunkTickMs")
            && resume.contains("chunkTicker.start()"),
        "the resume starts again from what the run had spent: {resume}"
    );

    let cont = function_body(&source, "continueCloudCheck");
    let started = cont
        .find("root.resumeChunkClock()")
        .expect("Continue starts the clock again");
    let launch = cont
        .find("root.launchCheck")
        .expect("Continue runs the Check");
    assert!(
        started < launch,
        "the clock is running again before the Chunk goes out: {cont}"
    );

    // Cancel leaves no ticker behind, whether it ends the run or stops it where
    // it stands to keep a partial review.
    let cancel = function_body(&source, "cancelCloudCheck");
    assert!(
        cancel.contains("root.clearChunkRun()") && cancel.contains("root.stopChunkRun()"),
        "both ways out of Cancel stop the run: {cancel}"
    );
    assert!(
        function_body(&source, "stopChunkRun").contains("chunkTicker.stop()"),
        "stopping the run stops the ticker"
    );
    assert!(
        function_body(&source, "clearChunkRun").contains("root.stopChunkRun()"),
        "clearing the run stops it first, so the ticker never outlives it"
    );
}

/// Both surfaces share one Check, so both have to draw the card and both have
/// to report its two answers.
#[test]
fn both_cards_draw_the_consent_and_report_both_answers() {
    let overlay = read("Overlay.qml");

    for needle in [
        r#"consentCard: root.phase === "cloudConsent" ? root.cloudConsentCard() : null"#,
        "onCloudContinueRequested: root.continueCloudCheck()",
        "onCloudCancelRequested: root.cancelCloudCheck()",
        r#"openrouterModel: root.setting("openrouterModel")"#,
        "cloudKey: root.cloudKey",
    ] {
        assert_eq!(
            overlay.matches(needle).count(),
            2,
            "Overlay.qml gives {needle} to QuickCard and to ComposeCard"
        );
    }

    for card in ["ui/QuickCard.qml", "ui/ComposeCard.qml"] {
        let source = read(card);
        assert!(
            source.contains("property var consentCard: null"),
            "{card} declares the card model"
        );
        assert!(
            source.contains("signal cloudContinueRequested()")
                && source.contains("signal cloudCancelRequested()"),
            "{card} reports both answers"
        );
        assert!(
            source.contains("onClicked: root.cloudContinueRequested()")
                && source.contains("onClicked: root.cloudCancelRequested()"),
            "{card} draws both buttons"
        );
        assert!(
            source.contains("openrouterModel: root.openrouterModel")
                && source.contains("cloudKey: root.cloudKey"),
            "{card} passes the cloud settings on to the Settings view"
        );
    }
}

/// The key state is read through `doctor`, which is the only thing that may
/// read the key file. A QML that opened that file itself is the regression.
#[test]
fn the_key_state_comes_from_doctor_and_never_from_the_file() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "refreshCloudKey");

    assert!(
        body.contains(r#""doctor", "--engine", Settings.CLOUD_ENGINE, "--json""#),
        "the key state is one doctor run: {body}"
    );
    assert!(
        function_body(&source, "onCloudKeyOutput").contains("Settings.keyState(report)"),
        "the reader lives in ui/settings.js, so a node test owns it"
    );
    for qml in ["Overlay.qml", "ui/SettingsView.qml", "BarWidget.qml"] {
        assert!(
            !read(qml).contains("openrouter-key"),
            "{qml} never names the key file"
        );
    }
}

/// The bar glyph of spec section 7, with the tooltip the spec words.
#[test]
fn the_bar_draws_the_cloud_glyph_for_the_cloud_engine() {
    let source = read("BarWidget.qml");

    assert!(
        source.contains(
            r#"readonly property bool cloudEngine: Settings.valueOf(root.settings, "engine") === Settings.CLOUD_ENGINE"#
        ),
        "the bar reads the engine through the rules of ui/settings.js"
    );
    assert!(
        source.contains(r#"text: root.cloudEngine ? "G " + root.cloudGlyph : "G""#),
        "the glyph sits beside the G, and only for the cloud engine"
    );
    assert!(
        source.contains(r#""Grammachy: cloud engine, text is sent to OpenRouter""#),
        "the tooltip is the one spec section 7 words"
    );
    assert!(
        source.contains(r#""Grammachy: check the selected text""#),
        "every other engine keeps the tooltip it had"
    );
}
