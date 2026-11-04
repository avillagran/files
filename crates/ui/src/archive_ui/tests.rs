use super::*;
use crate::{
    copy_job,
    copy_state::{Completion, Controller},
    model::Action,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home().join(".cache/files-test");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "archive-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("source.zip"), zip(1)).unwrap();
        Self(path)
    }
    fn prepared(&self, name: &str) -> crate::archive_operation::Prepared {
        crate::archive_operation::Prepared::capture(
            &self.0.join("source.zip"),
            &self.0,
            name.as_ref(),
            &Control::default(),
        )
        .unwrap()
    }
    fn model(&self) -> Model {
        let mut m = Model::new(self.0.clone());
        wait_model(&mut m, |m| m.loaded);
        let i = m
            .entries
            .iter()
            .position(|e| e.name == "source.zip")
            .unwrap();
        m.select_entry(i, false, false);
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn wait_model(m: &mut Model, check: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        m.pump();
        if check(m) {
            return;
        }
        assert!(Instant::now() < end, "{}", m.error);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn ack(c: &mut Controller) -> Box<copy_job::Outcome> {
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let p = c.pump();
        if let Some(Completion::Finished(o)) = p.completion {
            assert!(p.dispatched.is_none());
            return o;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn close(c: &mut Controller) {
    c.begin_close();
    let end = Instant::now() + Duration::from_secs(8);
    while !c.is_closed() {
        c.pump();
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
// Independent tiny stored-ZIP fixture encoder; no product decoder calls.
fn zip(count: usize) -> Vec<u8> {
    fn n16(v: &mut Vec<u8>, n: u16) {
        v.extend(n.to_le_bytes());
    }
    fn n32(v: &mut Vec<u8>, n: u32) {
        v.extend(n.to_le_bytes());
    }
    let data = b"payload";
    let mut crc = !0u32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    crc = !crc;
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    for i in 0..count {
        let name = format!("f{i}.txt");
        let offset = bytes.len() as u32;
        n32(&mut bytes, 0x04034b50);
        for n in [20, 0, 0, 0, 0] {
            n16(&mut bytes, n);
        }
        n32(&mut bytes, crc);
        n32(&mut bytes, data.len() as u32);
        n32(&mut bytes, data.len() as u32);
        n16(&mut bytes, name.len() as u16);
        n16(&mut bytes, 0);
        bytes.extend(name.as_bytes());
        bytes.extend(data);
        n32(&mut central, 0x02014b50);
        for n in [0x314, 20, 0, 0, 0, 0] {
            n16(&mut central, n);
        }
        n32(&mut central, crc);
        n32(&mut central, data.len() as u32);
        n32(&mut central, data.len() as u32);
        n16(&mut central, name.len() as u16);
        for _ in 0..4 {
            n16(&mut central, 0);
        }
        n32(&mut central, 0o100600 << 16);
        n32(&mut central, offset);
        central.extend(name.as_bytes());
    }
    let offset = bytes.len() as u32;
    let size = central.len() as u32;
    bytes.extend(central);
    n32(&mut bytes, 0x06054b50);
    n16(&mut bytes, 0);
    n16(&mut bytes, 0);
    n16(&mut bytes, count as u16);
    n16(&mut bytes, count as u16);
    n32(&mut bytes, size);
    n32(&mut bytes, offset);
    n16(&mut bytes, 0);
    bytes
}
#[test]
fn global_worker_extracts_without_copying_the_zip_or_changing_its_bytes() {
    let f = Fixture::new();
    let before = fs::read(f.0.join("source.zip")).unwrap();
    let prepared = f.prepared("out");
    let mut c = Controller::new();
    let initial = c.budget().accounted_bytes;
    c.enqueue(Request::reviewed_extract(1, prepared)).unwrap();
    assert!(c.budget().accounted_bytes > initial);
    let o = ack(&mut c);
    assert!(o.extracting);
    assert!(!o.moving);
    assert!(o.result.is_ok(), "{:?}", o.result);
    assert_eq!(o.report().unwrap().completed_count, 1);
    assert_eq!(fs::read(f.0.join("out/f0.txt")).unwrap(), b"payload");
    assert_eq!(fs::read(f.0.join("source.zip")).unwrap(), before);
    assert_eq!(c.budget().accounted_bytes, initial);
    close(&mut c);
}
#[test]
fn paused_plan_replacement_fails_and_report_reserve_allows_next_job_after_ack() {
    let f = Fixture::new();
    let prepared = f.prepared("out");
    let mut c = Controller::new();
    let initial = c.budget().accounted_bytes;
    c.enqueue(Request::reviewed_extract(1, prepared).start_paused())
        .unwrap();
    assert_eq!(c.pump().dispatched, Some(1));
    assert!(!f.0.join("out").exists());
    fs::rename(f.0.join("source.zip"), f.0.join("held.zip")).unwrap();
    fs::write(f.0.join("source.zip"), b"replacement").unwrap();
    assert!(c.resume(1));
    let o = ack(&mut c);
    assert!(matches!(o.result, Err(copy_job::Failure::Archive(_))));
    assert_eq!(c.budget().accounted_bytes, initial);
    if let Err(copy_job::Failure::Archive(crate::archive_operation::Failure::Core(e))) = &o.result {
        assert!(e.retained_bytes().unwrap() < c.budget().fixed_headroom_bytes);
    }
    assert!(!f.0.join("out").exists());
    // Outer UI owns at most one result and drops it before dispatching another.
    drop(o);
    c.enqueue(Request::new(
        2,
        vec![files_core::copy_engine::CopyRequest {
            source: f.0.join("source.zip"),
            destination: f.0.join("after-error"),
        }],
        Default::default(),
    ))
    .unwrap();
    assert_eq!(c.pump().dispatched, Some(2));
    assert!(ack(&mut c).result.is_ok());
    assert_eq!(fs::read(f.0.join("after-error")).unwrap(), b"replacement");
    assert_eq!(c.budget().accounted_bytes, initial);
    close(&mut c);
}
#[test]
fn archive_graph_over_queue_budget_is_returned_without_creating_a_stage() {
    let f = Fixture::new();
    fs::write(f.0.join("source.zip"), zip(7000)).unwrap();
    let p = f.prepared("large");
    let mut c = Controller::new();
    let rejected = c.enqueue(Request::reviewed_extract(1, p)).unwrap_err();
    assert_eq!(
        rejected.reason,
        crate::copy_state::RejectReason::MemoryBudget
    );
    assert!(rejected.request.is_extract());
    assert!(!f.0.join("large").exists());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
    close(&mut c);
}
#[test]
fn review_then_apply_uses_the_captured_path_and_rejects_old_button_epochs() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::ExtractArchive);
    let form = m.copy_form.as_ref().unwrap();
    assert!(form.extracting);
    let target = form.destination_path();
    let old = form.editor.epoch;
    m.action(Action::ApplyArchive(old, None));
    wait_model(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|d| !d.preparing)
    });
    let d = m.copy_form.as_ref().unwrap();
    assert!(d.archive.is_some(), "{}", d.error);
    assert!(!target.exists());
    m.action(Action::ApplyArchive(old, None));
    assert!(m.copy_form.is_some());
    assert!(!target.exists());
    let d = m.copy_form.as_ref().unwrap();
    let current = (d.editor.epoch, d.prepare_id);
    m.action(Action::ApplyArchive(current.0, current.1));
    wait_model(&mut m, |m| m.copy_last.is_some());
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(fs::read(target.join("f0.txt")).unwrap(), b"payload");
    m.copy.begin_close();
    while !m.copy.is_closed() {
        m.pump();
    }
}
#[test]
fn editing_destination_discards_the_prepared_token_and_cancel_rejects_late_reply() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::ExtractArchive);
    m.start_copy();
    wait_model(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|d| d.archive.is_some())
    });
    let d = m.copy_form.as_mut().unwrap();
    d.select_all();
    d.input("/different/new-target");
    assert!(d.archive.is_none());
    m.cancel_copy_dialog();
    assert!(m.copy_form.is_none());
    assert!(m.archive_preparing.is_none());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
    m.action(Action::ExtractArchive);
    m.start_copy();
    let id = m.archive_preparing.as_ref().unwrap().0;
    m.cancel_copy_dialog();
    assert!(m.archive_preparing.as_ref().unwrap().1.is_cancelled());
    m.receive_archive(id + 1, Err("wrong id".into()));
    assert!(m.archive_preparing.is_some());
    wait_model(&mut m, |m| m.archive_preparing.is_none());
    assert!(m.copy_form.is_none());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
}

