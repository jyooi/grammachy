//! `grammachy setup` and `grammachy setup --remove`, spec section 10.
//!
//! Every case works on copies of the two configuration files in a temporary
//! directory under `CARGO_TARGET_TMPDIR`, never on the real ones. No case
//! reloads a compositor and no case reaches the network: the reload and the
//! download are values the test hands in.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use grammachy::args::EngineSlug;
use grammachy::model::{self, Transfer};
use grammachy::setup::{Setup, SetupReport};
use grammachy::setup::{SetupEnvelope, State, Step};

/// The files a fresh Omarchy install carries, as this repository keeps them.
const BINDINGS_FIXTURE: &str = include_str!("fixtures/config/bindings.lua");
const MENU_FIXTURE: &str = include_str!("fixtures/config/omarchy-menu.jsonc");

/// One run's own copy of both files, removed with the target directory.
struct Home {
    directory: PathBuf,
    bindings: PathBuf,
    menu: PathBuf,
    models: PathBuf,
    key: PathBuf,
    reloads: Arc<AtomicUsize>,
    downloads: Arc<AtomicUsize>,
}

impl Home {
    fn new(name: &str) -> Home {
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("setup-{name}"));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the temporary home is created");

        let bindings = directory.join("bindings.lua");
        let menu = directory.join("omarchy-menu.jsonc");
        std::fs::write(&bindings, BINDINGS_FIXTURE).expect("the bindings copy is written");
        std::fs::write(&menu, MENU_FIXTURE).expect("the menu copy is written");

        // Safety: every openai download in this binary writes the same fake
        // bytes, so the override is the same value in every test.
        std::env::set_var(model::SHA256_ENV, model::sha256_hex(b"fake weights"));

