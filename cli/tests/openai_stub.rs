//! The `openai` adapter against a stub chat endpoint.
//!
//! No case starts a systemd unit. Every adapter here is built with a starter
//! the test owns, so the start behaviour is covered without llama.cpp and
//! without systemd, and the suite is the same on a developer machine and in CI.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::thread;
use std::time::{Duration, Instant};

use grammachy::args::CheckOptions;
use grammachy::engine::{Engine, EngineFailure};
use grammachy::engines::local::Started;
use grammachy::engines::openai::endpoint::Endpoint;
use grammachy::engines::openai::{Config, Openai, UnitAddress};

const TEXT: &str = "She bought three book from the store.";

/// A chat completion with the two suggestions of the recorded fixture.
const ANSWER: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant",
    "content":"[{\"original\": \"book\", \"fix\": \"books\", \"reason\": \"plural\", \"category\": \"grammar\"}]"}}]}"#;

/// What the stub tells the served-model probe of HUF-236 it holds.
///
/// The adapter reads this before its first Check, so every case here answers
/// it. `None` is the HTTP 404 of a server that names no model, and the guard
/// proceeds on that: `openaiBaseUrl` may name any OpenAI-compatible server.
type ServedModel = Option<&'static str>;

/// The model the recorded fixture's Settings ask for, which is what a server
/// has to hold for a Check to run at all.
const REQUESTED: &str = "qwen3.8-4b";

/// How long a stub waits for a port another case holds to come free again.
const REBIND_BUDGET: Duration = Duration::from_secs(5);

/// Who may take a loopback port at any one moment.
///
/// [`silent_address`] releases the port it picked, because a case needs one
/// that refuses a connection. Any bind after that may then take the very same
/// port and answer a Check the case expects nothing to answer. So a case that
/// needs a silent port holds this lock for writing for its whole run, and every
/// other bind takes it for reading. No port changes hands under a case that
/// relies on it.
static PORTS: RwLock<()> = RwLock::new(());

/// One loopback port, taken while no case holds a silent one.
fn bind_free_port() -> TcpListener {
    let _shared = PORTS
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    TcpListener::bind("127.0.0.1:0").expect("a loopback port is free")
}

/// How the stub answers one request.
#[derive(Clone, Copy)]
enum Answer {
    /// A `200` with this JSON body.
    Json(&'static str),
    /// This status line with an empty body.
    Status(&'static str),
    /// Read the request and never write, so the client runs out of time.
    Silence,
    /// A `503` for this many requests, then a `200` with this JSON body. That
    /// is llama.cpp: it binds the port before it has read the weights.
    LoadingThenJson(usize, &'static str),
}

/// A stub server on a port the operating system picks, torn down with the test.
struct Stub {
    address: String,
    /// Every request line and header block the stub read.
    seen: Arc<std::sync::Mutex<Vec<String>>>,
    /// Cleared by [`Stub::shut_down`], which is how a case frees the port.
    open: Arc<AtomicBool>,
}

impl Stub {
    fn serving(answer: Answer) -> Stub {
        Stub::holding(answer, None)
    }

    /// The same stub, naming the weights it serves to the probe.
    fn holding(answer: Answer, served: ServedModel) -> Stub {
        Stub::on(bind_free_port(), answer, served)
    }

    /// The same stub on a port a test already knows, which is how a starter
    /// brings a server up on a port that was silent a moment ago.
    ///
    /// [`PORTS`] keeps every other case out of this port, so only the operating
    /// system can hold it back now. It takes a moment to release one, and the
    /// bind waits that out rather than failing the build over it. This call
    /// takes no lock of its own, because the case that reserved the port holds
    /// the write side already.
    fn on_port(port: u16, answer: Answer, served: ServedModel) -> Stub {
        let deadline = Instant::now() + REBIND_BUDGET;
        let listener = loop {
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(listener) => break listener,
                Err(error) if Instant::now() >= deadline => {
                    panic!("port {port} stayed taken for the whole budget: {error}")
                }
                Err(_) => thread::sleep(Duration::from_millis(20)),
            }
        };
        Stub::on(listener, answer, served)
    }

    fn on(listener: TcpListener, answer: Answer, served: ServedModel) -> Stub {
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let open = Arc::new(AtomicBool::new(true));
        let answering = Arc::clone(&open);

        thread::spawn(move || {
            // Checks answered so far. The served-model probe stays out of the
            // count: it is not the request any case is about, and a llama.cpp
            // that is still loading answers it exactly as it answers a Check.
            let mut checks = 0usize;
            for stream in listener.incoming() {
                if !answering.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                let is_check = request.starts_with("POST ");
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                let loading = match answer {
                    Answer::LoadingThenJson(budget, _) => checks < budget,
                    _ => false,
                };
                if is_check {
                    checks += 1;
                }
                if !is_check && !matches!(answer, Answer::Silence) && !loading {
                    write_probe(&mut stream, served);
                    continue;
                }
                let served = checks;
                match answer {
                    Answer::Json(body) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    Answer::Status(line) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                    // Hold the connection open until the client gives up.
                    Answer::Silence => thread::sleep(Duration::from_secs(30)),
                    Answer::LoadingThenJson(budget, _) if served <= budget => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                    Answer::LoadingThenJson(_, body) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                }
            }
        });

        Stub {
            address,
            seen,
            open,
        }
    }

    /// Free the port, the way `systemctl --user stop` frees it.
    ///
    /// The accept loop owns the listener, so dropping this value alone leaves
    /// the port bound. One connection wakes that loop, it reads the flag, and
    /// the listener goes out of scope with the loop. What this server read comes
    /// back, because the case still has to say what it answered.
    fn shut_down(self) -> Vec<String> {
        let seen = self.requests();
        self.open.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.address);
        seen
    }

    /// A stub that answers the two probe routes differently.
    ///
    /// That is `llama-server --alias`: `/v1/models` reports the alias and
    /// `/props` still names the weights file it loaded.
    fn aliased(alias: &'static str, model_path: &'static str) -> Stub {
        let listener = bind_free_port();
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                let is_check = request.starts_with("POST ");
                let props = request.contains("/props");
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                if is_check {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ANSWER}",
                        ANSWER.len()
                    );
                    continue;
                }
                let body = match props {
                    true => format!(r#"{{"model_path":"{model_path}"}}"#),
                    false => format!(
                        r#"{{"object":"list","data":[{{"id":"{alias}","object":"model"}}]}}"#
                    ),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });

