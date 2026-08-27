//! The freshness rule of the capture and the Clear of the popup, spec
//! sections 3 and 6 (HUF-235).
//!
//! `ui/capture.test.js` drives the rule itself and counts the Checks a summon
//! starts, but it drives the steps itself: they are only the right steps if
//! `Overlay.qml` takes them in the same order. The overlay cannot be
//! instantiated outside the shell's plugin loader, so this test reads the file
//! the shell ships and holds those calls in place, the way `overlay_anchor.rs`
//! holds the Replace in place.
//!
//! No test here reads the machine, touches a clipboard, or runs an engine.

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

/// The bug: the compositor keeps the last primary selection, so a summon with
/// nothing highlighted read the text of the summon before it and checked it
/// again. One rule decides that, and the capture asks it before the Check.
#[test]
fn a_capture_is_measured_against_the_last_consumed_one_before_any_check() {
    let source = read("Overlay.qml");
    let captured = function_body(&source, "captured");

    assert!(
        captured.contains("Capture.isStale("),
        "the freshness rule of ui/capture.js is what decides: {captured}"
    );
    assert!(
        captured.contains("root.showNothingNew()"),
        "a stale capture lands on the empty state: {captured}"
    );

    let stale = captured
        .find("Capture.isStale(")
        .expect("the capture asks the rule");
    let checked = captured
        .find("root.runCheck(")
        .expect("a fresh capture is checked");
    assert!(
        stale < checked,
        "the rule answers before the Check: {captured}"
    );

    let empty = function_body(&source, "showNothingNew");
    assert!(
        empty.contains("root.phase = \"empty\"") && empty.contains("root.capturedText = \"\""),
        "the empty state drops the capture and shows itself: {empty}"
    );
    assert!(
        !empty.contains("root.runCheck(") && !empty.contains("root.launchCheck("),
        "nothing new to check starts no Check: {empty}"
    );
}

/// Spec section 3: the capture is recorded where it is taken, and nothing more
/// happens to the compositor there. A terminal drops its own highlight when it
/// loses primary ownership, so a release at capture time would take the
/// Selection away from under the Apply the reader is still deciding on.
#[test]
fn the_capture_is_recorded_where_it_is_taken_and_releases_nothing() {
    let source = read("Overlay.qml");
    let consume = function_body(&source, "consumeCapture");

    assert!(
        consume.contains("root.lastCapturedText = text")
            && consume.contains("root.lastCapturedWindow = window"),
        "the consumed capture is kept by text and by source window: {consume}"
    );
    assert!(
        !consume.contains("clearPrimary.running") && !consume.contains("root.releasePrimary()"),
        "the capture releases no selection: {consume}"
    );

    let captured = function_body(&source, "captured");
    assert!(
        !captured.contains("root.releasePrimary()"),
        "and neither does the step that takes it: {captured}"
    );
}

