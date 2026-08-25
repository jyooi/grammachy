//! The `motherTongue` the LanguageTool adapter puts on the wire, spec section 4.
//!
//! Each case runs one real Check against a stub server that records the
//! request body, so the assertion is about what LanguageTool receives and not
//! about an internal helper. No case starts a systemd unit.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;

use grammachy::args::{CheckOptions, NativeLanguage};
use grammachy::engine::Engine;
use grammachy::engines::languagetool::{Config, LanguageTool};

/// A stub that answers "no matches" and hands the request body back.
struct Recorder {
    address: String,
    bodies: Receiver<String>,
}

impl Recorder {
    fn start() -> Recorder {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let address = listener
            .local_addr()
            .expect("the port is known")
            .to_string();
        let (sender, bodies) = channel();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let body = read_request(&mut stream);
                if sender.send(body).is_err() {
                    break;
                }
                let answer = r#"{"matches":[]}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                );
            }
        });

        Recorder { address, bodies }
    }

    /// Run one Check and answer the form body the adapter sent.
    fn record(&self, text: &str, native: NativeLanguage) -> String {
        let adapter = LanguageTool::new(Config {
            address: self.address.clone(),
            timeout: Duration::from_secs(5),
            start_unit: false,
            startup_budget: Duration::from_millis(0),
        });
        let options = CheckOptions {
            native,
            ..CheckOptions::default()
        };

        adapter.check(text, &options).expect("the stub answers");
        self.bodies
            .recv_timeout(Duration::from_secs(5))
            .expect("the stub recorded the request")
    }
}

/// Read the headers, then exactly the announced body.
fn read_request(stream: &mut TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .expect("the body arrives whole");
    String::from_utf8(body).expect("the body is UTF-8")
}

/// The `motherTongue` field of one recorded form body, or `None`.
fn mother_tongue_of(body: &str) -> Option<String> {
    body.split('&')
        .find_map(|field| field.strip_prefix("motherTongue="))
        .map(percent_decode)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).expect("hex is ASCII");
            out.push(u8::from_str_radix(hex, 16).expect("a percent escape is hex"));
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).expect("the field is UTF-8")
}

#[test]
fn every_native_language_sends_the_mother_tongue_of_the_spec_table() {
    let cases = [
        (NativeLanguage::None, None),
        (NativeLanguage::Ms, None),
        (NativeLanguage::Zh, Some("zh-CN")),
        (NativeLanguage::Ja, Some("ja-JP")),
        (NativeLanguage::Es, Some("es")),
        (NativeLanguage::Fr, Some("fr")),
        (NativeLanguage::De, Some("de")),
        (NativeLanguage::Pt, Some("pt")),
    ];

    let recorder = Recorder::start();
    for (native, expected) in cases {
        let body = recorder.record("He go home.", native);

        assert_eq!(
            mother_tongue_of(&body).as_deref(),
            expected,
            "{} sends {expected:?}: {body}",
            native.as_str()
        );
    }
}

#[test]
fn the_recorded_request_carries_the_target_and_the_text_verbatim() {
    let recorder = Recorder::start();
    let body = recorder.record("He go home.", NativeLanguage::De);

    assert!(
        body.starts_with("language=en-US&text=He%20go%20home."),
        "{body}"
    );
}
