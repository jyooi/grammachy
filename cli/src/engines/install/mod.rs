//! `grammachy engine`, spec section 5.3.
//!
//! The optional engine components this machine keeps, and the three things a
//! user does with them: see what is on disk, put one in place, and take it off
//! again. `transfer`, `digest`, `disk`, and `cancel` are its own generic
//! download machinery (HUF-240 retired the `grammachy model` command that used
//! to share it), because an install is a download with one more step.
//!
//! Only `languagetool` is a component; `harper` is compiled into the binary
//! and has nothing to install (HUF-237).
//!
//! LanguageTool lands in `~/.local/share/grammachy/engines/languagetool/`, the
//! upstream release tree unpacked as it comes. That is user space, so the
//! whole feature needs no password: `install` writes a directory and `remove`
//! deletes one. The pacman package stays a first-class alternative and is
//! never touched by either verb; `doctor` reports it where it finds it.
//!
//! The row is pinned twice, by sha256 and by byte size.
//! Both numbers come from an unauthenticated request to the upstream host,
//! and the digest is the one the Arch `languagetool` package pins for the same
//! file.
//!
//! Every path and every side effect is a seam: `GRAMMACHY_ENGINES_DIR`,
//! `GRAMMACHY_ENGINE_BASE_URL`, `GRAMMACHY_ENGINE_SHA256`,
//! `GRAMMACHY_ENGINE_SIZE_BYTES`, plus the [`Downloader`], [`Extractor`], and
//! [`Stopper`] values. No test reaches the upstream host, the real engines
//! directory, or a real unit.

pub mod archive;
pub mod cancel;
pub mod envelope;

mod digest;
mod disk;
mod transfer;

use std::path::{Path, PathBuf};

use crate::args::{EngineNameArgs, EngineVerb};
use crate::engines::languagetool;

pub use archive::{extractor, Extractor};
pub use digest::sha256_hex;
pub use envelope::{EngineEnvelope, EngineReport, EngineRow, State};
pub use transfer::{Downloader, Failure, Stopper, Transfer, NOT_LOADED};

/// Points the CLI at another engines directory. The test suite sets it, so no
/// test writes the real one. Not a user-facing setting.
pub const DIRECTORY_ENV: &str = "GRAMMACHY_ENGINES_DIR";

/// Points the CLI at another release host, which is how the install is tested
/// against a stub server. Not a user-facing setting.
pub const BASE_URL_ENV: &str = "GRAMMACHY_ENGINE_BASE_URL";

/// Points the CLI at another expected digest for a small fake archive.
/// Not a user-facing setting.
pub const SHA256_ENV: &str = "GRAMMACHY_ENGINE_SHA256";

/// Points the CLI at another pinned size for a small fake archive.
///
/// The free-space check runs against the pinned sizes before the transfer does
/// anything, so without this seam every test of the install would need the
/// hundreds of megabytes a real row asks for. Not a user-facing setting.
pub const SIZE_ENV: &str = "GRAMMACHY_ENGINE_SIZE_BYTES";

/// Where the upstream release comes from.
const DEFAULT_BASE_URL: &str = "https://languagetool.org";

/// One optional component the Settings view offers.
///
/// `slug` is the engine slug, so a row and the `engine` setting are the same
/// word and the Settings dropdown can ask this list whether to draw a row at
/// all.
///
/// `directory_name` is what the archive unpacks into, and `entry` is the file
/// under the installed tree that proves the unpack finished. A directory alone
/// proves nothing: a `bsdtar` that died half way leaves one behind.
struct CatalogueRow {
    slug: &'static str,
    name: &'static str,
    version: &'static str,
    /// The path under the base URL, without a leading slash.
    path: &'static str,
    archive_name: &'static str,
    sha256: &'static str,
    size_bytes: u64,
    /// What the unpacked tree takes, so the free-space check measures the peak
    /// of the install rather than the archive alone.
    installed_bytes: u64,
    licence: &'static str,
    directory_name: &'static str,
    entry: &'static str,
    /// The transient user unit this component's server runs in, so `remove`
    /// stops the right one without a second table to keep in step.
    unit: &'static str,
    needs_java: bool,
}

/// The `languagetool` row, pinned from an unauthenticated request to
/// `languagetool.org` on 2026-08-27.
///
/// The digest is the one the Arch `languagetool` 6.6-2 package pins for the
/// same file, so two independent parties name the same bytes. `installed_bytes`
/// is that package's installed size, which is the unpacked tree: the archive is
/// jars, which barely compress, so the tree is not much larger than the zip.
const CATALOGUE: &[CatalogueRow] = &[CatalogueRow {
    slug: "languagetool",
    name: "LanguageTool",
    version: "6.6",
    path: "download/LanguageTool-6.6.zip",
    archive_name: "LanguageTool-6.6.zip",
    sha256: "53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631",
    size_bytes: 251_998_221,
    installed_bytes: 405_074_394,
    licence: "LGPL-2.1-or-later",
    directory_name: "LanguageTool-6.6",
    entry: "languagetool-server.jar",
    unit: languagetool::unit::UNIT_NAME,
    needs_java: true,
}];

