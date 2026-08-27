//! Unpacking one downloaded engine archive.
//!
//! The transfer of [`crate::model`] lands one file and is done. An engine is a
//! tree, so there is a second step, and it is the one that can leave a half
//! written directory behind. So the unpack goes into a staging directory
//! beside the final one and the tree is renamed into place only when it is
//! whole, the way the `.part` file of a weights download is renamed only when
//! its digest matches.
//!
//! The tool is `bsdtar`, which reads a zip as readily as a tar and is in the
//! Arch base group, so a machine that runs Omarchy already has it. `unzip` is
//! not in base and would be one more pacman step in front of a feature whose
//! whole point is that it needs none.
//!
//! [`Extractor`] is the seam: no test unpacks a real 250 MB archive.

use std::path::Path;
use std::process::Command;

/// What unpacks one archive into one directory.
///
/// The real one is [`bsdtar`]. Tests hand in their own, which is how the
/// install step is covered without an archive tool and without the network.
pub type Extractor = Box<dyn Fn(&Path, &Path) -> Result<(), String> + Send + Sync>;

/// The extractor this run uses.
pub fn extractor() -> Extractor {
    Box::new(bsdtar)
}

/// Unpack one archive into one directory, which must already exist.
pub fn bsdtar(archive: &Path, directory: &Path) -> Result<(), String> {
    let output = Command::new("bsdtar")
        .arg("--extract")
        .arg("--file")
        .arg(archive)
        .arg("--directory")
        .arg(directory)
        .output()
        .map_err(|error| {
            format!("bsdtar could not run: {error}. Install it with: sudo pacman -S libarchive")
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
