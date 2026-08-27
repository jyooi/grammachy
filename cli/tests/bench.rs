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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    answer_with("three book", "three books", "Plural after a number.", usage)
}

/// A chat completion that quotes one span of the fixture and rewrites it.
///
/// A quote no sentence holds is dropped by `issues::normalise`, so one answer
/// reaches exactly the sentences that hold the quote and leaves the rest with
/// a valid Check and no Issue.
fn answer_with(original: &str, fix: &str, reason: &str, usage: Option<Value>) -> String {
    answer_edits(&[(original, fix, reason)], usage)
}

/// A chat completion that quotes several spans of the fixture at once.
///
/// One answer reaches every sentence, and `issues::normalise` drops the quotes
/// that sentence does not hold. So a quote unique to one item is how a case
/// reaches a chosen set of items with a single stub answer.
fn answer_edits(edits: &[(&str, &str, &str)], usage: Option<Value>) -> String {
    let content = Value::Array(
        edits
            .iter()
            .map(|(original, fix, reason)| {
                json!({
                    "original": original,
                    "fix": fix,
                    "reason": reason,
                    "category": "grammar",
                })
            })
            .collect(),
    )
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
    bodies: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// How many Checks reached this endpoint.
    fn requests(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }

    /// Every request body this endpoint read.
    ///
    /// The body is what proves which mode a run asked the server for, and the
    /// Settings are the only thing that decides it (spec section 7).
    fn bodies(&self) -> Vec<String> {
        self.bodies
            .lock()
            .expect("the recorder is readable")
            .clone()
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
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&bodies);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = read_request(&mut stream);
            recorder
                .lock()
                .expect("the recorder is readable")
                .push(body);
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

    Stub {
        address,
        served,
        bodies,
    }
}

/// The address of a stub that answers every Check.
fn stub_server(answer: String) -> String {
    stub(answer, usize::MAX).address
}

/// A ticket per request across every stub of one case.
///
/// Two rows that run beside each other take tickets in turn, and two rows that
/// run one after the other take every ticket of the first before the second.
/// That is a fact about order alone, so no case has to time anything.
#[derive(Clone)]
struct Tickets(Arc<AtomicUsize>);

impl Tickets {
    fn new() -> Tickets {
        Tickets(Arc::new(AtomicUsize::new(0)))
    }

    fn take(&self) -> usize {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// Which kind of row one stub answers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Local,
    Cloud,
}

/// The rendezvous that proves a cloud row and a local row were in flight at
/// once, without any case having to time anything.
///
/// The first cloud request holds its answer until a local row has reached its
/// own stub, and the first local request records whether a cloud request was
/// in flight when it arrived. A runner that finished every local row before
/// starting a cloud one leaves nothing in flight at that moment, so
/// `overlapped` stays false and the case fails. Both waits are bounded, so a
/// runner that never overlaps ends the case rather than hanging it.
struct Gate {
    cloud_in_flight: AtomicUsize,
    /// Local arrivals so far. Only the first consults the gate, so the rest of
    /// a forty-sentence row costs nothing once the question is answered.
    local_arrivals: AtomicUsize,
    /// Local arrivals that have finished consulting the gate. The cloud hold
    /// waits on this rather than on `local_arrivals`, so it cannot end before
    /// the first local request has looked.
    local_seen: AtomicUsize,
    overlapped: AtomicUsize,
}

impl Gate {
    fn new() -> Arc<Gate> {
        Arc::new(Gate {
            cloud_in_flight: AtomicUsize::new(0),
            local_arrivals: AtomicUsize::new(0),
            local_seen: AtomicUsize::new(0),
            overlapped: AtomicUsize::new(0),
        })
    }

    /// Whether a cloud request was in flight when a local one arrived.
    fn overlapped(&self) -> bool {
        self.overlapped.load(Ordering::SeqCst) > 0
    }

