//! Start of the transient user unit that runs the LanguageTool HTTP server.
//!
//! Spec section 4 and section 10: transient units only, through `systemd-run
//! --user`, so removing the plugin leaves no unit file behind.
//!
//! The server command is the one the `languagetool` pacman package installs.
//! Package 6.6-2 ships a single launcher, `/usr/bin/languagetool`, which builds
//! the classpath from `/usr/share/java/languagetool` and execs
//! `org.languagetool.server.HTTPServer` when it is given `--http`. So the unit
//! runs:
//!
//! ```text
//! systemd-run --user --unit=grammachy-languagetool --collect \
//!   --setenv=JAVA_HOME=<jvm> \
//!   -- /usr/bin/languagetool --http --port 8081 --config <properties>
//! ```
//!
//! Two sharp edges of that launcher:
//!
//! - It runs `"$JAVA_HOME/bin/java"`, and Arch never exports `JAVA_HOME`, so
//!   the unit has to set it or the launcher fails.
//! - `--http` is what picks the plain HTTP server. `--config` on its own makes
//!   the launcher start the HTTPS server instead.
//!
//! No `--public` is passed, so the server listens on the loopback address only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The transient unit name from spec section 4.
pub const UNIT_NAME: &str = "grammachy-languagetool";

/// `maxTextLength` handed to the server, the same cap the CLI applies itself.
const MAX_TEXT_LENGTH: usize = 5_000;

/// The launcher the pacman package installs.
const PACKAGE_LAUNCHER: &str = "/usr/bin/languagetool";

/// Where `archlinux-java` points at the selected JVM.
const DEFAULT_JVM: &str = "/usr/lib/jvm/default";

/// Why the unit did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure(pub String);

/// The program, arguments, and environment that run the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCommand {
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
}

/// Read the server command from the installed package.
pub fn server_command(port: u16, config: &Path) -> Result<ServerCommand, StartFailure> {
    if !Path::new(PACKAGE_LAUNCHER).is_file() {
        return Err(StartFailure(format!(
            "The languagetool package is not installed: {PACKAGE_LAUNCHER} does not exist."
        )));
    }

    Ok(ServerCommand {
        program: PACKAGE_LAUNCHER.to_string(),
        arguments: vec![
            "--http".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--config".to_string(),
            config.to_string_lossy().to_string(),
        ],
        environment: vec![("JAVA_HOME".to_string(), java_home()?)],
    })
}

/// The JVM the launcher runs `bin/java` from.
fn java_home() -> Result<String, StartFailure> {
    if let Some(value) = std::env::var_os("JAVA_HOME") {
        let value = value.to_string_lossy().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
    }
    if Path::new(DEFAULT_JVM).join("bin/java").is_file() {
        return Ok(DEFAULT_JVM.to_string());
    }
    Err(StartFailure(format!(
        "No Java runtime was found. {DEFAULT_JVM}/bin/java does not exist and JAVA_HOME is not set."
    )))
}

/// Write the server properties file and answer its path.
///
/// `maxTextLength` is set here because the HTTP server reads it from a
/// properties file, not from a flag (spec section 4).
pub fn write_config() -> Result<PathBuf, StartFailure> {
    let directory = runtime_directory().join("grammachy");
    fs::create_dir_all(&directory).map_err(|error| {
        StartFailure(format!("{} is not writable: {error}", directory.display()))
    })?;

    let path = directory.join("languagetool.properties");
    fs::write(&path, format!("maxTextLength={MAX_TEXT_LENGTH}\n"))
        .map_err(|error| StartFailure(format!("{} is not writable: {error}", path.display())))?;
    Ok(path)
}

fn runtime_directory() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => std::env::temp_dir(),
    }
}

/// Start the transient unit, or answer `Ok(())` when it already runs.
pub fn start(port: u16) -> Result<(), StartFailure> {
    let config = write_config()?;
    let command = server_command(port, &config)?;

    let mut systemd_run = Command::new("systemd-run");
    systemd_run
        .arg("--user")
        .arg(format!("--unit={UNIT_NAME}"))
        .arg("--description=Grammachy LanguageTool server")
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
        return Ok(());
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // A unit left from an earlier Check is the outcome this call wanted.
    if message.contains("already exists") {
        return Ok(());
    }
    Err(StartFailure(format!(
        "systemd-run could not start {UNIT_NAME}: {message}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_file_caps_the_text_length() {
        let path = write_config().expect("the runtime directory is writable");
        let text = fs::read_to_string(&path).expect("the config file is readable");

        assert_eq!(text.trim(), "maxTextLength=5000");
    }

    #[test]
    fn the_server_command_is_the_package_launcher() {
        let config = Path::new("/run/user/1000/grammachy/languagetool.properties");
        let Ok(command) = server_command(8081, config) else {
            // The package is not installed here, which is its own reported
            // failure and is covered by the adapter tests.
            return;
        };

        assert_eq!(command.program, PACKAGE_LAUNCHER);
        assert_eq!(
            command.arguments,
            [
                "--http",
                "--port",
                "8081",
                "--config",
                "/run/user/1000/grammachy/languagetool.properties"
            ]
        );
        // The launcher needs this, because Arch never exports it.
        assert_eq!(command.environment[0].0, "JAVA_HOME");
        // No external access: the server never gets `--public`.
        assert!(!command.arguments.contains(&"--public".to_string()));
    }

    #[test]
    fn a_missing_package_names_the_launcher_it_looked_for() {
        if Path::new(PACKAGE_LAUNCHER).is_file() {
            return;
        }
        let failure = server_command(8081, Path::new("/tmp/x.properties"))
            .expect_err("the package is not installed");

        assert!(failure.0.contains(PACKAGE_LAUNCHER));
    }
}
