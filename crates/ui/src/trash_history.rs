//! A bounded undo batch, owned exclusively by the existing mutation worker.
//! No UI filesystem calls, no permanent deletion or cross-device copy fallback.
use crate::rename_job::Outcome;
use files_core::trash::{self, Failure, Receipt, TrashContext};
use std::{
    ffi::{OsStr, OsString},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX_ITEMS: usize = 64;
const MAX_NAMES_BYTES: usize = 256 * 1024;
const MAX_DETAILS_BYTES: usize = 4 * 1024 * 1024;

pub fn preflight<'a>(
    directory: &Path,
    names: impl Iterator<Item = &'a OsStr>,
) -> Result<(), String> {
    let parent_bytes = directory.as_os_str().as_encoded_bytes().len();
    if !directory.is_absolute() || parent_bytes > 4096 {
        return Err("Papelera: la carpeta debe ser absoluta y de hasta 4096 bytes".into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0usize;
    for (count, name) in names.enumerate() {
        if count >= MAX_ITEMS {
            return Err(
                "Reduce el lote de papelera: máximo 64 elementos; no se trunca la selección".into(),
            );
        }
        if !seen.insert(name) {
            return Err("Papelera: selección duplicada; no se inició ninguna operación".into());
        }
        let size = name.as_encoded_bytes().len();
        let mut components = Path::new(name).components();
        if size == 0
            || size > 4096
            || name.as_encoded_bytes().contains(&0)
            || !matches!(components.next(), Some(Component::Normal(part)) if part == name)
            || components.next().is_some()
        {
            return Err("Papelera: cada nombre debe ser un componente único sin NUL".into());
        }
        bytes = bytes
            .checked_add(size)
            .ok_or("Presupuesto de nombres de papelera excedido")?;
        if bytes > MAX_NAMES_BYTES
            || parent_bytes
                .checked_add(size)
                .and_then(|n| n.checked_add(1))
                .is_none_or(|n| n > 4096)
        {
            return Err("Papelera: máximo 256 KiB de nombres y 4096 bytes por ruta".into());
        }
    }
    if seen.is_empty() {
        return Err("Selecciona archivos o carpetas para enviar a la papelera".into());
    }
    Ok(())
}
pub fn root() -> Result<PathBuf, String> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| files_core::theme::home().join(".local/share"));
    if !data.is_absolute() || data.as_os_str().as_encoded_bytes().len() > 4080 {
        return Err("XDG_DATA_HOME debe ser absoluto y estar dentro del límite de ruta".into());
    }
    Ok(data.join("Trash"))
}
#[derive(Default)]
pub struct History {
    receipts: Vec<Receipt>,
    // Recovery-only descriptors are retained but never advertised as valid Undo.
    // Together both vectors contain at most MAX_ITEMS receipts.
    recovery: Vec<Receipt>,
}
impl History {
    pub fn available(&self) -> bool {
        !self.receipts.is_empty()
    }
    pub fn clear(&mut self) {
        self.receipts.clear();
        self.recovery.clear();
    }
    /// Old undo state survives failures before the first commit. On that first
    /// commit it is released before retaining the new batch, avoiding two full
    /// receipt batches/FD sets in memory. No per-file progress queue is retained.
    pub fn send(
        &mut self,
        directory: &Path,
        names: &[OsString],
        root: &Path,
        cancel: &AtomicBool,
        mut first_commit: impl FnMut(),
    ) -> Outcome {
        if let Err(message) = preflight(directory, names.iter().map(|n| n.as_os_str())) {
            return self.failure(message, Vec::new());
        }
        if cancel.load(Ordering::Acquire) {
            return self.failure("Papelera cancelada antes de empezar".into(), Vec::new());
        }
        let context = match TrashContext::open(root) {
            Ok(c) => c,
            Err(e) => {
                return self.failure(
                    format!("No se pudo abrir la papelera: {}", cause_message(&e.cause)),
                    failure_details(&e),
                );
            }
        };
        let mut committed = 0usize;
        for name in names {
            if cancel.load(Ordering::Acquire) {
                return self.failure(
                    format!(
                        "Papelera cancelada: {committed} de {} enviados; puedes deshacerlos",
                        names.len()
                    ),
                    Vec::new(),
                );
            }
            match context.trash(&directory.join(name), cancel) {
                Ok(receipt) => {
                    if committed == 0 {
                        self.clear();
                        first_commit();
                    }
                    self.receipts.push(receipt);
                    committed += 1;
                }
                Err(e) => {
                    return self.failure(
                        format!(
                            "Papelera: {committed} de {} enviados; {}",
                            names.len(),
                            cause_message(&e.cause)
                        ),
                        failure_details(&e),
                    );
                }
            }
        }
        Outcome {
            success: true,
            message: format!("Enviados a Papelera: {committed}"),
            details: Vec::new(),
            undo: self.available(),
            redo: false,
        }
    }
    pub fn restore(&mut self, cancel: &AtomicBool) -> Outcome {
        let mut restored = 0usize;
        let mut details = Vec::new();
        while let Some(receipt) = self.receipts.last() {
            match trash::restore(receipt, cancel) {
                Ok(report) => {
                    match &report.restored_path {
                        Some(path) if path != receipt.original_path()=>push_detail(&mut details,format!("Restaurado en la ubicación actual del padre: {path:?}")),
                        None=>push_detail(&mut details,"Objeto restaurado y confirmado por descriptor; su ruta actual no pudo resolverse".into()),
                        _=>{},
                    }
                    if report.unresolved_paths != 0 {
                        push_detail(
                            &mut details,
                            format!(
                                "{} ubicaciones restauradas o residuales no pudieron resolverse; no se ofrece reintentar un objeto ya restaurado",
                                report.unresolved_paths
                            ),
                        );
                    }
                    self.receipts.pop();
                    restored += 1;
                    for path in report.cleanup_paths {
                        push_detail(&mut details, format!("Metadatos pendientes: {path:?}"));
                    }
                    for error in report.cleanup_errors {
                        push_detail(&mut details, format!("Limpieza de metadatos: {error}"));
                    }
                }
                Err(e) => {
                    if !e.restore_retryable {
                        // Its trash/info slot no longer identifies the receipt;
                        // retaining it as ordinary Undo would be a stale retry.
                        self.recovery
                            .push(self.receipts.pop().expect("receipt exists"));
                        push_detail(&mut details, "El recibo requiere recuperación manual; se conservan sus descriptores hasta otra mutación confirmada o el cierre".into());
                    }
                    for detail in failure_details(&e) {
                        push_detail(&mut details, detail);
                    }
                    return self.failure(
                        format!(
                            "Restaurados {restored}; pendientes {} ({} requieren recuperación): {}",
                            self.receipts.len(),
                            self.recovery.len(),
                            cause_message(&e.cause)
                        ),
                        details,
                    );
                }
            }
        }
        Outcome {
            success: true,
            message: format!("Restaurados de Papelera: {restored}"),
            details,
            undo: false,
            redo: false,
        }
    }
    fn failure(&self, message: String, details: Vec<String>) -> Outcome {
        Outcome {
            success: false,
            message,
            details,
            undo: self.available(),
            redo: false,
        }
    }
}
fn cause_message(error: &std::io::Error) -> String {
    if error.raw_os_error() == Some(libc::EXDEV) {
        return "Otra unidad: papelera por volumen pendiente; no se copia ni borra el origen"
            .into();
    }
    match error.kind() {
        std::io::ErrorKind::AlreadyExists => {
            "El nombre original está ocupado; no se reemplazó".into()
        }
        std::io::ErrorKind::Interrupted => "Operación cancelada".into(),
        _ => error.to_string(),
    }
}
fn push_detail(details: &mut Vec<String>, message: String) {
    let retained = details.iter().map(String::len).sum::<usize>();
    if retained.saturating_add(message.len()) <= MAX_DETAILS_BYTES - 80 && details.len() < 255 {
        details.push(message);
    } else if !details
        .last()
        .is_some_and(|s| s == "Informe truncado; revisa los metadatos de la Papelera")
    {
        details.push("Informe truncado; revisa los metadatos de la Papelera".into());
    }
}
fn failure_details(failure: &Failure) -> Vec<String> {
    let mut details = Vec::new();
    for path in &failure.preserved_paths {
        push_detail(&mut details, format!("Conservado: {path:?}"));
    }
    for path in &failure.cleanup_paths {
        push_detail(&mut details, format!("Limpieza pendiente: {path:?}"));
    }
    for error in &failure.rollback_errors {
        push_detail(&mut details, format!("Recuperación: {error}"));
    }
    if failure.unresolved_paths != 0 {
        push_detail(
            &mut details,
            format!(
                "{} ubicaciones fijadas no pudieron resolverse; no se inventa una ruta de rescate",
                failure.unresolved_paths
            ),
        );
    }
    details
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn replaced_trash_slot_is_recovery_only_not_stale_undo() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "trash-history-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("original");
        std::fs::write(&source, b"preserved").unwrap();
        let root = dir.join("Trash");
        let flag = AtomicBool::new(false);
        let context = TrashContext::open(&root).unwrap();
        let receipt = context.trash(&source, &flag).unwrap();
        let slot = receipt.trashed_path().unwrap();
        let rescued = dir.join("relocated-by-actor");
        std::fs::rename(&slot, &rescued).unwrap();
        std::fs::write(&slot, b"replacement").unwrap();
        let mut history = History {
            receipts: vec![receipt],
            recovery: Vec::new(),
        };
        let result = history.restore(&flag);
        assert!(!result.success && !result.undo && !history.available());
        assert_eq!(history.recovery.len(), 1);
        assert_eq!(std::fs::read(&rescued).unwrap(), b"preserved");
        assert_eq!(std::fs::read(&slot).unwrap(), b"replacement");
        assert!(
            result
                .details
                .iter()
                .any(|d| d.contains("relocated-by-actor")),
            "{:?}",
            result.details
        );
        assert!(!source.exists());
        drop(history);
        drop(context);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn preflight_checks_borrowed_selection_and_recovers() {
        let names = (0..=MAX_ITEMS)
            .map(|i| OsString::from(format!("a{i}")))
            .collect::<Vec<_>>();
        assert!(preflight(Path::new("/home/test"), names.iter().map(|n| n.as_os_str())).is_err());
        assert!(
            preflight(
                Path::new("/home/test"),
                names[..MAX_ITEMS].iter().map(|n| n.as_os_str())
            )
            .is_ok()
        );
        assert!(
            preflight(
                Path::new("/home/test"),
                [OsStr::new("a"), OsStr::new("a")].into_iter()
            )
            .is_err()
        );
        for name in ["..", "a/b", "a/", ".", "/root", "a\0b"] {
            assert!(preflight(Path::new("/home/test"), [OsStr::new(name)].into_iter()).is_err());
        }
        assert!(preflight(Path::new("relative"), [OsStr::new("a")].into_iter()).is_err());
        assert!(preflight(Path::new("/home/test"), std::iter::empty()).is_err());
    }
}
