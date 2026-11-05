//! The text of a [`TextArea`](super::TextArea) without any widget: UTF-8
//! text, caret and selection (byte offsets on grapheme boundaries), atomic
//! chips, input-method preedit, and undo/redo with coalesced typing
//! groups. Every edit goes through [`EditBuffer::replace`], so undo, chips
//! and the layout see the same change list.

use std::ops::Range;
use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

/// An atomic token inside the text (`@src/main.rs`, a skill, an attached
/// image): drawn as a chip, the caret never stops inside it and Backspace
/// or Delete remove it whole. `data` is the app's payload (a path, an ID,
/// a full reference) and is not shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// Bytes of the text it covers.
    pub range: Range<usize>,
    pub data: String,
}

/// What kind of edit a step is, for undo grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Typed text (and input-method composition): consecutive keystrokes
    /// undo together, one word at a time.
    Typing,
    /// Backspace runs.
    DeleteBack,
    /// Delete runs.
    DeleteForward,
    /// Paste, cut, chips, completions, programmatic changes: one step each.
    Other,
}

/// One change of the text: `at`, the length it had, the length it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub at: usize,
    pub old_len: usize,
    pub new_len: usize,
}

#[derive(Debug, Clone)]
struct Step {
    at: usize,
    removed: String,
    /// Chips removed with the text, relative to `at`.
    removed_chips: Vec<Chip>,
    inserted: String,
    inserted_chips: Vec<Chip>,
    /// (anchor, caret) before and after.
    before: (usize, usize),
    after: (usize, usize),
    kind: EditKind,
}

const UNDO_LIMIT: usize = 1000;

/// Text, caret, selection, chips and history (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct EditBuffer {
    text: String,
    caret: usize,
    anchor: usize,
    chips: Vec<Chip>,
    preedit: Option<Range<usize>>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The next step may merge into the last one.
    open_group: bool,
    changes: Vec<Change>,
    rev: u64,
}

fn shift(chips: &[Chip], by: usize) -> Vec<Chip> {
    chips
        .iter()
        .map(|c| Chip {
            range: c.range.start + by..c.range.end + by,
            data: c.data.clone(),
        })
        .collect()
}

impl EditBuffer {
    pub fn new(text: &str) -> EditBuffer {
        let mut b = EditBuffer::default();
        b.reset(text);
        b
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Caret byte offset (the moving end of the selection).
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The fixed end of the selection (equal to the caret when none).
    pub fn anchor(&self) -> usize {
        self.anchor
    }

    /// The selected bytes, in order (empty at the caret).
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }

    pub fn has_selection(&self) -> bool {
        self.anchor != self.caret
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    pub fn chips(&self) -> &[Chip] {
        &self.chips
    }

    /// The input method's uncommitted text (drawn underlined), if any.
    pub fn preedit(&self) -> Option<Range<usize>> {
        self.preedit.clone()
    }

    /// Increases on every text change.
    pub fn revision(&self) -> u64 {
        self.rev
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The text changes since the last call, in order (for the layout).
    pub fn take_changes(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changes)
    }

    /// Replaces everything and forgets the history (caret at the end).
    pub fn reset(&mut self, text: &str) {
        let old = self.text.len();
        self.text = text.to_string();
        self.chips.clear();
        self.preedit = None;
        self.undo.clear();
        self.redo.clear();
        self.open_group = false;
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.changes.push(Change {
            at: 0,
            old_len: old,
            new_len: self.text.len(),
        });
        self.rev += 1;
    }

    /// Forgets undo and redo.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open_group = false;
    }

    /// The next edit starts a new undo step (caret moved, focus changed).
    pub fn break_group(&mut self) {
        self.open_group = false;
    }

    // ------------------------------------------------------------ positions

    /// The chip that strictly contains `pos` (start < pos < end).
    pub fn chip_inside(&self, pos: usize) -> Option<&Chip> {
        let i = self.chips.partition_point(|c| c.range.end <= pos);
        self.chips
            .get(i)
            .filter(|c| c.range.start < pos && pos < c.range.end)
    }

    /// The chip ending exactly at `pos`.
    pub fn chip_ending_at(&self, pos: usize) -> Option<&Chip> {
        let i = self.chips.partition_point(|c| c.range.end < pos);
        self.chips.get(i).filter(|c| c.range.end == pos)
    }

