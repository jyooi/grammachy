//! `grammachy model`, spec sections 5.3 and 10.
//!
//! The weights the `openai` engine runs on, and the three things a user does
//! with them: see what is on disk, fetch one, and delete one. `grammachy setup`
//! calls [`ensure`] for the one model the Settings name; the Models list in the
//! Settings view calls the three verbs of this module instead, so the download
//! no longer needs a terminal.
//!
//! The file lands in `~/.local/share/grammachy/models/`. Every model this
//! module knows is a [`CATALOGUE`] row pinned by sha256 and by byte size, the
//! way `cli.lock` pins the CLI binary. A weights file the user placed there by
//! hand is not a row: it stays reachable through the `openaiModel` text field,
//! because `unit::model_file` resolves a name to any `.gguf` that starts with
//! it, and no verb here ever touches it.
//!
//! Hardware tiers are the install step only (spec section 4): the same weights
//! file runs on both, and the tier decides which llama.cpp backend package the
//! machine wants. Nothing here installs a package, because pacman steps stay
//! manual.
//!
//! The transfer itself is `curl`, the same tool `bin/bootstrap.sh` uses for the
//! binary (spec section 10). `curl` resumes an interrupted transfer of a
//! multi-gigabyte file and retries a failed one, which the HTTP client of the
//! CLI does not. The download runs into a `.part` file.
//! The file is renamed only when it is whole and the pinned sha256 matches.
//!
//! Every path and every side effect is a seam: `GRAMMACHY_MODELS_DIR`,
//! `GRAMMACHY_MODEL_BASE_URL`, `GRAMMACHY_MODEL_SHA256`, `GRAMMACHY_LLAMA_STOP`,
//! plus the [`Downloader`] and [`Stopper`] values. No test reaches the real
//! weights host, the real models directory, or a real unit.

pub mod cancel;
pub mod digest;
pub mod disk;
pub mod envelope;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::args::{ModelNameArgs, ModelVerb};
use crate::bench::weights as licences;
use crate::engines::openai::unit;

pub use digest::sha256_hex;
pub use envelope::{ModelEnvelope, ModelReport, ModelRow, State};

/// Points the CLI at another models directory. The test suite sets it, so no
/// test writes the real one. Not a user-facing setting.
pub const DIRECTORY_ENV: &str = "GRAMMACHY_MODELS_DIR";

/// Points the CLI at another weights host, which is how the download is tested
/// against a stub server. Not a user-facing setting.
pub const BASE_URL_ENV: &str = "GRAMMACHY_MODEL_BASE_URL";

/// Points the CLI at another expected digest for a small fake file.
/// Not a user-facing setting.
pub const SHA256_ENV: &str = "GRAMMACHY_MODEL_SHA256";

/// Keeps `model remove` from stopping the real llama.cpp unit. Tests and CI set
/// it to `never`. Not a user-facing setting.
pub const STOP_ENV: &str = "GRAMMACHY_LLAMA_STOP";

/// Where the recommended weights come from.
const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// One model the Settings Models list offers.
///
/// Spec section 7 fixes the default and spec section 13.1 re-decides the
/// recommendation on every tag. Every row is pinned twice: `sha256` is what the
/// rename checks, and `size_bytes` is what the free-space check and the
/// progress bar measure against. Both come from the Hugging Face LFS pointer,
/// `x-linked-etag` and `x-linked-size`, of a request that carries no token.
///
/// A row belongs here only when its URL answers an unauthenticated 200. The
/// `google` path of the Gemma file answers 401, which is why the `unsloth`
/// mirror is the one named.
struct CatalogueRow {
    name: &'static str,
    repository: &'static str,
    file_name: &'static str,
    sha256: &'static str,
    size_bytes: u64,
}

const CATALOGUE: &[CatalogueRow] = &[
    CatalogueRow {
        name: "gemma-4-e4b-it",
        repository: "unsloth/gemma-4-E4B-it-GGUF",
        file_name: "gemma-4-E4B-it-Q4_K_M.gguf",
        sha256: "85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87",
        size_bytes: 4_977_171_584,
    },
    CatalogueRow {
        name: "qwen3-4b-instruct",
        repository: "unsloth/Qwen3-4B-Instruct-2507-GGUF",
        file_name: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
        size_bytes: 2_497_281_120,
    },
    CatalogueRow {
        name: "phi-4-mini-instruct",
        repository: "unsloth/Phi-4-mini-instruct-GGUF",
        file_name: "Phi-4-mini-instruct-Q4_K_M.gguf",
        sha256: "88c00229914083cd112853aab84ed51b87bdf6b9ce42f532d8c85c7c63b1730a",
        size_bytes: 2_491_874_272,
    },
];

