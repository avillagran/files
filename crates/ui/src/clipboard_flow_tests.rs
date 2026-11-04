//! Real AuxIo -> private buffer -> dialog -> global worker -> ACK fixtures.
use crate::{
    file_clipboard::Kind,
    model::{Action, Model, View},
};
use std::os::unix::ffi::OsStringExt;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
struct Fixture {
    root: PathBuf,
    a: PathBuf,
    b: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "clipboard-flow-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        fs::create_dir(&root).unwrap();
        let a = root.join("A");
        let b = root.join(OsString::from_vec(b"B-\xfe".to_vec()));
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        for n in ["a", "b", "c"] {
            fs::write(a.join(n), format!("A-{n}")).unwrap();
        }
        fs::write(b.join("same.txt"), b"B distinct").unwrap();
        Self { root, a, b }
    }
    fn model(&self) -> Model {
        let mut m = Model::new(self.a.clone());
        m.view = View::List;
        wait(&mut m, |m| m.loaded);
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn wait(m: &mut Model, p: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        m.pump();
        if p(m) {
            return;
        }
        assert!(
            Instant::now() < end,
            "{} | {} | {:?}",
            m.error,
            m.clipboard_state.error,
            m.operation_message
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn select(m: &mut Model, name: &OsStr, toggle: bool) {
    let i = m.entries.iter().position(|e| e.name == name).unwrap();
    m.select_entry(i, toggle, false);
}
fn capture(m: &mut Model, cut: bool) {
    m.action(if cut {
        Action::ClipboardCut
    } else {
        Action::ClipboardCopy
    });
    wait(m, |m| {
        !m.clipboard_state.busy() && m.clipboard_state.count() > 0
    });
    assert!(m.clipboard_state.error.is_empty());
}
fn paste_review(m: &mut Model, moving: bool) {
    m.action(Action::ClipboardPaste);
    assert!(m.copy_form.as_ref().unwrap().moving == moving);
    if moving {
        m.start_copy();
        wait(m, |m| m.copy_form.as_ref().is_some_and(|d| !d.preparing));
        assert!(
            m.copy_form.as_ref().unwrap().prepared.is_some(),
            "{}",
            m.copy_form.as_ref().unwrap().error
        );
    }
}
fn finish(m: &mut Model, id: u64) {
    wait(m, |m| {
        m.copy_last.as_ref().is_some_and(|o| o.id == id) && !m.copy_busy()
    });
}
#[test]
fn copy_capture_raw_source_survives_navigation_and_keeps_private_buffer() {
    let f = Fixture::new();
    let raw = OsString::from_vec(b"raw-\xff".to_vec());
    fs::write(f.a.join(&raw), b"raw A").unwrap();
    let mut m = f.model();
    select(&mut m, &raw, false);
    capture(&mut m, false);
    let generation = m.clipboard_state.generation();
    m.navigate(f.b.clone());
    wait(&mut m, |m| m.loaded);
    select(&mut m, OsStr::new("same.txt"), false);
    paste_review(&mut m, false);
    let form = m.copy_form.as_ref().unwrap();
    assert_eq!(form.source, f.a);
    assert_eq!(form.destination_path(), f.b);
    assert_eq!(form.names.as_ref(), std::slice::from_ref(&raw));
    m.start_copy();
    let id = m.copy_id;
    finish(&mut m, id);
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(fs::read(f.b.join(&raw)).unwrap(), b"raw A");
    assert_eq!(fs::read(f.a.join(&raw)).unwrap(), b"raw A");
    assert_eq!(fs::read(f.b.join("same.txt")).unwrap(), b"B distinct");
    assert_eq!(m.clipboard_state.generation(), generation);
    assert_eq!(m.clipboard_state.kind(), Some(Kind::Copy));
    assert_eq!(m.clipboard_state.count(), 1);
}
#[test]
fn cut_partial_collision_consumes_only_confirmed_then_retries_original_subset() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, OsStr::new("a"), false);
    select(&mut m, OsStr::new("b"), true);
    capture(&mut m, true);
    assert!(f.a.join("a").exists() && f.a.join("b").exists());
    m.navigate(f.b.clone());
    wait(&mut m, |m| m.loaded);
    paste_review(&mut m, true);
    fs::write(f.b.join("b"), b"actor occupied").unwrap();
    m.start_copy();
    let id = m.copy_id;
    finish(&mut m, id);
    assert!(m.copy_last.as_ref().unwrap().result.is_err());
    assert_eq!(m.clipboard_state.count(), 1);
    assert!(!f.a.join("a").exists());
    assert_eq!(fs::read(f.b.join("a")).unwrap(), b"A-a");
    assert_eq!(fs::read(f.b.join("b")).unwrap(), b"actor occupied");
    assert_eq!(fs::read(f.a.join("b")).unwrap(), b"A-b");
    for _ in 0..100 {
        let lease = m.clipboard_state.lease().unwrap();
        assert_eq!(lease.names.as_ref(), [OsString::from("b")]);
        drop(lease);
    }
    fs::remove_file(f.b.join("b")).unwrap();
    m.action(Action::DismissNotice);
    paste_review(&mut m, true);
    assert_eq!(
        m.copy_form.as_ref().unwrap().names.as_ref(),
        [OsString::from("b")]
    );
    m.start_copy();
    let id = m.copy_id;
    finish(&mut m, id);
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(m.clipboard_state.count(), 0);
    assert!(!f.a.join("b").exists());
    assert_eq!(fs::read(f.b.join("b")).unwrap(), b"A-b");
}
#[test]
fn changed_cut_source_rejects_without_moving_replacement() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, OsStr::new("a"), false);
    capture(&mut m, true);
    fs::rename(f.a.join("a"), f.a.join("preserved")).unwrap();
    fs::write(f.a.join("a"), b"actor replacement").unwrap();
    m.navigate(f.b.clone());
    wait(&mut m, |m| m.loaded);
    m.action(Action::ClipboardPaste);
    m.start_copy();
    wait(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|d| !d.preparing)
    });
    let form = m.copy_form.as_ref().unwrap();
    assert!(form.prepared.is_none() && !form.error.is_empty());
    assert_eq!(m.clipboard_state.count(), 1);
    assert_eq!(fs::read(f.a.join("a")).unwrap(), b"actor replacement");
    assert_eq!(fs::read(f.a.join("preserved")).unwrap(), b"A-a");
    assert!(!f.b.join("a").exists());
}
#[test]
fn old_cut_ack_does_not_consume_new_clipboard_during_paused_job() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, OsStr::new("a"), false);
    capture(&mut m, true);
    m.navigate(f.b.clone());
    wait(&mut m, |m| m.loaded);
    paste_review(&mut m, true);
    m.copy_form.as_mut().unwrap().start_paused = true;
    m.start_copy();
    let id = m.copy_id;
    wait(&mut m, |m| {
        m.copy.phase() == crate::copy_state::Phase::Paused
    });
    let old = m.clipboard_state.generation();
    m.navigate(f.a.clone());
    wait(&mut m, |m| m.loaded);
    select(&mut m, OsStr::new("c"), false);
    capture(&mut m, false);
    let new = m.clipboard_state.generation();
    assert_ne!(old, new);
    m.action(Action::CopyPause(id, false));
    finish(&mut m, id);
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(m.clipboard_state.generation(), new);
    assert_eq!(m.clipboard_state.kind(), Some(Kind::Copy));
    assert_eq!(
        m.clipboard_state.lease().unwrap().names.as_ref(),
        [OsString::from("c")]
    );
    assert_eq!(fs::read(f.b.join("a")).unwrap(), b"A-a");
}
#[test]
fn model_text_tokens_reject_aba_and_closed_context_without_file_operation() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::Omni);
    let a = m.text_context();
    assert!(a.is_some());
    assert!(m.text_paste.request(a).is_some());
    m.editing = false;
    m.action(Action::NewEntry(false));
    let b = m.text_context();
    assert!(b.is_some() && b != a);
    let initial = m.rename_dialog.as_ref().unwrap().fields.clone();
    assert!(m.text_paste.request(b).is_none());
    assert!(!m.receive_text_paste("file:///should-not-be-files"));
    assert_eq!(m.rename_dialog.as_ref().unwrap().fields, initial);
    assert!(m.text_paste.request(b).is_some());
    assert!(m.receive_text_paste("owned-text.txt"));
    assert_eq!(
        m.rename_dialog.as_ref().unwrap().fields[0],
        "owned-text.txt"
    );
    assert!(m.copy_form.is_none() && !m.copy_busy());
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    m.closing = true;
    assert!(!m.receive_text_paste("late"));
    assert_eq!(
        m.rename_dialog.as_ref().unwrap().fields[0],
        "owned-text.txt"
    );
    assert!(!m.text_paste.pending());
}
