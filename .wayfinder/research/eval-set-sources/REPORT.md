# HUF-203: Public GEC corpora with a writer native-language label

Collected 2026-08-25.
Wayfinder map: HUF-202.
Question: which public grammatical error correction corpora carry a writer native-language (L1) label for zh, ms, es, fr, de, pt, or ja, under what licence, and how many usable single-sentence items does each give per language.

## Summary

- One corpus answers the question on its own: the CLC FCE dataset in its BEA-2019 release (`fce_v2.1.bea19`).
  It is a free download with no registration, every essay carries an `l1` field, and six of the seven target languages are present with 964 to 2,805 error sentences each.
  Malay (`ms`) is absent from every corpus checked.
- The FCE licence allows non-commercial research use and publication of excerpts under 100 words only, so the eval set is fetch, never commit.
  A script downloads the tarball at run time, converts it into the fixture shape, and the result stays out of git.
- Recommended composition: 340 items, 300 from FCE (60 zh, 60 es, 50 fr, 40 de, 40 pt, 50 ja) plus the 40 hand-written items of `interference-30.json`, which remain the only `ms` source.
  `ms` stays at the 11 hand-written items until real user sentences arrive, as spec section 13.1 already requires.
- No other corpus is worth a converter for v1.
  W&I+LOCNESS, NUCLE, CoNLL-2014, JFLEG, and cLang-8 carry no L1 label.
  Lang-8 raw, ICNALE Edited Essays, EFCAMDAT, TOEFL11, PELIC, KJ, CLC, and LENS either lack span edits, lack a licence that allows a download by script, or are behind a form, a fee, or a password.

## Context

- The fixture shape is `cli/tests/fixtures/interference-30.json`: `id`, `native`, `text`, `expected_span` (`start`, `end` in UTF-16 code units, `text`), `expected_fix`, `error_type`.
  It has 40 items today: 30 with one error each (10 zh, 11 ms, 10 es, 9 fr) and 10 correct sentences.
- Spec section 13.1: the fixture prints a catch rate, is not a gate, and "grows only through real user sentences, at least 10 per native language before a language is called covered".
  A corpus-derived set is therefore a second fixture beside the hand-written one, not a replacement.
- `.wayfinder/research/native-languages.md` fixed the seven languages and their order: zh, ms, es, fr, de, pt, ja.

## Method

1. Read the BEA-2019 shared task page, the corpus home pages, the licence files inside each archive, and the papers that describe the corpora.
2. Downloaded, without registration, `fce_v2.1.bea19.tar.gz`, `wi+locness_v2.1.bea19.tar.gz`, `conll14st-test-data.tar.gz`, the CoNLL-2013 release, the JFLEG and cLang-8 repositories, PELIC, and `ICNALE_EE_3.1.zip` into the scratchpad.
3. Counted FCE sentences per L1 with two scripts, `count_fce_l1.py` and `count_fce_one_edit.py`, kept in the scratchpad.
   The BEA M2 files carry no L1, so each M2 sentence is aligned back to its JSON essay by consuming the essay text as a stream of alphanumeric characters in the order `json_to_m2.py` wrote them.
   The alignment consumed every essay in all three splits without a mismatch.
4. Filter applied: one M2 sentence, under 5,000 UTF-16 units (no FCE sentence reaches the limit), at least one edit whose type is not `noop` and whose span is not `-1 -1`.
   A second count keeps only sentences with exactly one edit and no `UNK` edit, which is the shape the fixture uses.

## Per corpus

### FCE (CLC FCE dataset, BEA-2019 release)

- Home: https://www.cl.cam.ac.uk/research/nl/bea2019st/ and https://ilexir.co.uk/datasets/index.html.
  Download: https://www.cl.cam.ac.uk/research/nl/bea2019st/data/fce_v2.1.bea19.tar.gz, 2.8 MB, HTTP 200 with no registration on 2026-08-25.