/// What one transfer did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    /// The whole file arrived.
    Finished,
    /// A cancel stopped it. The `.part` file is kept, so the next run resumes.
    Cancelled,
}

/// What fetches one URL into one path.
///
/// The real one is [`curl`]. Tests hand in their own, which is how the step is
/// covered without reaching the network.
pub type Downloader = Box<dyn Fn(&str, &Path) -> Result<Transfer, String> + Send + Sync>;

/// What stops the llama.cpp unit before its weights file is deleted.
///
/// The real one runs `systemctl --user stop`. Tests hand in their own, because
/// no test may touch the unit the live shell uses.
pub type Stopper = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Which llama.cpp backend package this machine wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// A GPU or an iGPU, which the Vulkan backend drives.
    Vulkan,
    /// No render device, so the CPU backend is the only one that runs.
    Cpu,
}

impl Tier {
    /// The pacman packages that make `llama-server` run on this machine.
    ///
    /// `ggml-cpu` is what the server needs on every machine. A render device
    /// earns `ggml-vulkan` beside it, which is the accelerator and not a
    /// replacement, so the Vulkan tier names both.
    pub fn backend_packages(self) -> &'static [&'static str] {
        match self {
            Tier::Vulkan => &["ggml-cpu", "ggml-vulkan"],
            Tier::Cpu => &["ggml-cpu"],
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

/// Where `setup` and the Models list keep the weights on this machine.
pub fn directory() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(DIRECTORY_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    unit::models_directory()
}

/// The file name, URL, pinned digest, and pinned size one model name stands
/// for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weights {
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

/// What the catalogue knows about this model, or `None` for a name the user
/// typed into Settings themselves.
pub fn weights(model: &str) -> Option<Weights> {
    let wanted = model.to_ascii_lowercase();
    let row = CATALOGUE
        .iter()
        .find(|row| row.name.eq_ignore_ascii_case(&wanted))?;
    let base = std::env::var(BASE_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let base = base.trim_end_matches('/');
    let sha256 = std::env::var(SHA256_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| row.sha256.to_string());

    Some(Weights {
        file_name: row.file_name.to_string(),
        url: format!("{base}/{}/resolve/main/{}", row.repository, row.file_name),
        sha256,
        size_bytes: row.size_bytes,
    })
}

/// Every catalogue name, in the order the Models list draws them.
pub fn names() -> Vec<&'static str> {
    CATALOGUE.iter().map(|row| row.name).collect()
}

/// The `.gguf` and the `.part` one catalogue row owns in one directory.
///
/// The row is about its own pinned file name and nothing else, so all three
/// verbs agree on one pair of paths: what Download writes, what the state is
/// read from, and what Remove deletes.
fn paths(directory: &Path, weights: &Weights) -> (PathBuf, PathBuf) {
    let final_path = directory.join(&weights.file_name);
    let partial = directory.join(format!("{}.part", weights.file_name));
    (final_path, partial)
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
///
/// This is the `grammachy setup` path, which knows one model name and has no
/// user to cancel it. The Models list calls [`fetch`] instead, because it needs
/// the free-space check and the cancel that section 5.3 promises.
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

    let (final_path, partial) = paths(directory, &weights);
    match download(&weights.url, &partial)? {
        Transfer::Finished => {}
        Transfer::Cancelled => {
            return Err(format!(
                "The download of {model} was cancelled. {} is kept, so the next run resumes it.",
                partial.display()
            ))
        }
    }
    promote(&partial, &final_path, &weights.sha256)?;

    Ok(Outcome::Downloaded(final_path))
}

/// Rename the `.part` file only when its digest matches the pin.
fn promote(partial: &Path, final_path: &Path, expected_sha256: &str) -> Result<(), String> {
    let actual = digest::sha256_path(partial)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "The downloaded file does not match the pinned digest. Expected {expected_sha256}, got {actual}."
        ));
    }
    std::fs::rename(partial, final_path).map_err(|error| {
        format!(
            "{} could not be renamed to {}: {error}",
            partial.display(),
            final_path.display()
        )
    })
}

