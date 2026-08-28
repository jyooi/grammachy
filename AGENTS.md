# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- `docs/spec/v1.md` is the authority on every contract.
  Section 5.1 fixes the `grammachy check` JSON envelope.
  Section 5.2 fixes the `grammachy chunk` JSON envelope.
  Section 10 is the packaging, section 11 the repository layout, section 13 the test plan.
  `CONTEXT.md` holds the domain glossary.
  Grammachy shipped a Local LLM engine (`openai`, backed by llama-server) and a Cloud LLM engine (`openrouter`), plus the eval and benchmark programme that picked and ranked models for them, and removed all of it (HUF-240): a two-sentence check took 17 s on a CPU-only laptop with thinking on, and 1.6 s with thinking off at a higher false-positive rate. `git log` before that removal has the code if it is ever needed again.
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
  Only `languagetool` and `harper` are `EngineSlug` variants; a stored `engine` of `openai` or `openrouter` in an existing user's `shell.json` fails `EngineSlug::from_stored` and falls back to the default (`cli/tests/settings.rs`, `cli/tests/cli.rs`).
- The `harper` adapter runs `harper-core` 2.8 in process and needs no server. Debug builds time out at 60 s so CI can load the dictionary. The shipped binary keeps the spec limit of 10 s.
  Harper counts `char`s and the contract counts UTF-16 code units, so `lints.rs` converts through `text::utf16_offsets`.
  The dictionary and rule set are built inside `Harper::check` only, so the default path never pays for them; `cli/tests/harper_lazy.rs` guards that with a counter.
  `harper-core` is edition 2024, which is why the crate `rust-version` is 1.85.
- `languagetool` runs a local server in a transient unit, and `cli/src/engines/local.rs` holds that plumbing: starting the unit, the runtime directory, and telling an unreachable-port error from a real failure.
  `unit.rs` documents the server command and its sharp edges: LanguageTool has two routes onto the machine (see the `grammachy engine` entry below).
  Tests must never reach a real server or a real unit.
  The seams are `GRAMMACHY_LANGUAGETOOL_ADDRESS` and `GRAMMACHY_LANGUAGETOOL_START=never`; `cli/tests/cli.rs` sets both.
  The live test in `languagetool_live.rs` skips when its port is silent, which keeps CI green without the package.
- `grammachy engine` lives in `cli/src/engines/install/`, spec section 5.3: `list`, `install`, and `remove` for the optional engine components.
  Only `languagetool` is a component; `harper` is compiled into the binary and has nothing to install (HUF-237).
  `transfer.rs`, `digest.rs`, `disk.rs`, and `cancel.rs` are its own generic download and unit-stop machinery: HUF-240 retired the `grammachy model` command that used to own this and share it, so it lives here now, reused wholesale because an install is a download with one more step.
  `archive.rs` is the unpack step: `bsdtar`, which reads a zip and is in the Arch base group, behind an `Extractor` value so no test unpacks a real archive.
  Its row is pinned twice, by the sha256 the Arch `languagetool` package pins for the same file and by the byte size an unauthenticated HEAD reports, plus `installed_bytes` from the package's installed size, so the free-space check measures the peak of the install rather than the archive alone.
  The install unpacks into `<dir>/<slug>.unpack` and renames into `<dir>/<slug>` only once the row's `entry` file is there, because a `bsdtar` that died half way leaves a directory behind and `install::installed` must never call that an engine.
  `install::installed` is the one reader the adapter, `doctor`, and the row state share.
  `STOP_ENV` (`GRAMMACHY_ENGINE_STOP`) keeps a stop from reaching the real unit; tests and CI set it to `never`. `NOT_LOADED` is what `stop_unit` reports for a unit that was not running, and `stop_found_nothing_to_stop` is the reader that lets a transient unit already collected through as success rather than failure.
  Seams are `GRAMMACHY_ENGINES_DIR`, `GRAMMACHY_ENGINE_BASE_URL`, `GRAMMACHY_ENGINE_SHA256`, `GRAMMACHY_ENGINE_SIZE_BYTES`, plus `STOP_ENV`.
  `cli/tests/engine_install.rs` owns its whole binary, because it pins a digest for the process.
