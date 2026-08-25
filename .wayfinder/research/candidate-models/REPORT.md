# HUF-204: Candidate model ids for the Vulkan tier and for OpenRouter

Date: 2026-08-25.
Wayfinder map: HUF-202 (parent), HUF-170.
Builds on: HUF-181 small model benchmark (`research/small-model-benchmark`), HUF-183 API model benchmark (`../api-model-benchmark/`).

## Question

Which exact model ids, files, and prices do the candidate families resolve to on 2026-08-25, and which further small local models deserve a row?

## Answer in short

- Local: `unsloth/gemma-4-E4B-it-GGUF` `gemma-4-E4B-it-Q4_K_M.gguf` (4.98 GB, Apache-2.0) stays the only local row that is both eligible and measured at 29 of 30.
  The file and digest in `cli/src/setup/model.rs` match the live Hugging Face LFS oid.
- `cli/src/bench/weights.rs` is stale for Gemma 4.
  It maps every `gemma-*` name to "Gemma Terms of Use, Restricted", but Google publishes Gemma 4 under Apache-2.0.
  So `weights::of("gemma-4-e4b-it")` answers "no, the license is neither Apache-2.0 nor MIT" for the model the spec already recommends.
  A `gemma-4` row with `Terms::Permissive` above the `gemma` row fixes it, and the test `a_license_that_allows_commercial_use_but_is_not_apache_or_mit_is_not_eligible` needs a different fixture.
- "Qwen 3.8 small" does not exist.
  Qwen3.8 ships as 27B dense and 2.4T-A95B only.
  The newest Qwen generation with a small dense instruct model is still Qwen3.5 (0.8B, 2B, 4B, 9B, from 2026-02).
- Muse Glimmer is `meta-models/Muse-Glimmer-30B`, a 29.6B dense model from Meta Superintelligence Lab, Apache-2.0, with a 15.9 GB 4-bit GGUF.
  It is outside the 8 GB tier by a factor of two and is a reference row only.
- Phi-4-mini has no successor.
  Microsoft's newest small text model is still `microsoft/Phi-4-mini-instruct` (2025-02, MIT), which HUF-181 measured at 18 of 30.
- Nothing under 4 GB changed since HUF-181.
  The only small model released since is `LiquidAI/LFM2.5-2.6B` (2026-08-01), and its LFM Open License v1.0 is neither Apache-2.0 nor MIT.
- Cloud: the OpenRouter catalogue moved since HUF-183.
  DeepSeek V4 Flash 0731 is now 0.0616 / 0.1232 per million (was 0.14 / 0.28), and `anthropic/claude-sonnet-5` and `openai/gpt-5.6-luna` are new.
  `google/gemini-3.7-flash` cannot switch reasoning off (`mandatory: true`), and Sonnet 5 plus every GPT-5.x row drop `temperature`.

## Method

- Hugging Face: `GET https://huggingface.co/api/models?search=...&sort=downloads` for discovery, then `GET https://huggingface.co/api/models/<repo>?blobs=true` for licence, GGUF metadata (architecture, context length), and the LFS `size` and `sha256` of each file.
  The sha256 is the LFS oid Hugging Face computes on upload, the same value `model.rs` pins.
- Licences: the `license` and `license_link` fields of each model card, checked against the licence page they link to.
- llama.cpp: the merged pull request that added each architecture (`GET https://api.github.com/search/issues?q=repo:ggml-org/llama.cpp is:pr is:merged in:title ...`), and the tag dates from `GET https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/<tag>`.
  The Arch package version comes from `https://archlinux.org/packages/extra/x86_64/llama-cpp/json/`.
- OpenRouter: `GET https://openrouter.ai/api/v1/models` (419 models on the fetch date), fields `pricing`, `supported_parameters`, `default_parameters`, and the per-model `reasoning` object.
  Documented at https://openrouter.ai/docs/use-cases/reasoning-tokens.
- First-party prices: https://platform.claude.com/docs/en/about-claude/pricing, https://developers.openai.com/api/docs/pricing, https://ai.google.dev/gemini-api/docs/pricing, https://api-docs.deepseek.com/quick_start/pricing.
- Nothing was downloaded and no model was run.
  Catch rates quoted below are the HUF-181 measurements.

## Local models