/// The downloader this run uses.
pub fn downloader() -> Downloader {
    Box::new(curl)
}

/// How often a running transfer is asked whether a cancel has arrived.
const POLL_MS: u64 = 100;

/// Fetch one URL into one path, resuming a `.part` file that is already there.
///
/// curl runs as a child rather than through `output()`, because a cancel has to
/// reach it: the signal handler only sets a flag, and this loop is what turns
/// that flag into a dead child and a kept `.part` file.
pub fn curl(url: &str, path: &Path) -> Result<Transfer, String> {
    let mut child = Command::new("curl")
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!("curl could not run: {error}. Install it with: sudo pacman -S curl")
        })?;

    loop {
        if cancel::requested() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Transfer::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(Transfer::Finished),
            Ok(Some(_)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(format!("curl could not fetch {url}: {}", stderr.trim()));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(POLL_MS)),
            Err(error) => return Err(format!("curl could not be waited for: {error}")),
        }
    }
}

/// The unit stopper this run uses.
pub fn stopper() -> Stopper {
    if std::env::var_os(STOP_ENV).is_some_and(|value| value == "never") {
        return Box::new(|_unit| Ok(()));
    }
    Box::new(stop_unit)
}

/// Stop one transient user unit.
///
/// A unit that is not running is the outcome this call wanted, and `systemctl`
/// says so with exit 0, so only a real failure comes back as an error.
pub fn stop_unit(unit: &str) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .arg("stop")
        .arg(unit)
        .output()
        .map_err(|error| format!("systemctl could not run: {error}"))?;

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "systemctl could not stop {unit}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Why a verb of `grammachy model` did not do what it was asked.
///
/// Each variant is one code of the error envelope in spec section 5.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// A name the catalogue does not carry, or a disk with no room for it.
    BadArguments(String),
    /// curl failed, or the finished file did not match the pinned digest.
    DownloadFailed(String),
    /// A SIGTERM arrived. The `.part` file is kept.
    Cancelled(String),
}

/// What one run of `grammachy model` works on.
pub struct Models {
    pub directory: PathBuf,
    pub download: Downloader,
    pub stop: Stopper,
}

impl Models {
    /// The run this machine gets, with the test seams applied.
    pub fn from_env() -> Result<Models, String> {
        Ok(Models {
            directory: directory()
                .ok_or_else(|| "HOME is not set, so there is no models directory.".to_string())?,
            download: downloader(),
            stop: stopper(),
        })
    }

    /// One row per catalogue entry, read from disk.
    pub fn list(&self) -> Vec<ModelRow> {
        names()
            .into_iter()
            .filter_map(|name| self.row(name))
            .collect()
    }

    /// The row one catalogue name has right now, or `None` for a name that is
    /// not in the catalogue at all.
    fn row(&self, name: &str) -> Option<ModelRow> {
        let weights = weights(name)?;
        let (final_path, partial) = paths(&self.directory, &weights);
        let partial_bytes = std::fs::metadata(&partial)
            .ok()
            .filter(|data| data.is_file())
            .map(|data| data.len());

        let state = if final_path.is_file() {
            State::Ready
        } else if partial_bytes.is_some() {
            State::Partial
        } else {
            State::Absent
        };

        Some(ModelRow {
            name: name.to_string(),
            file_name: weights.file_name,
            state,
            partial_bytes: partial_bytes.unwrap_or(0),
            size_bytes: weights.size_bytes,
            licence: licences::of(name).license.to_string(),
        })
    }

