//! The two hotkeys of spec section 2, written into `~/.config/hypr/bindings.lua`.
//!
//! Omarchy configures Hyprland in Lua: `hyprctl systeminfo` on a current
//! install answers `configProvider: lua`, so `hyprland.lua` is the entry point
//! and the `.conf` files beside it are never read. The block therefore holds
//! Lua, and it uses the two helpers that file is written around: `hl.unbind`
//! first, because SUPER + SHIFT + G carries an Omarchy default, then `o.bind`,
//! which is what puts the description into `omarchy menu keybindings`.
//!
//! The command is a Lua long bracket string, `[[...]]`, because the payload
//! already carries both single and double quotes and a long bracket needs no
//! escape at all.
//!
//! Hyprland only reads a changed file when it is told to, so `grammachy setup`
//! runs `hyprctl reload` after writing. That call is a value here, because no
//! test may talk to the running compositor.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::PLUGIN_ID;
use crate::setup::block::{self, Anchor, Block};

/// Points the CLI at another `bindings.lua`. The test suite sets it, so no
/// test writes the real file. Not a user-facing setting.
pub const PATH_ENV: &str = "GRAMMACHY_BINDINGS_LUA";

/// Keeps `setup` from reloading the compositor. Tests and CI set it to
/// `never`. Not a user-facing setting.
pub const RELOAD_ENV: &str = "GRAMMACHY_HYPRCTL_RELOAD";

/// What reloads Hyprland once the block is written.
///
/// The real one runs `hyprctl reload`. Tests hand in their own.
pub type Reloader = Box<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Where Hyprland keeps the user's bindings on this machine.
///
/// The product path is `$HOME` only, the same rule the Settings file follows
/// (spec section 7), so `XDG_CONFIG_HOME` is not read.
pub fn path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(PATH_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".config/hypr/bindings.lua"))
}

/// One hotkey: the default it replaces goes first, the new binding second.
fn binding(keys: &str, description: &str, payload: &str) -> String {
    format!(
        "hl.unbind(\"{keys}\")\n\
         o.bind(\"{keys}\", \"{description}\", \
         [[omarchy-shell shell summon {PLUGIN_ID} '{payload}']])\n"
    )
}

/// The block spec section 10 puts between the two markers.
pub fn block() -> Block {
    Block {
        markers: block::LUA,
        body: format!(
            "{}{}",
            binding("SUPER + G", "Grammachy", r#"{"mode":"quick"}"#),
            binding(
                "SUPER + SHIFT + G",
                "Grammachy compose",
                r#"{"mode":"compose"}"#
            ),
        ),
    }
}

/// Write the block, answering whether the file changed.
pub fn install(path: &Path) -> Result<bool, String> {
    write(path, |content| block().ensure(content, Anchor::EndOfFile))
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

/// The file as it stands. A file that is not there yet reads as empty, because
/// a fresh Omarchy install may carry no user bindings at all.
fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {error}", path.display())),
    }
}

/// The reloader this run uses, honouring the `never` test seam.
pub fn reloader_from_env() -> Reloader {
    if std::env::var_os(RELOAD_ENV).is_some_and(|value| value == "never") {
        return Box::new(|| Ok(()));
    }
    Box::new(reload)
}

/// Tell the running compositor to read its configuration again.
///
/// No compositor is a fact rather than a failure: `grammachy setup` runs from a
/// terminal that may not be a Hyprland session at all, and the block is already
/// on disk by then. The caller decides what to make of the error.
pub fn reload() -> Result<(), String> {
    let output = Command::new("hyprctl").arg("reload").output();
    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(format!(
            "hyprctl reload failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
        Err(error) => Err(format!("hyprctl could not run: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_bindings_are_the_ones_spec_section_2_names() {
        let body = block().body;

        assert_eq!(
            body,
            "hl.unbind(\"SUPER + G\")\n\
             o.bind(\"SUPER + G\", \"Grammachy\", \
             [[omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"quick\"}']])\n\
             hl.unbind(\"SUPER + SHIFT + G\")\n\
             o.bind(\"SUPER + SHIFT + G\", \"Grammachy compose\", \
             [[omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"compose\"}']])\n"
        );
    }

    #[test]
    fn the_payload_needs_no_escape_inside_the_long_bracket() {
        // A `]]` in the command would close the long bracket early. The two
        // payloads carry none, and this is the guard if one ever does.
        for line in block().body.lines().filter(|line| line.contains("[[")) {
            let command = line
                .split_once("[[")
                .and_then(|(_, rest)| rest.split_once("]]"))
                .expect("every long bracket is closed");
            assert!(!command.0.contains("]]"), "{}", command.0);
        }
    }
}
