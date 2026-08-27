//! `grammachy setup` and `grammachy setup --remove`, spec section 10.
//!
//! Every case works on copies of the two configuration files in a temporary
//! directory under `CARGO_TARGET_TMPDIR`, never on the real ones. No case
//! reloads a compositor: the reload is a value the test hands in.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use grammachy::setup::{Setup, SetupEnvelope, SetupReport, State, Step};

/// The files a fresh Omarchy install carries, as this repository keeps them.
const BINDINGS_FIXTURE: &str = include_str!("fixtures/config/bindings.lua");
const MENU_FIXTURE: &str = include_str!("fixtures/config/omarchy-menu.jsonc");

/// One run's own copy of both files, removed with the target directory.
struct Home {
    bindings: PathBuf,
    menu: PathBuf,
    reloads: Arc<AtomicUsize>,
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

        Home {
            bindings,
            menu,
            reloads: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// A setup whose reload only counts.
    fn setup(&self) -> Setup {
        let reloads = Arc::clone(&self.reloads);
        Setup {
            bindings_path: self.bindings.clone(),
            menu_path: self.menu.clone(),
            reload: Box::new(move || {
                reloads.fetch_add(1, Ordering::SeqCst);
                Ok(())
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

    let first = setup.install();
    let after_first = (home.bindings_text(), home.menu_text());
    let second = setup.install();

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

    setup.install();
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

    setup.install();
    setup.remove();
    let again = setup.remove();

    assert_eq!(step(&again, "hotkeys").state, State::Unchanged);
    assert_eq!(step(&again, "menu").state, State::Unchanged);
    assert_eq!(home.bindings_text(), BINDINGS_FIXTURE);
}

#[test]
fn the_written_block_carries_the_two_bindings_of_spec_section_2() {
    let home = Home::new("lines");

    home.setup().install();

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

    setup.install();
    assert_eq!(home.reloads.load(Ordering::SeqCst), 1);

    setup.remove();
    assert_eq!(home.reloads.load(Ordering::SeqCst), 2);
}
