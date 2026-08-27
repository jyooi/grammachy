//! Start of the transient user unit that runs the llama.cpp server.
//!
//! Spec section 4 and section 10: transient units only, through `systemd-run
//! --user`, so removing the plugin leaves no unit file behind. The unit runs:
//!
//! ```text
//! systemd-run --user --unit=grammachy-llama --collect \
//!   -- /usr/bin/llama-server --model <gguf> --host 127.0.0.1 --port 8080 \
//!      --ctx-size 4096 --parallel 1 --temp 0 --jinja \
//!      --reasoning-format deepseek --reasoning-budget 1024 \
//!      --reasoning-budget-message "Answer now."
//! ```
//!
//! The server is the `llama-cpp` package from the Arch `extra` repository,
//! which installs `/usr/bin/llama-server`. That package carries no compute
//! backend of its own: `ggml-cpu` or `ggml-vulkan` is what makes it run.
//! This adapter has no hardware facts, so [`INSTALL_LINE`] names both packages.
//! `grammachy doctor` prints the packages this machine's hardware tier wants.
//!
//! Four flags are decisions rather than defaults:
//!
//! - `--ctx-size 4096`. One Check is at most 5,000 UTF-16 units, about 1,400
//!   tokens of English, plus about 250 tokens of prompt and the 2,048 tokens
//!   the request asks for, which is the think and the answer together. That
//!   comes to about 3,700 and leaves the rest as headroom. HUF-171 ran the
//!   benchmark at 2,048, which fits a sentence and not a Check.
//! - `--parallel 1`. One slot, because a Check is one request at a time and
//!   every extra slot costs a KV cache. HUF-181 measured 7.3 GB resident for
//!   the recommended model on one slot.
//! - `--reasoning-format deepseek`. The think goes to `message.reasoning_content`
//!   and never to `message.content`, so the Issue parser never reads it. The
//!   `none` format leaves the think in the content, where a quoted bracket
//!   slices the suggestion array. `response::parse_array` guards that too,
//!   because `openaiBaseUrl` may name a server this adapter did not start.
//! - `--reasoning-budget 1024`. Thinking is on by default (spec section 4) and
//!   the request asks for 2,048 tokens, so the other half belongs to the
//!   answer. The budget message is what the server injects when the think runs
//!   out, which turns a runaway think into an answer rather than a timeout.
//!
//! The model file comes from `~/.local/share/grammachy/models/`, which is where
//! `grammachy model download` and the Settings Models list put it (spec
//! section 5.3). Nothing here downloads anything.

use std::path::{Path, PathBuf};

use crate::engines::local::{self, ServerCommand, StartFailure};

/// The transient unit name from spec section 4.
pub const UNIT_NAME: &str = "grammachy-llama";

/// Context window in tokens, sized for one whole Check.
const CONTEXT_SIZE: usize = 4_096;

/// How many tokens the model may think for, spec section 4. The other half of
/// the 2,048 token request belongs to the answer.
const REASONING_BUDGET: usize = 1_024;

/// Where the server puts the think. `deepseek` is `message.reasoning_content`,
/// which keeps it out of the content the Issue parser reads.
const REASONING_FORMAT: &str = "deepseek";

/// What the server injects when the reasoning budget runs out.
const REASONING_BUDGET_MESSAGE: &str = "Answer now.";

/// Where the `llama-cpp` package installs the server. `doctor` looks for it too.
pub const PACKAGE_SERVER: &str = "/usr/bin/llama-server";

/// The install line this adapter prints when the package is missing.
/// It names CPU first and the Vulkan backend beside it, because the adapter
/// has no hardware facts. `ggml-cpu` is the requirement and `ggml-vulkan` is
/// the accelerator, so a GPU machine wants both. `grammachy doctor` prints a
/// tier-specific line instead (spec section 4). Both packages are in the
/// official `extra` repository.
pub const INSTALL_LINE: &str =
    "sudo pacman -S llama-cpp ggml-cpu   (add ggml-vulkan for a GPU or an iGPU)";

/// Where the downloaded weights live.
///
/// The product path is `$HOME` only, the same rule the Settings file follows
/// (spec section 7), so `XDG_DATA_HOME` is not read.
pub fn models_directory() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".local/share/grammachy/models"))
}

