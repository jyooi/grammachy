//! End to end runs of `grammachy bench`, spec section 13.1.
//!
//! No case touches systemd and no case reaches a real server. LanguageTool is
//! pointed at a dead address, unit starts are forbidden, and the Models table
//! runs against a stub chat endpoint in this test binary. Harper runs for real,
//! because it needs no server.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use serde::Deserialize;
use serde_json::{json, Value};

use grammachy::settings::DEFAULT_OPENAI_MODEL;

struct Run {
    status: i32,
    stdout: String,
    stderr: String,
}

/// A chat completion that finds the plural mistake of the fixture and nothing
/// else, so every sentence gets a well-formed answer.
///
/// `usage` is what a cloud provider reports beside the answer. A local server
/// reports none, so `None` is the body the llama.cpp rows are answered with.
fn answer_body(usage: Option<Value>) -> String {
    let content = json!([{
        "original": "three book",
        "fix": "three books",
        "reason": "Plural after a number.",
        "category": "grammar",
    }])
    .to_string();
    let mut body = json!({
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": content } }]
    });
    if let Some(usage) = usage {
        body["usage"] = usage;
        body["timings"] = json!({ "prompt_ms": 12.5, "predicted_ms": 240.0 });
    }
    body.to_string()
}

/// The `usage` object of one cloud answer, priced or not.
fn cloud_usage(cost: Option<f64>) -> Value {
    let mut usage = json!({ "prompt_tokens": 31, "completion_tokens": 18 });
    if let Some(cost) = cost {
        usage["cost"] = json!(cost);
    }
    usage
}

/// A stub chat endpoint on a port the operating system picks.
struct Stub {
    address: String,
    served: Arc<AtomicUsize>,
}

impl Stub {
    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// How many Checks reached this endpoint.
    fn requests(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }
}

/// A stub that answers `answers` Checks and refuses every later one.
///
/// The refusal is HTTP 503, the answer llama.cpp gives before its weights are
/// loaded, which the adapter reads as a server that cannot run a Check yet.
/// That is how a case ends one row and leaves an earlier one measured.
fn stub(answer: String, answers: usize) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            read_request(&mut stream);
            if counter.fetch_add(1, Ordering::SeqCst) >= answers {
                let _ = write!(
                    stream,
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.flush();
                continue;
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                answer.len()
            );
            let _ = stream.flush();
        }
    });

    Stub { address, served }
}

/// The address of a stub that answers every Check.
fn stub_server(answer: String) -> String {
    stub(answer, usize::MAX).address
}

/// One entry of the record file, the shape the judge of HUF-205 reads.
#[derive(Debug, Deserialize)]
struct RecordedCheck {
    engine: String,
    model: String,
    id: String,
    valid: bool,
    latency_ms: u64,
    cost: Option<f64>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    prompt_ms: Option<f64>,
    generation_ms: Option<f64>,
    issues: Vec<RecordedIssue>,
}

/// One normalised Issue of a recorded Check, spec section 5.1.
#[derive(Debug, Deserialize)]
struct RecordedIssue {
    start: usize,
    end: usize,
    original: String,
    fix: String,
    reason: String,
    category: String,
}

/// One fixture item, read here so the record file is checked against the set
/// it was run on rather than against a count written twice.
#[derive(Debug, Deserialize)]
struct FixtureItem {
    id: String,
}

fn fixture_ids() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/interference-30.json");
    let text = std::fs::read_to_string(path).expect("the fixture is readable");
    let items: Vec<FixtureItem> = serde_json::from_str(&text).expect("the fixture is items");
    items.into_iter().map(|item| item.id).collect()
}

/// Read one whole request, headers and body.
///
/// The body must be drained too. A stub that answers and closes on an unread
/// body resets the connection, and the adapter then reports the server as
/// unreachable rather than reading the answer.
fn read_request(stream: &mut TcpStream) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let head = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    let _ = stream.read_exact(&mut body);
}

/// An address on the loopback interface with nothing listening on it.
fn silent_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    drop(listener);
    address
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bench");
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    dir
}

