//! Harper is initialised only when it is selected.
//!
//! Spec section 4 calls `harper-core` loaded only when selected, so the default
//! LanguageTool path pays nothing for it. The curated dictionary and rule set
//! are the whole cost, and this case proves nothing builds them until a Check
//! asks the `harper` engine for Issues.
//!
//! The case owns its own test binary, so no other case can move the counter.

use grammachy::args::{CheckOptions, EngineSlug};
use grammachy::engine;
use grammachy::engines::harper::initialisations;

#[test]
fn only_a_harper_check_initialises_harper() {
    assert_eq!(initialisations(), 0);

    // Building any adapter, the default one included, costs nothing.
    for slug in [
        EngineSlug::Languagetool,
        EngineSlug::Openai,
        EngineSlug::Harper,
    ] {
        drop(engine::resolve(slug));
    }
    assert_eq!(
        initialisations(),
        0,
        "resolving an engine initialised Harper"
    );

    let harper = engine::resolve(EngineSlug::Harper).expect("this build has the harper adapter");
    let options = CheckOptions {
        engine: EngineSlug::Harper,
        ..CheckOptions::default()
    };
    let issues = harper
        .check("He go home.", &options)
        .expect("the Check answers");

    assert_eq!(initialisations(), 1);
    assert!(!issues.is_empty(), "the Check found nothing to report");
}
