# Setup

Setup writes the two trigger hotkeys and the Apps menu row with no password, and `setup --remove` takes both out again.
A user runs it once after install, and again after a change of `quickHotkey` or `composeHotkey`.

## Sub-features

- `setup-install` writes one hotkey block and one menu row between `grammachy begin` and `grammachy end` markers.
- `setup-idempotent` leaves both files unchanged on a second run.
- `setup-remove` puts both files back byte for byte.
- `setup-hotkeys` writes the stored `quickHotkey` and `composeHotkey` in place of the defaults.

## How to get to it (user POV)

- Run `bin/grammachy setup` from the plugin folder, as step 4 of the README Install says.
- Run `bin/grammachy setup --remove`, as step 1 of the README Uninstall says.

## Driving it with vg

Preconditions:

- `vg doctor` exits 0.
- Seed the scratch files with the repo fixtures. Run `cp cli/tests/fixtures/config/bindings.lua "$(vg path bindings)"` and `cp cli/tests/fixtures/config/omarchy-menu.jsonc "$(vg path menu)"`.
- Record the hashes. Run `sha256sum "$(vg path bindings)" "$(vg path menu)" > "$(vg path scratch)/setup-before.sha"`.

- **Install.** Run `vg run setup -- setup`. Exit code `0` and three steps, `hotkeys`, `reload`, and `menu`, each `changed`.
- **Files after install.** Run `cat "$(vg path bindings)"`. One block between `-- grammachy begin` and `-- grammachy end` binds `SUPER + SHIFT + Q` to the quick payload and `SUPER + ALT + Q` to the compose payload. The menu file holds one `apps.grammachy` row.
- **Idempotent.** Run `vg run setup -- setup | jq -c '[.steps[] | {name, state}]'`. `hotkeys` and `menu` read `unchanged`.
- **Remove.** Run `vg run setup -- setup --remove`, then `sha256sum -c "$(vg path scratch)/setup-before.sha"`. Both files report `OK`.
- **Stored hotkeys.** Store a new key, then install again. Run `f=$(vg path shell-json); jq '.bar.layout.right[0].quickHotkey = "SUPER + CTRL + G"' "$f" > "$f.new" && mv "$f.new" "$f"` and `vg run setup -- setup`. `grep 'SUPER + CTRL + G' "$(vg path bindings)"` finds the `hl.unbind` and `o.bind` lines.

## Gotchas

- The `reload` step always reads `changed` under `vg`, because `GRAMMACHY_HYPRCTL_RELOAD=never` skips `hyprctl reload`. It proves nothing about Hyprland.
- Never run `setup` outside `vg run`. A bare run edits `~/.config/hypr/bindings.lua` and reloads the operator compositor.
- A missing bindings file reads as empty, so a run on a fresh scratch tree also works. The fixtures make the byte-for-byte remove check meaningful.
- The `cp` lines in the preconditions use paths relative to the checkout root, so run them there.