        Home {
            models: directory.join("models"),
            key: directory
                .join("config")
                .join("grammachy")
                .join("openrouter-key"),
            directory,
            bindings,
            menu,
            reloads: Arc::new(AtomicUsize::new(0)),
            downloads: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A setup whose reload only counts and whose download writes a small fake
    /// weights file.
    fn setup(&self) -> Setup {
        let reloads = Arc::clone(&self.reloads);
        let downloads = Arc::clone(&self.downloads);
        Setup {
            bindings_path: self.bindings.clone(),
            menu_path: self.menu.clone(),
            models_directory: self.models.clone(),
            key_path: self.key.clone(),
            reload: Box::new(move || {
                reloads.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            download: Box::new(move |_url, path| {
                downloads.fetch_add(1, Ordering::SeqCst);
                std::fs::write(path, b"fake weights")
                    .map(|()| Transfer::Finished)
                    .map_err(|error| error.to_string())
            }),
        }
    }

    fn bindings_text(&self) -> String {
        std::fs::read_to_string(&self.bindings).expect("the bindings copy is readable")
    }

    fn menu_text(&self) -> String {
        std::fs::read_to_string(&self.menu).expect("the menu copy is readable")
    }
}

fn report(envelope: &SetupEnvelope) -> &SetupReport {
    match envelope {
        SetupEnvelope::Report(report) => report,
        SetupEnvelope::Error(error) => panic!("setup failed: {}", error.error.message),
    }
}

fn step<'a>(envelope: &'a SetupEnvelope, name: &str) -> &'a Step {
    report(envelope)
        .steps
        .iter()
        .find(|step| step.name == name)
        .unwrap_or_else(|| panic!("the report carries a {name} step"))
}

#[test]
fn a_second_run_leaves_one_block_and_one_entry() {
    let home = Home::new("twice");
    let setup = home.setup();

    let first = setup.install(EngineSlug::Languagetool, "gemma-4-e4b-it");
    let after_first = (home.bindings_text(), home.menu_text());
    let second = setup.install(EngineSlug::Languagetool, "gemma-4-e4b-it");

    assert_eq!(step(&first, "hotkeys").state, State::Changed);
    assert_eq!(step(&second, "hotkeys").state, State::Unchanged);
    assert_eq!(step(&second, "menu").state, State::Unchanged);
    assert_eq!((home.bindings_text(), home.menu_text()), after_first);
    assert_eq!(
        home.bindings_text().matches("-- grammachy begin").count(),
        1
    );
    assert_eq!(home.menu_text().matches("grammachy.compose").count(), 1);
}

#[test]
fn remove_puts_both_files_back_byte_for_byte() {
    let home = Home::new("remove");
    let setup = home.setup();

    setup.install(EngineSlug::Languagetool, "gemma-4-e4b-it");
    assert_ne!(home.bindings_text(), BINDINGS_FIXTURE);
    let removal = setup.remove();

    assert_eq!(home.bindings_text(), BINDINGS_FIXTURE);
    assert_eq!(home.menu_text(), MENU_FIXTURE);
    assert_eq!(step(&removal, "hotkeys").state, State::Changed);
    assert_eq!(step(&removal, "menu").state, State::Changed);
}

#[test]
fn removing_twice_is_no_error() {
    let home = Home::new("remove-twice");
    let setup = home.setup();

    setup.install(EngineSlug::Languagetool, "gemma-4-e4b-it");
    setup.remove();
    let again = setup.remove();

    assert_eq!(step(&again, "hotkeys").state, State::Unchanged);
    assert_eq!(step(&again, "menu").state, State::Unchanged);
    assert_eq!(home.bindings_text(), BINDINGS_FIXTURE);
}

#[test]
fn the_written_block_carries_the_two_bindings_of_spec_section_2() {
    let home = Home::new("lines");

    home.setup().install(EngineSlug::Harper, "gemma-4-e4b-it");

    let text = home.bindings_text();
    assert!(
        text.contains(
            "hl.unbind(\"SUPER + G\")\n\
             o.bind(\"SUPER + G\", \"Grammachy\", \
             [[omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"quick\"}']])\n"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "hl.unbind(\"SUPER + SHIFT + G\")\n\
             o.bind(\"SUPER + SHIFT + G\", \"Grammachy compose\", \
             [[omarchy-shell shell summon io.github.jyooi.grammachy '{\"mode\":\"compose\"}']])\n"
        ),
        "{text}"
    );
}

#[test]
fn the_compositor_is_reloaded_once_a_run() {
    let home = Home::new("reload");
    let setup = home.setup();

    setup.install(EngineSlug::Harper, "gemma-4-e4b-it");
    assert_eq!(home.reloads.load(Ordering::SeqCst), 1);

    setup.remove();
    assert_eq!(home.reloads.load(Ordering::SeqCst), 2);
}

#[test]
fn another_engine_downloads_nothing() {
    for engine in [EngineSlug::Languagetool, EngineSlug::Harper] {
        let home = Home::new(&format!("engine-{}", engine.as_str()));

        let envelope = home.setup().install(engine, "gemma-4-e4b-it");

        assert_eq!(step(&envelope, "model").state, State::Skipped);
        assert_eq!(home.downloads.load(Ordering::SeqCst), 0);
        assert!(!home.models.exists());
    }
}

#[test]
fn the_openai_engine_downloads_the_weights_once() {
    let home = Home::new("weights");
    let setup = home.setup();

    let first = setup.install(EngineSlug::Openai, "gemma-4-e4b-it");
    let second = setup.install(EngineSlug::Openai, "gemma-4-e4b-it");

    assert_eq!(step(&first, "model").state, State::Changed);
    assert_eq!(step(&second, "model").state, State::Unchanged);
    assert_eq!(home.downloads.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read_to_string(home.models.join("gemma-4-E4B-it-Q4_K_M.gguf")).unwrap(),
        "fake weights"
    );
}

#[test]
fn a_model_the_catalogue_does_not_know_is_an_error() {
    let home = Home::new("unknown-model");

    let envelope = home
        .setup()
        .install(EngineSlug::Openai, "some-private-model");

    assert_eq!(envelope.exit_code(), 1);
    assert_eq!(home.downloads.load(Ordering::SeqCst), 0);
    assert!(home.bindings_text().contains("-- grammachy begin"));
    assert!(home.menu_text().contains("grammachy.compose"));
}

#[test]
fn remove_keeps_the_weights() {
    let home = Home::new("keep-weights");
    let setup = home.setup();

    setup.install(EngineSlug::Openai, "gemma-4-e4b-it");
    setup.remove();

    assert!(home.models.join("gemma-4-E4B-it-Q4_K_M.gguf").is_file());
}

#[test]
fn a_half_finished_download_never_becomes_the_model() {
    let home = Home::new("failed-download");
    let setup = Setup {
        bindings_path: home.bindings.clone(),
        menu_path: home.menu.clone(),
        models_directory: home.models.clone(),
        key_path: home.key.clone(),
        reload: Box::new(|| Ok(())),
        download: Box::new(|_url, path| {
            std::fs::write(path, b"half").expect("the partial file is written");
            Err("the connection dropped".to_string())
        }),
    };

    let envelope = setup.install(EngineSlug::Openai, "gemma-4-e4b-it");

    assert_eq!(envelope.exit_code(), 1);
    assert!(!home.models.join("gemma-4-E4B-it-Q4_K_M.gguf").exists());
    assert!(home
        .models
        .join("gemma-4-E4B-it-Q4_K_M.gguf.part")
        .is_file());
    assert!(home.bindings_text().contains("-- grammachy begin"));
    assert!(home.menu_text().contains("grammachy.compose"));
}

#[test]
fn a_download_failure_still_writes_hotkeys_and_menu() {
    let home = Home::new("download-failure-keeps-config");
    let setup = Setup {
        bindings_path: home.bindings.clone(),
        menu_path: home.menu.clone(),
        models_directory: home.models.clone(),
        key_path: home.key.clone(),
        reload: Box::new(|| Ok(())),
        download: Box::new(|_url, _path| Err("the host refused".to_string())),
    };

    let envelope = setup.install(EngineSlug::Openai, "gemma-4-e4b-it");

    assert_eq!(envelope.exit_code(), 1);
    assert!(home.bindings_text().contains("-- grammachy begin"));
    assert!(home.menu_text().contains("grammachy.compose"));
    assert!(!home.models.join("gemma-4-E4B-it-Q4_K_M.gguf").exists());
}

#[test]
fn the_hardware_tier_names_the_backend_packages() {
    let home = Home::new("tier");
    let without = model::tier_of(&home.directory);
    std::fs::write(home.directory.join("renderD128"), b"").expect("a render node stands in");
    let with = model::tier_of(&home.directory);

    assert_eq!(without, model::Tier::Cpu);
    assert_eq!(with, model::Tier::Vulkan);
    assert_eq!(without.backend_packages(), ["ggml-cpu"]);
    assert_eq!(with.backend_packages(), ["ggml-cpu", "ggml-vulkan"]);
}

/// The mode of one path, as the twelve low bits Unix keeps.
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .expect("the path exists")
        .permissions()
        .mode()
        & 0o777
}

