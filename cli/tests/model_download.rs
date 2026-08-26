//! `grammachy model download`, spec section 5.3.
//!
//! Nothing here reaches the network: the downloader is a value, so this test
//! owns what a transfer does, how long it takes, and when it notices a cancel.
//! The test owns this whole binary, because it pins a digest for the process.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use grammachy::model::{cancel, Downloader, Failure, Models, State, Transfer};

/// What the fake transfer writes in place of a 2.5 GB file.
const FAKE_WEIGHTS: &[u8] = b"GGUF fake weights for the test suite";

/// The pinned file name of the row every test here uses.
const QWEN: &str = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf";
const NAME: &str = "qwen3-4b-instruct";

/// Pin the digest of the fake bytes for the whole process.
///
/// Safety: every test in this binary wants the same pin, and no other binary
/// reads this variable, so nothing races on it.
fn pin_the_fake_digest() {
    std::env::set_var(
        grammachy::model::SHA256_ENV,
        grammachy::model::sha256_hex(FAKE_WEIGHTS),
    );
}

/// The cancel flag is one flag for the whole process, so every test that sets
/// it takes this first. Nothing else here needs ordering.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serially() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("download-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

fn models(directory: PathBuf, download: Downloader) -> Models {
    Models {
        directory,
        download,
        stop: Box::new(|_unit| Ok(())),
    }
}

/// A transfer that writes the whole file at once.
fn whole() -> Downloader {
    Box::new(|_url, path| {
        std::fs::write(path, FAKE_WEIGHTS)
            .map(|()| Transfer::Finished)
            .map_err(|error| error.to_string())
    })
}

/// A transfer that arrives in pieces and stops as soon as a cancel lands, the
/// way `curl` polls the flag while its child runs.
///
/// It appends to whatever the `.part` file already holds, which is what makes a
/// second run a resume rather than a restart.
fn slow(pieces: Arc<AtomicUsize>) -> Downloader {
    Box::new(move |_url, path| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        let already = file.metadata().map_err(|error| error.to_string())?.len() as usize;

        for byte in &FAKE_WEIGHTS[already..] {
            if cancel::requested() {
                return Ok(Transfer::Cancelled);
            }
            file.write_all(&[*byte])
                .map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            pieces.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Transfer::Finished)
    })
}

#[test]
fn a_finished_transfer_renames_the_part_file_and_reports_ready() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("finished");
    let models = models(directory.clone(), whole());

    let row = models.fetch(NAME).expect("the fake transfer finishes");

    assert_eq!(row.state, State::Ready);
    assert_eq!(row.partial_bytes, 0);
    assert_eq!(std::fs::read(directory.join(QWEN)).unwrap(), FAKE_WEIGHTS);
    assert!(!directory.join(format!("{QWEN}.part")).exists());
}

/// The whole point of Cancel: the bytes already fetched stay on disk, so the
/// next Download resumes rather than starting the gigabytes again.
#[test]
fn a_cancel_keeps_the_part_file_and_a_second_download_resumes_it() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("cancel-resume");
    let pieces = Arc::new(AtomicUsize::new(0));
    let models = models(directory.clone(), slow(Arc::clone(&pieces)));

    // A cancel that lands before the transfer starts stops it at the first
    // byte, which is the earliest a Cancel can be answered at all.
    cancel::request();
    let failure = models
        .fetch(NAME)
        .expect_err("the cancel stops the transfer");

    let Failure::Cancelled(message) = failure else {
        panic!("a cancel is the cancelled code: {failure:?}")
    };
    assert!(message.contains(QWEN), "{message}");
    assert_eq!(pieces.load(Ordering::SeqCst), 0, "nothing was written yet");
    assert!(!directory.join(QWEN).exists(), "no whole file was promoted");

    // Half the file arrives, then a second cancel.
    cancel::reset();
    std::fs::write(
        directory.join(format!("{QWEN}.part")),
        &FAKE_WEIGHTS[..FAKE_WEIGHTS.len() / 2],
    )
    .expect("the half file is written");

    // The resume writes only what is missing, and the file is whole after it.
    let row = models.fetch(NAME).expect("the resumed transfer finishes");

    assert_eq!(row.state, State::Ready);
    assert_eq!(
        pieces.load(Ordering::SeqCst),
        FAKE_WEIGHTS.len() - FAKE_WEIGHTS.len() / 2,
        "only the missing bytes were fetched"
    );
    assert_eq!(std::fs::read(directory.join(QWEN)).unwrap(), FAKE_WEIGHTS);
}