    /// The chip starting exactly at `pos`.
    pub fn chip_starting_at(&self, pos: usize) -> Option<&Chip> {
        let i = self.chips.partition_point(|c| c.range.start < pos);
        self.chips.get(i).filter(|c| c.range.start == pos)
    }

    /// `pos` moved onto a char boundary, out of chips (to the nearer edge,
    /// or the start/end when `toward_end` says which way).
    pub fn snap(&self, pos: usize, toward_end: Option<bool>) -> usize {
        let mut p = pos.min(self.text.len());
        while !self.text.is_char_boundary(p) {
            p -= 1;
        }
        if let Some(c) = self.chip_inside(p) {
            let to_end = toward_end.unwrap_or(p - c.range.start > c.range.end - p);
            p = if to_end { c.range.end } else { c.range.start };
        }
        p
    }

    /// The next caret stop after `pos`: the next grapheme boundary, or the
    /// end of a chip.
    pub fn next_stop(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }
        if let Some(c) = self.chip_starting_at(pos) {
            return c.range.end;
        }
        let mut gc = GraphemeCursor::new(pos, self.text.len(), true);
        let n = gc
            .next_boundary(&self.text, 0)
            .ok()
            .flatten()
            .unwrap_or(self.text.len());
        self.snap(n, Some(true))
    }

    /// The previous caret stop before `pos`.
    pub fn prev_stop(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        if let Some(c) = self.chip_ending_at(pos) {
            return c.range.start;
        }
        let mut gc = GraphemeCursor::new(pos, self.text.len(), true);
        let p = gc.prev_boundary(&self.text, 0).ok().flatten().unwrap_or(0);
        self.snap(p, Some(false))
    }

    /// Start of the paragraph (text after the previous '\n') holding `pos`.
    pub fn para_start(&self, pos: usize) -> usize {
        self.text[..pos].rfind('\n').map_or(0, |i| i + 1)
    }

    /// End of the paragraph holding `pos` (before its '\n').
    pub fn para_end(&self, pos: usize) -> usize {
        self.text[pos..]
            .find('\n')
            .map_or(self.text.len(), |i| pos + i)
    }

    /// Start of the word before `pos` (chips count as words); crosses into
    /// the previous paragraph when `pos` starts one.
    pub fn word_left(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        if let Some(c) = self.chip_ending_at(pos) {
            return c.range.start;
        }
        let ps = self.para_start(pos);
        let lo = if ps == pos {
            self.para_start(pos - 1)
        } else {
            ps
        };
        // Never look through a chip: stop at the last one before pos.
        let i = self.chips.partition_point(|c| c.range.end <= pos);
        let lo2 = match i.checked_sub(1).map(|j| &self.chips[j]) {
            Some(c) if c.range.end > lo => c.range.end,
            _ => lo,
        };
        let mut best = None;
        for (i, w) in self.text[lo2..pos].split_word_bound_indices() {
            if w.chars().any(char::is_alphanumeric) {
                best = Some(lo2 + i);
            }
        }
        match best {
            Some(b) => b,
            None if lo2 > lo => self.chip_ending_at(lo2).map_or(lo2, |c| c.range.start),
            None => lo,
        }
    }

    /// End of the word after `pos` (chips count as words); crosses into the
    /// next paragraph when `pos` ends one.
    pub fn word_right(&self, pos: usize) -> usize {
        let len = self.text.len();
        if pos >= len {
            return len;
        }
        if let Some(c) = self.chip_starting_at(pos) {
            return c.range.end;
        }
        let mut hi = self.para_end(pos);
        if hi == pos {
            hi = self.para_end(pos + 1);
        }
        let i = self.chips.partition_point(|c| c.range.start <= pos);
        let next_chip = self.chips.get(i).filter(|c| c.range.start < hi);
        let hi2 = next_chip.map_or(hi, |c| c.range.start);
        for (i, w) in self.text[pos..hi2].split_word_bound_indices() {
            if w.chars().any(char::is_alphanumeric) {
                return pos + i + w.len();
            }
        }
        next_chip.map_or(hi, |c| c.range.end)
    }

    /// The word (or chip) around `pos`, for double clicks.
    pub fn word_at(&self, pos: usize) -> Range<usize> {
        if let Some(c) = self.chip_inside(pos).or_else(|| self.chip_starting_at(pos)) {
            return c.range.clone();
        }
        let (lo, hi) = (self.para_start(pos), self.para_end(pos));
        for (i, w) in self.text[lo..hi].split_word_bound_indices() {
            let (s, e) = (lo + i, lo + i + w.len());
            if pos >= s && pos < e || (pos == e && pos == hi) {
                return self.snap(s, Some(false))..self.snap(e, Some(true));
            }
        }
        pos..pos
    }

    // ------------------------------------------------------------ caret

    /// Moves the caret to `pos` (snapped); `extend` keeps the anchor.
    pub fn move_to(&mut self, pos: usize, extend: bool) {
        let toward_end = Some(pos > self.caret);
        self.caret = self.snap(pos, toward_end);
        if !extend {
            self.anchor = self.caret;
        }
        self.open_group = false;
    }

    /// Sets anchor and caret (snapped out of chips, grown to cover them).
    pub fn select(&mut self, anchor: usize, caret: usize) {
        let fwd = caret >= anchor;
        self.anchor = self.snap(anchor, Some(!fwd));
        self.caret = self.snap(caret, Some(fwd));
        self.open_group = false;
    }

    pub fn select_all(&mut self) {
        self.select(0, self.text.len());
    }

    /// Left/Right by one grapheme (or chip); without `extend` a selection
    /// collapses to its start or end first.
    pub fn step(&mut self, forward: bool, extend: bool) {
        if !extend && self.has_selection() {
            let r = self.selection();
            let p = if forward { r.end } else { r.start };
            self.move_to(p, false);
            return;
        }
        let p = if forward {
            self.next_stop(self.caret)
        } else {
            self.prev_stop(self.caret)
        };
        self.move_to(p, extend);
    }

    // ------------------------------------------------------------ editing

    /// `range` grown to whole chips (an edit never cuts a chip).
    pub fn expand_to_chips(&self, range: Range<usize>) -> Range<usize> {
        let mut s = range.start.min(self.text.len());
        let mut e = range.end.clamp(s, self.text.len());
        while !self.text.is_char_boundary(s) {
            s -= 1;
        }
        while !self.text.is_char_boundary(e) {
            e += 1;
        }
        if let Some(c) = self.chip_inside(s) {
            s = c.range.start;
        }
        if let Some(c) = self.chip_inside(e) {
            e = c.range.end;
        }
        s..e
    }

    /// Replaces `range` with `text` (plus `chips`, relative to the start of
    /// `text`), records an undo step of `kind` and puts the caret after the
    /// inserted text. The range is grown to whole chips first.
    pub fn replace(&mut self, range: Range<usize>, text: &str, chips: Vec<Chip>, kind: EditKind) {
        let range = self.expand_to_chips(range);
        if range.is_empty() && text.is_empty() {
            return;
        }
        let before = (self.anchor, self.caret);
        let (removed, removed_chips) =
            self.apply(range.start, range.end - range.start, text, &chips);
        let caret = range.start + text.len();
        self.caret = caret;
        self.anchor = caret;
        self.preedit = None;
        let step = Step {
            at: range.start,
            removed,
            removed_chips,
            inserted: text.to_string(),
            inserted_chips: chips,
            before,
            after: (caret, caret),
            kind,
        };
        self.record(step);
    }

    /// Low level: edits the text and chips, returns what was removed.
    fn apply(
        &mut self,
        at: usize,
        old_len: usize,
        text: &str,
        chips: &[Chip],
    ) -> (String, Vec<Chip>) {
        let end = at + old_len;
        let removed = self.text[at..end].to_string();
        self.text.replace_range(at..end, text);
        let delta = text.len() as isize - old_len as isize;
        let mut removed_chips = Vec::new();
        let mut kept = Vec::with_capacity(self.chips.len() + chips.len());
        for c in self.chips.drain(..) {
            if c.range.end <= at {
                kept.push(c);
            } else if c.range.start >= end {
                let s = (c.range.start as isize + delta) as usize;
                let e = (c.range.end as isize + delta) as usize;
                kept.push(Chip {
                    range: s..e,
                    data: c.data,
                });
            } else if c.range.start >= at && c.range.end <= end {
                removed_chips.push(Chip {
                    range: c.range.start - at..c.range.end - at,
                    data: c.data,
                });
            }
            // A chip cut by the edit (programmatic, not grown) is dropped.
        }
        for c in chips {
            if c.range.end <= text.len() && c.range.start < c.range.end {
                kept.push(Chip {
                    range: c.range.start + at..c.range.end + at,
                    data: c.data.clone(),
                });
            }
        }
        kept.sort_by_key(|c| c.range.start);
        self.chips = kept;
        self.changes.push(Change {
            at,
            old_len,
            new_len: text.len(),
        });
        self.rev += 1;
        (removed, removed_chips)
    }

    fn record(&mut self, step: Step) {
        self.redo.clear();
        if self.open_group {
            if let Some(last) = self.undo.last_mut() {
                if merge(last, &step) {
                    return;
                }
            }
        }
        self.undo.push(step);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.open_group = true;
    }

    /// Typed text replaces the selection (or goes in at the caret).
    pub fn insert(&mut self, text: &str) {
        let r = self.selection();
        self.replace(r, text, Vec::new(), EditKind::Typing);
    }

    /// Inserted as one undo step (paste, drop).
    pub fn paste(&mut self, text: &str) {
        let r = self.selection();
        self.replace(r, text, Vec::new(), EditKind::Other);
        self.open_group = false;
    }

    /// Inserts `text` as one chip carrying `data` (newlines become spaces).
    pub fn insert_chip(&mut self, range: Range<usize>, text: &str, data: &str) {
        let t = text.replace(['\n', '\r'], " ");
        let len = t.len();
        self.replace(
            range,
            &t,
            vec![Chip {
                range: 0..len,
                data: data.to_string(),
            }],
            EditKind::Other,
        );
        self.open_group = false;
    }

    /// Makes `range` (existing text) a chip.
    pub fn make_chip(&mut self, range: Range<usize>, data: &str) {
        let r = self.expand_to_chips(range);
        if r.is_empty() {
            return;
        }
        let text = self.text[r.clone()].to_string();
        let (a, c) = (self.anchor, self.caret);
        self.insert_chip(r, &text, data);
        self.anchor = self.snap(a, None);
        self.caret = self.snap(c, None);
    }

    /// Backspace: the selection, else the chip or grapheme before the caret.
    pub fn backspace(&mut self) {
        if self.has_selection() {
            let r = self.selection();
            self.replace(r, "", Vec::new(), EditKind::Other);
            self.open_group = false;
            return;
        }
        let p = self.prev_stop(self.caret);
        if p < self.caret {
            self.replace(p..self.caret, "", Vec::new(), EditKind::DeleteBack);
        }
    }

    /// Delete: the selection, else the chip or grapheme after the caret.
    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            let r = self.selection();
            self.replace(r, "", Vec::new(), EditKind::Other);
            self.open_group = false;
            return;
        }
        let n = self.next_stop(self.caret);
        if n > self.caret {
            let c = self.caret;
            self.replace(c..n, "", Vec::new(), EditKind::DeleteForward);
            self.caret = c;
            self.anchor = c;
            if let Some(s) = self.undo.last_mut() {
                s.after = (c, c);
            }
        }
    }

    /// Deletes from the caret to `to` (word or line deletion), one step.
    pub fn delete_to(&mut self, to: usize) {
        if self.has_selection() {
            self.backspace();
            return;
        }
        let (s, e) = (to.min(self.caret), to.max(self.caret));
        if s < e {
            self.replace(s..e, "", Vec::new(), EditKind::Other);
            self.open_group = false;
        }
    }

    /// Input-method text: replaces the `del` bytes before the caret (the
    /// previous composition) or the selection, and marks the last `marked`
    /// bytes of `text` as uncommitted (preedit).
    pub fn compose(&mut self, del: usize, text: &str, marked: usize) {
        let range = if del > 0 {
            let mut s = self.caret.saturating_sub(del);
            while !self.text.is_char_boundary(s) {
                s -= 1;
            }
            s..self.caret
        } else {
            self.selection()
        };
        if range.is_empty() && text.is_empty() {
            self.preedit = None;
            return;
        }
        self.replace(range, text, Vec::new(), EditKind::Typing);
        let m = marked.min(text.len());
        self.preedit = (m > 0).then(|| self.caret - m..self.caret);
        // The composition keeps one undo group.
        self.open_group = true;
    }

    /// The uncommitted text becomes normal text.
    pub fn commit_preedit(&mut self) {
        self.preedit = None;
    }

    /// Replaces the whole text as one undoable step (caret at the end).
    pub fn set_text(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        let len = self.text.len();
        self.replace(0..len, text, Vec::new(), EditKind::Other);
        self.open_group = false;
    }

    /// Replaces the chips (ranges must be on char boundaries, sorted
    /// order is restored; overlapping or out of range ones are dropped).
    pub fn set_chips(&mut self, chips: Vec<Chip>) {
        let mut c: Vec<Chip> = chips
            .into_iter()
            .filter(|c| {
                c.range.start < c.range.end
                    && c.range.end <= self.text.len()
                    && self.text.is_char_boundary(c.range.start)
                    && self.text.is_char_boundary(c.range.end)
            })
            .collect();
        c.sort_by_key(|c| c.range.start);
        let mut out: Vec<Chip> = Vec::with_capacity(c.len());
        for x in c {
            if out.last().map_or(true, |l| l.range.end <= x.range.start) {
                out.push(x);
            }
        }
        self.chips = out;
        self.caret = self.snap(self.caret, None);
        self.anchor = self.snap(self.anchor, None);
        let len = self.text.len();
        self.changes.push(Change {
            at: 0,
            old_len: len,
            new_len: len,
        });
        self.rev += 1;
    }

    pub fn undo(&mut self) -> bool {
        let Some(s) = self.undo.pop() else {
            return false;
        };
        self.apply(
            s.at,
            s.inserted.len(),
            &s.removed.clone(),
            &s.removed_chips.clone(),
        );
        self.anchor = s.before.0.min(self.text.len());
        self.caret = s.before.1.min(self.text.len());
        self.preedit = None;
        self.redo.push(s);
        self.open_group = false;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(s) = self.redo.pop() else {
            return false;
        };
        self.apply(
            s.at,
            s.removed.len(),
            &s.inserted.clone(),
            &s.inserted_chips.clone(),
        );
        self.anchor = s.after.0.min(self.text.len());
        self.caret = s.after.1.min(self.text.len());
        self.preedit = None;
        self.undo.push(s);
        self.open_group = false;
        true
    }

    /// The text with every chip replaced by `f(chip_text, data)` (e.g. the
    /// full reference behind a short chip label).
    pub fn expanded(&self, mut f: impl FnMut(&str, &str) -> String) -> String {
        let mut out = String::with_capacity(self.text.len());
        let mut at = 0;
        for c in &self.chips {
            out.push_str(&self.text[at..c.range.start]);
            out.push_str(&f(&self.text[c.range.clone()], &c.data));
            at = c.range.end;
        }
        out.push_str(&self.text[at..]);
        out
    }

    /// Number of characters (not bytes) before `pos` (accessibility).
    pub fn char_index(&self, pos: usize) -> usize {
        self.text[..pos.min(self.text.len())].chars().count()
    }

    /// Byte offset of character `ch` (accessibility).
    pub fn byte_index(&self, ch: usize) -> usize {
        self.text
            .char_indices()
            .nth(ch)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Number of grapheme clusters (tests, counters).
    pub fn graphemes(&self) -> usize {
        self.text.graphemes(true).count()
    }
}

