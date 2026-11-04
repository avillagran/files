//! Strict bounded comparison frames. A validated report remains read-only history.
use crate::{
    directory_compare::{self as compare, Difference, Kind, Progress, Report, Row},
    relative_source::DirectoryIdentity,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Component, Path},
};
pub const PAGE_ROWS: usize = 128;
pub const PAGE_NAMES: usize = 4096;
pub const MAX_FRAME: usize = 64 * 1024;
pub const MAX_UPDATES: usize = 512;
// JSON string controls expand to six bytes; raw arrays expand to at most four.
// 512 overhead bytes per row also cover a page envelope even for singleton pages.
// Progress and terminal envelopes have separate allowances. No result truncation.
pub const MAX_STREAM: usize =
    6 * 2 * compare::MAX_NAMES + 512 * 2 * compare::MAX_ITEMS + MAX_UPDATES * 512 + 4096;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub id: u64,
    pub event: Event,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Progress {
        progress: Progress,
    },
    Page {
        offset: usize,
        rows: Vec<Row>,
    },
    Done {
        roots: [DirectoryIdentity; 2],
        progress: Progress,
    },
    Error {
        message: String,
    },
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Informe de comparación inválido",
    )
}
pub fn relative(path: &Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    !bytes.is_empty()
        && bytes.len() <= 4096
        && !bytes.contains(&0)
        && path.components().count() <= compare::MAX_DEPTH + 1
        && path.components().all(|p| matches!(p, Component::Normal(_)))
        && path
            .components()
            .collect::<std::path::PathBuf>()
            .as_os_str()
            == path.as_os_str()
}
pub fn validate_progress(p: Progress) -> io::Result<()> {
    let classified = p
        .equal
        .checked_add(p.different)
        .and_then(|n| n.checked_add(p.unknown))
        .ok_or_else(bad)?;
    if p.phase > 3
        || p.entries.iter().any(|&n| n > compare::MAX_ENTRIES)
        || p.compared > 2 * compare::MAX_ITEMS as u64
        || p.compared > p.entries.iter().sum::<u64>()
        || p.bytes_read > compare::MAX_READ
        || p.reasons & !63 != 0
        || classified > p.compared
        || (p.phase < 3 && classified != 0)
        || (p.phase == 3 && classified != p.compared)
        || (p.phase < 2 && (p.compared != 0 || p.bytes_read != 0))
        || (p.phase == 0 && p.entries[1] != 0)
        || (p.unknown > 0 && p.reasons == 0)
    {
        return Err(bad());
    }
    Ok(())
}
pub fn advances(a: Progress, b: Progress) -> bool {
    b.phase >= a.phase
        && b.entries.iter().zip(a.entries).all(|(b, a)| *b >= a)
        && b.compared >= a.compared
        && b.bytes_read >= a.bytes_read
        && b.reasons & a.reasons == a.reasons
        && b.equal >= a.equal
        && b.different >= a.different
        && b.unknown >= a.unknown
}
pub fn validate_row(row: &Row) -> io::Result<()> {
    if !relative(&row.path) || row.left.is_none() && row.right.is_none() {
        return Err(bad());
    }
    let same = row.left.is_some() && row.left == row.right;
    let shape = match row.difference {
        Difference::Equal => same && row.left != Some(Kind::Other),
        Difference::OnlyLeft => row.left.is_some() && row.right.is_none() && !row.metadata_differs,
        Difference::OnlyRight => row.right.is_some() && row.left.is_none() && !row.metadata_differs,
        Difference::Type => {
            row.left.is_some() && row.right.is_some() && !same && !row.metadata_differs
        }
        Difference::Content => same && row.left == Some(Kind::File),
        Difference::Children => same && row.left == Some(Kind::Directory),
        Difference::Link => same && row.left == Some(Kind::Symlink),
        Difference::Unknown => !row.metadata_differs || same,
    };
    if !shape {
        return Err(bad());
    }
    Ok(())
}
pub fn validate(report: &Report) -> io::Result<()> {
    validate_progress(report.progress)?;
    if report.progress.phase != 3 || report.rows.len() != report.progress.compared as usize {
        return Err(bad());
    }
    let mut names = [0usize; 2];
    let mut items = [0u64; 2];
    let mut equal = 0;
    let mut different = 0;
    let mut unknown = 0;
    let mut index = BTreeMap::new();
    for (i, row) in report.rows.iter().enumerate() {
        validate_row(row)?;
        if i > 0 && report.rows[i - 1].path >= row.path {
            return Err(bad());
        }
        for (side, kind) in [row.left, row.right].iter().enumerate() {
            if kind.is_some() {
                names[side] = names[side]
                    .checked_add(row.path.as_os_str().len())
                    .ok_or_else(bad)?;
                items[side] += 1;
                if names[side] > compare::MAX_NAMES
                    || items[side] > compare::MAX_ITEMS as u64
                    || items[side] > report.progress.entries[side]
                {
                    return Err(bad());
                }
            }
        }
        match row.difference {
            Difference::Equal if !row.metadata_differs => equal += 1,
            Difference::Unknown => unknown += 1,
            _ => different += 1,
        }
        index.insert(row.path.as_path(), i);
    }
    if (equal, different, unknown)
        != (
            report.progress.equal,
            report.progress.different,
            report.progress.unknown,
        )
    {
        return Err(bad());
    }
    let mut child_difference = vec![false; report.rows.len()];
    for row in &report.rows {
        if let Some(parent) = row.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let &i = index.get(parent).ok_or_else(bad)?;
            let p = &report.rows[i];
            if row.left.is_some() && p.left != Some(Kind::Directory)
                || row.right.is_some() && p.right != Some(Kind::Directory)
            {
                return Err(bad());
            }
            if p.left == Some(Kind::Directory) && p.right == Some(Kind::Directory) {
                let changed = row.difference != Difference::Equal || row.metadata_differs;
                child_difference[i] |= changed;
                if p.difference == Difference::Equal && changed
                    || p.difference == Difference::Children && row.difference == Difference::Unknown
                {
                    return Err(bad());
                }
            }
        }
    }
    for (i, row) in report.rows.iter().enumerate() {
        if row.difference == Difference::Children && !child_difference[i] {
            return Err(bad());
        }
    }
    Ok(())
}
pub fn page_end(rows: &[Row], start: usize) -> usize {
    let mut end = start.min(rows.len());
    let mut bytes = 0;
    while end < rows.len() && end - start < PAGE_ROWS {
        let next = rows[end].path.as_os_str().len();
        if next > PAGE_NAMES - bytes {
            break;
        }
        bytes += next;
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn one() -> Report {
        Report {
            roots: [DirectoryIdentity {
                device: 1,
                inode: 2,
            }; 2],
            progress: Progress {
                phase: 3,
                entries: [1, 1],
                compared: 1,
                equal: 1,
                ..Progress::default()
            },
            rows: vec![Row {
                path: "file".into(),
                left: Some(Kind::File),
                right: Some(Kind::File),
                difference: Difference::Equal,
                metadata_differs: false,
            }],
        }
    }
    #[test]
    fn report_shape_counts_order_and_canonical_paths_are_required() {
        validate(&one()).unwrap();
        for name in [
            "",
            "/absolute",
            "../escape",
            "a/../b",
            "a//b",
            "./a",
            "a/./b",
            "a/",
        ] {
            let mut r = one();
            r.rows[0].path = PathBuf::from(name);
            assert!(validate(&r).is_err(), "{name:?}");
        }
        let mut r = one();
        r.progress.equal = 0;
        assert!(validate(&r).is_err());
        let mut r = one();
        r.rows[0].difference = Difference::OnlyLeft;
        assert!(validate(&r).is_err());
        let mut r = one();
        r.rows[0].left = Some(Kind::Other);
        r.rows[0].right = Some(Kind::Other);
        assert!(validate(&r).is_err());
        let mut r = one();
        r.rows.push(r.rows[0].clone());
        r.progress.compared = 2;
        r.progress.equal = 2;
        r.progress.entries = [2, 2];
        assert!(validate(&r).is_err());
    }
    #[test]
    fn ancestors_must_exist_and_cannot_hide_differing_or_unknown_children() {
        let mut r = one();
        r.rows[0].path = "parent/child".into();
        assert!(validate(&r).is_err());
        r.rows.insert(
            0,
            Row {
                path: "parent".into(),
                left: Some(Kind::Directory),
                right: Some(Kind::Directory),
                difference: Difference::Equal,
                metadata_differs: false,
            },
        );
        r.progress.entries = [2, 2];
        r.progress.compared = 2;
        r.progress.equal = 2;
        validate(&r).unwrap();
        r.rows[1].difference = Difference::Content;
        r.progress.equal = 1;
        r.progress.different = 1;
        assert!(validate(&r).is_err());
        r.rows[0].difference = Difference::Children;
        r.progress.equal = 0;
        r.progress.different = 2;
        validate(&r).unwrap();
        r.rows[1].difference = Difference::Unknown;
        r.progress.different = 1;
        r.progress.unknown = 1;
        r.progress.reasons = compare::CHANGED;
        assert!(validate(&r).is_err());
        r.rows[0].difference = Difference::Unknown;
        r.progress.different = 0;
        r.progress.unknown = 2;
        validate(&r).unwrap();
        r.rows[0].left = Some(Kind::File);
        assert!(validate(&r).is_err());
    }
    #[test]
    fn progress_rejects_impossible_phases_overflow_and_reason_regression() {
        for p in [
            Progress {
                phase: 4,
                ..Progress::default()
            },
            Progress {
                equal: u64::MAX,
                different: 1,
                ..Progress::default()
            },
            Progress {
                phase: 0,
                entries: [0, 1],
                ..Progress::default()
            },
            Progress {
                phase: 1,
                bytes_read: 1,
                ..Progress::default()
            },
            Progress {
                phase: 2,
                reasons: 64,
                ..Progress::default()
            },
            Progress {
                phase: 3,
                entries: [1, 1],
                compared: 1,
                unknown: 1,
                ..Progress::default()
            },
        ] {
            assert!(validate_progress(p).is_err());
        }
        let a = Progress {
            phase: 2,
            entries: [3, 4],
            compared: 2,
            bytes_read: 8,
            reasons: compare::CHANGED,
            ..Progress::default()
        };
        validate_progress(a).unwrap();
        assert!(!advances(a, Progress { reasons: 0, ..a }));
        assert!(!advances(a, Progress { bytes_read: 7, ..a }));
        assert!(advances(a, Progress { bytes_read: 9, ..a }));
    }
    #[test]
    fn frames_reject_unknown_fields_and_preserve_raw_paths() {
        let frame = Frame {
            id: 9,
            event: Event::Progress {
                progress: Progress::default(),
            },
        };
        let mut value = serde_json::to_value(frame).unwrap();
        value["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Frame>(value).is_err());
        let value = serde_json::json!({"id":9,"event":{"kind":"progress","progress":{"phase":0,"entries":[0,0],"compared":0,"bytes_read":0,"equal":0,"different":0,"unknown":0,"reasons":0,"extra":1}}});
        assert!(serde_json::from_value::<Frame>(value).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut row = one().rows.remove(0);
            row.path = std::ffi::OsString::from_vec(b"raw\xff\n".to_vec()).into();
            let frame = Frame {
                id: 9,
                event: Event::Page {
                    offset: 0,
                    rows: vec![row],
                },
            };
            let wire = serde_json::to_vec(&frame).unwrap();
            let restored: Frame = serde_json::from_slice(&wire).unwrap();
            let Event::Page { rows, .. } = restored.event else {
                panic!()
            };
            assert_eq!(rows[0].path.as_os_str().as_encoded_bytes(), b"raw\xff\n");
        }
    }
    #[test]
    fn complete_maximum_control_name_report_fits_derived_stream_and_page_bounds() {
        let mut rows = Vec::new();
        for side in 0..2 {
            for i in 0..compare::MAX_ITEMS {
                rows.push(Row {
                    path: format!(
                        "{}{:05}{}",
                        "\u{1}".repeat(120),
                        i,
                        if side == 0 { 'L' } else { 'R' }
                    )
                    .into(),
                    left: (side == 0).then_some(Kind::File),
                    right: (side == 1).then_some(Kind::File),
                    difference: if side == 0 {
                        Difference::OnlyLeft
                    } else {
                        Difference::OnlyRight
                    },
                    metadata_differs: false,
                });
            }
        }
        rows.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        let report = Report {
            roots: [DirectoryIdentity {
                device: u64::MAX,
                inode: u64::MAX,
            }; 2],
            progress: Progress {
                phase: 3,
                entries: [compare::MAX_ITEMS as u64; 2],
                compared: rows.len() as u64,
                different: rows.len() as u64,
                ..Progress::default()
            },
            rows,
        };
        validate(&report).unwrap();
        let mut total = 0;
        let mut start = 0;
        while start < report.rows.len() {
            let end = page_end(&report.rows, start);
            assert!(end > start);
            let bytes = serde_json::to_vec(&Frame {
                id: u64::MAX,
                event: Event::Page {
                    offset: start,
                    rows: report.rows[start..end].to_vec(),
                },
            })
            .unwrap();
            assert!(bytes.len() <= MAX_FRAME);
            total += bytes.len() + 1;
            start = end;
        }
        let p = Progress {
            phase: 2,
            entries: [compare::MAX_ENTRIES; 2],
            compared: 2 * compare::MAX_ITEMS as u64,
            bytes_read: compare::MAX_READ,
            reasons: 63,
            ..Progress::default()
        };
        validate_progress(p).unwrap();
        let update = serde_json::to_vec(&Frame {
            id: u64::MAX,
            event: Event::Progress { progress: p },
        })
        .unwrap();
        assert!(update.len() < 512);
        total += MAX_UPDATES * (update.len() + 1);
        total += serde_json::to_vec(&Frame {
            id: u64::MAX,
            event: Event::Done {
                roots: report.roots,
                progress: report.progress,
            },
        })
        .unwrap()
        .len()
            + 1;
        assert!(total > 4 * 1024 * 1024);
        assert!(total <= MAX_STREAM);
    }
}

/// Typed stream ledger. Call `finish` only after EOF and successful child reap;
/// pages and even a terminal frame are not an early publication permission.
pub struct Collector {
    id: u64,
    read_limit: u64,
    expected: Option<[DirectoryIdentity; 2]>,
    previous: Progress,
    updates: usize,
    rows: Vec<Row>,
    names: [usize; 2],
    items: [usize; 2],
    terminal: Option<Result<Report, String>>,
    poisoned: bool,
}
impl Collector {
    pub fn new(
        id: u64,
        read_limit: u64,
        expected: Option<[DirectoryIdentity; 2]>,
    ) -> io::Result<Self> {
        if read_limit > compare::MAX_READ {
            return Err(bad());
        }
        Ok(Self {
            id,
            read_limit,
            expected,
            previous: Progress::default(),
            updates: 0,
            rows: Vec::new(),
            names: [0; 2],
            items: [0; 2],
            terminal: None,
            poisoned: false,
        })
    }
    pub fn accept(&mut self, frame: Frame) -> io::Result<Option<Progress>> {
        if self.poisoned {
            return Err(bad());
        }
        let result = self.accept_inner(frame);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn accept_inner(&mut self, frame: Frame) -> io::Result<Option<Progress>> {
        if frame.id != self.id || self.terminal.is_some() {
            return Err(bad());
        }
        match frame.event {
            Event::Progress { progress } => {
                validate_progress(progress)?;
                if progress.phase >= 3
                    || !self.rows.is_empty()
                    || self.updates >= MAX_UPDATES
                    || !advances(self.previous, progress)
                    || progress.bytes_read > self.read_limit
                {
                    return Err(bad());
                }
                self.updates += 1;
                self.previous = progress;
                Ok(Some(progress))
            }
            Event::Page { offset, rows } => {
                if offset != self.rows.len()
                    || rows.is_empty()
                    || rows.len() > PAGE_ROWS
                    || self.rows.len() + rows.len() > 2 * compare::MAX_ITEMS
                {
                    return Err(bad());
                }
                let mut names = self.names;
                let mut items = self.items;
                let mut page_names = 0usize;
                let mut previous = self.rows.last().map(|r| r.path.as_path());
                for row in &rows {
                    validate_row(row)?;
                    if previous.is_some_and(|p| p >= row.path.as_path()) {
                        return Err(bad());
                    }
                    previous = Some(&row.path);
                    page_names += row.path.as_os_str().len();
                    if page_names > PAGE_NAMES {
                        return Err(bad());
                    }
                    for (side, kind) in [row.left, row.right].iter().enumerate() {
                        if kind.is_some() {
                            names[side] += row.path.as_os_str().len();
                            items[side] += 1;
                            if names[side] > compare::MAX_NAMES || items[side] > compare::MAX_ITEMS
                            {
                                return Err(bad());
                            }
                        }
                    }
                }
                self.names = names;
                self.items = items;
                self.rows.extend(rows);
                Ok(None)
            }
            Event::Done { roots, progress } => {
                if self.expected.is_some_and(|ids| ids != roots)
                    || !advances(self.previous, progress)
                    || progress.bytes_read > self.read_limit
                {
                    return Err(bad());
                }
                let report = Report {
                    roots,
                    progress,
                    rows: std::mem::take(&mut self.rows),
                };
                validate(&report)?;
                self.terminal = Some(Ok(report));
                Ok(None)
            }
            Event::Error { message } => {
                if !self.rows.is_empty()
                    || message.is_empty()
                    || message.len() > 256
                    || message.chars().any(char::is_control)
                {
                    return Err(bad());
                }
                self.terminal = Some(Err(message));
                Ok(None)
            }
        }
    }
    pub fn finish(self) -> Result<Report, String> {
        if self.poisoned {
            return Err(bad().to_string());
        }
        self.terminal
            .unwrap_or_else(|| Err("Comparación sin frame terminal".into()))
    }
}

#[cfg(test)]
mod collector_tests {
    use super::*;
    fn roots() -> [DirectoryIdentity; 2] {
        [DirectoryIdentity {
            device: 3,
            inode: 4,
        }; 2]
    }
    fn row() -> Row {
        Row {
            path: "file".into(),
            left: Some(Kind::File),
            right: Some(Kind::File),
            difference: Difference::Equal,
            metadata_differs: false,
        }
    }
    fn done() -> Frame {
        Frame {
            id: 7,
            event: Event::Done {
                roots: roots(),
                progress: Progress {
                    phase: 3,
                    entries: [1, 1],
                    compared: 1,
                    equal: 1,
                    ..Progress::default()
                },
            },
        }
    }
    fn page() -> Frame {
        Frame {
            id: 7,
            event: Event::Page {
                offset: 0,
                rows: vec![row()],
            },
        }
    }
    #[test]
    fn exact_stream_finishes_but_missing_or_trailing_terminal_is_rejected() {
        let mut c = Collector::new(7, 0, Some(roots())).unwrap();
        c.accept(page()).unwrap();
        c.accept(done()).unwrap();
        let r = c.finish().unwrap();
        assert!(r.progress.complete());
        assert_eq!(r.rows.len(), 1);
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(page()).unwrap();
        assert!(c.finish().is_err());
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(page()).unwrap();
        c.accept(done()).unwrap();
        assert!(c.accept(done()).is_err());
        assert!(c.finish().is_err());
    }
    #[test]
    fn malformed_offsets_ids_order_and_late_progress_poison_the_entire_ledger() {
        for frame in [
            Frame {
                id: 8,
                event: Event::Page {
                    offset: 0,
                    rows: vec![row()],
                },
            },
            Frame {
                id: 7,
                event: Event::Page {
                    offset: 1,
                    rows: vec![row()],
                },
            },
            Frame {
                id: 7,
                event: Event::Page {
                    offset: 0,
                    rows: vec![row(), row()],
                },
            },
            Frame {
                id: 7,
                event: Event::Page {
                    offset: 0,
                    rows: vec![],
                },
            },
        ] {
            let mut c = Collector::new(7, 0, None).unwrap();
            assert!(c.accept(frame).is_err());
            assert!(c.accept(page()).is_err());
            assert!(c.finish().is_err());
        }
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(page()).unwrap();
        assert!(
            c.accept(Frame {
                id: 7,
                event: Event::Progress {
                    progress: Progress::default()
                }
            })
            .is_err()
        );
        assert!(c.finish().is_err());
    }
    #[test]
    fn intent_roots_and_configured_read_budget_are_enforced_at_terminal() {
        let mut c = Collector::new(
            7,
            0,
            Some(
                [DirectoryIdentity {
                    device: 0,
                    inode: 0,
                }; 2],
            ),
        )
        .unwrap();
        c.accept(page()).unwrap();
        assert!(c.accept(done()).is_err());
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(page()).unwrap();
        let mut end = done();
        if let Event::Done { progress, .. } = &mut end.event {
            progress.bytes_read = 1;
        }
        assert!(c.accept(end).is_err());
        assert!(Collector::new(7, compare::MAX_READ + 1, None).is_err());
    }
    #[test]
    fn error_is_terminal_and_never_accepts_partial_pages_or_controls() {
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(Frame {
            id: 7,
            event: Event::Error {
                message: "Raíz no disponible".into(),
            },
        })
        .unwrap();
        assert_eq!(c.finish().unwrap_err(), "Raíz no disponible");
        for message in ["", "bad\nmessage"] {
            let mut c = Collector::new(7, 0, None).unwrap();
            assert!(
                c.accept(Frame {
                    id: 7,
                    event: Event::Error {
                        message: message.into()
                    }
                })
                .is_err()
            );
        }
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(page()).unwrap();
        assert!(
            c.accept(Frame {
                id: 7,
                event: Event::Error {
                    message: "late".into()
                }
            })
            .is_err()
        );
    }
    #[test]
    fn progress_update_cap_and_latched_reasons_are_strict() {
        let mut c = Collector::new(7, 0, None).unwrap();
        for _ in 0..MAX_UPDATES {
            c.accept(Frame {
                id: 7,
                event: Event::Progress {
                    progress: Progress::default(),
                },
            })
            .unwrap();
        }
        assert!(
            c.accept(Frame {
                id: 7,
                event: Event::Progress {
                    progress: Progress::default()
                }
            })
            .is_err()
        );
        let mut c = Collector::new(7, 0, None).unwrap();
        c.accept(Frame {
            id: 7,
            event: Event::Progress {
                progress: Progress {
                    reasons: compare::LIMIT,
                    ..Progress::default()
                },
            },
        })
        .unwrap();
        assert!(
            c.accept(Frame {
                id: 7,
                event: Event::Progress {
                    progress: Progress::default()
                }
            })
            .is_err()
        );
    }
    #[test]
    fn two_maximum_root_requests_fit_existing_line_budget_without_loss() {
        use crate::protocol::{MAX_LINE_BYTES, Request};
        let left = std::path::PathBuf::from(format!("/{}", "\u{1}".repeat(4095)));
        let right = std::path::PathBuf::from(format!("/{}", "\u{2}".repeat(4095)));
        let request = Request::CompareDirectories {
            id: u64::MAX,
            left: left.clone(),
            right: right.clone(),
            expected: Some(roots()),
            read_limit: compare::MAX_READ,
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(bytes.len() < MAX_LINE_BYTES);
        let Request::CompareDirectories {
            left: a, right: b, ..
        } = serde_json::from_slice(&bytes).unwrap()
        else {
            panic!()
        };
        assert_eq!((a, b), (left, right));
    }
}
