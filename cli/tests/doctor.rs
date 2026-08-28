//! `grammachy doctor`, spec sections 4, 8, 10, and 12.
//!
//! Every test here builds the machine it wants as recorded [`Facts`] and reads
//! the report back, so no test looks at the developer's own machine. The one
//! end to end test runs the binary and asserts only what holds on any machine.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

use grammachy::args::EngineSlug;
use grammachy::doctor::facts::UnitState;
use grammachy::doctor::{self, Facts, Report};

/// A machine where every piece is in place and the unit is stopped.
fn ready() -> Facts {
    Facts {
        binary: Some(PathBuf::from("/home/u/plugin/bin/grammachy")),
        version: "0.1.0".to_string(),
        // LanguageTool is an opt-in component, so the ready machine is one
        // where the user added it from Settings (HUF-237). The pacman package
        // is the alternative and has its own cases below.
        languagetool_tree: Some(PathBuf::from(
            "/home/u/.local/share/grammachy/engines/languagetool",
        )),
        languagetool_launcher: None,
        java: Some(PathBuf::from("/usr/lib/jvm/default/bin/java")),
        languagetool_address: "127.0.0.1:8081".to_string(),
        languagetool_unit: UnitState::Stopped,
        binaries: vec![
            "curl".to_string(),
            "wl-copy".to_string(),
            "bsdtar".to_string(),
        ],
    }
}

fn text_of(facts: &Facts, engine: EngineSlug) -> String {
    doctor::run(facts, engine, false).text
}

/// The lines that report a missing piece.
fn missing_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.trim_start().starts_with("missing"))
        .collect()
}

/// The lines that report a piece the machine simply has not added, HUF-237.
fn optional_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.trim_start().starts_with("optional"))
        .collect()
}

#[test]
fn a_ready_machine_reports_nothing_missing() {
    let facts = ready();

    for engine in [EngineSlug::Languagetool, EngineSlug::Harper] {
        let output = doctor::run(&facts, engine, false);

        assert_eq!(output.exit_code, 0, "{engine:?} is ready: {}", output.text);
        assert!(
            missing_lines(&output.text).is_empty(),
            "{engine:?}: {}",
            output.text
        );
        assert!(
            !output.text.contains("Run:"),
            "nothing to install: {}",
            output.text
        );
        assert!(
            !output.text.contains("Doctor installs nothing"),
            "the manual-step footer is only for a missing piece: {}",
            output.text
        );
    }
}

/// HUF-237: a fresh install never fetched LanguageTool, so the line says the
/// engine is optional rather than that something is missing, and it names the
/// verb that adds it without a password.
#[test]
fn a_missing_languagetool_is_optional_and_names_the_no_sudo_install() {
    let mut facts = ready();
    facts.languagetool_tree = None;
    facts.languagetool_launcher = None;

    let text = text_of(&facts, EngineSlug::Languagetool);

    assert!(
        missing_lines(&text).is_empty(),
        "an engine nobody added is not a broken install: {text}"
    );
    assert_eq!(optional_lines(&text).len(), 1, "{text}");
    assert!(
        text.contains("grammachy engine install languagetool"),
        "the exact command is printed: {text}"
    );
    assert!(
        !text.contains("sudo pacman -S languagetool"),
        "the install needs no password any more: {text}"
    );
    // The engine still cannot run, so a Check on it is still refused.
    assert_eq!(
        doctor::run(&facts, EngineSlug::Languagetool, false).exit_code,
        1
    );
}

/// The Java runtime serves LanguageTool alone, so on a machine that has no
/// LanguageTool a missing runtime is optional for the same reason.
#[test]
fn a_missing_java_is_optional_only_while_languagetool_is_absent() {
    let mut facts = ready();
    facts.languagetool_tree = None;
    facts.languagetool_launcher = None;
    facts.java = None;

    let text = text_of(&facts, EngineSlug::Languagetool);
    assert!(missing_lines(&text).is_empty(), "{text}");
    // The LanguageTool check, the Java check, and the jre-openjdk row of the
    // dependency table. libarchive is present on this machine.
    assert_eq!(optional_lines(&text).len(), 3, "{text}");

    // Once LanguageTool is on the machine the server cannot start without a
    // runtime, so the same fact is a real fault.
    facts.languagetool_launcher = Some(PathBuf::from("/usr/bin/languagetool"));
    let text = text_of(&facts, EngineSlug::Languagetool);
    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(text.contains("omarchy pkg add jre-openjdk"), "{text}");
}