    fn arrive(&self, role: Role) {
        match role {
            Role::Cloud => {
                let first = self.cloud_in_flight.fetch_add(1, Ordering::SeqCst) == 0;
                if first {
                    // Hold the row open, so a local row still to come meets
                    // it. The bound sits far above the slowest debug Harper,
                    // which builds its whole dictionary before the first local
                    // server answers anything. Only a run with no overlap at
                    // all pays the wait. The adapter gives up on a Check after
                    // 30 s, so a hold this long loses this first Check. The
                    // row then moves on to its next sentence against the same
                    // stub, and the overlap proof stands, because no case
                    // asserts what a cloud row measured.
                    wait_until(Duration::from_secs(300), || {
                        self.local_seen.load(Ordering::SeqCst) > 0
                    });
                }
            }
            Role::Local => {
                if self.local_arrivals.fetch_add(1, Ordering::SeqCst) == 0 {
                    let busy = wait_until(Duration::from_secs(5), || {
                        self.cloud_in_flight.load(Ordering::SeqCst) > 0
                    });
                    if busy {
                        self.overlapped.store(1, Ordering::SeqCst);
                    }
                }
                self.local_seen.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn leave(&self, role: Role) {
        if role == Role::Cloud {
            self.cloud_in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Poll until the condition holds or the wait runs out, and say which happened.
fn wait_until(limit: Duration, ready: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }
    ready()
}

/// A stub that answers every connection on a thread of its own, so two rows
/// really are served at once. The single-threaded [`stub`] would hide that.
struct Concurrent {
    address: String,
    /// The ticket of every request, beside the model id its body named.
    seen: Arc<Mutex<Vec<(usize, String)>>>,
}

impl Concurrent {
    fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// The tickets of the requests that named one model, in the order taken.
    fn tickets_for(&self, model: &str) -> Vec<usize> {
        self.seen
            .lock()
            .expect("the log is not poisoned")
            .iter()
            .filter(|(_, named)| named == model)
            .map(|(ticket, _)| *ticket)
            .collect()
    }
}

fn concurrent_stub(answer: String, tickets: &Tickets, gate: &Arc<Gate>, role: Role) -> Concurrent {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    let seen: Arc<Mutex<Vec<(usize, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    let tickets = tickets.clone();
    let gate = Arc::clone(gate);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (recorder, tickets, gate, answer) = (
                Arc::clone(&recorder),
                tickets.clone(),
                Arc::clone(&gate),
                answer.clone(),
            );
            thread::spawn(move || {
                let body = read_body(&mut stream);
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push((tickets.take(), model_of(&body)));
                gate.arrive(role);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                );
                let _ = stream.flush();
                gate.leave(role);
            });
        }
    });

    Concurrent { address, seen }
}

/// The `model` field of one chat completion request.
fn model_of(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value["model"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Whether two rows took their tickets in turn rather than one after the other.
fn interleaved(first: &[usize], second: &[usize]) -> bool {
    let (Some(a_first), Some(a_last)) = (first.first(), first.last()) else {
        return false;
    };
    let (Some(b_first), Some(b_last)) = (second.first(), second.last()) else {
        return false;
    };
    a_first < b_last && b_first < a_last
}

/// A stub that rate limits the first Check and answers every later one.
///
/// That is the transient answer of spec section 4.1: the row must pay a pause
/// and keep its Check rather than count it invalid.
fn rate_limiting_stub(answer: String) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&bodies);

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = read_request(&mut stream);
            recorder
                .lock()
                .expect("the recorder is readable")
                .push(body);
            if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = write!(
                    stream,
                    "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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

    Stub {
        address,
        served,
        bodies,
    }
}

/// One entry of the record file, the shape the judge of HUF-205 reads.
#[derive(Debug, Deserialize)]
struct RecordedCheck {
    engine: String,
    model: String,
    /// The mode the row ran in, `None` for every engine but the local LLM.
    thinking: Option<bool>,
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

/// Read one whole request and answer with its body.
///
/// The body must be drained too. A stub that answers and closes on an unread
/// body resets the connection, and the adapter then reports the server as
/// unreachable rather than reading the answer.
fn read_request(stream: &mut TcpStream) -> String {
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
    String::from_utf8_lossy(&body).to_string()
}

/// Read one whole request and answer its body.
fn read_body(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8_lossy(&head).to_ascii_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    let _ = stream.read_exact(&mut body);
    String::from_utf8_lossy(&body).to_string()
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

/// Evals spec section 4.1: `--thinking` owns the mode of every local row, and
/// its default is the product default `on`.
///
/// The stored entry still decides the address, the rule of spec section 7, but
/// not the mode. A benchmark file has to be the output of its own Command line
/// rather than of a Setting its reader cannot see, and `--thinking both` runs
/// one row in each mode whatever that Setting says.
#[test]
fn the_thinking_flag_and_not_the_stored_setting_decides_the_mode_of_a_row() {
    for (flag, expected) in [(Some("off"), false), (Some("on"), true), (None, true)] {
        let endpoint = stub(answer_body(None), usize::MAX);
        // A stored `false` the flag has to win over, on every case.
        let settings = settings_file(
            "thinking.json",
            &format!(
                r#""openaiBaseUrl": "{}", "localThinking": false"#,
                endpoint.url()
            ),
        );

        let mut arguments = vec!["--engine", "openai", "--model", "qwen2.5-7b-instruct"];
        if let Some(flag) = flag {
            arguments.extend(["--thinking", flag]);
        }
        let run = bench(&settings, &arguments);

        assert_eq!(run.status, 0, "{}", run.stdout);
        let bodies = endpoint.bodies();
        assert!(!bodies.is_empty(), "the run reached the stub endpoint");
        for body in &bodies {
            let request: serde_json::Value =
                serde_json::from_str(body).expect("the request body is one JSON object");
            assert_eq!(
                request["chat_template_kwargs"]["enable_thinking"],
                serde_json::json!(expected),
                "the flag {flag:?} is what the request carries: {body}"
            );
            // HUF-226: the mode also picks the route the request is forced on.
            assert_eq!(
                request.get("grammar").is_some(),
                !expected,
                "thinking off takes the compact grammar and thinking on the schema: {body}"
            );
            assert_eq!(
                request.get("response_format").is_some(),
                expected,
                "thinking off takes the compact grammar and thinking on the schema: {body}"
            );
        }
    }
}

/// Evals spec section 4.1: one benchmark file holds both modes of every local
/// model, and the Cost table is where each row names its own.
#[test]
fn thinking_both_prints_two_rows_for_one_local_model() {
    let endpoint = stub(answer_body(None), usize::MAX);
    let settings = settings_file(
        "thinking-both.json",
        &format!(r#""openaiBaseUrl": "{}""#, endpoint.url()),
    );

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen2.5-7b-instruct",
            "--thinking",
            "both",
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stdout);
    assert!(
        run.stdout.contains(
            "Command: `grammachy bench --engine openai --model qwen2.5-7b-instruct --thinking both`."
        ),
        "the file names the flag it was generated with: {}",
        run.stdout
    );

    let cost: Vec<&str> = run
        .stdout
        .lines()
        .filter(|line| {
            line.starts_with("| `qwen2.5-7b-instruct` | on |")
                || line.starts_with("| `qwen2.5-7b-instruct` | off |")
        })
        .collect();
    assert_eq!(cost.len(), 2, "one Cost row per mode: {}", run.stdout);
    assert!(
        cost[0].starts_with("| `qwen2.5-7b-instruct` | on |"),
        "{cost:?}"
    );
    assert!(
        cost[1].starts_with("| `qwen2.5-7b-instruct` | off |"),
        "{cost:?}"
    );
    assert!(
        run.stdout
            .contains("Resident memory of `qwen2.5-7b-instruct` with thinking off is"),
        "the prose under the table says which row it means: {}",
        run.stdout
    );

    // This run may start no unit, so neither row paid for a server start and
    // neither wall time may claim one.
    let wall: Vec<&str> = run
        .stdout
        .lines()
        .filter(|line| line.starts_with("Wall time of `qwen2.5-7b-instruct`"))
        .collect();
    assert_eq!(wall.len(), 2, "one wall time per row: {}", run.stdout);
    for (line, mode) in wall.iter().zip(["on", "off"]) {
        let head = format!("Wall time of `qwen2.5-7b-instruct` with thinking {mode}: ");
        assert!(line.starts_with(&head), "{wall:?}");
        assert!(
            line.ends_with(" s for the whole fixture."),
            "no row of a run that starts no server may claim a start: {wall:?}"
        );
    }
    assert!(
        run.stdout.contains(
            "Recommended local model, the Settings default and the README line: `qwen2.5-7b-instruct`, with thinking"
        ),
        "the README line names the mode the winning row ran under: {}",
        run.stdout
    );

    // Both rows ran the whole fixture, one in each mode.
    let modes: Vec<bool> = endpoint
        .bodies()
        .iter()
        .map(|body| {
            serde_json::from_str::<serde_json::Value>(body).expect("one JSON object")
                ["chat_template_kwargs"]["enable_thinking"]
                .as_bool()
                .expect("every local request names its mode")
        })
        .collect();
    assert!(modes.iter().any(|on| *on), "one row thought");
    assert!(modes.iter().any(|on| !*on), "the other did not");
}

/// A cloud row prints `-`, because the local thinking key is a llama.cpp
/// chat-template argument and never reaches a provider.
#[test]
fn a_cloud_row_names_no_thinking_mode() {
    let settings = settings_file("thinking-cloud.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_body(Some(cloud_usage(Some(0.0001)))))
    );

    let run = bench_cloud(
        &settings,
        &[
            "--cloud-model",
            "google/gemini-3.7-flash",
            "--max-cost",
            "10",
            "--thinking",
            "both",
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stdout);
    assert!(
        cost_row(&run.stdout, "google/gemini-3.7-flash")
            .starts_with("| `google/gemini-3.7-flash` | - |"),
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
        "| `qwen2.5-3b-instruct` | on | skipped | skipped | skipped | skipped | Qwen Research License | never, the weights are non-commercial |"
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
    // Only the local LLM engine has a thinking mode, so no other row names
    // one. `judge.py` reads this field to select the product-default rows.
    assert!(harper.iter().all(|c| c.thinking.is_none()), "{harper:?}");
    assert!(cloud.iter().all(|c| c.thinking.is_none()), "{cloud:?}");

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

/// Evals spec section 4.3: one entry per engine, model, thinking mode, and
/// item, so the judge can select the product-default rows of a `both` run.
#[test]
fn record_keeps_one_entry_per_thinking_mode_of_a_local_model() {
    let endpoint = stub(answer_body(None), usize::MAX);
    let settings = settings_file(
        "record-thinking.json",
        &format!(r#""openaiBaseUrl": "{}""#, endpoint.url()),
    );
    let directory = scratch_dir().join("record-thinking-run");
    let _ = std::fs::remove_dir_all(&directory);

    let run = bench(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen2.5-7b-instruct",
            "--thinking",
            "both",
            "--record",
            directory.to_str().expect("the scratch path is UTF-8"),
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stdout);
    let text =
        std::fs::read_to_string(directory.join("checks.json")).expect("checks.json is written");
    let checks: Vec<RecordedCheck> =
        serde_json::from_str(&text).expect("the file is a list of Checks");

    let ids = fixture_ids();
    for mode in [Some(true), Some(false)] {
        let rows: Vec<&RecordedCheck> = checks
            .iter()
            .filter(|c| c.model == "qwen2.5-7b-instruct" && c.thinking == mode)
            .collect();
        assert_eq!(
            rows.iter().map(|c| c.id.clone()).collect::<Vec<String>>(),
            ids,
            "the {mode:?} row records every item"
        );
    }
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.model == "qwen2.5-7b-instruct")
            .count(),
        ids.len() * 2,
        "two rows for the named model and no more"
    );
    // The Engines `openai` row runs its own model once, in the product
    // default, because that table has no Thinking column to name a mode in.
    let engine_row: Vec<&RecordedCheck> = checks
        .iter()
        .filter(|c| c.engine == "openai" && c.model == DEFAULT_OPENAI_MODEL)
        .collect();
    assert_eq!(engine_row.len(), ids.len(), "one Engines row");
    assert!(
        engine_row.iter().all(|c| c.thinking == Some(true)),
        "{engine_row:?}"
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

/// Spec section 4.1: a silent forty-minute command is unacceptable, so every
/// sentence says on stderr which row it belongs to and how long it took.
#[test]
fn every_sentence_prints_one_progress_line_naming_its_row_item_and_time() {
    let settings = settings_file("progress.json", r#""engine": "harper""#);

    let run = bench(&settings, &[]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    let ids = fixture_ids();
    let lines: Vec<&str> = run
        .stderr
        .lines()
        .filter(|line| line.starts_with("grammachy bench: harper "))
        .collect();

    assert_eq!(
        lines.len(),
        ids.len(),
        "one line per sentence:\n{}",
        run.stderr
    );
    assert!(
        lines[0].starts_with(&format!(
            "grammachy bench: harper {} (1/{}) ",
            ids[0],
            ids.len()
        )),
        "the line names the row, the item, and its place in the fixture: {}",
        lines[0]
    );
    for (line, id) in lines.iter().zip(&ids) {
        assert!(line.contains(id), "{line} names its item");
        assert!(line.contains(" s, row "), "{line} names both times");
    }
}

/// Spec section 4.1: cloud rows run in parallel with each other and with the
/// local rows, and the tables still print in the order the plan named.
#[test]
fn cloud_rows_run_beside_each_other_and_beside_the_local_rows() {
    let tickets = Tickets::new();
    let gate = Gate::new();
    let local = concurrent_stub(answer_body(None), &tickets, &gate, Role::Local);
    let cloud = concurrent_stub(
        answer_body(Some(cloud_usage(Some(0.0001)))),
        &tickets,
        &gate,
        Role::Cloud,
    );
    let settings = settings_file(
        "parallel.json",
        &format!(r#""openaiBaseUrl": "{}""#, local.url()),
    );

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openai",
            "--model",
            "qwen3.5-4b",
            "--cloud-model",
            "deepseek/deepseek-v4-flash-0731",
            "--cloud-model",
            "google/gemini-3.7-flash",
            "--max-cost",
            "10",
        ],
        &cloud.url(),
    );

    assert_eq!(run.status, 0, "{}", run.stderr);

    let first_cloud = cloud.tickets_for("deepseek/deepseek-v4-flash-0731");
    let second_cloud = cloud.tickets_for("google/gemini-3.7-flash");
    assert_eq!(first_cloud.len(), fixture_ids().len());
    assert_eq!(second_cloud.len(), fixture_ids().len());
    assert!(
        interleaved(&first_cloud, &second_cloud),
        "the two cloud rows ran one after the other rather than beside each other"
    );
    assert!(
        gate.overlapped(),
        "no cloud Check was in flight when a local row reached its server"
    );

    // Whatever order they finished in, the tables print the plan's order:
    // local rows first, then the cloud rows as the flags named them.
    let planned = [
        "qwen3.5-4b",
        "deepseek/deepseek-v4-flash-0731",
        "google/gemini-3.7-flash",
    ];
    for table in ["### Quality", "### Cost", "### Throughput"] {
        let printed: Vec<&str> = run
            .stdout
            .split(table)
            .nth(1)
            .expect("the table is printed")
            .lines()
            .skip_while(|line| !line.starts_with("| `"))
            .take_while(|line| line.starts_with("| `"))
            .filter_map(|line| line.split('`').nth(1))
            .collect();
        assert_eq!(printed, planned, "{table} is out of plan order");
    }
}

/// Spec section 4.1: a transient answer costs the row a pause, never a Check.
#[test]
fn a_rate_limited_cloud_check_is_retried_once_and_says_so() {
    let settings = settings_file("cloud-retry.json", r#""engine": "harper""#);
    let stub = rate_limiting_stub(answer_body(Some(cloud_usage(Some(0.0001)))));

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "10",
        ],
        &format!("http://{}", stub.address),
    );

    assert_eq!(
        run.status, 0,
        "one rate limit must not fail the run: {}",
        run.stderr
    );
    assert!(
        run.stderr.contains(
            "grammachy: OpenRouter answered HTTP 429 for deepseek/deepseek-v4-flash-0731; retrying once"
        ),
        "the retry is logged:\n{}",
        run.stderr
    );
    // The retried Check answered, so the row is measured rather than skipped
    // and every sentence of the fixture is valid.
    let quality = row(&run.stdout, "deepseek/deepseek-v4-flash-0731");
    assert!(
        quality.contains("| 40 of 40 (100.0%) |"),
        "no Check was lost to the rate limit: {quality}"
    );
}

/// Six quotes, each held by exactly one fixture item, whose rewrite lands on a
/// committed hand label of `cli/tests/fixtures/judge-labels.json`.
///
/// Every one of them touches an expected span without reproducing
/// `expected_text`, so the row earns six non-exact hits and the gate is
/// measured on the sample it asks for. Two of the six labels say useful.
const LABELLED_EDITS: [(&str, &str, &str); 6] = [
    ("very like", "like very", "Word order."),
    (
        "saw to my cousin",
        "saw",
        "No preposition before the object.",
    ),
    ("to be doctor", "to a doctor", "An article before the noun."),
    (
        "He is teacher",
        "He a teacher",
        "An article before the noun.",
    ),
    (
        "received an information",
        "received information",
        "Uncountable.",
    ),
    (
        "explained me the problem",
        "explained the problem to me",
        "The indirect object takes a preposition.",
    ),
];

/// What the judge says about each of the six, agreeing with every hand label.
const LABELLED_JUDGEMENTS: &str = r#"{
  "zh-06": { "I like very this song.": { "useful": false, "reason": "The adverb is still misplaced." } },
  "es-05": { "Yesterday I saw at the market.": { "useful": false, "reason": "The object is gone." } },
  "es-07": { "My sister wants to a doctor when she grows up.": { "useful": false, "reason": "The verb is gone." } },
  "zh-04": { "He a teacher at a school near my house.": { "useful": false, "reason": "The verb is gone." } },
  "fr-07": { "I received information about the schedule.": { "useful": true, "reason": "Correct English." } },
  "fr-05": { "She explained the problem to me very clearly.": { "useful": true, "reason": "Correct English." } }
}"#;

/// The whole judgements route of the evals spec section 4.4, end to end.
///
/// The stub answers six items with a rewrite each, so six non-exact hits carry
/// a committed hand label. A judgements file that agrees with all six clears
/// both halves of the gate, the 80% agreement and the labelled sample, so the
/// Useful fix column prints and the file says it counts in the ranking.
#[test]
fn judgements_print_the_useful_fix_column_and_the_gate_the_run_cleared() {
    let settings = settings_file("judgements.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_edits(
            &LABELLED_EDITS,
            Some(cloud_usage(Some(0.0001))),
        ))
    );
    let judgements = scratch_dir().join("judgements-agree.json");
    std::fs::write(&judgements, LABELLED_JUDGEMENTS).expect("the judgements file is written");

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "10",
            "--judgements",
            judgements.to_str().expect("the scratch path is UTF-8"),
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("| F0.5 | Exact fix | Useful fix | False positives |"),
        "{}",
        run.stdout
    );
    let quality = row(&run.stdout, "deepseek/deepseek-v4-flash-0731");
    assert!(
        quality.contains("| 2 of 6 (33.3%) |"),
        "six non-exact hits, two judged useful: {quality}"
    );
    assert!(
        run.stdout.contains(
            "The judge agreed with the hand labels on 6 of 6 (100.0%), at or above the 80% gate.\nThe Useful fix column counts in the ranking.\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("Ranking: exact fix rate plus the non-exact hits the judge called useful, over the interference sentences,"),
        "{}",
        run.stdout
    );
}

/// One matched hand label is no measurement of the judge, so the gate keeps the
/// raw ranking rather than swapping the whole file's recommendation on it.
#[test]
fn a_run_under_the_labelled_sample_prints_the_column_but_never_ranks() {
    let settings = settings_file("judgements-unproven.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_with(
            "very like",
            "like very",
            "Word order.",
            Some(cloud_usage(Some(0.0001))),
        ))
    );
    // The hand label of `zh-06`: `I like very this song.` is not useful.
    let judgements = scratch_dir().join("judgements-one-label.json");
    std::fs::write(
        &judgements,
        r#"{ "zh-06": { "I like very this song.": { "useful": false, "reason": "The adverb is still misplaced." } } }"#,
    )
    .expect("the judgements file is written");

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "10",
            "--judgements",
            judgements.to_str().expect("the scratch path is UTF-8"),
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    let quality = row(&run.stdout, "deepseek/deepseek-v4-flash-0731");
    assert!(
        quality.contains("| 0 of 1 (0.0%) |"),
        "the column still prints the measurement: {quality}"
    );
    assert!(
        run.stdout.contains(
            "This run matched 1 hand label, under the 5 the 80% gate needs, so the judge is unproven here.\nThe Useful fix column does not count in the ranking, because this run matched under the 5 hand labels the gate needs.\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("Ranking: exact fix rate, then F0.5"),
        "{}",
        run.stdout
    );
}

/// A judgements file the judge and the labels do not meet on cannot be ranked
/// on, and the file has to say that rather than quietly ranking anyway.
#[test]
fn a_run_no_hand_label_covers_prints_the_column_but_excludes_it_from_the_ranking() {
    let settings = settings_file("judgements-unlabelled.json", r#""engine": "harper""#);
    let stub = format!(
        "http://{}",
        stub_server(answer_with(
            "three book",
            "three bookes",
            "Plural after a number.",
            Some(cloud_usage(Some(0.0001))),
        ))
    );
    // `zh-02` with this result text carries no hand label, so the gate cannot
    // be measured on this run.
    let judgements = scratch_dir().join("judgements-unlabelled-file.json");
    std::fs::write(
        &judgements,
        r#"{ "zh-02": { "She bought three bookes from the store.": { "useful": true, "reason": "Closer to the plural." } } }"#,
    )
    .expect("the judgements file is written");

    let run = bench_cloud(
        &settings,
        &[
            "--engine",
            "openrouter",
            "--model",
            "deepseek/deepseek-v4-flash-0731",
            "--max-cost",
            "10",
            "--judgements",
            judgements.to_str().expect("the scratch path is UTF-8"),
        ],
        &stub,
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    let quality = row(&run.stdout, "deepseek/deepseek-v4-flash-0731");
    assert!(quality.contains("| 1 of 1 (100.0%) |"), "{quality}");
    assert!(
        run.stdout.contains(
            "so the 80% gate could not be measured.\nThe Useful fix column does not count in the ranking, because no hand label covers a hit of this run.\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("Ranking: exact fix rate, then F0.5"),
        "{}",
        run.stdout
    );
}

/// A judgements file the run cannot read ends it before the first row, the
/// same promise `--record` makes: a run never pays for a report it discards.
#[test]
fn a_judgements_file_that_cannot_be_read_is_refused_before_any_row_runs() {
    let settings = settings_file("judgements-missing.json", r#""engine": "harper""#);
    let missing = scratch_dir().join("no-such-judgements.json");
    let _ = std::fs::remove_file(&missing);

    let run = bench(
        &settings,
        &[
            "--judgements",
            missing.to_str().expect("the scratch path is UTF-8"),
        ],
    );

    assert_eq!(run.status, 1, "{}", run.stdout);
    let envelope: Value = serde_json::from_str(&run.stdout).expect("an error envelope");
    assert_eq!(envelope["error"]["code"], "bad_arguments", "{envelope}");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("--judgements"),
        "{envelope}"
    );
}
