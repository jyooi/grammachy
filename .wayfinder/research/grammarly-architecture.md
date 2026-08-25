# Research: how Grammarly checks grammar, and what Grammachy v1 can borrow

Linear HUF-182, child of map HUF-170. Collected 2026-08-25.
Sources are first party where possible: Grammarly engineering blog, Grammarly papers, the GECToR repos, dataset pages.
Statements marked "Inferred" are my reading, not a sourced fact.

## 1. Pipeline shape

### Three engines in production

Grammarly states its GEC stack has three complementary parts:

- A large seq2seq Transformer "machine translation" model for whole-sentence rewrites.
- A sequence tagger ("tag, not rewrite") that "identifies localized issues one by one" with full-sentence context.
- Pattern rules "based on syntax patterns that range from capitalizing the word 'I' to suggesting where you need a comma."
  Source: https://www.grammarly.com/blog/engineering/innovating-the-basics/

Grammarly calls the overall design hybrid: "custom-made rules, deep neural networks, and language models, among others."
Source: https://www.grammarly.com/blog/engineering/under-the-hood-at-grammarly-leveraging-transformer-language-models-for-grammatical-error-correction/

The same post says plain Transformer LMs (BERT, GPT-1, GPT-2) can score sentence variants left to right, but "cannot capture errors as nuanced as more sophisticated methods" and need a generated inflection lexicon so the model knows "sit, sitting, sat, and sits are related."

### GECToR, the sequence tagger

GECToR (Omelianchuk et al., BEA 2020) is a BERT-like encoder "stacked with two linear layers, with softmax layers on the top."
One head detects whether a token is erroneous, the other predicts an edit tag.
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/ and https://arxiv.org/abs/2005.12592

Tag vocabulary: about 5,000 tags cover 98% of CoNLL-2014 errors.
Basic tags are `$KEEP`, `$DELETE`, 1,167 `$APPEND_x`, 3,802 `$REPLACE_x`.
Twenty "g-transformations" (`$CASE`, `$MERGE`, `$SPLIT`, `$NOUN_NUMBER`, `$VERB_FORM`) encode grammatical changes without vocabulary tags.
"Using only the top 100 basic tags, our model achieved 60% error coverage; adding in the g-transformations bumped our error coverage up to 80%."
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/

Inference is iterative: the corrected output is re-tagged, typically 2 to 3 passes, with quality traded for speed when passes are capped.
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/

Training is three stages: 9M synthetic PIE pairs, then about 500k real learner sentences, then 34k sentences that include error-free examples.
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/

Scores: single model F0.5 65.3 (CoNLL-2014) and 72.4 (BEA-2019 test), ensemble 66.5 and 73.6.
Source: https://arxiv.org/abs/2005.12592

### Tagging versus seq2seq

Speed on CoNLL-2014 test, Tesla V100, batch 128:
Transformer-NMT beam 12 takes 4.35 s, beam 1 takes 0.71 s, GECToR 5 iterations 0.40 s, GECToR 1 iteration 0.20 s.
The paper claims "up to 10 times as fast as a Transformer-based seq2seq GEC system."
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/ and https://arxiv.org/abs/2005.12592

Tags are also the explanation hook: "The tags also make it possible (though not trivial!) to describe the changes being made to the text in a human-readable manner."
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/

### Ensembles and distillation

Tarnavskyi, Chernodub, Omelianchuk (BEA 2022) moved to Large encoders (RoBERTa, DeBERTa, XLNet).
Large encoders were "2.3 to 2.5 times slower inference on average."
Ensembling is a "majority vote on output edits": keep only edits that a majority of models made.
Best quorum was N_models minus 1, and more than four models gave no gain.
Ensemble F0.5 76.05 on BEA-2019 test, versus 73.70 for the 2020 ensemble.
Knowledge distillation: the ensemble corrected public news and blog text (Troy-1BW, Troy-Blogs), and a single tagger trained on that reached 73.21.
Source: https://www.grammarly.com/blog/engineering/experimenting-with-gector/ and https://arxiv.org/abs/2203.13064