/// The pacman package is an alternative Grammachy never installs and never
/// removes, so the report says where that LanguageTool came from.
#[test]
fn a_languagetool_from_the_package_says_so() {
    let mut facts = ready();
    facts.languagetool_tree = None;
    facts.languagetool_launcher = Some(PathBuf::from("/usr/bin/languagetool"));

    let report = Report::new(&facts, EngineSlug::Languagetool);
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "languagetool")
        .expect("the LanguageTool check is there");

    assert!(report.ready, "{}", report.diagnosis);
    assert!(check.ok);
    assert!(!check.optional);
    assert_eq!(check.state, Some("package"));
    assert!(
        check.detail.contains("from the languagetool package"),
        "{}",
        check.detail
    );
}

/// The installed tree wins over the package, because it is the one the adapter
/// runs and the one `grammachy engine remove` can take away again.
#[test]
fn the_installed_tree_wins_over_the_package() {
    let mut facts = ready();
    facts.languagetool_launcher = Some(PathBuf::from("/usr/bin/languagetool"));

    let report = Report::new(&facts, EngineSlug::Languagetool);
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "languagetool")
        .expect("the LanguageTool check is there");

    assert_eq!(check.state, Some("installed"));
    assert_eq!(
        check.detail,
        "/home/u/.local/share/grammachy/engines/languagetool"
    );
}

#[test]
fn an_installed_languagetool_has_no_install_line() {
    let text = text_of(&ready(), EngineSlug::Languagetool);

    assert!(
        !text.contains("grammachy engine install languagetool"),
        "the line is gone once the component is there: {text}"
    );
    assert!(!text.contains("sudo pacman -S languagetool"), "{text}");
}

#[test]
fn a_missing_java_runtime_prints_its_pacman_line() {
    let mut facts = ready();
    facts.java = None;

    let text = text_of(&facts, EngineSlug::Languagetool);

    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(text.contains("omarchy pkg add jre-openjdk"), "{text}");
}

/// Harper needs nothing but the binary, so nothing LanguageTool needs ever
/// fails it.
#[test]
fn a_piece_another_engine_needs_never_fails_this_engine() {
    let mut facts = ready();
    // LanguageTool is on the machine (the ready fixture's tree), so a missing
    // Java runtime is a real fault for it rather than the optional case above.
    facts.java = None;

    let output = doctor::run(&facts, EngineSlug::Harper, false);
    assert_eq!(output.exit_code, 0, "{}", output.text);
    // The report still lists what is missing, whichever engine asked.
    assert_eq!(missing_lines(&output.text).len(), 1, "{}", output.text);

    assert_eq!(
        doctor::run(&facts, EngineSlug::Languagetool, false).exit_code,
        1
    );
}

#[test]
fn a_binary_that_cannot_name_itself_fails_every_engine() {
    let mut facts = ready();
    facts.binary = None;

    for engine in [EngineSlug::Languagetool, EngineSlug::Harper] {
        let output = doctor::run(&facts, engine, false);

        assert_eq!(output.exit_code, 1, "{engine:?}: {}", output.text);
        assert_eq!(missing_lines(&output.text).len(), 1, "{}", output.text);
    }
}

#[test]
fn a_running_unit_reads_as_running_and_a_stopped_one_is_no_fault() {
    let mut facts = ready();
    facts.languagetool_unit = UnitState::Running;

    let text = text_of(&facts, EngineSlug::Languagetool);
    assert!(
        text.contains("grammachy-languagetool is running."),
        "{text}"
    );
    assert!(missing_lines(&text).is_empty(), "{text}");

    // Stopped is the normal state between logins: the next Check starts it.
    let text = text_of(&ready(), EngineSlug::Languagetool);
    assert!(
        text.contains("grammachy-languagetool is not running. The next Check starts it."),
        "{text}"
    );
    assert!(missing_lines(&text).is_empty(), "{text}");
}

#[test]
fn a_systemd_that_does_not_answer_is_a_fault() {
    let mut facts = ready();
    facts.languagetool_unit = UnitState::Unknown;

    let output = doctor::run(&facts, EngineSlug::Languagetool, false);

    assert_eq!(output.exit_code, 1, "{}", output.text);
    assert!(
        output.text.contains("systemctl --user did not answer"),
        "{}",
        output.text
    );
}

#[test]
fn every_engine_slug_gets_its_own_ready_diagnosis() {
    let facts = ready();

    let languagetool = Report::new(&facts, EngineSlug::Languagetool);
    assert!(languagetool.ready);
    assert!(
        languagetool.diagnosis.contains("127.0.0.1:8081"),
        "{}",
        languagetool.diagnosis
    );

    let harper = Report::new(&facts, EngineSlug::Harper);
    assert!(harper.ready);
    assert!(
        harper.diagnosis.contains("inside the companion binary"),
        "{}",
        harper.diagnosis
    );
}