- Licence: `fce/licence.txt` inside the archive, "CLC FCE Dataset Licence Agreement", University of Cambridge.
  Clause 3: "a non-exclusive non-transferable right to use the licensed dataset for non-commercial research and educational purposes."
  Clause 4: non-commercial "exclude[s] without limitation any use of the licensed dataset or information derived from the dataset for or as part of a product or service which is sold, offered for sale, licensed, leased or rented."
  Clause 6: "The Licensee may publish excerpts of less than 100 words from the licensed dataset."
  Citation required: Yannakoudakis, Briscoe, Medlock, ACL 2011.
- Commit or fetch: fetch, never commit.
  Clause 6 caps what may be published at 100 words, and a 300 sentence file is far beyond that.
  Clause 3 covers a benchmark run by a developer, and the eval set never ships inside the binary or the plugin, so run-time download for the bench is inside the licence.
- L1 label: yes, the `l1` field of every essay in `fce/json/fce.{train,dev,test}.json`, ISO 639-1.
  16 codes present: ca, de, el, es, fr, it, ja, ko, nl, pl, pt, ru, sv, th, tr, zh.
  Essays per target L1: es 398, fr 291, ja 160, de 138, pt 136, zh 132.
  `ms` is absent.
- Counts, all three splits, single M2 sentence, counted on 2026-08-25:

| L1 | Sentences | With 1+ edit | Exactly 1 edit, no UNK |
|---|---|---|---|
| zh | 2,047 | 1,339 | 489 |
| es | 4,617 | 2,918 | 1,001 |
| fr | 3,738 | 2,410 | 891 |
| de | 1,912 | 1,233 | 518 |
| pt | 1,624 | 1,012 | 340 |
| ja | 2,373 | 1,300 | 598 |
| ms | 0 | 0 | 0 |
| all 16 L1s | 33,236 | 20,877 | not counted |

  The "with 1+ edit" column excludes `noop` and `-1 -1` spans but keeps `UNK` edits.
  The last column also drops any sentence that carries an `UNK` edit, because those mark a passage the annotator could not correct.
- Annotation format: two parallel forms.
  `json/` has one essay per line with the raw text and `edits` as `[start, end, replacement, code]` in character offsets of that text.
  `m2/` has tokenised sentences with ERRANT edits in token offsets.
  The raw text has no character outside the Basic Multilingual Plane, so the JSON character offsets equal UTF-16 offsets after the sentence start is subtracted.
  The codes are the CLC two-letter scheme (Nicholls 2003): first letter R replace, M missing, U unnecessary, D derivation, F form, I inflection, and so on; second letter the word class, for example `RT` wrong preposition and `IV` verb inflection.
- Conversion to the fixture shape:
  1. Read `json/fce.*.json`, keep essays whose `l1` is a target code.
  2. Split the essay text into sentences by the same M2 alignment the count scripts use, so sentence boundaries equal the ones the shared task chose.
  3. Keep sentences with exactly one edit that lies inside the sentence, no `UNK` code, and a replacement that differs from the original.
  4. `text` is the raw sentence, `expected_span.start` and `end` are the JSON offsets minus the sentence start, `expected_span.text` is the slice, `expected_fix` is the replacement.
  5. Missing-word edits (`M*` codes) have a zero-width span.
     Widen them to the following word and prepend the insertion to the fix, the way `zh-04` in the fixture writes `teacher` to `a teacher`, so every item has a non-empty span.
  6. `error_type` maps the CLC code to the fixture vocabulary: `RT` preposition, `IV` and `TV` tense, `IN` and `CN` plural, `MD` and `UD` article, `AG*` agreement, `W` word_order, `R*` on nouns and verbs false_friend only when the word is a known cognate, else `lexis`.
     The fixture vocabulary gains `lexis`, `spelling`, and `punctuation` because the corpus has them and the hand-written set did not.
  7. `id` is `fce-<l1>-<n>`, `native` is the `l1` code.
- Estimated yield per target L1 after step 3 to 5 is the last column of the table, 340 to 1,001 items, so a 25 to 60 item slice per language is a small random sample with a fixed seed.

### W&I+LOCNESS (BEA-2019)