- The default engine is `harper` (spec section 4, HUF-237), so a fresh install runs no download and no pacman command.
  It lives in `args::CheckOptions::default`, the `engine` descriptor of `ui/settings.js`, `Settings.BUILT_IN_ENGINE`, and `ui/SettingsView.qml`; `cli/tests/overlay_engines.rs` keeps the four equal.
  A test that runs the binary with no `--engine` gets a Harper result envelope rather than an `engine_unavailable` error.
- `languagetool::unit::server_command` reads two routes in order: the tree `engine install` unpacked, run as `java -cp <tree>/languagetool-server.jar:<tree>/libs/*`, then `/usr/bin/languagetool --http` from the pacman package.
  The tree wins, because a user who added it from Settings asked for the release this build pins.
  Neither being there is not a fault of the machine: `doctor` calls that `optional` rather than `missing`.
  `Check::optional` is that word, and only `languagetool` and `java` ever set it, `java` only while LanguageTool is absent.
  `ok` still answers the engine question, so `doctor --engine languagetool` on such a machine still exits 1.
  The `languagetool` check carries a `state` word (`installed`, `package`, `absent`) documented in `docs/doctor.md` and held by `cli/tests/overlay_engines.rs`.
  `report::LANGUAGETOOL_INSTALL_COMMAND` is the one command every line that offers it names, and it carries no `sudo`.
- The Engines list of spec sections 5.3 and 7 is `ui/engines.js` plus `ui/EnginesView.qml`, embedded by `SettingsView.qml` and drawn for every engine.
  `Engines.unavailable` feeds `Settings.engineOptions`, which drops an absent engine from the dropdown and always keeps the value the reader is on, so the box is never blank while the file says otherwise.
  `Settings.engineAfterRemoval` is the fallback rule and `Overlay.fallBackFromRemovedEngine` is its one caller; a component the pacman package still supplies moves no setting, which `Engines.isAvailable` decides.
  `confirmEngine` is a `phase` with its own `Keymap.MODE_ENGINE_CONFIRM`, and `resetRun` leaves `engines`, `engineBusy`, `engineActionProcess`, and `enginePoll` alone so closing the overlay never cancels an install.
  `cli/tests/overlay_engines.rs` keeps `Overlay.qml`, `ui/SettingsView.qml`, and both sides of every shared constant in step, and `ui/engines.test.js` runs the whole route against a stub binary.
- `grammachy setup` lives in `cli/src/setup/`, spec section 10.
  It prints one JSON envelope (`SetupEnvelope`).
  Exit 1 uses `setup_failed`.
  `block.rs` owns the marked block both configuration files carry and the rule that makes `--remove` byte exact: the region always carries the newline on each side, so insertion and removal are the same substring.
  `bindings.rs` holds the two `hl.unbind` plus `o.bind` pairs of spec section 2 and the `hyprctl reload`; the file is `bindings.lua`, because Omarchy answers `configProvider: lua` and never reads the `.conf` files beside it.
  `menu.rs` holds the `grammachy.compose` row, which names `"parent": "root"` because nothing else creates a `grammachy` submenu.
  `Setup` holds only `bindings_path`, `menu_path`, and `reload`; `install()` writes the hotkeys, then reloads, then the menu entry, and `remove()` reverses both.
  Every path and the one side effect are seams: `GRAMMACHY_BINDINGS_LUA`, `GRAMMACHY_MENU_JSONC`, `GRAMMACHY_HYPRCTL_RELOAD=never`, plus the `Reloader` value.
  No test may touch a real config file or a real compositor.
- Settings resolve in `cli/src/settings.rs`: flags, then the plugin entry in `$HOME/.config/omarchy/shell.json`, then the defaults of spec section 7.
  The product path is that HOME path only. The CLI does not read `$XDG_CONFIG_HOME`.
  The entry is looked up by plugin id in `bar.layout.{left,center,right}` first and in the top level `plugins` array next, the order `shell.qml` writes them in.
  `GRAMMACHY_SHELL_JSON` is the test seam; no test may read or write the real file.
  Unknown stored keys are ignored without error, and a stored `engine` the CLI does not recognise falls back to the default engine the same way (`cli/tests/settings.rs`), which is what keeps an old user's `shell.json` from breaking after HUF-240.
