//! Where the quick popup opens and where Replace types, spec sections 3 and 6.
//!
//! `ui/anchor.test.js` runs the arithmetic and drives the Replace against a
//! stub `hyprctl`, but it drives the steps itself: they are only the right
//! steps if `Overlay.qml` takes them in the same order. The overlay cannot be
//! instantiated outside the shell's plugin loader, so this test reads the file
//! the shell ships and holds those calls in place, the way `overlay_chunks.rs`
//! holds the chunked Check in place.
//!
//! No test here reads the machine, moves the focus, or types anything.

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

/// Spec section 3: the compositor still calls the source window the active one
/// while the popup is hidden, so that is where the answer is read. The capture
/// waits on it rather than racing it, or the card opens in the wrong place and
/// then jumps.
#[test]
fn the_capture_reads_the_source_window_before_it_takes_the_selection() {
    let source = read("Overlay.qml");
    let start = function_body(&source, "startQuick");

    assert!(
        start.contains("root.probeSourceWindow()"),
        "a summon asks the compositor which window is active: {start}"
    );
    assert!(
        !start.contains("root.beginPrimaryPaste()"),
        "the capture waits on that answer rather than racing it: {start}"
    );

    let probed = function_body(&source, "onSourceProbed");
    assert!(
        probed.contains("Anchor.readActiveWindow(") && probed.contains("root.sourceWindow ="),
        "the answer is read through the shared reader and kept: {probed}"
    );
    assert!(
        probed.contains("root.beginPrimaryPaste()"),
        "the capture goes on once the answer is in: {probed}"
    );
    assert!(
        function_body(&source, "probeSourceWindow").contains("Anchor.activeWindowCommand()"),
        "the query is the one `ui/anchor.js` owns"
    );
}

/// The bug: the card hung off the bar corner from `barPosition` and `barSize`
/// alone, however far that was from the text. It is placed beside the source
/// window now, and one pure function decides that.
#[test]
fn the_quick_card_is_placed_from_the_source_window() {
    let source = read("Overlay.qml");

    assert!(
        source.contains("Anchor.placeCard({"),
        "the quick card takes its place from `ui/anchor.js`"
    );
    assert!(
        source.contains("window: root.sourceWindow"),
        "the placement is about the window the Selection came from"
    );
    assert!(
        source.contains("x: card.placement.x") && source.contains("y: card.placement.y"),
        "the card reads both coordinates from that one answer"
    );
    // The old corner arithmetic must not survive beside the new placement, or
    // one of the two silently wins.
    for corner in [
        "parent.width - card.width - root.gap",
        "parent.height - card.height - root.gap",
    ] {
        assert!(
            !source.contains(corner),
            "the bar corner belongs to `ui/anchor.js` now, not to the card: {corner}"
        );
    }

    // Hyprland reports a window in the global layout and this surface covers
    // one monitor of it, so the placement needs the surface's own origin.
    assert!(
        source.contains("panel.screen ? panel.screen.x : 0")
            && source.contains("panel.screen ? panel.screen.y : 0"),
        "the placement converts the global layout to this surface"
    );
}

/// Spec section 9: Compose is centred and knows nothing about a source window.
#[test]
fn the_compose_card_stays_centred() {
    let source = read("Overlay.qml");
    let compose = source
        .find("ComposeCard {")
        .expect("the overlay hosts the Compose card");
    let body = &source[compose..];

    assert!(
        body.contains("anchors.centerIn: parent"),
        "Compose is still centred"
    );
    assert!(
        !body[..body.find("onCloseRequested").unwrap_or(body.len())].contains("Anchor."),
        "Compose is placed by nothing but the screen it is centred on"
    );
}

/// The acceptance criterion of this ticket: Replace types into the window the
/// Selection came from. Closing the popup hands the keyboard to whatever the
/// compositor picks, so the paste asks for that window by address first.
#[test]
fn replace_asks_for_the_source_window_before_it_types() {
    let source = read("Overlay.qml");

    // The timer that waits out the layer surface no longer types by itself.
    let timer = source
        .find("id: pasteTimer")
        .expect("the overlay waits before it pastes");
    let timer_body = &source[timer..timer + 400];
    assert!(
        timer_body.contains("root.focusSourceWindow("),
        "the wait ends on the ask, not on the keystroke: {timer_body}"
    );
    assert!(
        !timer_body.contains("pasteKeystroke.running = true"),
        "nothing is typed straight off the timer: {timer_body}"
    );

    let focus = function_body(&source, "focusSourceWindow");
    assert!(
        focus.contains("Anchor.focusCommand("),
        "the ask is the dispatch `ui/anchor.js` owns: {focus}"
    );

    let verify = function_body(&source, "verifySourceFocus");
    assert!(
        verify.contains("Anchor.activeWindowCommand()"),
        "the ask is checked against who holds the keyboard: {verify}"
    );

    // The dispatch exits 0 for a window that is gone, so only the check may
    // let the keystroke out.
    let verified = function_body(&source, "onSourceFocusVerified");
    let checked = verified
        .find("Anchor.isFocused(")
        .expect("the paste is gated on the check");
    let typed = verified
        .find("root.launchPasteKeystroke(")
        .expect("the paste happens once the check passes");
    assert!(
        checked < typed,
        "nothing is typed before the check answers: {verified}"
    );
    assert!(
        verified.contains("root.showSourceGone("),
        "a window that is gone gets the notice instead: {verified}"
    );
}