- Download: https://www.cl.cam.ac.uk/research/nl/bea2019st/data/wi+locness_v2.1.bea19.tar.gz, no registration.
- Licence: `licence.wi.txt`, the same eight clauses as FCE with the Write and Improve citation; `license.locness.txt` for the native essays.
- L1 label: none.
  The JSON records `id`, `cefr`, `userid`, `text`, `edits` only, checked on `A.train.json`.
  The shared task page groups texts by CEFR level and names no L1.
- The 2024 successor, the Write and Improve Corpus 2024 (https://researchdatasets.cambridge.org/datasets/write-and-improve-corpus-2024), is behind a registration form, is non-commercial, forbids passing the data on, and its page names CEFR labels and error annotation but no L1 field.
- Verdict: not usable for a per-L1 set.

### NUCLE and CoNLL-2013/2014

- Home: https://www.comp.nus.edu.sg/~nlp/corpora.html and https://www.comp.nus.edu.sg/~nlp/conll14st.html.
- NUCLE licence: "NUS Non-commercial research/trial corpus license", signed form, and the corpus "shall not be ... distributed, licensed or otherwise transferred or made available to any third party".
  The 2014 test data is "distributed freely" under a copyright notice with no licence text (`conll14st-test-data/README`).
- L1 label: none.
  The README describes "short English texts written by non-native speakers of English" and the SGML carries `nid` and `teacher_id` only.
  The writers are NUS undergraduates, so most are Chinese, Malay, Indian, or Indonesian speakers, but no per-essay label exists and the counts per L1 are not published.
- Verdict: not usable for a per-L1 set, and the NUCLE licence forbids redistribution anyway.

### JFLEG

- Home: https://github.com/keisks/jfleg.
  Licence: CC BY-NC-SA 4.0 (`README.md`).
- 754 dev and 747 test sentences from the GUG corpus, each with four fluency rewrites.
- L1 label: none in the repository, and the paper describes the sentences as GUG sentences with no writer metadata.
- Verdict: not usable for a per-L1 set.

### Lang-8 (NAIST Lang-8 Learner Corpora, BEA-2019 subset, cLang-8)

- Home: https://sites.google.com/site/naistlang8corpora/.
  Access: a Google form, after which "a link ... will be emailed to you immediately" (BEA-2019 page).
- Licence: "The corpora are distributed for research or educational purposes only, and is provided without any warranty. If you would like to use it for commercial purpose, please talk to support@lang-8.com" (readme-raw).
  No redistribution clause is published, so treat a public commit as not allowed.
- L1 label: yes in the raw format only.
  Each record is `["journal_id", "sentence_id", "learning_language", "native_language", [learner sentences], [[corrections]]]` (readme-raw).
  The BEA-2019 English subset and cLang-8 drop the field.
- Counts per native language: not published.
  The NAIST page publishes counts per learning language only (English 237,843 journals in the raw v2.0 dump).
  Mizumoto et al. 2012 (https://aclanthology.org/C12-2084.pdf) report 509,116 English sentence pairs by Japanese-L1 writers, which is the only per-L1 figure in print.
  The site's user base is Japanese-heavy, so ja is large, zh and es and fr and de are present, and `ms` is unknown.
- Annotation format: whole-sentence corrections by other users, often several per sentence, with optional `[f-red]` and `[sline]` markup.
  Conversion needs a diff (ERRANT) to recover spans, and the corrections are noisy, unreviewed, and sometimes rewrite the sentence.
- Verdict: a possible second source for ja and zh later, fetch only, behind a form.
  Not worth a converter while FCE covers both.

### CLC (Cambridge Learner Corpus)

- Home: https://www.cambridge.org/elt/corpus/learner_corpus2.htm.
  Access "is currently restricted to authors and researchers working on projects and publications for Cambridge University Press, and researchers at Cambridge English Language Assessment."
- The "Open Cambridge Learner English Corpus" on Sketch Engine (https://www.sketchengine.eu/cambridge-learner-corpus/) has 2.9 million words, first language metadata, and is "uncoded", so it carries no error annotation.
- Verdict: closed; the FCE release is its only open, error-coded, L1-labelled subset.

### ICNALE (Edited Essays module)

- Home: https://language.sakura.ne.jp/icnale/, modules at https://language.sakura.ne.jp/icnale/modules.html, download at https://language.sakura.ne.jp/icnale/download.html.
- Licence: Terms of Use clause 7, "It is prohibited to reproduce and/or redistribute a part or the whole of the ICNALE data."
  The zip files are public links, but "Registered users are given a password for unzipping the file(s)."
  I downloaded `ICNALE_EE_3.1.zip` (11 MB) and confirmed 3,612 of its 3,695 entries are encrypted.
- L1 label: by region, not by L1.
  Edited Essays regions and participants: CHN 40, HKG 30, IDN 33, JPN 40, KOR 40, PAK 23, PHL 30, SIN 20, THA 32, TWN 40.
  Table 2 of Ishikawa 2018 (https://jaecs.com/jnl/ECS25/ECS25_117-130.pdf) gives 80 essays each for China and Japan, 20 per CEFR band.
  Malaysia appears only in Written Essays Plus (107 participants) and Spoken Dialogues (20), neither of which is edited, and the MYS label mixes Malay, Chinese, and Tamil speakers.
- Annotation format: per essay an `_ORIG.txt`, an `_EDIT.txt`, and an `_ORIG+EDIT.doc` with Word track changes.
  There is no span or error type; a converter must diff the two texts.
- Counts per L1: essays only; sentences are not published.
  At roughly 230 words per essay, CHN and JPN give on the order of 1,000 sentences each, but many carry several edits.
- Verdict: a possible second zh and ja source, behind registration, redistribution prohibited, no error types.
  Not needed while FCE covers both.

### EFCAMDAT

- Home: https://ef-lab.mmll.cam.ac.uk/EFCAMDAT.html.
  Access: academic email, Google account, and "Your application ... will need to be approved by administrators".
- Labels nationality, not L1.
  A cleaned subcorpus keeps "the 11 most represented nationalities", and an error-coded subcorpus is distributed as XLSX.
  Counts per nationality in the error-coded subset are not published on the page.
- Verdict: not usable; nationality is not L1, access needs approval, and the licence PDF is only shown after login.

### TOEFL11 (ETS Corpus of Non-Native Written English, LDC2014T06)

- Home: https://catalog.ldc.upenn.edu/LDC2014T06.
  1,100 essays for each of Arabic, Chinese, French, German, Hindi, Italian, Japanese, Korean, Spanish, Telugu, Turkish.
- No error annotation; essays come "in original raw and tokenized forms".
  LDC licence with a fee.
- Verdict: L1 labels without corrections, so not usable.

### PELIC

- Home: https://github.com/ELI-Data-Mining-Group/PELIC-dataset.
  Licence: CC BY-NC-ND 4.0.
- 1,177 students, L1 labels with Arabic, Chinese, Japanese, Korean, Spanish, and Turkish as the six largest groups.
- No error annotation; only a separate spelling-correction pass exists.
- Verdict: not usable.

### Newer sets

- LENS (Acharya et al., EMNLP 2025, https://aclanthology.org/2025.emnlp-main.766/): 687 essays from 72 learners and 15 L1s, with L1 interference annotations.
  Target L1 counts from Table 11: Chinese 133 documents, French 19, Portuguese 3; no es, de, ja, or ms.
  Promised "under a Creative Commons BY-NC-SA 4.0 license upon publication via a GitHub repository" with a terms agreement.
  https://github.com/p-acharya/LENSCorpus was empty on 2026-08-25.
- Konan-JIEM (KJ) corpus (https://www.gsk.or.jp/en/catalog/gsk2015-a/): 233 essays by Japanese college students with manual error tags; 2,411 sentences in the 170 essay edition Mizumoto 2012 used.
  Sold by GSK, 22,000 to 88,000 JPY, "No commercial use", so neither commit nor fetch.
- cLang-8 (https://github.com/google-research-datasets/clang8): targets only; sources need the Lang-8 form, and the L1 field is dropped.
- Speak and Improve 2025 is speech, not writing.
- No Hugging Face dataset card found carries both an L1 field and span-level corrections for a target language.
  `jhu-clsp/jfleg` and `agentlans/grammar-correction` have no L1.

### Malay and Chinese sources with no download

- MACLE (Malaysian Corpus of Learner English, University of Malaya, about 800,000 words of undergraduate argumentative essays) and EMAS (Form Four essays) are described only in papers, and EMAS is "untagged and unedited".
  Neither has a download page, a licence, or error annotation.
  Sources: https://www.semanticscholar.org/paper/9d54f03e1f1be7c137401af90775be0874622389 and https://www.researchgate.net/publication/265097553.
- CLEC (Chinese Learner English Corpus, Gui and Yang 2002, about 1 million tokens, error-tagged) has no public download.
  Source: https://cogcomp.seas.upenn.edu/page/resource_view/46.
- Verdict: `ms` has no public error-annotated source at all.
  `zh` needs none of these because FCE has 1,339 zh error sentences.

## Recommendation

- Build `cli/tests/fixtures/eval-340` as fetch, never commit: a script downloads `fce_v2.1.bea19.tar.gz` from cl.cam.ac.uk, verifies a pinned sha256, converts as described above, and writes the JSON into a cache directory that `.gitignore` covers.
  The script commits nothing but the seed and the sha256.
  `grammachy bench` gains a `--fixture <path>` flag or reads the cache path, and CI skips the FCE table when the cache is absent, the way live engine tests skip today.
- Composition, 340 items:

| Native | FCE items | Hand-written items | Total | Basis |
|---|---|---|---|---|
| zh | 60 | 10 | 70 | 489 single-edit sentences available |
| ms | 0 | 11 | 11 | no source; grows only through user sentences |
| es | 60 | 10 | 70 | 1,001 available |
| fr | 50 | 9 | 59 | 891 available |
| de | 40 | 0 | 40 | 518 available |
| pt | 40 | 0 | 40 | 340 available |
| ja | 50 | 0 | 50 | 598 available |
| correct | 0 | 10 | 10 | keep for false positives |

  Every language with a source is above the 25 floor; zh and es get the most because they are the top two of `native-languages.md`.
  Sample with a fixed seed, stratified by CLC code so no language is all prepositions, and cap each essay at three items so one writer does not dominate.
- Add 40 correct FCE sentences (M2 blocks with only `noop`), stratified the same way, so the false-positive count is measured on real learner prose too.
  That takes the set to 380.
- Languages with no source: `ms` only.
  Report it as uncovered in the benchmark file until 10 real user sentences exist.
- Do not spend time on Lang-8, ICNALE, or LENS for v1.
  Revisit ICNALE Edited Essays only if a second zh or ja source is wanted, and LENS only once its repository is populated.

## Second pass

A second, independent pass on the same question is `REPORT-second-pass.md` beside this file.
It agrees on every source and licence verdict.
It differs in the FCE filter: it also drops spelling and punctuation edits, which gives fr 679, es 669, ja 446, zh 392, de 342, pt 238 single-edit items.
It adds that TLE (UD English-ESL) is a 500 per L1 subset of the same FCE text with the text withheld, so it is no new source.
It proposes a committed sidecar of ids, offsets, codes, and fixes, with the sentence text filled at fetch time.

## Open points

- The FCE licence is a research licence; the bench is a developer tool, and the fixture never ships in the product, but a lawyer has not read clause 4 against an open-source product that is free.
  If that reading fails, the fallback is a hand-written set only.
- The CLC code to `error_type` map above is a proposal; `false_friend` needs a cognate list per language to be assigned honestly, else those items fall to `lexis`.
- Sentence splitting relies on the shared task's own segmentation through the M2 alignment; a few FCE sentences hold two clauses the annotator treated as one, and they pass the filter.
