//! The cancel signal of `grammachy model download`, spec section 5.3.
//!
//! The shell has no other way to stop a transfer than to signal the process it
//! started, so what this covers is the one thing `model_download.rs` cannot:
//! that a real SIGTERM sets the flag rather than ending the run, which is what
//! keeps the `.part` file.
//!
//! The test owns this whole binary, because it takes the signal disposition of
//! the process over. It never starts a transfer and never touches the network.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use grammachy::model::{cancel, Downloader, Failure, Models, Transfer};

/// The cancel flag and the signal disposition are one per process, so the two
/// tests here take this first.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serially() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn a_sigterm_is_a_cancel_rather_than_the_end_of_the_run() {
    let _guard = serially();
    cancel::reset();
    assert!(!cancel::requested(), "nothing has been cancelled yet");

    cancel::listen();
    // Safety: `raise` sends the signal to this process, whose handler is the
    // one just installed. Without `listen` this call would end the run.
    unsafe {
        libc::raise(libc::SIGTERM);
    }

    assert!(
        cancel::requested(),
        "the run is still here and it knows it was asked to stop"
    );
}

/// The flag is what a transfer reads, so the run that follows a signal answers
/// `cancelled` and leaves the `.part` file where it is.
#[test]
fn the_transfer_after_the_signal_answers_cancelled_and_keeps_the_part_file() {
    let _guard = serially();
    cancel::reset();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("model-cancel-signal");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    let partial = directory.join("Qwen3-4B-Instruct-2507-Q4_K_M.gguf.part");
    std::fs::write(&partial, b"half a model").expect("the part file is written");

    let download: Downloader = Box::new(|_url, _path| {
        if cancel::requested() {
            return Ok(Transfer::Cancelled);
        }
        Ok(Transfer::Finished)
    });
    let models = Models {
        directory: directory.clone(),
        download,
        stop: Box::new(|_unit| Ok(())),
    };

    cancel::listen();
    // Safety: as above. The handler only stores into an atomic.
    unsafe {
        libc::raise(libc::SIGTERM);
    }
    let failure = models
        .fetch("qwen3-4b-instruct")
        .expect_err("the signal stopped the transfer");

    assert!(matches!(failure, Failure::Cancelled(_)), "{failure:?}");
    assert_eq!(std::fs::read(&partial).unwrap(), b"half a model");
}
