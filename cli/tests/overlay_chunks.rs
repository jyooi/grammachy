fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

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
    assert!(
        function_body(&read("Overlay.qml"), "runChunk").contains("Splice.chunkText("),
        "runChunk sends the Chunk's own text"
    );
}

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

    let replace = function_body(&source, "replaceDraft");
    assert!(
        replace.contains("root.phase !== \"confirm\"")
            && replace.contains("root.draftText = root.pendingDraft"),
        "the confirm is what replaces the Draft: {replace}"
    );

    assert!(
        source.contains("onComposeRequested: root.composeWith(root.capturedText)"),
        "the popup's Compose button and the too-long card carry the Selection over"
    );
    assert!(
        source.contains("root.composeWith(payload.text)"),
        "a compose payload with a text lands on the same route"
    );
}

#[test]
fn a_retry_after_an_engine_change_packs_the_draft_again() {
    let source = read("Overlay.qml");

    assert!(
        function_body(&source, "runChunkList").contains("root.chunkEngine ="),
        "the run records the Engine its Chunk list was packed for"
    );

    let retry = function_body(&source, "retryRemaining");
    assert!(
        retry.contains(
            "Limits.checkLimit(root.chunkEngine) !== Limits.checkLimit(root.setting(\"engine\"))"
        ),
        "Retry remaining compares the sizes rather than the slugs: {retry}"
    );
    assert!(
        retry.contains("root.chunkEngine = root.setting(\"engine\")"),
        "a list of the same size resumes on the Engine the reader picked: {retry}"
    );
    assert!(
        retry.contains("root.runChunkList()"),
        "a dropped Chunk list is packed again: {retry}"
    );

    let dropped = function_body(&source, "dropChunkListForNewEngine");
    for cleared in [
        "root.chunks = []",
        "root.chunkIndex = 0",
        "root.issues = []",
    ] {
        assert!(
            dropped.contains(cleared),
            "the dropped run clears {cleared}: {dropped}"
        );
    }
}

#[test]
fn every_chunk_is_checked_on_the_engine_its_list_was_packed_for() {
    let source = read("Overlay.qml");
    let run_chunk = function_body(&source, "runChunk");

    assert!(
        run_chunk.contains("root.runEngine()"),
        "the Chunk names the Engine of its own list: {run_chunk}"
    );
    assert!(
        !run_chunk.contains("root.setting(\"engine\")"),
        "the Chunk must not name the live setting: {run_chunk}"
    );
    assert!(
        function_body(&source, "runEngine").contains("root.chunkEngine"),
        "the run's Engine is the recorded one"
    );

    let command = function_body(&source, "checkCommand");
    assert!(
        command.contains("command.push(\"--engine\", engineSlug)"),
        "the check command names the Engine it was handed: {command}"
    );
    assert!(
        !command.contains("root.setting(\"engine\")"),
        "the check command must not read the setting itself: {command}"
    );

    let failed = function_body(&source, "showChunkError");
    assert!(
        failed.contains("engineSlug: root.runEngine()")
            && failed.contains("root.engineLabel(root.runEngine())"),
        "the inline card names the Engine the Chunk ran on: {failed}"
    );
}
