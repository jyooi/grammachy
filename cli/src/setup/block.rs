//! The marked block both configuration files carry.
//!
//! Spec section 10 fixes the shape for `bindings.lua`: everything Grammachy
//! owns sits between `-- grammachy begin` and `-- grammachy end`, so a second
//! `grammachy setup` replaces the block instead of adding one, and
//! `grammachy setup --remove` takes exactly that block out again.
//!
//! The menu extension is JSONC, so the same idea uses `//` comments there.
//! Both files therefore share one rule about where the block starts and ends.
//!
//! The rule that makes removal byte exact: the inserted region always begins
//! with the newline before the begin marker and always ends with the newline
//! after the end marker. Insertion appends that one region and removal deletes
//! that same region, so a file that carried nothing else of ours comes back as
//! it was, to the byte.

use std::ops::Range;

/// The comment lead and the indent one file uses for the two markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Markers {
    /// What starts a comment line, such as `--` or `//`.
    pub comment: &'static str,
    /// What sits in front of the comment lead, such as the two spaces the menu
    /// extension indents its members by.
    pub indent: &'static str,
}

/// The markers of `~/.config/hypr/bindings.lua`.
pub const LUA: Markers = Markers {
    comment: "--",
    indent: "",
};

/// The markers of `~/.config/omarchy/extensions/omarchy-menu.jsonc`.
pub const JSONC: Markers = Markers {
    comment: "//",
    indent: "  ",
};

impl Markers {
    pub fn begin(&self) -> String {
        format!("{}{} grammachy begin", self.indent, self.comment)
    }

    pub fn end(&self) -> String {
        format!("{}{} grammachy end", self.indent, self.comment)
    }
}

/// Where a block goes in a file that has none yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// After everything the file already holds, which is where a Hyprland
    /// binding block belongs: a later `o.bind` for a key wins.
    EndOfFile,
    /// Directly after the opening brace of a JSON object, so the new member
    /// never needs a comma in front of it and the members already there keep
    /// their own punctuation.
    InsideOpeningBrace,
}

/// The block one file owns: its markers and the lines between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub markers: Markers,
    /// The lines between the markers, each already terminated by a newline.
    pub body: String,
}

impl Block {
    /// The exact region the file carries, newline before the begin marker and
    /// newline after the end marker included.
    fn region(&self) -> String {
        format!(
            "\n{}\n{}{}\n",
            self.markers.begin(),
            self.body,
            self.markers.end()
        )
    }

    /// The file with exactly one copy of this block, whatever it held before.
    ///
    /// An existing block is replaced in place, so a hand edit inside the
    /// markers is overwritten and the rest of the file is left alone.
    pub fn ensure(&self, content: &str, anchor: Anchor) -> Result<String, String> {
        let stripped = match find(content, &self.markers) {
            Some(_) => self.strip(content),
            None => content.to_string(),
        };
        let at = match anchor {
            Anchor::EndOfFile => stripped.len(),
            Anchor::InsideOpeningBrace => object_start(&stripped)?,
        };

        let mut next = String::with_capacity(stripped.len() + self.body.len() + 64);
        next.push_str(&stripped[..at]);
        next.push_str(&self.region());
        next.push_str(&stripped[at..]);
        Ok(next)
    }

    /// The file without this block, or the file itself when it holds none.
    pub fn strip(&self, content: &str) -> String {
        match find(content, &self.markers) {
            Some(region) => {
                let mut next = String::with_capacity(content.len());
                next.push_str(&content[..region.start]);
                next.push_str(&content[region.end..]);
                next
            }
            None => content.to_string(),
        }
    }
}

/// Whether the file already carries a block with these markers.
pub fn is_present(content: &str, markers: &Markers) -> bool {
    find(content, markers).is_some()
}

/// The byte range of the block region, or `None` when the file holds none.
///
/// A marker only counts on a line of its own, so a binding line that mentions
/// the words in a payload never matches.
fn find(content: &str, markers: &Markers) -> Option<Range<usize>> {
    let begin = markers.begin();
    let end = markers.end();

    let begin_line = marker_line(content, &begin)?;
    let end_line = marker_line(&content[begin_line.end..], &end)
        .map(|line| (begin_line.end + line.start)..(begin_line.end + line.end))?;

    // The region carries the newline on each side, which is what makes
    // removal put the file back exactly as it was.
    let start = begin_line.start.saturating_sub(1);
    let mut stop = end_line.end;
    if content[stop..].starts_with('\n') {
        stop += 1;
    }
    Some(start..stop)
}

/// The byte range of the line that holds nothing but this marker.
fn marker_line(content: &str, marker: &str) -> Option<Range<usize>> {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if line.trim_end_matches('\n').trim_end() == marker {
            return Some(start..(offset - usize::from(line.ends_with('\n'))));
        }
    }
    None
}

/// The offset just after the opening brace of a JSON object.
fn object_start(content: &str) -> Result<usize, String> {
    content
        .find('{')
        .map(|at| at + 1)
        .ok_or_else(|| "The file holds no JSON object to extend.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Block {
        Block {
            markers: LUA,
            body: "o.bind(\"SUPER + G\", \"Grammachy\", \"true\")\n".to_string(),
        }
    }

    #[test]
    fn a_second_run_leaves_one_block() {
        let original = "o.bind(\"SUPER + RETURN\", \"Terminal\", \"true\")\n";

        let once = block().ensure(original, Anchor::EndOfFile).unwrap();
        let twice = block().ensure(&once, Anchor::EndOfFile).unwrap();

        assert_eq!(once, twice);
        assert_eq!(twice.matches("-- grammachy begin").count(), 1);
    }

    #[test]
    fn removal_puts_the_file_back_byte_for_byte() {
        for original in ["a\n", "a", "", "a\n\n", "{\n}\n"] {
            let with = block().ensure(original, Anchor::EndOfFile).unwrap();
            assert_eq!(block().strip(&with), original, "original was {original:?}");
        }
    }

    #[test]
    fn a_hand_edit_inside_the_markers_is_replaced() {
        let original = "o.bind(\"SUPER + RETURN\", \"Terminal\", \"true\")\n";
        let with = block().ensure(original, Anchor::EndOfFile).unwrap();
        let edited = with.replace("Grammachy", "Edited");

        let again = block().ensure(&edited, Anchor::EndOfFile).unwrap();

        assert!(!again.contains("Edited"));
        assert_eq!(again, with);
    }

    #[test]
    fn the_json_block_lands_inside_the_opening_brace() {
        let member = Block {
            markers: JSONC,
            body: "  \"grammachy.compose\": {},\n".to_string(),
        };
        let original = "{\n  // a comment\n}\n";

        let with = member.ensure(original, Anchor::InsideOpeningBrace).unwrap();

        assert!(with.starts_with("{\n  // grammachy begin\n"), "{with}");
        assert_eq!(member.strip(&with), original);
    }

    #[test]
    fn a_file_without_an_object_is_an_error() {
        let member = Block {
            markers: JSONC,
            body: "  \"x\": {},\n".to_string(),
        };

        assert!(member.ensure("", Anchor::InsideOpeningBrace).is_err());
    }

    #[test]
    fn a_marker_inside_a_payload_does_not_count() {
        let content = "o.bind(\"SUPER + G\", \"X\", \"echo -- grammachy begin\")\n";

        assert!(!is_present(content, &LUA));
    }
}
