use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use grammachy::engines::install::{EngineEnvelope, EngineRow, Engines};
use grammachy::engines::install::{Failure, State, Stopper, Transfer};
use grammachy::engines::languagetool;

const ARCHIVE: &str = "LanguageTool-6.6.zip";

const ENTRY: &str = "languagetool-server.jar";

fn scratch(name: &str) -> PathBuf {
    let directory =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("engine-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

fn engines(directory: PathBuf) -> (Engines, Arc<AtomicUsize>) {
    let stops = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&stops);
    let stop: Stopper = Box::new(move |_unit| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    (
        Engines {
            directory,
            download: Box::new(|_url, _path, _max_bytes| Ok(Transfer::Finished)),
            extract: Box::new(|_archive, _into, _admission| Ok(())),
            stop,
        },
        stops,
    )
}

fn engines_that_cannot_stop(directory: PathBuf, why: String) -> Engines {
    Engines {
        directory,
        download: Box::new(|_url, _path, _max_bytes| Ok(Transfer::Finished)),
        extract: Box::new(|_archive, _into, _admission| Ok(())),
        stop: Box::new(move |_unit| Err(why.clone())),
    }
}

fn install_tree(directory: &std::path::Path) -> PathBuf {
    let tree = directory.join("languagetool");
    std::fs::create_dir_all(tree.join("libs")).expect("the tree is created");
    std::fs::write(tree.join(ENTRY), b"jar").expect("the server jar is written");
    tree
}

fn row<'a>(rows: &'a [EngineRow], slug: &str) -> &'a EngineRow {
    rows.iter()
        .find(|row| row.slug == slug)
        .unwrap_or_else(|| panic!("{slug} has a row"))
}

#[test]
fn the_row_reports_what_the_directory_actually_holds() {
    let directory = scratch("states");
    let (engines, _) = engines(directory.clone());
    assert_eq!(row(&engines.list(), "languagetool").state, State::Absent);

    std::fs::write(directory.join(format!("{ARCHIVE}.part")), vec![0u8; 4_096])
        .expect("the part file is written");
    assert_eq!(row(&engines.list(), "languagetool").state, State::Partial);

    let tree = install_tree(&directory);
    let rows = engines.list();
    assert_eq!(rows.len(), 1, "one row per catalogue entry");
    assert_eq!(row(&rows, "languagetool").state, State::Ready);
    assert_eq!(row(&rows, "languagetool").path, tree.display().to_string());
}

/// A `bsdtar` that died half way leaves a directory behind, which is not an
/// install. Only the entry file settles it, so the next Install starts over
/// rather than running a tree with no server in it.
#[test]
fn a_directory_without_the_server_jar_is_not_installed() {
    let directory = scratch("half-unpacked");
    std::fs::create_dir_all(directory.join("languagetool/libs")).expect("the tree is created");
    let (engines, _) = engines(directory);

    assert_eq!(row(&engines.list(), "languagetool").state, State::Absent);
}

#[test]
fn an_installed_tree_never_claims_the_package_supplies_it() {
    let directory = scratch("from-package");
    install_tree(&directory);
    let (engines, _) = engines(directory);

    assert!(!row(&engines.list(), "languagetool").from_package);
}

#[test]
fn partial_bytes_is_the_length_of_the_part_file_and_zero_otherwise() {
    let directory = scratch("partial-bytes");
    std::fs::write(directory.join(format!("{ARCHIVE}.part")), vec![7u8; 1_234])
        .expect("the part file is written");
    let (engines, _) = engines(directory.clone());

    assert_eq!(row(&engines.list(), "languagetool").partial_bytes, 1_234);

    install_tree(&directory);
    let rows = engines.list();
    assert_eq!(row(&rows, "languagetool").state, State::Ready);
    assert_eq!(
        row(&rows, "languagetool").partial_bytes,
        0,
        "a finished row reports no part length, whatever is beside it"
    );
}

#[test]
fn the_row_carries_its_pinned_size_its_licence_and_the_java_requirement() {
    let (engines, _) = engines(scratch("pins"));

    let rows = engines.list();
    let languagetool = row(&rows, "languagetool");

    assert_eq!(languagetool.name, "LanguageTool");
    assert_eq!(languagetool.version, "6.6");
    assert_eq!(languagetool.size_bytes, 251_998_221);
    assert_eq!(languagetool.licence, "LGPL-2.1-or-later");
    assert!(languagetool.needs_java);
}

