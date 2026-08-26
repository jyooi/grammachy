//! How much room is left for a weights file, spec section 5.3.
//!
//! A model is gigabytes, so the Models list says what the disk has before the
//! user asks for one, and `model download` refuses rather than filling the disk
//! and failing on the last byte.
//!
//! The models directory may not exist yet, and `statvfs` answers only for a
//! path that does. So the walk goes up to the nearest ancestor that is there,
//! which is on the same file system as the directory that will be created.

use std::path::Path;

/// Free bytes on the file system that holds this path, or `None` when no
/// ancestor of it could be asked.
///
/// The number is the unprivileged one, `f_bavail`, because the reserved blocks
/// of a file system are not room a download may use.
pub fn free_bytes(path: &Path) -> Option<u64> {
    let existing = nearest_existing(path)?;
    statvfs_available(&existing)
}

/// The nearest ancestor of this path that exists, itself included.
fn nearest_existing(path: &Path) -> Option<std::path::PathBuf> {
    let mut candidate = path;
    loop {
        if candidate.exists() {
            return Some(candidate.to_path_buf());
        }
        candidate = candidate.parent()?;
    }
}

#[cfg(unix)]
fn statvfs_available(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let name = CString::new(path.as_os_str().as_bytes()).ok()?;
    // Safety: `statvfs` fills the struct it is handed and reads the C string
    // only for the length of this call. A failed call leaves it untouched,
    // which is why the return value is checked before anything is read.
    let stats = unsafe {
        let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if libc::statvfs(name.as_ptr(), stats.as_mut_ptr()) != 0 {
            return None;
        }
        stats.assume_init()
    };

    let block = u64::from(stats.f_frsize as u32).max(1);
    Some(block.saturating_mul(stats.f_bavail as u64))
}

/// What a transfer still needs and what the disk has for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortfall {
    /// Bytes still to fetch.
    pub remaining: u64,
    /// Bytes the file system has free.
    pub free: u64,
}

/// Why this transfer does not fit, or `None` when it does.
///
/// A resumed transfer never rewrites what the `.part` file already holds, so
/// only the missing bytes are asked for. A disk that cannot be measured at all
/// is not a refusal: guessing that there is no room would stop a download that
/// would have worked.
pub fn shortfall(size_bytes: u64, already: u64, free: Option<u64>) -> Option<Shortfall> {
    let remaining = size_bytes.saturating_sub(already);
    let free = free?;
    if free >= remaining {
        return None;
    }
    Some(Shortfall { remaining, free })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one thing worth asserting without knowing the machine: a real
    /// directory answers, and a path under it that does not exist yet answers
    /// the same number, because it will be created on the same file system.
    #[test]
    fn a_directory_that_does_not_exist_yet_answers_for_its_parent() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let missing = here.join("no-such-directory/nor-this-one");

        let free = free_bytes(here).expect("the crate directory is on a file system");
        assert!(free > 0);
        assert_eq!(free_bytes(&missing), Some(free));
    }

    #[test]
    fn a_resumed_transfer_only_asks_for_the_bytes_it_still_needs() {
        // Half the file is already on disk, and the disk holds the other half.
        assert_eq!(shortfall(1_000, 500, Some(500)), None);
        assert_eq!(
            shortfall(1_000, 0, Some(500)),
            Some(Shortfall {
                remaining: 1_000,
                free: 500
            })
        );
    }

    #[test]
    fn a_file_already_whole_needs_nothing_and_a_disk_nobody_could_measure_refuses_nothing() {
        assert_eq!(shortfall(1_000, 1_000, Some(0)), None);
        assert_eq!(shortfall(1_000, 2_000, Some(0)), None);
        assert_eq!(shortfall(1_000, 0, None), None);
    }

    #[test]
    fn the_nearest_existing_ancestor_is_the_one_asked() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));

        assert_eq!(nearest_existing(here).as_deref(), Some(here));
        assert_eq!(
            nearest_existing(&here.join("a/b/c")).as_deref(),
            Some(here),
            "the walk stops at the first ancestor that is there"
        );
    }
}
