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