/// Write one temporary `shell.json`, so no run reads the real one.
///
/// An entry that names no `openaiBaseUrl` gets a silent one. The default is a
/// fixed loopback port, so a machine that already runs llama.cpp there would
/// otherwise answer a run that is meant to find nothing.
///
/// `GRAMMACHY_LLAMA_START=never` stops a start rather than a connection, so the
/// address, not that seam, is what keeps the suite off a live server.
fn settings_file(name: &str, entry_body: &str) -> PathBuf {
    let path = scratch_dir().join(name);
    // A comma only ever joins two fields that are both there, so an empty body
    // stays valid JSON rather than falling back to the real 127.0.0.1:8080.
    let mut fields: Vec<String> = vec![r#""id": "io.github.jyooi.grammachy""#.to_string()];
    if !entry_body.trim().is_empty() {
        fields.push(entry_body.trim().trim_matches(',').to_string());
    }
    if !entry_body.contains("openaiBaseUrl") {
        fields.push(format!(r#""openaiBaseUrl": "http://{}""#, silent_address()));
    }
    let entry = fields.join(", ");
    let document = format!(
        r#"{{ "bar": {{ "layout": {{ "left": [], "center": [
            {{ {entry} }}
        ], "right": [] }} }}, "plugins": [] }}"#
    );
    serde_json::from_str::<serde_json::Value>(&document).expect("the settings file is valid JSON");
    std::fs::write(&path, document).expect("the settings file is written");
    path
}

/// An entry body that names nothing still has to leave a readable file.
///
/// A stray comma made the document unparseable, `StoredSettings::load` then read
/// no entry, and the run fell back to the built-in `127.0.0.1:8080`, which is a
/// real llama-server on a developer machine. The suite stayed green throughout.
#[test]
fn an_entry_that_names_nothing_still_carries_the_silent_address() {
    for body in ["", "  ", r#""engine": "harper""#] {
        let path = settings_file("empty-entry.json", body);
        let text = std::fs::read_to_string(&path).expect("the file is written");
        let document: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"));
        let entry = &document["bar"]["layout"]["center"][0];

        assert_eq!(entry["id"], "io.github.jyooi.grammachy", "{text}");
        let base_url = entry["openaiBaseUrl"]
            .as_str()
            .unwrap_or_else(|| panic!("the entry names a base URL: {text}"));
        // A port nothing listens on, so no run reaches a real llama-server.
        assert!(base_url.starts_with("http://127.0.0.1:"), "{text}");
    }
}

/// Run `grammachy bench` with the seams that keep the suite off this machine.
///
/// The cloud engine is seamed too, onto a dead address and a scratch key file,
/// so no case can reach openrouter.ai or read the real key.
fn bench(settings: &Path, arguments: &[&str]) -> Run {
    bench_cloud(settings, arguments, &format!("http://{}", silent_address()))
}

/// The same run with the cloud engine pointed at one stub endpoint.
fn bench_cloud(settings: &Path, arguments: &[&str], openrouter_url: &str) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_grammachy"))
        .arg("bench")
        .args(arguments)
        .env("GRAMMACHY_LANGUAGETOOL_ADDRESS", silent_address())
        .env("GRAMMACHY_LANGUAGETOOL_START", "never")
        .env("GRAMMACHY_LLAMA_START", "never")
        .env("GRAMMACHY_SHELL_JSON", settings)
        .env("GRAMMACHY_OPENROUTER_URL", openrouter_url)
        .env("GRAMMACHY_OPENROUTER_KEY_FILE", key_file(settings))
        .output()
        .expect("the binary runs");

    Run {
        status: output.status.code().expect("the binary exits with a code"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

/// A scratch OpenRouter key for one case, so no case reads the real one.
///
/// The path carries the name of the case's own settings file. Cargo runs the
/// cases at once. A shared path is empty for a moment on every rewrite, and a
/// child that reads the key in that moment finds none.
fn key_file(settings: &Path) -> PathBuf {
    let case = settings
        .file_stem()
        .expect("the settings file has a name")
        .to_string_lossy()
        .to_string();
    let path = scratch_dir().join(format!("openrouter-key-{case}"));
    std::fs::write(&path, "test-key").expect("the key file is written");
    path
}

/// The first row of one engine or model: the Engines row, or the Quality row
/// of a model.
fn row<'a>(report: &'a str, name: &str) -> &'a str {
    nth_row(report, name, 0)
}

/// The Cost row of one model, the second table it appears in.
fn cost_row<'a>(report: &'a str, name: &str) -> &'a str {
    nth_row(report, name, 1)
}

