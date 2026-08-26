//! The `openrouter` adapter against a stub endpoint.
//!
//! No case reaches openrouter.ai. The endpoint seam points the adapter at a
//! loopback stub and the key seam at a file in the scratch directory, so the
//! suite is the same on a developer machine and in CI.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use grammachy::args::CheckOptions;
use grammachy::engine::{Engine, EngineFailure};
use grammachy::engines::openrouter::{Config, Openrouter};

const TEXT: &str = "She bought three book from the store.";

const ANSWER: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant",
    "content":"[{\"original\": \"book\", \"fix\": \"books\", \"reason\": \"plural\", \"category\": \"grammar\"}]"}}],
    "usage":{"prompt_tokens":120,"completion_tokens":30,"cost":0.000021}}"#;

const ANSWER_WITHOUT_COST: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"[]"}}],"usage":{"prompt_tokens":1}}"#;

#[derive(Clone, Copy)]
enum Reply {
    Json(&'static str),
    Status(&'static str),
    Silence,
}

struct Stub {
    address: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    fn serving(reply: Reply) -> Stub {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&seen);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let request = read_request(&mut stream);
                recorder
                    .lock()
                    .expect("the log is not poisoned")
                    .push(request);
                match reply {
                    Reply::Json(body) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    Reply::Status(line) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {line}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                    Reply::Silence => thread::sleep(Duration::from_secs(10)),
                }
            }
        });

        Stub { address, seen }
    }

    fn url(&self) -> String {
        format!("http://{}/api/v1/chat/completions", self.address)
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().expect("the log is not poisoned").clone()
    }
}

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

fn key_file(name: &str, contents: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("openrouter");
    std::fs::create_dir_all(&dir).expect("the scratch directory is created");
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("the key file is written");
    path
}

fn adapter(url: &str, key: Option<PathBuf>, timeout: Duration) -> Openrouter {
    Openrouter::new(Config {
        timeout,
        key_file: key,
        url: url.to_string(),
    })
}

fn options() -> CheckOptions {
    CheckOptions {
        openrouter_model: "deepseek/deepseek-v4-flash-0731".to_string(),
        ..CheckOptions::default()
    }
}

#[test]
fn a_good_answer_becomes_issues_with_its_cost() {
    let stub = Stub::serving(Reply::Json(ANSWER));
    let key = key_file("good", "sk-or-test\n");

    let answer = adapter(&stub.url(), Some(key), Duration::from_secs(2))
        .answer(TEXT, &options())
        .expect("the stub answers");

    assert_eq!(answer.issues.len(), 1);
    assert_eq!(answer.issues[0].fix, "books");
    assert_eq!(answer.cost, Some(0.000021));

    let request = &stub.requests()[0];
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-or-test"),
        "{request}"
    );
    assert!(
        request.contains("X-Title: Grammachy") || request.contains("x-title: Grammachy"),
        "{request}"
    );
    assert!(
        request.contains("\"usage\":{\"include\":true}"),
        "{request}"
    );
    assert!(
        request.contains("\"reasoning\":{\"enabled\":false}"),
        "{request}"
    );
    assert!(
        request.contains("deepseek/deepseek-v4-flash-0731"),
        "{request}"
    );
}

#[test]
fn an_answer_without_cost_still_yields_issues() {
    let stub = Stub::serving(Reply::Json(ANSWER_WITHOUT_COST));
    let key = key_file("nocost", "sk-or-test");

    let answer = adapter(&stub.url(), Some(key), Duration::from_secs(2))
        .answer(TEXT, &options())
        .expect("the stub answers");

    assert!(answer.issues.is_empty());
    assert_eq!(answer.cost, None);
}

#[test]
fn a_missing_key_file_sends_nothing() {
    let stub = Stub::serving(Reply::Json(ANSWER));
    let missing = Path::new(env!("CARGO_TARGET_TMPDIR")).join("openrouter/absent-key");

    let failure = adapter(&stub.url(), Some(missing), Duration::from_secs(2))
        .answer(TEXT, &options())
        .expect_err("no key, no request");

    assert!(
        matches!(&failure, EngineFailure::Unavailable(message) if message.contains("no_key")),
        "{failure:?}"
    );
    assert!(stub.requests().is_empty(), "nothing was sent");
}

#[test]
fn an_empty_model_id_is_bad_arguments_before_anything_is_sent() {
    let stub = Stub::serving(Reply::Json(ANSWER));
    let key = key_file("empty-model", "sk-or-test");
    let options = CheckOptions {
        openrouter_model: "  ".to_string(),
        ..CheckOptions::default()
    };

    let failure = adapter(&stub.url(), Some(key), Duration::from_secs(2))
        .answer(TEXT, &options)
        .expect_err("no model, no request");

    assert!(
        matches!(failure, EngineFailure::BadArguments(_)),
        "{failure:?}"
    );
    assert!(stub.requests().is_empty());
}

#[test]
fn http_statuses_map_onto_the_agreed_reasons() {
    let cases = [
        ("401 Unauthorized", "rejected_key"),
        ("403 Forbidden", "rejected_key"),
        ("402 Payment Required", "no_credit"),
        ("429 Too Many Requests", "rate_limited"),
    ];
    for (line, reason) in cases {
        let stub = Stub::serving(Reply::Status(line));
        let key = key_file("status", "sk-or-test");
        let failure = adapter(&stub.url(), Some(key), Duration::from_secs(2))
            .answer(TEXT, &options())
            .expect_err(line);
        assert!(
            matches!(&failure, EngineFailure::Unavailable(message) if message.contains(reason)),
            "{line}: {failure:?}"
        );
    }

    let stub = Stub::serving(Reply::Status("404 Not Found"));
    let key = key_file("status", "sk-or-test");
    let failure = adapter(&stub.url(), Some(key), Duration::from_secs(2))
        .answer(TEXT, &options())
        .expect_err("unknown model");
    assert!(
        matches!(failure, EngineFailure::BadArguments(_)),
        "{failure:?}"
    );

    let stub = Stub::serving(Reply::Status("500 Internal Server Error"));
    let failure = adapter(
        &stub.url(),
        Some(key_file("status", "sk-or-test")),
        Duration::from_secs(2),
    )
    .answer(TEXT, &options())
    .expect_err("server error");
    assert!(matches!(failure, EngineFailure::Failed(_)), "{failure:?}");
}

#[test]
fn a_silent_endpoint_is_a_timeout() {
    let stub = Stub::serving(Reply::Silence);
    let key = key_file("silent", "sk-or-test");

    let failure = adapter(&stub.url(), Some(key), Duration::from_millis(300))
        .answer(TEXT, &options())
        .expect_err("the stub never answers");

    assert!(matches!(failure, EngineFailure::Timeout(_)), "{failure:?}");
}

#[test]
fn a_dead_port_is_unreachable() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let url = format!(
        "http://{}/api/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let key = key_file("dead", "sk-or-test");

    let failure = adapter(&url, Some(key), Duration::from_secs(2))
        .answer(TEXT, &options())
        .expect_err("nothing listens");

    assert!(
        matches!(&failure, EngineFailure::Unavailable(message) if message.contains("unreachable")),
        "{failure:?}"
    );
}