fn sync_recovery(f: &Fixture) -> crate::archive_operation::Failure {
    let p = files_core::archive_extract_fs::Prepared::capture(
        &f.0.join("source.zip"),
        &f.0,
        "published".as_ref(),
        Default::default(),
        &Control::default(),
    )
    .unwrap();
    crate::archive_operation::Failure::Core(
        p.execute_observing_publish(
            &Control::default(),
            |_| {},
            || {
                Err(std::io::Error::other(
                    "private observer failure before sync",
                ))
            },
        )
        .unwrap_err(),
    )
}
#[test]
fn recovery_cannot_enter_pending_queue_and_cancel_preserves_capability() {
    use crate::archive_operation::{Prepared, RecoveryAction};
    let f = Fixture::new();
    let p = Prepared::recovery(sync_recovery(&f), RecoveryAction::Sync).unwrap();
    let mut c = Controller::new();
    let mut rejected = c.enqueue(Request::reviewed_extract(1, p)).unwrap_err();
    assert_eq!(
        rejected.reason,
        crate::copy_state::RejectReason::InputBounds
    );
    let p = rejected.request.take_archive().unwrap();
    c.start_recovery(Request::reviewed_extract(2, p).start_paused())
        .unwrap();
    assert!(c.cancel(2));
    let o = ack(&mut c);
    let Err(copy_job::Failure::Archive(e)) = o.result else {
        panic!()
    };
    assert!(e.cancelled());
    assert!(e.allows(RecoveryAction::Sync));
    fs::remove_file(f.0.join("source.zip")).unwrap();
    let p = Prepared::recovery(e, RecoveryAction::Sync).unwrap();
    c.start_recovery(Request::reviewed_extract(3, p)).unwrap();
    let o = ack(&mut c);
    let report = o.result.unwrap();
    assert_eq!(report.completed_count, 1);
    assert_eq!(fs::read(f.0.join("published/f0.txt")).unwrap(), b"payload");
    close(&mut c);
}
#[test]
fn recovery_review_stamps_and_full_queue_rejection_return_original_result() {
    use crate::archive_operation::RecoveryAction;
    let f = Fixture::new();
    let error = sync_recovery(&f);
    let mut m = f.model();
    m.copy_id = 20;
    m.copy_last = Some(Box::new(copy_job::Outcome {
        exdev: None,
        moving: false,
        extracting: true,
        creating: false,
        archive_action: None,
        id: 1,
        result: Err(copy_job::Failure::Archive(error)),
        progress: Default::default(),
    }));
    m.copy_notice_id = Some(1);
    m.notice_open = true;
    m.copy.hold_queue(true);
    for id in 2..10 {
        m.copy
            .enqueue(Request::new(
                id,
                vec![files_core::copy_engine::CopyRequest {
                    source: f.0.join("source.zip"),
                    destination: f.0.join(format!("pending-{id}")),
                }],
                Default::default(),
            ))
            .unwrap();
    }
    m.review_archive_recovery(999, RecoveryAction::Sync);
    assert!(m.archive_confirmation.is_none());
    m.review_archive_recovery(1, RecoveryAction::Sync);
    let old = m.archive_confirmation.unwrap().stamp;
    m.pause_archive_recovery(old);
    let current = m.archive_confirmation.unwrap().stamp;
    assert_ne!(old, current);
    m.commit_archive_recovery(old);
    assert_eq!(m.copy_last.as_ref().unwrap().id, 1);
    m.commit_archive_recovery(current);
    assert!(m.archive_confirmation.is_none());
    assert_eq!(m.copy_last.as_ref().unwrap().id, 1);
    assert_eq!(m.archive_recovery_actions(), (false, true));
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
    m.review_archive_recovery(1, RecoveryAction::Sync);
    let review = m.archive_confirmation.unwrap().stamp;
    m.clear_before_archive_recovery(review);
    let fresh = m.archive_confirmation.unwrap().stamp;
    m.commit_archive_recovery(review);
    assert!(m.copy_last.is_some());
    m.commit_archive_recovery(fresh);
    wait_model(&mut m, |m| m.copy_last.as_ref().is_some_and(|o| o.id > 20));
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(fs::read_dir(&f.0).unwrap().count(), 2);
    close(&mut m.copy);
}

