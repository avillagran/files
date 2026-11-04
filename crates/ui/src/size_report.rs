//! Bounded text observations, never filesystem snapshots or mutation authority.
use crate::size_queue::{MAX_ROWS, Record, Status};
use std::{
    fmt::{self, Write},
    ops::Range,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub const LIMIT: usize = 64 * 1024;
const TAIL: &str = "\n[Informe truncado a64KiB; nombres/errores restantes no incluidos. No usar este texto como autoridad para operaciones.]\n";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub measured: usize,
    pub queued: usize,
    pub running: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub unobserved: usize,
    pub bytes: Option<u64>,
    pub overflow: bool,
}
impl Summary {
    pub fn from_rows(rows: &[Record]) -> Self {
        Self::from_observations(rows.iter().map(Some))
    }
    fn from_observations<'a>(rows: impl Iterator<Item = Option<&'a Record>>) -> Self {
        let mut s = Self::default();
        let mut sum = Some(0u64);
        for row in rows {
            let Some(row) = row else {
                s.unobserved += 1;
                continue;
            };
            match row.status {
                Status::Measured { bytes, .. } => {
                    s.measured += 1;
                    sum = sum.and_then(|n| n.checked_add(bytes));
                }
                Status::Queued => s.queued += 1,
                Status::Running { .. } => s.running += 1,
                Status::Failed(_) => s.failed += 1,
                Status::Cancelled => s.cancelled += 1,
            }
        }
        s.overflow = sum.is_none();
        s.bytes = if s.measured == 0 { None } else { sum };
        s
    }
    pub fn label(self) -> String {
        if self.measured == 0 {
            return "∑ Sin mediciones completadas".into();
        }
        if self.overflow {
            return format!("∑ {} medidas · subtotal excede u64", self.measured);
        }
        format!(
            "∑ Aparente {} · {} medidas",
            files_core::format_size(self.bytes.unwrap_or(0)),
            self.measured
        )
    }
}
pub struct Report {
    pub text: String,
    pub summary: Summary,
    pub truncated: bool,
    pub measured_ids: std::sync::Arc<[u64]>,
}
struct Bounded {
    text: String,
    truncated: bool,
}
impl Write for Bounded {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.truncated {
            return Err(fmt::Error);
        }
        let remaining = (LIMIT - TAIL.len()).saturating_sub(self.text.len());
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
fn stamp(time: SystemTime) -> String {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => format!("{}.{:09}", d.as_secs(), d.subsec_nanos()),
        Err(e) => format!(
            "-{}.{:09}",
            e.duration().as_secs(),
            e.duration().subsec_nanos()
        ),
    }
}
pub fn capture(root: &Path, rows: &[Record], at: SystemTime) -> Result<Report, &'static str> {
    if rows.len() > MAX_ROWS || root.as_os_str().len() > 4096 {
        return Err("Informe fuera de los límites de contexto");
    }
    capture_rows(
        root,
        rows.iter().map(|r| (r.name.as_os_str(), Some(r))),
        rows.len(),
        at,
        false,
    )
}
pub fn capture_marked<'a>(
    root: &Path,
    rows: &'a [Record],
    names: impl Iterator<Item = &'a std::ffi::OsStr>,
    at: SystemTime,
) -> Result<Report, &'static str> {
    files_core::navigation_history::validate(root)?;
    if rows.len() > MAX_ROWS {
        return Err("Cola del informe fuera de límite");
    }
    let mut chosen = Vec::new();
    let mut bytes = 0usize;
    for name in names {
        let raw = name.as_encoded_bytes();
        if chosen.len() == MAX_ROWS
            || raw.is_empty()
            || raw.contains(&0)
            || raw.contains(&b'/')
            || name == "."
            || name == ".."
            || root.as_os_str().len() + 1 + raw.len() > 4096
            || chosen.contains(&name)
        {
            return Err("Informe de marcadas fuera de los límites; reduce marcas");
        }
        bytes = bytes.saturating_add(raw.len());
        if bytes > 64 * 1024 {
            return Err("Nombres del informe exceden64KiB");
        }
        chosen.push(name);
    }
    if chosen.is_empty() {
        return Err("Marca carpetas visibles para capturar el informe; no se usa el foco");
    }
    capture_rows(
        root,
        chosen
            .iter()
            .map(|&name| (name, rows.iter().find(|r| r.name == name))),
        chosen.len(),
        at,
        true,
    )
}
fn capture_rows<'a>(
    root: &Path,
    rows: impl Iterator<Item = (&'a std::ffi::OsStr, Option<&'a Record>)> + Clone,
    count: usize,
    at: SystemTime,
    marked: bool,
) -> Result<Report, &'static str> {
    let summary = Summary::from_observations(rows.clone().map(|(_, r)| r));
    let has_blocks = rows
        .clone()
        .any(|(_, row)| row.is_some_and(|r| r.mode == files_core::folder_usage::Mode::Allocated));
    let measured_ids = rows
        .clone()
        .filter_map(|(_, r)| {
            r.filter(|r| matches!(r.status, Status::Measured { .. }))
                .map(|r| r.id)
        })
        .collect();
    let mut out = Bounded {
        text: String::with_capacity(LIMIT),
        truncated: false,
    };
    let _ = writeln!(
        out,
        "INFORME DE TAMAÑOS · observaciones aparentes\nCarpeta: {root:?}\nCaptura Unix UTC: {}\nFilas: {} · medidas: {} · pendientes: {} · en curso: {} · errores: {} · canceladas: {}",
        stamp(at),
        count,
        summary.measured,
        summary.queued,
        summary.running,
        summary.failed,
        summary.cancelled
    );
    let _ = writeln!(
        out,
        "Alcance: {} · sin observación: {}",
        if marked {
            "carpetas marcadas visibles"
        } else {
            "toda la cola"
        },
        summary.unobserved
    );
    let subtotal = match summary.bytes {
        Some(bytes) => format!("{bytes} bytes"),
        None if summary.overflow => "EXCEDE u64; no se presenta un total numérico".into(),
        None => "SIN MEDICIONES; no equivale a cero".into(),
    };
    let _ = writeln!(
        out,
        "Subtotal medido: {subtotal}\nSólo mediciones completadas, de momentos distintos. Progreso no es total.\nIncluye ocultos internos; no sigue enlaces ni deduplica hardlinks.\nNo es uso físico, espacio liberable, snapshot del filesystem ni permiso de mutación.\nOrden: {}. Nombres/rutas debug escapados; nunca reinterpretar.\n",
        if marked {
            "listado visible marcado"
        } else {
            "admisión en cola"
        }
    );
    if has_blocks {
        let _ = writeln!(
            out,
            "Bloques: sólo archivos regulares; excluye directorios y enlaces.\nEl subtotal anterior es aparente y cuenta hardlinks por nombre.\nPor identidad: una observación dentro de cada carpeta; no es una unión entre carpetas.\nSin observación o no disponible no equivale a cero.\n"
        );
    }
    for (name, observed) in rows {
        let Some(row) = observed else {
            if writeln!(
                out,
                "#— · Nombre: {name:?}\nSIN OBSERVACIÓN · sin total medido"
            )
            .is_err()
            {
                break;
            }
            continue;
        };
        if writeln!(out, "#{} · Nombre: {:?}", row.id, name).is_err() {
            break;
        }
        let result = match &row.status {
            Status::Measured { bytes, entries, at } => writeln!(
                out,
                "MEDIDO · {bytes} bytes aparentes · {entries} entradas · Unix UTC {}",
                stamp(*at)
            ),
            Status::Queued => writeln!(out, "PENDIENTE · sin total medido"),
            Status::Running { bytes, entries } => writeln!(
                out,
                "EN CURSO · ≥{bytes} bytes visitados · {entries} entradas · sin total medido"
            ),
            Status::Failed(error) => writeln!(out, "ERROR · sin total medido · {error:?}"),
            Status::Cancelled => writeln!(out, "CANCELADO · sin total medido"),
        };
        if result.is_err() {
            break;
        }
        if row.mode == files_core::folder_usage::Mode::Allocated {
            if writeln!(
                out,
                "Modo solicitado: aparente + bloques de archivos regulares"
            )
            .is_err()
            {
                break;
            }
            if matches!(row.status, Status::Measured { .. }) {
                let result = match row.allocation {
                    Some(observation) => writeln!(
                        out,
                        "Bloques por nombre: {}\nPor identidad en esta carpeta: {}",
                        observation.by_name.label(),
                        observation.by_identity.label()
                    ),
                    None => writeln!(out, "Bloques: SIN OBSERVACIÓN; no equivale a cero"),
                };
                if result.is_err() {
                    break;
                }
            }
        }
    }
    if out.truncated {
        out.text.push_str(TAIL);
    }
    Ok(Report {
        text: out.text,
        summary,
        truncated: out.truncated,
        measured_ids,
    })
}
const MAX_LINES: usize = 4096;
#[derive(Default)]
pub struct Layout {
    key: Option<(i32, u32)>,
    pub lines: Vec<Range<usize>>,
    pub truncated: bool,
}
impl Layout {
    pub fn invalidate(&mut self) {
        self.key = None;
        self.lines.clear();
        self.truncated = false;
    }
    /// Cache byte ranges over immutable report text; never clone it per repaint.
    pub fn ensure(
        &mut self,
        text: &str,
        width: i32,
        font_key: u32,
        mut measure: impl FnMut(&str) -> f32,
    ) {
        if self.key == Some((width, font_key)) {
            return;
        }
        self.invalidate();
        self.key = Some((width, font_key));
        if self.lines.capacity() < MAX_LINES {
            self.lines.reserve_exact(MAX_LINES);
        }
        let mut start = 0;
        let mut used = 0.;
        let mut end = 0;
        for (at, c) in text.char_indices() {
            if c == '\n' {
                if self.lines.len() == MAX_LINES {
                    self.truncated = true;
                    return;
                }
                self.lines.push(start..at);
                start = at + c.len_utf8();
                end = start;
                used = 0.;
                continue;
            }
            let next = at + c.len_utf8();
            let advance = measure(&text[at..next]);
            if at > start && used + advance > width.max(1) as f32 {
                if self.lines.len() == MAX_LINES {
                    self.truncated = true;
                    return;
                }
                self.lines.push(start..at);
                start = at;
                used = 0.;
            }
            used += advance;
            end = next;
        }
        if start < end {
            if self.lines.len() < MAX_LINES {
                self.lines.push(start..end);
            } else {
                self.truncated = true;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocated_report_captures_per_root_values_and_unavailable_without_inventing_zero_or_union() {
        use files_core::folder_usage::{Mode, Observation, Reason, Value};
        let mut a = row(1, measured(7));
        a.mode = Mode::Allocated;
        a.allocation = Some(Observation {
            by_name: Value::Bytes(8192),
            by_identity: Value::Bytes(4096),
        });
        let mut b = row(
            2,
            Status::Measured {
                bytes: 9,
                entries: 65537,
                at: UNIX_EPOCH,
            },
        );
        b.mode = Mode::Allocated;
        b.allocation = Some(Observation {
            by_name: Value::Bytes(512),
            by_identity: Value::Unavailable(Reason::IdentityLimit),
        });
        let mut rows = [a, b, row(3, measured(100))];
        let all = capture(Path::new("/r"), &rows, UNIX_EPOCH).unwrap();
        assert_eq!(all.summary.bytes, Some(116));
        assert!(all.text.contains("Bloques por nombre: 8192 bytes"));
        assert!(
            all.text
                .contains("Por identidad en esta carpeta: 4096 bytes")
        );
        assert!(
            all.text
                .contains("NO DISPONIBLE · límite de65.536identidades")
        );
        assert!(all.text.contains("no es una unión entre carpetas"));
        let marked = capture_marked(
            Path::new("/r"),
            &rows,
            [std::ffi::OsStr::new("row1")].into_iter(),
            UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(marked.summary.bytes, Some(7));
        assert_eq!(marked.measured_ids.as_ref(), [1]);
        assert!(!marked.text.contains("NO DISPONIBLE"));
        rows[0].allocation = None;
        rows[0].status = Status::Queued;
        let recaptured = capture(Path::new("/r"), &rows, UNIX_EPOCH).unwrap();
        assert!(!recaptured.text.contains("Bloques por nombre: 8192 bytes"));
        assert!(marked.text.contains("Bloques por nombre: 8192 bytes"));
    }
    fn row(id: u64, status: Status) -> Record {
        Record {
            mode: files_core::folder_usage::Mode::Apparent,
            allocation: None,
            id,
            name: format!("row{id}").into(),
            status,
        }
    }
    fn measured(bytes: u64) -> Status {
        Status::Measured {
            bytes,
            entries: 1,
            at: UNIX_EPOCH,
        }
    }
    #[test]
    fn marked_report_keeps_zero_unknown_and_nonfinal_states_separate_and_checks_overflow() {
        let make = |id, name: &str, status| Record {
            mode: files_core::folder_usage::Mode::Apparent,
            allocation: None,
            id,
            name: name.into(),
            status,
        };
        let rows = vec![
            make(
                1,
                "zero",
                Status::Measured {
                    bytes: 0,
                    entries: 0,
                    at: UNIX_EPOCH,
                },
            ),
            make(2, "failed", Status::Failed("denied".into())),
            make(3, "queued", Status::Queued),
            make(
                4,
                "running",
                Status::Running {
                    bytes: 999,
                    entries: 1,
                },
            ),
            make(5, "cancelled", Status::Cancelled),
            make(
                6,
                "other",
                Status::Measured {
                    bytes: 9999,
                    entries: 1,
                    at: UNIX_EPOCH,
                },
            ),
        ];
        let names = [
            "zero",
            "failed",
            "queued",
            "running",
            "cancelled",
            "missing",
        ];
        let report = capture_marked(
            Path::new("/root"),
            &rows,
            names.iter().map(std::ffi::OsStr::new),
            UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(report.summary.bytes, Some(0));
        assert_eq!(
            (
                report.summary.measured,
                report.summary.failed,
                report.summary.queued,
                report.summary.running,
                report.summary.cancelled,
                report.summary.unobserved
            ),
            (1, 1, 1, 1, 1, 1)
        );
        assert_eq!(report.measured_ids.as_ref(), [1]);
        assert!(report.text.contains("≥999") && !report.text.contains("9999 bytes"));
        let overflow = vec![
            make(
                7,
                "max",
                Status::Measured {
                    bytes: u64::MAX,
                    entries: 1,
                    at: UNIX_EPOCH,
                },
            ),
            make(
                8,
                "one",
                Status::Measured {
                    bytes: 1,
                    entries: 1,
                    at: UNIX_EPOCH,
                },
            ),
        ];
        let report = capture_marked(
            Path::new("/root"),
            &overflow,
            [std::ffi::OsStr::new("max"), std::ffi::OsStr::new("one")].into_iter(),
            UNIX_EPOCH,
        )
        .unwrap();
        assert!(report.summary.overflow && report.summary.bytes.is_none());
        assert!(report.text.contains("EXCEDE u64"));
    }
    #[test]
    fn marked_report_subtotal_ids_and_missing_rows_exclude_unselected_observations() {
        let rows = vec![
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 1,
                name: "A".into(),
                status: Status::Measured {
                    bytes: 8,
                    entries: 1,
                    at: UNIX_EPOCH,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 2,
                name: "B".into(),
                status: Status::Measured {
                    bytes: 100,
                    entries: 2,
                    at: UNIX_EPOCH,
                },
            },
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 3,
                name: "C".into(),
                status: Status::Measured {
                    bytes: 9,
                    entries: 1,
                    at: UNIX_EPOCH,
                },
            },
        ];
        let names = [
            std::ffi::OsStr::new("C"),
            std::ffi::OsStr::new("missing"),
            std::ffi::OsStr::new("A"),
        ];
        let report =
            capture_marked(Path::new("/root"), &rows, names.into_iter(), UNIX_EPOCH).unwrap();
        assert_eq!(report.summary.bytes, Some(17));
        assert_eq!(report.summary.unobserved, 1);
        assert_eq!(report.measured_ids.as_ref(), [3, 1]);
        assert!(report.text.contains("SIN OBSERVACIÓN") && !report.text.contains(r#"Nombre: "B""#));
        assert!(report.text.contains("listado visible marcado"));
        let unknown = capture_marked(
            Path::new("/root"),
            &rows,
            [std::ffi::OsStr::new("unknown")].into_iter(),
            UNIX_EPOCH,
        )
        .unwrap();
        assert_eq!(unknown.summary.bytes, None);
        assert_eq!(unknown.summary.unobserved, 1);
        assert!(unknown.measured_ids.is_empty());
        assert!(capture_marked(Path::new("/root"), &rows, [].into_iter(), UNIX_EPOCH).is_err());
        assert!(
            capture_marked(
                Path::new("/root"),
                &rows,
                [std::ffi::OsStr::new("A"), std::ffi::OsStr::new("A")].into_iter(),
                UNIX_EPOCH
            )
            .is_err()
        );
    }
    #[test]
    fn subtotal_distinguishes_unknown_valid_zero_progress_and_overflow() {
        let rows = [
            row(1, Status::Queued),
            row(
                2,
                Status::Running {
                    bytes: u64::MAX,
                    entries: 1,
                },
            ),
            row(3, Status::Failed("error".into())),
            row(4, Status::Cancelled),
        ];
        let s = Summary::from_rows(&rows);
        assert_eq!(s.bytes, None);
        assert!(!s.overflow);
        assert_eq!((s.queued, s.running, s.failed, s.cancelled), (1, 1, 1, 1));
        assert_eq!(Summary::from_rows(&[row(0, measured(0))]).bytes, Some(0));
        let s = Summary::from_rows(&[row(1, measured(u64::MAX)), row(2, measured(1))]);
        assert!(s.overflow && s.bytes.is_none());
    }
    #[test]
    fn captured_report_is_bounded_utf8_and_explicitly_truncated_with_counts_first() {
        let rows = (0..MAX_ROWS)
            .map(|i| Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: i as u64,
                name: "ñ\n".repeat(2048).into(),
                status: measured(i as u64),
            })
            .collect::<Vec<_>>();
        let report = capture(Path::new("/owned"), &rows, UNIX_EPOCH).unwrap();
        assert!(report.truncated && report.text.len() <= LIMIT && report.text.ends_with(TAIL));
        assert!(report.text.contains("medidas: 128"));
        assert_eq!(report.summary.bytes, Some((0..128).sum()));
    }
    #[cfg(unix)]
    #[test]
    fn raw_names_errors_and_timestamps_remain_escaped_not_path_authority() {
        use std::os::unix::ffi::OsStringExt;
        let rows = [
            Record {
                mode: files_core::folder_usage::Mode::Apparent,
                allocation: None,
                id: 1,
                name: std::ffi::OsString::from_vec(b"raw-\xff\n".to_vec()),
                status: Status::Failed("bad\nerror".into()),
            },
            row(2, measured(8)),
        ];
        let report = capture(
            Path::new("/owned"),
            &rows,
            UNIX_EPOCH - std::time::Duration::from_nanos(1),
        )
        .unwrap();
        assert!(
            report.text.contains("raw-\\xFF\\n")
                && report.text.contains("bad\\nerror")
                && report.text.contains("-0.000000001")
        );
        assert_eq!(report.summary.bytes, Some(8));
    }
    #[test]
    fn layout_caches_utf8_ranges_and_bounds_pathological_narrow_views() {
        let mut layout = Layout::default();
        let text = "ññ\n字x";
        let mut calls = 0;
        layout.ensure(text, 2, 1, |_| {
            calls += 1;
            1.
        });
        let first = calls;
        assert_eq!(
            layout
                .lines
                .iter()
                .map(|r| &text[r.clone()])
                .collect::<Vec<_>>(),
            ["ññ", "字x"]
        );
        layout.ensure(text, 2, 1, |_| {
            calls += 1;
            1.
        });
        assert_eq!(calls, first);
        layout.ensure(text, 1, 1, |_| {
            calls += 1;
            1.
        });
        assert!(calls > first);
        layout.invalidate();
        layout.ensure(&"x".repeat(65536), 1, 1, |_| 1.);
        assert!(layout.truncated && layout.lines.len() == MAX_LINES);
    }
}
