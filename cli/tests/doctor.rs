//! `grammachy doctor`, spec sections 4, 8, 10, and 12.
//!
//! Every test here builds the machine it wants as recorded [`Facts`] and reads
//! the report back, so no test looks at the developer's own machine. The one
//! end to end test runs the binary and asserts only what holds on any machine.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

use grammachy::args::EngineSlug;
use grammachy::doctor::facts::{DrmCard, KeyState, UnitState};
use grammachy::doctor::{self, Facts, Report};

/// A machine where every piece is in place and both units are stopped.
fn ready() -> Facts {
    Facts {
        binary: Some(PathBuf::from("/home/u/plugin/bin/grammachy")),
        version: "0.1.0".to_string(),
        languagetool_launcher: Some(PathBuf::from("/usr/bin/languagetool")),
        java: Some(PathBuf::from("/usr/lib/jvm/default/bin/java")),
        languagetool_address: "127.0.0.1:8081".to_string(),
        llama_server: Some(PathBuf::from("/usr/bin/llama-server")),
        models_directory: Some(PathBuf::from("/home/u/.local/share/grammachy/models")),
        model: "qwen3.8-4b".to_string(),
        model_file: Some(PathBuf::from(
            "/home/u/.local/share/grammachy/models/Qwen3.8-4B-Q4_K_M.gguf",
        )),
        openai_endpoint: Ok("127.0.0.1:8080".to_string()),
        openrouter_key: KeyState::Ready {
            path: key_path(),
            mode: 0o600,
        },
        openrouter_model: "deepseek/deepseek-v4-flash".to_string(),
        languagetool_unit: UnitState::Stopped,
        llama_unit: UnitState::Stopped,
        cards: vec![DrmCard {
            driver: "amdgpu".to_string(),
            pci_address: Some("0000:65:00.0".to_string()),
        }],
        ggml_backends: vec![
            "libggml-cpu-zen4.so".to_string(),
            "libggml-vulkan.so".to_string(),
        ],
    }
}