Majority vote on span edits is a precision filter by construction.
Inferred: this is why the ensemble gain shows mostly as precision.

### Ranking and filtering for low false positives

Two inference knobs are the documented precision controls:

- Confidence bias: "a permanent positive confidence bias to the probability of the $KEEP tag."
- Minimum error probability: "a sentence-level minimum error probability threshold for the output of the error detection layer."
  Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/

Published settings: RoBERTa bias 0.2 and min error prob 0.5; XLNet 0.2 and 0.5; BERT 0.1 and 0.41.
Source: https://github.com/grammarly/gector
Large RoBERTa uses 0.1 and 0.65; the large ensemble uses 0.3 and 0.55.
Source: https://github.com/MaksTarnavskyi/gector-large

Product side: Grammarly beta tests several GEC versions with different precision and recall trade-offs and picks by user behaviour, "Which suggestions do users accept, ignore, or dismiss?"
Suggestions with consistently high ignore rates get refined.
Source: https://www.grammarly.com/blog/engineering/innovating-the-basics/ and https://www.grammarly.com/blog/product/how-does-grammarly-work/

Evaluation sets come from "our team of expert analytical and computational linguists."
Source: https://www.grammarly.com/blog/engineering/innovating-the-basics/

Grammarly does not publish the ranker or the rule-versus-model arbitration.
Inferred: rules cover the fixed high-precision cases (capital "I", comma patterns) and the tagger covers the open set.

## 2. Span localisation and the suggestion card

A suggestion is a Delta, the same format as text and edits:
`category: "correctness", transform: [{retain: 9}, {"insert": "shock"}, {"delete": 6}]`.
A Delta Manager "rebases" each suggestion Delta when the user edits, so spans stay attached.
A Highlights Manager renders the underline.
The card offers Accept and Dismiss.
Source: https://www.grammarly.com/blog/engineering/how-suggestions-work-grammarly-editor/

In the editor, each card carries "Learn more" for "a detailed explanation behind each suggestion", for example why a verb tense fits the context.
Underlines are colour coded red, blue, green, purple, plus gray for style guides.
Source: https://support.grammarly.com/hc/en-us/articles/360003474732-Grammarly-Editor-user-guide

The browser extension buffers text revisions while the user types and processes them when typing stops.
This cut text input lag by about 91%.
Source: https://www.grammarly.com/blog/engineering/reducing-text-input-lag/

Inferred: the short card title comes from the edit tag class (for example `$VERB_FORM` maps to a tense reason) plus rule ids, since the blog says tags make human-readable descriptions possible.
No source documents the title generator.

## 3. Native language or user context tuning

Documented user settings are dialect only: American, British, Canadian, Australian, Indian.
Source: https://support.grammarly.com/hc/en-us/articles/115000089992-Select-between-British-English-American-English-Canadian-English-Australian-English-and-Indian-English

Style settings (formal versus casual) apply per site or text field.
Source: https://www.grammarly.com/blog/product/how-does-grammarly-work/

No first-party source documents a native-language (L1) setting or L1-conditioned models.
Training data is L1 mixed: NUCLE is essays by Asian undergraduates at NUS, Lang-8 is a global learner site.
Source: https://www.cl.cam.ac.uk/research/nl/bea2019st/

Grammarly's UA-GEC corpus shows their error taxonomy style for a non-English language: spelling, punctuation, grammar subtypes (Case, Gender, Number, Tense, Prepositions), fluency subtypes (Calque, Collocation).
Markup is `{error=>edit:::error_type=Tag}`.
Source: https://github.com/grammarly/ua-gec

## 4. Latency and where inference runs

Historic position: "it runs in the cloud, rather than locally on your device."
Source: https://www.grammarly.com/blog/product/how-does-grammarly-work/

