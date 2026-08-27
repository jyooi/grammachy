# Project agent memory

This file is the project's committed home for project-intrinsic agent knowledge: build, test, release, architecture, and sharp-edge notes that should travel with the code.

- `docs/spec/v1.md` is the authority on every contract.
  Section 5.1 fixes the `grammachy check` JSON envelope.
  Section 5.2 fixes the `grammachy chunk` JSON envelope.
  Section 10 is the packaging, section 11 the repository layout, section 13 the test plan.
  `docs/spec/evals.md` is the authority on the eval sets, the metrics, the `bench` runner, the `openrouter` cloud engine, and the recommendation rules.
  It amends v1 sections 1, 4, 5.2, 6, 7, 10, 11, and 13.1.
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
  `GRAMMACHY_LLAMA_START=never` stops a start and never a connection.
  The default `openaiBaseUrl` is `127.0.0.1:8080`, which is a real llama-server on a developer machine.
  Every test settings file must name a silent `openaiBaseUrl` of its own.
  `cli/tests/bench.rs` adds one for any entry body that does not.
  The `openai` adapter takes its starter as a value, so `cli/tests/openai_stub.rs` covers the start behaviour with no systemd at all.
  A test that runs the binary must also point `openaiBaseUrl` away from the default `127.0.0.1:8080`, because a developer machine may answer there.
  `cli/tests/bench.rs` writes a dead address into every settings file for that reason.
  Live tests in `languagetool_live.rs`, `openai_live.rs`, and `interference_catch_rate.rs` skip when their port is silent, which keeps CI green without the packages.
- llama.cpp binds its port before it has read the weights.
  Until they are loaded it answers HTTP 503, which is minutes for a 5 GB file.
  `openai/mod.rs` maps that one status to `engine_unavailable`.
  `start_and_retry` then waits it out rather than failing the first Check of a session.
- The `openai` base URL host must be loopback, and `cli/src/engines/openai/endpoint.rs` is the only place that decides it.
  A remote host is `bad_arguments` and no request is made; that is a product guarantee, so keep it tested.
  Its prompt in `prompt.rs` is the wording HUF-181 measured, and the "shortest exact substring" rule is what makes the spans usable rather than whole-sentence rewrites.
  Thinking (spec section 4) travels on the request as `chat_template_kwargs.enable_thinking` and never on the unit.
  That is what makes a change of the Setting need no restart.
  The unit only bounds and routes the think, with `--reasoning-budget` and `--reasoning-format deepseek`.
  That format keeps the think in `message.reasoning_content`.
  `response::parse_array` drops a leading think anyway, because `openaiBaseUrl` may name a server this adapter did not start.
  The default lives twice, in `settings::DEFAULT_LOCAL_THINKING` and in the `localThinking` descriptor of `ui/settings.js`.
  `cli/tests/overlay_thinking.rs` keeps the two equal and keeps the Toggle inside the group the engine hides.
- `grammachy model` lives in `cli/src/model/`, spec section 5.3: `list`, `download`, and `remove` for the Local LLM weights, plus the `ensure` that `setup` still calls.
  `setup/model.rs` moved here.
  `setup/mod.rs` calls `model::ensure` and owns nothing about weights any more.
  The catalogue is `mod.rs` and every row is pinned twice, by sha256 and by byte size.
  Both numbers are the `x-linked-etag` and `x-linked-size` of an unauthenticated Hugging Face request.
  A row belongs there only when that request answers 200 without a token.
  The three verbs agree on one pair of paths, the row's pinned file name and its `.part`.
  So a hand-placed `.gguf` is never listed and never deleted.
  The licence of a row comes from `bench::weights::of`, the one product rule of spec section 13.1.
  `cancel.rs` is the whole cancel.
  The SIGTERM handler only sets a flag, and `curl` polls it so the child dies and the `.part` file stays.
  Seams are `GRAMMACHY_MODELS_DIR`, `GRAMMACHY_MODEL_BASE_URL`, `GRAMMACHY_MODEL_SHA256`, `GRAMMACHY_MODEL_SIZE_BYTES`, `GRAMMACHY_LLAMA_STOP`, plus the `Downloader` and `Stopper` values.
  `GRAMMACHY_MODEL_SIZE_BYTES` is what lets a test drive the transfer without the gigabytes of free disk the pinned size asks for.
  `cli/tests/model_download.rs` and `cli/tests/model_cancel.rs` each own their whole binary, because one sets a digest for the process and the other takes the signal disposition over.
