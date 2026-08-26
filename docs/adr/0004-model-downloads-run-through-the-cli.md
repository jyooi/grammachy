---
status: accepted
---

# Model downloads run through the CLI, progress is polled, and Cancel is a signal

The Local LLM engine needs a weights file of several gigabytes, and until now the only way to get one was `grammachy setup` in a terminal.
That gave no progress, no cancel, and no way to delete a model again.
The Settings view now owns all three, and this is how.

The download runs in the CLI, not in the shell.
`grammachy model download <name>` spawns `curl` and hands back one JSON envelope, the same shape every other subcommand uses.
The shell starts that process and reads its stdout, which is what it already does for `check`, `chunk`, and `doctor`.

Progress is polled rather than streamed.
The CLI prints nothing at all while `curl` runs, and the shell runs `grammachy model list` once a second to read the length of the `.part` file.
The bar is that length against the pinned byte size of the row.

Cancel is a SIGTERM to the running process.
The CLI installs a handler that only sets a flag; the transfer loop reads the flag, kills its `curl` child, and exits 1 with code `cancelled`.
The `.part` file is left where it is, so the next Download resumes rather than restarts.

## Considered options

- **A progress line on stdout or stderr, read as it arrives.**
  It would give a smoother bar, but it breaks the one promise every subcommand makes: exactly one JSON object on stdout, and stderr for logs only.
  A shell that has to parse two kinds of output on one stream is a shell that has to guess which it is looking at.
- **The download in QML, through Quickshell.**
  There is no HTTP client in the plugin, and adding TLS to the shell to fetch a file the CLI already knows how to fetch puts the pinned digest in two places.
- **Killing the process to cancel.**
  `Process.running = false` sends SIGKILL, which leaves `curl` running as an orphan writing a file nobody waits for.
  The signal is what lets the CLI take its child down with it.
- **Deleting the `.part` file on cancel.**
  It makes Cancel simple and the resume impossible.
  A 5 GB transfer the user stopped at 80 percent is the case the whole feature exists for.

## Consequences

- The bar steps once a second rather than continuously, so it is animated across each step.
- One download runs at a time.
  Every other row's Download is disabled while one is in flight, because a second `curl` would halve the bandwidth of the first.
- Closing the overlay does not cancel a download: the process is the CLI's, and the overlay is only watching it.
- The CLI links `libc` for `statvfs` and `signal`.
  It was already in the dependency tree, so the binary does not grow.
- Every catalogue row is pinned twice, by sha256 and by byte size.
  The digest is what the rename checks; the size is what the free-space check and the bar measure against.
