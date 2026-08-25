//! The two size limits the shell draws have to be the ones the CLI enforces.
//!
//! The too-long card of spec section 6 shows the Check limit in a size bar and
//! offers `Check the first N only`; Compose refuses a Draft over the cap of
//! spec section 9 before it sends anything. The overlay cannot ask the CLI for
//! either number before it has text to send, so the QML carries its own copies.
//! This test is what keeps them honest.

use grammachy::check::MAX_UTF16_UNITS;
use grammachy::chunk::MAX_DRAFT_UTF16_UNITS;

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
    let rest = &source[start..];
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{name} is a plain integer literal"))
}

#[test]
fn the_overlay_check_limit_equals_the_cli_limit() {
    assert_eq!(
        int_property(&read("Overlay.qml"), "checkLimitUnits"),
        MAX_UTF16_UNITS
    );
}

#[test]
fn the_quick_card_default_limit_equals_the_cli_limit() {
    assert_eq!(
        int_property(&read("ui/QuickCard.qml"), "limitUnits"),
        MAX_UTF16_UNITS
    );
}

#[test]
fn the_overlay_draft_cap_equals_the_cli_draft_limit() {
    assert_eq!(
        int_property(&read("Overlay.qml"), "draftCapUnits"),
        MAX_DRAFT_UTF16_UNITS
    );
}

#[test]
fn the_compose_card_default_limits_equal_the_cli_limits() {
    let source = read("ui/ComposeCard.qml");
    assert_eq!(int_property(&source, "checkLimitUnits"), MAX_UTF16_UNITS);
    assert_eq!(
        int_property(&source, "draftCapUnits"),
        MAX_DRAFT_UTF16_UNITS
    );
}