fn inode_of(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).expect("the path exists").ino()
}

#[test]
fn the_key_lands_in_a_private_file_inside_a_private_directory() {
    let home = Home::new("key-write");
    let setup = home.setup();

    let envelope = setup.write_key("sk-or-v1-example\n");

    assert_eq!(envelope.exit_code(), 0);
    assert_eq!(report(&envelope).mode, "key");
    assert_eq!(step(&envelope, "key").state, State::Changed);
    assert_eq!(
        std::fs::read_to_string(&home.key).expect("the key file is readable"),
        "sk-or-v1-example\n"
    );
    assert_eq!(mode_of(&home.key), 0o600);
    assert_eq!(
        mode_of(home.key.parent().expect("the key has a directory")),
        0o700
    );
}

/// The key file is the one thing here a report could leak, so it never does.
#[test]
fn no_step_of_a_key_run_prints_the_key() {
    let home = Home::new("key-secret");

    let envelope = home.setup().write_key("sk-or-v1-secret");

    assert!(
        !envelope.to_json().contains("sk-or-v1-secret"),
        "{envelope:?}"
    );
}

#[test]
fn writing_the_same_key_twice_changes_nothing_the_second_time() {
    let home = Home::new("key-twice");
    let setup = home.setup();

    let first = setup.write_key("sk-or-v1-same");
    let second = setup.write_key("sk-or-v1-same");
    let third = setup.write_key("sk-or-v1-other");

    assert_eq!(step(&first, "key").state, State::Changed);
    assert_eq!(step(&second, "key").state, State::Unchanged);
    assert_eq!(step(&third, "key").state, State::Changed);
    assert_eq!(mode_of(&home.key), 0o600);
}

