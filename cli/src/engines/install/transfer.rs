use std::path::Path;
use std::process::Command;

use super::{cancel, digest};

/// What one transfer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Finished,
    Cancelled,
}

/// What fetches one URL into one path, within one byte limit.
///
/// The real one is [`curl`]. Tests hand in their own, which is how the step is
/// covered without reaching the network.
pub type Downloader = Box<dyn Fn(&str, &Path, u64) -> Result<Transfer, String> + Send + Sync>;

pub const CONNECT_TIMEOUT_SECONDS: u64 = 30;

pub const MAX_TIME_SECONDS: u64 = 1_800;

pub const STALL_BYTES_PER_SECOND: u64 = 10_240;
pub const STALL_SECONDS: u64 = 30;

/// What stops a transient user unit.
///
/// `Engines::remove` uses it before it deletes an installed tree that still
/// holds its server's jars open. The real one runs `systemctl --user stop`.
/// Tests hand in their own, because no test may touch a unit the live shell
/// uses.
pub type Stopper = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub const STOP_ENV: &str = "GRAMMACHY_ENGINE_STOP";

pub fn downloader() -> Downloader {
    Box::new(curl)
}

const POLL_MS: u64 = 100;

pub fn curl(url: &str, path: &Path, max_bytes: u64) -> Result<Transfer, String> {
    let mut child = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--proto")
        .arg("=https")
        .arg("--proto-redir")
        .arg("=https")
        .arg("--tlsv1.2")
        .arg("--connect-timeout")
        .arg(CONNECT_TIMEOUT_SECONDS.to_string())
        .arg("--max-time")
        .arg(MAX_TIME_SECONDS.to_string())
        .arg("--speed-limit")
        .arg(STALL_BYTES_PER_SECOND.to_string())
        .arg("--speed-time")
        .arg(STALL_SECONDS.to_string())
        .arg("--max-filesize")
        .arg(max_bytes.to_string())
        .arg("--retry")
        .arg("3")
        .arg("--silent")
        .arg("--show-error")
        .arg("--continue-at")
        .arg("-")
        .arg("--output")
        .arg(path)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "curl could not run: {error}. Add curl through Omarchy Install. Open SUPER+SPACE, then Install, then Package."
            )
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

pub fn stopper() -> Stopper {
    if std::env::var_os(STOP_ENV).is_some_and(|value| value == "never") {
        return Box::new(|_unit| Ok(()));
    }
    Box::new(stop_unit)
}

const NOT_LOADED_EXIT: i32 = 5;

/// What [`stop_unit`] reports for a unit that was not running.
///
/// The wording is this project's own, because code reads it and a `systemctl`
/// message follows the locale. [`stop_found_nothing_to_stop`] is the reader.
pub const NOT_LOADED: &str = "the unit is not loaded, so nothing was running";

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

pub fn stop_found_nothing_to_stop(why: &str) -> bool {
    why.contains(NOT_LOADED)
}

/// Why an install-backed verb did not do what it was asked.
///
/// Each variant is one code of the error envelope in spec section 5.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    BadArguments(String),
    DownloadFailed(String),
    Cancelled(String),
}

pub(crate) fn promote(
    partial: &Path,
    final_path: &Path,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<(), String> {
    let actual_size = std::fs::metadata(partial)
        .map(|data| data.len())
        .map_err(|error| format!("{} could not be read: {error}", partial.display()))?;
    if actual_size != expected_size {
        let next = remove_wrong_partial(partial);
        return Err(format!(
            "The downloaded file is not the pinned size. Expected {expected_size} bytes, got {actual_size}. {next}"
        ));
    }
    let actual = digest::sha256_path(partial)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let next = remove_wrong_partial(partial);
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

pub(crate) fn remove_wrong_partial(partial: &Path) -> String {
    match std::fs::remove_file(partial) {
        Ok(()) => format!(
            "{} was deleted, so the next download starts over.",
            partial.display()
        ),
        Err(error) => format!(
            "{} could not be deleted ({error}). Remove it before the next download.",
            partial.display()
        ),
    }
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

        promote(
            &partial,
            &final_path,
            &digest::sha256_hex(bytes),
            bytes.len() as u64,
        )
        .expect("the digest matches");

        assert_eq!(std::fs::read(&final_path).unwrap(), bytes);
        assert!(!partial.exists());
    }

    #[test]
    fn a_mismatched_digest_removes_the_part_file() {
        let directory = scratch("digest-mismatch");
        let bytes = b"small fake archive";
        let partial = directory.join("archive.zip.part");
        let final_path = directory.join("archive.zip");
        std::fs::write(&partial, bytes).expect("the part file is written");
        let expected = digest::sha256_hex(b"other bytes");
        let actual = digest::sha256_hex(bytes);

        let error = promote(&partial, &final_path, &expected, bytes.len() as u64)
            .expect_err("the digest differs");

        assert!(error.contains(&expected), "{error}");
        assert!(error.contains(&actual), "{error}");
        assert!(!final_path.exists());
        assert!(!partial.exists(), "the wrong bytes were deleted");
    }

    #[test]
    fn a_wrong_size_is_refused_before_the_digest() {
        let directory = scratch("size-mismatch");
        let bytes = b"small fake archive";
        let partial = directory.join("archive.zip.part");
        let final_path = directory.join("archive.zip");
        std::fs::write(&partial, bytes).expect("the part file is written");

        let error = promote(
            &partial,
            &final_path,
            &digest::sha256_hex(bytes),
            bytes.len() as u64 + 1,
        )
        .expect_err("the size differs");

        assert!(error.contains("not the pinned size"), "{error}");
        assert!(
            !error.contains("digest"),
            "the size check ran first: {error}"
        );
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