#[test]
fn a_running_unit_changes_the_ready_diagnosis() {
    let mut facts = ready();
    facts.languagetool_unit = UnitState::Running;

    assert!(Report::new(&facts, EngineSlug::Languagetool)
        .diagnosis
        .contains("its unit runs on 127.0.0.1:8081"));
}

#[test]
fn every_engine_slug_gets_its_own_failing_diagnosis() {
    let mut facts = ready();
    facts.languagetool_tree = None;
    facts.languagetool_launcher = None;
    facts.binary = None;

    let languagetool = Report::new(&facts, EngineSlug::Languagetool);
    assert!(!languagetool.ready);
    assert!(
        languagetool
            .diagnosis
            .contains("its own path is not readable"),
        "the first failing piece wins: {}",
        languagetool.diagnosis
    );

    // With the binary in place, each engine names its own first missing piece.
    facts.binary = Some(PathBuf::from("/home/u/plugin/bin/grammachy"));

    assert_eq!(
        Report::new(&facts, EngineSlug::Languagetool).diagnosis,
        "LanguageTool is optional and is not installed. Add it in Settings, Engines. \
Run: grammachy engine install languagetool"
    );
    // Harper needs nothing but the binary, so it stays ready.
    let harper = Report::new(&facts, EngineSlug::Harper);
    assert!(harper.ready, "{}", harper.diagnosis);
}

#[test]
fn the_envelope_carries_the_contract_version_and_the_checks() {
    let json = doctor::run(&ready(), EngineSlug::Languagetool, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");

    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["engine"], "languagetool");
    assert_eq!(value["ready"], true);
    assert!(value["diagnosis"].as_str().is_some());

    let checks = value["checks"].as_array().expect("checks is an array");
    let ids: Vec<&str> = checks
        .iter()
        .map(|check| check["id"].as_str().expect("every id is a string"))
        .collect();
    assert_eq!(ids, ["binary", "languagetool", "java", "unit:languagetool"]);
    // A check that needs nothing carries no remedy key at all.
    assert!(checks.iter().all(|check| check.get("remedy").is_none()));
}

#[test]
fn a_missing_piece_carries_its_remedy_in_the_envelope() {
    let mut facts = ready();
    facts.java = None;

    let json = doctor::run(&facts, EngineSlug::Languagetool, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
    let check = value["checks"]
        .as_array()
        .expect("checks is an array")
        .iter()
        .find(|check| check["id"] == "java")
        .expect("the java check is there")
        .clone();

    assert_eq!(value["ready"], false);
    assert_eq!(check["ok"], false);
    assert_eq!(check["remedy"], "omarchy pkg add jre-openjdk");
    assert_eq!(check["engines"], serde_json::json!(["languagetool"]));
}

/// The one test that runs the real binary. It asserts only what holds whatever
/// this machine has installed, so CI and a developer machine agree.
#[test]
fn the_binary_prints_a_report_and_a_json_envelope() {
    let text = run_binary(&["doctor"]);
    assert!(text.starts_with("Grammachy doctor"), "{text}");
    // Spec section 4: a fresh install checks with Harper, which needs nothing
    // downloaded and no pacman command (HUF-237).
    assert!(text.contains("Engine harper"), "{text}");
    // Nothing is installed for the user, whatever is missing.
    assert!(!text.contains("Installing"), "{text}");

    let json = run_binary(&["doctor", "--json"]);
    let value: Value = serde_json::from_str(json.trim()).expect("stdout is one JSON object");
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["engine"], "harper");
    assert_eq!(json.trim().lines().count(), 1, "one line only: {json}");
    // Harper needs only the binary, which is running, so this machine is
    // ready with nothing installed at all.
    assert_eq!(value["ready"], true, "{json}");
}

