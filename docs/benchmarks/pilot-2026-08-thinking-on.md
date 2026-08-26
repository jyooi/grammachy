# Grammachy benchmark 0.1.0

Fixture: 30 interference sentences and 10 correct ones, `cli/tests/fixtures/interference-30.json`.
Machine: 16 GB tier, 24 CPUs, 27 GB RAM.
Command: `grammachy bench --engine openai --model gemma-4-e4b-it --model qwen3.5-4b --cloud-model deepseek/deepseek-v4-flash-0731 --cloud-model google/gemini-3.7-flash --max-cost 10`.

## Engines

| Engine | Catch rate | False positives | p50 latency | Resident memory |
|---|---|---|---|---|
| `languagetool` | 10 of 30 (33.3%) | 0 of 10 | 21 ms | 498 MB |
| `harper` | 5 of 30 (16.7%) | 0 of 10 | 14 ms | 303 MB |
| `openai` | 29 of 30 (96.7%) | 0 of 10 | 15777 ms | 1.4 GB |

Resident memory of `languagetool` is the RSS of its server process.
Resident memory of `harper` is the growth of this process's own peak RSS, because it runs in process.
Resident memory of `openai` is the RSS of its server process.

## Models

### Quality

| Model | Catch rate | Precision | Recall | F0.5 | Exact fix | False positives | Style creep | Valid |
|---|---|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | 30 of 30 (100.0%) | 30 of 31 (96.8%) | 30 of 30 (100.0%) | 97.4% | 26 of 30 (86.7%) | 0 of 10 | 3.3 | 40 of 40 (100.0%) |
| `qwen3.5-4b` | 0 of 30 (0.0%) | 0 of 0 (0.0%) | 0 of 30 (0.0%) | 0.0% | 0 of 30 (0.0%) | 0 of 10 | 0.0 | 40 of 40 (100.0%) |
| `deepseek/deepseek-v4-flash-0731` | 28 of 30 (93.3%) | 26 of 28 (92.9%) | 26 of 30 (86.7%) | 91.5% | 21 of 30 (70.0%) | 0 of 10 | 6.7 | 40 of 40 (100.0%) |
| `google/gemini-3.7-flash` | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 29 of 30 (96.7%) | 96.7% | 27 of 30 (90.0%) | 0 of 10 | 3.3 | 39 of 40 (97.5%) |

### Cost

| Model | p50 latency | p95 latency | Resident memory | Cost per 1,000 Checks | Weights license | Recommended |
|---|---|---|---|---|---|---|
| `gemma-4-e4b-it` | 16717 ms | 71347 ms | 1.6 GB | 0.00 (local) | Apache-2.0 | recommended |
| `qwen3.5-4b` | 41014 ms | 41377 ms | 3.2 GB | 0.00 (local) | Apache-2.0 | eligible |
| `deepseek/deepseek-v4-flash-0731` | 6950 ms | 16788 ms | not measured | 0.01 USD | hosted | eligible |
| `google/gemini-3.7-flash` | 2918 ms | 11727 ms | not measured | n/a | hosted | recommended cloud model |

Wall time of `gemma-4-e4b-it`: 1051 s for the whole fixture, server start included.
Wall time of `qwen3.5-4b`: 1586 s for the whole fixture, server start included.
Wall time of `deepseek/deepseek-v4-flash-0731`: 284 s for the whole fixture.
Wall time of `google/gemini-3.7-flash`: 141 s for the whole fixture.
Cloud spend of this run: 0.0144 USD of the 10 USD cap.

### Recall by native language

| Model | zh | ms | fr | es |
|---|---|---|---|---|
| `gemma-4-e4b-it` | 8 of 8 | 8 of 8 | 7 of 7 | 7 of 7 |
| `qwen3.5-4b` | 0 of 8 | 0 of 8 | 0 of 7 | 0 of 7 |
| `deepseek/deepseek-v4-flash-0731` | 5 of 8 | 8 of 8 | 6 of 7 | 7 of 7 |
| `google/gemini-3.7-flash` | 8 of 8 | 8 of 8 | 6 of 7 | 7 of 7 |

Recommended local model, the Settings default and the README line: `gemma-4-e4b-it`.
Recommended cloud model, the `openrouterModel` line of the README: `google/gemini-3.7-flash`. Cloud is never the default engine.
Ranking: exact fix rate, then F0.5, then lower p50 (HUF-205). Floors: validity at least 95% and no more false positives than the default engine, `languagetool`, which earned 0. A recommended local model must also fit the machine tier above (spec section 13.1).

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
- Cost per 1,000 Checks: the sum of `usage.cost` over the row divided by its Checks, times 1,000. Local rows cost nothing per Check.
- Every sentence is checked with the Native language the fixture records for it, which is what the shell passes on a real Check.

