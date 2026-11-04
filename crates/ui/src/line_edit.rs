//! Bounded UTF-8 scalar editor; owns no text, paths or clipboard.
use std::{collections::VecDeque, ops::Range};
const HISTORY_BYTES: usize = 32 * 1024;
const HISTORY_STATES: usize = 16;
#[derive(Clone)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}
#[derive(Default)]
pub struct Cursor {
    pub caret: usize,
    pub anchor: usize,
    pub revision: u64,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
}
#[derive(Clone, Copy)]
pub enum Motion {
    Left,
    Right,
    Start,
    End,
    WordLeft,
    WordRight,
}
fn boundary(text: &str, mut n: usize) -> usize {
    n = n.min(text.len());
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    n
}
fn previous(text: &str, n: usize) -> usize {
    text[..n]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}
fn next(text: &str, n: usize) -> usize {
    text[n..]
        .chars()
        .next()
        .map(|c| n + c.len_utf8())
        .unwrap_or(text.len())
}
fn separator(c: char) -> bool {
    c == '/' || c.is_whitespace()
}
impl Cursor {
    pub fn range(&self, text: &str) -> Range<usize> {
        let a = boundary(text, self.anchor);
        let b = boundary(text, self.caret);
        a.min(b)..a.max(b)
    }
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn select(&mut self, text: &str, anchor: usize, caret: usize) {
        self.anchor = boundary(text, anchor);
        self.caret = boundary(text, caret);
        self.touch();
    }
    pub fn all(&mut self, text: &str) {
        self.select(text, 0, text.len());
    }
    pub fn motion(&mut self, text: &str, motion: Motion, extend: bool) {
        let range = self.range(text);
        let p = boundary(text, self.caret);
        let word_left = || {
            let mut p = p;
            while p > 0 {
                let q = previous(text, p);
                if !separator(text[q..].chars().next().unwrap()) {
                    break;
                }
                p = q;
            }
            while p > 0 {
                let q = previous(text, p);
                if separator(text[q..].chars().next().unwrap()) {
                    break;
                }
                p = q;
            }
            p
        };
        let word_right = || {
            let mut p = p;
            while p < text.len() {
                let c = text[p..].chars().next().unwrap();
                if !separator(c) {
                    break;
                }
                p += c.len_utf8();
            }
            while p < text.len() {
                let c = text[p..].chars().next().unwrap();
                if separator(c) {
                    break;
                }
                p += c.len_utf8();
            }
            p
        };
        let n = match motion {
            Motion::Start => 0,
            Motion::End => text.len(),
            Motion::Left if !extend && !range.is_empty() => range.start,
            Motion::Right if !extend && !range.is_empty() => range.end,
            Motion::Left => previous(text, p),
            Motion::Right => next(text, p),
            Motion::WordLeft => word_left(),
            Motion::WordRight => word_right(),
        };
        if !extend {
            self.anchor = n;
        }
        self.caret = n;
        self.touch();
    }
    fn history_size(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|s| s.text.capacity())
            .sum()
    }
    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > HISTORY_STATES
            || self.history_size() > HISTORY_BYTES
        {
            if !self.undo.is_empty() {
                self.undo.pop_front();
            } else {
                self.redo.pop_front();
            }
        }
    }
    pub fn insert(
        &mut self,
        text: &mut String,
        input: &str,
        replace_all: bool,
    ) -> Result<(), &'static str> {
        self.insert_bounded(text, input, replace_all, crate::omnibar_input::LIMIT)
    }
    pub fn insert_bounded(
        &mut self,
        text: &mut String,
        input: &str,
        replace_all: bool,
        limit: usize,
    ) -> Result<(), &'static str> {
        let range = if replace_all {
            0..text.len()
        } else {
            self.range(text)
        };
        let clean = || input.chars().filter(|c| !matches!(c, '\n' | '\r' | '\0'));
        let mut length = text.len() - range.len();
        let mut inserted = 0usize;
        for c in clean() {
            length = length
                .checked_add(c.len_utf8())
                .filter(|&n| n <= limit)
                .ok_or(crate::omnibar_input::TOO_LONG)?;
            inserted += c.len_utf8();
        }
        if length > limit {
            return Err(crate::omnibar_input::TOO_LONG);
        }
        if range.is_empty() && inserted == 0 {
            return Ok(());
        }
        // Preflight completes before cloning history or copying even the first input byte.
        self.redo.clear();
        self.undo.push_back(Snapshot {
            text: text.clone(),
            caret: self.caret,
            anchor: self.anchor,
        });
        self.trim();
        let clean = clean().collect::<String>();
        text.replace_range(range.clone(), &clean);
        self.caret = range.start + inserted;
        self.anchor = self.caret;
        self.touch();
        Ok(())
    }
    pub fn compose(
        &mut self,
        text: &mut String,
        input: &str,
        delete_bytes: usize,
        replace_all: bool,
    ) -> Result<(), &'static str> {
        self.compose_bounded(
            text,
            input,
            delete_bytes,
            replace_all,
            crate::omnibar_input::LIMIT,
        )
    }
    pub fn composition_range(
        &self,
        text: &str,
        delete_bytes: usize,
        replace_all: bool,
    ) -> std::ops::Range<usize> {
        if replace_all {
            return 0..text.len();
        }
        let range = self.range(text);
        if range.is_empty() {
            boundary(
                text,
                boundary(text, self.caret).saturating_sub(delete_bytes),
            )..range.end
        } else {
            range
        }
    }
    pub fn compose_bounded(
        &mut self,
        text: &mut String,
        input: &str,
        delete_bytes: usize,
        replace_all: bool,
        limit: usize,
    ) -> Result<(), &'static str> {
        let original = (self.caret, self.anchor, self.revision);
        if !replace_all && self.range(text).is_empty() {
            self.anchor = boundary(
                text,
                boundary(text, self.caret).saturating_sub(delete_bytes),
            );
        }
        let changed = replace_all
            || !self.range(text).is_empty()
            || input.chars().any(|c| !matches!(c, '\r' | '\n' | '\0'));
        let result = self.insert_bounded(text, input, replace_all, limit);
        if result.is_err() {
            (self.caret, self.anchor, self.revision) = original;
        } else if changed && let Some(saved) = self.undo.back_mut() {
            saved.caret = original.0;
            saved.anchor = original.1;
        }
        result
    }
    pub fn delete(&mut self, text: &mut String, backward: bool, word: bool, replace_all: bool) {
        if replace_all {
            self.all(text);
        }
        let original = (self.caret, self.anchor);
        if self.range(text).is_empty() {
            self.motion(
                text,
                match (backward, word) {
                    (true, true) => Motion::WordLeft,
                    (true, false) => Motion::Left,
                    (false, true) => Motion::WordRight,
                    (false, false) => Motion::Right,
                },
                true,
            );
        }
        let changed = !self.range(text).is_empty();
        let _ = self.insert(text, "", false);
        if changed && let Some(saved) = self.undo.back_mut() {
            saved.caret = original.0;
            saved.anchor = original.1;
        }
    }
    pub fn undo(&mut self, text: &mut String, redo: bool) {
        let state = if redo {
            self.redo.pop_back()
        } else {
            self.undo.pop_back()
        };
        let Some(state) = state else {
            return;
        };
        let current = Snapshot {
            text: std::mem::replace(text, state.text),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.caret = boundary(text, state.caret);
        self.anchor = boundary(text, state.anchor);
        if redo {
            self.undo.push_back(current);
        } else {
            self.redo.push_back(current);
        }
        self.trim();
        self.touch();
    }
    pub fn component(&mut self, text: &str, at: usize) {
        let p = boundary(text, at);
        let mut a = p;
        let mut b = p;
        while a > 0 {
            let q = previous(text, a);
            if separator(text[q..].chars().next().unwrap()) {
                break;
            }
            a = q;
        }
        while b < text.len() {
            let c = text[b..].chars().next().unwrap();
            if separator(c) {
                break;
            }
            b += c.len_utf8();
        }
        if a == b && b < text.len() {
            b = next(text, b);
        }
        self.select(text, a, b);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_midline_replace_backspace_delete_and_scalar_boundaries() {
        let mut text = "/日本/á😀.txt".to_string();
        let mut c = Cursor::default();
        c.all(&text);
        c.motion(&text, Motion::Left, false);
        assert_eq!(c.caret, 0);
        c.motion(&text, Motion::WordRight, false);
        assert_eq!(&text[..c.caret], "/日本");
        c.motion(&text, Motion::Right, false);
        c.motion(&text, Motion::Right, true);
        assert_eq!(&text[c.range(&text)], "á");
        c.insert(&mut text, "é", false).unwrap();
        assert_eq!(text, "/日本/é😀.txt");
        c.delete(&mut text, false, false, false);
        assert_eq!(text, "/日本/é.txt");
        c.delete(&mut text, true, false, false);
        assert_eq!(text, "/日本/.txt");
        c.undo(&mut text, false);
        assert_eq!(text, "/日本/é.txt");
        c.undo(&mut text, true);
        assert_eq!(text, "/日本/.txt");
    }
    #[test]
    fn rejected_paste_preserves_text_capacity_caret_history_and_revision() {
        let mut c = Cursor::default();
        let mut text = "keep".to_string();
        c.select(&text, 1, 3);
        let before = (
            text.clone(),
            text.capacity(),
            c.caret,
            c.anchor,
            c.revision,
            c.history_size(),
        );
        assert!(
            c.insert(&mut text, &"x".repeat(1024 * 1024), false)
                .is_err()
        );
        assert_eq!(
            (
                text.clone(),
                text.capacity(),
                c.caret,
                c.anchor,
                c.revision,
                c.history_size()
            ),
            before
        );
        c.insert(&mut text, "\ná\r\0", false).unwrap();
        assert_eq!(text, "káp");
    }
    #[test]
    fn history_is_bounded_and_new_edit_drops_redo() {
        let mut c = Cursor::default();
        let mut text = String::new();
        for _ in 0..30 {
            c.insert(&mut text, &"a".repeat(100), false).unwrap();
        }
        assert!(c.history_size() <= HISTORY_BYTES && c.undo.len() + c.redo.len() <= HISTORY_STATES);
        c.undo(&mut text, false);
        let n = text.len();
        c.insert(&mut text, "z", false).unwrap();
        c.undo(&mut text, true);
        assert_eq!(text.len(), n + 1);
    }
}
