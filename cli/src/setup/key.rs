//! The OpenRouter key file, spec sections 4 and 10.
//!
//! `printf '%s' "$KEY" | grammachy setup --openrouter-key` is the one route a
//! key takes onto the machine. The key never lands in `shell.json` and never
//! reaches QML, so nothing that draws a card can read it, and no argument list
//! carries it where `ps` would show it.
//!
//! The directory is 0700 and the file is 0600. A write never reuses the inode
//! that is already there, because a file loosened by hand would hold the new
//! key at its old mode for as long as the write takes, and a descriptor opened
//! before a `chmod` keeps the access it was opened with. Every write therefore
//! makes a private file beside the target and renames it over, which is atomic
//! and leaves no readable window at all. The adapter in
//! [`crate::engines::openrouter`] reads the same path, and both sides take the
//! same test seam, so no test writes the real key.

use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Mode of `~/.config/grammachy/`.
pub const DIRECTORY_MODE: u32 = 0o700;

/// Mode of the key file itself.
pub const FILE_MODE: u32 = 0o600;

/// The command that writes the key, which every remedy names.
pub const WRITE_COMMAND: &str = r#"printf '%s' "$KEY" | grammachy setup --openrouter-key"#;

/// Where the key lives on this machine, with the adapter's test seam applied.
pub fn path() -> Option<PathBuf> {
    crate::engines::openrouter::Config::from_env().key_file
}

/// The key as it may be stored, or why the text on stdin is not a key.
///
/// A key is one token: OpenRouter keys carry no space, so text that holds one
/// is a pasted command line or a whole file rather than a key, and writing it
/// would earn a `rejected_key` on the next Check instead of a message here.
pub fn parse(stdin: &str) -> Result<String, String> {
    let key = stdin.trim();
    if key.is_empty() {
        return Err("No key on stdin. Run: ".to_string()
            + WRITE_COMMAND
            + ", with the key in $KEY.");
    }
    if key.chars().any(char::is_whitespace) {
        return Err("The text on stdin is not one key: it carries a space or a line break.".into());
    }
    Ok(key.to_string())
}

/// Where a write stages the key before it renames it over the target.
fn staging_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".new");
    path.with_file_name(name)
}

/// The mode of a path right now, or `None` when nothing is there.
fn mode_of(path: &Path) -> Option<u32> {
    std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o777)
}

/// Write the key, making the directory when it is not there.
///
/// Answers whether the file changed, so a second run of the same key reports
/// `unchanged` the way every other setup step does. A repaired mode is a
/// change too: the run tightened a key another user could read.
pub fn write(path: &Path, key: &str) -> Result<bool, String> {
    let contents = format!("{key}\n");
    let before = std::fs::read_to_string(path).ok();
    let before_mode = mode_of(path);

    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(DIRECTORY_MODE))
            .map_err(|error| {
                format!("{} could not be made private: {error}", directory.display())
            })?;
    }

    let staging = staging_of(path);
    stage(&staging, contents.as_bytes()).inspect_err(|_| {
        let _ = std::fs::remove_file(&staging);
    })?;
    std::fs::rename(&staging, path).map_err(|error| {
        let _ = std::fs::remove_file(&staging);
        format!("{} could not be written: {error}", path.display())
    })?;

    Ok(before.as_deref() != Some(contents.as_str()) || before_mode != Some(FILE_MODE))
}

/// Make a fresh private file at `path` and put `contents` in it.
///
/// The path is unlinked first and then created new, so the bytes never reach
/// an inode that was already there and no open descriptor survives the write.
/// `umask` can only clear bits of the creation mode, so the mode is set again
/// afterwards to make it exactly [`FILE_MODE`].
fn stage(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write;

    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{} could not be written: {error}", path.display())),
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(path)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(FILE_MODE))
        .map_err(|error| format!("{} could not be made private: {error}", path.display()))?;
    file.write_all(contents)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(())
}

/// Delete the key file. A file that is not there is not a failure.
pub fn remove(path: &Path) -> Result<bool, String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("{} could not be deleted: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_one_token() {
        assert_eq!(parse("  sk-or-v1-abc\n").unwrap(), "sk-or-v1-abc");
        assert!(parse("").is_err());
        assert!(parse("   \n").is_err());
        assert!(parse("export KEY=sk-or-v1-abc").is_err());
        assert!(parse("sk-or-v1-abc\nsk-or-v1-def").is_err());
    }

    #[test]
    fn the_empty_key_message_names_the_command() {
        let message = parse("").expect_err("empty stdin is not a key");
        assert!(
            message.contains("grammachy setup --openrouter-key"),
            "{message}"
        );
    }
}
