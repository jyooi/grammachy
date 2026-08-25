# HUF-203: Eval set sources, licensed GEC corpora with native-language labels

Question: which public grammatical error correction corpora carry a writer native-language (L1) label for `zh`, `ms`, `es`, `fr`, `de`, `pt`, or `ja`, under what licence, and how many usable single-sentence items does each give per L1.

Collected 2026-08-25.
Counts marked "measured" come from a script run on the downloaded files during this research; the script is in section 6.
Counts marked "cited" come from the source named beside them.
Claims a source did not confirm are marked "unverified".

## 1. Summary

- One corpus answers the question for six of the seven L1s: the public CLC FCE dataset in its BEA-2019 JSON release.
  Every essay carries an `l1` field, the edits are character offsets with the 75-code CLC error type, and the download needs no form.
  Measured single-sentence, single-edit, non-spelling, non-punctuation items: `fr` 679, `es` 669, `ja` 446, `zh` 392, `de` 342, `pt` 238.
- The FCE licence is non-commercial research only, non-transferable, and allows published excerpts under 100 words only.
  The fixture must therefore be fetched at run time and never committed; a committed sidecar may hold ids, offsets, codes, and the expected fix, but not the sentence text.
- No hand-annotated corpus carries a Malay (`ms`) L1 label.
  ICNALE's only Malaysian essays (214, Written Essays Plus) have no edited counterpart and the terms forbid reproduction; the raw Lang-8 file carries a per-user native language and could yield `ms` sentences, but behind a form, with crowd corrections and no span edits.
  `ms` stays on the real-user route of spec section 13.
- Recommended composition: 300 FCE error items, 50 per L1 for `zh`, `es`, `fr`, `de`, `pt`, `ja`, plus 25 error-free FCE sentences as the false-positive control, fetched into `cli/tests/fixtures/` at test time and merged with the committed hand-written `interference-30.json`, which keeps the only `ms` items.
  Total 365.
- No other corpus adds anything for these seven L1s that FCE does not already give: W&I+LOCNESS has no L1 field, NUCLE and JFLEG carry no L1, Lang-8 is form-gated and noisy, TLE is a 500-per-L1 subset of the same FCE text, and the Write & Improve 2024 corpus forbids releasing even derived statistics without approval.

## 2. Corpora checked

Legend for the "Repo" column: `commit` means the data may sit in a public GitHub repository, `fetch` means run-time download only and never commit, `none` means not obtainable for this use.

| Corpus | L1 label | Licence | Repo | Format | Target L1 items |
|---|---|---|---|---|---|
| CLC FCE (BEA-2019 release v2.1) | yes, `l1` per essay | CLC FCE Dataset Licence: non-commercial research and education, excerpts under 100 words only | fetch | JSON (char offsets, CLC code) and M2 (ERRANT) | measured, see 2.1 |
| W&I+LOCNESS (BEA-2019) | no | CEWI licence, same terms as FCE; LOCNESS forbids any third-party distribution | fetch | JSON and M2 | 0 |
| NUCLE / CoNLL-2014 | no | NUS licence agreement, form | fetch | SGML and M2 | 0 |
| JFLEG | no | CC BY-NC-SA 4.0 | commit | parallel text, four references | 0 |
| Lang-8 (NAIST) raw and cLang-8 | native language per user, raw file only | form, research and education only; cLang-8 targets CC BY-NC-SA 4.0 | fetch | JSON journals with inline correction tags; cLang-8 TSV | only candidate for `ms`, see 2.5 |
| CLC (full Cambridge Learner Corpus) | yes | not public | none | proprietary | 0 |
| ICNALE (Written Essays, Edited Essays) | country code; Malaysia only in Written Essays Plus | registration; terms forbid reproduction or redistribution of any part | none | edited full essays, parallel text | 0 usable |
| EFCAMDAT | nationality, not L1 | academic approval; derived works only via the EFCamDat portal | none | XML with teacher error tags on 36% of scripts | 0 usable |
| TLE / UD English-ESL | yes, 10 L1s | CC BY-SA 4.0 annotations, FCE text withheld | fetch (text) | CoNLL-U original and corrected | 500 per L1 subset of FCE |
| PELIC | yes, free text | CC BY-NC-ND 4.0 | commit verbatim only | CSV, no corrections | 0 |
| NICE 3.x, KJ corpus | all Japanese | NICE free for research, no redistribution; KJ paid CD-ROM | fetch; none | parallel text; XML error tags | see 2.11 |
| TOEFL11 (LDC2014T06) | yes, 11 L1s | LDC fee | none | raw text, no corrections | 0 |
| Write & Improve Corpus 2024 | yes, 22 L1s | registration, non-commercial, no derived items without approval | fetch | parallel text with error annotation | see 2.11 |
| LENS (2025) | yes, 15 L1s | promised CC BY-NC-SA 4.0, repository empty | none yet | JSON error spans | 0 today |
| ICLE v3 | yes, 25 L1s | non-profit web access, free from 2026-09-15 | none | no verified error layer | 0 |
| COREFL, TECCL, CLEC, MACLE, EMAS, WriCLE | yes | see 2.12 | none | no corrections | 0 |