On-device GEC, first generation: a T5-based GEC model, cloud version "almost 4 GB", shrunk to "less than 300 MB" with BFLOAT16, throughput raised from 70 to 297 tokens per second.
Targets were "100+ tokens/second" and "less than 100 milliseconds" response.
Runs through a Rust SDK on Mac (Metal), Windows, and the Chrome extension, with "no degradation in quality" reported.
Source: https://www.grammarly.com/blog/engineering/on-device-models-scale/

On-device, second generation: a Llama-style decoder of about 1B parameters, 4-bit quantised (70% memory reduction), run with Apple MLX on M-series Macs.
It handles spelling, grammar, and joint detection.
Target 50 tokens per second, achieved about 210 tokens per second on M2.
Known weak spots: proper nouns, articles, premature tense edits.
Source: https://www.grammarly.com/blog/engineering/efficient-on-device-writing-assistance/

Inferred: Grammarly did not publish CPU-only numbers for any on-device model, and both on-device generations are seq2seq or decoder models, not GECToR.

## 5. What is open source and runnable locally today

### GECToR code and weights

| Item | Encoder | BEA-2019 test F0.5 | Params | Code licence | Weights licence | URL |
|---|---|---|---|---|---|---|
| grammarly/gector | BERT, RoBERTa, XLNet base | 68.0, 71.8, 71.2 | about 110M to 125M | Apache-2.0 | Not stated separately | https://github.com/grammarly/gector |
| MaksTarnavskyi/gector-large | RoBERTa large | 73.1 | about 355M | Apache-2.0 | Not stated separately | https://github.com/MaksTarnavskyi/gector-large |
| MaksTarnavskyi/gector-large | Ensemble RoBERTa + DeBERTa + XLNet large | 76.05 | 3 large models | Apache-2.0 | Not stated separately | https://github.com/MaksTarnavskyi/gector-large |
| gotutiyan/gector (reimplementation) | RoBERTa base, DeBERTa base, RoBERTa large, DeBERTa large | 71.2, 72.0, 73.1, 73.8 | 0.1B for base | MIT | "Only non-commercial purposes" | https://github.com/gotutiyan/gector and https://huggingface.co/gotutiyan/gector-roberta-base-5k |

Parameter counts: roberta-base is "0.1B params", MIT licence.
Source: https://huggingface.co/roberta-base
Inferred: the large counts follow the standard RoBERTa-large size; the gector repos do not print sizes.

Inferred: the Grammarly weights are trained on non-commercial corpora (NUCLE, FCE, Lang-8, W&I+LOCNESS) and the repo licence covers code.
The gotutiyan model cards make that restriction explicit.
Treat all GECToR weights as non-commercial unless retrained on permitted data.

Inference knobs are the same across repos: `additional_confidence` or `keep_confidence`, `min_error_probability`, `iteration_count` or `n_iteration`, batch size.
Source: https://github.com/grammarly/gector and https://github.com/gotutiyan/gector

CPU latency: no repo or paper publishes CPU numbers.
GPU: 0.20 s to 0.40 s for the 1,312-sentence CoNLL-2014 test at batch 128 on a V100.
Source: https://www.grammarly.com/blog/engineering/gec-tag-not-rewrite/
Inferred estimate: a base encoder forward pass on a short sentence on a modern laptop CPU is in the tens to low hundreds of milliseconds, so 2 to 3 GECToR passes land at roughly 0.1 s to 0.5 s per sentence and a few seconds per paragraph.
Must be measured on the target machine.

### Datasets

