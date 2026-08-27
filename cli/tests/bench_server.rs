//! A bench row runs the product's own llama.cpp server, evals spec section 4.1.
//!
//! The reasoning flags of spec section 6 live on the transient unit, and the
//! numbers a benchmark file prints are only the product's numbers while the
//! server behind them is the product's server. `bench` therefore builds no
//! server command of its own: it stops the unit between two models and lets
//! the `openai` adapter start it again through `engines::openai::unit`.
//!
//! No case here starts a server. The unit's own arguments are asserted in
//! `engines::openai::unit`; this file keeps `bench` from growing a second
//! copy of them.

use grammachy::engines::openai::unit;

fn bench_source() -> String {
    let directory = format!("{}/src/bench", env!("CARGO_MANIFEST_DIR"));
    let mut source = String::new();
    for entry in std::fs::read_dir(&directory).expect("the bench module is readable") {
        let path = entry.expect("the entry is readable").path();
        if path.extension().is_some_and(|kind| kind == "rs") {
            source.push_str(&std::fs::read_to_string(&path).expect("the file is readable"));
        }
    }
    source
}

#[test]
fn the_bench_starts_no_server_command_of_its_own() {
    let source = bench_source();

    for flag in [
        "--reasoning-budget",
        "--reasoning-format",
        "--ctx-size",
        "--parallel",
        "llama-server",
    ] {
        assert!(
            !source.contains(flag),
            "cli/src/bench/ names {flag}, so it builds a server the product does not run"
        );
    }
}

/// The unit a bench row's adapter starts is the unit a Check starts, so the
/// reasoning flags of evals spec section 6 are on it.
#[test]
fn the_product_unit_carries_the_reasoning_flags_a_bench_row_measures() {
    let command = unit::server_command(
        std::path::Path::new("/models/gemma.gguf"),
        "127.0.0.1",
        8080,
    );

    for flag in ["--jinja", "--reasoning-format", "--reasoning-budget"] {
        assert!(
            command.arguments.iter().any(|argument| argument == flag),
            "the server command carries {flag}: {:?}",
            command.arguments
        );
    }
}
