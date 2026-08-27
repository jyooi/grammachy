//! Validation that `grammachy check` does before any engine runs.

use grammachy::args::{CheckOptions, EngineSlug, NativeLanguage, TargetEnglish};
use grammachy::check;
use grammachy::envelope::Envelope;

/// The Check size limit of the default engine (spec section 4).
const MAX_UTF16_UNITS: usize = EngineSlug::Languagetool.check_limit_utf16();

fn code_of(envelope: &Envelope) -> String {
    let value: serde_json::Value = serde_json::from_str(&envelope.to_json()).unwrap();
    value["error"]["code"].as_str().unwrap().to_string()
}

fn message_of(envelope: &Envelope) -> String {
    let value: serde_json::Value = serde_json::from_str(&envelope.to_json()).unwrap();
    value["error"]["message"].as_str().unwrap().to_string()
}

/// Text of exactly `units` UTF-16 units, ending with one astral character.
fn text_of_units(units: usize) -> String {
    let mut text = "a".repeat(units - 2);
    text.push('\u{1F600}');
    text
}

#[test]
fn empty_stdin_is_an_empty_selection() {
    let envelope = check::run("", &CheckOptions::default());

    assert_eq!(envelope.exit_code(), 1);
    assert_eq!(code_of(&envelope), "empty_selection");
}

#[test]
fn whitespace_only_stdin_is_an_empty_selection() {
    let envelope = check::run(" \n\t", &CheckOptions::default());

    assert_eq!(code_of(&envelope), "empty_selection");
}

#[test]
fn an_astral_character_counts_as_two_units() {
    assert_eq!(check::utf16_len("\u{1F600}"), 2);
    assert_eq!(check::utf16_len("ab"), 2);
    assert_eq!(check::utf16_len("\u{00E9}"), 1);
    assert_eq!(
        check::utf16_len(&text_of_units(MAX_UTF16_UNITS)),
        MAX_UTF16_UNITS
    );
}

#[test]
fn text_at_the_limit_passes_validation() {
    let text = text_of_units(MAX_UTF16_UNITS);

    // Validation, not the engine, so no test reaches for a server.
    assert!(check::validate(&text, MAX_UTF16_UNITS).is_none());
}

#[test]
fn text_one_unit_over_the_limit_is_too_long() {
    let text = text_of_units(MAX_UTF16_UNITS + 1);
    let envelope = check::run(&text, &CheckOptions::default());

    assert_eq!(envelope.exit_code(), 1);
    assert_eq!(code_of(&envelope), "text_too_long");
}

#[test]
fn the_last_astral_character_can_cross_the_limit() {
    // 4,999 units of filler plus one astral character makes 5,001 units.
    let mut text = "a".repeat(MAX_UTF16_UNITS - 1);
    text.push('\u{1F600}');

    assert_eq!(check::utf16_len(&text), MAX_UTF16_UNITS + 1);
    assert_eq!(
        code_of(&check::validate(&text, MAX_UTF16_UNITS).expect("the text is too long")),
        "text_too_long"
    );
}

/// The limit belongs to the Engine (spec section 4), and the two remaining
/// engines share one: 5,000 units.
#[test]
fn each_engine_names_its_own_check_size_limit() {
    assert_eq!(EngineSlug::Languagetool.check_limit_utf16(), 5_000);
    assert_eq!(EngineSlug::Harper.check_limit_utf16(), 5_000);
}

/// The message has to name the limit that refused the text, or the card shows
/// a size bar the reader cannot place.
#[test]
fn the_too_long_message_names_the_selected_engine_limit() {
    for engine in [EngineSlug::Languagetool, EngineSlug::Harper] {
        let limit = engine.check_limit_utf16();
        let envelope = check::validate(&"a".repeat(limit + 1), limit)
            .expect("the text is over this engine's limit");

        assert_eq!(
            message_of(&envelope),
            format!(
                "The selection is {} units long, over the limit of {limit}.",
                limit + 1
            )
        );
    }
}

#[test]
fn built_in_defaults_are_the_spec_defaults() {
    let options = CheckOptions::default();

    assert_eq!(options.native, NativeLanguage::None);
    assert_eq!(options.target, TargetEnglish::EnUs);
    // Spec section 4, HUF-237: a fresh install checks with Harper, which is
    // compiled into the binary. Every other engine is something the user adds.
    assert_eq!(options.engine, EngineSlug::Harper);
}
