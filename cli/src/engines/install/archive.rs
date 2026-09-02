//! Unpacking one downloaded engine archive.
//!
//! The transfer of [`super::transfer`] lands one file and is done. An engine is
//! a tree, so there is a second step, and it is the one that can leave a half
//! written directory behind. So the unpack goes into a staging directory
//! beside the final one and the tree is renamed into place only when it is
//! whole, the way the `.part` file of the archive itself is renamed only when
//! its digest matches.
//!
//! The tool is `bsdtar`, which reads a zip as readily as a tar and is in the
//! Arch base group, so a machine that runs Omarchy already has it. `unzip` is
//! not in base and would be one more pacman step in front of a feature whose
//! whole point is that it needs none.
//!
//! Before `bsdtar` runs, [`super::zip::admit`] reads the central directory
//! and refuses an archive whose members would land outside the pinned tree,
//! or exceed the members and unpacked bytes the row pins. `bsdtar` refuses
//! absolute and `..` paths of its own accord, and the admission makes the
//! same rule explicit and adds the bounds.
//!
//! [`Extractor`] is the seam: no test unpacks a real 250 MB archive.

use std::path::Path;
use std::process::Command;

use super::zip::{self, Admission};

/// What unpacks one archive into one directory, within one admission.
///
/// The real one admits the archive and runs [`bsdtar`]. Tests hand in their
/// own, which is how the install step is covered without an archive tool and
/// without the network.
pub type Extractor = Box<dyn Fn(&Path, &Path, &Admission) -> Result<(), String> + Send + Sync>;

/// The extractor this run uses.
pub fn extractor() -> Extractor {
    Box::new(|archive, directory, admission| {
        zip::admit(archive, admission)?;
        bsdtar(archive, directory)
    })
}

/// Unpack one archive into one directory, which must already exist.
pub fn bsdtar(archive: &Path, directory: &Path) -> Result<(), String> {
    let output = Command::new("bsdtar")
        .arg("--extract")
        // The archive is admitted already, and these keep an unpack inside
        // the staging directory whatever a member says about itself.
        .arg("--no-same-owner")
        .arg("--no-same-permissions")
        .arg("--file")
        .arg(archive)
        .arg("--directory")
        .arg(directory)
        .output()
        .map_err(|error| {
            format!(
                "bsdtar could not run: {error}. Add libarchive through Omarchy Install. Open SUPER+SPACE, then Install, then Package."
            )
        })?;

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "bsdtar could not unpack {}: {}",
        archive.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}
