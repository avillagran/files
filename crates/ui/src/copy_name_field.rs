//! One visible basename editor over a bounded, captured per-root plan.
use crate::{
    copy_dialog::Editor,
    copy_names::{MAX_NAME_BYTES, Plan},
    line_edit::Motion,
};
use std::{ffi::OsString, sync::Arc};
pub struct Field {
    pub plan: Plan,
    pub row: usize,
    pub draft: String,
    pub editor: Editor,
    dirty: bool,
    opaque: bool,
}
impl Field {
    pub fn new(names: Arc<[OsString]>) -> Self {
        let mut f = Self {
            plan: Plan::new(names),
            row: 0,
            draft: String::new(),
            editor: Editor::default(),
            dirty: false,
            opaque: false,
        };
        f.load();
        f
    }
    fn load(&mut self) {
        let value = self.plan.target(self.row).unwrap_or_default();
        self.opaque = value
            .to_str()
            .is_none_or(|s| s.chars().any(char::is_control))
            || value.len() > MAX_NAME_BYTES;
        self.draft = if self.opaque {
            format!("{value:?}")
        } else {
            value.to_str().unwrap().into()
        };
        if self.draft.len() > 4096 {
            let mut n = 4093;
            while !self.draft.is_char_boundary(n) {
                n -= 1;
            }
            self.draft.truncate(n);
            self.draft.push('…');
        }
        self.editor = Editor::default();
        self.editor.cursor.all(&self.draft);
        self.dirty = false;
    }
    pub fn select_all(&mut self) {
        self.editor.cursor.all(&self.draft);
        self.editor.touch();
    }
    pub fn changed_count(&self) -> usize {
        let base = self.plan.modified();
        if !self.dirty {
            return base;
        }
        let Some(original) = self.plan.names().get(self.row) else {
            return base;
        };
        let stored = self.plan.target(self.row) != Some(original.as_os_str());
        let proposed = std::ffi::OsStr::new(&self.draft) != original.as_os_str();
        match (stored, proposed) {
            (false, true) => base + 1,
            (true, false) => base - 1,
            _ => base,
        }
    }
    pub fn selected_text(&self) -> &str {
        &self.draft[self.editor.cursor.range(&self.draft)]
    }
    pub fn motion(&mut self, m: Motion, shift: bool) {
        self.editor.cursor.motion(&self.draft, m, shift);
        self.editor.touch();
    }
    pub fn input(&mut self, text: &str) -> Result<(), &'static str> {
        if text.chars().any(char::is_control) {
            return Err("El nombre debe ser una línea sin controles");
        }
        if self.opaque {
            if self.editor.cursor.range(&self.draft) != (0..self.draft.len()) {
                return Err("Nombre raw: Ctrl+A para sustituirlo completo");
            }
            if text.len() > MAX_NAME_BYTES {
                return Err("Nombre demasiado largo (máximo255bytes)");
            }
            self.draft.clear();
            self.draft.push_str(text);
            self.editor = Editor::default();
            self.editor.cursor.motion(&self.draft, Motion::End, false);
            self.opaque = false;
        } else {
            self.editor
                .cursor
                .insert_bounded(&mut self.draft, text, false, MAX_NAME_BYTES)?;
        }
        self.dirty = true;
        self.editor.touch();
        Ok(())
    }
    pub fn delete(&mut self, back: bool, word: bool) -> Result<(), &'static str> {
        if self.opaque {
            return self.input("");
        }
        self.editor
            .cursor
            .delete(&mut self.draft, back, word, false);
        self.dirty = true;
        self.editor.touch();
        Ok(())
    }
    pub fn undo(&mut self, redo: bool) {
        if !self.opaque {
            self.editor.cursor.undo(&mut self.draft, redo);
            self.dirty = true;
            self.editor.touch();
        }
    }
    pub fn commit(&mut self) -> Result<(), &'static str> {
        if self.dirty {
            self.plan.set(self.row, &self.draft)?;
            self.dirty = false;
        }
        Ok(())
    }
    pub fn step(&mut self, delta: i32) -> Result<(), &'static str> {
        self.commit()?;
        let next = self
            .row
            .saturating_add_signed(delta as isize)
            .min(self.plan.names().len().saturating_sub(1));
        if next != self.row {
            self.row = next;
            self.load();
        }
        Ok(())
    }
    pub fn reset(&mut self) {
        self.plan.reset(self.row);
        self.load();
    }
    pub fn original_label(&self) -> String {
        let n = self
            .plan
            .names()
            .get(self.row)
            .map(OsString::as_os_str)
            .unwrap_or_default();
        files_core::name_label::label(n).into_owned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn original_review_distinguishes_raw_controls_and_literal_escapes_without_editing_plan() {
        use std::os::unix::ffi::OsStringExt;
        let names: Arc<[OsString]> = vec![
            OsString::from_vec(b"raw-\xff\n.txt".to_vec()),
            OsString::from(r"raw-\xFF\n.txt"),
            OsString::from("bidi-\u{202e}.txt"),
        ]
        .into();
        let mut field = Field::new(names.clone());
        let draft = field.draft.clone();
        assert_eq!(field.original_label(), r"raw-\xFF\n.txt");
        assert_eq!(field.draft, draft);
        field.commit().unwrap();
        assert_eq!(field.plan.target(0), Some(names[0].as_os_str()));
        field.step(1).unwrap();
        assert_eq!(field.original_label(), r"raw-\\xFF\\n.txt");
        field.step(1).unwrap();
        assert_eq!(field.original_label(), r"bidi-\u{202E}.txt");
        assert_eq!(field.plan.modified(), 0);
        for (i, name) in names.iter().enumerate() {
            assert_eq!(field.plan.target(i), Some(name.as_os_str()));
        }
    }
    #[test]
    fn row_switch_commits_names_and_rejects_stale_text_identity() {
        let mut f = Field::new(vec!["a".into(), "b".into()].into());
        let old = f.editor.epoch;
        f.input("other").unwrap();
        f.step(1).unwrap();
        assert_ne!(old, f.editor.epoch);
        assert_eq!(f.plan.target(0), Some(std::ffi::OsStr::new("other")));
        assert_eq!(f.draft, "b");
        f.input("..").unwrap();
        assert!(f.step(-1).is_err());
        assert_eq!(f.row, 1);
        f.reset();
        assert_eq!(f.draft, "b");
    }
    #[cfg(unix)]
    #[test]
    fn raw_presentation_cannot_become_target_without_complete_explicit_edit() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(b"a-\xff".to_vec());
        let mut f = Field::new(vec![raw.clone()].into());
        f.motion(Motion::End, false);
        assert!(f.input("b").is_err());
        f.commit().unwrap();
        assert_eq!(f.plan.target(0), Some(raw.as_os_str()));
        f.select_all();
        f.input("safe").unwrap();
        f.undo(false);
        f.commit().unwrap();
        assert_eq!(f.plan.target(0), Some(std::ffi::OsStr::new("safe")));
    }
    #[test]
    fn changed_count_includes_visible_draft_and_undo_to_original() {
        let mut f = Field::new(vec!["a".into(), "b".into()].into());
        assert_eq!(f.changed_count(), 0);
        f.input("new-a").unwrap();
        assert_eq!(f.changed_count(), 1);
        assert_eq!(f.plan.modified(), 0);
        f.step(1).unwrap();
        f.input("new-b").unwrap();
        assert_eq!(f.changed_count(), 2);
        f.undo(false);
        assert_eq!(f.changed_count(), 1);
        f.step(-1).unwrap();
        f.reset();
        assert_eq!(f.changed_count(), 0);
    }
}
