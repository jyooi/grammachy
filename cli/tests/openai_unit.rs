//! The `grammachy-llama` unit start, in isolation.
//!
//! Nothing here runs `systemd-run` or llama.cpp. The pieces the start is built
//! from are pure functions over a directory and a model name, so they are
//! covered on a machine with neither the package nor the weights (spec section
//! 13). The "does nothing when the port already answers" half of the start
//! lives in `openai_stub.rs`, where the adapter is what decides.

use std::path::{Path, PathBuf};

use grammachy::engines::openai::unit::{model_file, models_directory, server_command, UNIT_NAME};

fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is created");
    directory
}

#[test]
fn the_unit_is_the_one_the_spec_names() {
    assert_eq!(UNIT_NAME, "grammachy-llama");
}

#[test]
fn the_server_command_binds_the_loopback_address_only() {
    let command = server_command(Path::new("/models/gemma.gguf"), "127.0.0.1", 8080);

    assert_eq!(command.program, "/usr/bin/llama-server");
    let host = command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "--host")
        .expect("the command sets a host");
    assert_eq!(host[1], "127.0.0.1");
    // No flag opens the server to the network.
    assert!(!command
        .arguments
        .iter()
        .any(|argument| argument == "--public"));
}

#[test]
fn the_server_command_fits_one_whole_check() {
    let command = server_command(Path::new("/models/gemma.gguf"), "::1", 9090);

    let context = command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "--ctx-size")
        .expect("the command sets a context size");
    // One Check is 5,000 UTF-16 units, roughly 1,400 tokens, plus the prompt
    // and the answer. The benchmark ran at 2,048, which fits a sentence only.
    assert!(
        context[1].parse::<usize>().expect("it is a number") >= 4_096,
        "the context is {}",
        context[1]
    );

    let port = command
        .arguments
        .windows(2)
        .find(|pair| pair[0] == "--port")
        .expect("the command sets a port");
    assert_eq!(port[1], "9090");
}

#[test]
fn the_exact_file_name_wins() {
    let directory = scratch("llama-exact");
    std::fs::write(directory.join("gemma-4-e4b-it.gguf"), b"x").unwrap();
    std::fs::write(directory.join("gemma-4-e4b-it-Q4_K_M.gguf"), b"x").unwrap();

    let found = model_file(&directory, "gemma-4-e4b-it").expect("the exact file is found");

    assert_eq!(found, directory.join("gemma-4-e4b-it.gguf"));
}

#[test]
fn a_quantised_download_still_matches_the_model_name() {
    let directory = scratch("llama-quantised");
    std::fs::write(directory.join("gemma-4-E4B-it-Q4_K_M.gguf"), b"x").unwrap();
    std::fs::write(directory.join("gemma-4-e4b-it.txt"), b"x").unwrap();

    let found = model_file(&directory, "gemma-4-e4b-it").expect("the download is found");

    assert_eq!(found, directory.join("gemma-4-E4B-it-Q4_K_M.gguf"));
}

#[test]
fn another_model_in_the_directory_is_never_used() {
    let directory = scratch("llama-other");
    std::fs::write(directory.join("qwen3.5-9b-Q4_K_M.gguf"), b"x").unwrap();

    let failure = model_file(&directory, "gemma-4-e4b-it").expect_err("nothing matches");

    assert!(failure.0.contains("gemma-4-e4b-it"), "{}", failure.0);
    assert!(failure.0.contains("Settings, Models"), "{}", failure.0);
}

#[test]
fn a_directory_that_does_not_exist_says_so_without_panicking() {
    let missing = scratch("llama-missing").join("not-here");

    let failure = model_file(&missing, "gemma-4-e4b-it").expect_err("the directory is absent");

    assert!(failure.0.contains("Settings, Models"), "{}", failure.0);
}

#[test]
fn the_models_directory_is_the_home_path_of_the_spec() {
    let directory = models_directory().expect("HOME is set in the test environment");

    assert!(
        directory.ends_with(".local/share/grammachy/models"),
        "{}",
        directory.display()
    );
}
