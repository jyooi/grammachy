# Research: native language list for the v1 dropdown

Collected 2026-08-25 for HUF-184.
Question: which native languages (L1s) should the `nativeLanguage` dropdown offer, in what order, and what can each engine do per language today.

## Summary

- Keep the charting list `zh`, `ms`, `fr`, `es` and extend it to seven: `zh`, `ms`, `es`, `fr`, `de`, `pt`, `ja`, plus `none`.
- LanguageTool `motherTongue` is a false-friend filter only.
  For English text it loads rules whose `<pattern lang="en">` has a `<translation>` in the mother tongue.
  It has zero rules for `zh` and rejects `ms`, `ko`, `hi`, `id` with HTTP 400.
- Build L1 rule packs in the order `zh`, `ms`, `es`, `fr`, `de`, `pt`, `ja`.
  The first two have no LanguageTool coverage at all, so a pack adds the most there.

## Method

1. Ranked L1s by speaker count (Ethnologue via Wikipedia) and by English-learner or English-user estimates per country (Wikipedia country table, British Council, EF EPI).
2. Rated interference documentation by presence of a chapter in Swan and Smith, "Learner English" (2nd edition), plus learner-corpus and error-analysis literature.
3. Read LanguageTool source for how `motherTongue` works and counted rules per language in `false-friends.xml`.
4. Probed the public LanguageTool API with each candidate code.

## How LanguageTool uses `motherTongue`

- API doc: "A language code of the user's native language, enabling false friends checks for some language pairs."
  Source: https://languagetool.org/http-api/languagetool-swagger.json
- The server parses the value with `Languages.getLanguageForShortCode`, which throws for unknown codes and the request fails with HTTP 400.
  Source: https://github.com/languagetool-org/languagetool/blob/master/languagetool-server/src/main/java/org/languagetool/server/TextChecker.java (`parseLanguage`, `motherTongueParam`)
- `JLanguageTool` only uses the value to load `false-friends.xml` through `FalseFriendRuleLoader`.
  No other rule reads it.
  Source: https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/java/org/languagetool/JLanguageTool.java (`loadFalseFriendRules`)
- `FalseFriendRuleHandler` keeps a rule only when the `<pattern lang>` equals the text language and a `<translation lang>` equals the mother tongue.
  Source: https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/java/org/languagetool/rules/patterns/FalseFriendRuleHandler.java
- The list of accepted codes comes from `GET /v2/languages`. On 2026-08-25 it returned 62 entries.
  Relevant codes: `zh-CN`, `ja-JP`, `ar`, `fa`, `ta-IN`, `tl-PH`, `km-KH`, `de`, `fr`, `es`, `pt`, `it`, `nl`, `pl-PL`, `ru-RU`, `sv`, `ko`, `hi`, `id`, `ms`, `vi`, `th`, `tr` are absent.
  Source: https://api.languagetool.org/v2/languages
- The runtime `motherTongue` is cheap to add for supported codes but its effect on interference errors was measured at zero on the 30 sentence set (HUF-175).

### Rules in `false-friends.xml` that fire on English text, per mother tongue

Counted on the master branch on 2026-08-25 with a script over `<pattern lang="en">` and its `<translation lang>`.
The file has 814 rule groups and 14 language codes in total.
Source: https://github.com/languagetool-org/languagetool/blob/master/languagetool-core/src/main/resources/org/languagetool/rules/false-friends.xml

| Mother tongue | Rules on English text |
|---|---|
| pl | 117 |
| de | 87 |
| gl | 73 |
| pt | 72 |
| fr | 69 |
| it | 31 |
| ru | 28 |
| nl | 9 |
| es | 8 |
| ja | 7 |
| sv | 5 |
| ga | 3 |
| ca, da | 0 |
| zh, ms, ko, hi, id, ar, vi, th, tr | 0 (zh and ar accepted, the rest rejected) |

