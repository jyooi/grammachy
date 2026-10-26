---
name: verify-grammachy
description: Drive the Grammachy companion CLI (check, chunk, doctor, engine, setup) the way the overlay and a user do, with isolated state, and capture proof. Use to prove a Grammachy CLI change end to end, or to learn which popup steps stay manual on the live Omarchy desktop.
---

# Verify Grammachy

Grammachy has two user surfaces.
The driven surface is the Rust companion CLI at `cli/`.
The overlay in `Overlay.qml`, `BarWidget.qml`, and `ui/` calls that CLI for every Check.
The QML popup is the second surface, and an agent does not drive it.
Read [the popup feature file](features/popup.md) for the reason and for the manual steps.

All commands below use `vg.sh` as a short name for the helper.
Define it once in each shell:

```bash
vg() { "$(git rev-parse --show-toplevel)/.agents/skills/verify-grammachy/scripts/vg.sh" "$@"; }
```

Run `vg` from inside the checkout under test.
The helper finds the checkout from its own path, so a call from another directory also works with the full path.

## Isolation

`vg.sh run` gives every CLI call six variables that point all state into a scratch tree.

| Variable | What it isolates |
|---|---|
| `GRAMMACHY_SHELL_JSON` | The Settings entry, in place of `~/.config/omarchy/shell.json` |
| `GRAMMACHY_ENGINES_DIR` | Installed engines, in place of `~/.local/share/grammachy/engines` |
| `GRAMMACHY_BINDINGS_LUA` | The hotkey block, in place of `~/.config/hypr/bindings.lua` |
| `GRAMMACHY_MENU_JSONC` | The menu row, in place of `~/.config/omarchy/extensions/omarchy-menu.jsonc` |
| `GRAMMACHY_HYPRCTL_RELOAD=never` | `setup` does not run `hyprctl reload` |
| `GRAMMACHY_ENGINE_STOP=never` | `engine remove` does not stop a systemd unit |

Never run the CLI outside `vg.sh run`.
A bare call reads and writes the operator files under `~/.config/hypr` and `~/.config/omarchy`.

Two pieces of state cannot be isolated, so the helper refuses them by default.

- The LanguageTool unit is always `grammachy-languagetool`, and the live shell uses the same unit.
  A `check` on `languagetool` starts or reuses it, and it holds about 1 GB of memory.
  The helper refuses it unless `VG_ALLOW_SHARED_UNIT=1` is set with operator approval.
- A real `engine install` downloads about 250 MB from languagetool.org.
  The helper refuses it unless the stub runs or `VG_ALLOW_DOWNLOAD=1` is set.

Two runs from different checkouts never share state, because the scratch tree name comes from a hash of the checkout path.
Two runs from the same checkout share one scratch tree, so do not run them in parallel.

## Launch

There is no server to keep alive.
Each `vg.sh run` starts one short CLI process under a 60 second timeout.

1. Build the release binary once: `vg build`.
   It runs `cargo build --release` in `cli/` and prints `grammachy <version>`.
   A cold build takes about one minute.
2. Make a fresh scratch tree: `vg launch`.
   It prints `ready: scratch=<dir> evidence=<dir> binary=<path>`.
   The tree holds a `shell.json` with an empty Grammachy entry, so every Setting resolves to its default.

Set `VG_EVIDENCE=<dir>` to put the proof somewhere other than the default.
Set `VG_HOME=<dir>` to move both the scratch tree and the default evidence directory.
Set `VG_TIMEOUT=<seconds>` to change the timeout of one CLI call.

## Doctor

Run `vg doctor` first, and again whenever a result looks wrong.
It changes nothing.
It exits 1 and prints a `FAIL:` line when one of these is not true:

- The binary exists, and its version matches `cli/Cargo.toml`.
- No file under `cli/src` is newer than the binary.
- The scratch tree exists.

It also prints the commit, the evidence directory, the stub state, and the state of the shared unit.
It prints `live plugin: present` when the operator has the plugin installed.
That line is a warning, not a fault.

`vg doctor` checks the harness.
`grammachy doctor` is a product feature, and [its own feature file](features/doctor.md) covers it.

## Drive

```bash
vg run <feature-id> [--stdin <file> | --text <string>] -- <grammachy arguments>
```

The feature ID names the evidence directory, so use the file name of the feature in `features/`.
`--text` sends the string on stdin with no final newline.
`--stdin` sends a file.
With neither, stdin is empty.

The helper prints the command and the stderr on its own stderr, and the CLI stdout on its stdout.
So `vg run check --text "I has two book." -- check | jq .issues` works.
The helper exits with the exit code of the CLI, or 2 when it refuses a call.

To change a Setting, edit the scratch entry with `jq`:

```bash
f=$(vg path shell-json)
jq '.bar.layout.right[0].targetEnglish = "en-GB"' "$f" > "$f.new" && mv "$f.new" "$f"
```

`vg path` also prints `engines`, `bindings`, `menu`, `evidence`, and `scratch`.

The stub stands in for the release host of `engine install`.
`vg stub start` serves a small fake `LanguageTool-6.6.zip` over HTTPS on a free loopback port.
The installer allows only HTTPS, so the stub makes a self-signed certificate and passes it to curl through `CURL_CA_BUNDLE`.
It also pins the fake digest and size through `GRAMMACHY_ENGINE_SHA256` and `GRAMMACHY_ENGINE_SIZE_BYTES`.
`vg stub stop` ends it.

The feature map in [features/README.md](features/README.md) holds one recipe for each feature.

## Evidence

Each `vg run` writes six files under `<evidence>/<feature-id>/`, with a two-digit step number `NN`.

| File | Content |
|---|---|
| `NN.cmd` | The CLI arguments |
| `NN.stdin` | The exact bytes sent on stdin |
| `NN.stdout` | The envelope or report |
| `NN.stderr` | The CLI stderr |
| `NN.exit` | The exit code |
| `NN.meta` | Feature ID, commit, binary version, UTC time, and the isolation variables |

The default evidence directory is `${TMPDIR:-/tmp}/verify-grammachy-<hash>/evidence`.
`vg launch` and `vg doctor` print the exact path.

Proof standards:

- Drive the CLI with the arguments and stdin that the overlay or a user sends.
  Do not call library functions or test helpers.
- Capture the action and the resulting state.
  For a Check, keep the stdin and the envelope.
  For `setup` and `engine`, also keep a second read-only view, such as `engine list`, a `sha256sum`, or a `find`.
- Record side effects in files with `sha256sum`, `find`, or `cat`, in addition to the envelope.
- The stub replaces only the release host, which the CLI already isolates behind `GRAMMACHY_ENGINE_BASE_URL`.
  A stub install does not prove the real download, the real digest, or the real 2,032-member unpack.
- `GRAMMACHY_HYPRCTL_RELOAD=never` makes the `reload` step report `changed` with no reload.
  That step proves nothing about Hyprland.
- Report a refused or skipped step with the command and the reason.
  Do not report it as verified through another path.

## Cleanup

```bash
vg cleanup
```

It stops the stub through its recorded PID and deletes the scratch tree.
It keeps the evidence directory and prints its path.
Run it after every run, and also after a failed run.
`vg launch` refuses to start while a stub PID from an earlier run is still recorded.
The helper starts no other process that lives longer than one call.

## Other checks

Unit tests are not user paths, so this skill does not run them.
Section 16 of `docs/dev.md` lists `cargo test`, `qmllint`, and the `node --test` suite for `ui/`.
