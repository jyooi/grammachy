# Grammachy benchmark 0.1.0

Fixture: 30 interference sentences and 10 correct ones, `cli/tests/fixtures/interference-30.json`.
Machine: 16 GB tier, 24 CPUs, 27 GB RAM.
Command: `grammachy bench --engine openai --model gemma-4-e4b-it --thinking on`.

## Engines

| Engine | Catch rate | False positives | p50 latency | Resident memory |
|---|---|---|---|---|
| `languagetool` | 10 of 30 (33.3%) | 0 of 10 | 21 ms | 484 MB |
| `harper` | 5 of 30 (16.7%) | 0 of 10 | 13 ms | 303 MB |
| `openai` | 29 of 30 (96.7%) | 0 of 10 | 13935 ms | 2.1 GB |

Resident memory of `languagetool` is the RSS of its server process.
Resident memory of `harper` is the growth of this process's own peak RSS, because it runs in process.
Resident memory of `openai` is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.

## Models

### Quality

| Model | Catch rate | Precision | Recall | F0.5 | Exact fix | False positives | Style creep | Valid |
|---|---|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 30 of 30 (100.0%) | 100.0% | 27 of 30 (90.0%) | 0 of 10 | 0.0 | 40 of 40 (100.0%) |

### Cost

| Model | Thinking | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |
|---|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | on | 14570 ms | 32206 ms | 2.2 GB | 0.00 (local) | Apache-2.0 | recommended |

Thinking is the local mode the row ran in, from `--thinking`. A cloud row prints `-`: the mode is a llama.cpp chat-template argument and never reaches a provider.
Resident memory of `gemma-4-e4b-it` is the device memory its server process holds, read from the DRM fdinfo of that process rather than from its RSS.
Wall time of `gemma-4-e4b-it`: 654 s for the whole fixture.

### Throughput

| Model | Time to first token (p50) | Output tokens per second | Output tokens per Check (p50) | Output tokens per Issue |
|---|---|---|---|---|
| `gemma-4-e4b-it` | 547 ms | 24.8 | 338 | 514.2 |

Time to first token and the token rate come from the model server's own timings. A rate marked `whole request` is output tokens over the request time as seen from this machine, network included, because the provider reports no timings.
Output tokens per Issue is the output tokens of the row over the Issues the same Checks answered, so it prices one Issue rather than one Check.

### Recall by native language

| Model | zh | ms | fr | es |
|---|---|---|---|---|
| `gemma-4-e4b-it` | 8 of 8 | 8 of 8 | 7 of 7 | 7 of 7 |

### Chunk

| Model | Thinking | Wall time | Valid | Recall |
|---|---|---|---|---|
| `gemma-4-e4b-it` | on | 444 s | 7 of 7 (100.0%) | 74 of 188 (39.4%) |

Every local row checks one Draft per native language from `cli/tests/fixtures/chunks/`, each a few paragraphs at the Check size limit of the local engine. The table is a gate rather than a ranking: it says whether a whole Compose Chunk comes back inside the timeout.

Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`, with thinking on.
Ranking: exact fix rate, then F0.5, then lower p50 (HUF-205). Floors: validity at least 95% and no more false positives than the default engine, `languagetool`, which earned 0. A recommended local model must also fit the machine tier above (`docs/spec/evals.md` section 5).

## Skipped

Every engine and model of this run was reachable.

## Regression rule

A release must not drop the catch rate of the default engine, `languagetool`, and must not raise its false positives, against the previous file in `docs/benchmarks/`. A row that was skipped in one file and measured in the next is a new measurement, not a regression.

## How the numbers are measured

- Catch rate: an interference sentence is caught when at least one Issue overlaps a span the fixture expects. A right span with a wrong Fix still counts, because the Panel shows the span and lets the user Skip the Fix.
- Precision, recall, F0.5: an Issue pairs with the first unpaired expected edit it overlaps, provided it reaches no more than three words past the edit on either side. Precision is pairs over Issues, recall is pairs over expected edits, both over the whole fixture.
- Exact fix: every Fix of the Check applied to the sentence equals the corrected sentence the fixture holds, after collapsing runs of whitespace.
- False positives: correct sentences that earned at least one Issue. One sentence counts once, however many Issues it earned.
- Style creep: unpaired Issues on interference sentences, per 100 interference sentences.
- Valid: Checks that returned a result. An invalid Check counts as zero Issues, so a miss, and stays out of precision, exact fix, and latency.
- p50 and p95 latency: nearest rank over the valid Checks of the fixture, correct sentences included, measured in process around one Check.
- Cost per 1,000 Checks: the sum of `usage.cost` over the row divided by the number of Checks that reported a cost, times 1,000. A cloud answer that reports no cost ends its row as skipped, because the run cannot then measure what it spends. A cloud row where no Check answered prints `n/a`. Local rows cost nothing per Check.
- Every sentence is checked with the Native language the fixture records for it, which is what the shell passes on a real Check.
- Thinking: the mode `--thinking` gave the local rows. `both` runs every local model twice, once in each mode. The Engines table's `openai` row runs once, in the mode the flag names, and under `both` in the product default. A cloud row prints `-`.

