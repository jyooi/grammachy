//! The dependency table of spec section 10 lives twice: in
//! `cli/src/doctor/deps.rs`, which `doctor --json` prints, and in
//! `ui/deps.js`, which the setup card reads before `bin/grammachy` exists.
//! Neither side can load the other, so this test reads the shell's table and
//! holds the two equal.
//!
//! No test here opens a terminal or installs a package.

use grammachy::doctor::deps::{self, SPECS};

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The text between `var DEPENDENCIES = [` and its closing `]`.
fn js_table(source: &str) -> &str {
    let start = source
        .find("var DEPENDENCIES = [")
        .expect("ui/deps.js declares DEPENDENCIES");
    let end = source[start..].find("\n]\n").expect("DEPENDENCIES closes") + start;
    &source[start..end]
}

/// The `{ ... }` literals of the table, in order.
fn js_rows(table: &str) -> Vec<&str> {
    table
        .split("\n  {")
        .skip(1)
        .map(|row| row.split("\n  }").next().expect("a row closes"))
        .collect()
}

fn field(row: &str, name: &str) -> String {
    let needle = format!("{name}: ");
    let start = row
        .find(&needle)
        .unwrap_or_else(|| panic!("the row carries {name}: {row}"))
        + needle.len();
    let rest = &row[start..];
    let end = rest.find(",\n").unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

#[test]
fn the_js_table_equals_the_rust_table() {
    let source = read("ui/deps.js");
    let rows = js_rows(js_table(&source));
    assert_eq!(rows.len(), SPECS.len(), "one JS row per Rust spec");

    for (row, spec) in rows.iter().zip(SPECS.iter()) {
        assert_eq!(field(row, "name"), quoted(spec.name));
        assert_eq!(field(row, "package"), quoted(spec.package));
        assert_eq!(field(row, "purpose"), quoted(spec.purpose));
        assert_eq!(field(row, "required"), spec.required.to_string());
        assert_eq!(field(row, "probe"), quoted(spec.probe));
        let used_by: Vec<String> = spec.used_by.iter().map(|part| quoted(part)).collect();
        assert_eq!(field(row, "usedBy"), format!("[{}]", used_by.join(", ")));
    }

    assert!(
        source.contains(&format!(
            "var INSTALL_HINT_TAIL = \"{}\"",
            deps::INSTALL_HINT_TAIL
        )),
        "ui/deps.js names the same install hint"
    );
}
