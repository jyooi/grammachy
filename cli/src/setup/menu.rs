//! The `apps.grammachy` row in the Omarchy menu, spec sections 2 and 10.
//!
//! The extension file is JSONC whose object keys are the row ids. Omarchy
//! infers a row's parent from its dotted id, so `apps.grammachy` sits under
//! Apps. The Apps submenu is provider-driven. Static children declared in
//! JSONC survive provider refreshes.
//!
//! The action sends `{"mode":"quick"}`, the same payload as the quick
//! hotkey, so `Overlay.open()` shows the default surface.
//!
//! The block sits directly inside the opening brace, so the new member never
//! needs a comma in front of it and the members already in the file keep their
//! own punctuation. The trailing comma the member carries is what the Omarchy
//! menu parser strips before it reads the JSON.

use std::path::{Path, PathBuf};

use crate::settings::PLUGIN_ID;
use crate::setup::block::{self, Anchor, Block};

/// Points the CLI at another menu extension. The test suite sets it, so no
/// test writes the real file. Not a user-facing setting.
pub const PATH_ENV: &str = "GRAMMACHY_MENU_JSONC";

/// The row id spec section 10 fixes.
pub const ENTRY_ID: &str = "apps.grammachy";

/// Nerd Font `md-spellcheck`, shown in the Apps submenu icon column.
const ENTRY_ICON: char = '\u{f04c6}';

/// What an extension file holds when `setup` has to create one.
const EMPTY_DOCUMENT: &str = "{\n}\n";

/// Where Omarchy keeps the user's menu extension on this machine.
pub fn path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(PATH_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".config/omarchy/extensions/omarchy-menu.jsonc"))
}

/// The block spec section 10 puts between the two markers.
pub fn block() -> Block {
    // The payload is JSON inside a JSON string inside a single quoted shell
    // word, so the quotes are escaped once for the menu file and the shell
    // takes the rest as it stands.
    let action = format!("omarchy-shell shell summon {PLUGIN_ID} '{{\\\"mode\\\":\\\"quick\\\"}}'");
    Block {
        markers: block::JSONC,
        body: format!(
            "  \"{ENTRY_ID}\": {{\"icon\":\"{ENTRY_ICON}\",\"label\":\"Grammachy\",\
             \"action\":\"{action}\"}},\n"
        ),
    }
}

/// Write the block, answering whether the file changed.
pub fn install(path: &Path) -> Result<bool, String> {
    write(path, |content| {
        block().ensure(content, Anchor::InsideOpeningBrace)
    })
}

/// Take the block out, answering whether the file changed.
pub fn remove(path: &Path) -> Result<bool, String> {
    write(path, |content| Ok(block().strip(content)))
}

fn write(path: &Path, change: impl FnOnce(&str) -> Result<String, String>) -> Result<bool, String> {
    let before = read(path)?;
    let after = change(&before)?;
    if after == before {
        return Ok(false);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} could not be created: {error}", parent.display()))?;
    }
    std::fs::write(path, &after)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(true)
}

/// The file as it stands, or the empty object a fresh install would carry.
fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(EMPTY_DOCUMENT.to_string())
        }
        Err(error) => Err(format!("{} could not be read: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the Omarchy menu parser does before it reads the JSON: drop a
    /// whole comment line, then drop a comma that only a closing bracket
    /// follows. `MenuModel.js` in the Omarchy shell is the original.
    fn parse(content: &str) -> serde_json::Value {
        let without_comments: String = content
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        let bytes: Vec<char> = without_comments.chars().collect();
        let mut stripped = String::with_capacity(without_comments.len());
        for (at, character) in bytes.iter().enumerate() {
            if *character == ',' {
                let next = bytes[at + 1..]
                    .iter()
                    .find(|candidate| !candidate.is_whitespace());
                if matches!(next, Some('}') | Some(']')) {
                    continue;
                }
            }
            stripped.push(*character);
        }

        serde_json::from_str(&stripped).expect("the extension file stays valid JSON")
    }

    #[test]
    fn the_written_file_is_still_readable_json() {
        let with = block()
            .ensure("{\n  // a comment\n}\n", Anchor::InsideOpeningBrace)
            .unwrap();

        let document = parse(&with);

        assert!(document[ENTRY_ID].get("parent").is_none());
        assert_eq!(document[ENTRY_ID]["label"], "Grammachy");
        assert_eq!(document[ENTRY_ID]["icon"], ENTRY_ICON.to_string());
        assert_eq!(
            document[ENTRY_ID]["action"],
            "omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"quick\"}'"
        );
    }

    #[test]
    fn an_existing_member_keeps_its_punctuation() {
        let original = "{\n  \"personal\": {\"label\": \"Personal\"}\n}\n";

        let with = block()
            .ensure(original, Anchor::InsideOpeningBrace)
            .unwrap();

        let document = parse(&with);
        assert_eq!(document["personal"]["label"], "Personal");
        assert_eq!(document[ENTRY_ID]["label"], "Grammachy");
    }

    #[test]
    fn a_second_run_leaves_one_entry() {
        let once = block()
            .ensure("{\n}\n", Anchor::InsideOpeningBrace)
            .unwrap();
        let twice = block().ensure(&once, Anchor::InsideOpeningBrace).unwrap();

        assert_eq!(once, twice);
        assert_eq!(twice.matches(ENTRY_ID).count(), 1);
    }

    #[test]
    fn an_old_compose_block_is_replaced_by_the_apps_row() {
        let old = Block {
            markers: block::JSONC,
            body: "  \"grammachy.compose\": {\"icon\":\"\",\"label\":\"Grammachy compose\",\
             \"parent\":\"root\",\"action\":\"omarchy-shell shell summon io.github.jyooi.grammachy '{\\\"mode\\\":\\\"compose\\\"}'\"},\n"
                .to_string(),
        };
        let original = "{\n  // a comment\n}\n";
        let with_old = old.ensure(original, Anchor::InsideOpeningBrace).unwrap();
        assert!(with_old.contains("grammachy.compose"));

        let with_new = block()
            .ensure(&with_old, Anchor::InsideOpeningBrace)
            .unwrap();
        let document = parse(&with_new);

        assert!(document.get("grammachy.compose").is_none());
        assert_eq!(document[ENTRY_ID]["label"], "Grammachy");
        assert!(document[ENTRY_ID].get("parent").is_none());
        assert_eq!(
            document[ENTRY_ID]["action"],
            "omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"quick\"}'"
        );
        assert_eq!(with_new.matches("// grammachy begin").count(), 1);
    }
}