        Stub {
            address,
            seen,
            open: Arc::new(AtomicBool::new(true)),
        }
    }

    /// A unit an earlier session left behind, still reading its weights.
    ///
    /// It answers 503 until the first Check has been tried, which is llama.cpp
    /// reading a file. After that it names `before` until the stop lands and
    /// `after` once it has, which is what one reload looks like from here.
    fn leftover(before: &'static str, after: &'static str) -> (Stub, Stops) {
        let listener = bind_free_port();
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let stops = Stops::default();
        let stopped = Arc::clone(&stops.0);

        thread::spawn(move || {
            let mut checks = 0usize;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                let is_check = request.starts_with("POST ");
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                if is_check {
                    checks += 1;
                    if checks == 1 {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                        continue;
                    }
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ANSWER}",
                        ANSWER.len()
                    );
                    continue;
                }
                if checks == 0 {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    continue;
                }
                let served = match stopped.load(Ordering::SeqCst) {
                    0 => before,
                    _ => after,
                };
                write_probe(&mut stream, Some(served));
            }
        });

        (
            Stub {
                address,
                seen,
                open: Arc::new(AtomicBool::new(true)),
            },
            stops,
        )
    }

    /// A stub that swaps the weights it names once the unit is stopped.
    ///
    /// That is what a reload looks like from the client side: the wrong model
    /// until the stop lands, and the right one after it.
    fn reloading(answer: Answer, before: &'static str, after: &'static str) -> (Stub, Stops) {
        let listener = bind_free_port();
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let stops = Stops::default();
        let stopped = Arc::clone(&stops.0);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                let is_check = request.starts_with("POST ");
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                if !is_check {
                    let served = match stopped.load(Ordering::SeqCst) {
                        0 => before,
                        _ => after,
                    };
                    write_probe(&mut stream, Some(served));
                    continue;
                }
                let Answer::Json(body) = answer else { continue };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });

        (
            Stub {
                address,
                seen,
                open: Arc::new(AtomicBool::new(true)),
            },
            stops,
        )
    }

    /// A 307 whose Location is another server, so a follow would be visible.
    fn redirecting(location: &str) -> Stub {
        let listener = bind_free_port();
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);
        let location = location.to_string();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                let _ = write!(
                    stream,
                    "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        });

        Stub {
            address,
            seen,
            open: Arc::new(AtomicBool::new(true)),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().expect("the log is not poisoned").clone()
    }

    /// The Checks alone, without the served-model probes the adapter sends
    /// before the first Check of its life (HUF-236).
    fn checks(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .filter(|request| request.starts_with("POST "))
            .collect()
    }
}

