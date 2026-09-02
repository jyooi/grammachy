//! What `doctor` sees of this machine.
//!
//! Every field here is a recorded fact rather than a call, so the report is a
//! pure function of [`Facts`] and every test runs against facts it wrote
//! itself. [`Facts::collect`] is the one place that touches the real machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engines::install;
use crate::engines::languagetool;
use crate::engines::listener::{self, Owned};

/// Whether a transient unit runs right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Running,
    Stopped,
    /// `systemctl --user` could not be asked, so the CLI cannot start the unit.
    Unknown,
}

/// Everything one `doctor` run knows about this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The companion binary that is running, when its path is knowable.
    pub binary: Option<PathBuf>,
    /// The version of that binary.
    pub version: String,
    /// The LanguageTool tree `grammachy engine install languagetool` unpacks
    /// under HOME. It is the route that needs no password (HUF-237).
    pub languagetool_tree: Option<PathBuf>,
    /// The LanguageTool launcher the pacman package installs, which is the
    /// alternative Grammachy never installs and never removes.
    pub languagetool_launcher: Option<PathBuf>,
    /// The `bin/java` the launcher runs, through `JAVA_HOME` or the default JVM.
    pub java: Option<PathBuf>,
    /// The loopback listener the `languagetool` unit owns, when it runs one.
    pub languagetool_address: Option<String>,
    pub languagetool_unit: UnitState,
    /// The binaries on `PATH` that stand for the system packages of
    /// [`super::deps`]. The Java runtime counts through `java` above, because
    /// the launcher reads `JAVA_HOME` rather than `PATH`.
    pub binaries: Vec<String>,
}

impl Facts {
    /// Read this machine. The one function here that is not a pure value.
    pub fn collect() -> Self {
        Facts {
            binary: std::env::current_exe().ok(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            languagetool_tree: install::installed("languagetool"),
            languagetool_launcher: existing_file(languagetool::unit::PACKAGE_LAUNCHER),
            java: languagetool::unit::java_home()
                .ok()
                .map(|home| PathBuf::from(home).join("bin/java")),
            languagetool_address: match listener::owned_listener(languagetool::unit::UNIT_NAME) {
                Owned::Listening(address) => Some(address.to_string()),
                _ => None,
            },
            languagetool_unit: unit_state(languagetool::unit::UNIT_NAME),
            binaries: super::deps::SPECS
                .iter()
                .map(|spec| spec.probe)
                .filter(|probe| on_path(probe))
                .map(str::to_string)
                .collect(),
        }
    }

    /// Whether the binary one package supplies is on this machine.
    ///
    /// `java` answers through `JAVA_HOME` or the default JVM, which is what
    /// the LanguageTool launcher runs, so a runtime `PATH` alone knows would
    /// not start the unit.
    pub fn has_binary(&self, probe: &str) -> bool {
        if probe == "java" {
            return self.java.is_some();
        }
        self.binaries.iter().any(|name| name == probe)
    }

    /// Where LanguageTool is on this machine, whichever route put it there.
    ///
    /// The installed tree wins, because that is the one the adapter runs and
    /// the one `grammachy engine remove` acts on.
    pub fn languagetool(&self) -> Option<&Path> {
        self.languagetool_tree
            .as_deref()
            .or(self.languagetool_launcher.as_deref())
    }
}

/// Whether `PATH` carries one executable, the same lookup a shell makes.
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
}

fn existing_file(path: &str) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    path.is_file().then_some(path)
}

/// Ask systemd whether one transient unit runs.
///
/// `is-active` exits non-zero for an inactive unit, so the exit status says
/// nothing useful and only the word on stdout does.
fn unit_state(unit: &str) -> UnitState {
    let output = Command::new("systemctl")
        .args(["--user", "is-active", unit])
        .output();
    match output {
        Ok(output) => match String::from_utf8_lossy(&output.stdout).trim() {
            "active" | "activating" | "reloading" => UnitState::Running,
            "" => UnitState::Unknown,
            _ => UnitState::Stopped,
        },
        Err(_) => UnitState::Unknown,
    }
}
