//! `grammachy model list` and `grammachy model remove`, spec section 5.3.
//!
//! Every run here works on a scratch directory and on values handed in, so no
//! test reads the real models directory, stops the real llama.cpp unit, or
//! reaches the weights host. The download verb owns its own test binary,
//! `model_download.rs`, because it sets the digest for the whole process.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use grammachy::model::{Failure, ModelEnvelope, Models, State, Stopper, Transfer};

/// The pinned file name of each catalogue row, so a test can put one in place.
const GEMMA: &str = "gemma-4-E4B-it-Q4_K_M.gguf";
const QWEN: &str = "Qwen3-4B-Instruct-2507-Q4_K_M.gguf";

fn scratch(name: &str) -> PathBuf {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("model-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

/// A run whose two side effects are counters rather than the machine.
fn models(directory: PathBuf) -> (Models, Arc<AtomicUsize>) {
    let stops = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&stops);
    let stop: Stopper = Box::new(move |_unit| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    (
        Models {
            directory,
            download: Box::new(|_url, _path| Ok(Transfer::Finished)),
            stop,
        },
        stops,
    )
}

fn row<'a>(rows: &'a [grammachy::model::ModelRow], name: &str) -> &'a grammachy::model::ModelRow {
    rows.iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("{name} has a row"))
}

/// The acceptance criterion of the Models list: the state of every row is read
/// from the disk it is about.
#[test]
fn every_catalogue_row_reports_what_the_directory_actually_holds() {
    let directory = scratch("states");
    std::fs::write(directory.join(GEMMA), b"whole weights").expect("the ready file is written");
    std::fs::write(directory.join(format!("{QWEN}.part")), vec![0u8; 4_096])
        .expect("the part file is written");
    let (models, _) = models(directory);

    let rows = models.list();

    assert_eq!(rows.len(), 3, "one row per catalogue entry");
    assert_eq!(row(&rows, "gemma-4-e4b-it").state, State::Ready);
    assert_eq!(row(&rows, "qwen3-4b-instruct").state, State::Partial);
    assert_eq!(row(&rows, "phi-4-mini-instruct").state, State::Absent);
}

/// The shell polls `model list` while a download runs and reads `partialBytes`,
/// so that number is the length of the `.part` file and nothing else.
#[test]
fn partial_bytes_is_the_length_of_the_part_file_and_zero_otherwise() {
    let directory = scratch("partial-bytes");
    std::fs::write(directory.join(format!("{QWEN}.part")), vec![7u8; 1_234])
        .expect("the part file is written");
    std::fs::write(directory.join(GEMMA), b"whole").expect("the ready file is written");
    let (models, _) = models(directory);

    let rows = models.list();

    assert_eq!(row(&rows, "qwen3-4b-instruct").partial_bytes, 1_234);
    assert_eq!(row(&rows, "gemma-4-e4b-it").partial_bytes, 0);
    assert_eq!(row(&rows, "phi-4-mini-instruct").partial_bytes, 0);
}

/// Every row carries the pinned size the progress bar measures against, and the
/// licence from the one table spec section 13.1 fixes.
#[test]
fn every_row_carries_its_pinned_size_and_its_licence() {
    let (models, _) = models(scratch("pins"));

    let rows = models.list();

    assert_eq!(row(&rows, "gemma-4-e4b-it").size_bytes, 4_977_171_584);
    assert_eq!(row(&rows, "qwen3-4b-instruct").size_bytes, 2_497_281_120);
    assert_eq!(row(&rows, "phi-4-mini-instruct").size_bytes, 2_491_874_272);
    assert_eq!(row(&rows, "qwen3-4b-instruct").licence, "Apache-2.0");
    assert_eq!(row(&rows, "phi-4-mini-instruct").licence, "MIT");
    for row in &rows {
        assert!(row.file_name.ends_with(".gguf"), "{}", row.name);
    }
}

/// Spec section 5.3: the report names the directory and the free bytes, so the
/// Settings view can say what a download would cost before it starts one.
#[test]
fn the_report_names_the_directory_and_the_free_bytes() {
    let directory = scratch("report");
    let (models, _) = models(directory.clone());

    let json = serde_json::to_value(models.list_envelope()).expect("the envelope serialises");

    assert_eq!(json["contractVersion"], 1);
    assert_eq!(json["verb"], "list");
    assert_eq!(json["directory"], directory.display().to_string());
    assert!(
        json["freeBytes"].as_u64().expect("freeBytes is a number") > 0,
        "the scratch directory sits on a file system with room"
    );
    assert_eq!(
        json["models"].as_array().expect("models is a list").len(),
        3
    );
    assert_eq!(json["models"][0]["name"], "gemma-4-e4b-it");
    assert_eq!(json["models"][0]["state"], "absent");
    assert_eq!(json["models"][0]["partialBytes"], 0);
}

#[test]
fn a_row_serialises_with_the_names_the_shell_reads() {
    let directory = scratch("serialise");
    std::fs::write(directory.join(format!("{QWEN}.part")), vec![0u8; 9])
        .expect("the part file is written");
    let (models, _) = models(directory);

    let json = serde_json::to_value(models.list_envelope()).expect("the envelope serialises");
    let qwen = &json["models"][1];

    assert_eq!(qwen["name"], "qwen3-4b-instruct");
    assert_eq!(qwen["fileName"], QWEN);
    assert_eq!(qwen["state"], "partial");
    assert_eq!(qwen["partialBytes"], 9);
    assert_eq!(qwen["sizeBytes"], 2_497_281_120u64);
    assert_eq!(qwen["licence"], "Apache-2.0");
}