| Dataset | Use | Licence or terms | URL |
|---|---|---|---|
| W&I+LOCNESS v2.1 | Train and BEA-2019 dev/test, 43,169 sentences | Non-commercial only | https://www.cl.cam.ac.uk/research/nl/bea2019st/ |
| FCE v2.1 | Train, 1,244 exam answers | Non-commercial only | same |
| NUCLE | Train, 1,400 essays, form required | Non-commercial only | same, and https://www.comp.nus.edu.sg/~nlp/conll14st.html |
| Lang-8 Corpus of Learner English | Train, form required | Non-commercial only | same |
| CoNLL-2014 test | Test, open download; NUCLE needs a signed licence | Non-commercial for NUCLE | https://www.comp.nus.edu.sg/~nlp/conll14st.html |
| cLang-8 | Train, 2,372,119 English pairs, needs raw Lang-8 | CC BY-NC-SA 4.0, code Apache-2.0 | https://github.com/google-research-datasets/clang8 |
| PIE synthetic | Pretrain, 9M pairs | Generated from open text, see repo | https://github.com/grammarly/gector |
| UA-GEC (Ukrainian) | Reference taxonomy only | CC BY 4.0 | https://github.com/grammarly/ua-gec |

Metric: ERRANT span-based F0.5, precision weighted twice recall.
Source: https://www.cl.cam.ac.uk/research/nl/bea2019st/

### Licence summary for a personal open-source plugin

- Allowed: GECToR code (Apache-2.0 and MIT), roberta-base (MIT), UA-GEC (CC BY 4.0), the tag and threshold design.
- Allowed for personal non-commercial use, and redistributable with the same restriction: gotutiyan weights, cLang-8 (BY-NC-SA, share-alike applies to derived data).
- Restricted: W&I+LOCNESS, FCE, NUCLE, Lang-8 are non-commercial and some need a signed form, so an Apache or MIT plugin must not bundle them or redistribute weights as if they were Apache.
- Inferred: shipping a GECToR checkpoint inside an MIT plugin is a licence mismatch unless the checkpoint is retrained on synthetic and permissive data.
  Safe path is to download weights at first run and mark them non-commercial.

## What Grammachy v1 can borrow

- A GECToR-style tagger as the local neural engine.
  Take the gotutiyan RoBERTa base 5k checkpoint (0.1B params, about 500 MB fp32, about 125 MB int8, sizes inferred from parameter count).
  Expected CPU cost per sentence is 0.1 s to 0.5 s for 2 to 3 passes, inferred, so run it on the selected text only and cap passes at 2.
  Export to ONNX and quantise to int8 before measuring on the target laptop.
- Precision-first thresholds.
  Copy the two knobs verbatim: a positive `$KEEP` bias and a sentence-level minimum error probability.
  Start near the published RoBERTa values (bias 0.2, min error prob 0.5) and tune upward, since a single-user tool should prefer silence over a wrong card.
- Majority vote as an optional precision filter.
  Run Harper or LanguageTool rules alongside the tagger and only auto-surface an edit that two engines agree on; show single-engine edits as low-confidence.
- A layered pipeline in the Grammarly shape.
  Rules for fixed cases (capital "I", dialect spelling, punctuation), tagger for the open set, no seq2seq rewrite in v1.
- Reason-per-span cards.
  Represent each suggestion as a span Delta (retain, insert, delete) plus a category and a one-line reason.
  Derive the reason from the tag class: `$VERB_FORM` gives "verb form", `$NOUN_NUMBER` gives "singular or plural", `$CASE` gives "capitalisation", `$APPEND_the` gives "missing article", and rule ids give the rule title.
  Keep Accept, Dismiss, and a "Learn more" body.
- Native-language awareness as a rule layer, not a model.
  Grammarly ships nothing here, so v1 adds L1 rule packs (articles and plurals for Mandarin and Malay speakers, false friends for French and Spanish speakers) and uses the UA-GEC style taxonomy to label them.
- Debounce like the extension: check when the selection is made, never per keystroke.
- Dialect switch as the only user setting that changes engine behaviour.

Out of reach for v1:

- Large-encoder ensembles (three 355M models, 2.3 to 2.5 times slower each) and their 76 F0.5.
- Grammarly's on-device T5 and 1B decoder models, which are not released.
- Whole-sentence seq2seq rewrites, fluency and clarity suggestions.
- Grammarly's engagement-driven threshold tuning across millions of users.
- Retraining GECToR on commercial-safe data, which needs the restricted corpora to match published scores.
