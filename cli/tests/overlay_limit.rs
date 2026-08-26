//! The size limits the shell draws have to be the ones the CLI enforces.
//!
//! The too-long card of spec section 6 shows the Check limit in a size bar and
//! offers `Check the first N only`; Compose refuses a Draft over the cap of
//! spec section 9 before it sends anything. The overlay cannot ask the CLI for
//! either number before it has text to send, so the QML carries its own copies.
//! This test is what keeps them honest.
//!
//! The Check limit belongs to the Engine (spec section 4), so the shell copy
//! is the table in `ui/limits.js` rather than one integer property. That file
//! is plain JavaScript, so this test runs it under node and compares answers.
//! `Overlay.qml` cannot be instantiated outside the shell's plugin loader, so
//! the two assertions about it stay source-scanning guards.

use grammachy::args::EngineSlug;
use grammachy::chunk::MAX_DRAFT_UTF16_UNITS;

const SLUGS: [EngineSlug; 3] = [
    EngineSlug::Languagetool,
    EngineSlug::Openai,
    EngineSlug::Harper,
];

/// The engine a Check runs on when nothing is stored (spec section 7).
const DEFAULT_ENGINE: EngineSlug = EngineSlug::Languagetool;

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The value of `readonly property int <name>: <n>` or `property int <name>: <n>`.
fn int_property(source: &str, name: &str) -> usize {
    let needle = format!("property int {name}:");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("the QML declares {name}"))
        + needle.len();
    integer_after(&source[start..], name)
}

/// The answer `Limits.checkLimit` gives each slug, read by running the module
/// under node rather than by reading its text.
///
/// `ui/limits.js` is plain JavaScript that node loads the way `limits.test.js`
/// does, so the contract it owns is the value it returns. `None` says node is
/// not on this machine, which is a skip rather than a failure.
fn node_check_limits(slugs: &[EngineSlug]) -> Option<Vec<usize>> {
    let module = format!("{}/../ui/limits.js", env!("CARGO_MANIFEST_DIR"));
    let program = format!(
        "const Limits = require({});\
         const slugs = {};\
         process.stdout.write(JSON.stringify(slugs.map(function (slug) {{ return Limits.checkLimit(slug) }})))",
        serde_json::to_string(&module).expect("the module path is a JSON string"),
        serde_json::to_string(&slugs.iter().map(|slug| slug.as_str()).collect::<Vec<_>>())
            .expect("the slugs are a JSON array"),
    );

    let output = match std::process::Command::new("node")
        .arg("-e")
        .arg(program)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("node ran: {error}"),
    };
    assert!(
        output.status.success(),
        "node loaded ui/limits.js: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(serde_json::from_slice(&output.stdout).expect("node answered a list of numbers"))
}

/// The first `lines` lines of the binding `<name>:` in a QML file.
fn binding_lines(source: &str, name: &str, lines: usize) -> String {
    let needle = format!("{name}:");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("the QML declares {name}"));
    source[start..]
        .lines()
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn integer_after(rest: &str, name: &str) -> usize {
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{name} is a plain integer literal"))
}

/// The one shell-side table of Check limits has to answer what the CLI does.
///
/// This runs `Limits.checkLimit` rather than reading `ui/limits.js`, so a
/// rename or a rewrite of the table is free and only a changed answer fails.
#[test]
fn the_limits_module_answers_the_cli_limit_of_every_engine() {
    let Some(answers) = node_check_limits(&SLUGS) else {
        eprintln!("skipped: node is not on PATH, so ui/limits.js cannot be run");
        return;
    };

    assert_eq!(answers.len(), SLUGS.len());
    for (slug, answer) in SLUGS.iter().zip(answers) {
        assert_eq!(answer, slug.check_limit_utf16(), "{}", slug.as_str());
    }
}

/// The overlay must read the limit off the engine setting rather than carry a
/// number of its own, or the too-long card fires at the wrong size.
#[test]
fn the_overlay_takes_its_check_limit_from_the_selected_engine() {
    let source = read("Overlay.qml");

    assert!(source.contains("import \"ui/limits.js\" as Limits"));
    assert!(source.contains(
        "readonly property int checkLimitUnits: Limits.checkLimit(root.setting(\"engine\"))"
    ));
}

/// The quick popup card is handed that limit, so its own default only has to
/// be the default engine's.
#[test]
fn the_quick_card_default_limit_equals_the_default_engine_limit() {
    assert_eq!(
        int_property(&read("ui/QuickCard.qml"), "limitUnits"),
        DEFAULT_ENGINE.check_limit_utf16()
    );
    assert!(read("Overlay.qml").contains("limitUnits: root.checkLimitUnits"));
}

#[test]
fn the_overlay_draft_cap_equals_the_cli_draft_limit() {
    assert_eq!(
        int_property(&read("Overlay.qml"), "draftCapUnits"),
        MAX_DRAFT_UTF16_UNITS
    );
}

/// Compose refuses only a Draft over the cap: anything under it is checked in
/// Chunks (spec section 9), so the Check limit is not a bound it draws.
#[test]
fn the_compose_card_default_cap_equals_the_cli_draft_limit() {
    let source = read("ui/ComposeCard.qml");
    assert_eq!(
        int_property(&source, "draftCapUnits"),
        MAX_DRAFT_UTF16_UNITS
    );
}

/// The first-N note counts what one Check read, so it must be worded from the
/// text that Check ran on rather than from the live limit.
///
/// The limit belongs to the Engine (spec section 4) and the gear sits on the
/// hero while the answer is on screen, so a note read off the limit renames
/// itself to a number no Check ever used the moment the Engine changes. The
/// wording itself is `Format.truncatedNote`, which `ui/format.test.js` runs.
#[test]
fn the_first_n_note_is_worded_from_the_checked_text() {
    let note = binding_lines(&read("ui/QuickCard.qml"), "noteText", 3);

    assert!(
        note.contains("Format.truncatedNote(root.sourceText.length"),
        "the note counts the text the Check ran on: {note}"
    );
    assert!(
        !note.contains("limitUnits"),
        "the note must not follow the live limit: {note}"
    );
}