    /// Fetch one catalogue model, resuming its `.part` file.
    ///
    /// The free-space check is what keeps a download from filling the disk and
    /// then failing on the last byte: only the bytes still missing are asked
    /// for, because a resumed transfer never rewrites what is already there.
    pub fn fetch(&self, name: &str) -> Result<ModelRow, Failure> {
        let weights = weights(name).ok_or_else(|| self.unknown(name))?;
        let (final_path, partial) = paths(&self.directory, &weights);
        if final_path.is_file() {
            return self.finished_row(name);
        }

        let already = std::fs::metadata(&partial)
            .map(|data| data.len())
            .unwrap_or(0);
        if let Some(short) = disk::shortfall(
            weights.size_bytes,
            already,
            disk::free_bytes(&self.directory),
        ) {
            return Err(Failure::BadArguments(format!(
                "{name} needs {} more bytes and {} has {} free.",
                short.remaining,
                self.directory.display(),
                short.free
            )));
        }

        std::fs::create_dir_all(&self.directory).map_err(|error| {
            Failure::DownloadFailed(format!(
                "{} could not be created: {error}",
                self.directory.display()
            ))
        })?;

        match (self.download)(&weights.url, &partial).map_err(Failure::DownloadFailed)? {
            Transfer::Finished => {}
            Transfer::Cancelled => {
                return Err(Failure::Cancelled(format!(
                    "The download of {name} stopped. {} is kept, so Download resumes it.",
                    partial.display()
                )))
            }
        }

        promote(&partial, &final_path, &weights.sha256).map_err(Failure::DownloadFailed)?;
        self.finished_row(name)
    }

    /// Delete one catalogue model's `.gguf` and its `.part`.
    ///
    /// The unit is stopped first when it is running on this very file, because
    /// llama.cpp keeps the weights open and a Check that starts again would
    /// otherwise read a file that is no longer there. The setting is left
    /// alone: which model the engine asks for is the user's choice, and this
    /// verb only says what is on disk.
    pub fn delete(&self, name: &str, in_use: bool) -> Result<ModelRow, Failure> {
        let weights = weights(name).ok_or_else(|| self.unknown(name))?;
        let (final_path, partial) = paths(&self.directory, &weights);

        if in_use && final_path.is_file() {
            (self.stop)(unit::UNIT_NAME).map_err(Failure::BadArguments)?;
        }

        for path in [&final_path, &partial] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Failure::BadArguments(format!(
                        "{} could not be deleted: {error}",
                        path.display()
                    )))
                }
            }
        }

        self.finished_row(name)
    }

    /// Whether the weights the Settings ask for are the ones this row holds.
    ///
    /// `unit::model_file` is what the engine itself resolves the setting with,
    /// so asking it is what makes the answer the same one a Check would get.
    pub fn is_in_use(&self, name: &str, setting: &str) -> bool {
        let Some(weights) = weights(name) else {
            return false;
        };
        unit::model_file(&self.directory, setting)
            .is_ok_and(|path| path == self.directory.join(&weights.file_name))
    }

    fn finished_row(&self, name: &str) -> Result<ModelRow, Failure> {
        self.row(name).ok_or_else(|| self.unknown(name))
    }

    fn unknown(&self, name: &str) -> Failure {
        Failure::BadArguments(format!(
            "{name} is not one of the models Grammachy can fetch: {}.",
            names().join(", ")
        ))
    }
}

/// One verb of `grammachy model`, as one envelope.
pub fn run(verb: &ModelVerb, openai_model: &str) -> ModelEnvelope {
    let models = match Models::from_env() {
        Ok(models) => models,
        Err(message) => return ModelEnvelope::bad_arguments(message),
    };

    match verb {
        ModelVerb::List => models.list_envelope(),
        ModelVerb::Download(ModelNameArgs { name }) => {
            // Only a download can be cancelled, so only a download listens.
            cancel::listen();
            match models.fetch(name) {
                Ok(row) => models.report("download", vec![row]),
                Err(failure) => ModelEnvelope::failure(failure),
            }
        }
        ModelVerb::Remove(ModelNameArgs { name }) => {
            let in_use = models.is_in_use(name, openai_model);
            match models.delete(name, in_use) {
                Ok(row) => models.report("remove", vec![row]),
                Err(failure) => ModelEnvelope::failure(failure),
            }
        }
    }
}

impl Models {
    /// The whole Models list as one envelope, which is what `model list` prints.
    pub fn list_envelope(&self) -> ModelEnvelope {
        self.report("list", self.list())
    }