/// Answer one served-model probe: the OpenAI model list, or a 404.
fn write_probe(stream: &mut TcpStream, served: ServedModel) {
    let Some(served) = served else {
        let _ = write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    };
    let body = format!(r#"{{"object":"list","data":[{{"id":"{served}","object":"model"}}]}}"#);
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// Read one request, headers and body, and answer it as text.
fn read_request(stream: &mut TcpStream) -> String {
    let mut seen = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).unwrap_or(0) == 1 {
        seen.push(byte[0]);
        if seen.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let headers = String::from_utf8_lossy(&seen).to_string();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);

    let mut body = vec![0u8; length];
    if length > 0 {
        let _ = stream.read_exact(&mut body);
    }
    format!("{headers}{}", String::from_utf8_lossy(&body))
}

/// How many times the adapter asked for a server to be started.
#[derive(Default)]
struct Starts(Arc<AtomicUsize>);

impl Starts {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// How many times the adapter asked for the server to be stopped.
#[derive(Default)]
struct Stops(Arc<AtomicUsize>);

impl Stops {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

/// An adapter that records its start calls instead of running systemd.
///
/// Nothing comes up behind a recording starter, so one probe is all the retry
/// loop needs to conclude and the budget is zero.
fn adapter(timeout: Duration, start_unit: bool, starts: &Starts) -> Openai {
    adapter_with_budget(timeout, start_unit, Duration::from_millis(0), starts)
}

/// The same recording adapter with a startup budget, for the one case where the
/// stub does come up on a later request.
fn adapter_with_budget(
    timeout: Duration,
    start_unit: bool,
    startup_budget: Duration,
    starts: &Starts,
) -> Openai {
    let counter = Arc::clone(&starts.0);
    Openai::with_starter(
        Config {
            timeout,
            start_unit,
            startup_budget,
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Started::Fresh)
        }),
    )
}

/// A recording adapter that also records the stop a reload runs.
///
/// No case may reach the unit the live shell uses, so the stopper is the test's
/// own here exactly as the starter is.
fn adapter_with_stopper(unit_at: &str, start_unit: bool, starts: &Starts, stops: &Stops) -> Openai {
    adapter_with_stopper_and_budget(unit_at, start_unit, Duration::from_millis(0), starts, stops)
}

/// The seam that says where the `grammachy-llama` unit listens.
///
/// The guard stops that unit only for the address it serves, so a case whose
/// stub stands for the unit hands in the stub's own address. A case that stands
/// for an Ollama, or for a hand-run server, hands in another one.
fn unit_at(address: &str) -> UnitAddress {
    let address = address.trim_start_matches("http://").to_string();
    Box::new(move || Some(address.clone()))
}

/// The same adapter with a startup budget, for the cases where the server does
/// come up, or finishes loading, on a later request.
fn adapter_with_stopper_and_budget(
    unit_address: &str,
    start_unit: bool,
    startup_budget: Duration,
    starts: &Starts,
    stops: &Stops,
) -> Openai {
    let started = Arc::clone(&starts.0);
    let stopped = Arc::clone(&stops.0);
    Openai::with_server_control(
        Config {
            timeout: Duration::from_secs(2),
            start_unit,
            startup_budget,
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            started.fetch_add(1, Ordering::SeqCst);
            Ok(Started::Fresh)
        }),
        Box::new(move |_unit: &str| {
            stopped.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
        unit_at(unit_address),
    )
}

fn options(base_url: &str) -> CheckOptions {
    CheckOptions {
        openai_base_url: base_url.to_string(),
        ..CheckOptions::default()
    }
}

/// The port half of one `host:port` address.
fn port_of(address: &str) -> u16 {
    address
        .rsplit(':')
        .next()
        .and_then(|port| port.parse().ok())
        .expect("the address names a port")
}

/// An address on the loopback interface with nothing listening on it.
///
/// The guard that comes back keeps every other case out of the port pool for
/// as long as the caller holds it, because this port is free again the moment
/// this returns. Bind it to a name, not to `_`, or it drops at once.
fn silent_address() -> (String, RwLockWriteGuard<'static, ()>) {
    let held = PORTS
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();
    drop(listener);
    (address, held)
}

#[test]
fn the_slug_is_the_engine_the_envelope_reports() {
    assert_eq!(
        adapter(Duration::from_secs(1), false, &Starts::default()).slug(),
        "openai"
    );
}

#[test]
fn a_good_answer_becomes_issues() {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let starts = Starts::default();

    let issues = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect("the stub answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].original, "book");
    assert_eq!(issues[0].fix, "books");
}

/// Spec section 4: the thinking Setting travels on the request, not on the
/// unit, so a change of it reaches the very next Check with no restart.
#[test]
fn the_thinking_setting_is_carried_on_every_request() {
    for thinking in [true, false] {
        let stub = Stub::serving(Answer::Json(ANSWER));
        let starts = Starts::default();
        let options = CheckOptions {
            local_thinking: thinking,
            ..options(&stub.base_url())
        };

        adapter(Duration::from_secs(2), true, &starts)
            .check(TEXT, &options)
            .expect("the stub answers");

        let request = stub.requests().join("");
        assert!(
            request.contains(&format!(
                r#""chat_template_kwargs":{{"enable_thinking":{thinking}}}"#
            )),
            "thinking {thinking} is on the wire:\n{request}"
        );
        assert!(request.contains(r#""max_tokens":2048"#), "{request}");
    }
}

#[test]
fn a_port_that_already_answers_starts_no_unit() {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let starts = Starts::default();

    adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect("the stub answers");

    assert_eq!(starts.count(), 0, "a running server is never restarted");
}

#[test]
fn a_silent_port_starts_the_unit_once() {
    let (address, _ports) = silent_address();
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&format!("http://{address}")))
        .expect_err("nothing listens on the port");

    assert_eq!(starts.count(), 1, "the adapter asked for one start");
    match failure {
        // The recording starter brings nothing up, so the retry gives up and
        // the shell sees the card that tells the user to check the engine.
        EngineFailure::Unavailable(message) => {
            assert!(
                message.contains(&address),
                "the message names it: {message}"
            )
        }
        other => panic!("expected engine_unavailable, got {other:?}"),
    }
}

#[test]
fn a_silent_port_starts_no_unit_when_the_seam_forbids_it() {
    let (address, _ports) = silent_address();
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), false, &starts)
        .check(TEXT, &options(&format!("http://{address}")))
        .expect_err("nothing listens on the port");

    assert_eq!(starts.count(), 0);
    assert!(
        matches!(failure, EngineFailure::Unavailable(_)),
        "{failure:?}"
    );
}

#[test]
fn a_server_that_never_answers_is_a_timeout() {
    let stub = Stub::serving(Answer::Silence);
    let starts = Starts::default();

    let failure = adapter(Duration::from_millis(300), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the stub never answers");

    match failure {
        EngineFailure::Timeout(message) => assert!(
            message.contains(&stub.address),
            "the message names the address: {message}"
        ),
        other => panic!("expected a timeout, got {other:?}"),
    }
    // A server that is up but slow is not a server that needs starting.
    assert_eq!(starts.count(), 0);
}

/// llama.cpp answers 503 while it loads the weights, which the adapter must
/// wait out rather than report; the pilot of HUF-209 failed every sentence
/// of both local rows before this was covered.
#[test]
fn a_server_that_is_still_loading_is_waited_for_not_reported() {
    let stub = Stub::serving(Answer::Status("503 Service Unavailable"));
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the stub never finishes loading");

    assert_eq!(
        starts.count(),
        1,
        "a loading server is asked for once and then waited for"
    );
    assert!(
        matches!(&failure, EngineFailure::Unavailable(message) if message.contains("still loading")),
        "expected engine_unavailable, got {failure:?}"
    );
}

#[test]
fn a_server_error_is_an_engine_error() {
    let stub = Stub::serving(Answer::Status("500 Internal Server Error"));
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the stub fails the request");

    assert!(
        matches!(failure, EngineFailure::Failed(ref message) if message.contains("500")),
        "expected engine_error, got {failure:?}"
    );
}

/// llama.cpp binds its port before it has read the weights and answers 503
/// until it has. That is the server not being up yet, so it is what the startup
/// budget waits out rather than an engine error the user is shown.
#[test]
fn a_server_still_loading_its_weights_is_waited_out_rather_than_failed() {
    let stub = Stub::serving(Answer::LoadingThenJson(2, ANSWER));
    let starts = Starts::default();
    let adapter = adapter_with_budget(
        Duration::from_secs(2),
        true,
        Duration::from_secs(5),
        &starts,
    );

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the server finishes loading and answers");

    assert_eq!(issues.len(), 1);
    // One Check found it loading, the retry loop found it loading again, and
    // the third one got the answer. The probes before them found it loading
    // too, which is what a llama.cpp reading its weights answers everything.
    assert_eq!(stub.checks().len(), 3);
    // The unit is asked once and never again: `systemctl start` on a unit that
    // is already running is a no-op, so waiting is what earns the answer. A
    // count that climbed with the retries would mean the loop was restarting a
    // server that was already coming up.
    assert_eq!(
        starts.count(),
        1,
        "the unit is asked for once and then waited for"
    );
}

/// With no budget left, a server that is still loading is the
/// `engine_unavailable` card, which is the one that explains a first Check.
#[test]
fn a_server_still_loading_with_no_budget_left_is_engine_unavailable() {
    let stub = Stub::serving(Answer::Status("503 Service Unavailable"));
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the stub never finishes loading");

    match failure {
        EngineFailure::Unavailable(message) => assert!(
            message.contains("still loading"),
            "the message says what the server is doing: {message}"
        ),
        other => panic!("expected engine_unavailable, got {other:?}"),
    }
}

#[test]
fn an_answer_that_is_not_a_chat_completion_is_an_engine_error() {
    let stub = Stub::serving(Answer::Json("this is not JSON"));
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the body does not parse");

    assert!(
        matches!(failure, EngineFailure::Failed(_)),
        "expected engine_error, got {failure:?}"
    );
}

#[test]
fn a_remote_base_url_is_refused_before_anything_is_sent() {
    let starts = Starts::default();

    for base_url in [
        "https://api.openai.com/v1",
        "http://example.com:8080",
        "http://192.168.1.10:8080",
    ] {
        let failure = adapter(Duration::from_secs(2), true, &starts)
            .check(TEXT, &options(base_url))
            .expect_err("a remote host is refused");

        assert!(
            matches!(failure, EngineFailure::BadArguments(_)),
            "{base_url}: expected bad_arguments, got {failure:?}"
        );
    }
    assert_eq!(starts.count(), 0, "a refused base URL starts nothing");
}

#[test]
fn the_request_carries_the_prompt_the_model_and_the_key() {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let starts = Starts::default();
    let options = CheckOptions {
        openai_base_url: stub.base_url(),
        openai_model: "some-other-model".to_string(),
        openai_api_key: "sk-local-secret".to_string(),
        native: grammachy::args::NativeLanguage::Zh,
        ..CheckOptions::default()
    };

    adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options)
        .expect("the stub answers");

    let requests = stub.checks();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];

    assert!(
        request.starts_with("POST /v1/chat/completions "),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-local-secret"),
        "{request}"
    );
    assert!(request.contains("some-other-model"), "{request}");
    assert!(request.contains("Mandarin Chinese"), "{request}");
    assert!(request.contains("She bought three book"), "{request}");
}