/// A key file made loose by hand becomes private again on the next write.
#[test]
fn a_rewrite_tightens_a_loose_key_file() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::new("key-loose");
    let setup = home.setup();
    setup.write_key("sk-or-v1-first");
    std::fs::set_permissions(&home.key, std::fs::Permissions::from_mode(0o644))
        .expect("the mode is loosened by hand");

    setup.write_key("sk-or-v1-second");

    assert_eq!(mode_of(&home.key), 0o600);
}

/// The new key never lands in the loose inode: a descriptor opened before the
/// write keeps the access it was opened with, so a rewrite makes a fresh file
/// and renames it over the old one.
#[test]
fn a_rewrite_never_puts_the_key_in_the_loose_file() {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let home = Home::new("key-loose-inode");
    let setup = home.setup();
    setup.write_key("sk-or-v1-first");
    std::fs::set_permissions(&home.key, std::fs::Permissions::from_mode(0o644))
        .expect("the mode is loosened by hand");
    let loose = std::fs::File::open(&home.key).expect("another reader holds the loose file open");
    let loose_inode = loose.metadata().expect("the open file has metadata").ino();

    setup.write_key("sk-or-v1-second");

    let mut held = String::new();
    (&loose)
        .read_to_string(&mut held)
        .expect("the held descriptor still reads");
    assert!(!held.contains("sk-or-v1-second"), "{held:?}");
    assert_ne!(inode_of(&home.key), loose_inode);
    assert_eq!(mode_of(&home.key), 0o600);
    assert_eq!(
        std::fs::read_to_string(&home.key).expect("the key file is readable"),
        "sk-or-v1-second\n"
    );
}

/// Repairing the mode of an unchanged key is a change: the run tightened a
/// file another user could read, so the envelope must not say nothing happened.
#[test]
fn tightening_a_loose_key_file_reports_a_change() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::new("key-loose-state");
    let setup = home.setup();
    setup.write_key("sk-or-v1-same");
    std::fs::set_permissions(&home.key, std::fs::Permissions::from_mode(0o644))
        .expect("the mode is loosened by hand");

    let repair = setup.write_key("sk-or-v1-same");
    let again = setup.write_key("sk-or-v1-same");

    assert_eq!(step(&repair, "key").state, State::Changed);
    assert_eq!(step(&again, "key").state, State::Unchanged);
    assert_eq!(mode_of(&home.key), 0o600);
}

#[test]
fn stdin_that_is_not_one_key_is_refused_and_writes_nothing() {
    let home = Home::new("key-bad-stdin");
    let setup = home.setup();

    for stdin in ["", "   \n", "sk-or-v1-one sk-or-v1-two", "sk-or\nv1"] {
        let envelope = setup.write_key(stdin);

        assert_eq!(envelope.exit_code(), 1, "{stdin:?}");
        assert!(!home.key.exists(), "{stdin:?}");
    }
}

#[test]
fn remove_deletes_the_key_and_says_so_once() {
    let home = Home::new("key-remove");
    let setup = home.setup();
    setup.install(EngineSlug::Languagetool, "gemma-4-e4b-it");
    setup.write_key("sk-or-v1-goes-away");

    let first = setup.remove();
    let second = setup.remove();

    assert_eq!(step(&first, "key").state, State::Changed);
    assert_eq!(step(&second, "key").state, State::Unchanged);
    assert!(!home.key.exists());
}
