//! The two hotkeys of spec section 2, written into `~/.config/hypr/bindings.conf`.
//!
//! Both lines are `bindd` because Omarchy lists every description in
//! `omarchy menu keybindings`. The payload is single quoted so Hyprland hands
//! the JSON to the shell unchanged; it carries no `#`, which Hyprland would
//! read as the start of a comment.
//!
//! Hyprland only reads a changed file when it is told to, so `grammachy setup`
//! runs `hyprctl reload` after writing. That call is a value here, because no
//! test may talk to the running compositor.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::PLUGIN_ID;
use crate::setup::block::{self, Anchor, Block};

/// Points the CLI at another `bindings.conf`. The test suite sets it, so no
/// test writes the real file. Not a user-facing setting.
pub const PATH_ENV: &str = "GRAMMACHY_BINDINGS_CONF";

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
    Some(PathBuf::from(home).join(".config/hypr/bindings.conf"))
}

/// The one command both hotkeys run, with the payload of spec section 2.
fn summon(payload: &str) -> String {
    format!("omarchy-shell shell summon {PLUGIN_ID} '{payload}'")
}

/// The block spec section 10 puts between the two markers.
pub fn block() -> Block {
    let quick = summon(r#"{"mode":"quick"}"#);
    let compose = summon(r#"{"mode":"compose"}"#);
    Block {
        markers: block::HYPRLAND,
        body: format!(
            "bindd = SUPER, G, Grammachy, exec, {quick}\n\
             bindd = SUPER SHIFT, G, Grammachy compose, exec, {compose}\n"
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
/// No compositor is a fact, not a failure: `grammachy setup` runs from a
/// terminal that may not be a Hyprland session at all, and the block is
/// already on disk by then.
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
    fn the_two_lines_are_the_ones_spec_section_2_names() {
        let body = block().body;

        assert_eq!(
            body,
            "bindd = SUPER, G, Grammachy, exec, omarchy-shell shell summon \
             io.github.jyooi.grammachy '{\"mode\":\"quick\"}'\n\
             bindd = SUPER SHIFT, G, Grammachy compose, exec, omarchy-shell shell summon \
             io.github.jyooi.grammachy '{\"mode\":\"compose\"}'\n"
        );
    }

    #[test]
    fn the_payload_carries_no_hyprland_comment_character() {
        assert!(!block().body.contains('#'));
    }
}
