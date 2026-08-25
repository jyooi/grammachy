//! The download step of `grammachy setup`, against a stub server.
//!
//! Nothing here reaches the real weights host: the base URL points at a stub on
//! the loopback interface that serves a few bytes standing in for the model.
//! The test owns this whole binary, because it sets the base URL for the
//! process.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use grammachy::setup::model;

/// What the stub serves in place of a 4.7 GB file.
const FAKE_WEIGHTS: &[u8] = b"GGUF fake weights for the test suite";

/// A stub weights host on a port the operating system picks.
fn stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
    let address = listener
        .local_addr()
        .expect("the port is known")
        .to_string();

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            read_request(&mut stream);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                FAKE_WEIGHTS.len()
            );
            let _ = stream.write_all(FAKE_WEIGHTS);
        }
    });

    format!("http://{address}")
}

fn read_request(stream: &mut TcpStream) {
    let mut seen = Vec::new();
    let mut byte = [0u8; 1];
    while stream.read(&mut byte).unwrap_or(0) == 1 {
        seen.push(byte[0]);
        if seen.ends_with(b"\r\n\r\n") {
            break;
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("download-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

/// curl is what `bin/bootstrap.sh` uses too, and it is not worth failing CI
/// over when a container leaves it out.
fn curl_is_installed() -> bool {
    Command::new("curl")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn the_weights_arrive_and_a_second_run_skips_them() {
    if !curl_is_installed() {
        eprintln!("curl is not installed, so the download step is not exercised.");
        return;
    }
    let base = stub();
    // Safety: this test binary holds one test, so nothing else reads the
    // environment while it is changed.
    std::env::set_var(model::BASE_URL_ENV, &base);
    std::env::set_var(model::SHA256_ENV, model::sha256_hex(FAKE_WEIGHTS));

    let directory = scratch("weights");
    let download = model::downloader();

    let first = model::ensure("gemma-4-e4b-it", &directory, &download).expect("the stub answers");
    let second = model::ensure("gemma-4-e4b-it", &directory, &download).expect("the file is here");

    let expected = directory.join("gemma-4-E4B-it-Q4_K_M.gguf");
    assert_eq!(first, model::Outcome::Downloaded(expected.clone()));
    assert_eq!(second, model::Outcome::Present(expected.clone()));
    assert_eq!(std::fs::read(&expected).unwrap(), FAKE_WEIGHTS);
    // The transfer runs into a .part file, which the rename consumes.
    assert!(!directory.join("gemma-4-E4B-it-Q4_K_M.gguf.part").exists());
}
