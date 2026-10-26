# Doctor

Doctor reports, one line for each piece, what this machine still needs for the chosen engine, and it installs nothing.
The overlay reads its JSON for the setup card, the Engines list, and the diagnosis line of the `engine_unavailable` card.

## Sub-features

- `doctor-text` prints the pieces, the dependency table, and the engine answer.
- `doctor-json` prints the same report as one JSON object on one line.
- `doctor-exit` exits 0 when the chosen engine is ready and 1 when a piece is missing.
- `doctor-state` names how LanguageTool came to this machine: `installed`, `package`, or `absent`.

## How to get to it (user POV)

- Run `bin/grammachy doctor` in a terminal, as the README tells a user to do.
- Open the `engine_unavailable` card in the popup. Its diagnosis line comes from `doctor --json`.
- Open Settings, Engines in the popup. The dependency rows come from `doctor --json`.

## Driving it with vg

Preconditions:

- `vg doctor` exits 0.
- The scratch engines directory is empty, which `vg launch` makes sure of.

- **Text report.** Run `vg run doctor -- doctor`. Exit code `0`, a first line `Grammachy doctor`, an `ok` line for `Grammachy CLI` with the binary path of this checkout, a `Dependencies` table, and `Engine harper is ready.`.
- **JSON report.** Run `vg run doctor -- doctor --json`. One line of JSON with `"contractVersion":1`, `"engine":"harper"`, and `"ready":true`.
- **Check IDs.** Run `vg run doctor -- doctor --json | jq -r '[.checks[].id] | join(",")'`. The answer is `binary,languagetool,java,unit:languagetool`.
- **Dependency rows.** Run `vg run doctor -- doctor --json | jq -c '[.dependencies[] | {package, required, present}]'`. Four rows in the order `curl`, `wl-clipboard`, `libarchive`, `jre-openjdk`.
- **LanguageTool state.** Run `vg run doctor -- doctor --engine languagetool --json | jq -r '.ready, (.checks[] | select(.id == "languagetool") | .state)'`. The state word matches the machine. It reads `package` when the Arch package is present and `absent` when it is not.
- **Installed state.** Install the stub engine as the engine recipe shows, then run the same command. The state word is `installed`.

## Gotchas

- The `languagetool` state depends on the machine. A tree in the scratch engines directory reads `installed`. Without one, the Arch package reads `package`, and a machine with neither reads `absent`.
- `doctor --engine languagetool` exits 1 on a machine with no LanguageTool and no Java. That is correct, not a fault of the run.
- The `LanguageTool unit` line reads the state of the shared unit. It changes when the live shell runs a LanguageTool Check.
- Doctor runs `systemctl --user is-active` with no timeout of its own. The 60 second timeout of `vg run` bounds it.
