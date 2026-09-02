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
//!      org.languagetool.server.HTTPServer --port <port> --config <properties>
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
//!   -- /usr/bin/languagetool --http --port <port> --config <properties>
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
use crate::engines::listener::Peer;
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

/// Where the server properties file lives while the session lasts.
pub fn config_path() -> PathBuf {
    local::runtime_directory()
        .join("grammachy")
        .join("languagetool.properties")
}

/// Write the server properties file and answer its path.
///
/// `maxTextLength` is set here because the HTTP server reads it from a
/// properties file, not from a flag (spec section 4).
pub fn write_config() -> Result<PathBuf, StartFailure> {
    let path = config_path();
    let directory = path.parent().expect("the config path has a directory");
    fs::create_dir_all(directory).map_err(|error| {
        StartFailure(format!("{} is not writable: {error}", directory.display()))
    })?;

    fs::write(&path, format!("maxTextLength={MAX_TEXT_LENGTH}\n"))
        .map_err(|error| StartFailure(format!("{} is not writable: {error}", path.display())))?;
    Ok(path)
}

/// Prove that the running unit is one this plugin started.
///
/// The unit must be transient, because `start` only ever makes transient
/// units. Its `ExecStart` must be a command line this build would run on
/// this machine. That is the installed tree or the pacman launcher, on the
/// port the unit was given, with the properties file of this session.
///
/// A unit that fails this was made by something else under the same name.
/// Whoever made it, the Selection does not go to it.
pub fn launched_here(peer: &Peer) -> Result<(), String> {
    if !peer.transient {
        return Err("the unit is not transient, so systemd-run did not make it".to_string());
    }
    let port = port_of(&peer.exec_start.argv)
        .ok_or_else(|| "the unit's command line names no --port".to_string())?;
    let java_home = java_home().map_err(|StartFailure(why)| why)?;
    let config = config_path();
    let mut candidates = Vec::new();
    if let Some(tree) = install::installed("languagetool") {
        candidates.push(tree_command(&tree, port, &config, java_home.clone()));
    }
    if Path::new(PACKAGE_LAUNCHER).is_file() {
        candidates.push(package_command(port, &config, java_home));
    }
    let matches = candidates.iter().any(|candidate| {
        candidate.program == peer.exec_start.path
            && candidate.command_line() == peer.exec_start.argv
    });
    if matches {
        Ok(())
    } else {
        Err(format!(
            "the unit runs {}, which is not the command this plugin starts",
            peer.exec_start.argv
        ))
    }
}

/// The value after `--port` on one command line.
fn port_of(argv: &str) -> Option<u16> {
    let mut words = argv.split(' ');
    while let Some(word) = words.next() {
        if word == "--port" {
            return words.next()?.parse().ok();
        }
    }
    None
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

    fn peer(transient: bool, program: &str, argv: &str) -> Peer {
        Peer {
            address: "127.0.0.1:8081".parse().unwrap(),
            pid: 4242,
            transient,
            exec_start: crate::engines::listener::ExecStart {
                path: program.to_string(),
                argv: argv.to_string(),
            },
        }
    }

    /// A unit file under the same name, or a transient unit that runs some
    /// other program, is not one this plugin started.
    #[test]
    fn a_unit_this_plugin_did_not_start_is_refused() {
        let launcher = format!(
            "{PACKAGE_LAUNCHER} --http --port 8081 --config {}",
            config_path().display()
        );
        let file_unit = peer(false, PACKAGE_LAUNCHER, &launcher);
        let refused = launched_here(&file_unit).expect_err("a unit file is not transient");
        assert!(refused.contains("not transient"), "{refused}");

        let no_port = peer(true, "/usr/bin/nc", "/usr/bin/nc -l 127.0.0.1 8081");
        let refused = launched_here(&no_port).expect_err("no port");
        assert!(refused.contains("no --port"), "{refused}");

        let other = peer(true, "/usr/bin/nc", "/usr/bin/nc -l --port 8081");
        let refused = launched_here(&other).expect_err("another program");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );
    }

    /// On a machine with the pacman launcher, the exact command `start`
    /// would run passes, and the same command against another properties
    /// file does not.
    #[test]
    fn the_launcher_command_of_this_session_passes() {
        if !Path::new(PACKAGE_LAUNCHER).is_file() || java_home().is_err() {
            return;
        }
        let expected = package_command(8081, &config_path(), java_home().unwrap());
        let ours = peer(true, &expected.program, &expected.command_line());
        launched_here(&ours).expect("the command this plugin starts");

        let elsewhere = package_command(8081, Path::new("/tmp/x.properties"), java_home().unwrap());
        let refused = launched_here(&peer(true, &elsewhere.program, &elsewhere.command_line()))
            .expect_err("another properties file");
        assert!(refused.contains("not the command"), "{refused}");
    }

    #[test]
    fn the_port_is_read_from_the_command_line() {
        assert_eq!(
            port_of("/usr/bin/x --http --port 43210 --config /a"),
            Some(43210)
        );
        assert_eq!(port_of("/usr/bin/x --port"), None);
        assert_eq!(port_of("/usr/bin/x --port many"), None);
        assert_eq!(port_of("/usr/bin/x"), None);
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