#[test]
fn thinking_off_sends_the_grammar_and_no_response_format() {
    // HUF-219: llama-server is the one server that takes a raw GBNF, and the
    // grammar is what forbids the whitespace the response format allowed.
    let body = sent_body(false);

    assert_eq!(
        body["grammar"],
        serde_json::json!(grammachy::engines::openai::prompt::GRAMMAR)
    );
    assert!(body.get("response_format").is_none(), "{body}");
}

#[test]
fn thinking_on_sends_the_response_format_and_no_grammar() {
    // A raw grammar bounds the whole generation, so it leaves a thinking model
    // no room to think. Spec section 4 keeps both Toggle positions live.
    let body = sent_body(true);

    assert_eq!(body["response_format"]["type"], "json_schema");
    assert!(body.get("grammar").is_none(), "{body}");
}

/// The JSON body of the one request a Check with this thinking Setting sends.
fn sent_body(thinking: bool) -> serde_json::Value {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let starts = Starts::default();
    let options = CheckOptions {
        local_thinking: thinking,
        ..options(&stub.base_url())
    };

    adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options)
        .expect("the stub answers");

    let request = stub.checks().remove(0);
    serde_json::from_str(
        request
            .split_once("\r\n\r\n")
            .expect("the request has a body")
            .1,
    )
    .expect("the body is JSON")
}

