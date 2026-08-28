//! The README `### Dependencies` section and the `doctor` dependency table
//! must name the same packages, so a reviewer who reads either sees the whole
//! set and neither can drift (spec section 10).
//!
//! The section carries one Markdown table with the package in its first
//! column, and this test reads that column back.

use grammachy::doctor::deps::{self, SPECS};

fn readme() -> String {
    let path = format!("{}/../README.md", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).expect("README.md is readable")
}

/// The lines of one `###` section, up to the next heading of any level.
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    text.lines()
        .skip_while(|line| line.trim() != heading)
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .collect()
}

/// The `(package, required)` rows of the section's Markdown table.
fn table_rows(lines: &[&str]) -> Vec<(String, bool)> {
    lines
        .iter()
        .filter(|line| line.starts_with("| `"))
        .map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let package = cells[1].trim_matches('`').to_string();
            let required = match cells[3] {
                "yes" => true,
                "no" => false,
                other => panic!("the Required cell of {package} is yes or no, not {other}"),
            };
            (package, required)
        })
        .collect()
}

#[test]
fn the_readme_dependencies_section_names_exactly_the_packages_doctor_declares() {
    let text = readme();
    let lines = section(&text, "### Dependencies");
    assert!(
        !lines.is_empty(),
        "README.md has a ### Dependencies section"
    );

    let declared: Vec<(String, bool)> = SPECS
        .iter()
        .map(|spec| (spec.package.to_string(), spec.required))
        .collect();
    assert_eq!(table_rows(&lines), declared);
}

#[test]
fn the_readme_dependencies_section_states_the_rule() {
    let text = readme();
    let body = section(&text, "### Dependencies").join("\n");

    assert!(body.contains(deps::INSTALL_COMMAND), "{body}");
    assert!(body.contains("no `sudo` and no `pacman`"), "{body}");
    assert!(
        body.contains("`grammachy doctor` lists every dependency"),
        "{body}"
    );
    assert!(
        !body.contains("sudo pacman -S"),
        "no install line bypasses omarchy pkg add: {body}"
    );
}

/// Removal stays manual: the plugin removes no package, and the README names
/// the one optional line a user may run.
#[test]
fn the_readme_uninstall_section_names_the_optional_java_removal() {
    let text = readme();
    let body = section(&text, "## Uninstall").join("\n");
    assert!(body.contains("`sudo pacman -Rs jre-openjdk`"), "{body}");
}