/// The archive URL, pinned digest, and pinned sizes one slug stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub archive_name: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub installed_bytes: u64,
    /// The directory the archive unpacks into, inside the staging directory.
    pub directory_name: String,
}

/// What the catalogue knows about this slug, or `None` for an engine that has
/// nothing to install.
pub fn release(slug: &str) -> Option<Release> {
    let row = row_of(slug)?;
    let base = std::env::var(BASE_URL_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let base = base.trim_end_matches('/');
    let sha256 = std::env::var(SHA256_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| row.sha256.to_string());
    let size_bytes = std::env::var(SIZE_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(row.size_bytes);
    // One seam moves both numbers, because a test that shrinks the archive to
    // a handful of bytes wants the whole free-space question shrunk with it.
    let installed_bytes = match std::env::var(SIZE_ENV) {
        Ok(value) if value.trim().parse::<u64>().is_ok() => size_bytes,
        _ => row.installed_bytes,
    };

    Some(Release {
        archive_name: row.archive_name.to_string(),
        url: format!("{base}/{}", row.path),
        sha256,
        size_bytes,
        installed_bytes,
        directory_name: row.directory_name.to_string(),
    })
}

fn row_of(slug: &str) -> Option<&'static CatalogueRow> {
    let wanted = slug.trim();
    CATALOGUE
        .iter()
        .find(|row| row.slug.eq_ignore_ascii_case(wanted))
}

/// Every slug this subcommand knows, in the order the Settings view draws them.
pub fn slugs() -> Vec<&'static str> {
    CATALOGUE.iter().map(|row| row.slug).collect()
}

/// Whether one engine slug is a component that can be installed and removed.
///
/// The Settings dropdown asks this before it asks [`installed`]: an engine with
/// nothing to install is always offered.
pub fn is_component(slug: &str) -> bool {
    row_of(slug).is_some()
}

/// Where the components live on this machine.
///
/// The product path is the HOME one: the shell stores its settings under HOME
/// (spec section 7), so `XDG_DATA_HOME` is not read.
pub fn directory() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os(DIRECTORY_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".local/share/grammachy/engines"))
}

/// The installed tree of one slug on this machine, or `None` when this verb has
/// not put one there.
///
/// This is the one reader the engine adapters and `doctor` share, so all three
/// agree on what "installed" means: the entry file of the row is a file. A
/// directory left by a `bsdtar` that died half way answers `None`, which is
/// what makes the next Install start over rather than run a broken tree.
pub fn installed(slug: &str) -> Option<PathBuf> {
    let row = row_of(slug)?;
    let path = directory()?.join(row.slug);
    path.join(row.entry).is_file().then_some(path)
}

/// The three paths one row owns in one directory: the installed tree, the
/// archive being fetched, and the directory the archive unpacks into.
///
/// All three verbs agree on them, so what Install writes, what the state is
/// read from, and what Remove deletes can never drift apart.
fn paths(directory: &Path, slug: &str, release: &Release) -> Paths {
    Paths {
        tree: directory.join(slug),
        partial: directory.join(format!("{}.part", release.archive_name)),
        archive: directory.join(&release.archive_name),
        staging: directory.join(format!("{slug}.unpack")),
    }
}

struct Paths {
    tree: PathBuf,
    partial: PathBuf,
    archive: PathBuf,
    staging: PathBuf,
}

/// What one run of `grammachy engine` works on.
pub struct Engines {
    pub directory: PathBuf,
    pub download: Downloader,
    pub extract: Extractor,
    pub stop: Stopper,
}

impl Engines {
    /// The run this machine gets, with the test seams applied.
    pub fn from_env() -> Result<Engines, String> {
        Ok(Engines {
            directory: directory()
                .ok_or_else(|| "HOME is not set, so there is no engines directory.".to_string())?,
            download: transfer::downloader(),
            extract: extractor(),
            stop: transfer::stopper(),
        })
    }

    /// One row per catalogue entry, read from disk.
    pub fn list(&self) -> Vec<EngineRow> {
        slugs()
            .into_iter()
            .filter_map(|slug| self.row(slug))
            .collect()
    }

