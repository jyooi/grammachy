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
- `check` and `chunk` print exactly one JSON envelope on stdout and use stderr for logs only.
  Exit 0 carries a result, exit 1 carries an error envelope.
  Spans are UTF-16 code units, because the shell indexes the text in JavaScript.
- `check` and `chunk` each own a result envelope and share the error envelope in `cli/src/envelope.rs`.
  `check` uses `Envelope`, `chunk` uses `ChunkEnvelope` in `cli/src/chunk.rs`.
- Engine adapters plug into the `Engine` trait in `cli/src/engine.rs` and live under `cli/src/engines/`.
  `engine::resolve` answers `None` for a slug with no adapter, which surfaces as `engine_unavailable`.
  Every adapter maps its own engine answer to Issues and then hands the list to `issues::normalise`, which owns the sort, overlap, and no-op guarantees of spec section 5.1.
- The `harper` adapter runs `harper-core` 2.8 in process and needs no server. Debug builds time out at 60 s so CI can load the dictionary. The shipped binary keeps the spec limit of 10 s.
  Harper counts `char`s and the contract counts UTF-16 code units, so `lints.rs` converts through `text::utf16_offsets`.
  The dictionary and rule set are built inside `Harper::check` only, so the default path never pays for them; `cli/tests/harper_lazy.rs` guards that with a counter.
  `harper-core` is edition 2024, which is why the crate `rust-version` is 1.85.
- `languagetool` and `openai` both run a local server in a transient unit, and `cli/src/engines/local.rs` holds what that costs them in common.
  Each `unit.rs` documents its own server command and its sharp edges: `/usr/bin/languagetool` needs `JAVA_HOME` and `--http`; `/usr/bin/llama-server` comes from `llama-cpp` plus a separate `ggml-cpu` or `ggml-vulkan` backend package.
  Tests must never reach a real server or a real unit.
  The seams are `GRAMMACHY_LANGUAGETOOL_ADDRESS`, `GRAMMACHY_LANGUAGETOOL_START=never`, and `GRAMMACHY_LLAMA_START=never`; `cli/tests/cli.rs` sets all three.
  The `openai` adapter takes its starter as a value, so `cli/tests/openai_stub.rs` covers the start behaviour with no systemd at all.
  Live tests in `languagetool_live.rs`, `openai_live.rs`, and `interference_catch_rate.rs` skip when their port is silent, which keeps CI green without the packages.
- The `openai` base URL host must be loopback, and `cli/src/engines/openai/endpoint.rs` is the only place that decides it.
  A remote host is `bad_arguments` and no request is made; that is a product guarantee, so keep it tested.
  Its prompt in `prompt.rs` is the wording HUF-181 measured, and the "shortest exact substring" rule is what makes the spans usable rather than whole-sentence rewrites.
- Settings resolve in `cli/src/settings.rs`: flags, then the plugin entry in `$HOME/.config/omarchy/shell.json`, then the defaults of spec section 7.
  The product path is that HOME path only. The CLI does not read `$XDG_CONFIG_HOME`.
  The entry is looked up by plugin id in `bar.layout.{left,center,right}` first and in the top level `plugins` array next, the order `shell.qml` writes them in.
  `GRAMMACHY_SHELL_JSON` is the test seam; no test may read or write the real file.
- `grammachy bench` in `cli/src/bench/` is the one subcommand that prints Markdown on stdout rather than a JSON envelope; a failure still prints the error envelope.
  One run is the whole benchmark file: `grammachy bench ... > docs/benchmarks/<version>.md`, nothing added by hand.
  `--engine openai --model <name>` fills the Models table and does not narrow the Engines table.
  An engine the machine cannot reach is a skipped row, never an error, so a machine without llama.cpp still produces a valid file.
  `cli/src/bench/weights.rs` is the product rule for which models may be recommended (spec section 13.1).
- `doctor` reports the install state and the one-line engine diagnosis the `engine_unavailable` card shows.
  `docs/doctor.md` documents its envelope, exit code, and hardware tiers.
  `cli/src/doctor/facts.rs` is the only place that reads the machine, so the report is a pure function of recorded `Facts` and no test reads real hardware.
- `manifest.json` version must equal the crate version; `cli/tests/manifest.rs` enforces that.
- The Omarchy plugin is the repo root: `manifest.json`, `BarWidget.qml`, `Overlay.qml`, and `ui/`.
  `Overlay.qml` owns capture, the CLI run, the review state, and the settings storage; `ui/QuickCard.qml`, `ui/SettingsView.qml`, and `ui/MarkedText.qml` only draw.
  `ui/splice.js`, `ui/tokens.js`, and `ui/settings.js` are loaded by QML and by node, so they may use neither's API.
  Their `*.test.js` siblings run under `node --test`; add a new one to `.github/workflows/ci.yml` and to `docs/dev.md`.
  `docs/dev.md` is the only route onto a live desktop, including the manual smoke items.
- Settings storage is the plugin entry in `shell.json`, read through `Overlay.setting(name, fallback)` and written through `Overlay.persistSetting(name, value)`; nothing else may touch the file.
  `ui/settings.js` owns the spec section 7 rules and is the shell-side counterpart of `cli/src/settings.rs`; the two must agree on the entry lookup and on what counts as unknown.
  `shell.updateEntryInline` replaces the entry rather than merging into it, so every write carries the whole stored entry, which is what keeps the file-only keys and any untouched unknown value.
  `Dropdown` writes its own `value` on select, which drops a declarative binding, so a live external write needs the `onXChanged` re-assert in `ui/SettingsView.qml`.
  The manifest `barWidget.defaults` and `barWidget.schema` are documentation: the shell stores them and never merges them, so no QML may read them.
- The plugin CI job clones `basecamp/omarchy` at the tag in `OMARCHY_REF`, because both the `qs.*` QML modules and `omarchy-plugin-validate` come from that tree.
  Raise the tag when the plugin starts to need a newer shell.
- The `qmllint` on `PATH` is the Qt 5 syntax verifier: it prints nothing and reports a syntax error through its exit status alone.
  Its JavaScript parser also rejects ECMAScript reserved words such as `native` as identifiers, with no message.
  Run `/usr/lib/qt6/bin/qmllint <file>` for line and column; ignore its import and unqualified-access warnings, which the shell's own plugins raise too.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
