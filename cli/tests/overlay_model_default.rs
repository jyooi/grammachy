//! The recommended local model is one name that lives in four places.
//!
//! `docs/spec/evals.md` section 5 decides it from the benchmark tables, and
//! `settings::DEFAULT_OPENAI_MODEL` is what the CLI answers with. The overlay
//! carries its own copies, because no QML can ask the CLI for a default before
//! a Check: `ui/settings.js` holds the descriptor fallback, `ui/SettingsView.qml`
//! holds the property the view draws, and `manifest.json` holds the two the
//! shell stores. `Overlay.qml` cannot be instantiated outside the shell's
//! plugin loader, so reading the QML source is what keeps those copies in step.
//! The manifest is a document the shell parses, so it is parsed here too.
//!
//! The case at the end is the other half: the name has to be a row the rules
//! of `bench::weights` allow. A default nobody may recommend would be a
//! product bug the parity cases alone would pass.

use grammachy::bench::weights::{self, Terms};
use grammachy::settings::DEFAULT_OPENAI_MODEL;

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

#[test]
fn every_copy_of_the_local_model_default_names_the_same_row() {
    let model = DEFAULT_OPENAI_MODEL;

    assert!(
        read("ui/settings.js").contains(&format!(
            r#"openaiModel: {{ type: "string", fallback: "{model}" }}"#
        )),
        "ui/settings.js names {model}"
    );
    assert!(
        read("ui/SettingsView.qml").contains(&format!(r#"property string openaiModel: "{model}""#)),
        "ui/SettingsView.qml names {model}"
    );

    // The manifest is a machine-consumed document, so it is read as one rather
    // than as text: a reformat or a reordered key must not change what passes.
    let manifest: serde_json::Value =
        serde_json::from_str(&read("manifest.json")).expect("manifest.json is JSON");
    assert_eq!(
        manifest["barWidget"]["defaults"]["openaiModel"],
        serde_json::json!(model),
        "the manifest defaults name {model}"
    );
    let schema = manifest["barWidget"]["schema"]
        .as_array()
        .expect("the manifest schema is a list of descriptors");
    let descriptor = schema
        .iter()
        .find(|entry| entry["key"] == serde_json::json!("openaiModel"))
        .expect("the manifest schema holds an openaiModel descriptor");
    assert_eq!(
        descriptor["defaultValue"],
        serde_json::json!(model),
        "the manifest schema names {model}"
    );
}

/// Evals spec section 5: the Settings default is the recommended local model,
/// so it has to clear the bars a recommended row clears.
#[test]
fn the_local_model_default_clears_the_recommendation_bars() {
    let model = DEFAULT_OPENAI_MODEL;
    let licence = weights::of(model);

    assert_eq!(licence.terms, Terms::Permissive, "{model} is Apache or MIT");
    assert_eq!(licence.objection(), None, "{model}");

    let file_bytes = grammachy::model::catalogue_size_bytes(model);
    assert!(
        file_bytes.is_some(),
        "{model} is a catalogue row, so Settings, Models can download it"
    );
    assert_eq!(
        weights::file_objection(file_bytes),
        None,
        "{model} is at or under the {} GB file ceiling",
        weights::FILE_CEILING_GB
    );
}

/// The cloud default is the `openrouterModel` line, and cloud is never the
/// default engine (evals spec sections 5 and 5.1).
#[test]
fn the_cloud_default_is_a_model_id_and_never_the_default_engine() {
    assert!(
        grammachy::settings::DEFAULT_OPENROUTER_MODEL.contains('/'),
        "a cloud default is a provider-qualified id"
    );
    assert_ne!(
        grammachy::args::CheckOptions::default().engine,
        grammachy::args::EngineSlug::Openrouter,
        "cloud is never the default engine"
    );
}
