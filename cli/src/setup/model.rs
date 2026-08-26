//! The weights `grammachy setup` downloads for the `openai` engine.
//!
//! Spec section 10: the file lands in
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
//! binary (spec section 10). `curl` resumes an interrupted transfer of a
//! multi-gigabyte file and retries a failed one, which the HTTP client of the
//! CLI does not. The download runs into a `.part` file.
//! The file is renamed only when it is whole and the pinned sha256 matches.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engines::openai::unit;

/// Points the CLI at another models directory. The test suite sets it, so no
/// test writes the real one. Not a user-facing setting.
pub const DIRECTORY_ENV: &str = "GRAMMACHY_MODELS_DIR";

/// Points the CLI at another weights host, which is how the download is tested
/// against a stub server. Not a user-facing setting.
pub const BASE_URL_ENV: &str = "GRAMMACHY_MODEL_BASE_URL";

/// Points the CLI at another expected digest for a small fake file.
/// Not a user-facing setting.
pub const SHA256_ENV: &str = "GRAMMACHY_MODEL_SHA256";

/// Where the recommended weights come from.
const DEFAULT_BASE_URL: &str = "https://huggingface.co";

/// One model the Settings dropdown recommends.
/// Spec section 7 is the default. Spec section 13.1 re-decides it on every tag.
struct CatalogueRow {
    name: &'static str,
    repository: &'static str,
    file_name: &'static str,
    sha256: &'static str,
}

/// The unsloth GGUF answers unauthenticated HTTP 200.
/// The google path of the same file name answers 401.
/// The digest is the Hugging Face LFS oid. The file is 4977171584 bytes.
const CATALOGUE: &[CatalogueRow] = &[CatalogueRow {
    name: "gemma-4-e4b-it",
    repository: "unsloth/gemma-4-E4B-it-GGUF",
    file_name: "gemma-4-E4B-it-Q4_K_M.gguf",
    sha256: "85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87",
}];

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

/// The file name, URL, and pinned digest one model name stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weights {
    pub file_name: String,
    pub url: String,
    pub sha256: String,
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
    promote(&partial, &final_path, &weights.sha256)?;

    Ok(Outcome::Downloaded(final_path))
}

/// Rename the `.part` file only when its digest matches the pin.
fn promote(partial: &Path, final_path: &Path, expected_sha256: &str) -> Result<(), String> {
    let actual = sha256_path(partial)?;
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

/// Hex sha256 of these bytes.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex_encode(&hasher.finalize())
}

fn sha256_path(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    filled: usize,
    bit_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            filled: 0,
            bit_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.filled] = byte;
            self.filled += 1;
            if self.filled == 64 {
                self.compress();
                self.bit_len += 512;
                self.filled = 0;
            }
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.bit_len + (self.filled as u64) * 8;
        self.buffer[self.filled] = 0x80;
        self.filled += 1;
        if self.filled > 56 {
            self.buffer[self.filled..].fill(0);
            self.compress();
            self.buffer.fill(0);
            self.filled = 0;
        }
        self.buffer[self.filled..56].fill(0);
        self.buffer[56..].copy_from_slice(&bit_len.to_be_bytes());
        self.compress();

        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..][..4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let start = index * 4;
            *word = u32::from_be_bytes([
                self.buffer[start],
                self.buffer[start + 1],
                self.buffer[start + 2],
                self.buffer[start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
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
        assert_eq!(
            weights.url,
            "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf"
        );
        assert_eq!(
            weights.sha256,
            "85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87"
        );
    }

    #[test]
    fn an_unknown_model_has_none() {
        assert!(weights("something-the-user-typed").is_none());
    }

    #[test]
    fn sha256_matches_the_empty_and_abc_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }

    #[test]
    fn ensure_rejects_a_corrupt_download() {
        let directory = scratch("ensure-mismatch");
        const BYTES: &[u8] = b"not the model";
        let download: Downloader =
            Box::new(|_url, path| std::fs::write(path, BYTES).map_err(|error| error.to_string()));

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
