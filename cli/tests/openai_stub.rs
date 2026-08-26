//! The `openai` adapter against a stub chat endpoint.
//!
//! No case starts a systemd unit. Every adapter here is built with a starter
//! the test owns, so the start behaviour is covered without llama.cpp and
//! without systemd, and the suite is the same on a developer machine and in CI.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use grammachy::args::CheckOptions;
use grammachy::engine::{Engine, EngineFailure};
use grammachy::engines::openai::endpoint::Endpoint;
use grammachy::engines::openai::{Config, Openai};

const TEXT: &str = "She bought three book from the store.";

/// A chat completion with the two suggestions of the recorded fixture.
const ANSWER: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant",
    "content":"[{\"original\": \"book\", \"fix\": \"books\", \"reason\": \"plural\", \"category\": \"grammar\"}]"}}]}"#;

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
}

impl Stub {
    fn serving(answer: Answer) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);

        thread::spawn(move || {
            let mut served = 0usize;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                served += 1;
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
                    Answer::LoadingThenJson(loading, _) if served <= loading => {
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

        Stub { address, seen }
    }

    /// A 307 whose Location is another server, so a follow would be visible.
    fn redirecting(location: &str) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
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

        Stub { address, seen }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().expect("the log is not poisoned").clone()
    }
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

/// An adapter that records its start calls instead of running systemd.
fn adapter(timeout: Duration, start_unit: bool, starts: &Starts) -> Openai {
    let counter = Arc::clone(&starts.0);
    Openai::with_starter(
        Config {
            timeout,
            start_unit,
            // Nothing comes up behind a recording starter, so one probe is all
            // the retry loop needs to conclude.
            startup_budget: Duration::from_millis(0),
        },
        Box::new(move |_model: &str, _endpoint: &Endpoint| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    )
}

fn options(base_url: &str) -> CheckOptions {
    CheckOptions {
        openai_base_url: base_url.to_string(),
        ..CheckOptions::default()
    }
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
    let address = silent_address();
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
    let address = silent_address();
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
    let adapter = Openai::with_starter(
        Config {
            timeout: Duration::from_secs(2),
            start_unit: true,
            startup_budget: Duration::from_secs(5),
        },
        Box::new(|_model: &str, _endpoint: &Endpoint| Ok(())),
    );

    let issues = adapter
        .check(TEXT, &options(&stub.base_url()))
        .expect("the server finishes loading and answers");

    assert_eq!(issues.len(), 1);
    // One request found it loading, the retry loop found it loading again, and
    // the third one got the answer.
    assert_eq!(stub.requests().len(), 3);
    assert_eq!(starts.count(), 0, "no start was recorded on this adapter");
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

    let requests = stub.requests();
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
fn a_redirect_is_not_followed() {
    let target = Stub::serving(Answer::Json(ANSWER));
    let location = format!("{}/v1/chat/completions", target.base_url());
    let stub = Stub::redirecting(&location);
    let starts = Starts::default();

    let failure = adapter(Duration::from_secs(2), true, &starts)
        .check(TEXT, &options(&stub.base_url()))
        .expect_err("a redirect is not a chat completion");

    assert_eq!(stub.requests().len(), 1, "the local port is contacted once");
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

    let requests = stub.requests();
    assert!(
        !requests[0].to_ascii_lowercase().contains("authorization:"),
        "{}",
        requests[0]
    );
}
