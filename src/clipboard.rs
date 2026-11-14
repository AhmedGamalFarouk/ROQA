//! The Shelf's Copied list: a small newest-first history of copied text. Pure state, no I/O; the OS side is
//! `clipboard_os`. Contents are never logged anywhere. Text only: images and files are not kept.

use crate::store::Store;

/// Entries kept; a new one evicts the oldest.
pub const MAX: usize = 12;
/// Characters kept per entry (longer text is cut, so the store file stays small).
pub const MAX_CHARS: usize = 400;
/// Cells the file grid leaves for entries (the fourth is Undo's); with more entries than this the last cell is "More" and the rest page.
pub const VISIBLE: usize = crate::shelf::VISIBLE;
/// Entries per page when "More" takes the last cell.
const PER_PAGE: usize = VISIBLE - 1;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Clips {
    items: Vec<String>,
    page: usize,
}

impl Clips {
    pub fn from_store(store: &Store) -> Self {
        let mut clips = Self::default();
        for text in store.clips.iter().filter(|t| !t.trim().is_empty()) {
            let text: String = text.chars().take(MAX_CHARS).collect();
            if clips.items.len() < MAX && !clips.items.contains(&text) {
                clips.items.push(text);
            }
        }
        clips
    }
    /// True when the store's copy changed, so the caller marks it dirty.
    pub fn write_to(&self, store: &mut Store) -> bool {
        let changed = store.clips != self.items;
        store.clips.clone_from(&self.items);
        changed
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn items(&self) -> &[String] {
        &self.items
    }
    pub fn has_more(&self) -> bool {
        self.items.len() > VISIBLE
    }
    fn start(&self) -> usize {
        if self.has_more() {
            (self.page % self.items.len().div_ceil(PER_PAGE)) * PER_PAGE
        } else {
            0
        }
    }
    /// The entries in the grid: all of them, or one page beside the "More" cell.
    pub fn visible(&self) -> &[String] {
        let start = self.start();
        let len = if self.has_more() { PER_PAGE } else { VISIBLE };
        &self.items[start..(start + len).min(self.items.len())]
    }
    /// The grid cells drawn: the entries plus "More" when there is overflow.
    pub fn slot_count(&self) -> usize {
        self.visible().len() + usize::from(self.has_more())
    }
    /// The list index behind grid cell `slot`; none for the "More" cell or an empty one.
    pub fn absolute(&self, slot: usize) -> Option<usize> {
        (slot < self.visible().len()).then(|| self.start() + slot)
    }
    pub fn next_page(&mut self) {
        if self.has_more() {
            self.page = self.start() / PER_PAGE + 1;
        }
    }
    /// A copy arrives: whitespace-only text is ignored, an equal older entry moves to the front, the list
    /// stays within `MAX`. False when nothing changed (the newest already is this text).
    pub fn add(&mut self, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let text: String = text.chars().take(MAX_CHARS).collect();
        if self.items.first() == Some(&text) {
            return false;
        }
        self.items.retain(|t| *t != text);
        self.items.insert(0, text);
        self.items.truncate(MAX);
        self.page = 0;
        true
    }
    pub fn remove(&mut self, index: usize) -> bool {
        let found = index < self.items.len();
        if found {
            self.items.remove(index);
            self.page = self.page.min(self.items.len().saturating_sub(1) / PER_PAGE);
        }
        found
    }
    pub fn clear(&mut self) -> bool {
        let had = !self.items.is_empty();
        self.items.clear();
        self.page = 0;
        had
    }
}

/// One line for a capsule: whitespace runs collapse to a single space.
pub fn preview(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The store line for an entry: `\` and line breaks escaped so it stays one line.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

pub fn decode(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_dedupe_cap_and_page() {
        let mut clips = Clips::default();
        assert!(!clips.add("  \n"));
        assert!(clips.add("a") && !clips.add("a"));
        assert!(
            clips.add("b") && clips.add("a"),
            "an older equal moves to the front"
        );
        assert_eq!(clips.items(), ["a", "b"]);
        for n in 0..20 {
            clips.add(&n.to_string());
        }
        assert_eq!(clips.items().len(), MAX);
        assert!(clips.has_more() && clips.slot_count() == VISIBLE);
        assert_eq!(clips.visible().len(), PER_PAGE);
        clips.next_page();
        assert_eq!(clips.absolute(0), Some(PER_PAGE));
        assert!(clips.add(&"x".repeat(MAX_CHARS * 2)));
        assert_eq!(clips.items()[0].chars().count(), MAX_CHARS);
    }

    #[test]
    fn a_store_line_survives_breaks_and_backslashes() {
        let text = "a\\b\r\nc=d";
        assert_eq!(decode(&encode(text)), text);
        assert!(!encode(text).contains(['\n', '\r']));
        assert_eq!(preview(" a \n\n b\t"), "a b");
    }
}
