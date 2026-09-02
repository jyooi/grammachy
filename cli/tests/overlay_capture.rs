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

/// The balanced `{ ... }` block that follows `needle`, which is how a QML
/// handler such as `onExited` is read as well as a `function`.
fn block_after(source: &str, needle: &str, label: &str) -> String {
    let start = source
        .find(needle)
        .unwrap_or_else(|| panic!("the source declares {label}"));
    let open = source[start..]
        .find('{')
        .unwrap_or_else(|| panic!("{label} has a body"))
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
    panic!("{label} has a closing brace")
}

/// The body of one `function <name>(...)` in a QML or JavaScript file.
fn function_body(source: &str, name: &str) -> String {
    block_after(source, &format!("function {name}("), name)
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

/// Spec section 3: only a run that took a Selection releases one, and it
/// releases at most once, whichever exit it takes. A run has four exits - the
/// close, the Clear, the keystroke that ends a Replace, and a source window
/// that is gone - so the rule lives in the one function all of them call rather
/// than at each of them.
#[test]
fn one_function_owns_the_release_and_every_exit_calls_it_plainly() {
    let source = read("Overlay.qml");
    let release = function_body(&source, "releasePrimary");

    let claimed = release
        .find("if (!root.runCaptured) return")
        .expect("the release asks whether this run captured");
    let waiting = release
        .find("if (root.replacePending) return")
        .expect("a Replace still to type holds the release back");
    let dropped = release
        .find("root.runCaptured = false")
        .expect("the release drops the claim, so no run releases twice");
    let ran = release
        .find("clearPrimary.running = true")
        .expect("and then it runs the clear");
    assert!(
        claimed < dropped && waiting < dropped,
        "the claim goes only once both questions are answered: {release}"
    );
    assert!(
        dropped < ran,
        "and it goes before the clear runs, so no second exit repeats it: {release}"
    );

    // The wait has to answer before the drop. A Replace closes the popup first
    // and types afterwards, so the claim must outlive that close.
    assert!(
        waiting < dropped,
        "a Replace keeps its claim through the close that armed the wait: {release}"
    );

    // One caller of the command, so no exit can go around the rule above.
    assert_eq!(
        source.matches("clearPrimary.running = true").count(),
        1,
        "`releasePrimary` is the one thing that runs the clear"
    );

    // Every exit calls it plainly. A call site that carried its own copy of the
    // guard would be a second rule to keep in step.
    assert_eq!(
        source.matches("root.releasePrimary()").count(),
        3,
        "the close, the Clear, and the paste keystroke are the three exits"
    );
    for exit in ["close", "clearCapture"] {
        let body = function_body(&source, exit);
        assert!(
            !body.contains("root.runCaptured = false"),
            "{exit} keeps no copy of the drop: {body}"
        );
    }

    // The paste keystroke is the exit that outlives the close, and it is the
    // one most likely to drift, so it is named here.
    let typed = source
        .find("id: pasteKeystroke")
        .expect("the overlay types with wtype");
    let exited = block_after(&source[typed..], "onExited", "the wtype exit handler");
    assert!(
        exited.contains("root.replacePending = false") && exited.contains("root.releasePrimary()"),
        "the keystroke ends the wait and then calls the one release: {exited}"
    );
    assert!(
        !exited.contains("root.runCaptured"),
        "and it keeps no copy of the guard either: {exited}"
    );

    // `checkLastAgain` runs the kept text inside the summon that is open, so it
    // must not claim a capture that summon never took.
    assert!(
        !function_body(&source, "checkLastAgain").contains("root.runCaptured = true"),
        "the kept text is no capture, so it claims no selection"
    );
}

/// Spec section 6: Replace only works while the Selection is still highlighted
/// in the source window. `checkLastAgain` reaches a result on a run that took
/// nothing, and the popup has already released the primary selection, so Apply
/// there is copy-only. The label the card prints is the one thing that happens,
/// so the card and the key read the same fact the release rests on.
#[test]
fn apply_replaces_only_on_a_run_that_holds_a_selection() {
    let source = read("Overlay.qml");
    let apply = function_body(&source, "applyCorrected");

    assert!(
        apply.contains("root.surface === \"quick\" && root.autoReplace && root.runCaptured"),
        "all three have to hold before Apply types anywhere: {apply}"
    );
    assert!(
        !function_body(&source, "checkLastAgain").contains("root.runCaptured = true"),
        "the kept text claims no Selection, which is what makes it copy-only"
    );
    assert!(
        source.contains("runCaptured: root.runCaptured"),
        "the quick card is told the same fact, so the button agrees with the key"
    );

    // The card decides the label and the tooltip from one property, so the two
    // can never disagree with each other or with Apply.
    let card = read("ui/QuickCard.qml");
    assert!(
        card.contains("readonly property bool replaces: root.autoReplace && root.runCaptured"),
        "one property of the card says whether Apply replaces"
    );
    let label = function_body(&card, "applyLabel");
    assert!(
        label.contains("root.replaces") && !label.contains("root.autoReplace"),
        "the label follows that property rather than the setting alone: {label}"
    );
    assert!(
        card.contains(r#"tooltipText: root.replaces ? "Ctrl + Enter, or Ctrl + C to copy only""#),
        "and so does the tooltip"
    );
}

/// Spec sections 2 and 3: Compose captures nothing, so a Compose that closes
/// owns no primary selection to release and no capture to record. `resetRun`
/// drops the source window and leaves the text of the run before it in place,
/// so the text alone cannot say whether this run captured.
#[test]
fn a_run_that_captured_nothing_records_nothing_and_releases_nothing() {
    let source = read("Overlay.qml");

    assert!(
        source.contains("property bool runCaptured: false"),
        "the overlay records whether this run took a Selection"
    );
    assert!(
        function_body(&source, "captured").contains("root.runCaptured = true"),
        "the one step that takes a Selection is what arms it"
    );
    assert!(
        function_body(&source, "resetRun").contains("root.runCaptured = false"),
        "and every summon starts with nothing captured"
    );
    assert!(
        !function_body(&source, "showCompose").contains("root.runCaptured = true"),
        "Compose takes no Selection of its own"
    );
    assert!(
        !function_body(&source, "showNothingNew").contains("root.runCaptured = true"),
        "and a summon that found nothing new took none either"
    );

    // The record sits behind that one question, so a surface that captured
    // nothing records nothing. The release asks the same question of its own,
    // which `one_function_owns_the_release_and_every_exit_calls_it_plainly`
    // holds, so the close calls it plainly.
    let close = function_body(&source, "close");
    let asked = close
        .find("if (root.runCaptured)")
        .expect("the close asks whether this run captured");
    let recorded = close
        .find("root.consumeCapture(")
        .expect("the close records the capture");
    let released = close
        .find("root.releasePrimary()")
        .expect("the close releases the selection");
    assert!(
        asked < recorded && recorded < released,
        "the record sits behind that question, and the release follows it: {close}"
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

    // Both steps sit in the one `wl-copy` exit handler, so the two offsets are
    // measured inside that block alone. A whole-file search would find some
    // later `root.close()` whatever the order in the handler is.
    let copied = source
        .find("id: copyProcess")
        .expect("the overlay copies the Corrected text with wl-copy");
    let exited = block_after(&source[copied..], "onExited", "the wl-copy exit handler");
    let armed = exited
        .find("root.replacePending = true")
        .expect("the Replace arms the wait");
    let closed = exited
        .find("root.close()")
        .expect("the Replace closes the popup");
    assert!(
        armed < closed,
        "the wait is armed before the close: {exited}"
    );

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

    // The button shows on every quick card but the empty state and the
    // too-long card, so the key has to reach the same set. The review mode
    // covers a result with Issues; this mode covers the rest.
    let mode = function_body(&source, "keyMode");
    assert!(
        mode.contains("Keymap.MODE_QUICK_CLEAR"),
        "a quick card with no Issues to decide still answers Ctrl + L: {mode}"
    );
    for phase in ["checking", "error", "notice"] {
        assert!(
            mode.contains(&format!("root.phase === \"{phase}\"")),
            "the {phase} card is one of them: {mode}"
        );
    }
    assert!(
        !mode.contains("root.phase === \"empty\"") && !mode.contains("root.phase === \"toolong\""),
        "and the two cards that draw no Clear button are not: {mode}"
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
    let notice = "No new selection. Highlight text and press SUPER + SHIFT + Q, or paste here.";
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

/// Every `wl-paste` the overlay runs is the bounded command of
/// `ui/capture.js`, with a byte bound in front of the collector and a clock
/// on the selection owner, so no producer decides what the shell holds. A
/// clipboard cut at its bound is not borrowed, because it could not go back
/// whole.
#[test]
fn every_paste_runs_the_bounded_command_and_a_cut_clipboard_is_not_borrowed() {
    let source = read("Overlay.qml");

    assert!(
        !source.contains("\"wl-paste\""),
        "no Process names wl-paste directly: ui/capture.js builds every paste command"
    );
    for (process, command) in [
        ("primaryPaste", "Capture.primaryCommand()"),
        ("savedClipboard", "Capture.borrowCommand()"),
        ("fallbackPaste", "Capture.fallbackCommand()"),
    ] {
        // The lines of this Process, up to the next `id:` of the file.
        let start = source
            .find(&format!("id: {process}"))
            .unwrap_or_else(|| panic!("Overlay.qml declares {process}"));
        let rest = &source[start..];
        let end = rest[1..]
            .find("\n    id: ")
            .map(|at| at + 1)
            .unwrap_or(rest.len());
        let block = &rest[..end];
        assert!(
            block.contains(&format!("command: {command}")),
            "{process} runs {command}: {block}"
        );
    }

    let borrowed = function_body(&source, "onClipboardBorrowed");
    let overflow = borrowed
        .find("Capture.pasteOverflowed(text)")
        .expect("the borrow asks whether the clipboard was cut");
    let keystroke = borrowed
        .find("copyKeystroke.running = true")
        .expect("the borrow sends the keystroke");
    assert!(
        overflow < keystroke,
        "the bound is asked before the keystroke: {borrowed}"
    );
    assert!(
        borrowed.contains("root.showNothingNew()"),
        "a cut clipboard lands on the empty state: {borrowed}"
    );
}
