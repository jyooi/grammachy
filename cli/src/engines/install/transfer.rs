//! The generic download and unit-stop machinery `grammachy engine` runs on.
//!
//! `grammachy model` used to own this and `grammachy engine install` reused it
//! wholesale, because an install is the same download with one more step
//! (HUF-240 removed the model command, so this is its one remaining home).
//!
//! The transfer itself is `curl`, the same tool `bin/bootstrap.sh` uses for the
//! binary (spec section 10). `curl` resumes an interrupted transfer and
//! retries a failed one, which the HTTP client of the CLI does not. The
//! download runs into a `.part` file, renamed only when it is whole and its
//! sha256 matches the pin.

use std::path::Path;
use std::process::Command;

use super::{cancel, digest};

/// What one transfer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    /// The whole file arrived.
    Finished,
    /// A cancel stopped it. The `.part` file is kept, so the next run resumes.
    Cancelled,
}

/// What fetches one URL into one path.
///
/// The real one is [`curl`]. Tests hand in their own, which is how the step is
/// covered without reaching the network.
pub type Downloader = Box<dyn Fn(&str, &Path) -> Result<Transfer, String> + Send + Sync>;

/// What stops a transient user unit.
///
/// `Engines::remove` uses it before it deletes an installed tree that still
/// holds its server's jars open. The real one runs `systemctl --user stop`.
/// Tests hand in their own, because no test may touch a unit the live shell
/// uses.
pub type Stopper = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Keeps a stop from reaching the real unit. Tests and CI set it to `never`.
/// Not a user-facing setting.
pub const STOP_ENV: &str = "GRAMMACHY_LLAMA_STOP";

/// The downloader this run uses.
pub fn downloader() -> Downloader {
    Box::new(curl)
}

/// How often a running transfer is asked whether a cancel has arrived.
const POLL_MS: u64 = 100;

/// Fetch one URL into one path, resuming a `.part` file that is already there.
///
/// curl runs as a child rather than through `output()`, because a cancel has to
/// reach it: the signal handler only sets a flag, and this loop is what turns
/// that flag into a dead child and a kept `.part` file.
pub fn curl(url: &str, path: &Path) -> Result<Transfer, String> {
    let mut child = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--retry")
        .arg("3")
        // No progress meter. Nothing drains stderr until the transfer is over,
        // so a meter that writes for an hour would fill the pipe and stop curl
        // dead. `--show-error` keeps the one line a failure needs.
        .arg("--silent")
        .arg("--show-error")
        // Carry on where an interrupted run stopped. curl answers 33 when the
        // server cannot resume, which the caller turns into a plain message.
        .arg("--continue-at")
        .arg("-")
        .arg("--output")
        .arg(path)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("curl could not run: {error}. Install it with: sudo pacman -S curl")
        })?;

    loop {
        if cancel::requested() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Transfer::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(Transfer::Finished),
            Ok(Some(_)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(format!("curl could not fetch {url}: {}", stderr.trim()));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(POLL_MS)),
            Err(error) => return Err(format!("curl could not be waited for: {error}")),
        }
    }
}

/// The unit stopper this run uses.
pub fn stopper() -> Stopper {
    if std::env::var_os(STOP_ENV).is_some_and(|value| value == "never") {
        return Box::new(|_unit| Ok(()));
    }
    Box::new(stop_unit)
}

/// What `systemctl` exits with for a unit it does not hold.
const NOT_LOADED_EXIT: i32 = 5;

/// What [`stop_unit`] reports for a unit that was not running.
///
/// The wording is this project's own, because code reads it and a `systemctl`
/// message follows the locale. [`stop_found_nothing_to_stop`] is the reader.
pub const NOT_LOADED: &str = "the unit is not loaded, so nothing was running";

/// Stop one transient user unit.
///
/// A transient unit is collected when it stops, so a unit that is not running
/// is not loaded either and `systemctl` exits 5 on it. That reads as an error
/// here, and each caller decides what a stop it could not run means to it.
pub fn stop_unit(unit: &str) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .arg("stop")
        .arg(unit)
        .output()
        .map_err(|error| format!("systemctl could not run: {error}"))?;

    if output.status.success() {
        return Ok(());
    }
    if output.status.code() == Some(NOT_LOADED_EXIT) {
        return Err(format!("systemctl could not stop {unit}: {NOT_LOADED}"));
    }
    Err(format!(
        "systemctl could not stop {unit}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Whether one stop failure says the unit was not running.
///
/// A caller that only needs the unit gone has what it wanted here, so this is
/// the one stop failure it may pass. Anything else is a stop that did not do
/// its job.
pub fn stop_found_nothing_to_stop(why: &str) -> bool {
    why.contains(NOT_LOADED)
}

/// Why an install-backed verb did not do what it was asked.
///
/// Each variant is one code of the error envelope in spec section 5.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// A name the catalogue does not carry, or a disk with no room for it.
    BadArguments(String),
    /// curl failed, or the finished file did not match the pinned digest.
    DownloadFailed(String),
    /// A SIGTERM arrived. The `.part` file is kept.
    Cancelled(String),
}

/// Rename the `.part` file only when its digest matches the pin.
///
/// A mismatch deletes the partial. Those bytes are whole and wrong, so a resume
/// would ask for a range past the end of the file and re-hash the same wrong
/// bytes for ever: only a clean start can recover, and the next transfer is
/// what makes it. A cancel is the other case and keeps its `.part` file.
pub(crate) fn promote(
    partial: &Path,
    final_path: &Path,
    expected_sha256: &str,
) -> Result<(), String> {
    let actual = digest::sha256_path(partial)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let next = match std::fs::remove_file(partial) {
            Ok(()) => format!(
                "{} was deleted, so the next download starts over.",
                partial.display()
            ),
            Err(error) => format!(
                "{} could not be deleted ({error}). Remove it before the next download.",
                partial.display()
            ),
        };
        return Err(format!(
            "The downloaded file does not match the pinned digest. Expected {expected_sha256}, got {actual}. {next}"
        ));
    }
    std::fs::rename(partial, final_path).map_err(|error| {
        format!(
            "{} could not be renamed to {}: {error}",
            partial.display(),
            final_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_digest_renames_the_part_file() {
        let directory = scratch("digest-match");
        let bytes = b"small fake archive";
        let partial = directory.join("archive.zip.part");
        let final_path = directory.join("archive.zip");
        std::fs::write(&partial, bytes).expect("the part file is written");

        promote(&partial, &final_path, &digest::sha256_hex(bytes)).expect("the digest matches");

        assert_eq!(std::fs::read(&final_path).unwrap(), bytes);
        assert!(!partial.exists());
    }

    /// A whole file that is not the pinned one cannot be resumed into the right
    /// one, so it goes and the next download starts over. A cancel is the other
    /// case and keeps its `.part` file.
    #[test]
    fn a_mismatched_digest_removes_the_part_file() {
        let directory = scratch("digest-mismatch");
        let bytes = b"small fake archive";
        let partial = directory.join("archive.zip.part");
        let final_path = directory.join("archive.zip");
        std::fs::write(&partial, bytes).expect("the part file is written");
        let expected = digest::sha256_hex(b"other bytes");
        let actual = digest::sha256_hex(bytes);

        let error = promote(&partial, &final_path, &expected).expect_err("the digest differs");

        assert!(error.contains(&expected), "{error}");
        assert!(error.contains(&actual), "{error}");
        assert!(!final_path.exists());
        assert!(!partial.exists(), "the wrong bytes were deleted");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("grammachy-install-transfer-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the scratch directory is created");
        directory
    }
}