#[test]
fn a_redirect_is_not_followed() {
    let target = Stub::serving(Answer::Json(ANSWER));
    let location = format!("{}/v1/chat/completions", target.base_url());
    let stub = Stub::redirecting(&location);
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("a redirect is not a chat completion");

    assert_eq!(stub.checks().len(), 1, "the local port is checked once");
    assert!(
        target.requests().is_empty(),
        "the POST must not repeat to Location"
    );
    assert!(
        matches!(failure, EngineFailure::Failed(_)),
        "expected engine_error, got {failure:?}"
    );
}

#[test]
fn an_empty_api_key_sends_no_authorization_header() {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let starts = Starts::default();

    adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect("the stub answers");

    let requests = stub.checks();
    assert!(
        !requests[0].to_ascii_lowercase().contains("authorization:"),
        "{}",
        requests[0]
    );
}

/// HUF-236. llama-server ignores the `model` field of the request, so a server
/// left over from an earlier session answers with the weights it already
/// holds. The guard reads what it serves before the first Check, and a named
/// mismatch is refused rather than checked.
#[test]
fn a_server_that_serves_another_model_never_answers_a_check() {
    let stub = Stub::holding(Answer::Json(ANSWER), Some("granite-4.2-3b-Q4_K_M.gguf"));
    let (starts, stops) = (Starts::default(), Stops::default());

    let failure = adapter_with_stopper(&stub.address, false, &starts, &stops)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the server holds another model");

    assert!(
        stub.checks().is_empty(),
        "nothing is checked against the wrong model: {:?}",
        stub.requests()
    );
    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
}

/// The weights the server named are the weights the Check asked for, so the
/// Check runs and the row can report what it measured on.
#[test]
fn a_server_that_serves_the_requested_weights_is_checked_and_named() {
    let served = "qwen3.8-4b-Q4_K_M.gguf";
    let stub = Stub::holding(Answer::Json(ANSWER), Some(served));
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper(&stub.address, true, &starts, &stops);

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the stub answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(adapter.served_weights().as_deref(), Some(served));
    assert_eq!(stops.count(), 0, "a server that matches is never reloaded");
    assert_eq!(starts.count(), 0);
}

/// A mismatch the adapter may reload is reloaded, not refused: the stop frees
/// the port and the server comes back on the weights the Check asked for.
#[test]
fn a_mismatch_is_reloaded_before_the_first_check() {
    let (stub, stops) = Stub::reloading(
        Answer::Json(ANSWER),
        "granite-4.2-3b-Q4_K_M.gguf",
        "qwen3.8-4b-Q4_K_M.gguf",
    );
    let starts = Starts::default();
    let adapter = adapter_with_stopper(&stub.address, true, &starts, &stops);

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the reloaded server answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(stops.count(), 1, "the wrong weights are dropped once");
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf")
    );
    assert_eq!(starts.count(), 0, "the reloaded port never fell silent");
    assert_eq!(stub.checks().len(), 1);
}

/// `openaiBaseUrl` may name any OpenAI-compatible server, and only llama-server
/// says which weights it loaded. A server that names none leaves the question
/// open, and an open question is not a mismatch.
#[test]
fn a_server_that_names_no_model_is_checked_rather_than_refused() {
    let stub = Stub::serving(Answer::Json(ANSWER));
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper(&stub.address, true, &starts, &stops);

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the stub answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(adapter.served_weights(), None);
    assert_eq!(stops.count(), 0);
}

/// The guard is one question per row, not one per sentence: a benchmark row of
/// hundreds of items must not pay for hundreds of probes.
#[test]
fn the_server_is_asked_what_it_serves_once_for_the_whole_row() {
    let stub = Stub::holding(Answer::Json(ANSWER), Some("qwen3.8-4b-Q4_K_M.gguf"));
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper(&stub.address, true, &starts, &stops);
    let options = options(&stub.base_url());

    for _ in 0..3 {
        adapter.check(TEXT, &options).expect("the stub answers");
    }

    let probes = stub.requests().len() - stub.checks().len();
    assert_eq!(probes, 1, "{:?}", stub.requests());
    assert_eq!(stub.checks().len(), 3);
}