/// The key file path every cloud test talks about.
fn key_path() -> PathBuf {
    PathBuf::from("/home/u/.config/grammachy/openrouter-key")
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

#[test]
fn a_ready_machine_reports_nothing_missing() {
    let facts = ready();

    for engine in [
        EngineSlug::Languagetool,
        EngineSlug::Openai,
        EngineSlug::Harper,
    ] {
        let output = doctor::run(&facts, engine, false);

        assert_eq!(output.exit_code, 0, "{engine:?} is ready: {}", output.text);
        assert!(
            missing_lines(&output.text).is_empty(),
            "{engine:?}: {}",
            output.text
        );
        assert!(
            !output.text.contains("sudo pacman"),
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

#[test]
fn a_missing_languagetool_prints_its_pacman_line() {
    let mut facts = ready();
    facts.languagetool_launcher = None;

    let text = text_of(&facts, EngineSlug::Languagetool);

    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(
        text.contains("sudo pacman -S languagetool"),
        "the exact command is printed: {text}"
    );
    assert!(
        text.contains("/usr/bin/languagetool does not exist"),
        "{text}"
    );
    assert_eq!(
        doctor::run(&facts, EngineSlug::Languagetool, false).exit_code,
        1
    );
}

#[test]
fn an_installed_languagetool_has_no_pacman_line() {
    let text = text_of(&ready(), EngineSlug::Languagetool);

    assert!(
        !text.contains("sudo pacman -S languagetool"),
        "the line is gone once the package is there: {text}"
    );
}

#[test]
fn a_missing_java_runtime_prints_its_pacman_line() {
    let mut facts = ready();
    facts.java = None;

    let text = text_of(&facts, EngineSlug::Languagetool);

    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(text.contains("sudo pacman -S jre-openjdk"), "{text}");
}

#[test]
fn a_missing_llama_server_names_every_backend_package_of_the_tier() {
    let mut facts = ready();
    facts.llama_server = None;

    // A discrete GPU, so the CPU backend the server needs and Vulkan beside it.
    let text = text_of(&facts, EngineSlug::Openai);
    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(
        text.contains("sudo pacman -S llama-cpp ggml-cpu ggml-vulkan"),
        "{text}"
    );
    assert!(
        text.contains("Hardware tier discrete-gpu, so llama.cpp wants ggml-cpu and ggml-vulkan."),
        "the footer agrees with the remedy: {text}"
    );

    // The same machine with no graphics processor asks for the CPU backend.
    facts.cards = Vec::new();
    let text = text_of(&facts, EngineSlug::Openai);
    assert!(text.contains("sudo pacman -S llama-cpp ggml-cpu"), "{text}");
    assert!(!text.contains("ggml-vulkan"), "{text}");
}

/// Running the whole remedy has to leave the machine ready, so the packages the
/// llama.cpp line names are exactly the ones the backend check then wants.
#[test]
fn installing_the_whole_llama_remedy_satisfies_the_backend_check() {
    let mut facts = ready();
    facts.llama_server = None;
    facts.ggml_backends = Vec::new();

    let report = Report::new(&facts, EngineSlug::Openai);
    let remedy = report
        .checks
        .iter()
        .find(|check| check.id == "llama.cpp")
        .and_then(|check| check.remedy.clone())
        .expect("a missing server carries its install line");

    // The user runs it: the server and every named backend arrive.
    facts.llama_server = Some(PathBuf::from("/usr/bin/llama-server"));
    facts.ggml_backends = remedy
        .split_whitespace()
        .filter(|word| word.starts_with("ggml-"))
        .map(|package| match package {
            "ggml-cpu" => "libggml-cpu-zen4.so".to_string(),
            other => format!("lib{other}.so"),
        })
        .collect();

    let after = Report::new(&facts, EngineSlug::Openai);
    assert!(after.ready, "{}", after.diagnosis);
    assert!(
        !after.checks.iter().any(|check| check.remedy.is_some()),
        "nothing is left to run: {}",
        doctor::run(&facts, EngineSlug::Openai, false).text
    );
}

/// Spec section 4: `llama-cpp` carries no compute backend, so an installed
/// server beside an empty `/usr/lib/ggml` is a broken engine, not a ready one.
#[test]
fn an_installed_server_with_no_backend_is_still_a_missing_piece() {
    let mut facts = ready();
    facts.ggml_backends = Vec::new();

    let text = text_of(&facts, EngineSlug::Openai);

    // The server itself is there, so the llama.cpp line is not the missing one.
    assert!(text.contains("/usr/bin/llama-server"), "{text}");
    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(
        text.contains("sudo pacman -S ggml-cpu ggml-vulkan"),
        "{text}"
    );
}

/// The CPU tier has no Vulkan device, so it owes nothing to `ggml-vulkan`.
#[test]
fn the_cpu_tier_asks_for_the_cpu_backend_alone() {
    let mut facts = ready();
    facts.cards = Vec::new();
    facts.ggml_backends = Vec::new();

    let text = text_of(&facts, EngineSlug::Openai);

    assert!(text.contains("sudo pacman -S ggml-cpu"), "{text}");
    assert!(!text.contains("pacman -S ggml-cpu ggml-vulkan"), "{text}");
}

/// A GPU machine that has only the CPU backend runs, on the CPU, so the Vulkan
/// line is advice and never a fault. Failing it would hide the real cause.
#[test]
fn a_gpu_machine_with_only_the_cpu_backend_is_ready_and_told_about_vulkan() {
    let mut facts = ready();
    facts.ggml_backends = vec!["libggml-cpu-zen4.so".to_string()];

    let output = doctor::run(&facts, EngineSlug::Openai, false);

    assert_eq!(output.exit_code, 0, "{}", output.text);
    assert!(missing_lines(&output.text).is_empty(), "{}", output.text);
    assert!(
        output.text.contains("llama.cpp runs on the CPU"),
        "{}",
        output.text
    );
    assert!(
        output.text.contains("sudo pacman -S ggml-vulkan"),
        "{}",
        output.text
    );
    assert!(
        Report::new(&facts, EngineSlug::Openai).ready,
        "the engine runs, only on the CPU"
    );
}

/// The advisory Vulkan line must never take the diagnosis away from the piece
/// that actually stops the engine.
#[test]
fn a_missing_accelerator_never_hides_the_real_cause() {
    let mut facts = ready();
    facts.ggml_backends = vec!["libggml-cpu-zen4.so".to_string()];
    facts.model_file = None;

    let report = Report::new(&facts, EngineSlug::Openai);

    assert!(!report.ready);
    assert_eq!(
        report.diagnosis,
        "No weights for qwen3.8-4b in /home/u/.local/share/grammachy/models. Run: grammachy model download qwen3.8-4b"
    );
}

/// A GPU machine with the accelerator and no CPU backend is a genuine failure.
///
/// The old install line was `sudo pacman -S llama-cpp ggml-vulkan`, and
/// `ggml-vulkan` depends on no CPU backend, so this is what a machine that
/// followed it looks like. The line must name the gap without denying the
/// accelerator that is there.
#[test]
fn a_missing_cpu_backend_is_a_fault_even_beside_vulkan() {
    let mut facts = ready();
    facts.ggml_backends = vec!["libggml-vulkan.so".to_string()];

    let output = doctor::run(&facts, EngineSlug::Openai, false);
    let report = Report::new(&facts, EngineSlug::Openai);
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "backend")
        .expect("the backend check is there");

    assert_eq!(output.exit_code, 1, "{}", output.text);
    assert_eq!(missing_lines(&output.text).len(), 1, "{}", output.text);
    assert!(!check.ok);
    assert_eq!(check.remedy.as_deref(), Some("sudo pacman -S ggml-cpu"));
    assert!(check.detail.contains("ggml-cpu"), "{}", check.detail);
    assert!(
        !check.detail.contains("no compute backend"),
        "ggml-vulkan is installed, so the line may not deny it: {}",
        check.detail
    );
    assert!(
        !check.detail.contains("ggml-vulkan"),
        "the line names the gap only: {}",
        check.detail
    );
    assert_eq!(
        report.diagnosis,
        "llama.cpp is missing the ggml-cpu backend, which it needs to answer at all. Run: sudo pacman -S ggml-cpu"
    );
}

/// Spec section 8: the `engine_unavailable` card shows one line, and a missing
/// backend has to be a line a user can act on.
#[test]
fn the_diagnosis_names_the_missing_backend() {
    let mut facts = ready();
    facts.ggml_backends = Vec::new();

    let report = Report::new(&facts, EngineSlug::Openai);

    assert!(!report.ready);
    assert_eq!(
        report.diagnosis,
        "llama.cpp is missing the ggml-cpu and ggml-vulkan backends. It needs ggml-cpu to answer at all. Run: sudo pacman -S ggml-cpu ggml-vulkan"
    );
}

/// The default engine owes nothing to llama.cpp, backend included.
#[test]
fn a_missing_backend_never_fails_another_engine() {
    let mut facts = ready();
    facts.ggml_backends = Vec::new();

    for engine in [EngineSlug::Languagetool, EngineSlug::Harper] {
        let report = Report::new(&facts, engine);
        assert!(report.ready, "{engine:?} owes nothing to llama.cpp");
    }
}

#[test]
fn missing_weights_point_at_the_download_verb_and_never_at_pacman() {
    let mut facts = ready();
    facts.model_file = None;

    let text = text_of(&facts, EngineSlug::Openai);

    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(text.contains("No weights for qwen3.8-4b"), "{text}");
    assert!(
        text.contains("Run: grammachy model download qwen3.8-4b"),
        "{text}"
    );
}

/// The `openaiModel` field takes any name, and `unit::model_file` resolves a
/// hand-placed `.gguf`. `model download` refuses such a name, so naming the
/// verb here would hand the reader a line that always fails.
#[test]
fn missing_weights_for_a_name_outside_the_catalogue_name_no_command_to_run() {
    let mut facts = ready();
    facts.model = "something-the-user-typed".to_string();
    facts.model_file = None;

    let report = Report::new(&facts, EngineSlug::Openai);
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "model")
        .expect("the model check is there");

    assert!(!check.ok);
    assert_eq!(
        check.remedy, None,
        "a name the catalogue does not carry has no download to run"
    );
    assert!(
        check.detail.contains("something-the-user-typed"),
        "{}",
        check.detail
    );
    assert!(
        check.detail.contains("Settings, Models"),
        "the detail says what does help: {}",
        check.detail
    );
    assert!(
        !text_of(&facts, EngineSlug::Openai).contains("model download"),
        "no line the reader could copy and watch fail"
    );
}

#[test]
fn a_model_directory_that_cannot_exist_is_reported_without_a_command() {
    let mut facts = ready();
    facts.models_directory = None;
    facts.model_file = None;

    let report = Report::new(&facts, EngineSlug::Openai);
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "model")
        .expect("the model check is there");

    assert!(!check.ok);
    assert_eq!(check.remedy, None, "there is nothing to run");
    assert!(check.detail.contains("HOME is not set"), "{}", check.detail);
}

#[test]
fn a_base_url_off_this_machine_is_reported_as_the_broken_setting_it_is() {
    let mut facts = ready();
    facts.openai_endpoint = Err(
        "The OpenAI base URL must stay on this machine. Its host is api.openai.com, and v1 accepts only localhost, 127.0.0.1, and ::1."
            .to_string(),
    );

    let text = text_of(&facts, EngineSlug::Openai);

    assert_eq!(missing_lines(&text).len(), 1, "{text}");
    assert!(text.contains("must stay on this machine"), "{text}");
    // A setting is not a package, so no install line is invented for it.
    assert!(!text.contains("sudo pacman"), "{text}");
}

#[test]
fn a_binary_that_cannot_name_itself_fails_every_engine() {
    let mut facts = ready();
    facts.binary = None;

    for engine in [
        EngineSlug::Languagetool,
        EngineSlug::Openai,
        EngineSlug::Harper,
    ] {
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
fn a_piece_another_engine_needs_never_fails_this_engine() {
    let mut facts = ready();
    facts.llama_server = None;
    facts.model_file = None;

    // The default engine owes nothing to llama.cpp.
    let output = doctor::run(&facts, EngineSlug::Languagetool, false);
    assert_eq!(output.exit_code, 0, "{}", output.text);
    // The report still lists what is missing, whichever engine asked.
    assert_eq!(missing_lines(&output.text).len(), 2, "{}", output.text);

    assert_eq!(doctor::run(&facts, EngineSlug::Openai, false).exit_code, 1);
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

    let openai = Report::new(&facts, EngineSlug::Openai);
    assert!(openai.ready);
    assert!(
        openai.diagnosis.contains("127.0.0.1:8080"),
        "{}",
        openai.diagnosis
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
    facts.llama_unit = UnitState::Running;

    assert!(Report::new(&facts, EngineSlug::Languagetool)
        .diagnosis
        .contains("its unit runs on 127.0.0.1:8081"));
    assert!(Report::new(&facts, EngineSlug::Openai)
        .diagnosis
        .contains("its unit runs on 127.0.0.1:8080"));
}

#[test]
fn every_engine_slug_gets_its_own_failing_diagnosis() {
    let mut facts = ready();
    facts.languagetool_launcher = None;
    facts.llama_server = None;
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
        "LanguageTool is not installed: /usr/bin/languagetool does not exist. Run: sudo pacman -S languagetool"
    );
    assert_eq!(
        Report::new(&facts, EngineSlug::Openai).diagnosis,
        "llama.cpp is not installed: /usr/bin/llama-server does not exist. Run: sudo pacman -S llama-cpp ggml-cpu ggml-vulkan"
    );
    // Harper needs nothing but the binary, so it stays ready.
    let harper = Report::new(&facts, EngineSlug::Harper);
    assert!(harper.ready, "{}", harper.diagnosis);
}

#[test]
fn the_envelope_carries_the_contract_version_and_the_tier() {
    let json = doctor::run(&ready(), EngineSlug::Openai, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");

    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["engine"], "openai");
    assert_eq!(value["ready"], true);
    assert_eq!(value["hardwareTier"], "discrete-gpu");
    assert_eq!(
        value["backendPackages"],
        serde_json::json!(["ggml-cpu", "ggml-vulkan"])
    );
    assert!(value["diagnosis"].as_str().is_some());

    let checks = value["checks"].as_array().expect("checks is an array");
    let ids: Vec<&str> = checks
        .iter()
        .map(|check| check["id"].as_str().expect("every id is a string"))
        .collect();
    assert_eq!(
        ids,
        [
            "binary",
            "languagetool",
            "java",
            "llama.cpp",
            "backend",
            "model",
            "endpoint",
            "key",
            "unit:languagetool",
            "unit:llama",
        ]
    );
    // A check that needs nothing carries no remedy key at all.
    assert!(checks.iter().all(|check| check.get("remedy").is_none()));
}

#[test]
fn a_missing_piece_carries_its_remedy_in_the_envelope() {
    let mut facts = ready();
    facts.llama_server = None;

    let json = doctor::run(&facts, EngineSlug::Openai, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
    let check = value["checks"]
        .as_array()
        .expect("checks is an array")
        .iter()
        .find(|check| check["id"] == "llama.cpp")
        .expect("the llama.cpp check is there")
        .clone();

    assert_eq!(value["ready"], false);
    assert_eq!(check["ok"], false);
    assert_eq!(
        check["remedy"],
        "sudo pacman -S llama-cpp ggml-cpu ggml-vulkan"
    );
    assert_eq!(check["engines"], serde_json::json!(["openai"]));
}

/// The one test that runs the real binary. It asserts only what holds whatever
/// this machine has installed, so CI and a developer machine agree.
#[test]
fn the_binary_prints_a_report_and_a_json_envelope() {
    let text = run_binary(&["doctor"]);
    assert!(text.starts_with("Grammachy doctor"), "{text}");
    assert!(text.contains("Hardware tier"), "{text}");
    assert!(text.contains("Engine languagetool"), "{text}");
    // Nothing is installed for the user, whatever is missing.
    assert!(!text.contains("Installing"), "{text}");

    let json = run_binary(&["doctor", "--json"]);
    let value: Value = serde_json::from_str(json.trim()).expect("stdout is one JSON object");
    assert_eq!(value["contractVersion"], 1);
    assert_eq!(value["engine"], "languagetool");
    assert_eq!(json.trim().lines().count(), 1, "one line only: {json}");

    // Harper needs only the binary, which is running, so this machine is ready.
    let harper = run_binary(&["doctor", "--engine", "harper", "--json"]);
    let value: Value = serde_json::from_str(harper.trim()).expect("stdout is one JSON object");
    assert_eq!(value["ready"], true, "{harper}");
}

/// Spec section 4: the cloud engine needs the key file, and `doctor` reads it.
#[test]
fn a_stored_key_makes_the_cloud_engine_ready() {
    let facts = ready();

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert!(report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 0);
    assert_eq!(
        report.diagnosis,
        "The key is in place and the model is deepseek/deepseek-v4-flash. \
Checks send text to openrouter.ai."
    );
}

/// Spec section 7: `openrouterModel` has no built-in default, so the ready
/// line names that case rather than an empty model id.
#[test]
fn a_ready_cloud_engine_with_no_model_asks_for_one() {
    let mut facts = ready();
    facts.openrouter_model = String::new();

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert!(report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 0);
    assert_eq!(
        report.diagnosis,
        "The key is in place. Set the cloud model in Settings before a Check."
    );
}

/// A field of blanks is a field nobody filled in, and `settings::non_empty` is
/// the one rule that says so.
#[test]
fn a_blank_cloud_model_reads_as_no_model_at_all() {
    let mut facts = ready();
    facts.openrouter_model = "   ".to_string();

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert_eq!(
        report.diagnosis,
        "The key is in place. Set the cloud model in Settings before a Check."
    );
}

#[test]
fn a_missing_key_names_the_command_that_stores_one() {
    let mut facts = ready();
    facts.openrouter_key = KeyState::Missing(key_path());

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert!(!report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 1);
    assert!(
        report
            .diagnosis
            .contains("grammachy setup --openrouter-key"),
        "the diagnosis names the command: {}",
        report.diagnosis
    );
    assert!(
        report.diagnosis.contains("openrouter-key"),
        "the diagnosis names the file: {}",
        report.diagnosis
    );
    assert!(
        Report::new(&facts, EngineSlug::Harper).ready,
        "a missing cloud key never fails another engine"
    );
}

#[test]
fn an_empty_key_file_is_not_a_key() {
    let mut facts = ready();
    facts.openrouter_key = KeyState::Empty(key_path());

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert!(!report.ready, "{report:?}");
    assert!(
        report.diagnosis.contains("is empty"),
        "{}",
        report.diagnosis
    );
}

/// The report never states a mode it did not read: 0400 and 0700 are private
/// too, so the passing line names what the file actually is.
#[test]
fn the_key_check_names_the_mode_it_read() {
    for mode in [0o600, 0o400, 0o700] {
        let mut facts = ready();
        facts.openrouter_key = KeyState::Ready {
            path: key_path(),
            mode,
        };

        let report = Report::new(&facts, EngineSlug::Openrouter);
        let key = report
            .checks
            .iter()
            .find(|check| check.id == "key")
            .expect("the report carries the key check");

        assert!(key.ok, "{key:?}");
        assert!(key.detail.contains(&format!("0{mode:o}")), "{key:?}");
    }
}

/// A key another user can read is a key this machine no longer keeps.
#[test]
fn a_loose_key_file_fails_the_check_and_asks_for_chmod() {
    let mut facts = ready();
    facts.openrouter_key = KeyState::Loose {
        path: key_path(),
        mode: 0o644,
    };

    let report = Report::new(&facts, EngineSlug::Openrouter);

    assert!(!report.ready, "{report:?}");
    assert!(report.diagnosis.contains("0644"), "{}", report.diagnosis);
    assert!(
        report
            .diagnosis
            .contains("chmod 600 /home/u/.config/grammachy/openrouter-key"),
        "{}",
        report.diagnosis
    );
}

/// The shell tells a missing key from a key it found but cannot use, and
/// `detail` is prose and no contract, so the envelope carries a state word.
#[test]
fn the_key_check_carries_one_state_word_per_key_state() {
    let cases = [
        (
            KeyState::Ready {
                path: key_path(),
                mode: 0o600,
            },
            "ready",
        ),
        (KeyState::Missing(key_path()), "missing"),
        (KeyState::Empty(key_path()), "empty"),
        (
            KeyState::Loose {
                path: key_path(),
                mode: 0o644,
            },
            "loose",
        ),
        (KeyState::NoHome, "noHome"),
    ];

    for (state, word) in cases {
        let mut facts = ready();
        facts.openrouter_key = state;

        let json = doctor::run(&facts, EngineSlug::Openrouter, true).text;
        let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");
        let key = value["checks"]
            .as_array()
            .expect("checks is an array")
            .iter()
            .find(|check| check["id"] == "key")
            .expect("the report carries the key check")
            .clone();

        assert_eq!(key["state"], word, "{key}");
    }
}

/// Only the `key` check has states the shell must tell apart, so no other
/// check may grow a word the shell would then have to read.
#[test]
fn no_other_check_carries_a_state_word() {
    let json = doctor::run(&ready(), EngineSlug::Openrouter, true).text;
    let value: Value = serde_json::from_str(&json).expect("the envelope is one JSON object");

    for check in value["checks"].as_array().expect("checks is an array") {
        if check["id"] == "key" {
            continue;
        }
        assert!(check.get("state").is_none(), "{check}");
    }
}

/// The report says the state of the key file and never a byte of the key.
#[test]
fn no_report_line_can_carry_the_key_itself() {
    for state in [
        KeyState::Ready {
            path: key_path(),
            mode: 0o600,
        },
        KeyState::Missing(key_path()),
        KeyState::Empty(key_path()),
        KeyState::Loose {
            path: key_path(),
            mode: 0o604,
        },
        KeyState::NoHome,
    ] {
        let mut facts = ready();
        facts.openrouter_key = state;

        let report = Report::new(&facts, EngineSlug::Openrouter);

        assert!(!report.to_json().contains("sk-or-"), "{}", report.to_json());
    }
}

fn run_binary(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .args(args)
        // No test reads the developer's real settings file (spec section 7).
        .env("GRAMMACHY_SHELL_JSON", "/nonexistent/shell.json")
        .output()
        .expect("the binary runs");

    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}
