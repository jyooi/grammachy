---
status: accepted
---

# Fetch the non-commercial eval corpus at run time and redistribute none of it

The eval set needs a public learner corpus with a per-writer native-language label, and the CLC FCE dataset is the only one.
Its licence limits use to non-commercial research and caps any excerpt at 100 words.
The bench downloads the tarball at run time into a gitignored cache, and the repo commits no corpus text.
The committed sidecar holds ids, document and sentence index, offsets, and error codes only.
The stance is that the bench is research into which engine to recommend, and Grammachy is free software under MIT.
Grammachy is not sold, offered for sale, licensed for money, leased, or rented.

## Considered options

- Commit the corpus sentences with the code: one file to read and no network step, but the repo would redistribute licensed text.
- Write the whole eval set by hand: no licence question at all, but no native-language label from a real writer and months of work.
- Ask Cambridge for written permission: the clearest record, but weeks of delay for a developer tool that ships no corpus text.
- Fetch at run time and commit a selection sidecar: the repo stays clean, and every string in it is the project's own.

## Consequences

- The repo carries a MIT `LICENSE` file, so the free-software half of the stance is on record.
- A commercial fork must delete the fetch step.
- Without the cache the eval tables are skipped with a reason, so a clean clone still runs the bench.
- The benchmark file prints missed item ids only, and sentence, fix, and model output text live in the gitignored record file.
- The fetch step prints the licence path and the non-commercial line to stderr on the first fill.
- The residual risk is a takedown request, whose cost is one deleted fetch step.
- The fallback is a hand-written per-language set in the same item shape, so metrics, runner, and tables do not change.