/// Spec section 3: the primary selection goes when the popup closes, whether
/// the run ended in Apply, Replace, Clear, or Close. One function decides it
/// and one process does it.
#[test]
fn the_primary_selection_is_released_when_the_popup_closes() {
    let source = read("Overlay.qml");

    assert!(
        source.contains(r#"command: ["wl-copy", "--primary", "--clear"]"#),
        "the release is `wl-copy --primary --clear`"
    );
    let release = function_body(&source, "releasePrimary");
    assert!(
        release.contains("clearPrimary.running = true"),
        "one function runs it: {release}"
    );

    let close = function_body(&source, "close");
    assert!(
        close.contains("root.consumeCapture(root.capturedText"),
        "a popup that closes records the capture it holds: {close}"
    );
    assert!(
        close.contains("root.releasePrimary()"),
        "and releases the selection it came from: {close}"
    );
    assert!(
        function_body(&source, "clearCapture").contains("root.releasePrimary()"),
        "Clear ends a run too, so it releases the same way"
    );
}

/// Replace is the one path that outlives the close: it closes the popup, asks
/// for the source window, and only then types over the highlight that is still
/// there. Releasing the primary selection at the close would take that
/// highlight away before the keystroke landed.
#[test]
fn a_replace_holds_the_release_back_until_it_has_typed() {
    let source = read("Overlay.qml");

    let release = function_body(&source, "releasePrimary");
    assert!(
        release.contains("if (root.replacePending) return"),
        "a Replace still to type holds the release back: {release}"
    );

    assert!(
        source.contains("root.replacePending = true"),
        "the Replace path arms that wait before it closes the popup"
    );
    let armed = source
        .find("root.replacePending = true")
        .expect("the Replace arms the wait");
    let closed = source[armed..]
        .find("root.close()")
        .expect("the Replace closes the popup after it");
    assert!(closed > 0, "the wait is armed before the close");

    // The keystroke is what ends it, and a Replace that never types must not
    // leave the release waiting for ever.
    let keystroke = source
        .find("id: pasteKeystroke")
        .expect("the overlay types with wtype");
    let tail = &source[keystroke..];
    assert!(
        tail.contains("root.replacePending = false") && tail.contains("root.releasePrimary()"),
        "the keystroke ends the wait and releases the selection: {tail}"
    );
    assert!(
        function_body(&source, "showSourceGone").contains("root.replacePending = false"),
        "a source window that is gone ends the wait too"
    );
}

/// Step 2 of spec section 3. A field with nothing selected answers Ctrl + C by
/// leaving the clipboard as it was, so what comes back is an earlier copy
/// rather than a Selection.
#[test]
fn a_ctrl_c_that_copied_nothing_is_not_a_selection() {
    let source = read("Overlay.qml");
    let fallback = function_body(&source, "onFallbackCaptured");

    assert!(
        fallback.contains("Capture.copiedNothing("),
        "the fallback asks whether the clipboard moved at all: {fallback}"
    );
    assert!(
        fallback.contains("root.showNothingNew()"),
        "a clipboard that did not move lands on the empty state: {fallback}"
    );
    assert!(
        !fallback.contains("Errors.EMPTY_SELECTION"),
        "one empty answer, not two cards for the same situation: {fallback}"
    );

    let borrowed = fallback
        .find("var borrowed = root.borrowedClipboard")
        .expect("the fallback holds what the clipboard had");
    let restored = fallback
        .find("root.restoreBorrowedClipboard()")
        .expect("the borrow goes back");
    assert!(
        borrowed < restored,
        "what the clipboard held is read before the borrow goes back: {fallback}"
    );
}

/// Spec section 6: `Check last text again` runs the kept text and captures
/// nothing, so a selection that changed since cannot reach the engine.
#[test]
fn check_last_text_again_runs_the_kept_text_with_no_capture() {
    let source = read("Overlay.qml");
    let again = function_body(&source, "checkLastAgain");

    assert!(
        again.contains("root.runCheck(root.lastCapturedText)"),
        "the kept text is what runs: {again}"
    );
    assert!(
        !again.contains("root.probeSourceWindow()") && !again.contains("root.beginPrimaryPaste()"),
        "no second capture is taken: {again}"
    );
    assert!(
        again.contains("root.lastCapturedText.length === 0"),
        "with nothing kept there is nothing to run: {again}"
    );
}

/// Spec section 6: Clear drops the capture and the review, keeps the popup
/// open on the empty state, and never touches the Draft, which is the one
/// thing the plugin keeps.
#[test]
fn clear_lands_on_the_empty_state_and_leaves_the_draft_alone() {
    let source = read("Overlay.qml");
    let clear = function_body(&source, "clearCapture");

    for reset in [
        "root.issues = []",
        "root.decisions = []",
        "root.focusIndex = 0",
        "root.applied = false",
        "root.engine = \"\"",
    ] {
        assert!(clear.contains(reset), "Clear resets {reset}: {clear}");
    }
    assert!(
        clear.contains("root.restoreBorrowedClipboard()"),
        "Clear puts a borrowed clipboard back: {clear}"
    );
    assert!(
        clear.contains("root.showNothingNew()"),
        "Clear lands on the same empty state: {clear}"
    );
    assert!(
        !clear.contains("root.opened = false") && !clear.contains("root.close()"),
        "the popup stays open: {clear}"
    );
    assert!(
        !clear.contains("draftText") && !clear.contains("root.clearDraft()"),
        "the Draft is the one thing Clear never touches: {clear}"
    );
    assert!(
        !clear.contains("root.lastCapturedText = "),
        "the kept text survives Clear, so the empty state still offers it: {clear}"
    );
}

/// Spec section 6: Clear has a key and a button, and both reach the same one
/// function. `ui/keymap.test.js` owns which press it is.
#[test]
fn clear_is_reachable_from_the_key_map_and_from_the_hero() {
    let source = read("Overlay.qml");

    assert!(
        source.contains("l: Qt.Key_L"),
        "the key map is given the code it compares against"
    );
    assert!(
        function_body(&source, "handleKey")
            .contains("else if (action === Keymap.CLEAR) root.clearCapture()"),
        "the Clear action lands on the one function"
    );
    assert!(
        source.contains("onClearRequested: root.clearCapture()"),
        "the hero button lands on the same one"
    );

    let card = read("ui/QuickCard.qml");
    assert!(
        card.contains(r#"id: "clear", text: "Clear""#),
        "the quick card hero carries the Clear button"
    );
    assert!(
        card.contains("Capture.NOTHING_NEW") && card.contains("Capture.CHECK_LAST_AGAIN"),
        "the empty state prints the one line and the one button that ui/capture.js words"
    );
}

/// The wording of the empty state is the product's, so it lives in one file
/// and the spec says the same thing.
#[test]
fn the_empty_state_says_the_same_thing_as_the_spec() {
    let notice = "No new selection. Highlight text and press SUPER + G, or paste here.";
    let button = "Check last text again";

    let capture = read("ui/capture.js");
    assert!(
        capture.contains(notice) && capture.contains(button),
        "ui/capture.js words both"
    );

    let spec = read("docs/spec/v1.md");
    assert!(
        spec.contains(notice) && spec.contains(button),
        "spec sections 3 and 6 record the same wording"
    );
}
