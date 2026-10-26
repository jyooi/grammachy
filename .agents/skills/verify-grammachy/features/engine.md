# Engine

Engine lists, installs, and removes the optional LanguageTool component under HOME, with no password.
Settings, Engines in the popup runs these verbs for its download button, its Cancel, and its bin.

## Sub-features

- `engine-list` prints one row with the state, sizes, licence, and the launcher path.
- `engine-install` fetches the pinned archive, verifies its digest and size, and unpacks it.
- `engine-remove` deletes the installed tree, the archive, and any part file.
- `engine-partial` keeps a part file after a failed transfer and reports `partial`.

## How to get to it (user POV)

- Open Settings in the popup and press the download button or the bin on the LanguageTool row.
- Run `bin/grammachy engine list`, `engine install languagetool`, or `engine remove languagetool` in a terminal.

## Driving it with vg

Preconditions:

- `vg doctor` exits 0.
- `vg stub start` prints `stub ready: https://127.0.0.1:<port>`.

- **List before.** Run `vg run engine -- engine list`. Exit code `0`, `"state":"absent"`, and `directory` is the scratch engines directory.
- **Install.** Run `vg run engine -- engine install languagetool`. Exit code `0`, `"state":"ready"`, `"fromPackage":false`, and a `path` under the scratch engines directory.
- **Files after install.** Run `find "$(vg path engines)"`. The tree holds `languagetool/languagetool-server.jar` and no `.zip` or `.part` file.
- **Doctor agrees.** Run `vg run engine -- doctor --json | jq -r '.checks[] | select(.id == "languagetool") | .state'`. The answer is `installed`.
- **Remove.** Run `vg run engine -- engine remove languagetool`. Exit code `0` and `"state":"absent"`.
- **Files after remove.** Run `find "$(vg path engines)"`. Only the empty engines directory is left.
- **Partial.** Run `vg stub stop`, then `VG_ALLOW_DOWNLOAD=1 GRAMMACHY_ENGINE_BASE_URL=https://127.0.0.1:9 vg run engine -- engine install languagetool`. Exit code `1` and `"code":"download_failed"`. Then `vg run engine -- engine list` reports `"state":"partial"`, and `engine remove languagetool` clears it.

## Gotchas

- The installer allows HTTPS only. A plain HTTP stub fails with `Protocol "http" is disabled`.
- The stub jar is a text file. Never run a LanguageTool Check after a stub install, because the shared unit would try to run that file. The helper refuses that Check.
- A stub install proves the transfer, the digest check, the unpack, and the state reports. It does not prove the real host, the real 250 MB archive, or its free-space check.
- `GRAMMACHY_ENGINE_STOP=never` keeps `remove` away from the shared unit. Without it, `remove` stops `grammachy-languagetool` for the live shell.
- `"fromPackage":true` with `"state":"absent"` is correct when the Arch package is present. The package is the launcher until a Settings install exists.
- The Partial step sets `GRAMMACHY_ENGINE_BASE_URL` in the caller shell. `vg run` passes the caller environment through, so the variable reaches the CLI.
