//! Every `Text` item that draws dynamic content must draw it as plain text.
//!
//! The Selection, the Issues an Engine answers, and the messages an error
//! envelope carries are all strings the plugin did not write. A `Text` item
//! left on its default `AutoText` mode reads markup in such a string as rich
//! text, which can restyle the card or ask Qt to fetch an image. So every
//! `Text` whose `text` is not a string literal declares `Text.PlainText`.
//!
//! `Overlay.qml` and the cards cannot be instantiated outside the shell's
//! plugin loader, so this is a source-scanning guard, like `overlay_limit.rs`.

use std::path::{Path, PathBuf};

/// One `Text {` item of one QML file, as the lines of its block.
struct TextItem {
    file: String,
    line: usize,
    block: Vec<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every QML file the plugin draws: the two roots and the cards under `ui/`.
fn qml_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = vec![root.join("Overlay.qml"), root.join("BarWidget.qml")];
    let ui = std::fs::read_dir(root.join("ui")).expect("ui/ is readable");
    for entry in ui {
        let path = entry.expect("the entry is readable").path();
        if path.extension().is_some_and(|extension| extension == "qml") {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// A line with its string literals blanked, so a brace inside one is not
/// counted as a block edge.
fn without_strings(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut inside = false;
    for character in line.chars() {
        match character {
            '"' => inside = !inside,
            _ if inside => {}
            other => out.push(other),
        }
    }
    out
}

/// The `Text {` items of one file.
///
/// A block runs from the line that opens it to the line that closes it, by
/// counting braces. `MarkedText {` and other types whose name ends in `Text`
/// are not `Text` and are skipped.
fn text_items(path: &Path) -> Vec<TextItem> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    let lines: Vec<&str> = source.lines().collect();
    let file = path
        .strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string();

    let mut items = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "Text {" {
            continue;
        }
        let mut depth = 0i32;
        let mut block = Vec::new();
        for inner in &lines[index..] {
            let bare = without_strings(inner);
            depth += bare.matches('{').count() as i32;
            depth -= bare.matches('}').count() as i32;
            block.push((*inner).to_string());
            if depth == 0 {
                break;
            }
        }
        assert!(depth == 0, "{file}:{}: the Text block closes", index + 1);
        items.push(TextItem {
            file: file.clone(),
            line: index + 1,
            block,
        });
    }
    items
}

/// The value of the first `text:` binding in a block, or `None` when the item
/// is only a container for a binding set elsewhere.
fn text_binding(block: &[String]) -> Option<String> {
    block.iter().find_map(|line| {
        line.trim()
            .strip_prefix("text:")
            .map(|value| value.trim().to_string())
    })
}

/// Whether one `text:` value is a string literal and nothing else.
fn is_literal(value: &str) -> bool {
    value.len() >= 2
        && value.starts_with('"')
        && value.ends_with('"')
        && !value[1..value.len() - 1].contains('"')
}

fn declares_plain_text(block: &[String]) -> bool {
    block
        .iter()
        .any(|line| line.trim() == "textFormat: Text.PlainText")
}

#[test]
fn the_scan_finds_the_text_items_of_the_cards() {
    let total: usize = qml_files().iter().map(|path| text_items(path).len()).sum();
    assert!(total > 30, "the scan reads the Text items, found {total}");
}

/// The guard itself: dynamic content is drawn as plain text everywhere.
#[test]
fn every_dynamic_text_item_draws_plain_text() {
    let mut offenders = Vec::new();
    for path in qml_files() {
        for item in text_items(&path) {
            let Some(value) = text_binding(&item.block) else {
                continue;
            };
            if is_literal(&value) || declares_plain_text(&item.block) {
                continue;
            }
            offenders.push(format!("{}:{} text: {value}", item.file, item.line));
        }
    }
    assert!(
        offenders.is_empty(),
        "these Text items draw dynamic content without Text.PlainText:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_literal_check_reads_only_a_whole_string() {
    assert!(is_literal("\"Accept\""));
    assert!(is_literal("\"\""));
    assert!(!is_literal("root.title"));
    assert!(!is_literal("\"Issue \" + root.count"));
    assert!(!is_literal("root.issue ? root.issue.fix : \"\""));
}