/// A silent port needs no guard: the start path brings the server up for
/// `openaiModel` itself, so nothing is left to disagree with.
#[test]
fn a_silent_port_is_started_rather_than_refused() {
    let (address, _ports) = silent_address();
    let (starts, stops) = (Starts::default(), Stops::default());

    let failure = adapter_with_stopper(&address, true, &starts, &stops)
        .check(TEXT, &options(&format!("http://{address}")))
        .expect_err("nothing listens on the port");

    assert_eq!(starts.count(), 1, "the start path still runs");
    assert_eq!(stops.count(), 0, "there is nothing to reload");
    assert!(
        matches!(failure, EngineFailure::Unavailable(_)),
        "{failure:?}"
    );
}

/// An adapter whose starter brings a stub up on a port that is silent now.
///
/// That is the bench path: the run stops the unit before a Models row, so the
/// row's adapter probes a port with nothing on it and learns nothing from it.
fn adapter_that_brings_up(
    port: u16,
    served: &'static str,
    started_as: Started,
    held: &Arc<std::sync::Mutex<Option<Stub>>>,
    stops: &Stops,
) -> Openai {
    let slot = Arc::clone(held);
    let stopped = Arc::clone(&stops.0);
    Openai::with_server_control(
        Config {
            timeout: Duration::from_secs(2),
            start_unit: true,
            startup_budget: Duration::from_secs(5),
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            let mut slot = slot.lock().expect("the stub slot is readable");
            if slot.is_none() {
                *slot = Some(Stub::on_port(port, Answer::Json(ANSWER), Some(served)));
            }
            Ok(started_as)
        }),
        Box::new(move |_unit: &str| {
            stopped.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
        unit_at(&format!("127.0.0.1:{port}")),
    )
}

/// A silent port answers the guard nothing, so the answer is not the final one.
/// Once the start path has a server up, the guard asks it what it holds, and
/// the row can name the weights it was measured on.
#[test]
fn a_server_the_start_path_brought_up_is_asked_what_it_serves() {
    let (address, _ports) = silent_address();
    let held = Arc::new(std::sync::Mutex::new(None));
    let stops = Stops::default();
    let served = "qwen3.8-4b-Q4_K_M.gguf";
    let adapter = adapter_that_brings_up(port_of(&address), served, Started::Fresh, &held, &stops);

    let issues = adapter
        .check(TEXT, &options(&format!("http://{address}")))
        .expect("the started server answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some(served),
        "the row names what the server it started actually held"
    );
    assert_eq!(stops.count(), 0, "nothing disagreed, so nothing reloaded");
}

/// The other half of the same window. The server the start path brought up
/// holds another model, so the answer it already gave is thrown away and the
/// Check refuses rather than reporting a quality those weights never produced.
#[test]
fn a_started_server_that_holds_another_model_refuses_the_check() {
    let (address, _ports) = silent_address();
    let held = Arc::new(std::sync::Mutex::new(None));
    let stops = Stops::default();
    let adapter = adapter_that_brings_up(
        port_of(&address),
        "granite-4.2-3b-Q4_K_M.gguf",
        Started::Fresh,
        &held,
        &stops,
    );

    let failure = adapter
        .check(TEXT, &options(&format!("http://{address}")))
        .expect_err("the started server holds another model");

    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
    assert_eq!(
        adapter.served_weights(),
        None,
        "a refused Check names no weights"
    );
    let stub = held.lock().expect("the stub slot is readable");
    let checks = stub
        .as_ref()
        .expect("the starter brought a stub up")
        .checks();
    assert_eq!(
        checks.len(),
        1,
        "the one answer the wrong weights gave is thrown away: {checks:?}"
    );
}

/// llama.cpp answers HTTP 503 while it reads its weights, so a probe of a
/// loading server learns nothing either. The guard asks again once the load
/// finishes, which is what keeps the original HUF-236 failure out of that
/// window.
#[test]
fn a_server_that_was_still_loading_is_asked_again_once_it_answers() {
    let served = "qwen3.8-4b-Q4_K_M.gguf";
    let stub = Stub::holding(Answer::LoadingThenJson(1, ANSWER), Some(served));
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper_and_budget(
        &stub.address,
        true,
        Duration::from_secs(5),
        &starts,
        &stops,
    );

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the server finishes loading and answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(adapter.served_weights().as_deref(), Some(served));
}

/// The same window, with the wrong weights behind it.
#[test]
fn a_server_that_finished_loading_another_model_refuses_the_check() {
    let stub = Stub::holding(
        Answer::LoadingThenJson(1, ANSWER),
        Some("granite-4.2-3b-Q4_K_M.gguf"),
    );
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper_and_budget(
        &stub.address,
        true,
        Duration::from_secs(5),
        &starts,
        &stops,
    );

    let failure = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the loaded weights are another model");

    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
    assert_eq!(adapter.served_weights(), None);
}

/// The guard asks where the unit listens and then stops it, and those are two
/// calls. A transient unit is collected the moment it ends, so one that ended
/// between them is no longer loaded and the stop fails on it. The reload cannot
/// happen, so the Check ends on the one refusal that names both models rather
/// than on an engine error about a machine that works.
///
/// A hand-run llama-server, an Ollama, or an LM Studio never reaches this: the
/// unit holds no address for their port, so they end on the refusal that
/// `a_mismatch_on_another_server_stops_no_unit` covers.
#[test]
fn a_stop_that_fails_between_the_address_and_the_stop_still_names_both_models() {
    let stub = Stub::holding(Answer::Json(ANSWER), Some("granite-4.2-3b-Q4_K_M.gguf"));
    let starts = Starts::default();
    let started = Arc::clone(&starts.0);
    let adapter = Openai::with_server_control(
        Config {
            timeout: Duration::from_secs(2),
            start_unit: true,
            startup_budget: Duration::from_millis(0),
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            started.fetch_add(1, Ordering::SeqCst);
            Ok(Started::Fresh)
        }),
        Box::new(|unit: &str| {
            Err(format!(
                "systemctl could not stop {unit}: Unit {unit}.service not loaded."
            ))
        }),
        unit_at(&stub.address),
    );

    let failure = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the port holds another model and no stop can free it");

    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
            assert!(
                message.contains("not loaded"),
                "the refusal keeps what the stop said: {message}"
            );
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
    assert!(
        stub.checks().is_empty(),
        "nothing is checked against the wrong model: {:?}",
        stub.requests()
    );
    assert_eq!(starts.count(), 0, "a refused Check starts nothing");
}