- `doctor` reports the install state and the one-line engine diagnosis the `engine_unavailable` card shows.
  `docs/doctor.md` documents its envelope and exit code.
  `cli/src/doctor/facts.rs` is the only place that reads the machine, so the report is a pure function of recorded `Facts` and no test reads real hardware.
  `Facts` carries only `binary`, `version`, `languagetool_tree`, `languagetool_launcher`, `java`, `languagetool_address`, and `languagetool_unit`; the checks are `binary`, `languagetool`, `java`, and `unit:languagetool`.
- Compose (spec section 9) keeps the Draft in `Overlay.draftText` and nowhere else: no file, no clipboard, no setting.
  `ui/DraftField.qml` is the text area; it forwards key presses to the overlay's key catcher through `Keys.forwardTo` with `Keys.priority: Keys.BeforeItem`, which is what lets Ctrl + Enter run the Check while every printable key still types.
  `Overlay.restoreFocus` is the one place that decides whether the Draft or the key catcher holds the keyboard.
  `ui/format.js` owns the one refusal Compose can print, the cap, so its wording has a node test; anything under the cap is checked in Chunks and is never refused.
- The chunked Check is the loop between `Overlay.startComposeCheck` and `Overlay.finishChunkRun`: one `grammachy chunk`, then `launchCheck` per Chunk, with `absorbChunk` moving each answer by that Chunk's `start` before `Splice.verifiedIssues` checks it against the whole Draft.
  `chunkRun` is what tells the shared `onCheckOutput` that this Check is a Chunk rather than a Selection.
  Cancel only sets `chunkCancelled`, which `absorbChunk` reads after the merge, so the Chunk in flight is always kept; a failure leaves `chunkIndex` on the Chunk that failed, which is what makes `Retry remaining` a resume rather than a restart.
  `chunkEngine` records the Engine the Chunk list was packed for, and every Check of that run names it.
  A setting changed mid-run therefore reaches the next run, not the Chunks already cut.
  `retryRemaining` compares `Limits.checkLimit` rather than the slugs, so a list of another size re-packs through `dropChunkListForNewEngine`.
  A list of the same size resumes on the Engine the reader picked at the failure.
  `ui/errors.js` owns both envelope readers and the inline card: `readChunks` for spec 5.2 and `chunkCard` for the two recovery buttons of section 9.
  `ui/errors.test.js` runs the whole loop against a stub binary that answers both subcommands, and `cli/tests/overlay_chunks.rs` is what keeps `Overlay.qml` on those same calls, because no QML test can.
- Every Compose trigger that carries a text lands on `Overlay.composeWith`, which is the only route to the replace confirm of spec section 2; `showCompose` is the kept-Draft route that SUPER + SHIFT + G and the menu entry take.
  `ui/CardHero.qml` takes an `actions` list for its trailing edge, which is how the popup gets its Compose button and Compose gets its Cancel without the hero knowing either.
- `manifest.json` version must equal the crate version, and `cli.lock` version must too; `cli/tests/manifest.rs` enforces both.
  The Check size limit belongs to the Engine: `EngineSlug::check_limit_utf16` in `cli/src/args.rs` is the Rust authority and `ui/limits.js` is the shell copy. Both remaining engines share one limit, 5,000 UTF-16 code units.
  `check` and `chunk` both take that limit as an argument, so `chunk` has its own `--engine` and packs to the same number the Check will refuse at.
  The Draft cap `chunk::MAX_DRAFT_UTF16_UNITS` is one number and lives twice, in Rust and in the QML that refuses an oversize Draft.
  `cli/tests/overlay_limit.rs` keeps every copy equal.