- `grammachy setup` lives in `cli/src/setup/`, spec section 10.
  It prints one JSON envelope (`SetupEnvelope`).
  Exit 1 uses `setup_failed`.
  `block.rs` owns the marked block both configuration files carry and the rule that makes `--remove` byte exact: the region always carries the newline on each side, so insertion and removal are the same substring.
  `bindings.rs` holds the two `hl.unbind` plus `o.bind` pairs of spec section 2 and the `hyprctl reload`; the file is `bindings.lua`, because Omarchy answers `configProvider: lua` and never reads the `.conf` files beside it.
  `menu.rs` holds the `grammachy.compose` row, which names `"parent": "root"` because nothing else creates a `grammachy` submenu.
  The weights step calls `model::ensure`, which downloads with `curl`, the tool `bin/bootstrap.sh` uses, because `curl` resumes an interrupted multi-gigabyte transfer.
  A failed model step still writes the hotkeys and menu.
  Hardware tiers only name the llama.cpp backend packages, because the weights file is the same on both (spec section 4).
  Every path and both side effects are seams: `GRAMMACHY_BINDINGS_LUA`, `GRAMMACHY_MENU_JSONC`, `GRAMMACHY_HYPRCTL_RELOAD=never`, plus the `Reloader` value and the `cli/src/model/` seams above.
  No test may touch a real config file, a real compositor, or the real weights host.
- Settings resolve in `cli/src/settings.rs`: flags, then the plugin entry in `$HOME/.config/omarchy/shell.json`, then the defaults of spec section 7.
  The product path is that HOME path only. The CLI does not read `$XDG_CONFIG_HOME`.
  The entry is looked up by plugin id in `bar.layout.{left,center,right}` first and in the top level `plugins` array next, the order `shell.qml` writes them in.
  `GRAMMACHY_SHELL_JSON` is the test seam; no test may read or write the real file.
- `grammachy bench` in `cli/src/bench/` is the one subcommand that prints Markdown on stdout rather than a JSON envelope; arguments that describe no run still print the error envelope.
  A `--record` write that fails after the rows ran prints the report and exits 1, because the run already paid for those numbers.
  One run is the whole benchmark file: `grammachy bench ... > docs/benchmarks/<version>.md`, nothing added by hand.
  `--engine openai --model <name>` fills the Models table and does not narrow the Engines table.
  An engine the machine cannot reach is a skipped row, never an error, so a machine without llama.cpp still produces a valid file.
  `cli/src/bench/weights.rs` is the product rule for which models may be recommended (`docs/spec/evals.md` section 5).
  `openrouter` is the cloud engine in `cli/src/engines/openrouter/`, and it reuses the `openai` request, prompt, and mapping.
  It is why the binary carries a TLS stack: `ureq` runs with the `rustls` feature for it.
  A cloud row needs `--max-cost <usd>`, the cap on the whole run, and the flag is refused when no cloud row runs.
  `Spend` in `cli/src/bench/mod.rs` owns both ways a cloud row ends and what the report prints as the run's spend, because a row the cap ended carries no tally.
  A cloud answer with no `usage.cost` ends its row and every later cloud row, because a run that cannot measure its spend cannot hold the cap.
  `--record <dir>` writes `checks.json`, one entry per engine, model, and item, which the judge of a later ticket reads.
  `Plan::of` proves the directory holds that file before the first row, so a directory the run cannot write never discards a report it already paid for.
  The run writes `checks.json.pending` and renames it, so the record of an earlier run stays whole until this run has one of its own.
  That file is gitignored, because it is the only place model output text lands.
  `cli/src/bench/fixture.rs` is the one loader and `cli/src/bench/metrics.rs` is the one metrics module.
  Both sets share the item shape `{ id, native, text, edits[], expected_text }` of the evals spec.
  Every metric of that spec has a unit test in `metrics.rs` that runs from recorded answers, so no test needs a live model.
  `cli/tests/bench.rs` must seam every server the run can reach, LanguageTool and the OpenAI base URL both.
  The OpenAI default is a fixed loopback port, so a machine that already runs llama.cpp there answers a case meant to find nothing.
- `doctor` reports the install state and the one-line engine diagnosis the `engine_unavailable` card shows.
  `docs/doctor.md` documents its envelope, exit code, and hardware tiers.
  `cli/src/doctor/facts.rs` is the only place that reads the machine, so the report is a pure function of recorded `Facts` and no test reads real hardware.
  The `backend` check reads the library names under `/usr/lib/ggml`, because `llama-cpp` carries no compute backend of its own.
  A server without one starts and then answers nothing, which reads as a broken engine rather than a missing package.
  `ggml-cpu` is the requirement and `ggml-vulkan` is the accelerator, so a missing `ggml-cpu` fails the check and a missing `ggml-vulkan` is only a note.
  `HardwareTier::backend_packages` is the single rule the llama.cpp remedy, the backend remedy, the human footer, and the `backendPackages` field all read.
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
- `manifest.json` version must equal the crate version; `cli/tests/manifest.rs` enforces that.
  The Check size limit belongs to the Engine: `EngineSlug::check_limit_utf16` in `cli/src/args.rs` is the Rust authority and `ui/limits.js` is the shell copy.
  `check` and `chunk` both take that limit as an argument, so `chunk` has its own `--engine` and packs to the same number the Check will refuse at.
  The Draft cap `chunk::MAX_DRAFT_UTF16_UNITS` is one number and lives twice, in Rust and in the QML that refuses an oversize Draft.
  `cli/tests/overlay_limit.rs` keeps every copy equal.
