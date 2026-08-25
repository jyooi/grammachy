//! UTF-16 span math, the unit every span in the contract uses.
//!
//! The shell indexes the text in JavaScript, so spans count UTF-16 code units
//! rather than bytes or characters (spec section 5.1). LanguageTool counts the
//! same way, because Java strings are UTF-16 too.

/// Length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The byte index of UTF-16 offset `offset`, or `None` when the offset is past
/// the end or lands inside a surrogate pair.
pub fn byte_index_of_utf16(text: &str, offset: usize) -> Option<usize> {
    let mut units = 0;
    for (byte_index, character) in text.char_indices() {
        if units == offset {
            return Some(byte_index);
        }
        units += character.len_utf16();
        // The offset split a surrogate pair, so no byte index answers it.
        if units > offset {
            return None;
        }
    }
    (units == offset).then_some(text.len())
}

/// The half-open slice `[start, end)` in UTF-16 code units, or `None` when the
/// span is reversed, out of range, or lands inside a surrogate pair.
pub fn utf16_slice(text: &str, start: usize, end: usize) -> Option<&str> {
    if start > end {
        return None;
    }
    let from = byte_index_of_utf16(text, start)?;
    let to = byte_index_of_utf16(text, end)?;
    Some(&text[from..to])
}

/// A prefix table from char index to UTF-16 offset, with one extra entry that
/// holds the length of the whole text.
///
/// Engines that count in `char`s, as `harper-core` does, index into this to
/// reach the UTF-16 offsets the contract asks for. One table serves every span
/// of one Check.
pub fn utf16_offsets(text: &str) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(text.chars().count() + 1);
    let mut units = 0;
    for character in text.chars() {
        offsets.push(units);
        units += character.len_utf16();
    }
    offsets.push(units);
    offsets
}
