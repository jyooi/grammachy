//! What a local-server engine needs to start and reach its own server.
//!
//! `languagetool` talks HTTP to a server on the loopback interface and starts
//! it itself as a transient user unit when the port does not answer. Spec
//! section 4 names the unit, `grammachy-languagetool`; spec section 10 fixes
//! the mechanism: `systemd-run --user` only, so removing the plugin leaves no
//! unit file behind.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;

/// Why the unit did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure(pub String);

/// The program, arguments, and environment that run one server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCommand {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}

/// What one unit was doing before this call.
///
/// A caller that must know whose server answers the port reads this. Only a
/// `Fresh` unit runs the command that this call built, so only a `Fresh` one
/// holds what the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Started {
    /// systemd-run created the unit, so it runs the command above.
    Fresh,
    /// A unit of that name was there already, so this call started nothing and
    /// the server on the port is whatever an earlier one left.
    AlreadyRunning,
}

/// Start one transient user unit, or say that it was already running.
pub fn start_unit(
    unit: &str,
    description: &str,
    command: &ServerCommand,
) -> Result<Started, StartFailure> {
    let mut systemd_run = Command::new("systemd-run");
    systemd_run
        .arg("--user")
        .arg(format!("--unit={unit}"))
        .arg(format!("--description={description}"))
        // Collect a failed unit so the next Check may start it again.
        .arg("--collect");
    for (name, value) in &command.environment {
        systemd_run.arg(format!("--setenv={name}={value}"));
    }
    let output = systemd_run
        .arg("--")
        .arg(&command.program)
        .args(&command.arguments)
        .output()
        .map_err(|error| StartFailure(format!("systemd-run could not run: {error}")))?;

    if output.status.success() {
        return Ok(Started::Fresh);
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // A unit left from an earlier Check is the outcome this call wanted.
    if message.contains("already exists") {
        return Ok(Started::AlreadyRunning);
    }
    Err(StartFailure(format!(
        "systemd-run could not start {unit}: {message}"
    )))
}

/// Where a server keeps the files it only needs while the session lasts.
pub fn runtime_directory() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => std::env::temp_dir(),
    }
}

/// Whether an I/O error means nothing is listening yet.
pub fn is_unreachable(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::AddrNotAvailable
            | ErrorKind::NotConnected
            | ErrorKind::BrokenPipe
    )
}
