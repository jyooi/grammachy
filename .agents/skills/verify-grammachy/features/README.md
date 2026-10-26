# Grammachy verification map

This directory is the maintained source for verification of the user-facing behavior of Grammachy.
Read this index before you drive the CLI, then use the matching feature file as the recipe.

## Baseline preconditions

- Define the `vg` shell function from `SKILL.md`.
- Run `vg build`, then `vg launch`, from the checkout under test.
- Require `vg doctor` to exit 0 before the first step.
- The scratch `shell.json` holds an empty Grammachy entry, so Engine is `harper`, Target English is `en-US`, and Native language is `none`.
- Never drive the CLI outside `vg run`, and never drive the live popup.

## Driving conventions

- Start every recipe from a fresh `vg launch` unless its preconditions say otherwise.
- Pass the feature ID, which is the file name without `.md`, as the first argument of `vg run`.
- Treat every command as literal. Keep quoted text and flags unchanged.
- Pipe the stdout of `vg run` into `jq` to read one field. The helper prints its own lines on stderr.
- Reset a changed Setting with `vg launch` instead of a manual edit.

## Proof and skip reporting

- Keep the `NN.cmd`, `NN.stdin`, `NN.stdout`, `NN.stderr`, `NN.exit`, and `NN.meta` files of every step.
- Pair a write with a second read-only view, such as `engine list`, `sha256sum`, or `find`.
- Record the feature ID and sub-feature ID with every artifact.
- Report a refused step with the exact `vg:` message and the unmet precondition.
- Do not report a manual popup step as verified through the CLI.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph about the user-visible behavior.
It then has exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with vg` starts with `Preconditions:` and pairs each user action with an exact command and an observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

## Features

- [Check](./check.md) covers one Check of a Selection, the error envelopes, and the Settings order.
- [Chunk](./chunk.md) covers how a long Draft splits into Chunks for Compose.
- [Doctor](./doctor.md) covers the install report in text and JSON, and its exit code.
- [Engine](./engine.md) covers list, install, and remove of the optional LanguageTool component.
- [Setup](./setup.md) covers the hotkey block and the menu row, and their removal.
- [Popup](./popup.md) covers the quick popup and the Compose card, which stay manual.
