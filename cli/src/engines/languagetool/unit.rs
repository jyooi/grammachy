//! Start of the transient user unit that runs the LanguageTool HTTP server.
//!
//! Spec section 4 and section 10: transient units only, through `systemd-run
//! --user`, so removing the plugin leaves no unit file behind.
//!
//! LanguageTool is an opt-in component (HUF-237), so the server can come from
//! either of two places and [`server_command`] reads them in this order:
//!
//! 1. The tree `grammachy engine install languagetool` unpacks under
//!    `~/.local/share/grammachy/engines/languagetool/`. That is the one this
//!    project puts there, so it wins: a user who installed it from Settings
//!    gets the release this build pins whatever else the machine carries.
//! 2. `/usr/bin/languagetool` from the Arch `languagetool` package, which is
//!    an alternative Grammachy never installs and never removes.
//!
//! Neither being there is not a fault of the machine: it is an engine the user
//! has not added yet, which is what `doctor` says and what the Settings row
//! offers to fix.
//!
//! The two run the server differently. The unpacked tree is jars, so the unit
//! runs the JVM itself:
//!
//! ```text
//! systemd-run --user --unit=grammachy-languagetool --collect \
//!   -- <jvm>/bin/java \
//!      -cp <tree>/languagetool-server.jar:<tree>/libs/* \
//!      org.languagetool.server.HTTPServer --port 8081 --config <properties>
//! ```
//!
//! The `libs/*` wildcard is expanded by the JVM and never by a shell, so it
//! survives `systemd-run` passing the argument through untouched. The server
//! jar's own manifest already names those jars relative to itself; naming them
//! again costs nothing and does not depend on that manifest.
//!
//! Package 6.6-2 ships a single launcher instead, `/usr/bin/languagetool`,
//! which builds the classpath from `/usr/share/java/languagetool` and execs
//! `org.languagetool.server.HTTPServer` when it is given `--http`:
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
//! No `--public` is passed either way, so the server listens on the loopback
//! address only.

use std::fs;
use std::path::{Path, PathBuf};

use crate::engines::install;
pub use crate::engines::local::StartFailure;
use crate::engines::local::{self, ServerCommand};

/// The transient unit name from spec section 4.
pub const UNIT_NAME: &str = "grammachy-languagetool";

/// `maxTextLength` handed to the server, the same cap the CLI applies itself.
const MAX_TEXT_LENGTH: usize = 5_000;

/// The launcher the pacman package installs. `doctor` looks for it too.
pub const PACKAGE_LAUNCHER: &str = "/usr/bin/languagetool";

/// The server jar of the unpacked upstream release, under the installed tree.
pub const SERVER_JAR: &str = "languagetool-server.jar";

/// The class the server runs from, in both the tree and the package launcher.
const SERVER_CLASS: &str = "org.languagetool.server.HTTPServer";

/// Where `archlinux-java` points at the selected JVM.
const DEFAULT_JVM: &str = "/usr/lib/jvm/default";

/// Read the server command from whichever LanguageTool this machine has.
///
/// The installed tree wins over the pacman package, because a user who added
/// LanguageTool from Settings asked for the release this build pins.
pub fn server_command(port: u16, config: &Path) -> Result<ServerCommand, StartFailure> {
    if let Some(tree) = install::installed("languagetool") {
        return Ok(tree_command(&tree, port, config, java_home()?));
    }
    if Path::new(PACKAGE_LAUNCHER).is_file() {
        return Ok(package_command(port, config, java_home()?));
    }
    Err(StartFailure(format!(
        "LanguageTool is not installed. Add it in Settings, or run: grammachy engine install languagetool. \
The pacman package works too, and neither {PACKAGE_LAUNCHER} nor an installed tree is here."
    )))
}

/// The JVM run against the unpacked upstream release.
fn tree_command(tree: &Path, port: u16, config: &Path, java_home: String) -> ServerCommand {
    let classpath = format!(
        "{}:{}",
        tree.join(SERVER_JAR).display(),
        tree.join("libs/*").display()
    );

    ServerCommand {
        program: format!("{java_home}/bin/java"),
        arguments: vec![
            "-cp".to_string(),
            classpath,
            SERVER_CLASS.to_string(),
            "--port".to_string(),
            port.to_string(),
            "--config".to_string(),
            config.to_string_lossy().to_string(),
        ],
        environment: Vec::new(),
    }
}