    /// Every verb answers the same shape, so one answer refreshes the list
    /// however the shell got it.
    fn report(&self, verb: &'static str, models: Vec<ModelRow>) -> ModelEnvelope {
        ModelEnvelope::report(ModelReport {
            contract_version: crate::envelope::CONTRACT_VERSION,
            verb,
            directory: self.directory.display().to_string(),
            free_bytes: disk::free_bytes(&self.directory).unwrap_or(0),
            models,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_without_a_render_node_is_the_cpu_tier() {
        let empty = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");

        assert_eq!(tier_of(&empty), Tier::Cpu);
        assert_eq!(Tier::Cpu.backend_packages(), ["ggml-cpu"]);
        assert_eq!(Tier::Vulkan.backend_packages(), ["ggml-cpu", "ggml-vulkan"]);
    }

    #[test]
    fn the_recommended_model_has_a_download() {
        let weights = weights("gemma-4-e4b-it").expect("the default model is in the catalogue");

        assert_eq!(weights.file_name, "gemma-4-E4B-it-Q4_K_M.gguf");
        assert_eq!(
            weights.url,
            "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf"
        );
        assert_eq!(
            weights.sha256,
            "85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87"
        );
        assert_eq!(weights.size_bytes, 4_977_171_584);
    }

    /// Spec section 13.1: a model may only be recommended on a permissive
    /// licence, so the two rows added beside the default carry one.
    #[test]
    fn every_catalogue_row_is_pinned_twice_and_names_its_licence() {
        assert_eq!(
            names(),
            ["gemma-4-e4b-it", "qwen3-4b-instruct", "phi-4-mini-instruct"]
        );
        for row in CATALOGUE {
            assert_eq!(row.sha256.len(), 64, "{} is pinned by digest", row.name);
            assert!(row.size_bytes > 0, "{} is pinned by size", row.name);
            assert!(
                row.file_name.ends_with(".gguf"),
                "{} names a weights file",
                row.name
            );
            assert_ne!(
                licences::of(row.name).license,
                "unknown",
                "{} has a checked licence",
                row.name
            );
        }
        assert_eq!(licences::of("qwen3-4b-instruct").license, "Apache-2.0");
        assert_eq!(licences::of("phi-4-mini-instruct").license, "MIT");
    }

    #[test]
    fn an_unknown_model_has_none() {
        assert!(weights("something-the-user-typed").is_none());
    }

    #[test]
    fn ensure_rejects_a_corrupt_download() {
        let directory = scratch("ensure-mismatch");
        const BYTES: &[u8] = b"not the model";
        let download: Downloader = Box::new(|_url, path| {
            std::fs::write(path, BYTES)
                .map(|()| Transfer::Finished)
                .map_err(|error| error.to_string())
        });

        let error = ensure("gemma-4-e4b-it", &directory, &download)
            .expect_err("the digest differs from the pin");

        assert!(
            error.contains("85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87"),
            "{error}"
        );
        assert!(error.contains(&sha256_hex(BYTES)), "{error}");
        assert!(!directory.join("gemma-4-E4B-it-Q4_K_M.gguf").exists());
        assert!(directory.join("gemma-4-E4B-it-Q4_K_M.gguf.part").is_file());
    }

    #[test]
    fn a_matching_digest_renames_the_part_file() {
        let directory = scratch("digest-match");
        let bytes = b"small fake weights";
        let partial = directory.join("model.gguf.part");
        let final_path = directory.join("model.gguf");
        std::fs::write(&partial, bytes).expect("the part file is written");

        promote(&partial, &final_path, &sha256_hex(bytes)).expect("the digest matches");

        assert_eq!(std::fs::read(&final_path).unwrap(), bytes);
        assert!(!partial.exists());
    }

    #[test]
    fn a_mismatched_digest_leaves_the_part_file() {
        let directory = scratch("digest-mismatch");
        let bytes = b"small fake weights";
        let partial = directory.join("model.gguf.part");
        let final_path = directory.join("model.gguf");
        std::fs::write(&partial, bytes).expect("the part file is written");
        let expected = sha256_hex(b"other bytes");
        let actual = sha256_hex(bytes);

        let error = promote(&partial, &final_path, &expected).expect_err("the digest differs");

        assert!(error.contains(&expected), "{error}");
        assert!(error.contains(&actual), "{error}");
        assert!(!final_path.exists());
        assert_eq!(std::fs::read(&partial).unwrap(), bytes);
    }

    fn scratch(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("grammachy-model-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the scratch directory is created");
        directory
    }
}