- The Omarchy plugin is the repo root: `manifest.json`, `BarWidget.qml`, `Overlay.qml`, and `ui/`.
  `Overlay.qml` owns capture, the CLI run, the key map dispatch, the Apply path, the review state, the Draft, and the settings storage.
  Every QML file in `ui/` only draws.
  `root.surface` is `"quick"` or `"compose"` and is what routes a summon (spec section 2); both surfaces share one `phase`, one Check, one review state, and one key map, so a change to either belongs in `Overlay.qml` rather than in a card.
  `Overlay.keyMode` is where a new `phase` has to be named, or its card silently inherits the review keys.
  `ui/QuickCard.qml` and `ui/ComposeCard.qml` are the two surfaces; `ui/CardHero.qml`, `ui/Inspector.qml`, `ui/ReviewCounts.qml`, `ui/MarkedText.qml`, `ui/ErrorCard.qml`, `ui/SettingsView.qml`, and `ui/ModelsView.qml` are shared parts, so a change to the hero, the inspector, or the counts reaches both at once.
  `ui/splice.js`, `ui/tokens.js`, `ui/keymap.js`, `ui/format.js`, `ui/settings.js`, `ui/errors.js`, `ui/models.js`, `ui/anchor.js`, and `ui/limits.js` are loaded by QML and by node, so they may use neither's API.
  Their `*.test.js` siblings run under `node --test`; add a new one to `.github/workflows/ci.yml` and to `docs/dev.md`.
  `keymap.js` takes the Qt key codes as an argument, which is what lets node run it, and a mode string that says which card the press landed on.
  Anything worth a test belongs in one of those rather than in QML, because the repo has no QML test harness: `Overlay.qml` cannot be instantiated outside the shell's plugin loader, so a standalone Quickshell config hangs on it.
  `ui/*.qml` alone is another matter: a throwaway Quickshell config whose root symlinks `Commons` and `Ui` from `/usr/share/omarchy/shell` draws a card under `QT_QPA_PLATFORM=offscreen`, which is how a layout is checked without the live shell.
  Offscreen still renders, so `card.grabToImage(function (r) { r.saveToFile(path) })` on a `Timer` that steps a phase and quits at the end gives one PNG per state, which is the cheapest way to actually look at a card.
  `QuickCard.maxCardHeight` is the whole bound of spec section 6; the card measures its own chrome and gives the rest to the scrolling text, so no part of the layout carries a guessed reserve.
  `docs/dev.md` is the only route onto a live desktop, including the manual smoke items and the Compose walkthrough.
  A plugin folder that is a symlink reloads as `docs/dev.md` step 4 says.
  A leaf card does run outside the shell: a scratch Quickshell config whose root directory holds `Commons` and `Ui` symlinks into `/usr/share/omarchy/shell` plus a `ui` symlink into the repo can instantiate `ComposeCard` or `QuickCard` in a `FloatingWindow`, which is the fastest way to see a layout change without installing the plugin.
- The Models list of spec sections 5.3 and 7 is `ui/models.js` plus `ui/ModelsView.qml`, embedded by `SettingsView.qml` and shown for the `openai` engine only.
  `models.js` owns the envelope reader, the row state rule, the byte formatting, the hint line, and the row buttons.
  `ui/models.test.js` runs the whole route against a stub binary that answers all three verbs.
  `Overlay.qml` owns the two processes and the one-second `modelPoll`.
  The CLI prints nothing while curl runs, so the `.part` length `model list` reports is the only progress there is.
  Cancel is `modelActionProcess.signal(15)` and never `running = false`, which would orphan curl.
  `resetRun` leaves `models`, `modelBusy`, `modelActionProcess`, and `modelPoll` alone, because closing the overlay must not cancel a download.
  It does drop an open confirm, the spec section 7 rule that hiding the list answers the question with Keep.
  `Overlay.modelsBusy` is the one fact the list draws its disabled buttons from: any verb in flight, including an open confirm.
  `confirmModel` is a `phase` with its own `Overlay.keyMode` entry, and `cli/tests/overlay_models.rs` keeps all of that in step.
  [ADR 0004](docs/adr/0004-model-downloads-run-through-the-cli.md) records why the download lives in the CLI.
- `ui/anchor.js` owns both answers the source window of spec section 3 gives: where the quick popup opens (`placeCard`) and where Replace types (`focusCommand`, `isFocused`).
  `Overlay.sourceWindow` is that one recorded fact, read by `hyprctl activewindow -j` before the capture, because the popup window itself takes the answer away.
  Omarchy answers `configProvider: lua`, so `hyprctl dispatch` reads Lua: the focus is `hl.dsp.focus({ window = "address:0x..." })` and never the `focuswindow address:<addr>` line of the `.conf` provider.
  That dispatch exits 0 for a window that is gone, so the `activewindow` check, not the exit status, is what lets the keystroke out.
  `hyprctl repl` is how to find any other dispatcher's Lua name and arguments.
  `cli/tests/overlay_anchor.rs` keeps `Overlay.qml` on those steps and in that order.
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
