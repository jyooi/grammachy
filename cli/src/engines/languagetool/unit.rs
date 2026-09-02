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
use crate::engines::listener::{self, Peer};
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

/// The tail every Grammachy properties file has, which is what the proof of
/// [`launched_here`] reads instead of the path of this session.
const CONFIG_SUFFIX: &str = "/grammachy/languagetool.properties";

/// The tail of every JVM this build runs the server with.
const JAVA_TAIL: &str = "/bin/java";

/// The classpath flag `tree_command` writes.
const CLASSPATH_FLAG: &str = " -cp ";

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
/// The proof reads the shape of the running command, never the environment
/// of this process. A rebuilt command line would depend on `JAVA_HOME` and on
/// `XDG_RUNTIME_DIR`, so a unit this plugin started under one environment
/// would fail under another.
///
/// Four things must hold. The unit is transient, because `start` only ever
/// makes transient units. Its `--port` is the port it listens on. Its
/// `--config` is a Grammachy properties file. And its program is one of the
/// two shapes [`server_command`] starts: the pacman launcher with `--http`,
/// or a JVM that runs [`SERVER_CLASS`] off the installed tree's server jar.
/// With no installed tree, the JVM shape refuses.
///
/// A unit that fails this was made by something else under the same name.
/// Whoever made it, the Selection does not go to it.
pub fn launched_here(peer: &Peer) -> Result<(), String> {
    shaped_like_ours(peer, install::installed("languagetool").as_deref())
}

/// [`launched_here`] against one installed tree, so the tests name their own.
///
/// The whole command line is compared, not one flag at a time. Every shape
/// this build starts is one exact string once the port and the properties
/// file are known, so a crafted line cannot hide an extra jar, a second flag,
/// or another main class between the parts a piecewise reader looks at.
fn shaped_like_ours(peer: &Peer, tree: Option<&Path>) -> Result<(), String> {
    if !peer.transient {
        return Err("the unit is not transient, so systemd-run did not make it".to_string());
    }
    let argv = peer.exec_start.argv.as_str();

    let head = if peer.exec_start.path == PACKAGE_LAUNCHER {
        format!("{PACKAGE_LAUNCHER} --http")
    } else if peer.exec_start.path.ends_with(JAVA_TAIL) {
        // The JVM path itself is free, because JAVA_HOME may differ between
        // the run that started the unit and this one.
        let at = argv.find(CLASSPATH_FLAG).ok_or_else(|| refused(peer))?;
        let jvm = &argv[..at];
        if !jvm.ends_with(JAVA_TAIL) {
            return Err(refused(peer));
        }
        let tree = tree.ok_or_else(|| {
            "the unit runs the JVM and no LanguageTool tree is installed".to_string()
        })?;
        format!(
            "{jvm}{CLASSPATH_FLAG}{}:{} {SERVER_CLASS}",
            tree.join(SERVER_JAR).display(),
            tree.join("libs/*").display()
        )
    } else {
        return Err(refused(peer));
    };

    let prefix = format!("{head} --port {} --config ", peer.address.port());
    let config = argv.strip_prefix(&prefix).ok_or_else(|| refused(peer))?;
    if config.contains(" --") || !config.ends_with(CONFIG_SUFFIX) {
        return Err(format!(
            "the unit reads {config}, which is not a Grammachy properties file"
        ));
    }
    Ok(())
}

