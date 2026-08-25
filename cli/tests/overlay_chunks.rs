//! The chunked Check of spec section 9 makes promises no node test can reach.
//!
//! `ui/errors.test.js` runs the whole route against a stub binary, but it drives
//! the loop itself: the shared functions it calls are only the right ones if
//! `Overlay.qml` calls the same ones in the same order. The overlay cannot be
//! instantiated outside the shell's plugin loader, so this test reads the file
//! the shell ships and holds those calls in place, the way `overlay_errors.rs`
//! holds Retry in place.
//!
//! No test here starts an engine, a server, or a unit.

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

/// Spec section 9: a Compose Check is one `grammachy chunk` and then one
/// `grammachy check` per Chunk, never one Check on the whole Draft.
#[test]
fn a_compose_check_starts_with_the_chunk_list() {
    let source = read("Overlay.qml");
    let body = function_body(&source, "startComposeCheck");

    assert!(
        body.contains("root.runChunkList()"),
        "the Compose Check opens with the Chunk list: {body}"
    );
    assert!(
        !body.contains("root.runCheck("),
        "the Compose Check never sends the whole Draft to one check: {body}"
    );
    assert!(
        function_body(&source, "runChunkList").contains("\"chunk\""),
        "runChunkList runs the chunk subcommand"
    );
}

/// The Chunk list arrives, the run walks it from the first Chunk.
#[test]
fn the_chunk_list_starts_the_walk_at_the_first_chunk() {
    let body = function_body(&read("Overlay.qml"), "onChunkListOutput");

    assert!(
        body.contains("Errors.readChunks("),
        "the Chunk list is read through the shared reader: {body}"
    );
    assert!(
        body.contains("root.chunkIndex = 0") && body.contains("root.runChunk()"),
        "the walk starts at the first Chunk: {body}"
    );
}

/// The acceptance criterion of this ticket: every Chunk's spans move by that
/// Chunk's own start, and the moved spans are verified against the whole Draft.
#[test]
fn every_chunk_is_shifted_by_its_own_start_and_verified_against_the_draft() {
    let body = function_body(&read("Overlay.qml"), "absorbChunk");

    assert!(
        body.contains("Splice.shiftIssues(") && body.contains("chunk.start"),
        "absorbChunk moves the spans by the Chunk start: {body}"
    );
    assert!(
        body.contains("Splice.verifiedIssues(root.selectionText,"),
        "the moved spans are verified against the whole Draft: {body}"
    );
    assert!(
        body.contains("Splice.mergeIssues(root.issues,"),
        "the Chunk's Issues merge into the one list: {body}"
    );
    // The Chunk's own text is sliced with the same helper the node test uses,
    // so a boundary is never read one way here and another way there.
    assert!(
        function_body(&read("Overlay.qml"), "runChunk").contains("Splice.chunkText("),
        "runChunk sends the Chunk's own text"
    );
}

/// Spec section 9: Cancel stops after the Chunk in flight and keeps what
/// finished, so it never kills the process and never drops an Issue.
#[test]
fn cancel_stops_after_the_chunk_in_flight_and_keeps_what_finished() {
    let source = read("Overlay.qml");
    let cancel = function_body(&source, "cancelChunkRun");

    assert!(
        cancel.contains("root.chunkCancelled = true"),
        "Cancel marks the run rather than ending it: {cancel}"
    );
    for killed in ["running = false", "root.issues = []", "signal"] {
        assert!(
            !cancel.contains(killed),
            "Cancel must not reach {killed}: {cancel}"
        );
    }

    let absorb = function_body(&source, "absorbChunk");
    let merge = absorb
        .find("Splice.mergeIssues(")
        .expect("absorbChunk merges the Chunk's Issues");
    let stop = absorb
        .find("root.chunkCancelled")
        .expect("absorbChunk honours a Cancel");
    assert!(
        merge < stop,
        "the Chunk in flight is merged before the Cancel stops the run: {absorb}"
    );
}

/// A failed Chunk keeps the Issues from the finished ones and leaves
/// `chunkIndex` on itself, so `Retry remaining` resumes there.
#[test]
fn a_failed_chunk_keeps_its_issues_and_retry_resumes_at_it() {
    let source = read("Overlay.qml");
    let failed = function_body(&source, "showChunkError");

    assert!(
        failed.contains("Errors.chunkCard("),
        "the failure draws the inline card of section 9: {failed}"
    );
    assert!(
        failed.contains("hasPartial: root.issues.length > 0"),
        "the card knows whether there is anything to review: {failed}"
    );
    for lost in [
        "root.issues = []",
        "root.chunkIndex = 0",
        "root.chunks = []",
    ] {
        assert!(
            !failed.contains(lost),
            "a failed Chunk must not reach {lost}: {failed}"
        );
    }

    let retry = function_body(&source, "retryRemaining");
    assert!(
        retry.contains("root.runChunk()"),
        "Retry remaining resumes the walk at the Chunk that failed: {retry}"
    );
    for restart in [
        "root.chunkIndex = 0",
        "root.issues = []",
        "root.chunkElapsedMs = 0",
    ] {
        assert!(
            !retry.contains(restart),
            "Retry remaining must not reach {restart}: {retry}"
        );
    }
}

/// Spec section 2: a payload or a button that carries a text replaces a
/// non-empty Draft only after a confirm.
#[test]
fn a_carried_text_replaces_a_non_empty_draft_only_after_a_confirm() {
    let source = read("Overlay.qml");
    let compose_with = function_body(&source, "composeWith");

    assert!(
        compose_with.contains("root.draftText.length === 0"),
        "an empty Draft is the one case that takes the text straight away: {compose_with}"
    );
    assert!(
        compose_with.contains("root.phase = \"confirm\""),
        "a non-empty Draft goes through the confirm: {compose_with}"
    );

    // Only the confirm may put the pending Draft in place.
    let replace = function_body(&source, "replaceDraft");
    assert!(
        replace.contains("root.phase !== \"confirm\"")
            && replace.contains("root.draftText = root.pendingDraft"),
        "the confirm is what replaces the Draft: {replace}"
    );

    // Both triggers of spec section 2 that carry the Selection land here.
    assert!(
        source.contains("onComposeRequested: root.composeWith(root.capturedText)"),
        "the popup's Compose button and the too-long card carry the Selection over"
    );
    assert!(
        source.contains("root.composeWith(payload.text)"),
        "a compose payload with a text lands on the same route"
    );
}