/// `openaiBaseUrl` accepts any loopback server, so it may name an Ollama or an
/// LM Studio on a port the `grammachy-llama` unit does not serve. A
/// disagreement about weights there must never take down a server the run was
/// not asked about, and must not pay the reload wait either.
#[test]
fn a_mismatch_on_another_server_stops_no_unit() {
    let stub = Stub::holding(Answer::Json(ANSWER), Some("granite-4.2-3b-Q4_K_M.gguf"));
    let (starts, stops) = (Starts::default(), Stops::default());
    let started = Arc::clone(&starts.0);
    let stopped = Arc::clone(&stops.0);
    let adapter = Openai::with_server_control(
        Config {
            timeout: Duration::from_secs(2),
            start_unit: true,
            startup_budget: Duration::from_millis(0),
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            started.fetch_add(1, Ordering::SeqCst);
            Ok(Started::Fresh)
        }),
        Box::new(move |_unit: &str| {
            stopped.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
        // The unit is on the default port, and the base URL is not.
        unit_at("127.0.0.1:8080"),
    );
    let began = Instant::now();

    let failure = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("the other server holds another model");

    assert_eq!(
        stops.count(),
        0,
        "a server the unit does not serve is never stopped"
    );
    assert_eq!(starts.count(), 0);
    assert!(stub.checks().is_empty(), "{:?}", stub.requests());
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "the refusal pays no reload wait"
    );
    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
            assert!(
                message.contains("does not serve this address"),
                "the refusal says why no reload ran: {message}"
            );
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
}

/// The HUF-236 case itself. A sibling task left the unit reading other weights,
/// so it answers 503 for minutes. The start call finds that unit already there
/// and starts nothing, so the weights on the port belong to an earlier session
/// and one stop reloads them. The Check recovers rather than refuses, and the
/// answer the leftover weights gave is thrown away with them.
#[test]
fn a_unit_that_was_already_running_is_reloaded_rather_than_refused() {
    let (stub, stops) = Stub::leftover("granite-4.2-3b-Q4_K_M.gguf", "qwen3.8-4b-Q4_K_M.gguf");
    let starts = Starts::default();
    let started = Arc::clone(&starts.0);
    let adapter = Openai::with_server_control(
        Config {
            timeout: Duration::from_secs(2),
            start_unit: true,
            startup_budget: Duration::from_secs(5),
        },
        // systemd-run answers this way for a unit an earlier session left, so
        // this call started nothing and the weights are not the ones it asked
        // for.
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            started.fetch_add(1, Ordering::SeqCst);
            Ok(Started::AlreadyRunning)
        }),
        Box::new({
            let stopped = Arc::clone(&stops.0);
            move |_unit: &str| {
                stopped.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }),
        unit_at(&stub.address),
    );

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the reloaded unit answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(stops.count(), 1, "the leftover unit is reloaded once");
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf"),
        "the Check is measured on the weights it asked for"
    );
    assert_eq!(
        stub.checks().len(),
        3,
        "the answer the leftover weights gave is thrown away and the Check runs again: {:?}",
        stub.requests()
    );
}

/// `llama-server --alias` renames what `/v1/models` reports and leaves `/props`
/// naming the weights file. A Check that worked before the guard must still
/// work, so a matching answer wins whichever route gave it.
#[test]
fn an_aliased_server_passes_on_the_weights_file_props_names() {
    let stub = Stub::aliased("local-llm", "/models/qwen3.8-4b-Q4_K_M.gguf");
    let (starts, stops) = (Starts::default(), Stops::default());
    let adapter = adapter_with_stopper(&stub.address, true, &starts, &stops);

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the alias hides a file that does match");

    assert_eq!(issues.len(), 1);
    assert_eq!(stops.count(), 0, "a match is never a reload");
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf"),
        "the row names the weights file and never the path"
    );
}

/// A port whose stop really frees it, the way `systemctl --user stop` does.
///
/// The hot-swap stubs above keep answering throughout, so they never drive the
/// guard's silent arm. This one models the production recovery: the stop leaves
/// nothing on the port, and the start path is what puts weights back on it.
struct Restartable {
    port: u16,
    /// The server on the port now. A stop takes it and frees the port.
    running: Arc<std::sync::Mutex<Option<Stub>>>,
    /// Every request the servers that are gone already read.
    gone: Arc<std::sync::Mutex<Vec<String>>>,
    stops: Arc<AtomicUsize>,
    starts: Arc<AtomicUsize>,
}