/// Only the `languagetool` check carries a state word: `detail` is prose and
/// no contract, so nothing else may read that instead.
#[test]
fn no_other_check_carries_a_state_word() {
    let json = doctor::run(&ready(), EngineSlug::Languagetool, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");

    for check in value["checks"].as_array().expect("checks is an array") {
        if check["id"] == "languagetool" {
            continue;
        }
        assert!(check.get("state").is_none(), "{check}");
    }
}

/// The `languagetool` check carries one word per route onto the machine, which
/// is what the Settings row reads rather than the prose of `detail`.
#[test]
fn the_languagetool_check_names_the_route_it_found() {
    let mut facts = ready();
    assert_eq!(state_of(&facts), "installed");

    facts.languagetool_tree = None;
    facts.languagetool_launcher = Some(PathBuf::from("/usr/bin/languagetool"));
    assert_eq!(state_of(&facts), "package");

    facts.languagetool_launcher = None;
    assert_eq!(state_of(&facts), "absent");
}

fn state_of(facts: &Facts) -> String {
    let json = doctor::run(facts, EngineSlug::Languagetool, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
    value["checks"]
        .as_array()
        .expect("checks is an array")
        .iter()
        .find(|check| check["id"] == "languagetool")
        .and_then(|check| check["state"].as_str())
        .expect("the LanguageTool check carries a state word")
        .to_string()
}

fn run_binary(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .args(args)
        // No test reads the developer's real settings file (spec section 7).
        .env("GRAMMACHY_SHELL_JSON", "/nonexistent/shell.json")
        // Nor the engines this machine has installed (spec section 5.4).
        .env("GRAMMACHY_ENGINES_DIR", "/nonexistent/engines")
        .output()
        .expect("the binary runs");

    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// Spec section 10: `doctor --json` is the one dependency table. Every row
/// names the Arch package, why it is there, and the exact `omarchy pkg add`
/// line that installs it, because the plugin runs no sudo and no pacman.
#[test]
fn the_envelope_carries_the_dependency_table() {
    let json = doctor::run(&ready(), EngineSlug::Harper, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");

    let rows = value["dependencies"]
        .as_array()
        .expect("dependencies is an array");
    let packages: Vec<&str> = rows
        .iter()
        .map(|row| row["package"].as_str().expect("every package is a string"))
        .collect();
    assert_eq!(
        packages,
        ["curl", "wl-clipboard", "libarchive", "jre-openjdk"]
    );

    for row in rows {
        let package = row["package"].as_str().unwrap();
        assert!(row["name"].as_str().is_some_and(|name| !name.is_empty()));
        assert!(row["purpose"]
            .as_str()
            .is_some_and(|purpose| purpose.ends_with('.')));
        assert_eq!(row["present"], true, "{package}");
        assert_eq!(
            row["installCommand"],
            format!("omarchy pkg add {package}"),
            "{package}"
        );
        assert!(!row["installCommand"].as_str().unwrap().contains("sudo"));
    }

    assert_eq!(rows[0]["required"], true);
    assert_eq!(rows[0]["usedBy"], serde_json::json!(["bootstrap"]));
    assert_eq!(rows[1]["required"], true);
    assert_eq!(rows[1]["usedBy"], serde_json::json!(["capture"]));
    assert_eq!(rows[2]["required"], false);
    assert_eq!(rows[2]["usedBy"], serde_json::json!(["languagetool"]));
    assert_eq!(rows[3]["required"], false);
    assert_eq!(rows[3]["usedBy"], serde_json::json!(["languagetool"]));
}

/// A required package that is absent reads `missing` in the text and
/// `present: false` in the envelope, and the engine answer does not move:
/// `ready` is about the engine, and the setup card is what refuses a
/// bootstrap without curl.
#[test]
fn an_absent_required_package_is_missing_and_the_engine_answer_stands() {
    let mut facts = ready();
    facts.binaries = vec!["curl".to_string(), "bsdtar".to_string()];

    let output = doctor::run(&facts, EngineSlug::Harper, false);
    assert_eq!(output.exit_code, 0, "{}", output.text);
    let lines = missing_lines(&output.text);
    assert_eq!(lines.len(), 1, "{}", output.text);
    assert!(lines[0].contains("wl-clipboard"), "{}", output.text);
    assert!(
        lines[0].contains("Run: omarchy pkg add wl-clipboard"),
        "{}",
        output.text
    );
    assert!(
        output.text.contains("Doctor installs nothing"),
        "{}",
        output.text
    );

    let json = doctor::run(&facts, EngineSlug::Harper, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
    assert_eq!(value["ready"], true);
    assert_eq!(value["dependencies"][1]["present"], false);
}

/// The Java runtime answers through `JAVA_HOME` or the default JVM, the same
/// route the LanguageTool launcher takes, and it is optional because Harper
/// needs none.
#[test]
fn the_java_package_follows_the_runtime_fact() {
    let mut facts = ready();
    facts.java = None;

    let text = text_of(&facts, EngineSlug::Harper);
    assert!(
        optional_lines(&text).iter().any(
            |line| line.contains("jre-openjdk") && line.contains("omarchy pkg add jre-openjdk")
        ),
        "{text}"
    );

    let json = doctor::run(&facts, EngineSlug::Harper, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
    assert_eq!(value["dependencies"][3]["present"], false);
    assert_eq!(value["dependencies"][3]["required"], false);
}
