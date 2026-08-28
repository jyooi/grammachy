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
    pub quick_hotkey: Option<String>,
    pub compose_hotkey: Option<String>,
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
            quick_hotkey: hotkey(entry, "quickHotkey"),
            compose_hotkey: hotkey(entry, "composeHotkey"),
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

/// A hotkey string, trimmed. Blank or missing reads as unknown, so the
/// default of spec section 2 applies. The CLI never parses Hyprland key
/// syntax beyond that.
fn hotkey(entry: &Value, key: &str) -> Option<String> {
    string(entry, key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