#[test]
fn remove_deletes_the_installed_tree_and_stops_the_unit() {
    let directory = scratch("remove");
    let tree = install_tree(&directory);
    std::fs::write(directory.join(ARCHIVE), b"zip").expect("the archive is written");
    std::fs::write(directory.join(format!("{ARCHIVE}.part")), b"part")
        .expect("the part file is written");
    let (engines, stops) = engines(directory.clone());

    let row = engines.remove("languagetool").expect("the tree goes");

    assert_eq!(row.state, State::Absent);
    assert!(!tree.exists());
    assert!(!directory.join(ARCHIVE).exists());
    assert!(!directory.join(format!("{ARCHIVE}.part")).exists());
    assert_eq!(
        stops.load(Ordering::SeqCst),
        1,
        "the server holds its jars open, so it is stopped before the tree goes"
    );
}

#[test]
fn remove_with_nothing_installed_stops_nothing_and_is_not_a_failure() {
    let (engines, stops) = engines(scratch("remove-absent"));

    let row = engines
        .remove("languagetool")
        .expect("there is nothing to do");

    assert_eq!(row.state, State::Absent);
    assert_eq!(stops.load(Ordering::SeqCst), 0);
}

/// A transient unit is collected the moment it stops, so `systemctl` exits 5
/// on one that was not running. That is the outcome Remove wanted, so it goes
/// on: the ordinary state before the first Check of a session is exactly that.
#[test]
fn a_unit_that_was_not_running_does_not_keep_the_tree() {
    let directory = scratch("stop-nothing");
    let tree = install_tree(&directory);
    let engines = engines_that_cannot_stop(
        directory,
        format!(
            "systemctl could not stop x: {}",
            grammachy::engines::install::NOT_LOADED
        ),
    );

    let row = engines
        .remove("languagetool")
        .expect("nothing was running, so nothing held the jars open");

    assert_eq!(row.state, State::Absent);
    assert!(!tree.exists());
}

#[test]
fn a_stop_that_failed_keeps_the_tree() {
    let directory = scratch("stop-failed");
    let tree = install_tree(&directory);
    let engines = engines_that_cannot_stop(
        directory,
        "systemctl could not stop x: Access denied".to_string(),
    );

    let failure = engines
        .remove("languagetool")
        .expect_err("the unit is still running on this tree");

    assert!(
        matches!(&failure, Failure::BadArguments(message) if message.contains("Access denied")),
        "{failure:?}"
    );
    assert!(tree.join(ENTRY).is_file(), "the tree is still there");
}

#[test]
fn remove_stops_the_languagetool_unit_by_name() {
    let directory = scratch("unit-name");
    install_tree(&directory);
    let named = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let recorded = Arc::clone(&named);
    let engines = Engines {
        directory,
        download: Box::new(|_url, _path, _max_bytes| Ok(Transfer::Finished)),
        extract: Box::new(|_archive, _into, _admission| Ok(())),
        stop: Box::new(move |unit| {
            recorded
                .lock()
                .expect("the lock is free")
                .push(unit.to_string());
            Ok(())
        }),
    };

    engines.remove("languagetool").expect("the tree goes");

    assert_eq!(
        named.lock().expect("the lock is free").as_slice(),
        [languagetool::unit::UNIT_NAME]
    );
}

#[test]
fn an_engine_that_has_nothing_to_install_is_refused_by_name() {
    let (engines, _) = engines(scratch("unknown"));

    for slug in ["harper", "gector", "something-else"] {
        let failure = engines
            .remove(slug)
            .err()
            .unwrap_or_else(|| panic!("{slug} has nothing to install, so it is refused"));

        assert!(
            matches!(&failure, Failure::BadArguments(message) if message.contains("languagetool")),
            "{slug}: {failure:?}"
        );
    }
}

#[test]
fn the_list_envelope_carries_the_directory_and_the_free_space() {
    let directory = scratch("envelope");
    let (engines, _) = engines(directory.clone());

    let json = serde_json::to_value(engines.list_envelope()).expect("the envelope serialises");

    assert_eq!(json["contractVersion"], 1);
    assert_eq!(json["verb"], "list");
    assert_eq!(json["directory"], directory.display().to_string());
    assert!(json["freeBytes"].as_u64().is_some());
    assert_eq!(json["engines"][0]["slug"], "languagetool");
    assert_eq!(json["engines"][0]["state"], "absent");
    assert_eq!(json["engines"][0]["needsJava"], true);
}

#[test]
fn an_unknown_slug_is_bad_arguments() {
    let (engines, _) = engines(scratch("bad-slug"));

    let envelope = match engines.remove("harper") {
        Ok(_) => panic!("harper is not a component"),
        Err(failure) => EngineEnvelope::failure(failure),
    };
    let json = serde_json::to_value(&envelope).expect("the envelope serialises");

    assert_eq!(envelope.exit_code(), 1);
    assert_eq!(json["error"]["code"], "bad_arguments");
    assert!(
        json["error"]["message"]
            .as_str()
            .expect("the message is text")
            .contains("languagetool"),
        "the refusal names what this subcommand does know: {json}"
    );
}
