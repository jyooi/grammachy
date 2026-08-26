//! The error cards of spec section 8 make three promises the CLI has to keep.
//!
//! `ui/errors.js` carries a copy of the per-engine Check timeout, because a run
//! that never answered leaves the shell nothing to read the number from. It
//! also carries a card for every code the CLI can emit. And `Overlay.qml`
//! promises that Retry re-runs the Check on the Selection that failed, with no
//! second capture.
//!
//! No test here starts an engine, a server, or a unit: it reads the two files
//! the shell ships and compares them with the constants beside it.

use grammachy::engines::{harper, languagetool, openai};

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The value of `<key>: <n>` inside the `TIMEOUT_SECONDS` table of the QML side.
fn timeout_seconds(source: &str, slug: &str) -> u64 {
    let table = source
        .split_once("var TIMEOUT_SECONDS = {")
        .expect("errors.js declares TIMEOUT_SECONDS")
        .1
        .split_once('}')
        .expect("the TIMEOUT_SECONDS table is closed")
        .0;
    let needle = format!("{slug}:");
    let start = table
        .find(&needle)
        .unwrap_or_else(|| panic!("TIMEOUT_SECONDS names {slug}"))
        + needle.len();
    let digits: String = table[start..]
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("the {slug} timeout is a plain integer literal"))
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

#[test]
fn the_overlay_timeouts_equal_the_adapter_timeouts() {
    let source = read("ui/errors.js");
    assert_eq!(
        timeout_seconds(&source, "languagetool"),
        languagetool::DEFAULT_TIMEOUT.as_secs()
    );
    assert_eq!(
        timeout_seconds(&source, "openai"),
        openai::DEFAULT_TIMEOUT.as_secs()
    );
    // The debug build gives Harper a longer budget so CI can load the
    // dictionary, so the shipped number is the one a user waits.
    assert_eq!(
        timeout_seconds(&source, "harper"),
        harper::SHIPPED_TIMEOUT_SECS
    );
}

/// Every `ErrorCode` of `cli/src/envelope.rs`, in its serialised snake_case.
///
/// `setup_failed` shares the enum with the Check codes.
/// A Check never answers it (spec sections 10 and 12).
/// The overlay has no card for it.
fn contract_codes() -> Vec<String> {
    let source = read("cli/src/envelope.rs");
    let block = source
        .split_once("pub enum ErrorCode {")
        .expect("envelope.rs declares ErrorCode")
        .1
        .split_once('}')
        .expect("the ErrorCode enum is closed")
        .0;

    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|line| {
            let variant = line.trim_end_matches(',');
            let mut snake = String::new();
            for (index, character) in variant.char_indices() {
                if character.is_ascii_uppercase() {
                    if index > 0 {
                        snake.push('_');
                    }
                    snake.push(character.to_ascii_lowercase());
                } else {
                    snake.push(character);
                }
            }
            snake
        })
        .collect()
}

#[test]
fn the_overlay_knows_every_code_the_cli_can_emit() {
    let source = read("ui/errors.js");
    // Three codes of the enum are not Check codes: `setup_failed` belongs to
    // `grammachy setup`, and `cancelled` and `download_failed` belong to
    // `grammachy model`. `cli/tests/overlay_models.rs` holds those two to
    // `ui/models.js`, which is the card that shows them.
    let codes: Vec<String> = contract_codes()
        .into_iter()
        .filter(|code| !["setup_failed", "cancelled", "download_failed"].contains(&code.as_str()))
        .collect();
    assert_eq!(codes.len(), 6, "spec section 5.1 fixes six codes");

    for code in codes {
        assert!(
            source.contains(&format!("\"{code}\"")),
            "ui/errors.js has a card for {code}"
        );
    }
}

/// Spec section 8: Retry re-runs the Check with the same Selection and no
/// re-capture, so a selection that changed in the source window since the
/// failure can never reach the engine.
#[test]
fn retry_reruns_the_failed_selection_and_never_captures_again() {
    let body = function_body(&read("Overlay.qml"), "retryCheck");

    assert!(
        body.contains("root.runCheck(root.selectionText)"),
        "retryCheck runs the Check on the text the last Check ran on: {body}"
    );
    for capture in [
        "startQuick",
        "beginPrimaryPaste",
        "wl-paste",
        "runGeneration",
    ] {
        assert!(
            !body.contains(capture),
            "retryCheck must not reach {capture}: {body}"
        );
    }
}
