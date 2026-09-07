//! The published tree, spec section 10.
//!
//! `omarchy plugin add` is a plain `git clone`, so every tracked file ships to
//! every user and the manifest has no packaging exclusion. A coding agent that
//! runs in or above the installed plugin directory would read an agent
//! instruction file as its own instructions, so the tracked tree carries none.
//! `docs/contributing.md` is the home for that knowledge.

use std::process::Command;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

const AGENT_INSTRUCTION_NAMES: &[&str] = &[
    "AGENTS.md",
    "CLAUDE.md",
    "CLAUDE.local.md",
    "GEMINI.md",
    "codex.md",
    ".cursorrules",
    ".clinerules",
    ".windsurfrules",
    "copilot-instructions.md",
];

const AGENT_INSTRUCTION_DIRS: &[&str] = &[".claude", ".cursor", ".codex", ".agents"];

fn tracked_files() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(REPO)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git ls-files fails");
    String::from_utf8(output.stdout)
        .expect("paths are UTF-8")
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_tracked_tree_carries_no_agent_instruction_file() {
    let files = tracked_files();
    assert!(
        files.iter().any(|path| path == "manifest.json"),
        "git ls-files ran from the plugin root"
    );

    let offending: Vec<&String> = files
        .iter()
        .filter(|path| {
            let mut parts = path.split('/').peekable();
            let mut hit = false;
            while let Some(part) = parts.next() {
                let is_leaf = parts.peek().is_none();
                if is_leaf {
                    hit |= AGENT_INSTRUCTION_NAMES
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(part));
                } else {
                    hit |= AGENT_INSTRUCTION_DIRS.contains(&part);
                }
            }
            hit
        })
        .collect();

    assert!(
        offending.is_empty(),
        "agent instruction files in the published tree: {offending:?}"
    );
}

#[test]
fn the_contributor_guide_is_tracked() {
    assert!(tracked_files()
        .iter()
        .any(|path| path == "docs/contributing.md"));
}