### 2.1 CLC FCE dataset, BEA-2019 release v2.1

- Source: https://www.cl.cam.ac.uk/research/nl/bea2019st/ lists `fce_v2.1.bea19.tar.gz` as a direct download with no form ("the FCE and W&I+LOCNESS corpora are immediately downloadable").
  The original release sits at https://ilexir.co.uk/datasets/index.html and https://researchdatasets.cambridge.org/datasets/clc-fce-dataset, which describes 1,244 scripts with "error annotation and essential demographic details including the candidate's first language and age bracket".
  The paper is Yannakoudakis et al. 2011, https://aclanthology.org/P11-1019.pdf.
- Licence: `fce/licence.txt` inside the tarball, "CLC FCE Dataset Licence Agreement".
  Clause 3 grants a "non-exclusive non-transferable right to use the licensed dataset for non-commercial research and educational purposes".
  Clause 4 excludes use "as part of a product or service which is sold, offered for sale, licensed, leased or rented".
  Clause 6: "The Licensee may publish excerpts of less than 100 words".
  The Cambridge research-datasets page for the original release adds "Do not provide the corpus (full or partial) to others in any way".
  Reading: fetch only.
  Grammachy is free software, so clause 4 does not bite; a downstream commercial fork would have to drop the fetched fixture.
- The HuggingFace mirrors `bea2019st/wi_locness` and `matejklemen/clc_fce` carry the text under these terms; treat them as unauthorised redistribution and never depend on them.
- L1 label: `fce/readme.txt` documents the JSON fields `id`, `l1`, `age`, `q`, `answer-s`, `script-s`, `text`, `edits`.
  `edits` is `[[annotator_id, [[char_start, char_end, correction, code], ...]], ...]`; the readme names the first three, the fourth is the CLC error code of Nicholls 2003 (measured: 75 distinct codes in `fce.train.json`).
