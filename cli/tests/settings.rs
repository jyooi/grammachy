use grammachy::args::{CheckArgs, CheckOptions, EngineSlug, NativeLanguage, TargetEnglish};
use grammachy::settings::StoredSettings;

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
           "targetEnglish": "en-AU",
           "engine": "gpt""#,
    );

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert_eq!(options, CheckOptions::default());
}

#[test]
fn a_stored_target_english_of_en_gb_reads_back() {
    let entry = stored(r#""targetEnglish": "en-GB""#);

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert_eq!(options.target, TargetEnglish::EnGb);
}

#[test]
fn a_stored_value_of_the_wrong_json_type_reads_as_the_default() {
    let entry = stored(r#""nativeLanguage": 7, "engine": true"#);

    assert_eq!(
        CheckOptions::resolve(&no_flags(), &entry),
        CheckOptions::default()
    );
}

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
fn a_stored_hotkey_is_trimmed() {
    let entry = stored(r#""quickHotkey": "  SUPER + H  ", "composeHotkey": "SUPER + SHIFT + H""#);

    assert_eq!(entry.quick_hotkey.as_deref(), Some("SUPER + H"));
    assert_eq!(entry.compose_hotkey.as_deref(), Some("SUPER + SHIFT + H"));
}

#[test]
fn an_empty_or_blank_stored_hotkey_reads_as_unknown() {
    let entry = stored(r#""quickHotkey": "", "composeHotkey": "   ""#);

    assert_eq!(entry.quick_hotkey, None);
    assert_eq!(entry.compose_hotkey, None);
}

#[test]
fn a_missing_stored_hotkey_reads_as_unknown() {
    let entry = stored("");

    assert_eq!(entry.quick_hotkey, None);
    assert_eq!(entry.compose_hotkey, None);
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
