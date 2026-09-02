//! The failure paths of the LanguageTool adapter, against a stub server.
//!
//! No case starts a systemd unit: every configuration sets `start_unit` to
//! false, so the tests are the same on a developer machine and in CI.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use grammachy::args::CheckOptions;
use grammachy::engine::{Engine, EngineFailure};
use grammachy::engines::languagetool::{Config, Endpoint, LanguageTool};

/// How the stub answers one request.
enum Answer {
    /// A `/v2/check` body with this JSON.
    Json(&'static str),
    /// This status line with an empty body.
    Status(&'static str),
    /// Read the request and never write, so the client runs out of time.
    Silence,
}

/// A stub server on a port the operating system picks, torn down with the test.
struct Stub {
    address: String,
}

impl Stub {
    fn serving(answer: Answer) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                read_request(&mut stream);
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
                }
            }
        });

        Stub { address }
    }
}

/// Drain one whole request, headers and body.
///
/// The body has to be read too. A stub that answers and closes while the
/// client is still writing gives the client a reset, which the adapter reads
/// as `engine_unavailable` rather than as the answer this stub sent.
fn read_request(stream: &mut TcpStream) {
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

    if length > 0 {
        let mut body = vec![0u8; length];
        let _ = stream.read_exact(&mut body);
    }
}

fn adapter(address: &str, timeout: Duration) -> LanguageTool {
    LanguageTool::new(Config {
        endpoint: Endpoint::fixed(address).expect("the stub is on loopback"),
        timeout,
        start_unit: false,
        startup_budget: Duration::from_millis(0),
    })
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
fn a_server_that_never_answers_is_a_timeout() {
    let stub = Stub::serving(Answer::Silence);
    let failure = adapter(&stub.address, Duration::from_millis(300))
        .check("He go home.", &CheckOptions::default())
        .expect_err("the stub never answers");

    match failure {
        EngineFailure::Timeout(message) => assert!(
            message.contains(&stub.address),
            "the message names the address: {message}"
        ),
        other => panic!("expected a timeout, got {other:?}"),
    }
}

#[test]
fn nothing_listening_is_engine_unavailable_with_the_address() {
    let address = silent_address();
    let failure = adapter(&address, Duration::from_secs(2))
        .check("He go home.", &CheckOptions::default())
        .expect_err("nothing listens on the port");

    match failure {
        EngineFailure::Unavailable(message) => assert!(
            message.contains(&address),
            "the message names the address: {message}"
        ),
        other => panic!("expected engine_unavailable, got {other:?}"),
    }
}

#[test]
fn a_server_error_is_an_engine_error() {
    let stub = Stub::serving(Answer::Status("500 Internal Server Error"));
    let failure = adapter(&stub.address, Duration::from_secs(2))
        .check("He go home.", &CheckOptions::default())
        .expect_err("the stub fails the request");

    assert!(
        matches!(failure, EngineFailure::Failed(ref message) if message.contains("500")),
        "expected engine_error, got {failure:?}"
    );
}

#[test]
fn an_answer_that_is_not_the_check_json_is_an_engine_error() {
    let stub = Stub::serving(Answer::Json("this is not JSON"));
    let failure = adapter(&stub.address, Duration::from_secs(2))
        .check("He go home.", &CheckOptions::default())
        .expect_err("the body does not parse");

    assert!(
        matches!(failure, EngineFailure::Failed(_)),
        "expected engine_error, got {failure:?}"
    );
}

#[test]
fn a_good_answer_becomes_issues() {
    let stub = Stub::serving(Answer::Json(
        r#"{"matches":[{"message":"Agreement error.","offset":3,"length":2,
            "replacements":[{"value":"goes"}],
            "rule":{"id":"AGREEMENT","issueType":"grammar","category":{"id":"GRAMMAR"}}}]}"#,
    ));

    let issues = adapter(&stub.address, Duration::from_secs(2))
        .check("He go home.", &CheckOptions::default())
        .expect("the stub answers");

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].start, 3);
    assert_eq!(issues[0].end, 5);
    assert_eq!(issues[0].original, "go");
    assert_eq!(issues[0].fix, "goes");
}

#[test]
fn the_slug_is_the_engine_the_envelope_reports() {
    assert_eq!(
        adapter("127.0.0.1:8081", Duration::from_secs(1)).slug(),
        "languagetool"
    );
}