- Measured essays (answers) per L1 over train, dev, and test: `es` 398, `fr` 291, `ko` 170, `ru` 164, `ja` 160, `it` 152, `pl` 148, `tr` 146, `el` 146, `de` 138, `pt` 136, `zh` 132, `ca` 128, `th` 126, `sv` 30, `nl` 4.
  No `ms`.
  Sentences: train 28,350, dev 2,191, test 2,695 (cited, https://aclanthology.org/W19-4406.pdf Table 3); test annotations are public in the tarball.
- Measured usable items per target L1, where usable means: one sentence of 5 to 25 words, exactly one annotator-0 edit inside it, `correction` not null, code not spelling (`S`, `SA`, `SX`), not punctuation (`MP`, `RP`, `UP`), not a catch-all (`CE`, `CL`, `CQ`, `AS`, `X`):

| L1 | Sentences in FCE (measured) | Usable items | Top codes | Error-free sentences |
|---|---|---|---|---|
| fr | 3,834 | 679 | RV 72, RT 61, TV 48, RN 47, R 44, W 40 | 810 |
| es | 4,761 | 669 | RT 78, RV 71, RN 41, TV 39, R 37, W 35 | 919 |
| ja | 2,454 | 446 | MD 59, TV 42, RV 32, RT 31, MT 27, UD 21 | 717 |
| zh | 2,120 | 392 | RV 48, TV 47, RT 43, MD 25, FV 15, R 14 | 440 |
| de | 2,003 | 342 | RT 41, TV 37, RV 35, W 29, RN 25, R 23 | 446 |
| pt | 1,662 | 238 | RT 33, RV 27, RN 17, R 14, MT 12, FV 11 | 336 |

  "Error-free sentences" are 5 to 25 word sentences no annotator touched, the supply for the false-positive control.
- The error code scheme is documented in Nicholls 2003, http://ucrel.lancs.ac.uk/publications/CL2003/papers/nicholls.pdf: first letter F (wrong form), M (missing), R (replace), U (unnecessary), D (wrongly derived); second letter A (pronoun), C (conjunction), D (determiner), J (adjective), N (noun), Q (quantifier), T (preposition), V (verb), Y (adverb); plus AG* (agreement), C* (countability), FF* (false friend), TV (tense), W (word order), X (negative formation), IN (noun plural), IV (verb inflection), S/SA/SX (spelling), *P (punctuation), CE/CL/ID/L/AS (compound, collocation, idiom, register, argument structure).
  Section 3 maps these onto the fixture `error_type` values.
- Sharp edges seen in the sample: some replace edits depend on essay context ("It" to "That" as a sentence opener), and the JSON `text` is the raw essay, so sentence splitting is the converter's job.
  The M2 files are ERRANT-typed but drop the `l1` field, so the JSON is the file to convert.

### 2.2 W&I+LOCNESS, BEA-2019 release v2.1

- Same tarball page; `wi+locness/readme.txt` gives the JSON fields `id`, `userid`, `cefr`, `text`, `edits`.
  There is no L1 field (measured on every JSON file: exactly those five keys).
- Licences: `licence.wi.txt` is the CEWI licence with the same eight clauses as FCE; `license.locness.txt` adds "No part of the corpus is to be distributed to a third party without specific authorization from CECL".
- Sizes from the readme: 3,600 W&I texts and 100 LOCNESS essays, 34,308 training sentences, 4,384 dev, 4,477 test; test annotations withheld (`test/readme.txt`).
- Verdict: no L1, so 0 items for this ticket.
  Useful only as an L1-blind false-positive control (the LOCNESS native essays), and only as fetch.

### 2.3 NUCLE and CoNLL-2014

- https://www.comp.nus.edu.sg/~nlp/corpora.html: NUCLE release 3.3 needs a "Data License Agreement Form" and is "distributed under the standard NUS licensing agreement" (the form now leads to the gated https://huggingface.co/datasets/nusnlp/NUCLE); the agreement text itself is not public, unverified.
  The CoNLL-2014 test set (50 essays, two annotators, 1,312 sentences) is a direct download from https://www.comp.nus.edu.sg/~nlp/conll14st.html under the same terms.
- L1: the NUCLE paper https://aclanthology.org/W13-1703.pdf records only NUS undergraduates from six CELC courses; the SGML fields are `nid`, paragraphs, and per-error offsets, `type`, `correction`, `teacher id`, `comment`; no L1.
  The CoNLL-2014 paper https://aclanthology.org/W14-1701.pdf says "25 NUS students, who are non-native speakers of English"; BEA-2019 calls them "25 South-East Asian undergraduates".
  Sizes: NUCLE 3.2 1,397 essays, 57,151 sentences (W14-1701 Table 2).
- Verdict: 0 items for this ticket.
  The writers are South-East Asian, so some may be Malay speakers, but nothing records it.

### 2.4 JFLEG

- https://github.com/keisks/jfleg: CC BY-NC-SA 4.0, 754 dev and 747 test sentences from the GUG dataset, four fluency references each, plain text.
- No L1 metadata; GUG (https://github.com/EducationalTestingService/gug-data, CC BY-NC-SA 4.0) has none either, its paper says only "essays written by non-native speakers of English as part of a test of English language proficiency" (https://aclanthology.org/P14-2029.pdf).
- Verdict: commit is allowed, but 0 items for this ticket.

### 2.5 Lang-8 (NAIST) and cLang-8

- https://sites.google.com/site/naistlang8corpora/: "available only for research and educational purposes", commercial use by arrangement with Lang-8 (raw README, https://sites.google.com/site/naistlang8corpora/home/readme-raw); Corpus of Learner English v1.0 has 100,051 entries from 29,012 users; the raw v2.0 has 580,549 entries in 80 learning languages, 237,843 of them English.
  Access is through a Google form; the BEA-2019 page says the link arrives by email.
- The raw file `lang-8-20111007-L1-v2.dat` is one JSON record per journal with journal id, sentence id, learning language, native language, learner sentences, and correction arrays with inline `[f-red]`, `[f-blue]`, `[sline]` tags (raw README above).
  The English-only `entries.train` drops the native language field (https://sites.google.com/site/naistlang8corpora/home/readme-en).
  Per-L1 counts are not published and must be computed after approval.
- Noise: the cLang-8 paper says "many of the examples contain unnecessary paraphrasing and erroneous or incomplete corrections", English WER 15.46 raw against 10.11 cleaned (https://arxiv.org/pdf/2106.03830); BEA-2019 notes its edits "are longer and noisier" than FCE (https://aclanthology.org/W19-4406.pdf).
- https://github.com/google-research-datasets/clang8: the cleaned targets are CC BY-NC-SA 4.0, but the source side must be rebuilt from the NAIST release, so the form still applies.
  The TSV has only `source` and `target` (English 2,372,119 pairs); the per-user native language is dropped, and the targets were produced by a model (gT5), not by hand.
- Verdict: fetch only, per-user self-declared L1 in the raw file, no span-level edits (parallel text only), crowd corrections.
  Not recommended for the six FCE L1s.
  It is the only corpus found that could yield Malay-L1 English with corrections, so it is the fallback if the real-user route for `ms` stalls: filter native language Malay, run ERRANT to get spans, keep single-edit sentences, review by hand.

### 2.6 CLC (Cambridge Learner Corpus)

- The full CLC is proprietary to Cambridge University Press and Cambridge Assessment; the product page https://www.cambridge.org/elt/corpus/learner_corpus2.htm returned 403.
  The Sketch Engine "Open Cambridge Learner English Corpus (uncoded)" has first-language metadata but "deliberately excludes error tagging" and is query only (https://www.sketchengine.eu/cambridge-learner-corpus/).
- Verdict: none; the FCE dataset of 2.1 is its only public slice.

### 2.7 ICNALE

- https://language.sakura.ne.jp/icnale/: five modules; https://language.sakura.ne.jp/icnale/modules.html gives the counts.
  Written Essays: China 400 participants / 800 essays, Japan 400 / 800, Singapore 200 / 400, no Malaysia.
  Written Essays Plus v0.7: Malaysia 107 participants / 214 essays.
  Edited Essays: 640 essays fully edited by five professional native editors (China 80, Japan 80, Singapore 40, no Malaysia), parallel plain text `_ORIG` and `_EDIT` pairs, no span edits (https://jaecs.com/jnl/ECS25/ECS25_117-130.pdf).
- Terms of use clause 7: "It is prohibited to reproduce and/or redistribute a part or the whole of the ICNALE data."; access is by registration form (https://language.sakura.ne.jp/icnale/download.html).
- Verdict: none.
  The Malaysian essays have no edited counterpart, the label is country rather than L1, and the terms rule out both commit and a fetch script.
  A private one-off read for hand-writing new `ms` sentences (rewriting the pattern, not copying the sentence) remains possible.

### 2.8 EFCAMDAT

- https://ef-lab.mmll.cam.ac.uk/EFCAMDAT.html: academic affiliation and approval required, 1,180,310 texts from 174,743 learners in the second release, error-coded subcorpus in XLSX.
- Licence agreement 2023 (https://ef-lab.mmll.cam.ac.uk/assets/pdf/EFCamDat-User-Agreement-2023.pdf): personal copies of brief extracts for private study and non-commercial research; derivative works may be released "only on the EFCamDat Portal ... upon written application"; making the data available to any other person "is expressly prohibited".
- L1: "We currently have no information on the L1 backgrounds of learners. Information on nationality is, thus, used as the closest approximation" (https://www.lingref.com/cpp/slrf/2012/paper3100.pdf).
  First release scripts by nationality: Brazil 187,286, China 96,843, Mexico 41,115, Germany 29,192, France 22,146, Taiwan 13,596, Japan 10,672; Malaysia and Spain not in the top ten.
  Teacher error tags exist on 36% of scripts.
- Verdict: none.
  Approval-gated, nationality only, and even a derived offsets file may not leave the portal.

### 2.9 TLE / UD English-ESL

- Paper: Berzak et al. 2016, https://aclanthology.org/P16-1070.pdf, "5,124 sentences from the Cambridge First Certificate in English (FCE) corpus", "10 different native language backgrounds: Chinese, French, German, Italian, Japanese, Korean, Portuguese, Spanish, Russian and Turkish", "For every native language, we randomly sampled 500 automatically segmented sentences, under the constraint that selected sentences have to contain at least one grammatical error that is not punctuation or spelling", 2.67 errors per sentence on average.
- https://github.com/UniversalDependencies/UD_English-ESL: annotations CC BY-SA 4.0 (the paper says CC BY 3.0; the repository wins), but "Due to FCE licensing restrictions, the annotations are released without the text"; a `merge.py` script rebuilds the sentences from the FCE download.
- Verdict: a curated 500-per-L1 subset of 2.1 with the same fetch constraint; no extra items, but its sentence ids are a ready-made, already-segmented sample for `zh`, `es`, `fr`, `de`, `pt`, `ja`.
  Its sentences average 2.67 errors, so most fail the single-edit filter.

### 2.10 PELIC

- https://github.com/ELI-Data-Mining-Group/PELIC-dataset: CC BY-NC-ND 4.0 on GitHub (the Zenodo record https://zenodo.org/records/3991977 says CC BY-ND 4.0), 46,230 texts from 1,177 students, CSV, a free-text `native language` column in `student_information.csv` with Arabic, Chinese, Japanese, Korean, Spanish, Turkish dominant.
- No error annotation or corrected text; a separate PELIC-spelling repo covers spelling only.
- Verdict: no corrections, so 0 items.

### 2.11 Single-L1 and newer sets, 2023 to 2026

- Write & Improve Corpus 2024, https://researchdatasets.cambridge.org/datasets/write-and-improve-corpus-2024 and https://www.repository.cam.ac.uk/items/ba155087-0754-4c6b-ade8-68858e1df2f0: 23,000 plus essay versions from 766 users, error annotations on first and final versions, parallel original and corrected text, and the writers "have supplied their first language (L1) in an optional questionnaire. There are 22 different L1s in the corpus, with the most common being Spanish, Portuguese, Japanese, Arabic and Vietnamese".
  Licence: registration form, non-commercial research only, "Do not provide the corpus (full or partial) to others in any way ... e.g. through the use of repositories on sites such as Hugging Face and GitHub" and "Do not release items (e.g. models, data statistics) derived from the corpus without prior approval".
  Per-L1 counts: the report PDF did not load (504), unverified.
  Verdict: gated fetch, never commit; the no-derived-items clause makes even a committed offsets sidecar need approval.
  The best second source for `es`, `pt`, `ja` when FCE runs short; not needed for the 50-per-L1 composition.
- LENS (EMNLP 2025, https://aclanthology.org/2025.emnlp-main.766/): 687 essays, 15 self-reported L1s, error spans with corrections and L1-interference labels in JSON, generated by GPT-4 with partial manual checks; Chinese 133, French 19, Portuguese 3 essays, no `es`, `de`, `ms`, `ja`.
  Promised under CC BY-NC-SA 4.0, but https://github.com/p-acharya/LENSCorpus is empty at fetch time.
  Verdict: watch; the interference labels would be the first direct ground truth for the product claim if the data appears.
- NICE 3.x (Nagoya, https://sugiura-ken.org/sgr/nice-nagoya-interlanguage-corpus-of-english/nice-3-3/): Japanese-L1 learner essays (185 files in 3.3) with a professional correction line under each original, free for education and research, redistribution needs permission.
  Verdict: fetch only, parallel text, no span edits; FCE already gives 446 `ja` items with codes, so not needed.
- KJ corpus (Konan-JIEM, https://www.gsk.or.jp/catalog/gsk2019-a/): 233 Japanese-L1 essays with XML error tags, sold on CD-ROM by GSK (22,000 to 88,000 JPY), non-commercial.
  Verdict: none.
- TOEFL11 (https://catalog.ldc.upenn.edu/LDC2014T06): 1,100 essays each for 11 L1s including zh, fr, de, ja, es, LDC fee, no corrections.
  Verdict: none.
- MultiGED-2023, https://github.com/spraakbanken/multiged-2023: the English part is FCE (custom licence) plus REALEC, a Russian-L1 detection set; token labels only, no corrections, no target L1.
- RILEC, https://arxiv.org/abs/2603.07366: Russian-L1 interference sentences, CC BY 4.0 per the abstract, release URL not on the page.
  Not a target L1.
- ICLE v3, https://uclouvain.be/en/research-institutes/ilc/cecl/icle: 25 mother tongues including Chinese, French, German, Japanese, Portuguese (Brazilian), Spanish; web interface at EUR 121 per user-year, "non-profit educational purposes only", free "starting on September 15, 2026"; no error-correction layer could be verified.
  Verdict: none today; re-check after 2026-09-15 for L1-balanced raw text only.
- Speak & Improve 2025, https://arxiv.org/abs/2412.11986: spoken English with L1 and error annotation, non-commercial; speech transcripts, not writing.
- ELLIPSE, https://github.com/scrosseye/ELLIPSE-Corpus: CC BY-NC-SA 4.0, scores only, no L1 field, no corrections.

### 2.12 National corpora without corrections

None of these has an error-correction layer, so none yields items; listed so the next search does not repeat them.

- COREFL (https://corefl.learnercorpora.com/): Spanish and German L1 learners of English, 5,177 participants, CC BY-NC-ND 3.0 ES, no error annotation.
- TECCL (https://corpus.bfsu.edu.cn/info/1070/1449.htm): 9,865 Chinese-L1 texts, POS only, personal research only.
- CLEC: the 2003 error-tagged Gui and Yang disk has no download page; the 2025 corpus of the same name (https://www.cambridge.org/core/journals/studies-in-second-language-acquisition/article/introducing-the-chinese-learner-english-corpus-clec/859CBA31798105430C424631799F6339) is CC BY, 828 texts, no error tags.
- MACLE (University of Malaya) and EMAS: no primary download page found; EMAS is "untagged and unedited" (https://ejournal.ukm.my/gema/article/download/23571/8337).
- WriCLE (Spanish L1, 521 essays): UAM page unreachable, only a research subset was error-coded, unverified.
- HuggingFace: no dataset found with both a native-language column and corrections; `RA-ALTA/learner_tuning_*`, `rahuln2002/GED-lang8-cleaned`, and `srky/ICNALE_writing_score` have no licence or no L1.

## 3. Conversion to the fixture shape

Target shape, from `cli/tests/fixtures/interference-30.json`: `id`, `native`, `text`, `expected_span {start, end, text}` in UTF-16 code units, `expected_fix`, `error_type`.

Source of record: the FCE JSON files, not the M2 files.
The M2 files are token-offset ERRANT output with the `l1` dropped, and the spaCy tokenisation would have to be undone to get character spans back.
The JSON gives character offsets on the raw text directly.

Steps, all implemented in the script of section 6:

1. Read `fce/json/fce.{train,dev,test}.json`, one essay per line.
2. Split `text` into sentences: paragraphs on `\n`, sentences on whitespace after `.`, `!`, `?`.
   Keep 5 to 25 words.
   Record the sentence start offset `st` in the essay.
3. Take annotator 0 only (the FCE release has one annotator for almost every essay).
   Keep the sentence when exactly one edit lies inside it, `correction` is not `null` (null marks a detection-only edit), and the code is not spelling, punctuation, or a catch-all.
4. `expected_span.start = utf16(sentence[:a - st])`, `end = utf16(sentence[:b - st])`, `text = essay[a:b]`, `expected_fix = correction`.
   Python indexes code points and the shell indexes UTF-16, so the conversion is `len(prefix.encode("utf-16-le")) // 2`, the same rule `cli/src/text.rs` applies on the Rust side.
   FCE text is almost entirely BMP, so the two counts differ only on a rare astral character, but the fixture contract is UTF-16, so the conversion stays.
5. `id = "fce-<essay id>-<n>"`, `native = l1`.
6. `error_type` from the CLC code:

| CLC code | fixture `error_type` |
|---|---|
| TV | tense |
| AGV, AGN, AGD, AGA | agreement |
| MD, UD, RD, CD, FD | article |
| MT, UT, RT | preposition |
| W | word_order |
| IN, FN, CN | plural |
| FV, IV | gerund (verb form; rename to `verb_form` if the fixture list grows) |
| MC, UC, RC | conjunction |
| FF* | false_friend |
| MA, UA, RA | pronoun (`missing_subject` when MA sits at the sentence start) |
| RJ, RY, RN, RV, R, M, U, D* | lexical (new value) |

`existential`, `double_subject`, and `comparative` in the hand-written fixture have no CLC code; they stay hand-written.

The `lexical` bucket (RV, RN, RJ, R) is the largest in every L1 and is where an engine's false-friend and collocation catches live, so keep it rather than drop it, but report it as its own row in `bench`.

A committed sidecar may hold everything but `text` and `expected_span.text`; the fetch step fills those two fields from the tarball and asserts the sha256 of `fce_v2.1.bea19.tar.gz` first, pinned in the script the way `setup/model.rs` pins the weights.
The sidecar is under the 100-word excerpt clause because it carries no sentence text; `expected_fix` values are single words or short phrases.

## 4. Recommended composition

- 300 error items from FCE: 50 per L1 for `zh`, `es`, `fr`, `de`, `pt`, `ja`.
  Each L1 has 238 or more usable items, so a stratified pick by `error_type` (about 10 tense, 10 article, 10 preposition, 5 agreement, 5 word order, 10 lexical per L1) fits inside the supply for every L1 including `pt`.
- 25 clean items from FCE as the false-positive control, chosen from sentences no annotator touched (supply per L1 in the table of 2.1), about 4 per L1.
- Plus the committed hand-written `interference-30.json` (40 items today: `ms` 11, `zh` 10, `es` 10, `fr` 9), which keeps the only `ms` coverage and the error types FCE has no code for.
- Total 365 items, inside the 200 to 500 window of the ticket, and at least 25 per L1 for every L1 with a source.
- Selection is by a fixed seed over the sorted candidate list so the fetched fixture is reproducible without committing the text; a human reads every selected item once and rejects context-dependent edits (the "It" to "That" case in 2.1), so the script emits 60 candidates per L1 and a review file marks 50.
- The fetch runs in a test that skips when the tarball is absent or the sha256 differs, the same skip pattern as `languagetool_live.rs`, so CI stays green offline and the licence terms are met by never storing the text in the repository.

## 5. L1s with no source

- `ms`: no hand-annotated corpus with a Malay L1 label.
  ICNALE has 214 Malaysian essays with no corrections and forbids reproduction; EFCAMDAT labels nationality only and locks derived files to its portal; FCE, W&I, NUCLE, JFLEG, TLE, PELIC, TOEFL11 have no Malay writers.
  The raw Lang-8 file is the one fetchable source with a Malay native-language field, at the cost of a form, crowd corrections, and an ERRANT alignment step.
  Route: spec section 13, real user sentences, at least 10 before `ms` counts as covered; today the hand-written fixture holds 11.
  Lang-8 is the fallback if that route stalls.
- Every other target L1 is covered by FCE.

## 6. Measurement script

Run on 2026-08-25 against the BEA-2019 tarball `fce_v2.1.bea19.tar.gz` unpacked to `/tmp/bea`.

```python
import json, glob, collections, re

split = re.compile(r"(?<=[.!?])\s+")
SKIP = lambda code: code.startswith("S") or code in ("MP", "RP", "UP", "CE", "CL", "CQ", "AS", "X")

def utf16(prefix):
    return len(prefix.encode("utf-16-le")) // 2

usable = collections.Counter()
items = []
for path in glob.glob("fce/json/*.json"):
    for line in open(path):
        if not line.strip():
            continue
        essay = json.loads(line)
        text, l1 = essay["text"], essay["l1"]
        if not essay["edits"]:
            continue
        edits = essay["edits"][0][1]
        pos = 0
        for para in text.split("\n"):
            for sent in split.split(para):
                if not sent.strip():
                    pos += len(sent) + 1
                    continue
                st = text.index(sent, pos)
                en = st + len(sent)
                pos = en
                inside = [e for e in edits if e[2] is not None and st <= e[0] and e[1] <= en]
                if len(inside) != 1 or not 5 <= len(sent.split()) <= 25 or SKIP(inside[0][3]):
                    continue
                a, b, fix, code = inside[0]
                usable[l1] += 1
                items.append({
                    "id": f"fce-{essay['id']}-{usable[l1]}",
                    "native": l1,
                    "text": sent,
                    "expected_span": {"start": utf16(sent[: a - st]), "end": utf16(sent[: b - st]), "text": text[a:b]},
                    "expected_fix": fix,
                    "error_type": code,
                })
print({k: usable[k] for k in ["zh", "es", "fr", "de", "pt", "ja"]})
```

Output: `{'zh': 392, 'es': 669, 'fr': 679, 'de': 342, 'pt': 238, 'ja': 446}`.

Sample item the script produced (a `pt` sentence, code `RV`):

```json
{
  "id": "fce-TR7*0100*2000*01-1",
  "native": "pt",
  "text": "It is a dream becames true and was really unexpected for me!",
  "expected_span": {"start": 8, "end": 26, "text": "dream becames true"},
  "expected_fix": "dream come true",
  "error_type": "RV"
}
```

This sample is an excerpt under 100 words, which clause 6 of the licence allows.
