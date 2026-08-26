//! The size limits the shell draws have to be the ones the CLI enforces.
//!
//! The too-long card of spec section 6 shows the Check limit in a size bar and
//! offers `Check the first N only`; Compose refuses a Draft over the cap of
//! spec section 9 before it sends anything. The overlay cannot ask the CLI for
//! either number before it has text to send, so the QML carries its own copies.
//! This test is what keeps them honest.
//!
//! The Check limit belongs to the Engine (spec section 4), so the shell copy
//! is the table in `ui/limits.js` rather than one integer property.

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
    integer_after(rest_of(source, start), name)
}

/// The value of `var <name> = <n>` in a JavaScript file.
fn js_number(source: &str, name: &str) -> usize {
    let needle = format!("var {name} = ");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("ui/limits.js declares {name}"))
        + needle.len();
    integer_after(rest_of(source, start), name)
}

fn rest_of(source: &str, start: usize) -> &str {
    &source[start..]
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
#[test]
fn the_limits_module_holds_the_cli_limit_of_every_engine() {
    let source = read("ui/limits.js");

    assert_eq!(
        js_number(&source, "LOCAL_CHECK_LIMIT_UNITS"),
        EngineSlug::Openai.check_limit_utf16()
    );
    assert_eq!(
        js_number(&source, "CHECK_LIMIT_UNITS"),
        EngineSlug::Languagetool.check_limit_utf16()
    );

    // Every other slug reads the wider limit, which is the fallback branch of
    // `checkLimit`, so only the local engine may differ from it.
    for slug in SLUGS {
        let expected = if slug == EngineSlug::Openai {
            js_number(&source, "LOCAL_CHECK_LIMIT_UNITS")
        } else {
            js_number(&source, "CHECK_LIMIT_UNITS")
        };
        assert_eq!(slug.check_limit_utf16(), expected, "{}", slug.as_str());
    }
}

/// `ui/limits.js` names the local engine by the slug the CLI accepts, because
/// that is the string the engine setting stores.
#[test]
fn the_limits_module_names_the_local_engine_slug() {
    assert!(read("ui/limits.js").contains(&format!(
        "var LOCAL_ENGINE = \"{}\"",
        EngineSlug::Openai.as_str()
    )));
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