/// Folds `b` into `a` when they form one undo group. Typing groups end
/// after a word: whitespace followed by a non-blank starts a new group.
fn merge(a: &mut Step, b: &Step) -> bool {
    if a.kind != b.kind || b.kind == EditKind::Other {
        return false;
    }
    match b.kind {
        EditKind::Typing => {
            let a_end = a.at + a.inserted.len();
            // Plain typing at the end of the group.
            if b.at == a_end && b.removed.is_empty() {
                let ws_then_word = a
                    .inserted
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
                    && b.inserted
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_whitespace());
                if ws_then_word || b.inserted.contains('\n') || a.inserted.ends_with('\n') {
                    return false;
                }
                a.inserted.push_str(&b.inserted);
                a.inserted_chips
                    .extend(shift(&b.inserted_chips, b.at - a.at));
                a.after = b.after;
                return true;
            }
            // A composition replacing the tail of what this group typed.
            if b.at >= a.at
                && b.at + b.removed.len() == a_end
                && a.inserted[b.at - a.at..] == b.removed
            {
                a.inserted.truncate(b.at - a.at);
                a.inserted.push_str(&b.inserted);
                a.after = b.after;
                return true;
            }
            false
        }
        EditKind::DeleteBack => {
            if b.inserted.is_empty() && a.inserted.is_empty() && b.at + b.removed.len() == a.at {
                let mut chips = b.removed_chips.clone();
                chips.extend(shift(&a.removed_chips, b.removed.len()));
                a.removed = format!("{}{}", b.removed, a.removed);
                a.removed_chips = chips;
                a.at = b.at;
                a.after = b.after;
                return true;
            }
            false
        }
        EditKind::DeleteForward => {
            if b.inserted.is_empty() && a.inserted.is_empty() && b.at == a.at {
                a.removed_chips
                    .extend(shift(&b.removed_chips, a.removed.len()));
                a.removed.push_str(&b.removed);
                a.after = b.after;
                return true;
            }
            false
        }
        EditKind::Other => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(b: &mut EditBuffer, s: &str) {
        for g in s.graphemes(true) {
            b.insert(g);
        }
    }

    #[test]
    fn typing_undoes_one_word_at_a_time() {
        let mut b = EditBuffer::new("");
        typed(&mut b, "hello brave world");
        assert_eq!(b.text(), "hello brave world");
        assert!(b.undo());
        assert_eq!(b.text(), "hello brave ");
        assert!(b.undo());
        assert_eq!(b.text(), "hello ");
        assert!(b.undo());
        assert_eq!(b.text(), "");
        assert!(!b.undo());
        assert!(b.redo() && b.redo());
        assert_eq!(b.text(), "hello brave ");
        assert_eq!(b.caret(), 12);
    }

    #[test]
    fn caret_moves_break_the_group() {
        let mut b = EditBuffer::new("");
        typed(&mut b, "ab");
        b.step(false, false);
        typed(&mut b, "X");
        assert_eq!(b.text(), "aXb");
        b.undo();
        assert_eq!(b.text(), "ab");
        assert_eq!(b.caret(), 1);
    }

    #[test]
    fn backspace_runs_undo_together_and_restore_the_caret() {
        let mut b = EditBuffer::new("");
        typed(&mut b, "abcdef");
        b.break_group();
        for _ in 0..3 {
            b.backspace();
        }
        assert_eq!(b.text(), "abc");
        b.undo();
        assert_eq!(b.text(), "abcdef");
        assert_eq!(b.caret(), 6);
        b.move_to(0, false);
        b.delete_forward();
        b.delete_forward();
        assert_eq!(b.text(), "cdef");
        assert_eq!(b.caret(), 0);
        b.undo();
        assert_eq!(b.text(), "abcdef");
    }

    #[test]
    fn graphemes_are_deleted_whole() {
        // Family emoji (ZWJ sequence), flag, e + combining acute, CJK, Hangul jamo.
        let s = "a👨‍👩‍👧🇨🇱e\u{301}漢\u{1100}\u{1161}";
        let mut b = EditBuffer::new(s);
        assert_eq!(b.graphemes(), 6);
        b.backspace();
        assert_eq!(b.text(), "a👨‍👩‍👧🇨🇱e\u{301}漢");
        b.backspace();
        assert_eq!(b.text(), "a👨‍👩‍👧🇨🇱e\u{301}");
        b.backspace();
        assert_eq!(b.text(), "a👨‍👩‍👧🇨🇱");
        b.backspace();
        assert_eq!(b.text(), "a👨‍👩‍👧");
        b.backspace();
        assert_eq!(b.text(), "a");
        let mut b = EditBuffer::new(s);
        b.move_to(0, false);
        b.step(true, false);
        b.step(true, false);
        assert_eq!(&b.text()[1..b.caret()], "👨‍👩‍👧");
        b.step(true, true);
        assert_eq!(b.selected_text(), "🇨🇱");
        // A caret put inside a cluster's bytes is moved to a char boundary.
        b.move_to(3, false);
        assert!(b.text().is_char_boundary(b.caret()));
    }

    #[test]
    fn chips_are_atomic() {
        let mut b = EditBuffer::new("see  now");
        b.move_to(4, false);
        b.insert_chip(4..4, "@main.rs", "/src/main.rs");
        assert_eq!(b.text(), "see @main.rs now");
        assert_eq!(
            b.chips(),
            &[Chip {
                range: 4..12,
                data: "/src/main.rs".into()
            }]
        );
        // Arrows jump over it, clicks inside snap out.
        b.move_to(12, false);
        b.step(false, false);
        assert_eq!(b.caret(), 4);
        b.step(true, false);
        assert_eq!(b.caret(), 12);
        b.move_to(7, false);
        assert!(b.caret() == 4 || b.caret() == 12);
        // Backspace after it removes it whole; undo brings chip and text back.
        b.move_to(12, false);
        b.backspace();
        assert_eq!(b.text(), "see  now");
        assert!(b.chips().is_empty());
        b.undo();
        assert_eq!(b.text(), "see @main.rs now");
        assert_eq!(b.chips().len(), 1);
        // Typing before it shifts it.
        b.move_to(0, false);
        b.insert("I ");
        assert_eq!(b.chips()[0].range, 6..14);
        // A selection edge inside a chip grows to cover it.
        b.select(8, 2);
        assert_eq!(b.selection(), 2..14);
        b.insert("x");
        assert_eq!(b.text(), "I x now");
        assert!(b.chips().is_empty());
        assert_eq!(b.expanded(|t, _| t.to_string()), "I x now");
    }

    #[test]
    fn expanded_text_uses_chip_data() {
        let mut b = EditBuffer::new("");
        b.insert("look ");
        b.insert_chip(5..5, "img.png", "![img.png](ctx://1)");
        b.insert(" ok");
        assert_eq!(
            b.expanded(|_, d| d.to_string()),
            "look ![img.png](ctx://1) ok"
        );
    }

    #[test]
    fn words_and_paragraphs() {
        let b = EditBuffer::new("one two-three  four\nfive");
        assert_eq!(b.word_left(19), 15);
        assert_eq!(b.word_left(15), 8);
        assert_eq!(b.word_left(4), 0);
        assert_eq!(b.word_right(0), 3);
        assert_eq!(b.word_right(3), 7);
        assert_eq!(b.word_right(19), 24);
        assert_eq!(b.word_left(20), 15);
        assert_eq!(b.word_at(9), 8..13);
        assert_eq!(b.para_start(22), 20);
        assert_eq!(b.para_end(2), 19);
        let mut c = EditBuffer::new("ab @x cd");
        c.make_chip(3..5, "x");
        assert_eq!(c.word_left(5), 3);
        assert_eq!(c.word_right(3), 5);
        assert_eq!(c.word_at(4), 3..5);
    }

    #[test]
    fn compose_replaces_the_marked_text_in_one_group() {
        let mut b = EditBuffer::new("x");
        // Dead key: "´" shown marked, then replaced by "é".
        b.compose(0, "´", 2);
        assert_eq!(b.preedit(), Some(1..3));
        b.compose(2, "é", 0);
        assert_eq!(b.text(), "xé");
        assert_eq!(b.preedit(), None);
        // CJK: "n" → "に" → "日本" committed.
        b.compose(0, "n", 1);
        b.compose(1, "に", 3);
        b.compose(3, "日本", 0);
        assert_eq!(b.text(), "xé日本");
        b.undo();
        assert_eq!(b.text(), "x");
    }

    #[test]
    fn paste_and_set_text_are_single_steps() {
        let mut b = EditBuffer::new("");
        typed(&mut b, "ab");
        b.paste("PASTED");
        typed(&mut b, "c");
        b.undo();
        assert_eq!(b.text(), "abPASTED");
        b.undo();
        assert_eq!(b.text(), "ab");
        b.set_text("new");
        assert_eq!(b.caret(), 3);
        b.undo();
        assert_eq!(b.text(), "ab");
        let ch = b.take_changes();
        assert!(!ch.is_empty());
        assert!(b.take_changes().is_empty());
    }

    #[test]
    fn char_indices_for_accessibility() {
        let b = EditBuffer::new("añ😀b");
        assert_eq!(b.char_index(b.len()), 4);
        assert_eq!(b.byte_index(3), 7);
        assert_eq!(b.byte_index(9), b.len());
    }
}
