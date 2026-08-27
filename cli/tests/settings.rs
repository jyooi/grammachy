//! Settings precedence, spec section 7: flags, then the plugin's entry in
//! `shell.json`, then the built-in defaults.
//!
//! No test reads or writes the real `~/.config/omarchy/shell.json`. The
//! documents live in strings, and the one test that goes through the file
//! layer writes a temporary file and points the binary at it.

use grammachy::args::{
    CheckArgs, CheckOptions, EngineSlug, NativeLanguage, TargetEnglish, Thinking,
};
use grammachy::settings::{
    StoredSettings, DEFAULT_LOCAL_THINKING, DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL,
    DEFAULT_OPENROUTER_MODEL, OPENROUTER_MODEL_PLACEHOLDER,
};

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
        thinking: Some(Thinking::Off),
        openrouter_model: Some("vendor/flagged".to_string()),
    }
}

fn no_flags() -> CheckArgs {
    CheckArgs {
        native: None,
        target: None,
        engine: None,
        thinking: None,
        openrouter_model: None,
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
    assert_eq!(options.openai_base_url, DEFAULT_OPENAI_BASE_URL);
    assert_eq!(options.openai_model, DEFAULT_OPENAI_MODEL);
    assert_eq!(options.openai_api_key, "");
    // Spec section 4: thinking is on by default for the local engine.
    assert!(options.local_thinking);
    assert_eq!(options.local_thinking, DEFAULT_LOCAL_THINKING);
    assert_eq!(options, CheckOptions::default());
}

#[test]
fn the_file_wins_over_the_defaults_for_every_key() {
    let entry = stored(
        r#""nativeLanguage": "ja",
           "targetEnglish": "en-US",
           "engine": "openai",
           "openaiBaseUrl": "http://localhost:9090",
           "openaiModel": "some-other-model",
           "openaiApiKey": "sk-local",
           "localThinking": false"#,
    );

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert!(!options.local_thinking);
    assert_eq!(options.native, NativeLanguage::Ja);
    assert_eq!(options.target, TargetEnglish::EnUs);
    assert_eq!(options.engine, EngineSlug::Openai);
    assert_eq!(options.openai_base_url, "http://localhost:9090");
    assert_eq!(options.openai_model, "some-other-model");
    assert_eq!(options.openai_api_key, "sk-local");
}

#[test]
fn a_flag_wins_over_the_file_for_every_flagged_key() {
    let entry = stored(
        r#""nativeLanguage": "ja", "targetEnglish": "en-US", "engine": "openai",
           "localThinking": true"#,
    );

    let options = CheckOptions::resolve(&all_flags(), &entry);

    assert_eq!(options.native, NativeLanguage::Fr);
    assert_eq!(options.target, TargetEnglish::EnUs);
    assert_eq!(options.engine, EngineSlug::Harper);
    assert!(!options.local_thinking);
}

/// Spec section 4: `--thinking` wins over the Setting in both directions, so
/// a stored `false` never keeps `--thinking on` from running a Check with it.
#[test]
fn the_thinking_flag_wins_over_the_stored_setting_in_both_directions() {
    let cases = [
        (r#""localThinking": false"#, Thinking::On, true),
        (r#""localThinking": true"#, Thinking::Off, false),
    ];

    for (entry_body, flag, expected) in cases {
        let args = CheckArgs {
            thinking: Some(flag),
            ..no_flags()
        };
        let options = CheckOptions::resolve(&args, &stored(entry_body));

        assert_eq!(
            options.local_thinking, expected,
            "{entry_body} with {flag:?}"
        );
    }
}

/// A stored `false` is a value, not a missing key, so it must not read as the
/// default the way an unknown value does.
#[test]
fn thinking_off_is_read_from_the_file_and_on_is_the_default() {
    assert_eq!(
        stored(r#""localThinking": false"#).local_thinking,
        Some(false)
    );
    assert_eq!(
        stored(r#""localThinking": true"#).local_thinking,
        Some(true)
    );
    assert_eq!(stored("").local_thinking, None);
    // Any other JSON type is unknown, which reads as the default.
    assert_eq!(stored(r#""localThinking": "off""#).local_thinking, None);
    assert!(
        CheckOptions::resolve(&no_flags(), &stored(r#""localThinking": "off""#)).local_thinking
    );
}

#[test]
fn a_flag_wins_over_the_defaults_when_the_file_is_silent() {
    let options = CheckOptions::resolve(&all_flags(), &StoredSettings::default());

    assert_eq!(options.native, NativeLanguage::Fr);
    assert_eq!(options.engine, EngineSlug::Harper);
    assert!(!options.local_thinking);
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
           "engine": "gpt",
           "openaiBaseUrl": "",
           "openaiModel": "   ",
           "localThinking": "yes""#,
    );

    let options = CheckOptions::resolve(&no_flags(), &entry);

    assert_eq!(options, CheckOptions::default());
}

#[test]
fn a_stored_value_of_the_wrong_json_type_reads_as_the_default() {
    let entry =
        stored(r#""nativeLanguage": 7, "engine": true, "openaiModel": ["a"], "localThinking": 1"#);

    assert_eq!(
        CheckOptions::resolve(&no_flags(), &entry),
        CheckOptions::default()
    );
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

#[test]
fn an_empty_api_key_is_a_value_and_not_a_missing_key() {
    let entry = stored(r#""openaiApiKey": """#);

    assert_eq!(entry.openai_api_key.as_deref(), Some(""));
    assert_eq!(
        CheckOptions::resolve(&no_flags(), &entry).openai_api_key,
        ""
    );
}

// -------------------------------------------------- the cloud model id
//
// Spec section 7: `openrouterModel` has no built-in default. A blank field, a
// blank flag, and a missing key are one answer, and the adapter turns that
// answer into `bad_arguments`. The placeholder is what the Settings field
// shows and is never a value.

#[test]
fn the_cloud_model_has_no_built_in_default() {
    assert_eq!(DEFAULT_OPENROUTER_MODEL, "");
    assert_eq!(
        CheckOptions::resolve(&no_flags(), &StoredSettings::default()).openrouter_model,
        ""
    );
    assert_ne!(OPENROUTER_MODEL_PLACEHOLDER, DEFAULT_OPENROUTER_MODEL);
}

#[test]
fn a_blank_cloud_model_reads_the_way_a_missing_one_does() {
    for body in [
        r#""openrouterModel": """#,
        r#""openrouterModel": "   ""#,
        r#""openrouterModel": null"#,
    ] {
        let entry = stored(body);
        assert_eq!(entry.openrouter_model, None, "{body}");
        assert_eq!(
            CheckOptions::resolve(&no_flags(), &entry).openrouter_model,
            "",
            "{body}"
        );
    }
}

#[test]
fn a_blank_flag_falls_back_rather_than_winning_as_an_empty_id() {
    let entry = stored(r#""openrouterModel": "vendor/stored""#);
    let blank = CheckArgs {
        openrouter_model: Some("  ".to_string()),
        ..no_flags()
    };

    assert_eq!(
        CheckOptions::resolve(&blank, &entry).openrouter_model,
        "vendor/stored"
    );
    assert_eq!(
        CheckOptions::resolve(&blank, &StoredSettings::default()).openrouter_model,
        ""
    );
}

#[test]
fn the_flag_wins_over_the_stored_cloud_model() {
    let entry = stored(r#""openrouterModel": "vendor/stored""#);

    assert_eq!(
        CheckOptions::resolve(&all_flags(), &entry).openrouter_model,
        "vendor/flagged"
    );
}
