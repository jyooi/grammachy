//! `grammachy engine install`, spec section 5.4.
//!
//! Nothing here reaches the network and nothing here runs `bsdtar`: the
//! transfer and the unpack are both values, so this test owns what arrives,
//! what it unpacks into, how long it takes, and when it notices a cancel. The
//! test owns this whole binary, because it pins a digest for the process.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use grammachy::engines::install::{
    self, cancel, Downloader, Engines, Extractor, Failure, State, Transfer,
};

/// What the fake transfer writes in place of a 250 MB archive.
const FAKE_ARCHIVE: &[u8] = b"PK fake LanguageTool release for the test suite";

/// The names the `languagetool` row pins.
const SLUG: &str = "languagetool";
const ARCHIVE: &str = "LanguageTool-6.6.zip";
const UNPACKS_INTO: &str = "LanguageTool-6.6";
const ENTRY: &str = "languagetool-server.jar";

/// Pin the digest of the fake bytes for the whole process.
///
/// Safety: every test in this binary wants the same pin, and no other binary
/// reads this variable, so nothing races on it.
fn pin_the_fake_digest() {
    std::env::set_var(
        install::SHA256_ENV,
        grammachy::engines::install::sha256_hex(FAKE_ARCHIVE),
    );
}

/// Pin the size of the fake bytes too.
///
/// `install` runs the free-space check against the pinned archive and tree
/// sizes before it calls the downloader, so without this seam every test here
/// would need the 650 MB the real row asks for and would fail on a small disk
/// for a reason that has nothing to do with what it asserts.
///
/// Safety: as above, one variable read by this binary alone.
fn pin_the_fake_size(bytes: u64) {
    std::env::set_var(install::SIZE_ENV, bytes.to_string());
}

fn pin_the_fake_release() {
    pin_the_fake_digest();
    pin_the_fake_size(FAKE_ARCHIVE.len() as u64);
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
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("install-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

fn engines(directory: PathBuf, download: Downloader, extract: Extractor) -> Engines {
    Engines {
        directory,
        download,
        extract,
        stop: Box::new(|_unit| Ok(())),
    }
}

/// A transfer that writes the whole archive at once.
fn whole() -> Downloader {
    Box::new(|_url, path, _max_bytes| {
        std::fs::write(path, FAKE_ARCHIVE)
            .map(|()| Transfer::Finished)
            .map_err(|error| error.to_string())
    })
}

/// An unpack that leaves the tree the real archive would.
fn unpacks_the_release() -> Extractor {
    Box::new(|_archive, into, _admission| {
        let tree = into.join(UNPACKS_INTO);
        std::fs::create_dir_all(tree.join("libs")).map_err(|error| error.to_string())?;
        std::fs::write(tree.join(ENTRY), b"jar").map_err(|error| error.to_string())?;
        std::fs::write(tree.join("libs/lucene.jar"), b"jar").map_err(|error| error.to_string())
    })
}

/// The acceptance criterion: one install, no sudo, and a tree the adapter can
/// run from.
#[test]
fn an_install_verifies_the_archive_and_leaves_the_unpacked_tree() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("whole");
    let engines = engines(directory.clone(), whole(), unpacks_the_release());

    let row = engines.install(SLUG).expect("the digest matches the pin");

    assert_eq!(row.state, State::Ready);
    let tree = directory.join(SLUG);
    assert!(tree.join(ENTRY).is_file(), "the server jar is in place");
    assert!(tree.join("libs/lucene.jar").is_file(), "so are its libs");
    assert_eq!(row.path, tree.display().to_string());
    assert!(!row.from_package, "this verb put it there, not pacman");
    // The archive is hundreds of megabytes and a re-install re-checks the
    // digest anyway, so keeping it would double what the component costs.
    assert!(!directory.join(ARCHIVE).exists());
    assert!(!directory.join(format!("{ARCHIVE}.part")).exists());
    assert!(!directory.join(format!("{SLUG}.unpack")).exists());
}

/// The pin is what makes the install safe: bytes that are not the release this
/// row names never reach the disk as an engine.
#[test]
fn an_archive_that_does_not_match_the_pin_is_refused() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("mismatch");
    let unpacked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&unpacked);
    let engines = engines(
        directory.clone(),
        // The same length as the pin, so the digest is what refuses it.
        Box::new(|_url, path, _max_bytes| {
            std::fs::write(path, FAKE_ARCHIVE.to_ascii_uppercase())
                .map(|()| Transfer::Finished)
                .map_err(|error| error.to_string())
        }),
        Box::new(move |_archive, _into, _admission| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
    );

    let failure = engines
        .install(SLUG)
        .expect_err("the digest differs from the pin");

    assert!(
        matches!(&failure, Failure::DownloadFailed(message)
            if message.contains(&grammachy::engines::install::sha256_hex(FAKE_ARCHIVE))),
        "{failure:?}"
    );
    assert_eq!(
        unpacked.load(Ordering::SeqCst),
        0,
        "nothing is unpacked until the digest has settled it"
    );
    assert!(!directory.join(SLUG).exists());
    assert!(
        !directory.join(format!("{ARCHIVE}.part")).exists(),
        "the wrong bytes go, so the next install is not the same failure again"
    );
}

