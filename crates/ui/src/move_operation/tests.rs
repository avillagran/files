use super::*;
use crate::{
    copy_job::Request,
    copy_state::{Completion, Controller, RejectReason},
    painter::{Image, Painter, Rect},
};
use files_core::{
    exdev_source,
    move_journal::Journal,
    move_manifest::{Identity, Kind, Manifest},
};
use std::{
    ffi::OsStr,
    fs::{self, File},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
struct Fixture {
    root: PathBuf,
    context: Context,
}
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(std::env::var_os("HOME").unwrap()).join(format!(
            ".cache/files-test/exdev-ui-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        for part in ["source", "destination", "journals"] {
            fs::create_dir_all(root.join(part)).unwrap();
            fs::set_permissions(root.join(part), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let source = root.join("source/item");
        let destination = root.join("destination/item");
        fs::write(&source, b"fixture untouched").unwrap();
        let source_parent = File::open(source.parent().unwrap()).unwrap();
        let destination_parent = File::open(destination.parent().unwrap()).unwrap();
        let inventory =
            exdev_source::capture(&source_parent, OsStr::new("item"), &AtomicBool::new(false))
                .unwrap();
        fn id(f: &File) -> Identity {
            let m = f.metadata().unwrap();
            Identity {
                device: m.dev(),
                inode: m.ino(),
                kind: Kind::Directory,
            }
        }
        let (_, entries) = inventory.into_parts();
        let job = [180; 16];
        let manifest = Manifest::new(
            job,
            source.parent().unwrap().as_os_str().as_bytes().to_vec(),
            destination
                .parent()
                .unwrap()
                .as_os_str()
                .as_bytes()
                .to_vec(),
            id(&source_parent),
            id(&destination_parent),
            entries,
        )
        .unwrap();
        let journals = root.join("journals");
        let journal = Journal::create(&journals, manifest).unwrap();
        let proof = journal.intent_proof().unwrap();
        drop(journal);
        Self {
            root,
            context: Context {
                job,
                proof: Some(proof),
                journals,
                source,
                destination,
                recovery: None,
                reviewed_phase: Some(Phase::Prepared),
            },
        }
    }
    fn owner(&self) -> Box<ExdevFailure> {
        let recovery = Recovery::open_reviewed(
            &self.context.journals,
            self.context.job,
            self.context.proof.as_ref().unwrap(),
        )
        .unwrap();
        failure(
            self.context.clone(),
            io::Error::other("interrupted fixture"),
            Some(Retained::Recovery(Box::new(recovery))),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn rejected_recovery_keeps_lock_until_admitted_worker_releases_owner_before_reopen() {
    let f = Fixture::new();
    let previous = f.owner();
    let mut controller = Controller::new();
    controller.hold_queue(true);
    controller
        .enqueue(Request::new(
            1,
            vec![CopyRequest {
                source: f.context.source.clone(),
                destination: f.root.join("unused-copy"),
            }],
            Default::default(),
        ))
        .unwrap();
    let request = Request::reviewed_move(
        1,
        Prepared::recovery(f.context.clone(), RecoveryAction::Restore, Some(previous)),
    );
    let mut rejected = controller.enqueue(request).unwrap_err();
    assert_eq!(rejected.reason, RejectReason::IdNotIncreasing);
    assert!(Journal::open_single_file(&f.context.journals, f.context.job).is_err());
    rejected.request.id = 2;
    controller.enqueue(rejected.request).unwrap();
    assert!(controller.cancel_pending(1).is_some());
    assert!(Journal::open_single_file(&f.context.journals, f.context.job).is_err());
    controller.hold_queue(false);
    let deadline = Instant::now() + Duration::from_secs(3);
    let outcome = loop {
        let pump = controller.pump();
        if let Some(Completion::Finished(o)) = pump.completion {
            break o;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(
        outcome.exdev.as_ref().unwrap().recovery,
        Some(RecoveryAction::Restore)
    );
    assert_eq!(
        Journal::open_single_file(&f.context.journals, f.context.job)
            .unwrap()
            .state()
            .phase(),
        Phase::Restored
    );
    assert!(f.context.source.exists());
    assert!(!f.context.destination.exists());
    controller.begin_close();
    while !controller.is_closed() {
        controller.pump();
        std::thread::yield_now();
    }
}
#[test]
fn pending_cancel_releases_owner_but_journal_remains_discoverable_and_reviewable() {
    let f = Fixture::new();
    let mut controller = Controller::new();
    controller.hold_queue(true);
    controller
        .enqueue(Request::reviewed_move(
            1,
            Prepared::recovery(f.context.clone(), RecoveryAction::Restore, Some(f.owner())),
        ))
        .unwrap();
    assert!(controller.cancel_pending(1).is_some());
    let catalog =
        crate::move_catalog::read_at(&f.context.journals, &AtomicBool::new(false)).unwrap();
    assert_eq!(catalog.entries.len(), 1);
    let context = catalog.entries.into_iter().next().unwrap().context.unwrap();
    assert_eq!(context.source, f.context.source);
    assert_eq!(context.destination, f.context.destination);
    execute_recovery(
        Context {
            recovery: Some(RecoveryAction::Restore),
            ..context
        },
        &Control::default(),
    )
    .unwrap();
    controller.begin_close();
    assert!(controller.is_closed());
}
#[test]
fn recovery_open_failure_keeps_review_context_and_replaced_same_job_never_mutates() {
    let a = Fixture::new();
    let b = Fixture::new();
    let name = fs::read_dir(&a.context.journals)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name();
    fs::rename(
        a.context.journals.join(&name),
        a.context.journals.join("preserved-original"),
    )
    .unwrap();
    fs::copy(
        b.context.journals.join(&name),
        a.context.journals.join(&name),
    )
    .unwrap();
    let error = execute_recovery(
        Context {
            recovery: Some(RecoveryAction::Restore),
            ..a.context.clone()
        },
        &Control::default(),
    )
    .unwrap_err();
    assert!(error.recovery_needed);
    assert_eq!(error.context.source, a.context.source);
    assert_eq!(error.context.proof, a.context.proof);
    assert!(error.cause.to_string().contains("intención"));
    for f in [&a, &b] {
        assert!(f.context.source.exists());
        assert!(!f.context.destination.exists());
    }
}
struct Recorder;
impl Painter for Recorder {
    fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
    fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
    fn text(&mut self, _: Rect, _: &str, _: i32, _: files_core::theme::Color, _: bool, _: bool) {}
    fn image(&mut self, _: Rect, _: &Image<'_>) {}
    fn clip(&mut self, _: Rect) {}
    fn unclip(&mut self) {}
}
#[test]
fn recovery_controls_fit_narrow_notice_and_terminal_results_offer_no_recovery() {
    use crate::model::{Action, Model};
    let f = Fixture::new();
    let mut model = Model::new(f.root.clone());
    model.notice_open = true;
    model.recovery_context = Some(Context {
        reviewed_phase: Some(Phase::DestinationSynced),
        ..f.context.clone()
    });
    for (width, height) in [(440, 440), (500, 440), (800, 500), (1000, 650)] {
        crate::mutations::paint(&mut Recorder, &mut model, width, height);
        let controls: Vec<_> = model
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::RecoverMove(_)))
            .collect();
        assert_eq!(controls.len(), 2);
        for hit in &model.hits {
            assert!(hit.rect.x >= 18 && hit.rect.x + hit.rect.w <= width - 18);
            assert!(hit.rect.y >= 25 && hit.rect.y + hit.rect.h <= height - 25);
        }
        let a = controls[0].rect;
        let b = controls[1].rect;
        assert!(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y);
    }
    model.recovery_context = Some(Context {
        reviewed_phase: Some(Phase::RestoreIntent),
        ..f.context.clone()
    });
    assert_eq!(model.recovery_actions(), (false, true));
    crate::mutations::paint(&mut Recorder, &mut model, 440, 440);
    assert_eq!(
        model
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::RecoverMove(RecoveryAction::Restore)))
            .count(),
        1
    );
    assert!(
        !model
            .hits
            .iter()
            .any(|h| matches!(h.action, Action::RecoverMove(RecoveryAction::Finish)))
    );
    for phase in [Phase::Completed, Phase::Restored] {
        model.recovery_context = Some(Context {
            reviewed_phase: Some(phase),
            ..f.context.clone()
        });
        assert_eq!(model.recovery_actions(), (false, false));
    }
    model.recovery_context = None;
    model.copy_last = Some(Box::new(crate::copy_job::Outcome {
        exdev: Some(f.context.clone()),
        moving: true,
        extracting: false,
        creating: false,
        archive_action: None,
        id: 4,
        result: Ok(Default::default()),
        progress: Default::default(),
    }));
    assert!(model.recovery_context().is_none());
    assert_eq!(model.recovery_actions(), (false, false));
}