// ------------------------------------------------------------------- remove

#[test]
fn remove_deletes_both_the_weights_and_the_part_file() {
    let directory = scratch("remove");
    std::fs::write(directory.join(GEMMA), b"whole").expect("the ready file is written");
    std::fs::write(directory.join(format!("{GEMMA}.part")), b"half")
        .expect("the part file is written");
    let (models, stops) = models(directory.clone());

    let row = models
        .delete("gemma-4-e4b-it", false)
        .expect("the files are deleted");

    assert_eq!(row.state, State::Absent);
    assert_eq!(row.partial_bytes, 0);
    assert!(!directory.join(GEMMA).exists());
    assert!(!directory.join(format!("{GEMMA}.part")).exists());
    assert_eq!(stops.load(Ordering::SeqCst), 0, "nothing was in use");
}

#[test]
fn remove_of_a_model_that_is_not_there_is_not_a_failure() {
    let directory = scratch("remove-absent");
    let (models, _) = models(directory);

    let row = models
        .delete("phi-4-mini-instruct", false)
        .expect("an absent model is already removed");

    assert_eq!(row.state, State::Absent);
}

/// The weights are open in the running unit, so it is stopped before the file
/// under it goes away.
#[test]
fn remove_of_the_model_in_use_stops_the_unit_first() {
    let directory = scratch("remove-in-use");
    std::fs::write(directory.join(GEMMA), b"whole").expect("the ready file is written");
    let (models, stops) = models(directory.clone());

    assert!(models.is_in_use("gemma-4-e4b-it", "gemma-4-e4b-it"));
    let row = models
        .delete("gemma-4-e4b-it", true)
        .expect("the file is deleted");

    assert_eq!(row.state, State::Absent);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert!(!directory.join(GEMMA).exists());
}

/// Only the file the setting resolves to counts as in use, and the answer comes
/// from the same resolver a Check uses.
#[test]
fn a_model_the_setting_does_not_name_is_not_in_use() {
    let directory = scratch("in-use");
    std::fs::write(directory.join(GEMMA), b"whole").expect("the gemma file is written");
    std::fs::write(directory.join(QWEN), b"whole").expect("the qwen file is written");
    let (models, _) = models(directory);

    assert!(models.is_in_use("gemma-4-e4b-it", "gemma-4-e4b-it"));
    assert!(!models.is_in_use("qwen3-4b-instruct", "gemma-4-e4b-it"));
    assert!(models.is_in_use("qwen3-4b-instruct", "qwen3-4b"));
}

/// Nothing on disk is nothing in use, so a Remove of an absent row never stops
/// the unit.
#[test]
fn an_empty_directory_has_nothing_in_use() {
    let (models, _) = models(scratch("in-use-empty"));

    assert!(!models.is_in_use("gemma-4-e4b-it", "gemma-4-e4b-it"));
}

/// A `.part` file alone leaves the unit alone: llama.cpp never opened it.
#[test]
fn removing_a_partial_download_never_stops_the_unit() {
    let directory = scratch("remove-partial");
    std::fs::write(directory.join(format!("{GEMMA}.part")), b"half")
        .expect("the part file is written");
    let (models, stops) = models(directory.clone());

    models
        .delete("gemma-4-e4b-it", true)
        .expect("the part file is deleted");

    assert_eq!(stops.load(Ordering::SeqCst), 0);
    assert!(!directory.join(format!("{GEMMA}.part")).exists());
}

// ------------------------------------------------------------------ unknown

#[test]
fn a_name_the_catalogue_does_not_carry_is_bad_arguments() {
    let (models, _) = models(scratch("unknown"));

    for failure in [
        models
            .delete("something-the-user-typed", false)
            .unwrap_err(),
        models.fetch("something-the-user-typed").unwrap_err(),
    ] {
        let Failure::BadArguments(message) = failure else {
            panic!("an unknown name is bad_arguments: {failure:?}")
        };
        assert!(message.contains("gemma-4-e4b-it"), "{message}");
        assert!(message.contains("phi-4-mini-instruct"), "{message}");
    }
}

#[test]
fn an_error_envelope_carries_the_code_and_exits_one() {
    let envelope = ModelEnvelope::failure(Failure::BadArguments("no such model".to_string()));

    assert_eq!(envelope.exit_code(), 1);
    let json: serde_json::Value =
        serde_json::from_str(&envelope.to_json()).expect("the envelope is one JSON object");
    assert_eq!(json["contractVersion"], 1);
    assert_eq!(json["error"]["code"], "bad_arguments");
    assert_eq!(json["error"]["message"], "no such model");
}

#[test]
fn the_two_download_codes_are_their_own() {
    let cancelled = ModelEnvelope::failure(Failure::Cancelled("stopped".to_string()));
    let failed = ModelEnvelope::failure(Failure::DownloadFailed("digest".to_string()));

    let cancelled: serde_json::Value = serde_json::from_str(&cancelled.to_json()).unwrap();
    let failed: serde_json::Value = serde_json::from_str(&failed.to_json()).unwrap();

    assert_eq!(cancelled["error"]["code"], "cancelled");
    assert_eq!(failed["error"]["code"], "download_failed");
}
