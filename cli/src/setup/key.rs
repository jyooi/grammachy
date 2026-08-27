//! The OpenRouter key file, spec sections 4 and 10.
//!
//! `printf '%s' "$KEY" | grammachy setup --openrouter-key` is the one route a
//! key takes onto the machine. The key never lands in `shell.json` and never
//! reaches QML, so nothing that draws a card can read it, and no argument list
//! carries it where `ps` would show it.
//!
//! The directory is 0700 and the file is 0600, set on every run rather than
//! only at creation, because a file that is already there keeps the mode it
//! was made with. The adapter in [`crate::engines::openrouter`] reads the same
//! path, and both sides take the same test seam, so no test writes the real
//! key.

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

/// Write the key, making the directory when it is not there.
///
/// Answers whether the file changed, so a second run of the same key reports
/// `unchanged` the way every other setup step does.
pub fn write(path: &Path, key: &str) -> Result<bool, String> {
    let contents = format!("{key}\n");
    let before = std::fs::read_to_string(path).ok();

    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("{} could not be made: {error}", directory.display()))?;
        std::fs::set_permissions(directory, std::fs::Permissions::from_mode(DIRECTORY_MODE))
            .map_err(|error| {
                format!("{} could not be made private: {error}", directory.display())
            })?;
    }

    // The mode is given at creation and set again after it, because an
    // existing file keeps the mode it was made with.
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(path)
            .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
        file.write_all(contents.as_bytes())
            .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(FILE_MODE))
        .map_err(|error| format!("{} could not be made private: {error}", path.display()))?;

    Ok(before.as_deref() != Some(contents.as_str()))
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