Target machine: AMD Radeon 890M iGPU, 27 GB shared RAM, `ggml-vulkan` backend (spec section 4, the 8 GB tier).
Size on disk is the GGUF file; resident memory adds the KV cache, which at `-c 2048` is under 0.5 GB for every model here.

### The named families

| Family | Resolves to | Hugging Face repository | GGUF file | Size | sha256 | Context | Licence | Eligible under `weights.rs` | llama.cpp |
|---|---|---|---|---|---|---|---|---|---|
| gemma-4-e4b-it | `google/gemma-4-E4B-it` (2026-03-02) | `unsloth/gemma-4-E4B-it-GGUF` | `gemma-4-E4B-it-Q4_K_M.gguf` | 4,977,171,584 B (4.98 GB) | `85a896a047553e842f25297ee5b031d64ff30147d9c4af17b1e4b394cd1fab87` | 131,072 | Apache-2.0 (card and https://ai.google.dev/gemma/docs/gemma_4_license) | Apache-2.0, yes in law; `weights.rs` says Restricted today (see below) | `gemma4`, PR #21309 merged 2026-04-02 |
| Qwen 3.x small instruct | `Qwen/Qwen3.5-4B` (2026-02-27) | `unsloth/Qwen3.5-4B-GGUF` | `Qwen3.5-4B-Q4_K_M.gguf` | 2,740,937,888 B (2.74 GB) | `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4` | 262,144 | Apache-2.0 | yes, matched by the `qwen3` prefix | `qwen35`, PR #19468 merged 2026-02-10 |
| Qwen 3.x, 7B class | `Qwen/Qwen3.5-9B` (2026-02-27) | `unsloth/Qwen3.5-9B-GGUF` | `Qwen3.5-9B-Q4_K_M.gguf` | 5,680,522,464 B (5.68 GB) | `03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8` | 262,144 | Apache-2.0 | yes | `qwen35` |
| "Qwen 3.8 small" | does not exist; `Qwen/Qwen3.8-27B` (2026-08-05) is the smallest Qwen3.8 | `unsloth/Qwen3.8-27B-GGUF` | `Qwen3.8-27B-UD-Q4_K_M.gguf` | 16,464,440,224 B (16.5 GB) | `322e194ff79741c7baa497c240f677f54b201b0efab44ca8e50f122b39123482` | 262,144 | Apache-2.0 | yes by licence, no by size | `qwen35` (same architecture as 3.5) |
| Muse Glimmer | `meta-models/Muse-Glimmer-30B` (2026-08-09), Meta Superintelligence Lab, 29.6B dense with a 1.8B vision encoder | `unsloth/Muse-Glimmer-30B-GGUF` | `Muse-Glimmer-30B-UD-Q4_K_XL.gguf` (no Q4_K_M is published) | 15,878,222,368 B (15.9 GB) | `82bece304887a313ece08400bc030f6066c7bff5b906b0cd40308ec8a409fd38` | 131,072 | Apache-2.0 | yes by licence, no by size; `weights.rs` has no `muse` row so it answers Unknown | `muse-glimmer`, PR #26841 merged 2026-08-10 |
| Phi-4-mini or successor | no successor; `microsoft/Phi-4-mini-instruct` (2025-02-19) | `unsloth/Phi-4-mini-instruct-GGUF` | `Phi-4-mini-instruct-Q4_K_M.gguf` | 2,491,874,272 B (2.49 GB) | `88c00229914083cd112853aab84ed51b87bdf6b9ce42f532d8c85c7c63b1730a` | 131,072 | MIT | yes, `phi-4` prefix | `phi3`, long supported |

Notes:

- Gemma 4 licence.
  `google/gemma-4-E4B-it` carries `license: apache-2.0` and links https://ai.google.dev/gemma/docs/gemma_4_license, whose page title is "Apache License 2.0".
  The card header reads "License: Apache 2.0, Authors: Google DeepMind".
  Gemma 3 and earlier stay under the Gemma Terms of Use, so the `gemma` prefix row in `weights.rs` is right for them and wrong for `gemma-4-*`.
- Gemma 4 alternatives in the same repo: `gemma-4-E4B-it-Q4_0.gguf` 4.84 GB (`4a403d2e...`) and `gemma-4-E4B-it-UD-Q4_K_XL.gguf` 5.13 GB (`3cf61de1...`).
  Google's own QAT file `google/gemma-4-E4B-it-qat-q4_0-gguf` `gemma-4-E4B_q4_0-it.gguf` is 5.15 GB (`676c3507...`) and is the file to try if Q4_K_M quality ever needs a check, since it was quantisation-aware trained.
- Muse Glimmer.
  The card says it is "distilled from Muse Spark and purpose-built for autonomous agentic tasks on consumer hardware", targets a "24 GB or 32 GB envelope", and lists a DFlash speculative drafter.
  A 15.9 GB dense model on a 27 GB shared iGPU would load, but at roughly a third of the Gemma-4-E4B token rate.
  Nothing in the card claims a grammar advantage.
  Its listed comparison points are Gemma4-31B and Qwen3.6-27B, so a 4B class comparison does not exist.
- Qwen3.8-27B.
  The Qwen organisation lists four Qwen3.8 repositories: 27B, 27B-FP8, 2.4T-A95B, 2.4T-A95B-FP8.
  Qwen3.6 (2026-04) ships as 27B and 35B-A3B; Qwen3.7 is API only (`qwen/qwen3.7-flash`, `-plus`, `-max` on OpenRouter, no weights on Hugging Face).
  So "Qwen 3.8 small" resolves to Qwen3.5-4B, the row HUF-181 measured at 22 of 30 with 18 useful fixes.
- Phi.
  Microsoft's Phi repositories newest first: Phi-Ground-Any (2026-05, grounding), Phi-4-reasoning-vision-15B (2026-01), Phi-tiny-MoE-instruct and Phi-mini-MoE-instruct (2025-06), Phi-4-mini-flash-reasoning (2025-06, reasoning, 1,203 downloads).
  None is a small instruct successor.
- Thinking must be off for every 2026 model.
  Qwen3.5 "operate[s] in thinking mode by default" and needs `chat_template_kwargs: {"enable_thinking": false}`; Gemma 4 thinks only when `<|think|>` opens the system prompt; SmolLM3 needs `/no_think`.
  HUF-181 verified that Qwen3.5 returns an empty `content` without the kwarg.
  The `openai` adapter prompt must carry this per family.

### Up to five more small instruct models

Chosen for: a GGUF in the 1.5 to 3 GB range, a permissive licence, and an instruct tune from 2025-07 or later.

| Model | Released | Repository | GGUF file | Size | sha256 | Context | Licence | Eligible | llama.cpp | Grammar evidence |
|---|---|---|---|---|---|---|---|---|---|---|
| Gemma-4-E2B-it | 2026-03-02 | `unsloth/gemma-4-E2B-it-GGUF` | `gemma-4-E2B-it-Q4_K_M.gguf` | 3,106,738,272 B (3.11 GB) | `740185b21d22ceb83a11c3aa62ad5842ef32c70f6096d756bbee85a1e4ec34b8` | 131,072 | Apache-2.0 | yes once `gemma-4` is added | `gemma4` | HUF-181: 26 caught, 18 useful, 4 false positives |
| Ministral-3-3B-Instruct-2512 | 2025-10-31 | `mistralai/Ministral-3-3B-Instruct-2512-GGUF` (first party) | `Ministral-3-3B-Instruct-2512-Q4_K_M.gguf` | 2,147,023,008 B (2.15 GB) | `9ed150d4367e68df0ac8e1540f6ddc65b42d0ee26378329d1ecbca60f93fc5f8` | 262,144 | Apache-2.0 | no row in `weights.rs`, so Unknown; `ministral` needs a Permissive row | `mistral3` | not measured; multilingual card lists en, fr, es, zh |
| Granite-4.1-3B | 2026-04-06 | `ibm-granite/granite-4.1-3b-GGUF` (first party) | `granite-4.1-3b-Q4_K_M.gguf` | 2,099,501,664 B (2.10 GB) | `662b0626cd58f443baea23559b469df6576a81d349649c59413b36a9fb32eb29` | 131,072 | Apache-2.0 | no row, Unknown; `granite` needs a row | `granite` (2024-09) | not measured |
| SmolLM3-3B | 2025-07-08 | `ggml-org/SmolLM3-3B-GGUF` | `SmolLM3-Q4_K_M.gguf` | 1,915,305,312 B (1.92 GB) | `8334b850b7bd46238c16b0c550df2138f0889bf433809008cc17a8b05761863e` | 65,536 | Apache-2.0 | no row, Unknown; `smollm3` needs a row | `smollm3`, PR #14581 (2025-07) | not measured; HUF-181 found it and did not run it |
| LFM2.5-2.6B | 2026-08-01 | `LiquidAI/LFM2.5-2.6B-GGUF` (first party) | `LFM2.5-2.6B-Q4_K_M.gguf` | 1,674,455,040 B (1.67 GB) | `02a8b7e17487d326e46d68ce0ba24211e1b80a14c4cd0597fa73c1cd697f52ed` | 131,072 | LFM Open License v1.0 | no: commercial use is licensed only below 10 million USD annual revenue (section 5), so `Terms::Restricted` | `lfm2`, PR #14620 (2025-07) | not measured; the newest small model since HUF-181 |

Considered and dropped:

- `Qwen/Qwen3-4B-Instruct-2507` (`unsloth/Qwen3-4B-Instruct-2507-GGUF`, Q4_K_M 2.50 GB, `3605803b...`, Apache-2.0): HUF-181 already measured its sibling Qwen3-4B at 22 to 24 caught; Qwen3.5-4B supersedes it.
- `LiquidAI/LFM2.5-8B-A1B` (Q4_K_M 5.16 GB): same licence problem as the 2.6B, and larger on disk than Gemma-4-E4B.
- `nvidia/NVIDIA-Nemotron-3-Nano-4B`: NVIDIA Nemotron Open Model License, not Apache-2.0 or MIT, and no first-party GGUF.
- `allenai/Olmo-3-7B-Instruct`: Apache-2.0 but 7B class, no first-party GGUF.
- `LGAI-EXAONE/EXAONE-4.0-1.2B`: EXAONE licence, non-commercial.
- Llama 3.2 3B and Gemma 3: HUF-181 measured them at 9 false positives each and they carry restricted licences.

### llama.cpp version requirement

llama.cpp moved from `bNNNN` build tags to semantic tags in August 2026: `v0.2.0` on 2026-08-21 and `v0.3.0` on 2026-08-25 (GitHub releases API).
Arch `extra/llama-cpp` is `0.2.0-1`, built from `tag=v0.2.0` (PKGBUILD), with `ggml` and `ggml-vulkan` `0.21.0`.
Every architecture in the tables above merged before `v0.2.0`:

| Architecture | First merged | Pull request |
|---|---|---|
| `gemma4` | 2026-04-02 | https://github.com/ggml-org/llama.cpp/pull/21309 (tokenizer fixes through #21534, 2026-04-09) |
| `qwen35` (Qwen3.5, 3.6, 3.8 dense) | 2026-02-10 | https://github.com/ggml-org/llama.cpp/pull/19468 |
| `muse-glimmer` | 2026-08-10 | https://github.com/ggml-org/llama.cpp/pull/26841 |
| `lfm2` | 2025-07-11 | https://github.com/ggml-org/llama.cpp/pull/14620 |
| `smollm3` | 2025-07-08 | https://github.com/ggml-org/llama.cpp/pull/14581 |
| `granite` | 2024-09-17 | https://github.com/ggml-org/llama.cpp/pull/9412 |
| `phi3`, `mistral3` | 2024 to 2025 | long supported |

So the Arch package satisfies every row.
`grammachy doctor` should treat `llama-cpp >= 0.2.0` as the floor if it ever checks a version, and should not parse `bNNNN` any more.

### What changed since HUF-181

HUF-181 ran on the morning of 2026-08-25 and concluded that no chat LLM under 4 GB qualified, with Gemma-4-E4B at 29 of 30 as the best LLM.
Since then:

- No new small dense instruct model appeared.
  LFM2.5-2.6B (2026-08-01) predates HUF-181 and is licence-blocked.
  Muse Glimmer (2026-08-09) and Qwen3.8-27B (2026-08-05) are 27B to 30B class.
- llama.cpp retagged to `v0.x` and Arch now ships `v0.2.0`, which covers Gemma 4, Qwen3.5, and Muse Glimmer without a manual build.
- The Gemma 4 licence is Apache-2.0, which HUF-181 already recorded in its table but which `weights.rs` still contradicts.
  Fixing that one row makes the spec's recommended model eligible in the Models table, and is the single change that matters for the 8 GB tier.
- The verdict stands: for the 8 GB Vulkan tier the row is Gemma-4-E4B-it Q4_K_M.
  Gemma-4-E2B-it Q4_K_M (3.1 GB) is the only sub-4 GB LLM with a permissive licence and a measured catch rate above 80 percent, but its 4 false positives of 10 rule it out as a default.

## Cloud models through OpenRouter

Fetched 2026-08-25 from `GET https://openrouter.ai/api/v1/models`; the rows below are in `../api-model-benchmark/prices.json`.
Prices are USD per million tokens.
"Est. per 1,000 Checks" assumes 250 input and 40 output tokens per Check (HUF-171 note).
"Temp" and "json_object" are from `supported_parameters`; OpenRouter silently drops a parameter that is not listed there, so an unlisted `temperature` means the request runs at the provider default.
"Reasoning off" is from the per-model `reasoning` object: `mandatory: true` means `effort: "none"` is rejected; `default_enabled: false` means the model does not reason unless asked.

| Asked for | OpenRouter id | Canonical slug | Input | Output | Temp | `response_format` json_object | Reasoning off | Est. per 1,000 Checks |
|---|---|---|---|---|---|---|---|---|
| Claude Haiku 4.5 | `anthropic/claude-haiku-4.5` | `anthropic/claude-4.5-haiku-20251001` | 1.00 | 5.00 | yes | yes (`response_format`, `structured_outputs`) | yes; `reasoning: {mandatory: false}`, off unless `reasoning.max_tokens` or `effort` is sent | 0.450 |
| Claude Sonnet 5 | `anthropic/claude-sonnet-5` | `anthropic/claude-sonnet-5-20260630` | 2.00 | 10.00 | no (`temperature` not in `supported_parameters`) | yes | yes; `default_enabled: true`, `mandatory: false`, send `reasoning: {enabled: false}` | 0.900 |
| Gemini 3.7 Flash | `google/gemini-3.7-flash` | `google/gemini-3.7-flash-20260813` | 0.375 | 1.875 | yes | yes | no; `mandatory: true`, efforts high, medium, low, default medium | 0.169 plus reasoning tokens at 1.875 |
| GPT nano (latest) | `openai/gpt-5.4-nano` | `openai/gpt-5.4-nano-20260317` | 0.20 | 1.25 | no (`default_parameters.temperature: null`) | yes | yes; `default_enabled: false`, efforts include `none` | 0.100 |
| GPT mini (latest) | `openai/gpt-5.4-mini` | `openai/gpt-5.4-mini-20260317` | 0.75 | 4.50 | no | yes | yes; `default_enabled: false`, efforts include `none` | 0.368 |
| GPT 5.6 small (newer than 5.4 nano) | `openai/gpt-5.6-luna` | `openai/gpt-5.6-luna-20260709` | 0.20 | 1.20 | no | yes | yes; `default_enabled: true`, efforts include `none` | 0.098 |
| DeepSeek V4 Flash | `deepseek/deepseek-v4-flash-0731` | `deepseek/deepseek-v4-flash-20260731` | 0.0616 | 0.1232 | yes | yes | yes; `default_enabled: true`, `mandatory: false`, send `reasoning: {enabled: false}` | 0.020 |
| DeepSeek V4 Flash alias | `~deepseek/deepseek-v4-flash-latest` | same | 0.035 | 0.10 | yes | yes | same | 0.013 |

Notes on the cloud rows:

- GPT-5.6 has no `nano` or `mini`.
  The catalogue lists `openai/gpt-5.6-luna`, `-sol`, `-terra`, each with a `-pro` variant.
  Luna is described as "a fast, cost-efficient model in OpenAI's GPT-5.6 series" at the GPT-5.4 nano price, so it is the newer small GPT.
  GPT-5.4 nano and mini stay the newest models with those names.
- Temperature.
  Every GPT-5.x row and Sonnet 5 omit `temperature` from `supported_parameters` and list `temperature: null` in `default_parameters`.
  OpenRouter drops the field, so a Check runs at the provider default (1.0 for OpenAI).
  Haiku 4.5, Gemini 3.7 Flash, and both DeepSeek rows honour it.
- Reasoning.
  Gemini 3.7 Flash is the only row that cannot switch reasoning off; the OpenRouter doc says a `mandatory` model "rejects" `effort: "none"`.
  Its reasoning tokens bill at the output rate (`internal_reasoning: 1.875`), so the 0.169 estimate is a floor.
  Gemini 3.1 Flash Lite (`google/gemini-3.1-flash-lite`, 0.25 / 1.50, `default_effort: minimal`, not mandatory) is the Gemini row that can run without reasoning.
- Price drift against HUF-183 (same day, earlier fetch).
  DeepSeek V4 Flash 0731 fell from 0.14 / 0.28 to 0.0616 / 0.1232, and the `-latest` alias output price rose from 0.075 to 0.10.
  The `-latest` alias is still cheaper than the dated id it "always redirects to", and the catalogue still does not explain why.
- First-party prices differ in two places.
  Gemini 3.7 Flash on the Gemini API is 0.75 / 3.75 through 2026-12-31 and 1.50 / 7.50 from 2027-01-01; OpenRouter's 0.375 / 1.875 equals Google's batch price.
  DeepSeek's own API lists `deepseek-v4-flash` at 0.22 / 0.66 peak and half that off-peak, with a cache hit at 0.007 to 0.014; OpenRouter's 0.0616 / 0.1232 is a third-party host.
  Claude and OpenAI prices match first party exactly: Haiku 4.5 1 / 5, Sonnet 5 2 / 10 (the introductory price is now permanent per the Anthropic pricing page), gpt-5.4-nano 0.20 / 1.25, gpt-5.4-mini 0.75 / 4.50, gpt-5.6-luna 0.20 / 1.20.
- Sonnet 5 uses the newer Claude tokenizer, which the Anthropic pricing page says "produces approximately 30% more tokens for the same text".
  Its per-Check estimate is therefore about 1.17 USD per 1,000, not 0.90.

## Recommended rows

Local, in order:

1. `gemma-4-e4b-it`: `unsloth/gemma-4-E4B-it-GGUF/gemma-4-E4B-it-Q4_K_M.gguf`, 4.98 GB, Apache-2.0, eligible once `weights.rs` learns `gemma-4`.
2. `qwen3.5-9b`: 5.68 GB, Apache-2.0, eligible, 26 of 30 in HUF-181, the fallback if Gemma 4 ever regresses.
3. `qwen3.5-4b`: 2.74 GB, Apache-2.0, eligible, 22 of 30, the reference row for machines with less than 8 GB free.
4. Reference only: `gemma-4-e2b-it` (false positives), `phi-4-mini-instruct` (18 of 30), `muse-glimmer-30b` and `qwen3.8-27b` (size).

Cloud, in order:

1. `deepseek/deepseek-v4-flash-0731`: temperature and json_object honoured, reasoning can be off, 0.020 USD per 1,000 Checks.
2. `anthropic/claude-haiku-4.5`: the only Claude row that honours temperature, 0.45 per 1,000.
3. `openai/gpt-5.6-luna` or `openai/gpt-5.4-nano`: no temperature control, reasoning off, about 0.10 per 1,000.
4. `google/gemini-3.7-flash` only if a run shows its mandatory reasoning is cheap for a 40 token answer; otherwise `google/gemini-3.1-flash-lite`.
5. `anthropic/claude-sonnet-5`: quality reference, 0.90 to 1.17 per 1,000, no temperature.

## Follow-ups this report implies

- Add `gemma-4` (Permissive, "Apache-2.0"), `ministral`, `granite`, `smollm3`, and `lfm2.5` (Restricted, "LFM Open License v1.0") rows to `cli/src/bench/weights.rs`, and change the Restricted test fixture from `DEFAULT_OPENAI_MODEL` to a Gemma 3 or Llama 3 name.
- The `openai` adapter needs a per-family "thinking off" switch (`enable_thinking` kwarg for Qwen3.5, no `<|think|>` for Gemma 4) before any Qwen3.5 row is measured through it.
- When the HUF-183 run gets a key, add `openai/gpt-5.6-luna` and `anthropic/claude-sonnet-5` to `OPENROUTER_MODELS`, and send `reasoning: {enabled: false}` rather than relying on the 400 retry, because Gemini 3.7 Flash will not 400 and will reason anyway.

## Limitations

- No file was downloaded and no model was run; every digest and size is the Hugging Face LFS record on 2026-08-25.
- Hugging Face `gguf.context_length` is the value in the GGUF header, not a tested working context.
- OpenRouter prices change without notice and differ by upstream host; re-fetch before pinning a default.
- The Gemma 4 licence is read from Google's model card and the licence page title; the full text of https://ai.google.dev/gemma/docs/gemma_4_license was not diffed against the Apache-2.0 text.
