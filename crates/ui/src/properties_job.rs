//! Properties operations for the EXISTING mutation worker. No thread, helper,
//! channel, unbounded history or UI filesystem access is created here.
//! A snapshot pins two FDs; the caller owns at most one active inspector snapshot.
use files_core::file_properties::{self, ChangeReport, Snapshot};
use std::{
    fmt::{self, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
const MAX_MESSAGE_BYTES: usize = 1024;
const MAX_WARNINGS: usize = 8;
pub struct Reply {
    pub snapshot: Option<Box<Snapshot>>,
    pub current_path: Option<PathBuf>,
    pub success: bool,
    /// True means the chmod syscall succeeded, even if later diagnostics failed.
    /// The existing worker clears other undo history ONLY after this is true.
    pub changed: bool,
    pub message: String,
    pub warnings: Vec<String>,
}
struct BoundedText {
    text: String,
}
impl fmt::Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = MAX_MESSAGE_BYTES.saturating_sub(self.text.len());
        if value.len() <= remaining {
            self.text.push_str(value);
            return Ok(());
        }
        let mut end = remaining;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&value[..end]);
        Err(fmt::Error)
    }
}
fn diagnostic(arguments: fmt::Arguments<'_>) -> String {
    let mut output = BoundedText {
        text: String::with_capacity(MAX_MESSAGE_BYTES),
    };
    let _ = output.write_fmt(arguments);
    output.text
}
fn warning(warnings: &mut Vec<String>, arguments: fmt::Arguments<'_>) {
    if warnings.len() < MAX_WARNINGS {
        warnings.push(diagnostic(arguments));
    }
}
fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Acquire)
}
fn inspect_failure(message: String) -> Reply {
    Reply {
        snapshot: None,
        current_path: None,
        success: false,
        changed: false,
        message,
        warnings: Vec::new(),
    }
}
pub fn inspect(path: PathBuf, cancel: &AtomicBool) -> Reply {
    if cancelled(cancel) {
        return inspect_failure("Lectura de propiedades cancelada".into());
    }
    let snapshot = match file_properties::capture(&path) {
        Ok(snapshot) => Box::new(snapshot),
        Err(error) => {
            return inspect_failure(diagnostic(format_args!(
                "No se pudieron leer las propiedades: {error}"
            )));
        }
    };
    if cancelled(cancel) {
        return inspect_failure("Lectura de propiedades cancelada".into());
    }
    let current_path = match snapshot.current_path() {
        Ok(path) => path,
        Err(error) => {
            return inspect_failure(diagnostic(format_args!(
                "No se pudo verificar la ubicación del objeto: {error}"
            )));
        }
    };
    if cancelled(cancel) {
        return inspect_failure("Lectura de propiedades cancelada".into());
    }
    Reply {
        snapshot: Some(snapshot),
        current_path: Some(current_path),
        success: true,
        changed: false,
        message: "Propiedades leídas".into(),
        warnings: Vec::new(),
    }
}
fn current_path(snapshot: &Snapshot, warnings: &mut Vec<String>) -> Option<PathBuf> {
    match snapshot.current_path() {
        Ok(path) => Some(path),
        Err(error) => {
            warning(
                warnings,
                format_args!("No se pudo verificar la ubicación actual: {error}"),
            );
            None
        }
    }
}
pub fn apply(snapshot: Box<Snapshot>, mode: u32, cancel: &AtomicBool) -> Reply {
    // Core checks cancel, identity, observed mode/uid/gid, owner and kind before
    // the syscall. Do not reclassify a confirmed commit as cancelled afterwards.
    match file_properties::chmod(&snapshot, mode, cancel) {
        Ok(report) => applied(snapshot, report),
        Err(error) => {
            let mut warnings = Vec::new();
            warning(
                &mut warnings,
                format_args!(
                    "La observación anterior no se actualizó. Vuelve a cargar las propiedades antes de confirmar otro cambio."
                ),
            );
            let current_path = current_path(&snapshot, &mut warnings);
            Reply {
                snapshot: Some(snapshot),
                current_path,
                success: false,
                changed: false,
                message: diagnostic(format_args!("No se cambiaron los permisos: {error}")),
                warnings,
            }
        }
    }
}
fn applied(mut snapshot: Box<Snapshot>, report: ChangeReport) -> Reply {
    let mut warnings = Vec::new();
    // Reserve the first warning for a required reload when the observer failed;
    // a full upstream diagnostic list cannot hide this actionable condition.
    if report.after.is_none() {
        warning(
            &mut warnings,
            format_args!(
                "Permisos aplicados, pero falta una observación posterior válida. Vuelve a cargar las propiedades antes de otro cambio."
            ),
        );
    }
    for message in &report.warnings {
        warning(&mut warnings, format_args!("{message}"));
    }
    if let Some(after) = report.after {
        snapshot.properties = after;
    }
    let current_path = current_path(&snapshot, &mut warnings);
    let keep_snapshot = report.after.is_some();
    Reply {
        snapshot: keep_snapshot.then_some(snapshot),
        current_path,
        success: true,
        changed: true,
        message: if keep_snapshot {
            "Permisos aplicados. Esta operación no tiene Deshacer."
        } else {
            "Permisos aplicados; vuelve a cargar las propiedades. Esta operación no tiene Deshacer."
        }
        .into(),
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    #[test]
    fn diagnostics_are_utf8_bounded_without_cloning_full_error_and_warning_count_is_fixed() {
        let error = io::Error::other("漢".repeat(5000));
        let text = diagnostic(format_args!("Error: {error}"));
        assert!(text.len() <= MAX_MESSAGE_BYTES);
        assert!(text.starts_with("Error: "));
        let mut warnings = Vec::new();
        for _ in 0..100 {
            warning(&mut warnings, format_args!("{error}"));
        }
        assert_eq!(warnings.len(), MAX_WARNINGS);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.len() <= MAX_MESSAGE_BYTES)
        );
    }
    #[test]
    fn cancelled_inspection_does_not_return_a_snapshot() {
        let reply = inspect(PathBuf::from("/unused/fixture"), &AtomicBool::new(true));
        assert!(!reply.success && !reply.changed);
        assert!(reply.snapshot.is_none() && reply.current_path.is_none());
    }
    #[cfg(target_os = "linux")]
    mod linux {
        use super::*;
        use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicU64};
        struct Fixture(PathBuf);
        impl Fixture {
            fn new() -> Self {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let base = files_core::theme::home().join(".cache/files-test");
                fs::create_dir_all(&base).unwrap();
                let path = base.join(format!(
                    "properties-adapter-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn file(&self) -> PathBuf {
                let path = self.0.join("propiedades-日本語");
                fs::write(&path, b"unchanged").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                path
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fn flag() -> AtomicBool {
            AtomicBool::new(false)
        }
        #[test]
        fn successful_apply_updates_observation_for_subsequent_own_edit() {
            let f = Fixture::new();
            let path = f.file();
            let reply = inspect(path.clone(), &flag());
            assert!(reply.success && !reply.changed);
            assert_eq!(reply.current_path.as_ref(), Some(&path));
            let reply = apply(reply.snapshot.unwrap(), 0o640, &flag());
            assert!(reply.success && reply.changed);
            assert_eq!(
                reply.snapshot.as_ref().unwrap().properties.mode & 0o777,
                0o640
            );
            let reply = apply(reply.snapshot.unwrap(), 0o600, &flag());
            assert!(reply.success && reply.changed);
            assert_eq!(fs::read(path).unwrap(), b"unchanged");
        }
        #[test]
        fn failed_stale_apply_retains_old_snapshot_requires_reload_and_keeps_external_mode() {
            let f = Fixture::new();
            let path = f.file();
            let reply = inspect(path.clone(), &flag());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
            let reply = apply(reply.snapshot.unwrap(), 0o644, &flag());
            assert!(!reply.success && !reply.changed);
            assert_eq!(reply.snapshot.unwrap().properties.mode & 0o777, 0o600);
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o640
            );
            assert!(
                reply
                    .warnings
                    .iter()
                    .any(|text| text.contains("Vuelve a cargar"))
            );
        }
        #[test]
        fn confirmed_commit_with_no_observation_keeps_changed_true_and_drops_snapshot() {
            let f = Fixture::new();
            let path = f.file();
            let reply = inspect(path, &flag());
            // Adapter contract fixture: simulate core Ok(after=None), without
            // pretending this unit test performed a real chmod syscall.
            let reply = applied(
                reply.snapshot.unwrap(),
                ChangeReport {
                    requested_mode: 0o640,
                    after: None,
                    warnings: vec!["Fallo diagnóstico inyectado".into(); 20],
                },
            );
            assert!(reply.success && reply.changed);
            assert!(reply.snapshot.is_none());
            assert_eq!(reply.warnings.len(), MAX_WARNINGS);
            assert!(reply.warnings[0].contains("Vuelve a cargar"));
        }
        #[test]
        fn replaced_inode_returns_old_snapshot_without_altering_replacement() {
            let f = Fixture::new();
            let path = f.file();
            let reply = inspect(path.clone(), &flag());
            fs::rename(&path, f.0.join("moved")).unwrap();
            fs::write(&path, b"foreign").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let reply = apply(reply.snapshot.unwrap(), 0o777, &flag());
            assert!(!reply.success && !reply.changed && reply.snapshot.is_some());
            assert!(reply.current_path.is_none());
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(fs::read(path).unwrap(), b"foreign");
        }
    }
}