/// The weights file one model name stands for.
///
/// The exact name wins. Failing that, any `.gguf` whose name begins with the
/// model name matches, because a download keeps the quantisation in the file
/// name, as in `gemma-4-e4b-it-Q4_K_M.gguf`. Matching ignores case, because
/// model cards and Settings disagree about it.
pub fn model_file(directory: &Path, model: &str) -> Result<PathBuf, StartFailure> {
    let exact = directory.join(format!("{model}.gguf"));
    if exact.is_file() {
        return Ok(exact);
    }

    let wanted = model.to_ascii_lowercase();
    let mut matches: Vec<PathBuf> = std::fs::read_dir(directory)
        .map_err(|error| {
            StartFailure(format!(
                "No model is installed: {} could not be read ({error}). Download {model} in Settings, Models.",
                directory.display()
            ))
        })?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|kind| kind == "gguf"))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(&wanted))
        })
        .collect();
    matches.sort();

    matches.into_iter().next().ok_or_else(|| {
        StartFailure(format!(
            "No weights file for {model} in {}. Download it in Settings, Models.",
            directory.display()
        ))
    })
}

/// The program and arguments that run the server for one model.
pub fn server_command(model_path: &Path, host: &str, port: u16) -> ServerCommand {
    ServerCommand {
        program: PACKAGE_SERVER.to_string(),
        arguments: vec![
            "--model".to_string(),
            model_path.to_string_lossy().to_string(),
            "--host".to_string(),
            host.to_string(),
            "--port".to_string(),
            port.to_string(),
            "--ctx-size".to_string(),
            CONTEXT_SIZE.to_string(),
            "--parallel".to_string(),
            "1".to_string(),
            "--temp".to_string(),
            "0".to_string(),
            // The chat template of the model file, so the prompt is wrapped the
            // way the model was trained. `chat_template_kwargs` of one request
            // reaches the template through it, which is what makes the
            // thinking Setting a per-request choice rather than a unit flag.
            "--jinja".to_string(),
            "--reasoning-format".to_string(),
            REASONING_FORMAT.to_string(),
            "--reasoning-budget".to_string(),
            REASONING_BUDGET.to_string(),
            "--reasoning-budget-message".to_string(),
            REASONING_BUDGET_MESSAGE.to_string(),
        ],
        environment: Vec::new(),
    }
}

/// Start the transient unit, or answer `Ok(())` when it already runs.
pub fn start(model: &str, host: &str, port: u16) -> Result<(), StartFailure> {
    if !Path::new(PACKAGE_SERVER).is_file() {
        return Err(StartFailure(format!(
            "llama.cpp is not installed: {PACKAGE_SERVER} does not exist. Install it with: {INSTALL_LINE}"
        )));
    }

    let directory = models_directory()
        .ok_or_else(|| StartFailure("No model directory: HOME is not set.".to_string()))?;
    let model_path = model_file(&directory, model)?;
    let command = server_command(&model_path, host, port);

    local::start_unit(UNIT_NAME, "Grammachy llama.cpp server", &command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_names_the_model_and_stays_on_the_loopback_interface() {
        let command = server_command(Path::new("/models/gemma.gguf"), "127.0.0.1", 8080);

        assert_eq!(command.program, PACKAGE_SERVER);
        assert_eq!(
            command.arguments,
            [
                "--model",
                "/models/gemma.gguf",
                "--host",
                "127.0.0.1",
                "--port",
                "8080",
                "--ctx-size",
                "4096",
                "--parallel",
                "1",
                "--temp",
                "0",
                "--jinja",
                "--reasoning-format",
                "deepseek",
                "--reasoning-budget",
                "1024",
                "--reasoning-budget-message",
                "Answer now.",
            ]
        );
        assert!(command.environment.is_empty());
    }

    #[test]
    fn a_missing_package_names_the_pacman_line() {
        if Path::new(PACKAGE_SERVER).is_file() {
            return;
        }
        let failure = start("gemma-4-e4b-it", "127.0.0.1", 8080).expect_err("nothing is installed");

        assert!(failure.0.contains("llama-cpp"), "{}", failure.0);
        assert!(failure.0.contains("ggml-cpu"), "{}", failure.0);
    }
}
