//! Bounded, copyable diagnostics. Rescue locations precede routine copied paths.
use crate::copy_job::{Failure, Outcome};
use std::fmt::{self, Write};
const LIMIT: usize = 64 * 1024;
const TAIL: &str = "\n[Informe visual truncado: conserva el resultado de la operación para revisar los detalles restantes.]\n";

pub struct Details {
    pub text: String,
    pub truncated: bool,
}
struct Bounded {
    text: String,
    truncated: bool,
}
impl Write for Bounded {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = (LIMIT - TAIL.len()).saturating_sub(self.text.len());
        if self.truncated {
            return Err(fmt::Error);
        }
        if value.len() <= remaining {
            self.text.push_str(value);
            return Ok(());
        }
        let mut n = remaining;
        while !value.is_char_boundary(n) {
            n -= 1;
        }
        self.text.push_str(&value[..n]);
        self.truncated = true;
        Err(fmt::Error)
    }
}
/// Debug-escaped paths distinguish control bytes and non-UTF8 file names.
/// This is presentation only: never parse displayed paths to perform operations.
pub fn describe(outcome: &Outcome) -> Details {
    let mut out = Bounded {
        text: String::new(),
        truncated: false,
    };
    let operation = if let Some(action) = outcome.archive_action {
        match action {
            crate::archive_operation::RecoveryAction::Cleanup => "limpieza de temporal ZIP",
            crate::archive_operation::RecoveryAction::Sync => "confirmación de guardado ZIP",
        }
    } else if outcome.creating {
        "creación ZIP"
    } else if outcome.extracting {
        "extracción ZIP"
    } else if outcome.exdev.is_some() {
        "movimiento EXDEV / recuperación"
    } else if outcome.moving {
        "movimiento nativo"
    } else {
        "copia"
    };
    let _ = writeln!(out, "Operación de {operation} #{}", outcome.id);
    let status = match &outcome.result {
        Ok(_)
            if outcome.exdev.as_ref().is_some_and(|c| {
                c.recovery == Some(crate::move_operation::RecoveryAction::Restore)
            }) =>
        {
            "Origen restaurado; cualquier destino publicado se conserva"
        }
        Ok(_)
            if outcome.archive_action
                == Some(crate::archive_operation::RecoveryAction::Cleanup) =>
        {
            "Temporal limpiado y guardado confirmado; no se publicó contenido nuevo"
        }
        Ok(_) if outcome.archive_action == Some(crate::archive_operation::RecoveryAction::Sync) => {
            "Guardado confirmado; contenido publicado conservado sin volver a extraer"
        }
        Ok(_) if outcome.creating => "ZIP creado y sincronizado sin sobrescribir",
        Ok(_) if outcome.extracting => "ZIP extraído y sincronizado sin sobrescribir",
        Ok(_) if outcome.exdev.is_some() => "Movimiento EXDEV terminado con journal y verificación",
        Ok(_) if outcome.moving => "Movimiento nativo terminado (mismo filesystem)",
        Ok(r) if r.renamed_count > 0 => "Copia finalizada conservando ambos con nombres numerados",
        Ok(r) if r.skipped_count > 0 => "Copia finalizada con destinos omitidos",
        Ok(_) => "Copia terminada",
        Err(Failure::Exdev(error)) if error.recovery_needed => {
            "EXDEV interrumpido; conserva journal y rutas para recuperación explícita"
        }
        Err(Failure::Exdev(_)) => "EXDEV detenido; revisa estado del origen y destino",
        Err(Failure::Archive(e)) if outcome.archive_action.is_some() && e.cancelled() => {
            "Recuperación cancelada; estado conservado para reintentar"
        }
        Err(Failure::Archive(_))
            if outcome.archive_action
                == Some(crate::archive_operation::RecoveryAction::Cleanup) =>
        {
            "Limpieza incompleta; conserva el temporal y revisa el último error"
        }
        Err(Failure::Archive(_))
            if outcome.archive_action == Some(crate::archive_operation::RecoveryAction::Sync) =>
        {
            "Guardado sin confirmar; no se volvió a extraer ni a copiar"
        }
        Err(Failure::Archive(e)) if outcome.creating && e.cancelled() => {
            "Creación ZIP cancelada; revisa el estado de los temporales"
        }
        Err(Failure::Archive(_)) if outcome.creating => {
            "Creación ZIP incompleta; revisa publicación y temporales"
        }
        Err(Failure::Archive(_)) => {
            "Extracción incompleta; revisa publicación y limpieza antes de reintentar"
        }
        Err(Failure::Batch(_)) => {
            "Movimiento multipadre incompleto; revisa cada raíz antes de reintentar"
        }
        Err(Failure::CancelledBeforeStart) => "Cancelada antes de comenzar",
        Err(Failure::Engine(_)) if outcome.moving && outcome.cancelled() => {
            "Movimiento cancelado; raíces anteriores siguen movidas, restantes en origen"
        }
        Err(Failure::Engine(_)) if outcome.moving => {
            "Movimiento incompleto; revisa destinos y orígenes, sin rollback global"
        }
        Err(Failure::Engine(_)) if outcome.cancelled() => {
            "Copia cancelada; revisa los destinos ya publicados"
        }
        Err(Failure::Engine(_)) => "La copia no terminó; revisa los detalles",
        Err(Failure::WorkerPanicked | Failure::WorkerLost) => {
            "Estado sin confirmar: revisa destinos y carpetas temporales antes de repetir"
        }
    };
    let _ = writeln!(out, "{status}");
    if let Err(Failure::Archive(error)) = &outcome.result {
        let _ = writeln!(out, "{error}");
    }
    if let Err(Failure::Batch(batch)) = &outcome.result {
        // Indices are those of the captured offer, not per-group report counts.
        let states = batch.ledger.states();
        let _ = writeln!(
            out,
            "Confirmadas: {} · pendientes: {} · sin confirmar: {}",
            states
                .iter()
                .filter(|s| **s == files_core::multi_origin::RootState::Confirmed)
                .count(),
            states
                .iter()
                .filter(|s| **s == files_core::multi_origin::RootState::Pending)
                .count(),
            states
                .iter()
                .filter(|s| **s == files_core::multi_origin::RootState::Uncertain)
                .count()
        );
        for (index, state) in batch.ledger.states().iter().enumerate() {
            if let Some(path) = batch.paths.get(index) {
                let _ = writeln!(
                    out,
                    "Raíz {} · {} · solicitada {:?} → {:?}",
                    index + 1,
                    match state {
                        files_core::multi_origin::RootState::Pending => "PENDIENTE",
                        files_core::multi_origin::RootState::Confirmed => "CONFIRMADA",
                        files_core::multi_origin::RootState::Uncertain => "SIN CONFIRMAR",
                    },
                    path.source,
                    path.destination
                );
            }
        }
        let _ = writeln!(out, "Causa: {}", outcome.result.as_ref().unwrap_err());
    }
    if let Some(context) = &outcome.exdev {
        let _ = writeln!(
            out,
            "Journal {} en {:?}\nOrigen capturado: {:?}\nDestino capturado: {:?}",
            context.job_label(),
            context.journals,
            context.source,
            context.destination
        );
        if let Some(error) = outcome.exdev_error() {
            let _ = writeln!(
                out,
                "Fase confirmada: {:?} · owner retenido: {:?}\nCausa: {}",
                error.phase,
                error.retained_phase(),
                error.cause
            );
            if let Some(path) = &error.source_location {
                let _ = writeln!(out, "Origen verificado por FD (conservar): {path:?}");
            }
            if let Some(path) = &error.destination_location {
                let _ = writeln!(out, "Destino verificado por FD (conservar): {path:?}");
            }
            for path in &error.staging_candidates {
                let _ = writeln!(out, "Staging candidato (no borrar por nombre): {path:?}");
            }
            if error.recovery_needed {
                let _ = writeln!(
                    out,
                    "Recuperación pendiente/estado desconocido. Reintentar exige reabrir y verificar; no elimines temporales por nombre."
                );
            }
        }
    }
    if let Some(report) = outcome.report() {
        if report.publication == files_core::copy_engine::Publication::VisibleExclusive {
            let _ = writeln!(
                out,
                "Publicación exclusiva visible: iniciadas {}, terminadas {}, creación incierta {}. Los destinos parciales se conservan; no hay rollback automático. Timestamps de enlaces omitidos: {}.",
                report.visible_started_count,
                report.visible_completed_count,
                report.visible_uncertain_count,
                report.symlink_times_omitted
            );
        }
        if outcome.moving && outcome.exdev.is_none() {
            let _ = writeln!(
                out,
                "Conteos siguientes son de esta ejecución: raíces, no descendientes. Rename conserva inode y metadatos;0 bytes copiados. EXDEV tiene una revisión y un journal independientes."
            );
        }
        let _ = writeln!(
            out,
            "Raíces publicadas: {} (rutas omitidas: {})\nCon nombre numerado: {}\nDestinos omitidos: {} (rutas omitidas del informe: {})\n{}: {}\nArchivos: {} · Carpetas: {} · Enlaces: {}",
            report.completed_count,
            report.completed_omitted,
            report.renamed_count,
            report.skipped_count,
            report.skipped_omitted,
            if outcome.creating {
                "Bytes de archivos leídos (antes de comprimir)"
            } else {
                "Bytes lógicos copiados"
            },
            report.bytes_copied,
            report.files_copied,
            report.directories_copied,
            report.symlinks_copied
        );
        let _ = writeln!(
            out,
            "Limpieza: {} rutas omitidas · {} errores omitidos · {} detalles truncados",
            report.cleanup_paths_omitted, report.cleanup_errors_omitted, report.details_truncated
        );
    }
    if let Some(error) = outcome.engine_error() {
        // Counters precede potentially long cause/paths so truncation cannot hide them.
        let _ = writeln!(
            out,
            "Rescate: {} ubicaciones registradas · {} omitidas\nLimpieza: {} errores registrados · {} omitidos · {} entradas sin resolver · {} detalles truncados",
            error.rescue_paths.len(),
            error.rescue_paths_omitted,
            error.cleanup_errors.len(),
            error.cleanup_errors_omitted,
            error.cleanup_unresolved_entries,
            error.details_truncated
        );
        for path in &error.rescue_paths {
            if writeln!(out, "Rescate (conservar): {path:?}").is_err() {
                break;
            }
        }
        let cause = if outcome.cancelled() {
            if outcome.moving {
                "Movimiento cancelado".to_owned()
            } else {
                "Copia cancelada".to_owned()
            }
        } else if error.cause.kind() == std::io::ErrorKind::AlreadyExists {
            "El destino ya existe; no se sobrescribió".to_owned()
        } else {
            error.cause.to_string()
        };
        let _ = writeln!(out, "Causa: {cause}");
        for error in &error.cleanup_errors {
            if writeln!(out, "Error de limpieza: {error}").is_err() {
                break;
            }
        }
    }
    if let Some(report) = outcome.report() {
        for path in &report.cleanup_paths {
            if writeln!(out, "Temporal pendiente de revisión: {path:?}").is_err() {
                break;
            }
        }
        for error in &report.cleanup_errors {
            if writeln!(out, "Error de limpieza: {error}").is_err() {
                break;
            }
        }
        for path in &report.skipped {
            if writeln!(out, "Destino solicitado omitido (ocupado): {path:?}").is_err() {
                break;
            }
        }
        for path in &report.completed {
            if writeln!(out, "Publicado: {path:?}").is_err() {
                break;
            }
        }
    }
    if out.truncated {
        out.text.push_str(TAIL);
    }
    Details {
        text: out.text,
        truncated: out.truncated,
    }
}

