//! The gitignored corpus cache `bench --eval-set` fills at run time.
//!
//! ADR 0003 is the licence stance: the CLC FCE dataset is licensed for
//! non-commercial research, so the repository redistributes none of it. The
//! tarball is fetched into a cache that git ignores, pinned by sha256 the way
//! `setup/model.rs` pins the weights, and the first fill prints the licence
//! path and its non-commercial line to stderr.
//!
//! A fill that cannot happen is never an error. The caller turns the message
//! into one skipped table with a reason, so a clean clone with no network
//! still produces a whole benchmark file.
//!
//! Every path and every side effect is a seam, so no test reaches
//! `cl.cam.ac.uk` or writes the developer's own cache.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::digest::sha256_path;
use crate::model::{self, Downloader, Transfer};

/// Points the CLI at another corpus cache. The test suite sets it, so no test
/// fills the real one. Not a user-facing setting.
pub const DIRECTORY_ENV: &str = "GRAMMACHY_EVAL_CACHE";

/// Points the CLI at another corpus host, which is how the fetch is tested
/// against a stub server. Not a user-facing setting.
pub const BASE_URL_ENV: &str = "GRAMMACHY_EVAL_BASE_URL";

/// Points the CLI at another expected digest for a small fake tarball.
/// Not a user-facing setting.
pub const SHA256_ENV: &str = "GRAMMACHY_EVAL_SHA256";

/// Set to `never` to forbid the fetch, so a run uses whatever the cache
/// already holds. Every test sets it unless the fetch itself is the subject.
pub const FETCH_ENV: &str = "GRAMMACHY_EVAL_FETCH";

/// Where the BEA-2019 release of the CLC FCE dataset comes from.
const DEFAULT_BASE_URL: &str = "https://www.cl.cam.ac.uk/research/nl/bea2019st/data";

/// The release this eval set is drawn from, spec `docs/spec/evals.md` section 2.
pub const RELEASE: &str = "fce_v2.1.bea19";

/// The tarball of that release.
const TARBALL: &str = "fce_v2.1.bea19.tar.gz";

/// The digest of that tarball, 2,774,021 bytes, measured once by hand.
const TARBALL_SHA256: &str = "c574c1cdba6d3ab5a87280f180133cdcf0609848f5dc87cfa2c3f4b0c07ec67e";

/// The directory the tarball unpacks into, inside the cache.
const ROOT: &str = "fce";

/// The licence file of the release, the path the first fill names on stderr.
const LICENCE_FILE: &str = "licence.txt";

/// The line of that licence the first fill quotes.
///
/// It is the clause that makes the dataset non-commercial, and it is well
/// under the licence's own 100-word excerpt cap.
const NON_COMMERCIAL_LINE: &str = "The Licensor hereby grants the Licensee a non-exclusive non-transferable right to use the licensed dataset for non-commercial research and educational purposes.";

/// The attribution line the benchmark file header carries.
pub const ATTRIBUTION: &str =
    "Eval set: CLC FCE (BEA-2019 v2.1), CLC FCE Dataset Licence, fetched at run time, not redistributed.";

/// The unpacked release on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    /// The `fce` directory the tarball unpacks into.
    root: PathBuf,
}

impl Cache {
    /// The M2 file of one split, the alignment the items are built from.
    pub fn m2(&self, split: &str) -> PathBuf {
        self.root
            .join("m2")
            .join(format!("fce.{split}.gold.bea19.m2"))
    }

    /// The JSON file of one split, which carries the writer's first language.
    pub fn json(&self, split: &str) -> PathBuf {
        self.root.join("json").join(format!("fce.{split}.json"))
    }

    /// The licence of the release, named on the first fill.
    pub fn licence(&self) -> PathBuf {
        self.root.join(LICENCE_FILE)
    }
}

/// Where the cache lives on this machine.
///
/// The default sits beside the crate, because the eval set is a developer and
/// release command rather than a shell surface, and because a path inside the
/// repository is the one git can be told to ignore.
pub fn directory() -> PathBuf {
    if let Some(value) = std::env::var_os(DIRECTORY_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".eval-cache")
}

/// Whether this run may fetch the tarball.
pub fn may_fetch() -> bool {
    std::env::var(FETCH_ENV).ok().as_deref() != Some("never")
}

