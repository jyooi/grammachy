---
status: accepted
---

# CLI in Rust, shipped as a pinned release binary inside the plugin folder

The companion CLI is thin glue between the Quickshell panel and the engines, and STACK.md would make TypeScript + Effect on bun the default.
We chose Rust because the CLI must run on a marketplace user's machine with no runtime installed: one 13 MB static binary against a 95 MB bun-compiled binary or a pacman dependency on bun.
An Omarchy plugin install is a plain `git clone` with no hook, so the binary is not built on the user's machine and not committed to git.
CI builds `grammachy-x86_64-linux` on every tag, the repo pins its sha256 in `cli.lock`, and `bin/bootstrap.sh` downloads and verifies it into the gitignored `bin/grammachy` when the panel's setup card is clicked.
Developers replace the download with `cargo build --release` and a copy.

## Considered options

- TypeScript + Effect run from source with bun from pacman: 100 ms start, readable diffs on `omarchy plugin update`, but a runtime dependency the plugin cannot install.
- TypeScript + Effect compiled with `bun build --compile`: 95 MB per release, too large to commit or to download on every update.
- Binary committed in git: zero steps after the clone, but 13 MB per release in history and a binary blob in every update review.
- `cargo build` on the user's machine: a 500 MB toolchain and a 90 s build for a marketplace user.

## Consequences

- Harper runs in process through `harper-core`, loaded only when the engine setting is `harper` (500 ms dictionary load).
- HTTP is blocking through `ureq`, no async runtime. The Compose window ticket may revisit this if it needs parallel chunk checks in one run.
- LanguageTool and llama.cpp run as transient user units through `systemd-run --user`, so plugin removal leaves no unit files behind.
- Every release needs two commits: the tag that CI builds, then the `cli.lock` bump with the new hash.