    /// The row one slug has right now, or `None` for a slug that is not a
    /// component at all.
    fn row(&self, slug: &str) -> Option<EngineRow> {
        let row = row_of(slug)?;
        let release = release(slug)?;
        let paths = paths(&self.directory, row.slug, &release);
        let partial_bytes = std::fs::metadata(&paths.partial)
            .ok()
            .filter(|data| data.is_file())
            .map(|data| data.len());
        let here = paths.tree.join(row.entry).is_file();
        let from_package = package_launcher(row.slug).is_some();

        let state = if here {
            State::Ready
        } else if partial_bytes.is_some() {
            State::Partial
        } else {
            State::Absent
        };

        // The installed tree first, because it is the one this verb put there
        // and the one `remove` acts on. A row the pacman package supplies and
        // this verb did not names that launcher instead, so the Settings view
        // can say the component is reachable without offering to remove it.
        let path = if here {
            paths.tree.display().to_string()
        } else {
            package_launcher(row.slug)
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        };

        Some(EngineRow {
            slug: row.slug.to_string(),
            name: row.name.to_string(),
            version: row.version.to_string(),
            state,
            // Spec section 5.3: the length of the `.part` file, and `0` for
            // any other state.
            partial_bytes: match state {
                State::Partial => partial_bytes.unwrap_or(0),
                _ => 0,
            },
            size_bytes: release.size_bytes,
            licence: row.licence.to_string(),
            needs_java: row.needs_java,
            path,
            from_package: from_package && !here,
        })
    }