/// The URL and digest this run expects of the tarball.
fn source() -> (String, String) {
    let base = std::env::var(BASE_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let sha256 = std::env::var(SHA256_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| TARBALL_SHA256.to_string());
    (format!("{}/{TARBALL}", base.trim_end_matches('/')), sha256)
}

/// Put the release in place, or say why this machine has none.
///
/// The message is the one sentence the skipped table carries, so it names what
/// the reader of the benchmark file would have to do.
pub fn ensure() -> Result<Cache, String> {
    ensure_in(&directory(), &model::downloader(), may_fetch())
}

/// The same step against one cache directory and one downloader.
///
/// `may_fetch` is the seam rather than the variable, so a case proves the
/// forbidden path without writing the environment of the whole test binary.
pub fn ensure_in(
    directory: &Path,
    download: &Downloader,
    may_fetch: bool,
) -> Result<Cache, String> {
    let cache = Cache {
        root: directory.join(ROOT),
    };
    if cache.m2("train").is_file() && cache.json("train").is_file() {
        return Ok(cache);
    }
    if !may_fetch {
        return Err(format!(
            "the {RELEASE} cache under {} is empty and {FETCH_ENV} forbids the fetch",
            directory.display()
        ));
    }

    fill(directory, download)?;
    if !cache.m2("train").is_file() {
        return Err(format!(
            "the {RELEASE} tarball unpacked without {}",
            cache.m2("train").display()
        ));
    }
    notice(&cache);
    Ok(cache)
}

/// Fetch the tarball, check its digest, and unpack it into the cache.
fn fill(directory: &Path, download: &Downloader) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("{} could not be created: {error}", directory.display()))?;

    let (url, sha256) = source();
    let partial = directory.join(format!("{TARBALL}.part"));
    let tarball = directory.join(TARBALL);
    match download(&url, &partial)
        .map_err(|error| format!("the {RELEASE} tarball is not here: {error}"))?
    {
        Transfer::Finished => {}
        // The `.part` file stays, so the next run carries on where this one
        // stopped. Nothing is unpacked from half a tarball.
        Transfer::Cancelled => {
            return Err(format!("the {RELEASE} download was cancelled"));
        }
    }

    let actual = sha256_path(&partial)?;
    if !actual.eq_ignore_ascii_case(&sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!(
            "the {RELEASE} tarball does not match the pinned digest. Expected {sha256}, got {actual}."
        ));
    }
    std::fs::rename(&partial, &tarball).map_err(|error| {
        format!(
            "{} could not be renamed to {}: {error}",
            partial.display(),
            tarball.display()
        )
    })?;

    unpack(directory, &tarball)
}

/// Unpack the verified tarball with `tar`, the tool every Omarchy machine has.
fn unpack(directory: &Path, tarball: &Path) -> Result<(), String> {
    let output = Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(directory)
        .output()
        .map_err(|error| {
            format!("tar could not run: {error}. Install it with: sudo pacman -S tar")
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "tar could not unpack {}: {}",
        tarball.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Print the licence path and its non-commercial line, on the first fill only.
///
/// The fill happens once per cache, so this runs once per machine. A later run
/// reads the cache and prints nothing.
fn notice(cache: &Cache) {
    eprintln!(
        "grammachy bench: fetched {RELEASE} into {}.",
        cache.licence().display()
    );
    eprintln!("grammachy bench: {NON_COMMERCIAL_LINE}");
    eprintln!("grammachy bench: {ATTRIBUTION}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The non-commercial line must be the licence's own wording, because the
    /// notice is what puts the reader on the licence.
    #[test]
    fn the_notice_quotes_the_non_commercial_clause() {
        assert!(NON_COMMERCIAL_LINE.contains("non-commercial research"));
        assert!(NON_COMMERCIAL_LINE.split_whitespace().count() < 100);
    }

    #[test]
    fn the_attribution_line_is_the_one_the_spec_fixes() {
        assert_eq!(
            ATTRIBUTION,
            "Eval set: CLC FCE (BEA-2019 v2.1), CLC FCE Dataset Licence, fetched at run time, not redistributed."
        );
    }

    #[test]
    fn an_empty_cache_with_the_fetch_forbidden_is_a_reason_rather_than_a_panic() {
        let directory = std::env::temp_dir().join("grammachy-eval-empty");
        let _ = std::fs::remove_dir_all(&directory);
        let downloader: Downloader = Box::new(|_, _| panic!("a forbidden fetch never downloads"));

        let error = ensure_in(&directory, &downloader, false).unwrap_err();

        assert!(error.contains("forbids the fetch"), "{error}");
    }
}