Spanish has only 8 English-text rules even though it is an accepted code.
Most Spanish rules in the file are Spanish-Portuguese pairs.

### API probe

`POST /v2/check` with `language=en-US` and each `motherTongue` on 2026-08-25:
`zh`, `zh-CN`, `fr`, `es`, `de`, `pt`, `ja`, `ar` return 200.
`ms`, `ko`, `hi`, `id` return 400 with "'ms' is not a language code known to LanguageTool".

## Ranked candidates

Speaker counts: Ethnologue 2026 as tabulated at https://en.wikipedia.org/wiki/List_of_languages_by_total_number_of_speakers (L1 in millions).
English users: https://en.wikipedia.org/wiki/List_of_countries_by_English-speaking_population (largest country of the L1, additional-language speakers).
Learner estimates: China 400 million learners (British Council, https://www.britishcouncil.cn/en/EnglishGreat/numbers); 200 million users in 2007 (Qu 2007, cited at https://en.wikipedia.org/wiki/English_education_in_China).
Interference documentation: chapter in Swan and Smith, "Learner English" 2nd ed., Cambridge 2001 (https://www.cambridge.org/us/cambridgeenglish/catalog/teacher-training-development-and-research/learner-english-2nd-edition), which covers 22 language backgrounds including Chinese, Malay/Indonesian, French, Spanish, German, Portuguese, Japanese, Korean, Arabic, Russian, Polish, Italian, Dutch, Turkish, Thai, Farsi, Greek, Scandinavian, South Asian and Dravidian languages, West African languages and Swahili.
Cambridge Learner Corpus analysis of L1 effects: https://www.cambridge.org/elt/blog/2020/03/02/understanding-common-learner-error-cambridge-learner-corpus/ (found that Latin-script L1s show higher interference error rates).

| Rank | L1 | ISO 639-1 | L1 speakers (M) | English learners or users | Interference docs | LT `motherTongue` | Recommendation |
|---|---|---|---|---|---|---|---|
| 1 | Mandarin Chinese | zh | 988 | ~400 M learners (China), 10 M fluent users | Strong: Swan and Smith chapter; ICNALE, CLC studies; articles, plurals, tense, countability | Accepted as `zh-CN`, 0 rules | v1, first rule pack |
| 2 | Spanish | es | 487 | 19 M (Spain), 16 M (Mexico), plus Latin America | Strong: chapter; large false-friend lists; CLC | Accepted, 8 rules | v1 |
| 3 | Hindi and other Indian L1s | hi | 347 | 269 M users (India), mostly English-medium | Medium: chapter on South Asian languages; Indian English is a variety, not interference | Rejected | Defer; Indian users are served by en-IN dialect not L1 pack |
| 4 | Portuguese | pt | 252 | 10 M (Brazil) | Strong: chapter; 72 LT rules | Accepted | v1 |
| 5 | Arabic | ar | (MSA not an L1) 335 total | 39 M (Egypt) | Medium: chapter; script and article errors | Accepted, 0 rules | v2 |
| 6 | French | fr | 75 | 16 M (France), plus Africa and Canada | Strong: chapter; 69 LT rules; classic false-friend lists | Accepted | v1 |
| 7 | German | de | 76 | 45 M (Germany) | Strong: chapter; 87 LT rules | Accepted | v1 |
| 8 | Japanese | ja | 124 | 35 M (Japan), EF EPI rank 92 | Strong: chapter; 7 LT rules; articles, plurals, wasei-eigo | Accepted | v1 |
| 9 | Malay and Indonesian | ms, id | ~78 (id) plus Malay | 15 M (Malaysia), 85 M (Indonesia) | Medium: chapter on Malay/Indonesian; Malaysian error-analysis studies (articles, subject-verb agreement, copula, tense) | Rejected | v1 for `ms` (home market), second rule pack |
| 10 | Korean | ko | 82 | EF EPI rank 50 | Strong: chapter; particle and article errors | Rejected | v2 |
| 11 | Russian | ru | 133 | 5 M (Russia) | Strong: chapter; 28 LT rules | Accepted | v2 |
| 12 | Vietnamese | vi | 86 | not tabulated | Medium: no chapter; local studies | Rejected | v3 |
| 13 | Turkish | tr | 86 | 12 M | Medium: chapter | Rejected | v3 |
| 14 | Italian | it | 60 | 17 M | Strong: chapter; 31 LT rules | Accepted | v2 |
| 15 | Polish | pl | ~40 | 19 M | Strong: chapter; 117 LT rules | Accepted | v2 |

Malay interference sources:
- Maros, Hua, Salehuddin, "Interference in learning English: grammatical errors in English essay writing among rural Malay secondary school students in Malaysia", https://www.researchgate.net/publication/237782813
- "Mother tongue interference in the writing of ESL Malay learners", https://www.researchgate.net/publication/322095096
- "Common errors made in English writing by Malaysian ESL learners", https://files.eric.ed.gov/fulltext/EJ1348597.pdf

## Proposed v1 dropdown

Enum values in the manifest `schema`, in display order:

| Value | Label |
|---|---|
| none | None |
| zh | Chinese (Mandarin) |
| ms | Malay |
| es | Spanish |
| fr | French |
| de | German |
| pt | Portuguese |
| ja | Japanese |

Reasons:
- `zh` stays first: largest learner population and best documented interference.
- `ms` stays: home market of the project and zero engine coverage, so the setting and the future pack give real value.
- `es` moves ahead of `fr`: more speakers and more learners, and LanguageTool covers Spanish false friends poorly, so a pack matters more.
- `de`, `pt`, `ja` are added: each has a Swan and Smith chapter, a large learner base, and LanguageTool accepts the code today at no cost.
- Hindi, Korean, Arabic, Russian, Italian, Polish wait for v2.
  Hindi users are better served by an English dialect choice.
  The others add dropdown length without an engine effect or a planned pack.

Engine mapping for the CLI:
- LanguageTool: `zh` sends `motherTongue=zh-CN`; `es`, `fr`, `de`, `pt` send the same code; `ja` sends `ja-JP`; `ms` and `none` send no parameter.
- LLM engine: the prompt names the language in English for every value except `none`.
- Harper: ignores the setting.

## L1 rule pack build order

1. `zh`: largest audience, no engine support, well documented error taxonomy (articles, plural marking, tense and aspect, countability, subject omission, "very like").
2. `ms`: home market, no engine support, documented taxonomy (articles, subject-verb agreement, copula omission, tense, plural marking, Malay word order in noun phrases).
3. `es`: large audience, LanguageTool has only 8 false-friend rules; pack adds false friends ("actually", "assist", "career", "sensible"), article with generic nouns, "have" plus age, adjective order.
4. `fr`: LanguageTool covers 69 false friends; pack adds tense ("since" with present), article use, "make" versus "do", adjective position.
5. `de`: LanguageTool covers 87 false friends; pack adds word order after adverbials, "since" and "for", "will" in conditionals, comma before "that".
6. `pt`: LanguageTool covers 72 false friends; pack adds articles with generic nouns, "have" plus years, double negatives.
7. `ja`: pack adds articles, plurals, countability, subject omission, "wasei-eigo" loans.

The first two packs give the most lift per hour because LanguageTool contributes nothing for them.
Packs 3 to 7 overlap with LanguageTool false friends, so they focus on grammar interference rather than vocabulary.

## Open points

- The Swan and Smith table of contents was confirmed from the Cambridge catalog listing in search results, not from the publisher page, which returned HTTP 403.
- Country English-user figures mix census, Eurobarometer and EF sources of different years, so treat them as order-of-magnitude.
- The measured zero effect of `motherTongue` on the 30 sentence set holds for `zh`; a false-friend rich language such as `de` or `pl` may show a nonzero effect on a vocabulary-heavy test set.
