//! What a downloaded archive holds, read before anything is unpacked.
//!
//! The digest pin says the bytes are the release the row names. This module
//! is the second guard: it reads the central directory of the zip first. It
//! refuses a member that would land outside the pinned tree or is a symbolic
//! link. It refuses an archive whose members or unpacked bytes exceed what
//! the row pins. So `bsdtar` never runs on an archive of unknown shape, and
//! the disk never takes more than the row says it will.
//!
//! The reader is the plain zip central directory (APPNOTE 4.3.7 and 4.3.12).
//! ZIP64 is refused. The pinned release does not use it, and a reader that
//! stops at 4 GiB and 65,535 members cannot be talked past them.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// What one row allows an archive to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    /// The one top-level directory every member must sit under.
    pub directory_name: String,
    /// The most members the archive may hold.
    pub max_members: u64,
    /// The most bytes the members may unpack to, added together.
    pub max_unpacked_bytes: u64,
}

/// What the central directory says the archive holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inventory {
    pub members: u64,
    pub unpacked_bytes: u64,
}

const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const CENTRAL_FILE_HEADER: u32 = 0x0201_4b50;
const END_RECORD_LEN: usize = 22;
const CENTRAL_HEADER_LEN: usize = 46;
/// The zip comment is at most 65,535 bytes, so the end record is within this
/// many bytes of the end of the file.
const END_SEARCH_LEN: u64 = 65_535 + END_RECORD_LEN as u64;
/// The unix file type bits in the high half of the external attributes.
const UNIX_TYPE_MASK: u32 = 0o170_000;
const UNIX_SYMLINK: u32 = 0o120_000;

/// Read the archive on disk and admit it, or say why not.
pub fn admit(archive: &Path, admission: &Admission) -> Result<Inventory, String> {
    let bytes = central_directory(archive)?;
    admit_central_directory(&bytes, admission)
}

/// The central directory bytes of one zip file.
fn central_directory(archive: &Path) -> Result<Vec<u8>, String> {
    let mut file = File::open(archive)
        .map_err(|error| format!("{} could not be opened: {error}", archive.display()))?;
    let length = file
        .metadata()
        .map_err(|error| format!("{} could not be read: {error}", archive.display()))?
        .len();

    let tail_start = length.saturating_sub(END_SEARCH_LEN);
    let mut tail = Vec::new();
    file.seek(SeekFrom::Start(tail_start))
        .and_then(|_| file.read_to_end(&mut tail))
        .map_err(|error| format!("{} could not be read: {error}", archive.display()))?;

    let end = end_record(&tail).ok_or_else(|| {
        format!(
            "{} is not a zip archive: no end of central directory record.",
            archive.display()
        )
    })?;

    let mut bytes = vec![0u8; end.size as usize];
    file.seek(SeekFrom::Start(end.offset))
        .and_then(|_| file.read_exact(&mut bytes))
        .map_err(|error| {
            format!(
                "{} could not be read at its central directory: {error}",
                archive.display()
            )
        })?;
    Ok(bytes)
}

struct EndRecord {
    size: u32,
    offset: u64,
}

/// The last end of central directory record in this tail, if there is one.
fn end_record(tail: &[u8]) -> Option<EndRecord> {
    if tail.len() < END_RECORD_LEN {
        return None;
    }
    let start = (0..=tail.len() - END_RECORD_LEN)
        .rev()
        .find(|&at| u32_at(tail, at) == END_OF_CENTRAL_DIRECTORY)?;
    let record = &tail[start..];
    Some(EndRecord {
        size: u32_at(record, 12),
        offset: u64::from(u32_at(record, 16)),
    })
}

/// Walk the central directory and admit what it lists.
///
/// This is the pure half: it takes the bytes rather than a path, so a test can
/// hand it a directory it built by hand.
pub fn admit_central_directory(bytes: &[u8], admission: &Admission) -> Result<Inventory, String> {
    let mut at = 0usize;
    let mut inventory = Inventory {
        members: 0,
        unpacked_bytes: 0,
    };

    while at < bytes.len() {
        if bytes.len() - at < CENTRAL_HEADER_LEN || u32_at(bytes, at) != CENTRAL_FILE_HEADER {
            return Err("The archive central directory is malformed.".to_string());
        }
        let header = &bytes[at..];
        let compressed = u32_at(header, 20);
        let unpacked = u32_at(header, 24);
        let name_len = usize::from(u16_at(header, 28));
        let extra_len = usize::from(u16_at(header, 30));
        let comment_len = usize::from(u16_at(header, 32));
        let external = u32_at(header, 38);
        let local_offset = u32_at(header, 42);

        if compressed == u32::MAX || unpacked == u32::MAX || local_offset == u32::MAX {
            return Err("The archive uses ZIP64, which this install does not read.".to_string());
        }

        let name_start = at + CENTRAL_HEADER_LEN;
        let name_end = name_start + name_len;
        if name_end > bytes.len() {
            return Err("The archive central directory is malformed.".to_string());
        }
        let name = std::str::from_utf8(&bytes[name_start..name_end])
            .map_err(|_| "The archive names a member that is not UTF-8.".to_string())?;

        admit_name(name, &admission.directory_name)?;
        if external >> 16 & UNIX_TYPE_MASK == UNIX_SYMLINK {
            return Err(format!(
                "The archive holds a symbolic link, {name}, which this install refuses."
            ));
        }

        inventory.members += 1;
        inventory.unpacked_bytes = inventory.unpacked_bytes.saturating_add(u64::from(unpacked));
        if inventory.members > admission.max_members {
            return Err(format!(
                "The archive holds more than {} members, which is more than the pinned release.",
                admission.max_members
            ));
        }
        if inventory.unpacked_bytes > admission.max_unpacked_bytes {
            return Err(format!(
                "The archive unpacks to more than {} bytes, which is more than the pinned release.",
                admission.max_unpacked_bytes
            ));
        }

        at = name_end + extra_len + comment_len;
    }

    Ok(inventory)
}