- Release and setup, spec section 10 (HUF-200). `.github/workflows/release.yml` builds `grammachy-x86_64-linux` for `x86_64-unknown-linux-musl` on every `v*` tag and attaches the binary and its `.sha256` to the release; `cli/Cargo.toml`'s `[profile.release]` already carries opt-level z, LTO, and strip, so the workflow adds no flags of its own.
  `cli.lock` at the repo root pins the released version and its sha256; a release is two commits, the tag and this bump, and `bin/release-lock.sh <tag>` makes the bump mechanical by downloading the asset, hashing it, and rewriting the file (`docs/dev.md` section 18).
  `cli.lock` ships with `sha256: ""` until the first tag exists.
  `bin/bootstrap.sh` is the end-user download: curl against the public release URL first, falling back to `gh release download` only on a 404 with `gh` authenticated, writing to a temp file beside the target and moving it into place only once the sha256 matches, so a mismatch or an interrupted run never leaves `bin/grammachy` half written.
  Its seams are `GRAMMACHY_BOOTSTRAP_LOCK`, `GRAMMACHY_BOOTSTRAP_OUT`, `GRAMMACHY_BOOTSTRAP_REPO`, `GRAMMACHY_BOOTSTRAP_BASE_URL`, `GRAMMACHY_BOOTSTRAP_CURL`, and `GRAMMACHY_BOOTSTRAP_GH` (`never` disables the gh fallback); `cli/tests/bootstrap.rs` runs the real script against a stub curl and never reaches the network.
  `ui/SetupCard.qml` draws the model `ui/setupCard.js` owns from `cli.lock`'s text and the run's own state (`Overlay.qml`'s `bootstrapRunning`, `bootstrapExitCode`, `bootstrapLog`, read through a `FileView` and a streaming `Process`); an empty pinned sha256 reads as `UNPINNED` and shows the developer path with no Install button.
  `startQuick` and `startComposeCheck` open the setup card when `bin/grammachy` is absent, before capture or chunking.
  `Overlay.showSetup` is also the `Setup` button of the `bad_arguments` card (spec section 8).
  Neither it nor `resetRun` touches the bootstrap state, so closing and reopening the popup mid-install leaves the run going, the same rule an engine install keeps.
- The Omarchy plugin is the repo root: `manifest.json`, `BarWidget.qml`, `Overlay.qml`, and `ui/`.
  `Overlay.qml` owns capture, the CLI run, the key map dispatch, the Apply path, the review state, the Draft, and the settings storage.
  Every QML file in `ui/` only draws.
  `root.surface` is `"quick"` or `"compose"` and is what routes a summon (spec section 2); both surfaces share one `phase`, one Check, one review state, and one key map, so a change to either belongs in `Overlay.qml` rather than in a card.
  `Overlay.keyMode` is where a new `phase` has to be named, or its card silently inherits the review keys.
  `ui/QuickCard.qml` and `ui/ComposeCard.qml` are the two surfaces.
  `ui/CardHero.qml`, `ui/Inspector.qml`, `ui/ReviewCounts.qml`, `ui/MarkedText.qml`, `ui/ErrorCard.qml`, `ui/SetupCard.qml`, and `ui/SettingsView.qml` are shared parts, so a change to the hero, the inspector, or the counts reaches both at once.
  `ui/splice.js`, `ui/tokens.js`, `ui/keymap.js`, `ui/format.js`, `ui/settings.js`, `ui/errors.js`, `ui/setupCard.js`, `ui/anchor.js`, `ui/capture.js`, and `ui/limits.js` are loaded by QML and by node, so they may use neither's API.
  Their `*.test.js` siblings run under `node --test`; add a new one to `.github/workflows/ci.yml` and to `docs/dev.md`.
  `keymap.js` takes the Qt key codes as an argument, which is what lets node run it, and a mode string that says which card the press landed on.
  Anything worth a test belongs in one of those rather than in QML, because the repo has no QML test harness: `Overlay.qml` cannot be instantiated outside the shell's plugin loader, so a standalone Quickshell config hangs on it.
  `ui/*.qml` alone is another matter: a throwaway Quickshell config whose root symlinks `Commons` and `Ui` from `/usr/share/omarchy/shell` draws a card under `QT_QPA_PLATFORM=offscreen`, which is how a layout is checked without the live shell.
  Offscreen still renders, so `card.grabToImage(function (r) { r.saveToFile(path) })` on a `Timer` that steps a phase and quits at the end gives one PNG per state, which is the cheapest way to actually look at a card.
  `QuickCard.maxCardHeight` is the whole bound of spec section 6; the card measures its own chrome and gives the rest to the scrolling text, so no part of the layout carries a guessed reserve.
  `docs/dev.md` is the only route onto a live desktop, including the manual smoke items and the Compose walkthrough.
  A plugin folder that is a symlink reloads as `docs/dev.md` step 4 says.
  A leaf card does run outside the shell: a scratch Quickshell config whose root directory holds `Commons` and `Ui` symlinks into `/usr/share/omarchy/shell` plus a `ui` symlink into the repo can instantiate `ComposeCard` or `QuickCard` in a `FloatingWindow`, which is the fastest way to see a layout change without installing the plugin.
- `ui/anchor.js` owns both answers the source window of spec section 3 gives: where the quick popup opens (`placeCard`) and where Replace types (`focusCommand`, `isFocused`).
  `Overlay.sourceWindow` is that one recorded fact, read by `hyprctl activewindow -j` before the capture, because the popup window itself takes the answer away.
  Omarchy answers `configProvider: lua`, so `hyprctl dispatch` reads Lua: the focus is `hl.dsp.focus({ window = "address:0x..." })` and never the `focuswindow address:<addr>` line of the `.conf` provider.
  That dispatch exits 0 for a window that is gone, so the `activewindow` check, not the exit status, is what lets the keystroke out.
  `hyprctl repl` is how to find any other dispatcher's Lua name and arguments.
  `cli/tests/overlay_anchor.rs` keeps `Overlay.qml` on those steps and in that order.
- `ui/capture.js` owns the freshness rule of spec section 3 (HUF-235) and the wording of the nothing-new state.
  The compositor keeps the primary selection, so `Overlay.lastCapturedText` and `lastCapturedWindow` are what say a capture is the one the last Check already ran on.
  `Overlay.consumeCapture` keeps that record and touches the compositor not at all; `Overlay.releasePrimary` is the one place that runs `wl-copy --primary --clear`, and it runs when the popup closes, never at capture time.
  A terminal drops its own highlight when it loses primary ownership and Replace pastes over that highlight, so `replacePending` holds the release back until the `wtype` keystroke is out.
  Step 2 has the same shape: a Ctrl + C that leaves the clipboard unmoved copied nothing, which `Capture.copiedNothing` decides.
  Nothing new is one card, so the capture no longer routes to `Errors.EMPTY_SELECTION`; that code stays for the CLI contract alone.
  `Overlay.clearCapture` is the Clear of spec section 6, and the one thing it must never touch is `draftText`.
  `ui/capture.test.js` counts the Checks a summon starts against a stub binary, and `cli/tests/overlay_capture.rs` keeps `Overlay.qml` on those same steps.
- `ui/errors.js` owns the whole route from the stdout of one Check to the card of spec section 8: `readCheck` reads the envelope and `card` builds the title, body, and buttons, so a node test can run a stub binary and read the card back.
  It carries its own copy of the per-engine timeout, because a Check that never answered leaves the shell nothing to read it from; `cli/tests/overlay_errors.rs` keeps that copy, the code list, and the no-re-capture promise of Retry in step.
  A test may never reach a real engine or touch the LanguageTool unit the live shell uses; a stub binary is the seam.
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
  Neither `qmllint` resolves `qs.*`, so neither catches a missing `import qs.Ui` or `import qs.Commons`: only loading the file finds that, which is the other reason to run a leaf card in a scratch config.

## Maintaining this file

Keep this file for knowledge useful to almost every future agent session in this project.
Do not repeat what the codebase already shows; point to the authoritative file or command instead.
Prefer rewriting or pruning existing entries over appending new ones.
When updating this file, preserve this bar for all agents and keep entries concise.
