//! Regression: inspection/failing chmod preserve history, committed chmod clears it.
use crate::rename_job::{Request, Response, Worker};
use files_core::rename_transaction::PreparedRename;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};
fn response(w: &mut Worker) -> Response {
    let start = Instant::now();
    loop {
        if let Some(r) = w.poll() {
            return r;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn property_reads_and_failed_changes_preserve_history_but_confirmed_chmod_clears_it() {
    let base = files_core::theme::home()
        .join(".cache/files-test")
        .join(format!("property-worker-{}", std::process::id()));
    fs::create_dir(&base).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(base.clone());
    fs::write(base.join("original"), b"original").unwrap();
    fs::set_permissions(base.join("original"), fs::Permissions::from_mode(0o600)).unwrap();
    let mut worker = Worker::new();
    let prepared =
        PreparedRename::capture(base.clone(), vec![("original".into(), "renamed".into())]).unwrap();
    assert!(worker.submit(Request::Apply { id: 1, prepared }).is_ok());
    assert!(matches!(response(&mut worker),Response::Finished(1,o,_)if o.success&&o.undo));
    assert!(
        worker
            .submit(Request::Inspect {
                id: 2,
                path: base.join("renamed")
            })
            .is_ok()
    );
    let Response::Properties(2, reply) = response(&mut worker) else {
        panic!("inspect reply")
    };
    assert!(reply.success && !reply.changed);
    let snapshot = reply.snapshot.unwrap();
    fs::set_permissions(base.join("renamed"), fs::Permissions::from_mode(0o640)).unwrap();
    assert!(
        worker
            .submit(Request::Permissions {
                id: 3,
                snapshot,
                mode: 0o644
            })
            .is_ok()
    );
    assert!(matches!(response(&mut worker),Response::Properties(3,r)if !r.success&&!r.changed));
    assert!(
        worker
            .submit(Request::History { id: 4, redo: false })
            .is_ok()
    );
    assert!(matches!(response(&mut worker),Response::Finished(4,o,_)if o.success&&o.redo));
    assert_eq!(
        fs::metadata(base.join("original"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
    assert!(
        worker
            .submit(Request::History { id: 5, redo: true })
            .is_ok()
    );
    assert!(matches!(response(&mut worker),Response::Finished(5,o,_)if o.success&&o.undo));
    assert!(
        worker
            .submit(Request::Inspect {
                id: 6,
                path: base.join("renamed")
            })
            .is_ok()
    );
    let Response::Properties(6, reply) = response(&mut worker) else {
        panic!("inspect reply")
    };
    assert!(
        worker
            .submit(Request::Permissions {
                id: 7,
                snapshot: reply.snapshot.unwrap(),
                mode: 0o600
            })
            .is_ok()
    );
    assert!(matches!(response(&mut worker),Response::Properties(7,r)if r.success&&r.changed));
    assert!(
        worker
            .submit(Request::History { id: 8, redo: false })
            .is_ok()
    );
    assert!(
        matches!(response(&mut worker),Response::Finished(8,o,_)if !o.success&&!o.undo&&!o.redo)
    );
    assert_eq!(fs::read(base.join("renamed")).unwrap(), b"original");
    assert_eq!(
        fs::metadata(base.join("renamed"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