/// Why one command line is not the one this build starts.
///
/// A port that does not match the listener is the one case worth its own
/// sentence, because a unit under this name on another port is what a reader
/// meets most often.
fn refused(peer: &Peer) -> String {
    let listening = peer.address.port();
    match listener::port_of(&peer.exec_start.argv) {
        Some(port) if port != listening => {
            format!("the unit's command line names port {port}, and it listens on port {listening}")
        }
        _ => format!(
            "the unit runs {}, which is not the command this plugin starts",
            peer.exec_start.argv
        ),
    }
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

    /// The properties file of a session, which the proof reads by its tail
    /// rather than by the runtime directory of this process.
    const CONFIG: &str = "/run/user/1000/grammachy/languagetool.properties";

    /// The tree a JVM command line names, which the tests hand in.
    const TREE: &str = "/home/someone/.local/share/grammachy/engines/languagetool";

    /// A unit file under the same name, or a transient unit that runs some
    /// other program, is not one this plugin started.
    #[test]
    fn a_unit_this_plugin_did_not_start_is_refused() {
        let launcher = format!("{PACKAGE_LAUNCHER} --http --port 8081 --config {CONFIG}");
        let file_unit = peer(false, PACKAGE_LAUNCHER, &launcher);
        let refused = launched_here(&file_unit).expect_err("a unit file is not transient");
        assert!(refused.contains("not transient"), "{refused}");

        let no_port = peer(true, "/usr/bin/nc", "/usr/bin/nc -l 127.0.0.1 8081");
        let refused = launched_here(&no_port).expect_err("no port");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );

        let no_config = peer(true, "/usr/bin/nc", "/usr/bin/nc -l --port 8081");
        let refused = launched_here(&no_config).expect_err("no properties file");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );

        let other = peer(
            true,
            "/usr/bin/nc",
            &format!("/usr/bin/nc -l --port 8081 --config {CONFIG}"),
        );
        let refused = launched_here(&other).expect_err("another program");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );
    }

    /// The pacman launcher shape passes on any machine, because the proof
    /// reads the command line rather than the environment.
    #[test]
    fn the_launcher_shape_of_this_plugin_passes() {
        let ours = format!("{PACKAGE_LAUNCHER} --http --port 8081 --config {CONFIG}");
        shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &ours), None)
            .expect("the launcher this plugin starts");

        let elsewhere = format!("{PACKAGE_LAUNCHER} --http --port 8081 --config /tmp/x.properties");
        let refused = shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &elsewhere), None)
            .expect_err("another properties file");
        assert!(
            refused.contains("not a Grammachy properties file"),
            "{refused}"
        );

        // Without `--http` the launcher starts the HTTPS server instead.
        let https = format!("{PACKAGE_LAUNCHER} --port 8081 --config {CONFIG}");
        let refused = shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &https), None)
            .expect_err("the HTTPS server");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );
    }

    /// The JVM shape names the installed tree's server jar on its classpath,
    /// and the JVM path itself is free, because `JAVA_HOME` may differ
    /// between the run that started the unit and this one.
    #[test]
    fn the_jvm_shape_needs_the_installed_tree_on_its_classpath() {
        let tree = Path::new(TREE);
        let ours = format!(
            "/usr/lib/jvm/java-21-openjdk/bin/java -cp {TREE}/{SERVER_JAR}:{TREE}/libs/* \
{SERVER_CLASS} --port 8081 --config {CONFIG}"
        );
        let running = peer(true, "/usr/lib/jvm/default/bin/java", &ours);
        shaped_like_ours(&running, Some(tree)).expect("the JVM command this plugin starts");

        let refused = shaped_like_ours(&running, None).expect_err("no tree is installed");
        assert!(refused.contains("no LanguageTool tree"), "{refused}");

        // A jar whose name only starts with the installed one is another
        // file, so the classpath entry has to end at a colon or at the end.
        let near = format!(
            "/usr/lib/jvm/default/bin/java -cp {TREE}/{SERVER_JAR}.evil:/tmp/x.jar \
{SERVER_CLASS} --port 8081 --config {CONFIG}"
        );
        let refused = shaped_like_ours(
            &peer(true, "/usr/lib/jvm/default/bin/java", &near),
            Some(tree),
        )
        .expect_err("another jar");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );

        let elsewhere = format!(
            "/usr/lib/jvm/default/bin/java -cp /tmp/evil.jar {SERVER_CLASS} --port 8081 --config {CONFIG}"
        );
        let refused = shaped_like_ours(
            &peer(true, "/usr/lib/jvm/default/bin/java", &elsewhere),
            Some(tree),
        )
        .expect_err("another classpath");
        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );
    }

    /// The exact match leaves no room between the classpath and the main
    /// class, so an extra jar and another main class cannot pass behind a
    /// copy of the server class later on the line.
    #[test]
    fn an_extra_jar_and_a_decoy_main_class_are_refused() {
        let tree = Path::new(TREE);
        let argv = format!(
            "/usr/lib/jvm/default/bin/java -cp {TREE}/{SERVER_JAR}:{TREE}/libs/*:/tmp/evil.jar \
Evil --port 8081 --config {CONFIG} --x {SERVER_CLASS}"
        );

        let refused = shaped_like_ours(
            &peer(true, "/usr/lib/jvm/default/bin/java", &argv),
            Some(tree),
        )
        .expect_err("the JVM runs Evil off /tmp/evil.jar");

        assert!(
            refused.contains("not the command this plugin starts"),
            "{refused}"
        );
    }

    /// The JVM takes the last classpath flag before the main class, so a
    /// first flag with the installed tree must not cover a second one.
    #[test]
    fn a_second_classpath_flag_is_refused() {
        let tree = Path::new(TREE);
        for flag in ["-cp", "-classpath", "--class-path"] {
            let argv = format!(
                "/usr/lib/jvm/default/bin/java -cp {TREE}/{SERVER_JAR}:{TREE}/libs/* \
{flag} /tmp/evil.jar {SERVER_CLASS} --port 8081 --config {CONFIG}"
            );

            let refused = shaped_like_ours(
                &peer(true, "/usr/lib/jvm/default/bin/java", &argv),
                Some(tree),
            )
            .expect_err("the JVM loads /tmp/evil.jar");

            assert!(
                refused.contains("not the command this plugin starts"),
                "{flag}: {refused}"
            );
        }
    }

    /// The server takes the last `--config`, so the proof has to read that
    /// one. A first flag with a good path must not cover a second with a
    /// crafted one.
    #[test]
    fn the_last_config_flag_is_the_one_proven() {
        let argv = format!(
            "{PACKAGE_LAUNCHER} --http --port 8081 --config {CONFIG} --config /tmp/evil.properties"
        );

        let refused = shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &argv), None)
            .expect_err("the server reads /tmp/evil.properties");

        assert!(refused.contains("/tmp/evil.properties"), "{refused}");
    }

    /// The suffix has to sit on a directory boundary. A directory whose name
    /// only ends with `grammachy` is another directory.
    #[test]
    fn a_directory_that_only_ends_with_the_name_is_refused() {
        let argv = format!(
            "{PACKAGE_LAUNCHER} --http --port 8081 \
--config /tmp/evilgrammachy/languagetool.properties"
        );

        let refused = shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &argv), None)
            .expect_err("another properties file");

        assert!(
            refused.contains("not a Grammachy properties file"),
            "{refused}"
        );
    }

    /// The `--config` value ends at the next flag, so a later argument that
    /// carries the suffix cannot stand in for the file the server reads.
    #[test]
    fn a_later_flag_that_carries_the_suffix_is_refused() {
        let argv = format!(
            "{PACKAGE_LAUNCHER} --http --port 8081 --config /tmp/evil.properties \
--extra /x{CONFIG_SUFFIX}"
        );

        let refused = shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &argv), None)
            .expect_err("the server reads /tmp/evil.properties");

        assert!(refused.contains("/tmp/evil.properties"), "{refused}");
        assert!(
            refused.contains("not a Grammachy properties file"),
            "{refused}"
        );
    }

    /// systemd prints `argv[]` space joined, so a home directory that holds a
    /// space must not make the proof refuse a unit this plugin started.
    #[test]
    fn a_tree_path_that_holds_a_space_still_passes() {
        let tree = Path::new("/home/jia yi/.local/share/grammachy/engines/languagetool");
        let config = "/run/user/1000/a b/grammachy/languagetool.properties";
        let argv = format!(
            "/usr/lib/jvm/default/bin/java -cp {0}/{SERVER_JAR}:{0}/libs/* \
{SERVER_CLASS} --port 8081 --config {config}",
            tree.display()
        );

        shaped_like_ours(
            &peer(true, "/usr/lib/jvm/default/bin/java", &argv),
            Some(tree),
        )
        .expect("a path with a space is one word of the command line");
    }

    /// The port on the command line must be the port the unit listens on, or
    /// the proof says nothing about where the Selection goes.
    #[test]
    fn a_port_the_unit_does_not_listen_on_is_refused() {
        let argv = format!("{PACKAGE_LAUNCHER} --http --port 9999 --config {CONFIG}");

        let refused =
            shaped_like_ours(&peer(true, PACKAGE_LAUNCHER, &argv), None).expect_err("another port");

        assert!(refused.contains("9999"), "{refused}");
        assert!(refused.contains("8081"), "{refused}");
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