/// The size is checked before the digest, so a short or long file is refused
/// without a hash, and the transfer is told the pinned size as its limit.
#[test]
fn an_archive_of_the_wrong_size_is_refused_before_the_digest() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("wrong-size");
    let limit = Arc::new(Mutex::new(0u64));
    let told = Arc::clone(&limit);
    let engines = engines(
        directory.clone(),
        Box::new(move |_url, path, max_bytes| {
            *told.lock().expect("the lock is free") = max_bytes;
            std::fs::write(path, b"not the release")
                .map(|()| Transfer::Finished)
                .map_err(|error| error.to_string())
        }),
        unpacks_the_release(),
    );

    let failure = engines.install(SLUG).expect_err("the size differs");

    assert!(
        matches!(&failure, Failure::DownloadFailed(message)
            if message.contains("not the pinned size") && !message.contains("digest")),
        "{failure:?}"
    );
    assert_eq!(
        *limit.lock().expect("the lock is free"),
        FAKE_ARCHIVE.len() as u64,
        "the transfer is bounded by the pinned size"
    );
    assert!(!directory.join(format!("{ARCHIVE}.part")).exists());
}

/// A `.part` file longer than the pin can never resume into the right file,
/// so it goes before the transfer starts.
#[test]
fn an_oversize_part_file_is_removed_before_the_transfer() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("oversize-part");
    let partial = directory.join(format!("{ARCHIVE}.part"));
    std::fs::write(&partial, [b'x'; 200]).expect("the part file is written");
    let resumed_from = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&resumed_from);
    let engines = engines(
        directory.clone(),
        Box::new(move |_url, path, _max_bytes| {
            *seen.lock().expect("the lock is free") =
                Some(std::fs::metadata(path).map(|data| data.len()).unwrap_or(0));
            std::fs::write(path, FAKE_ARCHIVE)
                .map(|()| Transfer::Finished)
                .map_err(|error| error.to_string())
        }),
        unpacks_the_release(),
    );

    engines
        .install(SLUG)
        .expect("the fresh transfer matches the pin");

    assert_eq!(
        *resumed_from.lock().expect("the lock is free"),
        Some(0),
        "the transfer started from nothing"
    );
}

/// A `bsdtar` that died half way leaves a staging directory with no server jar
/// in it. That is never renamed into place, so no half install is ever run.
#[test]
fn an_unpack_that_left_no_server_jar_installs_nothing() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("half-unpack");
    let engines = engines(
        directory.clone(),
        whole(),
        Box::new(|_archive, into, _admission| {
            std::fs::create_dir_all(into.join(UNPACKS_INTO)).map_err(|error| error.to_string())
        }),
    );

    let failure = engines
        .install(SLUG)
        .expect_err("the tree has no server jar");

    assert!(
        matches!(&failure, Failure::DownloadFailed(message) if message.contains(ENTRY)),
        "{failure:?}"
    );
    assert!(!directory.join(SLUG).exists());
    assert!(!directory.join(format!("{SLUG}.unpack")).exists());
}