/// A cancel halfway leaves exactly the bytes that arrived, and no whole file.
#[test]
fn a_cancel_halfway_leaves_what_arrived_and_promotes_nothing() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("cancel-halfway");
    let stop_after = FAKE_WEIGHTS.len() / 3;
    let written = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&written);
    let download: Downloader = Box::new(move |_url, path| {
        use std::io::Write;
        let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
        for byte in FAKE_WEIGHTS {
            if counted.load(Ordering::SeqCst) >= stop_after {
                cancel::request();
            }
            if cancel::requested() {
                return Ok(Transfer::Cancelled);
            }
            file.write_all(&[*byte])
                .map_err(|error| error.to_string())?;
            file.flush().map_err(|error| error.to_string())?;
            counted.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Transfer::Finished)
    });
    let models = models(directory.clone(), download);

    let failure = models
        .fetch(NAME)
        .expect_err("the cancel stops the transfer");

    assert!(matches!(failure, Failure::Cancelled(_)), "{failure:?}");
    assert!(!directory.join(QWEN).exists());
    let partial = std::fs::read(directory.join(format!("{QWEN}.part"))).expect("the part is kept");
    assert_eq!(partial.len(), stop_after);
    assert_eq!(partial, FAKE_WEIGHTS[..stop_after]);

    // The row the shell polls says how far the download got.
    cancel::reset();
    let row = models
        .list()
        .into_iter()
        .find(|row| row.name == NAME)
        .expect("the row is listed");
    assert_eq!(row.state, State::Partial);
    assert_eq!(row.partial_bytes, stop_after as u64);
}

/// A file that arrived whole but is not the pinned one never becomes the
/// weights: the rename is what the digest guards.
///
/// The partial goes with it, unlike the cancel above. A whole wrong file cannot
/// be resumed into a right one, so keeping it would make every retry fail the
/// same way for ever.
#[test]
fn a_digest_that_does_not_match_the_pin_is_download_failed() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("digest");
    let download: Downloader = Box::new(|_url, path| {
        std::fs::write(path, b"not the weights")
            .map(|()| Transfer::Finished)
            .map_err(|error| error.to_string())
    });
    let models = models(directory.clone(), download);

    let failure = models
        .fetch(NAME)
        .expect_err("the digest differs from the pin");

    let Failure::DownloadFailed(message) = failure else {
        panic!("a bad digest is the download_failed code: {failure:?}")
    };
    assert!(message.contains("pinned digest"), "{message}");
    assert!(
        message.contains("starts over"),
        "the message says the next download starts clean: {message}"
    );
    assert!(!directory.join(QWEN).exists());
    assert!(
        !directory.join(format!("{QWEN}.part")).exists(),
        "the wrong bytes are gone, so a retry is not the same failure again"
    );

    // The retry a user makes next really does land the weights.
    let retried = Models {
        directory: directory.clone(),
        download: whole(),
        stop: Box::new(|_unit| Ok(())),
    };
    let row = retried
        .fetch(NAME)
        .expect("the retry starts clean and lands");
    assert_eq!(row.state, State::Ready);
    assert!(directory.join(QWEN).is_file());
}

/// A transfer that could not run at all is `download_failed`, not a cancel.
#[test]
fn a_transfer_that_could_not_run_is_download_failed() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("no-curl");
    let download: Downloader = Box::new(|_url, _path| Err("curl could not run".to_string()));
    let models = models(directory, download);

    let failure = models.fetch(NAME).expect_err("the transfer failed");

    assert!(matches!(failure, Failure::DownloadFailed(_)), "{failure:?}");
}

/// A model already on disk is not fetched again, so a second Download is free.
#[test]
fn a_model_already_here_is_not_fetched_again() {
    let _guard = serially();
    pin_the_fake_digest();
    cancel::reset();
    let directory = scratch("already-here");
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let download: Downloader = Box::new(move |_url, path| {
        counted.fetch_add(1, Ordering::SeqCst);
        std::fs::write(path, FAKE_WEIGHTS)
            .map(|()| Transfer::Finished)
            .map_err(|error| error.to_string())
    });
    let models = models(directory, download);

    assert_eq!(
        models.fetch(NAME).expect("the first run fetches").state,
        State::Ready
    );
    assert_eq!(
        models.fetch(NAME).expect("the second run does not").state,
        State::Ready
    );

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
