//! Bounded, explicitly requested journal catalogue. Read-only; never resume jobs.
use crate::{
    model::{Action, Hit, Model},
    move_operation::Context,
    painter::{Painter, Rect},
};
use files_core::{
    move_journal::{Journal, Phase},
    move_manifest::Kind,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub struct Entry {
    pub job: [u8; 16],
    pub context: Option<Context>,
    pub phase: Option<Phase>,
    pub diagnostic: String,
}
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub truncated: bool,
}
#[derive(Default)]
pub struct Ui {
    pub open: bool,
    pub pending: Option<(u64, Arc<AtomicBool>)>,
    pub entries: Vec<Entry>,
    pub error: String,
    pub scroll: usize,
    pub truncated: bool,
}
impl Ui {
    pub fn close(&mut self) {
        self.open = false;
        if let Some((_, flag)) = &self.pending {
            flag.store(true, Ordering::Release);
        }
    }
}
pub fn read(cancel: &AtomicBool) -> Result<Catalog, String> {
    read_at(&files_core::theme::config_dir().join("operations"), cancel)
}
pub(crate) fn read_at(root: &std::path::Path, cancel: &AtomicBool) -> Result<Catalog, String> {
    use std::{
        fs,
        os::{
            fd::AsRawFd,
            unix::{
                ffi::{OsStrExt, OsStringExt},
                fs::MetadataExt,
            },
        },
    };
    if !root.is_absolute() || root.as_os_str().len() > 4096 {
        return Err("Ruta de journals no válida".into());
    }
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Catalog {
                entries: Vec::new(),
                truncated: false,
            });
        }
        Err(e) => return Err(e.to_string()),
    };
    use std::os::unix::fs::OpenOptionsExt;
    let m = file.metadata().map_err(|e| e.to_string())?;
    if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("Directorio de operaciones no privado; no se abre".into());
    }
    let mut catalog = Catalog {
        entries: Vec::new(),
        truncated: false,
    };
    let mut bytes = 0usize;
    for (seen, entry) in fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .map_err(|e| e.to_string())?
        .enumerate()
    {
        if cancel.load(Ordering::Acquire) {
            return Err("Lectura cancelada".into());
        }
        if seen >= 1024 || catalog.entries.len() >= 32 {
            catalog.truncated = true;
            break;
        }
        let name = entry.map_err(|e| e.to_string())?.file_name();
        let raw = name.as_bytes();
        if raw.len() != 45 || !raw.starts_with(b"move-") || !raw.ends_with(b".journal") {
            continue;
        }
        let mut job = [0u8; 16];
        let mut valid = true;
        for (i, pair) in raw[5..37].as_chunks::<2>().0.iter().enumerate() {
            fn nibble(b: u8) -> Option<u8> {
                match b {
                    b'0'..=b'9' => Some(b - b'0'),
                    b'a'..=b'f' => Some(b - b'a' + 10),
                    _ => None,
                }
            }
            if let (Some(a), Some(b)) = (nibble(pair[0]), nibble(pair[1])) {
                job[i] = a * 16 + b;
            } else {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }
        let (context, phase, diagnostic) = match Journal::open_at_single_file(&file, job) {
            Ok(j) => {
                let phase = j.state().phase();
                if matches!(phase, Phase::Completed | Phase::Restored) {
                    continue;
                }
                let m = j.manifest();
                if m.entries().len() != 1
                    || m.entries()[0].identity.kind != Kind::File
                    || m.entries()[0].revision.links != 1
                {
                    (
                        None,
                        Some(phase),
                        "Recuperación de árboles/hardlinks pendiente; journal conservado".into(),
                    )
                } else {
                    let cost = 2
                        * (m.source_parent().len()
                            + m.destination_parent().len()
                            + 2 * m.entries()[0].relative.len()
                            + 2)
                        + root.as_os_str().len()
                        + 2048;
                    if bytes.checked_add(cost).is_none_or(|n| n > 512 * 1024) {
                        catalog.truncated = true;
                        break;
                    }
                    bytes += cost;
                    let source =
                        PathBuf::from(std::ffi::OsString::from_vec(m.source_parent().to_vec()))
                            .join(std::ffi::OsStr::from_bytes(&m.entries()[0].relative));
                    let destination = PathBuf::from(std::ffi::OsString::from_vec(
                        m.destination_parent().to_vec(),
                    ))
                    .join(std::ffi::OsStr::from_bytes(&m.entries()[0].relative));
                    (
                        Some(Context {
                            job,
                            proof: Some(j.intent_proof().map_err(|e| e.to_string())?),
                            journals: root.to_path_buf(),
                            source,
                            destination,
                            recovery: None,
                            reviewed_phase: Some(phase),
                        }),
                        Some(phase),
                        format!("{phase:?} · elegir acción; no se reanuda automáticamente"),
                    )
                }
            }
            Err(e) => (
                None,
                None,
                format!("No abierto (ocupado/incompatible/error): {e}"),
            ),
        };
        catalog.entries.push(Entry {
            job,
            context,
            phase,
            diagnostic,
        });
    }
    catalog.entries.sort_unstable_by_key(|e| e.job);
    Ok(catalog)
}
impl Model {
    pub fn open_move_catalog(&mut self) {
        if self.mutation_applied || self.properties_dialog.is_some() {
            self.error = "Termina o cierra la operación activa antes de revisar journals".into();
            return;
        }
        self.preview.close();
        self.cancel_copy_dialog();
        self.cancel_rename();
        self.settings = false;
        self.notice_open = false;
        self.recovery_catalog.open = true;
        if self.recovery_catalog.pending.is_some() {
            return;
        }
        self.copy_id = self.copy_id.checked_add(1).expect("operation ID exhausted");
        let cancel = Arc::new(AtomicBool::new(false));
        match self.aux.tx.try_send(crate::aux_io::Request::MoveCatalog(
            self.copy_id,
            cancel.clone(),
        )) {
            Ok(()) => {
                self.recovery_catalog.pending = Some((self.copy_id, cancel));
                self.recovery_catalog.error.clear();
            }
            Err(_) => {
                self.recovery_catalog.error =
                    "Trabajador ocupado; vuelve a abrir para reintentar".into()
            }
        }
    }
    pub fn select_move_catalog(&mut self, index: usize) {
        let Some(entry) = self.recovery_catalog.entries.get(index) else {
            return;
        };
        self.recovery_context = entry.context.clone().or_else(|| {
            self.copy_last
                .as_ref()
                .and_then(|o| o.exdev_error())
                .filter(|e| e.recovery_needed)
                .map(|e| &e.context)
                .filter(|c| c.job == entry.job)
                .cloned()
        });
        self.operation_message = Some("Movimiento interrumpido · recuperación explícita".into());
        self.operation_details = vec![format!("Fase: {:?} · {}", entry.phase, entry.diagnostic)];
        if let Some(c) = &self.recovery_context {
            self.operation_details.extend([format!("Journal {} en {:?}",c.job_label(),c.journals),format!("Origen capturado: {:?}",c.source),format!("Destino capturado: {:?}",c.destination),"Finalizar verifica y renueva barreras; Restaurar conserva cualquier destino publicado. No se sobrescriben ocupantes.".into()]);
        }
        self.recovery_catalog.close();
        self.copy_notice_id = None;
        self.notice_open = true;
        self.notice_scroll = 0;
    }
}
fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Prepared => "Preparado · origen conservado",
        Phase::StageIntent => "Aislamiento interrumpido · revisar origen",
        Phase::SourceStaged => "Origen aislado · se puede restaurar",
        Phase::PublishIntent => "Publicación interrumpida · verificar destino",
        Phase::DestinationPublished => "Destino publicado · sincronización por confirmar",
        Phase::DestinationSynced => "Destino sincronizado · movimiento por finalizar",
        Phase::Deleting => "Finalización interrumpida · verificar antes de continuar",
        Phase::RestoreIntent => "Restauración interrumpida · se puede continuar",
        Phase::Completed => "Movimiento terminado",
        Phase::Restored => "Origen restaurado",
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    m.hits.clear();
    let width = (w - 32).clamp(300, 1000);
    let height = (h - 32).clamp(260, 620);
    let x = (w - width) / 2;
    let y = (h - height) / 2;
    let c = m.palette.clone();
    p.rounded_rect(Rect::new(x, y, width, height), 8 * m.radius, c.background);
    p.text(
        Rect::new(x + 16, y + 12, width - 32, 34),
        "Movimientos interrumpidos",
        18,
        c.foreground,
        true,
        false,
    );
    let text = if m.recovery_catalog.pending.is_some() {
        "Leyendo journals…"
    } else if !m.recovery_catalog.error.is_empty() {
        &m.recovery_catalog.error
    } else if m.recovery_catalog.truncated {
        "Lista parcial: se alcanzó el límite; puede haber más movimientos"
    } else if m.recovery_catalog.entries.is_empty() {
        "No hay movimientos interrumpidos registrados"
    } else {
        "Elige un journal para revisar. Ninguna operación se reanuda sola."
    };
    p.text(
        Rect::new(x + 16, y + 50, width - 32, 38),
        text,
        11,
        c.dark_foreground,
        true,
        false,
    );
    let rows = ((height - 150) / 52).max(0) as usize;
    m.recovery_catalog.scroll = m
        .recovery_catalog
        .scroll
        .min(m.recovery_catalog.entries.len().saturating_sub(rows.max(1)));
    for (row, entry) in m
        .recovery_catalog
        .entries
        .iter()
        .skip(m.recovery_catalog.scroll)
        .take(rows)
        .enumerate()
    {
        let r = Rect::new(x + 16, y + 96 + row as i32 * 52, width - 32, 46);
        p.rounded_rect(r, 4 * m.radius, c.selection);
        let job = entry
            .job
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let title = entry
            .context
            .as_ref()
            .map(|c| {
                format!(
                    "{:?} → {:?}",
                    c.source.file_name().unwrap_or(c.source.as_os_str()),
                    c.destination.parent().unwrap_or(&c.destination)
                )
            })
            .unwrap_or_else(|| format!("Journal no abierto · {job}"));
        let state = entry
            .phase
            .map(phase_label)
            .unwrap_or("Ocupado o no disponible; abre detalles");
        p.text(
            Rect::new(r.x + 10, r.y, r.w - 20, 22),
            &title,
            11,
            c.accent,
            true,
            false,
        );
        p.text(
            Rect::new(r.x + 10, r.y + 22, r.w - 20, 22),
            state,
            10,
            c.foreground,
            true,
            false,
        );
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action: Action::RecoverySelect(m.recovery_catalog.scroll + row),
        });
    }
    for (r, label, action) in [
        (
            Rect::new(x + 16, y + height - 44, 170, 32),
            crate::fonts::icon("  Actualizar", "Actualizar"),
            Action::RecoveryCatalog,
        ),
        (
            Rect::new(x + width - 150, y + height - 44, 134, 32),
            crate::fonts::icon("  Cerrar · Esc", "Cerrar · Esc"),
            Action::RecoveryClose,
        ),
    ] {
        p.rounded_rect(r, 4 * m.radius, c.selection);
        p.text(r, label, 12, c.accent, true, false);
        m.hits.push(Hit {
            generation: m.generation,
            rect: r,
            action,
        });
    }
}