/// Explicit user-requested full diagnostic; bounded independently of the64KiB view.
/// Current engine's capped report sets fit8MiB even when Debug escapes each raw byte.
pub fn full_text(outcome: &Outcome) -> Details {
    struct Full {
        text: String,
        truncated: bool,
    }
    impl Write for Full {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            let remaining = (8 * 1024 * 1024usize).saturating_sub(self.text.len());
            if value.len() > remaining {
                self.truncated = true;
                return Err(fmt::Error);
            }
            let next = self.text.len() + value.len();
            if next > self.text.capacity() {
                let capacity = next
                    .max(self.text.capacity().max(256).saturating_mul(2))
                    .min(8 * 1024 * 1024);
                self.text
                    .try_reserve_exact(capacity - self.text.len())
                    .map_err(|_| {
                        self.truncated = true;
                        fmt::Error
                    })?;
            }
            self.text.push_str(value);
            Ok(())
        }
    }
    let mut out = Full {
        text: String::new(),
        truncated: false,
    };
    let _ = write!(out, "{outcome:#?}");
    Details {
        text: out.text,
        truncated: out.truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copy_job::ProgressStats;
    use files_core::copy_engine::{CopyError, CopyReport};
    use std::io;
    fn outcome(result: Result<CopyReport, Failure>) -> Outcome {
        Outcome {
            moving: false,
            extracting: false,
            creating: false,
            archive_action: None,
            exdev: None,
            id: 19,
            result,
            progress: ProgressStats::default(),
        }
    }
    #[test]
    fn numbered_count_survives_omitted_path_details() {
        let report = CopyReport {
            completed_count: 1000,
            renamed_count: 1000,
            completed_omitted: 999,
            completed: vec!["/target/a (3).txt".into()],
            ..Default::default()
        };
        let result = describe(&outcome(Ok(report)));
        assert!(result.text.contains("Con nombre numerado: 1000"));
        assert!(result.text.contains("1000 (rutas omitidas: 999)"));
        assert!(result.text.contains("/target/a (3).txt"));
        assert!(result.text.contains("conservando ambos"));
    }
    #[test]
    fn partial_copy_preserves_counters_and_prioritizes_rescue() {
        let report = CopyReport {
            completed_count: 9,
            completed_omitted: 8,
            completed: vec!["/copied".into()],
            cleanup_paths_omitted: 2,
            ..Default::default()
        };
        let error = CopyError {
            cause: io::Error::other("disk full"),
            report: Box::new(report),
            rescue_paths: vec!["/rescue".into()],
            cleanup_errors: vec!["changed inode".into()],
            rescue_paths_omitted: 3,
            cleanup_errors_omitted: 4,
            cleanup_unresolved_entries: 5,
            details_truncated: 6,
        };
        let result = describe(&outcome(Err(Failure::Engine(Box::new(error)))));
        assert!(!result.truncated);
        assert!(result.text.contains("9 (rutas omitidas: 8)"));
        assert!(result.text.contains("3 omitidas"));
        assert!(
            result
                .text
                .contains("4 omitidos · 5 entradas sin resolver · 6 detalles truncados")
        );
        assert!(
            result.text.find("Rescate (conservar)").unwrap()
                < result.text.find("Publicado:").unwrap()
        );
        assert!(result.text.contains("changed inode"));
    }
    #[test]
    fn logical_bytes_and_cancel_cause_are_explicit_for_cloned_copies() {
        let error = CopyError {
            cause: io::Error::new(io::ErrorKind::Interrupted, "Copy cancelled"),
            report: Box::new(CopyReport {
                bytes_copied: 42,
                ..Default::default()
            }),
            rescue_paths: vec![],
            cleanup_errors: vec![],
            rescue_paths_omitted: 0,
            cleanup_errors_omitted: 0,
            cleanup_unresolved_entries: 0,
            details_truncated: 0,
        };
        let result = describe(&outcome(Err(Failure::Engine(Box::new(error)))));
        assert!(result.text.contains("Bytes lógicos copiados: 42"));
        assert!(result.text.contains("Causa: Copia cancelada"));
        assert!(!result.text.contains("Copy cancelled"));
    }
    #[test]
    fn text_cap_preserves_utf8_and_explicit_truncation() {
        let report = CopyReport {
            cleanup_errors: vec!["漢".repeat(LIMIT)],
            ..Default::default()
        };
        let result = describe(&outcome(Ok(report)));
        assert!(result.truncated);
        assert!(result.text.len() <= LIMIT);
        assert!(result.text.ends_with(TAIL));
    }
    #[test]
    fn explicit_full_report_is_bounded_even_when_debug_escaping_expands() {
        let report = CopyReport {
            cleanup_errors: vec!["\0".repeat(4 * 1024 * 1024)],
            ..Default::default()
        };
        let result = full_text(&outcome(Ok(report)));
        assert!(result.truncated);
        assert!(result.text.len() <= 8 * 1024 * 1024);
        assert!(result.text.capacity() <= 8 * 1024 * 1024);
        let complete = full_text(&outcome(Ok(CopyReport::default())));
        assert!(!complete.truncated);
        assert!(complete.text.contains("completed_count"));
    }
    #[test]
    fn uncertain_worker_state_is_not_reported_as_cancelled_or_successful() {
        for failure in [Failure::WorkerLost, Failure::WorkerPanicked] {
            let result = describe(&outcome(Err(failure)));
            assert!(result.text.contains("Estado sin confirmar"));
            assert!(!result.text.contains("Copia terminada"));
        }
    }
    #[cfg(unix)]
    #[test]
    fn non_utf8_and_newline_paths_are_distinguishable() {
        use std::os::unix::ffi::OsStringExt;
        let path = std::ffi::OsString::from_vec(b"/raw-\xff\nname".to_vec());
        let report = CopyReport {
            completed: vec![path.into()],
            ..Default::default()
        };
        let result = describe(&outcome(Ok(report)));
        assert!(result.text.contains("\\xFF") || result.text.contains("\\xff"));
        assert!(result.text.contains("\\nname"));
    }
    #[test]
    fn skipped_destinations_are_not_reported_as_published_and_omissions_are_explicit() {
        let r = CopyReport {
            skipped: vec!["/occupied".into()],
            skipped_count: 3,
            skipped_omitted: 2,
            ..Default::default()
        };
        let d = describe(&outcome(Ok(r)));
        assert!(d.text.contains("Copia finalizada con destinos omitidos"));
        assert!(
            d.text
                .contains("Destinos omitidos: 3 (rutas omitidas del informe: 2)")
        );
        assert!(
            d.text
                .contains("Destino solicitado omitido (ocupado): \"/occupied\"")
        );
        assert!(!d.text.contains("Publicado: \"/occupied\""));
    }
    #[test]
    fn skip_counters_precede_long_cleanup_diagnostics_and_survive_truncation() {
        let r = CopyReport {
            skipped_count: 42,
            skipped_omitted: 42,
            cleanup_errors: vec!["x".repeat(LIMIT)],
            ..Default::default()
        };
        let d = describe(&outcome(Ok(r)));
        assert!(d.truncated);
        assert!(
            d.text
                .contains("Destinos omitidos: 42 (rutas omitidas del informe: 42)")
        );
        assert!(d.text.ends_with(TAIL));
    }
}
