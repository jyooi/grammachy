//! The stored Settings layer, spec section 7.
//!
//! Storage is the plugin's entry in `~/.config/omarchy/shell.json`, written by
//! the overlay. The CLI only reads it: a missing file, a missing entry, or an
//! unknown stored value reads as the built-in default and nothing is ever
//! rewritten.
//!
//! The shell keeps a plugin entry either inline in `bar.layout.{left,center,
//! right}` when the plugin sits on the bar, or in the top level `plugins`
//! array otherwise. Both places are read, the bar layout first, because that is
//! the order `shell.qml` itself writes them in.

use std::path::PathBuf;

use serde_json::Value;

use crate::args::{EngineSlug, NativeLanguage, TargetEnglish};

/// The plugin id of `manifest.json`, which is also the entry id in `shell.json`.
pub const PLUGIN_ID: &str = "io.github.jyooi.grammachy";

/// Spec section 7 defaults for the two OpenAI text fields.
pub const DEFAULT_OPENAI_BASE_URL: &str = "http://127.0.0.1:8080";

/// The recommended local model of `docs/benchmarks/`, evals spec section 5.
///
/// `bench::weights` is the rule this name answers to: Apache-2.0 or MIT, a
/// weights file at or under 4 GB, inside the 8 GB tier, and measured with
/// thinking on. `gemma-4-e4b-it` scored well and is 4.98 GB, so it stays a
/// reference row of the benchmark files and is never the default.
pub const DEFAULT_OPENAI_MODEL: &str = "qwen3.8-4b";

/// Spec section 7: `openrouterModel` has no built-in default.
///
/// The cloud engine is the one engine that sends text off this machine, so a
/// model nobody chose is never asked for: a blank field, a blank flag, and a
/// missing key all resolve to this, and the adapter answers `bad_arguments`.
pub const DEFAULT_OPENROUTER_MODEL: &str = "";

/// What the `openrouterModel` field shows while it is empty, the recommended
/// cloud model of `docs/spec/evals.md` section 5.1 (HUF-206).
///
/// It is a placeholder and never a value: nothing reads it as a fallback.
/// `cli/tests/overlay_cloud.rs` keeps the copy in `ui/settings.js` equal to it.
pub const OPENROUTER_MODEL_PLACEHOLDER: &str = "google/gemini-3.7-flash";

/// Spec section 4: thinking is on by default for the local engine, everywhere.
pub const DEFAULT_LOCAL_THINKING: bool = true;

/// Points the CLI at another `shell.json`, so tests never read or write the
/// real one. Not a user-facing setting.
pub const PATH_ENV: &str = "GRAMMACHY_SHELL_JSON";

/// What the plugin entry says. `None` means the key is absent or unknown, so
/// the built-in default applies.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StoredSettings {
    pub native: Option<NativeLanguage>,
    pub target: Option<TargetEnglish>,
    pub engine: Option<EngineSlug>,
    pub openai_base_url: Option<String>,
    pub openai_model: Option<String>,
    pub openai_api_key: Option<String>,
    pub openrouter_model: Option<String>,
    pub local_thinking: Option<bool>,
}

impl StoredSettings {
    /// Read the entry from the `shell.json` this machine uses.
    ///
    /// Every failure answers empty settings, because a missing or broken file
    /// must not stop a Check.
    pub fn load() -> Self {
        match entry_path() {
            Some(path) => match std::fs::read_to_string(&path) {
                Ok(text) => Self::from_json(&text),
                Err(_) => StoredSettings::default(),
            },
            None => StoredSettings::default(),
        }
    }

    /// Read the entry out of one `shell.json` document.
    pub fn from_json(text: &str) -> Self {
        let Ok(document) = serde_json::from_str::<Value>(text) else {
            return StoredSettings::default();
        };
        match find_entry(&document) {
            Some(entry) => Self::from_entry(entry),
            None => StoredSettings::default(),
        }
    }

    fn from_entry(entry: &Value) -> Self {
        StoredSettings {
            native: string(entry, "nativeLanguage").and_then(NativeLanguage::from_stored),
            target: string(entry, "targetEnglish").and_then(TargetEnglish::from_stored),
            engine: string(entry, "engine").and_then(EngineSlug::from_stored),
            openai_base_url: stored_text(entry, "openaiBaseUrl"),
            openai_model: stored_text(entry, "openaiModel"),
            // The empty string is the default of the API key and also a
            // meaningful stored value, so it is kept as it stands.
            openai_api_key: string(entry, "openaiApiKey").map(str::to_string),
            openrouter_model: stored_text(entry, "openrouterModel"),
            local_thinking: boolean(entry, "localThinking"),
        }
    }
}

/// Where the shell keeps its configuration on this machine.
pub fn entry_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(PATH_ENV) {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".config/omarchy/shell.json"))
}

/// The plugin's entry, from the bar layout first and the `plugins` array next.
fn find_entry(document: &Value) -> Option<&Value> {
    for section in ["left", "center", "right"] {
        let entries = document
            .get("bar")
            .and_then(|bar| bar.get("layout"))
            .and_then(|layout| layout.get(section))
            .and_then(Value::as_array);
        if let Some(entry) = entries.and_then(|entries| entries.iter().find(|entry| is_ours(entry)))
        {
            return Some(entry);
        }
    }
    document
        .get("plugins")
        .and_then(Value::as_array)
        .and_then(|entries| entries.iter().find(|entry| is_ours(entry)))
}

fn is_ours(entry: &Value) -> bool {
    entry.get("id").and_then(Value::as_str) == Some(PLUGIN_ID)
}

/// A string value, or `None` for a missing key or any other JSON type.
fn string<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

/// A boolean value, or `None` for a missing key or any other JSON type, which
/// then reads as the built-in default the way an unknown value does.
fn boolean(entry: &Value, key: &str) -> Option<bool> {
    entry.get(key).and_then(Value::as_bool)
}

/// A stored text field that carries something, so a blank field reads as the
/// default instead of as an address or a model name that cannot work.
fn stored_text(entry: &Value, key: &str) -> Option<String> {
    non_empty(string(entry, key).unwrap_or_default()).map(str::to_string)
}

/// The one fallback rule every text setting shares: a value that carries
/// something, or `None`.
///
/// A blank flag, a blank field, and a missing key are the same answer, so the
/// layer above never has to tell them apart. `CheckOptions::resolve` reads the
/// flags through this and `stored_text` reads the file through it.
pub fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
