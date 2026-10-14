use grammachy::args::{CheckOptions, EngineSlug};
use grammachy::engine;
use grammachy::engines::harper::initialisations;

#[test]
fn only_a_harper_check_initialises_harper() {
    assert_eq!(initialisations(), 0);

    for slug in [EngineSlug::Languagetool, EngineSlug::Harper] {
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
