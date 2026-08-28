//! The system packages Grammachy leans on, and the one route that adds them.
//!
//! The plugin never runs `sudo` or `pacman` itself. Every package goes
//! through `omarchy pkg add <package>`, which installs only what is missing,
//! asks for the password itself, and verifies the result. The setup card and
//! the Engines page launch that command in a visible terminal; `doctor` lists
//! every package here with its state so a reviewer sees the whole set in one
//! place.
//!
//! `ui/deps.js` carries the same table for the shell, because the setup card
//! must know the required packages before `bin/grammachy` exists.
//! `cli/tests/overlay_deps.rs` keeps the two equal, and
//! `cli/tests/readme_dependencies.rs` keeps the README section equal to both.

use serde::Serialize;

use super::facts::Facts;

/// The command that installs any package, before the package name.
pub const INSTALL_COMMAND: &str = "omarchy pkg add";

/// What part of Grammachy needs a package.
pub const USED_BY_BOOTSTRAP: &str = "bootstrap";
pub const USED_BY_CAPTURE: &str = "capture";
pub const USED_BY_LANGUAGETOOL: &str = "languagetool";

/// One package as the table declares it, before the machine is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub package: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    /// The binary whose presence on `PATH` says the package is installed.
    pub probe: &'static str,
    pub used_by: &'static [&'static str],
}

/// Every package, in the order `doctor` prints them.
pub const SPECS: [Spec; 3] = [
    Spec {
        name: "curl",
        package: "curl",
        purpose: "bin/bootstrap.sh downloads the pinned companion binary with it.",
        required: true,
        probe: "curl",
        used_by: &[USED_BY_BOOTSTRAP],
    },
    Spec {
        name: "wl-clipboard",
        package: "wl-clipboard",
        purpose: "Capture, paste, and the restored Selection all go through wl-copy and wl-paste.",
        required: true,
        probe: "wl-copy",
        used_by: &[USED_BY_CAPTURE],
    },
    Spec {
        name: "Java runtime",
        package: "jre-openjdk",
        purpose: "LanguageTool runs on it, and Harper needs none.",
        required: false,
        probe: "java",
        used_by: &[USED_BY_LANGUAGETOOL],
    },
];

/// One row of the `dependencies` table of the `doctor` envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dependency {
    pub name: &'static str,
    pub package: &'static str,
    pub purpose: &'static str,
    pub required: bool,
    pub present: bool,
    #[serde(rename = "installCommand")]
    pub install_command: String,
    #[serde(rename = "usedBy")]
    pub used_by: Vec<&'static str>,
}

impl Dependency {
    /// The whole finding as one line, the shape every `doctor` line has.
    pub fn line(&self) -> String {
        if self.present {
            return self.purpose.to_string();
        }
        format!("{} Run: {}", self.purpose, self.install_command)
    }
}

/// The exact command that installs one or more packages.
pub fn install_command(packages: &[&str]) -> String {
    format!("{INSTALL_COMMAND} {}", packages.join(" "))
}

/// The table for one machine.
pub fn table(facts: &Facts) -> Vec<Dependency> {
    SPECS
        .iter()
        .map(|spec| Dependency {
            name: spec.name,
            package: spec.package,
            purpose: spec.purpose,
            required: spec.required,
            present: facts.has_binary(spec.probe),
            install_command: install_command(&[spec.package]),
            used_by: spec.used_by.to_vec(),
        })
        .collect()
}
