//! Settings precedence, spec section 7: flags, then the plugin's entry in
//! `shell.json`, then the built-in defaults.
//!
//! No test reads or writes the real `~/.config/omarchy/shell.json`. The
//! documents live in strings, and the one test that goes through the file
//! layer writes a temporary file and points the binary at it.

use grammachy::args::{CheckArgs, CheckOptions, EngineSlug, NativeLanguage, TargetEnglish};
use grammachy::settings::StoredSettings;

/// A `shell.json` whose bar layout carries the plugin entry.
fn document(entry_body: &str) -> String {
    format!(
        r#"{{
          "bar": {{ "layout": {{
            "left": [{{ "id": "omarchy.menu" }}],
            "center": [{{ "id": "io.github.jyooi.grammachy"{}{} }}],
            "right": []
          }} }},
          "plugins": []
        }}"#,
        if entry_body.is_empty() { "" } else { ", " },
        entry_body
    )
}

fn stored(entry_body: &str) -> StoredSettings {
    StoredSettings::from_json(&document(entry_body))
}

/// Flags with every key set, so a resolved value that is not this one came
/// from a lower layer.
fn all_flags() -> CheckArgs {
    CheckArgs {
        native: Some(NativeLanguage::Fr),
        target: Some(TargetEnglish::EnUs),
        engine: Some(EngineSlug::Harper),
    }
}

fn no_flags() -> CheckArgs {
    CheckArgs {
        native: None,
        target: None,
        engine: None,
    }
}

#[test]
fn defaults_apply_with_no_flags_and_no_file() {
    let options = CheckOptions::resolve(&no_flags(), &StoredSettings::default());

    assert_eq!(options.native, NativeLanguage::None);
    assert_eq!(options.target, TargetEnglish::EnUs);
    // Spec section 4, HUF-237: `harper` is the one engine compiled into the
    // binary, so a fresh install checks with it and downloads nothing.
    assert_eq!(options.engine, EngineSlug::Harper);
    assert_eq!(options, CheckOptions::default());
}

#[test]
fn the_file_wins_over_the_defaults_for_every_key() {
    let entry = stored(
        r#""nativeLanguage": "ja",
           "targetEnglish": "en-US",
           "engine": "languagetool""#,
    );

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert_eq!(options.native, NativeLanguage::Ja);
    assert_eq!(options.target, TargetEnglish::EnUs);
    assert_eq!(options.engine, EngineSlug::Languagetool);
}

#[test]
fn a_flag_wins_over_the_file_for_every_flagged_key() {
    let entry =
        stored(r#""nativeLanguage": "ja", "targetEnglish": "en-US", "engine": "languagetool""#);

    let options = CheckOptions::resolve(&all_flags(), &entry);

    assert_eq!(options.native, NativeLanguage::Fr);
    assert_eq!(options.target, TargetEnglish::EnUs);
    assert_eq!(options.engine, EngineSlug::Harper);
}

#[test]
fn a_flag_wins_over_the_defaults_when_the_file_is_silent() {
    let options = CheckOptions::resolve(&all_flags(), &StoredSettings::default());

    assert_eq!(options.native, NativeLanguage::Fr);
    assert_eq!(options.engine, EngineSlug::Harper);
}

#[test]
fn every_native_language_value_reads_back() {
    let cases = [
        ("none", NativeLanguage::None),
        ("zh", NativeLanguage::Zh),
        ("ms", NativeLanguage::Ms),
        ("es", NativeLanguage::Es),
        ("fr", NativeLanguage::Fr),
        ("de", NativeLanguage::De),
        ("pt", NativeLanguage::Pt),
        ("ja", NativeLanguage::Ja),
    ];

    for (value, expected) in cases {
        let entry = stored(&format!(r#""nativeLanguage": "{value}""#));
        assert_eq!(entry.native, Some(expected), "{value} reads back");
    }
}

#[test]
fn an_unknown_stored_value_reads_as_the_default() {
    let entry = stored(
        r#""nativeLanguage": "kl",
           "targetEnglish": "en-GB",
           "engine": "gpt""#,
    );

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert_eq!(options, CheckOptions::default());
}

#[test]
fn a_stored_value_of_the_wrong_json_type_reads_as_the_default() {
    let entry = stored(r#""nativeLanguage": 7, "engine": true"#);

    assert_eq!(
        CheckOptions::resolve(&no_flags(), &entry),
        CheckOptions::default()
    );
}

/// HUF-240: the `openai` and `openrouter` engines are gone. A `shell.json`
/// written by an older build may still name either one, and that stored
/// value must fall back to the default engine rather than erroring, the way
/// any other unrecognized `engine` value already does.
#[test]
fn a_stored_engine_of_a_removed_slug_falls_back_to_the_default() {
    for removed in ["openai", "openrouter"] {
        let entry = stored(&format!(r#""engine": "{removed}""#));

        assert_eq!(entry.engine, None, "{removed} does not parse to a slug");
        assert_eq!(
            CheckOptions::resolve(&no_flags(), &entry).engine,
            EngineSlug::Harper,
            "{removed} falls back to the default engine"
        );
    }
}

/// A key the spec never defined, or one this build no longer reads, must not
/// stop the rest of the entry from resolving.
#[test]
fn unknown_stored_keys_are_ignored_without_error() {
    let entry = stored(
        r#""nativeLanguage": "fr",
           "engine": "harper",
           "openaiBaseUrl": "http://localhost:9090",
           "openaiModel": "some-model",
           "localThinking": false,
           "someFutureKey": { "nested": true }"#,
    );

    assert_eq!(entry.native, Some(NativeLanguage::Fr));
    assert_eq!(entry.engine, Some(EngineSlug::Harper));

    let options = CheckOptions::resolve(&no_flags(), &entry);
    assert_eq!(options.native, NativeLanguage::Fr);
    assert_eq!(options.engine, EngineSlug::Harper);
}

#[test]
fn a_missing_entry_and_a_broken_file_read_as_the_defaults() {
    let no_entry = r#"{ "bar": { "layout": { "left": [], "center": [], "right": [] } } }"#;
    let no_plugin_key = r#"{ "version": 2 }"#;
    let not_json = "{ this is not JSON";

    for document in [no_entry, no_plugin_key, not_json, "", "[]"] {
        assert_eq!(
            StoredSettings::from_json(document),
            StoredSettings::default(),
            "{document} reads as the defaults"
        );
    }
}

#[test]
fn another_plugins_entry_is_never_read() {
    let other = r#"{ "bar": { "layout": { "center": [
        { "id": "omarchy.clock", "nativeLanguage": "zh", "engine": "harper" }
    ] } } }"#;

    assert_eq!(StoredSettings::from_json(other), StoredSettings::default());
}

#[test]
fn the_entry_is_also_read_from_the_plugins_array() {
    let document = r#"{
        "bar": { "layout": { "left": [], "center": [], "right": [] } },
        "plugins": [
            { "id": "io.github.jyooi.other" },
            { "id": "io.github.jyooi.grammachy", "nativeLanguage": "ms", "engine": "harper" }
        ]
    }"#;

    let entry = StoredSettings::from_json(document);

    assert_eq!(entry.native, Some(NativeLanguage::Ms));
    assert_eq!(entry.engine, Some(EngineSlug::Harper));
}