/// A cancel keeps the `.part` file, so Install resumes rather than restarts.
#[test]
fn a_cancel_keeps_the_part_file() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("cancel");
    let engines = engines(
        directory.clone(),
        Box::new(|_url, path, _max_bytes| {
            std::fs::write(path, &FAKE_ARCHIVE[..8]).map_err(|error| error.to_string())?;
            cancel::request();
            Ok(Transfer::Cancelled)
        }),
        unpacks_the_release(),
    );

    let failure = engines.install(SLUG).expect_err("the transfer was stopped");
    cancel::reset();

    assert!(
        matches!(&failure, Failure::Cancelled(message) if message.contains("resumes")),
        "{failure:?}"
    );
    assert!(directory.join(format!("{ARCHIVE}.part")).is_file());
    assert!(!directory.join(SLUG).exists());
    assert_eq!(
        engines.list()[0].state,
        State::Partial,
        "the list says the transfer can be picked up again"
    );
}

/// A component that is already there costs nothing: nothing is fetched and
/// nothing is unpacked, so the Settings row is safe to press twice.
#[test]
fn installing_a_component_that_is_already_there_fetches_nothing() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    let directory = scratch("already");
    let tree = directory.join(SLUG);
    std::fs::create_dir_all(&tree).expect("the tree is created");
    std::fs::write(tree.join(ENTRY), b"jar").expect("the server jar is written");
    let fetched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&fetched);
    let engines = engines(
        directory,
        Box::new(move |_url, _path, _max_bytes| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Transfer::Finished)
        }),
        unpacks_the_release(),
    );

    let row = engines.install(SLUG).expect("it is already installed");

    assert_eq!(row.state, State::Ready);
    assert_eq!(fetched.load(Ordering::SeqCst), 0);
}

/// The free-space check runs before the transfer, so a disk with no room for
/// the archive and the tree together refuses rather than filling up and
/// failing on the last byte.
#[test]
fn a_disk_with_no_room_refuses_before_it_fetches_anything() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_digest();
    // Larger than any file system this test could run on.
    pin_the_fake_size(u64::MAX / 4);
    let fetched = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&fetched);
    let engines = engines(
        scratch("no-room"),
        Box::new(move |_url, _path, _max_bytes| {
            counted.fetch_add(1, Ordering::SeqCst);
            Ok(Transfer::Finished)
        }),
        unpacks_the_release(),
    );

    let failure = engines.install(SLUG).expect_err("the disk has no room");
    pin_the_fake_size(FAKE_ARCHIVE.len() as u64);

    assert!(
        matches!(&failure, Failure::BadArguments(message) if message.contains("LanguageTool")),
        "{failure:?}"
    );
    assert_eq!(fetched.load(Ordering::SeqCst), 0);
}

/// The install fetches the row's own URL and unpacks the archive it verified,
/// so a stub server and a stub unpacker see exactly what the real ones would.
#[test]
fn the_install_fetches_the_pinned_url_and_unpacks_the_verified_archive() {
    let _guard = serially();
    cancel::reset();
    pin_the_fake_release();
    std::env::set_var(install::BASE_URL_ENV, "http://127.0.0.1:9/releases");
    let directory = scratch("url");
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorded = Arc::clone(&asked);
    let unpacked = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = Arc::clone(&unpacked);
    let engines = engines(
        directory.clone(),
        Box::new(move |url, path, _max_bytes| {
            recorded
                .lock()
                .expect("the lock is free")
                .push(url.to_string());
            std::fs::write(path, FAKE_ARCHIVE)
                .map(|()| Transfer::Finished)
                .map_err(|error| error.to_string())
        }),
        Box::new(move |archive, into, _admission| {
            seen.lock()
                .expect("the lock is free")
                .push(archive.display().to_string());
            let tree = into.join(UNPACKS_INTO);
            std::fs::create_dir_all(&tree).map_err(|error| error.to_string())?;
            std::fs::write(tree.join(ENTRY), b"jar").map_err(|error| error.to_string())
        }),
    );
    let outcome = engines.install(SLUG);
    std::env::remove_var(install::BASE_URL_ENV);
    outcome.expect("the digest matches the pin");

    assert_eq!(
        asked.lock().expect("the lock is free").as_slice(),
        ["http://127.0.0.1:9/releases/download/LanguageTool-6.6.zip"]
    );
    assert_eq!(
        unpacked.lock().expect("the lock is free").as_slice(),
        [directory.join(ARCHIVE).display().to_string()],
        "the unpack reads the verified archive and never the part file"
    );
}
