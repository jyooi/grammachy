//! Reading the fetched release: the M2 alignment and the writer's language.
//!
//! The M2 file is the alignment the items are built from, and it holds nothing
//! but sentences and edits. The writer's first language lives in the JSON file
//! of the same split, one essay per line, so both files are read and paired.
//!
//! The two files carry no shared key, but the pairing is exact all the same:
//! the converter that produced the M2 file wrote the sentences of one essay in
//! order, essays in JSON line order, and it only ever added or removed
//! whitespace. So one essay owns the run of M2 blocks whose text, with every
//! space removed, is that essay's own text with every space removed. That is
//! what [`documents`] walks, and a run that does not line up is a corpus this
//! build does not know rather than a guess.
//!
//! Nothing here is committed. Every string this module returns comes from the
//! gitignored cache (ADR 0003).

use std::collections::HashMap;
use std::path::Path;

use super::cache::Cache;

/// The splits of the release, in the order the document index counts them.
pub const SPLITS: [&str; 3] = ["train", "dev", "test"];

/// One sentence of the M2 alignment, with the essay it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The essay, counted over the splits in [`SPLITS`] order.
    pub document: usize,
    /// The place of this sentence inside that essay.
    pub sentence: usize,
    /// The writer's first language, an ISO 639-1 code from the JSON file.
    pub native: String,
    /// The sentence, as the M2 file tokenised it.
    pub tokens: Vec<String>,
    /// The edits of annotator 0, without the `noop` marker.
    pub edits: Vec<Edit>,
}

/// One `A` line of the M2 file, in token offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The first token the edit replaces. Equal to `end` for a missing word.
    pub start: usize,
    pub end: usize,
    /// The ERRANT code, such as `R:PREP`.
    pub code: String,
    /// The tokens that replace the span, space joined. Empty deletes it.
    pub correction: String,
}

impl Edit {
    /// Whether the edit adds a word rather than replacing one.
    pub fn is_insertion(&self) -> bool {
        self.start == self.end
    }
}

/// Every sentence of the release, in corpus order.
pub fn blocks(cache: &Cache) -> Result<Vec<Block>, String> {
    let mut out = Vec::new();
    let mut document = 0;
    for split in SPLITS {
        let m2 = read(&cache.m2(split))?;
        let json = read(&cache.json(split))?;
        pair(split, &m2, &json, &mut document, &mut out)?;
    }
    Ok(out)
}

/// The blocks of one split, from the text of its two files.
///
/// This is the seam a test drives: a synthetic M2 file and a synthetic JSON
/// file go through exactly the path the fetched release goes through.
pub fn blocks_of(
    split: &str,
    m2_text: &str,
    json_text: &str,
    first_document: usize,
) -> Result<Vec<Block>, String> {
    let mut out = Vec::new();
    let mut document = first_document;
    pair(split, m2_text, json_text, &mut document, &mut out)?;
    Ok(out)
}

/// Read one file of the cache.
fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))
}

/// Pair the essays of one split with their sentences and push the blocks.
fn pair(
    split: &str,
    m2_text: &str,
    json_text: &str,
    document: &mut usize,
    out: &mut Vec<Block>,
) -> Result<(), String> {
    let m2 = parse_m2(m2_text);
    let essays = parse_essays(json_text)
        .ok_or_else(|| format!("the {split} JSON is not one essay per line"))?;
    documents(split, &m2, &essays, document, out)
}

/// One essay of the JSON file: the language and the text, nothing else.
struct Essay {
    native: String,
    text: String,
}

/// Pair each essay with its run of M2 blocks and push them.
fn documents(
    split: &str,
    m2: &[Sentence],
    essays: &[Essay],
    document: &mut usize,
    out: &mut Vec<Block>,
) -> Result<(), String> {
    let mut next = 0;
    for essay in essays {
        let wanted = spaceless(&essay.text);
        let mut seen = String::new();
        let first = next;
        while next < m2.len() && seen.len() < wanted.len() {
            seen.push_str(&spaceless(&m2[next].tokens.join(" ")));
            next += 1;
        }
        if seen != wanted {
            return Err(format!(
                "the {split} split does not line up with its JSON at essay {}",
                *document
            ));
        }
        for (sentence, block) in m2[first..next].iter().enumerate() {
            out.push(Block {
                document: *document,
                sentence,
                native: essay.native.clone(),
                tokens: block.tokens.clone(),
                edits: block.edits.clone(),
            });
        }
        *document += 1;
    }
    Ok(())
}

