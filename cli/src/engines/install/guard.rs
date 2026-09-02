//! The checks on every path the install writes, made before the write.
//!
//! The engines directory is under HOME, and `curl --output` follows a
//! symbolic link that stands where the `.part` file goes. So nothing here
//! writes to a path it did not look at first. A path the install writes must
//! be absent, or a plain file or directory this user owns. A symbolic link,
//! a device, or another user's file refuses the install and names itself.
//!
//! The directory the unpack fills is made with an exclusive create, so a
//! directory that appears between the removal and the create refuses too.
//!
//! What the checks cannot close is the moment between a check and the write
//! that follows it. Only a program that runs as this user can act in that
//! moment, and such a program already owns the directory outright.

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The user this process runs as.
fn own_uid() -> u32 {
    // Safety: geteuid takes no arguments and cannot fail.
    unsafe { libc::geteuid() }
}

/// Make sure `directory` is a plain directory this user owns, and make it
/// when it is absent.
pub fn private_directory(directory: &Path) -> Result<(), String> {
    let data = match fs::symlink_metadata(directory) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(directory).map_err(|error| {
                format!("{} could not be created: {error}", directory.display())
            })?;
            fs::symlink_metadata(directory)
                .map_err(|error| format!("{} could not be read: {error}", directory.display()))?
        }
        Err(error) => {
            return Err(format!(
                "{} could not be read: {error}",
                directory.display()
            ))
        }
    };
    if data.file_type().is_symlink() {
        return Err(format!(
            "{} is a symbolic link, and the install writes only into a plain directory it owns.",
            directory.display()
        ));
    }
    if !data.is_dir() {
        return Err(format!("{} is not a directory.", directory.display()));
    }
    owned_here(directory, data.uid())
}

/// Refuse anything at `path` but a plain file this user owns. An absent path
/// passes.
pub fn plain_file_or_absent(path: &Path) -> Result<(), String> {
    let data = match fs::symlink_metadata(path) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{} could not be read: {error}", path.display())),
    };
    if data.file_type().is_symlink() {
        return Err(format!(
            "{} is a symbolic link, and the install writes only plain files. Remove it before the next install.",
            path.display()
        ));
    }
    if !data.is_file() {
        return Err(format!(
            "{} is not a plain file. Remove it before the next install.",
            path.display()
        ));
    }
    owned_here(path, data.uid())
}

/// Make an empty plain file at `path` when nothing is there. The transfer
/// then appends to a file this process made, not to whatever it finds.
pub fn create_if_absent(path: &Path) -> Result<(), String> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => plain_file_or_absent(path),
        Err(error) => Err(format!("{} could not be created: {error}", path.display())),
    }
}

/// An empty directory at `path` that this call made.
///
/// Whatever stood there goes first, link or tree, and the create is
/// exclusive, so the directory is one nothing else put in place.
pub fn fresh_directory(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => match fs::remove_file(path) {
            Ok(()) => {}
            Err(_) => return Err(format!("{} could not be removed: {error}", path.display())),
        },
    }
    fs::create_dir(path)
        .map_err(|error| format!("{} could not be created: {error}", path.display()))
}

fn owned_here(path: &Path, uid: u32) -> Result<(), String> {
    if uid == own_uid() {
        Ok(())
    } else {
        Err(format!(
            "{} is owned by user {uid}, not by this one.",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("grammachy-install-guard-{name}"));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("the scratch directory is created");
        directory
    }

    #[test]
    fn a_symbolic_link_is_refused_wherever_the_install_writes() {
        let directory = scratch("links");
        let target = directory.join("elsewhere");
        fs::write(&target, b"someone else's file").unwrap();
        let link = directory.join("archive.zip.part");
        symlink(&target, &link).unwrap();
        let linked_directory = directory.join("engines");
        symlink(&directory, &linked_directory).unwrap();

        let refused = plain_file_or_absent(&link).expect_err("a link");
        assert!(refused.contains("symbolic link"), "{refused}");
        let refused = create_if_absent(&link).expect_err("a link");
        assert!(refused.contains("symbolic link"), "{refused}");
        let refused = private_directory(&linked_directory).expect_err("a link");
        assert!(refused.contains("symbolic link"), "{refused}");
        assert_eq!(fs::read(&target).unwrap(), b"someone else's file");
    }

    /// A dangling link answers `NotFound` on a follow and `AlreadyExists` on
    /// a create, so the check has to read the link itself first.
    #[test]
    fn a_dangling_link_at_the_directory_is_named_as_a_link() {
        let directory = scratch("dangling");
        let linked_directory = directory.join("engines");
        symlink(directory.join("gone"), &linked_directory).unwrap();

        let refused = private_directory(&linked_directory).expect_err("a dangling link");

        assert!(refused.contains("symbolic link"), "{refused}");
        assert!(!directory.join("gone").exists(), "nothing was created");
    }

    #[test]
    fn a_plain_file_this_user_owns_passes_and_an_absent_one_is_made() {
        let directory = scratch("plain");
        let partial = directory.join("archive.zip.part");

        plain_file_or_absent(&partial).expect("absent");
        create_if_absent(&partial).expect("made");
        assert!(fs::symlink_metadata(&partial).unwrap().is_file());
        fs::write(&partial, b"some bytes").unwrap();
        create_if_absent(&partial).expect("already there and plain");
        assert_eq!(fs::read(&partial).unwrap(), b"some bytes");

        let not_plain = directory.join("dir");
        fs::create_dir(&not_plain).unwrap();
        let refused = plain_file_or_absent(&not_plain).expect_err("a directory");
        assert!(refused.contains("not a plain file"), "{refused}");
    }

    #[test]
    fn a_fresh_directory_replaces_a_link_a_file_or_a_tree() {
        let directory = scratch("fresh");
        let staging = directory.join("staging");

        fresh_directory(&staging).expect("absent");
        fs::write(staging.join("leftover"), b"x").unwrap();
        fresh_directory(&staging).expect("a tree");
        assert!(fs::read_dir(&staging).unwrap().next().is_none());

        // The removal must not follow the link and empty its target.
        let target = directory.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("marker"), b"keep me").unwrap();
        fs::remove_dir(&staging).unwrap();
        symlink(&target, &staging).unwrap();
        fresh_directory(&staging).expect("a link");
        assert!(!fs::symlink_metadata(&staging)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(target.join("marker")).unwrap(), b"keep me");

        fs::remove_dir(&staging).unwrap();
        fs::write(&staging, b"a file").unwrap();
        fresh_directory(&staging).expect("a file");
        assert!(fs::symlink_metadata(&staging).unwrap().is_dir());
    }
}
