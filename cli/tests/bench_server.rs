//! A bench row runs the product's own llama.cpp server, evals spec section 4.1.
//!
//! The reasoning flags of spec section 6 live on the transient unit, and the
//! numbers a benchmark file prints are only the product's numbers while the
//! server behind them is the product's server. `bench` therefore builds no
//! server command of its own: it stops the unit between two models and lets
//! the `openai` adapter start it again through `engines::openai::unit`.
//!
//! No case here starts a server. It asserts the arguments the one server
//! command carries, so a bench row measures the product's own server.

use grammachy::engines::openai::unit;

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