/// The launcher the pacman package installs.
fn package_command(port: u16, config: &Path, java_home: String) -> ServerCommand {
    ServerCommand {
        program: PACKAGE_LAUNCHER.to_string(),
        arguments: vec![
            "--http".to_string(),
            "--port".to_string(),
            port.to_string(),
            "--config".to_string(),
            config.to_string_lossy().to_string(),
        ],
        environment: vec![("JAVA_HOME".to_string(), java_home)],
    }
}

/// The JVM the launcher runs `bin/java` from. `doctor` reports the same one.
pub fn java_home() -> Result<String, StartFailure> {
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
    let directory = local::runtime_directory().join("grammachy");
    fs::create_dir_all(&directory).map_err(|error| {
        StartFailure(format!("{} is not writable: {error}", directory.display()))
    })?;

    let path = directory.join("languagetool.properties");
    fs::write(&path, format!("maxTextLength={MAX_TEXT_LENGTH}\n"))
        .map_err(|error| StartFailure(format!("{} is not writable: {error}", path.display())))?;
    Ok(path)
}

/// Start the transient unit, or answer `Ok(())` when it already runs.
pub fn start(port: u16) -> Result<(), StartFailure> {
    let config = write_config()?;
    let command = server_command(port, &config)?;

    local::start_unit(UNIT_NAME, "Grammachy LanguageTool server", &command).map(drop)
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

    /// The installed tree is jars, so the unit runs the JVM against the server
    /// jar and the `libs` beside it.
    #[test]
    fn the_tree_command_runs_the_jvm_against_the_unpacked_release() {
        let tree = Path::new("/home/someone/.local/share/grammachy/engines/languagetool");
        let config = Path::new("/run/user/1000/grammachy/languagetool.properties");

        let command = tree_command(tree, 8081, config, "/usr/lib/jvm/default".to_string());

        assert_eq!(command.program, "/usr/lib/jvm/default/bin/java");
        assert_eq!(
            command.arguments,
            [
                "-cp",
                "/home/someone/.local/share/grammachy/engines/languagetool/languagetool-server.jar:/home/someone/.local/share/grammachy/engines/languagetool/libs/*",
                "org.languagetool.server.HTTPServer",
                "--port",
                "8081",
                "--config",
                "/run/user/1000/grammachy/languagetool.properties"
            ]
        );
        // The JVM is run directly, so nothing has to read JAVA_HOME.
        assert!(command.environment.is_empty());
        // No external access: the server never gets `--public`.
        assert!(!command.arguments.contains(&"--public".to_string()));
    }

    #[test]
    fn the_package_command_is_the_launcher_with_its_java_home() {
        let config = Path::new("/run/user/1000/grammachy/languagetool.properties");

        let command = package_command(8081, config, "/usr/lib/jvm/default".to_string());

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
        assert_eq!(
            command.environment,
            [("JAVA_HOME".to_string(), "/usr/lib/jvm/default".to_string())]
        );
        assert!(!command.arguments.contains(&"--public".to_string()));
    }

    /// LanguageTool is opt in now, so a machine with neither the tree nor the
    /// package is told how to add it without a password (HUF-237).
    #[test]
    fn a_machine_with_neither_names_the_install_verb() {
        if Path::new(PACKAGE_LAUNCHER).is_file() || install::installed("languagetool").is_some() {
            return;
        }
        let failure = server_command(8081, Path::new("/tmp/x.properties"))
            .expect_err("LanguageTool is not on this machine");

        assert!(
            failure.0.contains("grammachy engine install languagetool"),
            "{}",
            failure.0
        );
        assert!(failure.0.contains(PACKAGE_LAUNCHER), "{}", failure.0);
        assert!(!failure.0.contains("sudo"), "{}", failure.0);
    }
}