/// The text with every whitespace character removed, and the punctuation the
/// converter normalised put back the same way it put it.
///
/// Tokenising only moves whitespace about, so two texts that differ in nothing
/// else are the same string here.
fn spaceless(text: &str) -> String {
    normalise(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The punctuation normalisation the release's own converter applied.
///
/// It is the `norm_dict` of `json_to_m2.py`, which the M2 file went through
/// and the JSON file did not, so the JSON side needs it to compare.
fn normalise(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{2019}' | '\u{00b4}' | '\u{2018}' | '\u{2032}' | '`' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{02dd}' | '\u{00a8}' | '\u{201e}' | '\u{300e}'
            | '\u{300f}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{00ac}' => '-',
            '\u{3001}' | '\u{ff0c}' => ',',
            '\u{ff1a}' => ':',
            '\u{ff1b}' => ';',
            '\u{ff1f}' => '?',
            '\u{ff01}' => '!',
            '\u{0650}' | '\u{200b}' => ' ',
            other => other,
        })
        .collect()
}

/// One `S` block of an M2 file, before it knows its essay.
struct Sentence {
    tokens: Vec<String>,
    edits: Vec<Edit>,
}

/// The blocks of one M2 document.
///
/// Only annotator 0 counts: the converter wrote the sentences of annotator 0
/// and the edits of every annotator against them, and a set built from two
/// annotators at once would hold edits that disagree. The `noop` marker says
/// annotator 0 changed nothing, so it is dropped rather than kept as an edit.
fn parse_m2(text: &str) -> Vec<Sentence> {
    let mut out: Vec<Sentence> = Vec::new();
    for line in text.lines() {
        if let Some(sentence) = line.strip_prefix("S ") {
            out.push(Sentence {
                tokens: sentence.split(' ').map(str::to_string).collect(),
                edits: Vec::new(),
            });
            continue;
        }
        let Some(annotation) = line.strip_prefix("A ") else {
            continue;
        };
        let Some(block) = out.last_mut() else {
            continue;
        };
        let fields: Vec<&str> = annotation.split("|||").collect();
        if fields.len() < 6 || fields[5] != "0" || fields[1] == "noop" {
            continue;
        }
        let mut span = fields[0].split(' ');
        let (Some(Ok(start)), Some(Ok(end))) = (
            span.next().map(str::parse::<usize>),
            span.next().map(str::parse::<usize>),
        ) else {
            continue;
        };
        block.edits.push(Edit {
            start,
            end,
            code: fields[1].to_string(),
            correction: fields[2].to_string(),
        });
    }
    out
}

/// The essays of one JSON document, one per line.
fn parse_essays(text: &str) -> Option<Vec<Essay>> {
    let mut out = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let essay: HashMap<String, serde_json::Value> = serde_json::from_str(line).ok()?;
        out.push(Essay {
            native: essay.get("l1")?.as_str()?.to_string(),
            text: essay.get("text")?.as_str()?.to_string(),
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "S I go to school yesterday .\n",
        "A 1 2|||R:VERB:TENSE|||went|||REQUIRED|||-NONE-|||0\n",
        "\n",
        "S She is a teacher .\n",
        "A -1 -1|||noop|||-NONE-|||REQUIRED|||-NONE-|||0\n",
        "\n",
        "S I like the apple .\n",
        "A 2 3|||U:DET||||||REQUIRED|||-NONE-|||0\n",
        "A 2 3|||R:DET|||an|||REQUIRED|||-NONE-|||1\n",
    );

    #[test]
    fn the_reader_keeps_annotator_zero_and_drops_the_noop_marker() {
        let blocks = parse_m2(SAMPLE);

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].edits.len(), 1);
        assert_eq!(blocks[0].edits[0].code, "R:VERB:TENSE");
        assert_eq!(blocks[0].edits[0].correction, "went");
        assert!(
            blocks[1].edits.is_empty(),
            "a noop sentence carries no edit"
        );
        assert_eq!(
            blocks[2].edits.len(),
            1,
            "the second annotator's edit is not kept"
        );
        assert_eq!(blocks[2].edits[0].correction, "", "a deletion has no text");
    }

    #[test]
    fn an_essay_owns_the_blocks_whose_text_is_its_own() {
        let m2 = parse_m2(SAMPLE);
        let essays = vec![
            Essay {
                native: "zh".to_string(),
                text: "I go to school yesterday.\n\nShe is a teacher.".to_string(),
            },
            Essay {
                native: "es".to_string(),
                text: "I like the apple.".to_string(),
            },
        ];
        let mut document = 0;
        let mut out = Vec::new();

        documents("train", &m2, &essays, &mut document, &mut out).unwrap();

        assert_eq!(document, 2);
        assert_eq!(
            out.iter()
                .map(|block| (block.document, block.sentence, block.native.as_str()))
                .collect::<Vec<_>>(),
            [(0, 0, "zh"), (0, 1, "zh"), (1, 0, "es")]
        );
    }

    #[test]
    fn a_corpus_that_does_not_line_up_is_a_message_rather_than_a_guess() {
        let m2 = parse_m2(SAMPLE);
        let essays = vec![Essay {
            native: "zh".to_string(),
            text: "Something else entirely.".to_string(),
        }];
        let mut document = 0;

        let error = documents("train", &m2, &essays, &mut document, &mut Vec::new()).unwrap_err();

        assert!(error.contains("does not line up"), "{error}");
    }

    #[test]
    fn the_curly_punctuation_of_the_json_matches_the_straight_punctuation_of_the_m2() {
        assert_eq!(
            spaceless("it \u{2019}s \u{201c}fine\u{201d}"),
            "it's\"fine\""
        );
    }
}
