# Project agent memory

Grammachy is a desktop grammar and style checker for Omarchy on Hyprland.
The Rust CLI in `cli/` runs the engines and prints one JSON envelope for `check`, `chunk`, `setup`, and `engine`.
The Omarchy Quickshell plugin at the repo root (`manifest.json`, `BarWidget.qml`, `Overlay.qml`, `ui/`) captures text, runs the CLI, and draws the cards.
`harper` is the default engine and is compiled in.
`languagetool` is an opt-in component.
The Local LLM and Cloud LLM engines were removed (HUF-240).
`git log` before that removal has the code.

## Authorities

- `docs/spec/v1.md` fixes every contract: 5.1 `check`, 5.2 `chunk`, 5.3 `engine`, 7 settings, 9 Compose, 10 packaging and setup, 11 layout, 13 test plan.
- `CONTEXT.md` holds the domain glossary.
- `docs/dev.md` is the only route onto a live desktop, including release steps and the smoke items.
- `docs/doctor.md` documents the `doctor` envelope, its `state` words, and its exit code.

## Build and test

- Run `cargo test`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt --check` from `cli/`.
  CI runs the same three.
- `ui/*.js` files are loaded by QML and by node, so they may use neither API.
  Their `*.test.js` siblings run under `node --test`.
  Add a new one to `.github/workflows/ci.yml` and `docs/dev.md`.
- The repo has no QML test harness.
  `Overlay.qml` cannot run outside the shell's plugin loader.
  `cli/tests/overlay_*.rs` keep it in step with the JS modules by reading its text.
- A leaf card in `ui/` runs in a scratch Quickshell config under `QT_QPA_PLATFORM=offscreen`.
  The config root holds `Commons` and `Ui` symlinks from `/usr/share/omarchy/shell`, plus a `ui` symlink into the repo.
- The plugin CI job clones `basecamp/omarchy` at `OMARCHY_REF` for the `qs.*` modules and `omarchy-plugin-validate`.
  Raise the tag when the plugin needs a newer shell.
- The `qmllint` on `PATH` is the Qt 5 verifier.
  It reports only through its exit status and rejects reserved words such as `native` as identifiers.
  Run `/usr/lib/qt6/bin/qmllint <file>` for line and column.
  Neither resolves `qs.*` imports, so only loading the file catches a missing import.

## CLI contracts

- `check`, `chunk`, `setup`, and `engine` print exactly one JSON envelope on stdout and log to stderr only.
  Exit 0 carries a result.
  Exit 1 carries an error envelope.
- Default `doctor` prints text.
  `--json` prints one JSON envelope.
  Exit 1 means a missing piece, not an error envelope.
- Spans are UTF-16 code units, because the shell indexes text in JavaScript.
  Harper counts `char`s, so `cli/src/engines/harper/lints.rs` converts through `text::utf16_offsets`.
- Engines implement the `Engine` trait in `cli/src/engine.rs` and live under `cli/src/engines/`.
  Every adapter hands its Issues to `issues::normalise`, which owns the sort, overlap, and no-op guarantees of spec 5.1.
- The `languagetool` adapter never trusts a port. `cli/src/engines/listener.rs` proves, before every request, that the unit's own process holds the loopback listener.
  Fixtures that name `127.0.0.1:8081` are sample text only.
- Only `languagetool` and `harper` are `EngineSlug` variants.
  A stored engine the CLI does not recognise falls back to the default rather than failing (`cli/tests/settings.rs`).
- Settings resolve in `cli/src/settings.rs`: flags, then the plugin entry in `$HOME/.config/omarchy/shell.json`, then spec 7 defaults.
  The CLI never reads `$XDG_CONFIG_HOME`.
  `ui/settings.js` is the shell-side twin and must agree on the lookup.
- The Check size limit lives in `EngineSlug::check_limit_utf16` and `ui/limits.js`.
  The Draft cap `chunk::MAX_DRAFT_UTF16_UNITS` lives in Rust and in the QML that refuses an oversize Draft.
  `cli/tests/overlay_limit.rs` keeps every copy equal.
- `manifest.json` and `cli.lock` versions must equal the crate version.
  `cli/tests/manifest.rs` enforces both.
- `cli/src/doctor/facts.rs` is the only place that reads the machine.
  The report is a pure function of `Facts`.
- The system package table lives in `cli/src/doctor/deps.rs` and again in `ui/deps.js`, because the setup card opens before `bin/grammachy` exists.
  `cli/tests/overlay_deps.rs` keeps the two equal and `cli/tests/readme_dependencies.rs` keeps the README section equal to both.
  The plugin does not install packages itself. A missing package is named for Omarchy Install.

## Plugin

- `Overlay.qml` owns capture, the CLI run, the key map dispatch, Apply, the review state, the Draft, and settings storage.
  Every QML file in `ui/` only draws.
  `ui/*.js` owns capture, settings, keymap, errors, and other logic.
  Both surfaces (`quick`, `compose`) share one `phase`, one Check, and one key map.
- Every `Text` whose `text` is not a string literal sets `textFormat: Text.PlainText`, because the Selection, the Issues, and the error messages are strings the plugin did not write.
  `cli/tests/overlay_text.rs` enforces it.
- Name every new `phase` in `Overlay.keyMode`.
  An unnamed phase uses `MODE_IDLE`.
  `keymap.js` then maps Esc to close and ignores Accept, Skip, and Apply.
- Omarchy answers `configProvider: lua`.
  Hyprland bindings go in `bindings.lua` and `hyprctl dispatch` takes Lua, never the `.conf` syntax.
  `hyprctl repl` shows any dispatcher's Lua name.
  See `ui/anchor.js` and `cli/src/setup/bindings.rs`.
  The two trigger keys are remappable settings, `quickHotkey` and `composeHotkey`.
  `bindings::Hotkeys::resolve` reads them through `StoredSettings` and uses the spec section 2 defaults for an empty or missing value.
- `cli/src/setup/menu.rs` writes the `apps.grammachy` row into the Omarchy menu extension.
  Omarchy infers the parent from the dotted id, so the row sits under Apps.
  The action summons with `{"mode":"quick"}`, the same payload as the quick hotkey.
- Settings storage is the plugin entry in `shell.json`, read through `Overlay.setting` and written through `Overlay.persistSetting`.
  Every write carries the whole entry, because `shell.updateEntryInline` replaces rather than merges.
  The manifest `barWidget.defaults` and `schema` are documentation only.
  No QML may read them.
- The Draft lives in `Overlay.draftText` only: no file, no clipboard, no setting.
  `Overlay.clearCapture` must never touch it.
- `Overlay.releasePrimary` is the one place that runs `wl-copy --primary --clear`, and it runs on close, never at capture time.

## Testing rules

- No test may reach a real engine server, a real systemd unit, a real config file, a real compositor, or the network.
  Stub binaries and the seams below are the only route.
- Seams: `GRAMMACHY_LANGUAGETOOL_ADDRESS` (loopback only, debug builds only), `GRAMMACHY_LANGUAGETOOL_START=never` (debug builds only), `GRAMMACHY_ENGINE_STOP=never`, `GRAMMACHY_ENGINES_DIR`, `GRAMMACHY_ENGINE_BASE_URL`, `GRAMMACHY_ENGINE_SHA256`, `GRAMMACHY_ENGINE_SIZE_BYTES`, `GRAMMACHY_SHELL_JSON`, `GRAMMACHY_BINDINGS_LUA`, `GRAMMACHY_MENU_JSONC`, `GRAMMACHY_HYPRCTL_RELOAD=never`, and the `GRAMMACHY_BOOTSTRAP_*` set in `bin/bootstrap.sh`.
- `cli/tests/engine_install.rs` owns its whole binary because it pins a digest for the process.
  `languagetool_live.rs` skips when the `grammachy-languagetool` user unit is not active.
- Debug builds of the `harper` adapter time out at 60 s so CI can load the dictionary.
  The shipped binary keeps the spec limit of 10 s.
  `cli/tests/harper_lazy.rs` guards that the dictionary loads only inside `Harper::check`.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
