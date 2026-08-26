//! The thinking Setting of spec section 4 lives twice, and both copies have to
//! agree.
//!
//! The CLI owns the default in `settings::DEFAULT_LOCAL_THINKING` and reads
//! `localThinking` out of `shell.json` itself. The overlay owns the control
//! that writes that key, and it carries its own copy of the default in
//! `ui/settings.js`, because no QML can ask the CLI for it before a Check.
//! `Overlay.qml` cannot be instantiated outside the shell's plugin loader, so
//! reading the source is what keeps the two in step.

use grammachy::settings::DEFAULT_LOCAL_THINKING;

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{relative} is readable: {error}"))
}

/// The `fallback` of one descriptor in `ui/settings.js`.
fn descriptor_fallback(source: &str, name: &str) -> String {
    let needle = format!("{name}: {{");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("ui/settings.js declares {name}"))
        + needle.len();
    let line = &source[start..];
    let end = line.find('}').expect("the descriptor is one line");
    let descriptor = &line[..end];
    let fallback = descriptor
        .split("fallback:")
        .nth(1)
        .unwrap_or_else(|| panic!("{name} carries a fallback"));
    fallback.trim().trim_end_matches(',').to_string()
}

#[test]
fn the_overlay_default_equals_the_cli_default() {
    let source = read("ui/settings.js");

    assert_eq!(
        descriptor_fallback(&source, "localThinking"),
        DEFAULT_LOCAL_THINKING.to_string()
    );
    assert!(
        source.contains(r#"localThinking: { type: "boolean""#),
        "localThinking is the boolean of spec section 7"
    );
}

/// Spec section 7: the Toggle is shown for the Local LLM engine only, so it
/// belongs inside the group the view hides with `showsOpenai`.
#[test]
fn the_toggle_sits_in_the_local_llm_group_and_writes_the_key() {
    let source = read("ui/SettingsView.qml");

    let group = source
        .find("visible: root.showsOpenai")
        .expect("the view hides one group for the Local LLM engine");
    let toggle = source
        .find(r#"root.settingChanged("localThinking""#)
        .expect("the Toggle writes localThinking");

    assert!(
        toggle > group,
        "the Toggle is inside the group the engine hides"
    );
    assert!(
        source.contains("checked: root.localThinking"),
        "the Toggle draws the stored value"
    );
}

/// Both surfaces share one Settings view, so both have to hand it the value.
#[test]
fn both_cards_are_given_the_stored_value() {
    let overlay = read("Overlay.qml");

    assert_eq!(
        overlay
            .matches(r#"localThinking: root.setting("localThinking") === true"#)
            .count(),
        2,
        "Overlay.qml gives the value to QuickCard and to ComposeCard"
    );

    for card in ["ui/QuickCard.qml", "ui/ComposeCard.qml"] {
        let source = read(card);
        assert!(
            source.contains("property bool localThinking: true"),
            "{card} declares the property with the spec section 7 default"
        );
        assert!(
            source.contains("localThinking: root.localThinking"),
            "{card} passes it on to the Settings view"
        );
    }
}
