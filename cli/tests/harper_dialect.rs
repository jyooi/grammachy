//! The Target English setting reaches Harper as a dialect, spec section 7.

use grammachy::args::{CheckOptions, EngineSlug, NativeLanguage, TargetEnglish};
use grammachy::engine::Engine;
use grammachy::engines::harper::Harper;
use std::time::Duration;

const TEXT: &str = "The colour of the centre will organise itself.";

fn flagged(target: TargetEnglish) -> Vec<String> {
    let options = CheckOptions {
        native: NativeLanguage::None,
        target,
        engine: EngineSlug::Harper,
    };
    let issues = Harper::new(Duration::from_secs(120))
        .check(TEXT, &options)
        .expect("Harper answers");
    issues.into_iter().map(|issue| issue.original).collect()
}

#[test]
fn british_spellings_pass_under_en_gb_and_fail_under_en_us() {
    let british = ["colour", "centre", "organise"];

    let under_gb = flagged(TargetEnglish::EnGb);
    for word in british {
        assert!(
            !under_gb.iter().any(|f| f == word),
            "{word} flagged under en-GB: {under_gb:?}"
        );
    }

    let under_us = flagged(TargetEnglish::EnUs);
    assert!(
        british
            .iter()
            .any(|word| under_us.iter().any(|f| f == word)),
        "no British spelling flagged under en-US: {under_us:?}"
    );
}

#[test]
fn en_gb_round_trips_through_the_stored_value() {
    assert_eq!(
        TargetEnglish::from_stored("en-GB"),
        Some(TargetEnglish::EnGb)
    );
    assert_eq!(TargetEnglish::EnGb.as_str(), "en-GB");
    assert_eq!(TargetEnglish::from_stored("en-AU"), None);
}