/// A member path is relative, has no `..`, and sits under the pinned directory.
fn admit_name(name: &str, directory_name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains('\0') {
        return Err(format!(
            "The archive names a member outside the pinned tree: {name:?}."
        ));
    }
    let top = name.split('/').next().unwrap_or_default();
    if top != directory_name {
        return Err(format!(
            "The archive names a member outside {directory_name}/: {name:?}."
        ));
    }
    if name.split('/').any(|part| part == "..") {
        return Err(format!(
            "The archive names a member that climbs out of the tree: {name:?}."
        ));
    }
    Ok(())
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Build one stored zip in memory, for tests that need a real archive.
///
/// Each member is `(name, bytes, unix mode)`. The CRC is not computed, because
/// nothing here reads member data. The central directory is what the reader
/// walks, and these bytes hold it correctly.
#[cfg(test)]
pub fn stored_zip(members: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, mode) in members {
        let offset = out.len() as u32;
        let size = data.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 14]);
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        central.extend_from_slice(&CENTRAL_FILE_HEADER.to_le_bytes());
        central.extend_from_slice(&(3u16 << 8).to_le_bytes());
        central.extend_from_slice(&[0u8; 14]);
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0u8; 8]);
        central.extend_from_slice(&(mode << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&END_OF_CENTRAL_DIRECTORY.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: u32 = 0o100_644;
    const DIRECTORY: u32 = 0o040_755;

    fn admission() -> Admission {
        Admission {
            directory_name: "Tree-1.0".to_string(),
            max_members: 4,
            max_unpacked_bytes: 100,
        }
    }

    fn admit_bytes(zip: &[u8]) -> Result<Inventory, String> {
        let directory = std::env::temp_dir().join(format!(
            "grammachy-zip-{}-{}",
            std::process::id(),
            zip.len()
        ));
        let _ = std::fs::create_dir_all(&directory);
        let path = directory.join("archive.zip");
        std::fs::write(&path, zip).expect("the archive is written");
        let outcome = admit(&path, &admission());
        let _ = std::fs::remove_dir_all(&directory);
        outcome
    }

    #[test]
    fn a_release_shaped_archive_is_admitted_with_its_inventory() {
        let zip = stored_zip(&[
            ("Tree-1.0/", b"", DIRECTORY),
            ("Tree-1.0/server.jar", b"jar bytes", FILE),
            ("Tree-1.0/libs/a.jar", b"more", FILE),
        ]);

        let inventory = admit_bytes(&zip).expect("the archive is the pinned shape");

        assert_eq!(
            inventory,
            Inventory {
                members: 3,
                unpacked_bytes: 13
            }
        );
    }

    #[test]
    fn a_member_outside_the_pinned_directory_is_refused() {
        let zip = stored_zip(&[("Other/server.jar", b"jar", FILE)]);
        let error = admit_bytes(&zip).expect_err("the top directory differs");
        assert!(error.contains("outside Tree-1.0/"), "{error}");
    }

    #[test]
    fn a_member_that_climbs_out_is_refused() {
        let zip = stored_zip(&[("Tree-1.0/../escape", b"x", FILE)]);
        let error = admit_bytes(&zip).expect_err("the member climbs out");
        assert!(error.contains("climbs out"), "{error}");
    }

    #[test]
    fn an_absolute_member_is_refused() {
        let zip = stored_zip(&[("/etc/passwd", b"x", FILE)]);
        let error = admit_bytes(&zip).expect_err("the member is absolute");
        assert!(error.contains("outside the pinned tree"), "{error}");
    }

    #[test]
    fn a_symbolic_link_is_refused() {
        let zip = stored_zip(&[("Tree-1.0/link", b"/etc", 0o120_777)]);
        let error = admit_bytes(&zip).expect_err("the member is a symlink");
        assert!(error.contains("symbolic link"), "{error}");
    }

    #[test]
    fn too_many_members_are_refused() {
        let zip = stored_zip(&[
            ("Tree-1.0/a", b"", FILE),
            ("Tree-1.0/b", b"", FILE),
            ("Tree-1.0/c", b"", FILE),
            ("Tree-1.0/d", b"", FILE),
            ("Tree-1.0/e", b"", FILE),
        ]);
        let error = admit_bytes(&zip).expect_err("five members exceed four");
        assert!(error.contains("more than 4 members"), "{error}");
    }

    #[test]
    fn too_many_unpacked_bytes_are_refused() {
        let zip = stored_zip(&[
            ("Tree-1.0/a", &[0u8; 60], FILE),
            ("Tree-1.0/b", &[0u8; 60], FILE),
        ]);
        let error = admit_bytes(&zip).expect_err("120 bytes exceed 100");
        assert!(error.contains("more than 100 bytes"), "{error}");
    }

    #[test]
    fn bytes_that_are_not_a_zip_are_refused() {
        let error = admit_bytes(b"PK fake release").expect_err("no end record");
        assert!(error.contains("not a zip archive"), "{error}");
    }

    #[test]
    fn a_zip64_marker_is_refused() {
        let mut zip = stored_zip(&[("Tree-1.0/a", b"x", FILE)]);
        let central_at = zip.len() - END_RECORD_LEN - (CENTRAL_HEADER_LEN + "Tree-1.0/a".len());
        zip[central_at + 24..central_at + 28].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = admit_bytes(&zip).expect_err("the size is the ZIP64 marker");
        assert!(error.contains("ZIP64"), "{error}");
    }
}