/// Spec section 6: the Corrected text is on the clipboard either way, so a
/// source window that closed is a notice and never a keystroke somewhere else.
#[test]
fn a_source_window_that_is_gone_is_a_notice_and_not_a_paste() {
    let source = read("Overlay.qml");
    let gone = function_body(&source, "showSourceGone");

    assert!(
        gone.contains("Anchor.SOURCE_GONE_TITLE") && gone.contains("Anchor.SOURCE_GONE_BODY"),
        "the wording lives in `ui/anchor.js`, where a node test can read it: {gone}"
    );
    assert!(
        gone.contains("root.opened = true"),
        "the card comes back to say what happened: {gone}"
    );
    assert!(
        !gone.contains("pasteKeystroke"),
        "nothing is typed anywhere: {gone}"
    );
}

/// A missing `hyprctl` must not leave the Apply half done: the capture goes on
/// without a source window, and the Replace refuses rather than hanging.
#[test]
fn a_compositor_that_never_answers_still_ends_the_apply() {
    let source = read("Overlay.qml");

    for process in ["id: sourceProbe", "id: focusSource", "id: verifySource"] {
        let at = source
            .find(process)
            .unwrap_or_else(|| panic!("the overlay declares {process}"));
        let body = &source[at..at + 900];
        assert!(
            body.contains("launchPending"),
            "{process} notices a binary that never started: {body}"
        );
    }

    let focus = function_body(&source, "focusSourceWindow");
    assert!(
        focus.contains("address.length === 0") && focus.contains("root.launchPasteKeystroke("),
        "with no source window recorded the paste is what it always was: {focus}"
    );
}

/// The overlay's copy of the wording is the module's, so the card and the node
/// test can never drift apart.
#[test]
fn the_notice_wording_lives_in_one_place() {
    let anchor = read("ui/anchor.js");

    assert!(
        anchor.contains("The source window closed"),
        "`ui/anchor.js` owns the notice title"
    );
    assert!(
        anchor.contains("clipboard"),
        "the notice says where the Corrected text went"
    );
    assert!(
        !read("Overlay.qml").contains("The source window closed"),
        "the overlay names the wording rather than repeating it"
    );
}

/// A summon that cancels Replace must not type into the next Check, and must
/// not open the gone-window notice on it. The capture path already keeps its
/// callbacks behind `runGeneration`; Replace has to do the same.
#[test]
fn replace_ignores_callbacks_from_a_cancelled_run() {
    let source = read("Overlay.qml");

    let reset = function_body(&source, "resetRun");
    let pending = reset
        .find("focusSource.launchPending = false")
        .expect("resetRun drops the focus start flag");
    let bump = reset
        .find("root.runGeneration += 1")
        .expect("resetRun bumps the generation");
    let stop = reset
        .find("focusSource.running = false")
        .expect("resetRun stops the focus process");
    assert!(
        pending < bump && bump < stop,
        "a cancelled Replace is stale before it is stopped: {reset}"
    );

    let copy = function_body(&source, "runCopy");
    assert!(
        copy.contains("copyProcess.generation = root.runGeneration"),
        "Replace records the generation when it starts: {copy}"
    );

    for name in [
        "focusSourceWindow",
        "verifySourceFocus",
        "onSourceFocusVerified",
        "showSourceGone",
        "launchPasteKeystroke",
    ] {
        let body = function_body(&source, name);
        assert!(
            body.contains("root.isLive("),
            "{name} refuses work from a cancelled Replace: {body}"
        );
    }

    let launch = function_body(&source, "launchPasteKeystroke");
    assert!(
        launch.contains("pasteKeystroke.running = true"),
        "the keystroke is the one launch that types: {launch}"
    );

    for process in ["id: focusSource", "id: verifySource"] {
        let at = source
            .find(process)
            .unwrap_or_else(|| panic!("the overlay declares {process}"));
        let body = &source[at..at + 900];
        assert!(
            body.contains("startedGeneration = root.runGeneration"),
            "{process} snapshots the generation when it starts: {body}"
        );
    }

    let timer = source
        .find("id: pasteTimer")
        .expect("the overlay waits before it pastes");
    let timer_body = &source[timer..timer + 400];
    assert!(
        timer_body.contains("root.isLive(pasteTimer.generation)"),
        "the wait itself is generation-gated: {timer_body}"
    );
}
