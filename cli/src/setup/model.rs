//! The weights `grammachy setup` downloads for the `openai` engine.
//!
//! Spec section 10, step 1: the file lands in
//! `~/.local/share/grammachy/models/`, and only when the engine setting is
//! `openai`. The other two engines need nothing on disk, so the step is skipped
//! for them and nothing is ever downloaded behind the user's back.
//!
//! Hardware tiers are the install step only (spec section 4): the same weights
//! file runs on both, and the tier decides which llama.cpp backend package the
//! machine wants. `setup` cannot install a package, because pacman steps stay
//! manual, so it names the one this machine needs and leaves it to the user.
//!
//! The transfer itself is `curl`, the same tool `bin/bootstrap.sh` uses for the
//! binary (spec section 10). Keeping it out of the CLI keeps TLS out of the
//! 13 MB static binary, whose own HTTP client only ever talks to a loopback
//! engine. The download runs into a `.part` file and is renamed once it is
//! whole, so an interrupted run resumes rather than leaving half a model behind
//! that `grammachy check` would then try to load.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engines::openai::unit;

/// Points the CLI at another models directory. The test suite sets it, so no
/// test writes the real one. Not a user-facing setting.
pub const DIRECTORY_ENV: &str = "GRAMMACHY_MODELS_DIR";

/// Points the CLI at another weights host, which is how the download is tested
/// against a stub server. Not a user-facing setting.
pub const BASE_URL_ENV: &str = "GRAMMACHY_MODEL_BASE_URL";

/// Where the recommended weights come from.
const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// What the CLI knows how to fetch: the repository and the file name of every
/// model the Settings dropdown recommends (spec section 7 default, spec
/// section 13.1 re-decides it on every tag).
const CATALOGUE: &[(&str, &str, &str)] = &[(
    "gemma-4-e4b-it",
    "google/gemma-4-E4B-it-GGUF",
    "gemma-4-E4B-it-Q4_K_M.gguf",
)];

/// What fetches one URL into one path.
///
/// The real one is [`curl`]. Tests hand in their own, which is how the step is
/// covered without reaching the network.
pub type Downloader = Box<dyn Fn(&str, &Path) -> Result<(), String> + Send + Sync>;

/// Which llama.cpp backend package this machine wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// A GPU or an iGPU, which the Vulkan backend drives.
    Vulkan,
    /// No render device, so the CPU backend is the only one that runs.
    Cpu,
}

impl Tier {
    /// The pacman package that makes `llama-server` run on this machine.
    pub fn backend_package(self) -> &'static str {
        match self {
            Tier::Vulkan => "ggml-vulkan",
            Tier::Cpu => "ggml-cpu",
        }
    }
}

/// The tier of this machine.
pub fn tier() -> Tier {
    tier_of(Path::new("/dev/dri"))
}

/// The tier a machine with this DRM directory has.
///
/// A render node is what a Vulkan driver needs, and it is the one signal that
/// is there on a headless session as well. Tests hand in their own directory,
/// so the answer never depends on the developer's hardware.
pub fn tier_of(dri_directory: &Path) -> Tier {
    let has_render_node = std::fs::read_dir(dri_directory)
        .map(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("renderD"))
            })
        })
        .unwrap_or(false);

    if has_render_node {
        Tier::Vulkan
    } else {
        Tier::Cpu
    }
}

/// Where `setup` keeps the weights on this machine.
pub fn directory() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(DIRECTORY_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    unit::models_directory()
}

/// The file name and URL one model name stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weights {
    pub file_name: String,
    pub url: String,
}

/// What the catalogue knows about this model, or `None` for a name the user
/// typed into Settings themselves.
pub fn weights(model: &str) -> Option<Weights> {
    let wanted = model.to_ascii_lowercase();
    let (_, repository, file_name) = CATALOGUE
        .iter()
        .find(|(name, _, _)| name.eq_ignore_ascii_case(&wanted))?;
    let base = std::env::var(BASE_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let base = base.trim_end_matches('/');

    Some(Weights {
        file_name: (*file_name).to_string(),
        url: format!("{base}/{repository}/resolve/main/{file_name}"),
    })
}

/// What one run of the model step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The weights were already on disk, so nothing was fetched.
    Present(PathBuf),
    /// The weights were fetched into this path.
    Downloaded(PathBuf),
}

/// Put the weights of one model in place, or answer that they already are.
pub fn ensure(model: &str, directory: &Path, download: &Downloader) -> Result<Outcome, String> {
    if let Ok(path) = unit::model_file(directory, model) {
        return Ok(Outcome::Present(path));
    }

    let weights = weights(model).ok_or_else(|| {
        format!(
            "No download is known for the model {model}. Put its .gguf file in {} yourself.",
            directory.display()
        )
    })?;

    std::fs::create_dir_all(directory)
        .map_err(|error| format!("{} could not be created: {error}", directory.display()))?;

    let final_path = directory.join(&weights.file_name);
    let partial = directory.join(format!("{}.part", weights.file_name));
    download(&weights.url, &partial)?;
    std::fs::rename(&partial, &final_path).map_err(|error| {
        format!(
            "{} could not be renamed to {}: {error}",
            partial.display(),
            final_path.display()
        )
    })?;

    Ok(Outcome::Downloaded(final_path))
}

/// The downloader this run uses.
pub fn downloader() -> Downloader {
    Box::new(curl)
}

/// Fetch one URL into one path, resuming a `.part` file that is already there.
pub fn curl(url: &str, path: &Path) -> Result<(), String> {
    let output = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--retry")
        .arg("3")
        // Carry on where an interrupted run stopped. curl answers 33 when the
        // server cannot resume, which the caller turns into a plain message.
        .arg("--continue-at")
        .arg("-")
        .arg("--output")
        .arg(path)
        .arg(url)
        .output()
        .map_err(|error| {
            format!("curl could not run: {error}. Install it with: sudo pacman -S curl")
        })?;

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "curl could not fetch {url}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_without_a_render_node_is_the_cpu_tier() {
        let empty = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        assert_eq!(tier_of(&empty), Tier::Cpu);
        assert_eq!(Tier::Cpu.backend_package(), "ggml-cpu");
        assert_eq!(Tier::Vulkan.backend_package(), "ggml-vulkan");
    }

    #[test]
    fn the_recommended_model_has_a_download() {
        let weights = weights("gemma-4-e4b-it").expect("the default model is in the catalogue");

        assert_eq!(weights.file_name, "gemma-4-E4B-it-Q4_K_M.gguf");
        assert!(weights.url.ends_with(&weights.file_name), "{}", weights.url);
    }

    #[test]
    fn an_unknown_model_has_none() {
        assert!(weights("something-the-user-typed").is_none());
    }
}