impl Restartable {
    /// A port already serving `first`, which a stop frees and a start fills
    /// again with `then`.
    fn serving(
        first: &'static str,
        then: &'static str,
    ) -> (Restartable, Openai, RwLockWriteGuard<'static, ()>) {
        let (address, ports) = silent_address();
        let port = port_of(&address);
        let running = Arc::new(std::sync::Mutex::new(Some(Stub::on_port(
            port,
            Answer::Json(ANSWER),
            Some(first),
        ))));
        let gone = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        let starts = Arc::new(AtomicUsize::new(0));

        let starting = Arc::clone(&running);
        let started = Arc::clone(&starts);
        let stopping = Arc::clone(&running);
        let collected = Arc::clone(&gone);
        let stopped = Arc::clone(&stops);
        let adapter = Openai::with_server_control(
            Config {
                timeout: Duration::from_secs(2),
                start_unit: true,
                startup_budget: Duration::from_secs(5),
            },
            Box::new(move |_model: &str, _endpoint: &Endpoint| {
                started.fetch_add(1, Ordering::SeqCst);
                let mut slot = starting.lock().expect("the port slot is readable");
                if slot.is_none() {
                    *slot = Some(Stub::on_port(port, Answer::Json(ANSWER), Some(then)));
                }
                Ok(Started::Fresh)
            }),
            Box::new(move |_unit: &str| {
                stopped.fetch_add(1, Ordering::SeqCst);
                let mut slot = stopping.lock().expect("the port slot is readable");
                if let Some(stub) = slot.take() {
                    collected
                        .lock()
                        .expect("the log is readable")
                        .extend(stub.shut_down());
                }
                Ok(())
            }),
            unit_at(&address),
        );

        (
            Restartable {
                port,
                running,
                gone,
                stops,
                starts,
            },
            adapter,
            ports,
        )
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The Checks the servers that are gone answered before they went.
    fn checks_of_gone_servers(&self) -> Vec<String> {
        self.gone
            .lock()
            .expect("the log is readable")
            .iter()
            .filter(|request| request.starts_with("POST "))
            .cloned()
            .collect()
    }

    /// The Checks the server on the port now has answered.
    fn checks_now(&self) -> Vec<String> {
        let slot = self.running.lock().expect("the port slot is readable");
        slot.as_ref().map(|stub| stub.checks()).unwrap_or_default()
    }

    /// Take the server off the port, the way a restart by hand does.
    fn goes_away(&self) {
        let stub = self
            .running
            .lock()
            .expect("the port slot is readable")
            .take()
            .expect("a server is on the port");
        self.gone
            .lock()
            .expect("the log is readable")
            .extend(stub.shut_down());
    }
}

/// The production HUF-236 recovery. A real `systemctl --user stop` frees the
/// port, so the guard sees silence rather than one model id swapped for another
/// on a live server. The start path then puts the requested weights there, and
/// no Check ever reaches the weights the run did not ask for.
#[test]
fn a_reload_that_frees_the_port_is_followed_by_a_start_and_a_fresh_check() {
    let (port, adapter, _ports) =
        Restartable::serving("granite-4.2-3b-Q4_K_M.gguf", "qwen3.8-4b-Q4_K_M.gguf");

    let issues = adapter
        .check(TEXT, &options(&port.base_url()))
        .expect("the started server answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(
        port.stops.load(Ordering::SeqCst),
        1,
        "the wrong weights are stopped once"
    );
    assert_eq!(
        port.starts.load(Ordering::SeqCst),
        1,
        "the freed port is what the start path fills"
    );
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf"),
        "the row names the weights the start path loaded"
    );
    assert!(
        port.checks_of_gone_servers().is_empty(),
        "no Check reached the weights the run did not ask for: {:?}",
        port.checks_of_gone_servers()
    );
    assert_eq!(
        port.checks_now().len(),
        1,
        "the one Check ran against the weights the start path loaded"
    );
}

/// The guard's own cache must not re-open the failure it exists to close. A row
/// probes once and matches, then its server goes away and the start path brings
/// one back on other weights - a restart by hand, a crash, or another
/// `grammachy check` reloading the same unit. Those later items must not be
/// measured, and the row must not name the first model over their numbers.
#[test]
fn a_server_that_comes_back_on_other_weights_mid_row_measures_no_more_items() {
    let (port, adapter, _ports) =
        Restartable::serving("qwen3.8-4b-Q4_K_M.gguf", "granite-4.2-3b-Q4_K_M.gguf");
    let options = options(&port.base_url());

    let issues = adapter
        .check(TEXT, &options)
        .expect("the row starts on the weights it asked for");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf")
    );

    port.goes_away();

    let failure = adapter
        .check(TEXT, &options)
        .expect_err("the weights that came back are not the ones this row asked for");

    match failure {
        EngineFailure::BadArguments(message) => {
            assert!(message.contains("granite-4.2-3b-Q4_K_M.gguf"), "{message}");
            assert!(message.contains(REQUESTED), "{message}");
        }
        other => panic!("expected bad_arguments, got {other:?}"),
    }
    assert_ne!(
        adapter.served_weights().as_deref(),
        Some("qwen3.8-4b-Q4_K_M.gguf"),
        "the row never names the first model over the other weights' numbers"
    );
}
