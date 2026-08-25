# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- `docs/spec/v1.md` is the authority on every contract.
  Section 5.1 fixes the `grammachy check` JSON envelope.
  Section 5.2 fixes the `grammachy chunk` JSON envelope.
  Section 10 is the packaging, section 11 the repository layout, section 13 the test plan.
  `CONTEXT.md` holds the domain glossary; `docs/adr/` records the settled decisions.
- The Rust CLI lives in `cli/` and is its own cargo package.
  Run `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` from `cli/`.
  CI runs the same three commands.
- The CLI prints exactly one JSON envelope on stdout and uses stderr for logs only.
  Exit 0 carries a result, exit 1 carries an error envelope.
  Spans are UTF-16 code units, because the shell indexes the text in JavaScript.
- Each subcommand owns its result envelope and shares the error envelope in `cli/src/envelope.rs`.
  `check` uses `Envelope`, `chunk` uses `ChunkEnvelope` in `cli/src/chunk.rs`.
- Engine adapters plug into the `Engine` trait in `cli/src/engine.rs` and live under `cli/src/engines/`.
  `engine::resolve` answers `None` for a slug with no adapter, which surfaces as `engine_unavailable`.
- The `languagetool` adapter starts the transient unit itself, so tests must never reach the real one.
  `cli/src/engines/languagetool/unit.rs` documents the exact server command read from the pacman package, including the two sharp edges of `/usr/bin/languagetool`: it needs `JAVA_HOME`, and `--http` is what picks the plain HTTP server.
  `GRAMMACHY_LANGUAGETOOL_ADDRESS` and `GRAMMACHY_LANGUAGETOOL_START=never` are the test seams; `cli/tests/cli.rs` sets both.
  Live tests in `cli/tests/languagetool_live.rs` and `cli/tests/interference_catch_rate.rs` skip when `127.0.0.1:8081` is silent, which keeps CI green without the package.
- Settings resolve in `cli/src/settings.rs`: flags, then the plugin entry in `$HOME/.config/omarchy/shell.json`, then the defaults of spec section 7.
  The product path is that HOME path only. The CLI does not read `$XDG_CONFIG_HOME`.
  The entry is looked up by plugin id in `bar.layout.{left,center,right}` first and in the top level `plugins` array next, the order `shell.qml` writes them in.
  `GRAMMACHY_SHELL_JSON` is the test seam; no test may read or write the real file.
- `manifest.json` version must equal the crate version; `cli/tests/manifest.rs` enforces that.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