fn nth_row<'a>(report: &'a str, name: &str, index: usize) -> &'a str {
    let head = format!("| `{name}` |");
    report
        .lines()
        .filter(|line| line.starts_with(&head))
        .nth(index)
        .unwrap_or_else(|| panic!("the report holds row {index} for {name}:\n{report}"))
}

#[test]
fn a_run_prints_the_engines_table_and_skips_what_it_cannot_reach() {
    let settings = settings_file("engines.json", r#""engine": "languagetool""#);

    let run = bench(&settings, &[]);

    assert_eq!(run.status, 0, "a skipped engine is not a failure");
    assert!(
        run.stdout.starts_with("# Grammachy benchmark "),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("## Engines"), "{}", run.stdout);

    // Harper needs no server, so it is measured on every machine.
    let harper = row(&run.stdout, "harper");
    assert!(harper.contains(" of 30 ("), "{harper}");
    assert!(harper.contains(" of 10 |"), "{harper}");
    assert!(harper.contains(" ms |"), "{harper}");

    // The two server engines have nothing to talk to in this test.
    assert_eq!(
        row(&run.stdout, "languagetool"),
        "| `languagetool` | skipped | skipped | skipped | skipped |"
    );
    assert!(
        run.stdout
            .contains("- Engine `languagetool`: LanguageTool did not answer"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("- Engine `openai`: No model server answered"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_run_names_the_machine_tier_and_the_regression_rule() {
    let settings = settings_file("tier.json", r#""engine": "harper""#);

    let run = bench(&settings, &[]);

    assert!(run.stdout.contains(" tier, "), "{}", run.stdout);
    assert!(run.stdout.contains("## Regression rule"), "{}", run.stdout);
    assert!(
        run.stdout
            .contains("must not drop the catch rate of the default engine, `languagetool`"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_named_model_is_evaluated_against_the_endpoint_of_the_settings() {
    let settings = settings_file(
        "models.json",
        &format!(
            r#""openaiBaseUrl": "http://{}""#,
            stub_server(answer_body(None))
        ),
    );

    let run = bench(
        &settings,
        &["--engine", "openai", "--model", "qwen2.5-7b-instruct"],
    );

    assert_eq!(run.status, 0);
    assert!(run.stdout.contains("## Models"), "{}", run.stdout);

    // The stub answers the plural mistake of zh-02 for every sentence, so the
    // row carries one catch and a false positive on every correct sentence.
    let model = row(&run.stdout, "qwen2.5-7b-instruct");
    assert!(model.contains("| 1 of 30 (3.3%) |"), "{model}");
    assert!(model.contains("| 0 of 10 |"), "{model}");
    let cost = cost_row(&run.stdout, "qwen2.5-7b-instruct");
    assert!(
        cost.contains("| 0.00 (local) | Apache-2.0 | recommended |"),
        "{cost}"
    );
    assert!(
        run.stdout
            .contains("grammachy bench --engine openai --model qwen2.5-7b-instruct"),
        "the file names the command that produced it:\n{}",
        run.stdout
    );
}

#[test]
fn a_model_with_non_commercial_weights_is_shown_but_never_recommended() {
    let settings = settings_file(
        "non-commercial.json",
        &format!(
            r#""openaiBaseUrl": "http://{}""#,
            stub_server(answer_body(None))
        ),
    );

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen2.5-3b-instruct",
            "--model",
            "qwen2.5-7b-instruct",
        ],
    );

    assert_eq!(run.status, 0);
    let restricted = cost_row(&run.stdout, "qwen2.5-3b-instruct");
    assert!(
        restricted.contains("| Qwen Research License | never, the weights are non-commercial |"),
        "{restricted}"
    );
    // It is still a full row: the table shows it for reference.
    assert!(
        row(&run.stdout, "qwen2.5-3b-instruct").contains("| 1 of 30 (3.3%) |"),
        "{}",
        run.stdout
    );
    assert!(
        cost_row(&run.stdout, "qwen2.5-7b-instruct").contains("| recommended |"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_model_on_an_engine_that_takes_none_is_a_bad_arguments_envelope() {
    let settings = settings_file("bad.json", r#""engine": "harper""#);

    let run = bench(
        &settings,
        &["--engine", "harper", "--model", "qwen2.5-7b-instruct"],
    );

    assert_eq!(run.status, 1);
    let envelope: serde_json::Value =
        serde_json::from_str(&run.stdout).expect("a failure prints one JSON envelope");
    assert_eq!(envelope["error"]["code"], "bad_arguments");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("the envelope carries a message")
            .contains("Only the openai and openrouter engines"),
        "{envelope}"
    );
}

#[test]
fn an_unreachable_model_still_carries_its_license_and_recommendation() {
    let settings = settings_file(
        "unreachable.json",
        &format!(r#""openaiBaseUrl": "http://{}""#, silent_address()),
    );

    let run = bench(
        &settings,
        &["--engine", "openai", "--model", "qwen2.5-3b-instruct"],
    );

    assert_eq!(
        run.status, 0,
        "an unreachable model is skipped, not an error"
    );
    assert_eq!(
        cost_row(&run.stdout, "qwen2.5-3b-instruct"),
        "| `qwen2.5-3b-instruct` | skipped | skipped | skipped | skipped | Qwen Research License | never, the weights are non-commercial |"
    );
    assert!(
        run.stdout
            .contains("- Model `qwen2.5-3b-instruct`: No model server answered"),
        "{}",
        run.stdout
    );
}

#[test]
fn record_writes_one_typed_entry_per_engine_model_and_fixture_item() {
    let settings = settings_file("record.json", r#""engine": "harper""#);
    let directory = scratch_dir().join("record-run");
    let _ = std::fs::remove_dir_all(&directory);
    let stub = format!(
        "http://{}",
        stub_server(answer_body(Some(cloud_usage(Some(0.0001)))))
    );

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "10",
            "--record",
            directory.to_str().expect("the scratch path is UTF-8"),
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stdout);
    let text =
        std::fs::read_to_string(directory.join("checks.json")).expect("checks.json is written");
    let checks: Vec<RecordedCheck> =
        serde_json::from_str(&text).expect("the file is a list of Checks");

    // Harper needs no server, so it and the cloud row are the two rows that
    // answer here. The other two engines have nothing to talk to.
    let ids = fixture_ids();
    let harper: Vec<&RecordedCheck> = checks.iter().filter(|c| c.engine == "harper").collect();
    let cloud: Vec<&RecordedCheck> = checks.iter().filter(|c| c.engine == "openrouter").collect();
    assert_eq!(
        checks.len(),
        harper.len() + cloud.len(),
        "no other engine answered"
    );
    assert_eq!(
        harper.iter().map(|c| c.id.clone()).collect::<Vec<String>>(),
        ids,
        "one entry per item, in fixture order"
    );
    assert_eq!(
        cloud.iter().map(|c| c.id.clone()).collect::<Vec<String>>(),
        ids
    );
    assert!(harper.iter().all(|c| c.model == "harper"), "{harper:?}");
    assert!(
        cloud
            .iter()
            .all(|c| c.model == "deepseek/deepseek-v4-flash-0731"),
        "{cloud:?}"
    );

    for check in &cloud {
        assert!(check.valid, "{check:?}");
        assert_eq!(check.cost, Some(0.0001), "{check:?}");
        assert_eq!(check.prompt_tokens, Some(31), "{check:?}");
        assert_eq!(check.completion_tokens, Some(18), "{check:?}");
        assert_eq!(check.prompt_ms, Some(12.5), "{check:?}");
        assert_eq!(check.generation_ms, Some(240.0), "{check:?}");
        assert!(check.latency_ms < 30_000, "{check:?}");
    }
    // A local engine charges nothing and reports no server timing.
    for check in &harper {
        assert_eq!(
            (check.cost, check.prompt_tokens, check.prompt_ms),
            (None, None, None),
            "{check:?}"
        );
    }

    // The stub quotes the plural mistake of zh-02, so that one entry carries a
    // normalised Issue and every other entry is a valid Check with none.
    let zh02 = cloud
        .iter()
        .find(|c| c.id == "zh-02")
        .expect("the fixture holds zh-02");
    assert_eq!(zh02.issues.len(), 1, "{zh02:?}");
    let issue = &zh02.issues[0];
    assert_eq!((issue.start, issue.end), (11, 21), "{issue:?}");
    assert_eq!(issue.original, "three book");
    assert_eq!(issue.fix, "three books");
    assert_eq!(issue.reason, "Plural after a number.");
    assert_eq!(issue.category, "grammar");
    assert!(
        cloud
            .iter()
            .filter(|c| c.id != "zh-02")
            .all(|c| c.valid && c.issues.is_empty()),
        "{cloud:?}"
    );
}

#[test]
fn the_cost_cap_ends_the_cloud_row_and_the_report_keeps_what_it_paid() {
    let settings = settings_file("cap.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_body(Some(cloud_usage(Some(0.03)))))
    );

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "0.05",
        ],
        &stub,
    );

    assert_eq!(
        run.status, 0,
        "a capped row is not a failure: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains(
            "- Model `deepseek/deepseek-v4-flash-0731`: cost cap 0.05 USD reached after 1 sentences"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains(
            "Cloud spend of this run: 0.0300 USD of the 0.05 USD cap, summed over the answers that reported a cost."
        ),
        "the row the cap ended carries no tally, and it was still billed:\n{}",
        run.stdout
    );
}

#[test]
fn a_cloud_answer_without_a_cost_ends_every_cloud_row() {
    let settings = settings_file("unpriced.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_body(Some(cloud_usage(None))))
    );

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--model",
            "google/gemini-3.7-flash",
            "--max-cost",
            "10",
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stdout);
    let unpriced = "carried no usage.cost, so this run cannot measure its spend";
    for model in ["deepseek/deepseek-v4-flash-0731", "google/gemini-3.7-flash"] {
        assert!(
            run.stdout.contains(&format!(
                "- Model `{model}`: the answer for fixture sentence zh-01 {unpriced}"
            )),
            "{}",
            run.stdout
        );
        assert!(
            cost_row(&run.stdout, model).contains("| skipped | skipped | skipped | skipped |"),
            "{}",
            run.stdout
        );
    }
    // The answer that ended the run was billed and reported no cost, so the
    // figure counts the priced answers only and says that it is a lower bound.
    assert!(
        run.stdout.contains(
            "Cloud spend of this run: 0.0000 USD of the 10 USD cap, summed over the answers that reported a cost."
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains(
            "An answer that reported no cost stays out of that sum, so the figure is a lower bound."
        ),
        "{}",
        run.stdout
    );
}

/// A pair the Engines table reports as measured must never leave the record
/// empty, whatever the Models row for the same model does afterwards.
#[test]
fn a_pair_the_engines_row_measured_stays_in_the_record_when_the_model_row_is_skipped() {
    let ids = fixture_ids();
    // The endpoint answers the Engines row's whole fixture and is gone by the
    // first sentence of the Models row.
    let endpoint = stub(answer_body(None), ids.len());
    let settings = settings_file(
        "record-skipped-model.json",
        &format!(r#""openaiBaseUrl": "{}""#, endpoint.url()),
    );
    let directory = scratch_dir().join("record-skipped-model");
    let _ = std::fs::remove_dir_all(&directory);

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            DEFAULT_OPENAI_MODEL,
            "--record",
            directory.to_str().expect("the scratch path is UTF-8"),
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains(&format!(
            "- Model `{DEFAULT_OPENAI_MODEL}`: The model server is still loading"
        )),
        "the Models row was skipped:\n{}",
        run.stdout
    );
    let engines = row(&run.stdout, "openai");
    assert!(
        engines.contains(" of 30 ("),
        "the Engines row still measured the pair: {engines}"
    );

    let text =
        std::fs::read_to_string(directory.join("checks.json")).expect("checks.json is written");
    let checks: Vec<RecordedCheck> =
        serde_json::from_str(&text).expect("the file is a list of Checks");
    let recorded: Vec<String> = checks
        .iter()
        .filter(|check| check.engine == "openai" && check.model == DEFAULT_OPENAI_MODEL)
        .map(|check| check.id.clone())
        .collect();

    assert_eq!(
        recorded, ids,
        "the measured pair is in the record exactly once"
    );
}

/// The same model named twice is one fixture run, not two, so a cloud row that
/// repeats is billed once.
#[test]
fn a_model_named_twice_runs_the_fixture_once() {
    let endpoint = stub(answer_body(None), usize::MAX);
    let settings = settings_file(
        "plan-duplicate.json",
        &format!(r#""openaiBaseUrl": "{}""#, endpoint.url()),
    );

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen3.5-4b",
            "--model",
            "qwen3.5-4b",
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(
        endpoint.requests(),
        fixture_ids().len() * 2,
        "the Engines row and one Models row, not two Models rows"
    );
    assert_eq!(
        run.stdout
            .lines()
            .filter(|line| line.starts_with("| `qwen3.5-4b` |"))
            .count(),
        4,
        "one row in each of the four Models tables:\n{}",
        run.stdout
    );
}

/// The Engines `openai` row runs the model the Settings name, so a `--model`
/// row that names the same model is the same Check run twice. The record file
/// promises one entry per engine, model, and item, so only one row writes it.
#[test]
fn a_model_row_that_repeats_the_engines_row_records_its_items_once() {
    let settings = settings_file(
        "record-default-model.json",
        &format!(
            r#""openaiBaseUrl": "http://{}""#,
            stub_server(answer_body(None))
        ),
    );
    let directory = scratch_dir().join("record-default-model");
    let _ = std::fs::remove_dir_all(&directory);

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            DEFAULT_OPENAI_MODEL,
            "--record",
            directory.to_str().expect("the scratch path is UTF-8"),
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    let text =
        std::fs::read_to_string(directory.join("checks.json")).expect("checks.json is written");
    let checks: Vec<RecordedCheck> =
        serde_json::from_str(&text).expect("the file is a list of Checks");

    let mut seen: Vec<(String, String, String)> = Vec::new();
    for check in &checks {
        let key = (check.engine.clone(), check.model.clone(), check.id.clone());
        assert!(!seen.contains(&key), "{key:?} is recorded twice");
        seen.push(key);
    }

    // The pair is still recorded, once per fixture item.
    let recorded: Vec<String> = checks
        .iter()
        .filter(|check| check.engine == "openai" && check.model == DEFAULT_OPENAI_MODEL)
        .map(|check| check.id.clone())
        .collect();
    assert_eq!(recorded, fixture_ids());

    // The Engines table still prints its own measured row.
    let engines = row(&run.stdout, "openai");
    assert!(engines.contains(" of 30 ("), "{engines}");
}

/// A run that reaches the record write has already paid for its numbers, so a
/// write that fails there must not take the report with it.
#[test]
fn a_record_write_that_fails_after_the_rows_still_prints_the_report() {
    let settings = settings_file("record-blocked.json", r#""engine": "harper""#);
    let directory = scratch_dir().join("record-blocked");
    let _ = std::fs::remove_dir_all(&directory);
    // A directory under the record file's own name lets the up-front probe of
    // the pending file pass and fails only the rename at the end of the run.
    std::fs::create_dir_all(directory.join("checks.json")).expect("the blocking directory is made");

    let run = bench(
        &settings,
        &[
            "--record",
            directory.to_str().expect("the scratch path is UTF-8"),
        ],
    );

    assert_eq!(run.status, 1, "the record failure is loud: {}", run.stderr);
    assert!(
        run.stdout.starts_with("# Grammachy benchmark "),
        "the paid report survives on stdout:\n{}",
        run.stdout
    );
    assert!(run.stdout.contains("## Engines"), "{}", run.stdout);
    assert!(
        run.stderr.contains("--record") && run.stderr.contains("cannot be written"),
        "stderr names the record failure:\n{}",
        run.stderr
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// The release habit of spec section 13.1: one file per version, produced by
/// the command above and committed as it was printed.
#[test]
fn the_committed_benchmark_file_of_this_version_is_the_output_of_the_command() {
    let version = env!("CARGO_PKG_VERSION");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/benchmarks")
        .join(format!("{version}.md"));

    let file = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{} is committed for version {version}: {error}",
            path.display()
        )
    });

    assert!(
        file.starts_with(&format!("# Grammachy benchmark {version}\n")),
        "the file is the whole output of the command, title first"
    );
    for heading in [
        "## Engines",
        "## Models",
        "## Skipped",
        "## Regression rule",
    ] {
        assert!(file.contains(heading), "the file holds {heading}");
    }
    assert!(file.contains(" tier, "), "the file names the machine tier");
    // The number itself is a release decision, not a gate (spec section 13),
    // so this only holds that the default engine was measured rather than
    // skipped in the file a release ships with.
    let default_engine = file
        .lines()
        .find(|line| line.starts_with("| `languagetool` |"))
        .expect("the file holds the row of the default engine");
    assert!(
        default_engine.contains(" of 30 ("),
        "the default engine is measured, not skipped: {default_engine}"
    );
}