    /// Fetch and unpack one component, resuming its `.part` file.
    ///
    /// The free-space check asks for the archive and the tree together,
    /// because both are on the disk at once: the archive is deleted only after
    /// the unpack has finished.
    pub fn install(&self, slug: &str) -> Result<EngineRow, Failure> {
        let row = row_of(slug).ok_or_else(|| self.unknown(slug))?;
        let release = release(slug).ok_or_else(|| self.unknown(slug))?;
        let paths = paths(&self.directory, row.slug, &release);
        if paths.tree.join(row.entry).is_file() {
            return self.finished_row(slug);
        }

        let already = std::fs::metadata(&paths.partial)
            .map(|data| data.len())
            .unwrap_or(0);
        if let Some(short) = disk::shortfall(
            release.size_bytes.saturating_add(release.installed_bytes),
            already,
            disk::free_bytes(&self.directory),
        ) {
            return Err(Failure::BadArguments(format!(
                "{} needs {} more bytes and {} has {} free.",
                row.name,
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

        match (self.download)(&release.url, &paths.partial).map_err(Failure::DownloadFailed)? {
            Transfer::Finished => {}
            Transfer::Cancelled => {
                return Err(Failure::Cancelled(format!(
                    "The download of {} stopped. {} is kept, so Install resumes it.",
                    row.name,
                    paths.partial.display()
                )))
            }
        }

        transfer::promote(&paths.partial, &paths.archive, &release.sha256)
            .map_err(Failure::DownloadFailed)?;
        self.unpack(row, &release, &paths)?;
        self.finished_row(slug)
    }

    /// Unpack the verified archive and rename the tree into place.
    ///
    /// The unpack goes into a staging directory of its own and is renamed only
    /// once the entry file is there, so a `bsdtar` that dies half way never
    /// leaves something the state reader would call installed. The archive
    /// goes last: keeping it would double what the component costs on disk for
    /// no gain, because a re-install re-checks the digest anyway.
    fn unpack(&self, row: &CatalogueRow, release: &Release, paths: &Paths) -> Result<(), Failure> {
        let failed = |message: String| Failure::DownloadFailed(message);

        let _ = std::fs::remove_dir_all(&paths.staging);
        std::fs::create_dir_all(&paths.staging).map_err(|error| {
            failed(format!(
                "{} could not be created: {error}",
                paths.staging.display()
            ))
        })?;

        (self.extract)(&paths.archive, &paths.staging).map_err(failed)?;

        let unpacked = paths.staging.join(&release.directory_name);
        if !unpacked.join(row.entry).is_file() {
            let _ = std::fs::remove_dir_all(&paths.staging);
            return Err(failed(format!(
                "{} does not hold {}/{}, so the archive is not the {} release this row pins.",
                paths.archive.display(),
                release.directory_name,
                row.entry,
                row.version
            )));
        }

        // A tree from an earlier install has to go first: a rename onto a
        // directory that is there fails, and the archive digest already said
        // these are the right bytes.
        let _ = std::fs::remove_dir_all(&paths.tree);
        std::fs::rename(&unpacked, &paths.tree).map_err(|error| {
            failed(format!(
                "{} could not be renamed to {}: {error}",
                unpacked.display(),
                paths.tree.display()
            ))
        })?;

        let _ = std::fs::remove_dir_all(&paths.staging);
        let _ = std::fs::remove_file(&paths.archive);
        Ok(())
    }

    /// Delete one component's installed tree, its archive, and its `.part`.
    ///
    /// The unit is stopped first when a tree is actually going, because the
    /// server holds its jars open and would serve a tree that is no longer
    /// there until the session ends. A unit that was not running holds nothing
    /// open, so that stop reports the outcome this verb wanted and is passed.
    ///
    /// The pacman package is never touched: this verb owns one directory under
    /// HOME and nothing else on the machine.
    pub fn remove(&self, slug: &str) -> Result<EngineRow, Failure> {
        let row = row_of(slug).ok_or_else(|| self.unknown(slug))?;
        let release = release(slug).ok_or_else(|| self.unknown(slug))?;
        let paths = paths(&self.directory, row.slug, &release);

        if paths.tree.exists() {
            if let Err(why) = (self.stop)(row.unit) {
                if !transfer::stop_found_nothing_to_stop(&why) {
                    return Err(Failure::BadArguments(why));
                }
            }
        }

        for path in [&paths.tree, &paths.staging] {
            match std::fs::remove_dir_all(path) {
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
        for path in [&paths.archive, &paths.partial] {
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

        self.finished_row(slug)
    }

    fn finished_row(&self, slug: &str) -> Result<EngineRow, Failure> {
        self.row(slug).ok_or_else(|| self.unknown(slug))
    }

    fn unknown(&self, slug: &str) -> Failure {
        Failure::BadArguments(format!(
            "{slug} is not one of the engines Grammachy can install: {}.",
            slugs().join(", ")
        ))
    }

    /// The whole list as one envelope, which is what `engine list` prints.
    pub fn list_envelope(&self) -> EngineEnvelope {
        self.report("list", self.list())
    }

    /// Every verb answers the same shape, so one answer refreshes the list
    /// however the shell got it.
    fn report(&self, verb: &'static str, engines: Vec<EngineRow>) -> EngineEnvelope {
        EngineEnvelope::report(EngineReport {
            contract_version: crate::envelope::CONTRACT_VERSION,
            verb,
            directory: self.directory.display().to_string(),
            free_bytes: disk::free_bytes(&self.directory).unwrap_or(0),
            engines,
        })
    }
}

/// The launcher the pacman package installs for one slug, when it is there.
///
/// The package is an alternative this verb never installs and never removes.
/// `doctor` and the Settings row both say where a component came from, so a
/// user who already ran `sudo pacman -S languagetool` is never asked to
/// download 250 MB of the same thing.
pub fn package_launcher(slug: &str) -> Option<PathBuf> {
    match slug {
        "languagetool" => {
            let path = PathBuf::from(languagetool::unit::PACKAGE_LAUNCHER);
            path.is_file().then_some(path)
        }
        _ => None,
    }
}

/// One verb of `grammachy engine`, as one envelope.
pub fn run(verb: &EngineVerb) -> EngineEnvelope {
    let engines = match Engines::from_env() {
        Ok(engines) => engines,
        Err(message) => return EngineEnvelope::bad_arguments(message),
    };

    match verb {
        EngineVerb::List => engines.list_envelope(),
        EngineVerb::Install(EngineNameArgs { slug }) => {
            // Only a transfer can be cancelled, so only a transfer listens.
            cancel::listen();
            match engines.install(slug) {
                Ok(row) => engines.report("install", vec![row]),
                Err(failure) => EngineEnvelope::failure(failure),
            }
        }
        EngineVerb::Remove(EngineNameArgs { slug }) => match engines.remove(slug) {
            Ok(row) => engines.report("remove", vec![row]),
            Err(failure) => EngineEnvelope::failure(failure),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one component of HUF-237, pinned twice from an unauthenticated
    /// request the way a weights row is.
    #[test]
    fn the_languagetool_row_is_pinned_twice_and_names_its_licence() {
        assert_eq!(slugs(), ["languagetool"]);

        let release = release("languagetool").expect("the row is in the catalogue");
        assert_eq!(
            release.url,
            "https://languagetool.org/download/LanguageTool-6.6.zip"
        );
        assert_eq!(
            release.sha256,
            "53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631"
        );
        assert_eq!(release.sha256.len(), 64);
        assert_eq!(release.size_bytes, 251_998_221);
        assert_eq!(release.installed_bytes, 405_074_394);
        assert_eq!(release.directory_name, "LanguageTool-6.6");

        for row in CATALOGUE {
            assert!(!row.licence.is_empty(), "{} names its licence", row.slug);
            assert!(
                row.path.ends_with(&format!("/{}", row.archive_name)),
                "{} fetches the archive it names",
                row.slug
            );
        }
    }

    /// The dropdown of spec section 7 asks this before it asks the disk, so an
    /// engine with nothing to install must never look like one that is absent.
    #[test]
    fn only_languagetool_is_a_component() {
        assert!(is_component("languagetool"));
        assert!(is_component("LanguageTool"), "the slug is matched by case");
        assert!(!is_component("harper"));
        assert!(!is_component("gector"));
    }

    #[test]
    fn an_engine_with_nothing_to_install_has_no_release() {
        assert!(release("harper").is_none());
    }
}
