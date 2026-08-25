//! Start of the transient user unit that runs the LanguageTool HTTP server.
//!
//! Spec section 4 and section 10: transient units only, through `systemd-run
//! --user`, so removing the plugin leaves no unit file behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The transient unit name from spec section 4.
pub const UNIT_NAME: &str = "grammachy-languagetool";

/// `maxTextLength` handed to the server, the same cap the CLI applies itself.
const MAX_TEXT_LENGTH: usize = 5_000;

/// The server launcher the pacman package installs.
///
/// The `languagetool` package ships this wrapper, which runs the LanguageTool
/// HTTP server class from the jars in `/usr/share/languagetool`.
const PACKAGE_SERVER: &str = "/usr/bin/languagetool-server";

/// Where the package puts its jars, used only when the wrapper is missing.
const PACKAGE_JAR_DIR: &str = "/usr/share/languagetool";

/// Why the unit did not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure(pub String);

/// The program and arguments that run the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCommand {
    pub program: String,
    pub arguments: Vec<String>,
}

/// Read the server command from the installed package.
///
/// The wrapper is preferred because it is the package's own entry point. The
/// jar call is the fallback for a package layout that ships no wrapper.
pub fn server_command(port: u16, config: &Path) -> Result<ServerCommand, StartFailure> {
    let port = port.to_string();
    let config = config.to_string_lossy().to_string();

    if Path::new(PACKAGE_SERVER).is_file() {
        return Ok(ServerCommand {
            program: PACKAGE_SERVER.to_string(),
            // No `--public`, so the server binds the loopback address only.
            arguments: vec!["--port".to_string(), port, "--config".to_string(), config],
        });
    }

    let jar = Path::new(PACKAGE_JAR_DIR).join("languagetool-server.jar");
    if jar.is_file() {
        return Ok(ServerCommand {
            program: "java".to_string(),
            arguments: vec![
                "-cp".to_string(),
                jar.to_string_lossy().to_string(),
                "org.languagetool.server.HTTPServer".to_string(),
                "--port".to_string(),
                port,
                "--config".to_string(),
                config,
            ],
        });
    }

    Err(StartFailure(format!(
        "The languagetool package is not installed. Neither {PACKAGE_SERVER} nor {} exists.",
        jar.display()
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
///
/// The exact command is
/// `systemd-run --user --unit grammachy-languagetool --collect -- <server> --port <port> --config <file>`.
pub fn start(port: u16) -> Result<(), StartFailure> {
    let config = write_config()?;
    let command = server_command(port, &config)?;

    let output = Command::new("systemd-run")
        .arg("--user")
        .arg(format!("--unit={UNIT_NAME}"))
        .arg("--description=Grammachy LanguageTool server")
        // Collect a failed unit so the next Check may start it again.
        .arg("--collect")
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
    fn the_server_command_carries_the_port_and_the_config() {
        let config = Path::new("/run/user/1000/grammachy/languagetool.properties");
        let Ok(command) = server_command(8081, config) else {
            // The package is not installed on this machine, which is its own
            // reported failure and is covered by the adapter tests.
            return;
        };

        assert!(command.arguments.contains(&"8081".to_string()));
        assert!(command
            .arguments
            .contains(&config.to_string_lossy().to_string()));
        // No external access: the server never gets `--public`.
        assert!(!command.arguments.contains(&"--public".to_string()));
    }
}