#[test]
fn create_archive_review_is_readonly_and_apply_uses_captured_names() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::CreateArchive);
    assert!(m.copy_form.as_ref().unwrap().creating);
    let destination = m.copy_form.as_ref().unwrap().destination_path();
    m.start_copy();
    wait_model(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|d| d.archive.is_some())
    });
    assert!(!destination.exists());
    let current = m.copy_form.as_ref().unwrap().editor.epoch;
    m.action(Action::ApplyArchive(current, None));
    assert!(m.copy_form.is_some());
    // A stale prepare-id must not commit. The current pair does.
    let d = m.copy_form.as_ref().unwrap();
    let pair = (d.editor.epoch, d.prepare_id);
    m.action(Action::ApplyArchive(pair.0, pair.1));
    wait_model(&mut m, |m| m.copy_last.is_some());
    let o = m.copy_last.as_ref().unwrap();
    assert!(o.creating);
    assert!(o.result.is_ok());
    assert!(
        crate::copy_report::describe(o)
            .text
            .contains("creación ZIP")
    );
    assert_eq!(fs::read(f.0.join("source.zip")).unwrap(), zip(1));
    assert!(destination.is_file());
    close(&mut m.copy);
}
#[test]
fn create_archive_source_change_after_review_never_publishes() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::CreateArchive);
    m.start_copy();
    wait_model(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|d| d.archive.is_some())
    });
    let d = m.copy_form.as_ref().unwrap();
    let pair = (d.editor.epoch, d.prepare_id);
    let dest = d.destination_path();
    fs::write(f.0.join("source.zip"), b"replacement").unwrap();
    m.action(Action::ApplyArchive(pair.0, pair.1));
    wait_model(&mut m, |m| m.copy_last.is_some());
    let o = m.copy_last.as_ref().unwrap();
    assert!(o.creating && o.result.is_err());
    assert!(!dest.exists());
    assert!(
        crate::copy_report::describe(o)
            .text
            .contains("Creación ZIP incompleta")
    );
    close(&mut m.copy);
}
