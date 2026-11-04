//! Strict bounded NDJSON. One latest counter slot; no provisional paths.
use files_core::{
    protocol::Response,
    search::{Progress, Report},
};
use std::{io, sync::Mutex};
const MAX_LINE: usize = 2 * 1024 * 1024;
const MAX_MESSAGES: usize = 256;
pub struct Stream {
    line: Vec<u8>,
    last: Progress,
    seen: bool,
    messages: usize,
    metadata: bool,
    content: bool,
    regex: bool,
    result: Option<io::Result<Report>>,
    candidates: Option<files_core::search_batch::Candidates>,
    snapshot_len: usize,
    failed: bool,
}
impl Stream {
    pub fn new(metadata: bool) -> Self {
        Self::new_mode(metadata, false, false)
    }
    pub fn new_mode(metadata: bool, content: bool, batches: bool) -> Self {
        Self {
            line: Vec::new(),
            last: Progress::default(),
            seen: false,
            messages: 0,
            metadata,
            content,
            regex: false,
            result: None,
            snapshot_len: 0,
            failed: false,
            candidates: batches.then(|| files_core::search_batch::Candidates::new(content)),
        }
    }
    pub fn expect_content(mut self, content: bool) -> Self {
        self.content = content;
        self
    }
    pub fn expect_regex(mut self, regex: bool) -> Self {
        self.regex = regex;
        self.candidates = self.candidates.map(|c| c.expect_regex(regex));
        self
    }
    pub fn with_batches(mut self) -> Self {
        self.candidates =
            Some(files_core::search_batch::Candidates::new(self.content).expect_regex(self.regex));
        self
    }
    pub fn invalidated(&self) -> bool {
        self.failed || matches!(&self.result, Some(Err(_)))
    }
    pub fn feed(&mut self, data: &[u8], slot: &Mutex<Option<Progress>>) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("Stream de búsqueda ya inválido"));
        }
        if let Err(error) = self.feed_inner(data, slot) {
            self.failed = true;
            self.candidates = None;
            return Err(error);
        }
        Ok(())
    }
    fn feed_inner(&mut self, data: &[u8], slot: &Mutex<Option<Progress>>) -> io::Result<()> {
        if self.result.is_some() && !data.is_empty() {
            return Err(io::Error::other("Datos después del resumen de búsqueda"));
        }
        for (i, part) in data.split(|&b| b == b'\n').enumerate() {
            if i > 0 {
                self.decode(slot)?;
                if self.result.is_some() {
                    self.line = Vec::new();
                } else {
                    self.line.clear();
                }
            }
            if self.result.is_some() && !part.is_empty() {
                return Err(io::Error::other("Datos después del resumen de búsqueda"));
            }
            let length = self
                .line
                .len()
                .checked_add(part.len())
                .filter(|&n| n <= MAX_LINE)
                .ok_or_else(|| io::Error::other("Línea de búsqueda superior a2MiB"))?;
            if length > self.line.capacity() {
                let target = self
                    .line
                    .capacity()
                    .saturating_mul(2)
                    .max(length)
                    .min(MAX_LINE);
                self.line
                    .try_reserve_exact(target - self.line.len())
                    .map_err(io::Error::other)?;
            }
            self.line.extend_from_slice(part);
        }
        Ok(())
    }
    fn decode(&mut self, slot: &Mutex<Option<Progress>>) -> io::Result<()> {
        self.messages += 1;
        if self.messages > MAX_MESSAGES || self.result.is_some() {
            return Err(io::Error::other("Demasiados mensajes o resumen duplicado"));
        }
        match serde_json::from_slice::<Response>(&self.line)? {
            Response::SearchProgress { id: 1, progress } => {
                progress.validate_after(self.last)?;
                if (!self.seen && progress != Progress::default())
                    || (!self.metadata && progress.metadata_reads != 0)
                {
                    return Err(io::Error::other("Inicio de progreso de búsqueda inválido"));
                }
                self.seen = true;
                self.last = progress;
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(progress);
            }
            Response::SearchBatch {
                id: 1,
                version: 1,
                batch,
            } => {
                let candidates = self
                    .candidates
                    .as_mut()
                    .ok_or_else(|| io::Error::other("Lotes no solicitados"))?;
                if !self.seen
                    || candidates
                        .hits()
                        .len()
                        .checked_add(batch.hits.len())
                        .is_none_or(|n| n as u64 > self.last.found)
                {
                    return Err(io::Error::other("Lote excede contadores publicados"));
                }
                candidates.push(batch)?;
            }
            Response::Search { id: 1, report } => {
                files_core::search::validate(&report)?;
                if !self.seen
                    || report.metadata_requested != self.metadata
                    || report.content.is_some() != self.content
                    || report.content.is_some_and(|s| s.regex != self.regex)
                    || Progress::from_report(&report) != self.last
                {
                    return Err(io::Error::other(
                        "Resumen de búsqueda no coincide con progreso",
                    ));
                }
                if let Some(candidates) = self.candidates.take() {
                    candidates.reconcile(&report)?;
                }
                self.result = Some(Ok(report));
            }
            Response::Error { id: 1, mut message } => {
                self.candidates = None;
                if message.len() > 512 {
                    let mut n = 512;
                    while !message.is_char_boundary(n) {
                        n -= 1;
                    }
                    message.truncate(n);
                }
                self.result = Some(Err(io::Error::other(message)));
            }
            _ => return Err(io::Error::other("Mensaje de búsqueda inesperado")),
        }
        Ok(())
    }
    /// One immutable latest-prefix snapshot; caller owns the single unread slot.
    pub fn take_snapshot(&mut self) -> Option<std::sync::Arc<[files_core::search::Hit]>> {
        let hits = self.candidates.as_ref()?.hits();
        if hits.len() == self.snapshot_len {
            return None;
        }
        self.snapshot_len = hits.len();
        Some(std::sync::Arc::from(hits.to_vec()))
    }
    pub fn finish(self) -> io::Result<Report> {
        if self.failed {
            return Err(io::Error::other("Stream de búsqueda inválido"));
        }
        if !self.line.is_empty() {
            return Err(io::Error::other("Respuesta de búsqueda truncada"));
        }
        self.result
            .unwrap_or_else(|| Err(io::Error::other("Búsqueda sin resumen final")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wire(r: Response) -> Vec<u8> {
        let mut b = serde_json::to_vec(&r).unwrap();
        b.push(b'\n');
        b
    }
    fn snapshot(p: Progress) -> Vec<u8> {
        wire(Response::SearchProgress { id: 1, progress: p })
    }
    fn named(name: &str) -> files_core::search::Hit {
        files_core::search::Hit {
            relative: name.into(),
            kind: files_core::listing::Kind::File,
            content: None,
            snippet: None,
        }
    }
    fn batch(offset: usize, names: &[&str]) -> Vec<u8> {
        wire(Response::SearchBatch {
            id: 1,
            version: 1,
            batch: files_core::search_batch::Batch {
                offset,
                hits: names.iter().map(|n| named(n)).collect(),
            },
        })
    }
    #[test]
    fn regex_span_stream_requires_opt_in_and_exact_batch_reconciliation() {
        let report = || Report {
            hits: vec![files_core::search::Hit {
                relative: "empty".into(),
                kind: files_core::listing::Kind::File,
                content: Some(files_core::search_content::Match {
                    line: 1,
                    byte: 0,
                    regex_end: Some(0),
                }),
                snippet: None,
            }],
            visited: 1,
            content: Some(files_core::search_content::Stats {
                regex: true,
                files: 1,
                ..Default::default()
            }),
            complete: true,
            ..Default::default()
        };
        let slot = Mutex::new(None);
        for expected in [false, true] {
            let mut stream = Stream::new_mode(false, true, true).expect_regex(expected);
            stream.feed(&snapshot(Progress::default()), &slot).unwrap();
            stream
                .feed(
                    &snapshot(Progress {
                        visited: 1,
                        found: 1,
                        ..Default::default()
                    }),
                    &slot,
                )
                .unwrap();
            let batch = wire(Response::SearchBatch {
                id: 1,
                version: 1,
                batch: files_core::search_batch::Batch {
                    offset: 0,
                    hits: report().hits,
                },
            });
            let accepted = stream.feed(&batch, &slot);
            assert_eq!(accepted.is_ok(), expected);
            if expected {
                stream
                    .feed(
                        &wire(Response::Search {
                            id: 1,
                            report: report(),
                        }),
                        &slot,
                    )
                    .unwrap();
                let r = stream.finish().unwrap();
                assert!(r.hits[0].content.unwrap().zero_width());
            } else {
                assert!(stream.finish().is_err());
            }
        }
        let mut stream = Stream::new(false).expect_content(true);
        stream.feed(&snapshot(Progress::default()), &slot).unwrap();
        stream
            .feed(
                &snapshot(Progress {
                    visited: 1,
                    found: 1,
                    ..Default::default()
                }),
                &slot,
            )
            .unwrap();
        assert!(
            stream
                .feed(
                    &wire(Response::Search {
                        id: 1,
                        report: report()
                    }),
                    &slot
                )
                .is_err()
        );
    }
    #[test]
    fn batches_fragmented_discovery_order_reconcile_exact_sorted_final() {
        let slot = Mutex::new(None);
        let mut s = Stream::new_mode(false, false, true);
        let mut bytes = snapshot(Progress::default());
        bytes.extend(snapshot(Progress {
            visited: 2,
            found: 2,
            ..Default::default()
        }));
        bytes.extend(batch(0, &["b"]));
        bytes.extend(batch(1, &["a"]));
        for chunk in bytes.chunks(3) {
            s.feed(chunk, &slot).unwrap();
        }
        assert_eq!(s.candidates.as_ref().unwrap().hits().len(), 2);
        s.feed(
            &wire(Response::Search {
                id: 1,
                report: Report {
                    hits: vec![named("a"), named("b")],
                    visited: 2,
                    complete: true,
                    ..Default::default()
                },
            }),
            &slot,
        )
        .unwrap();
        assert!(s.candidates.is_none());
        assert_eq!(s.finish().unwrap().hits.len(), 2);
    }
    #[test]
    fn batches_require_opt_in_version_initial_progress_and_exact_counts() {
        let slot = Mutex::new(None);
        let mut legacy = Stream::new(false);
        legacy.feed(&snapshot(Progress::default()), &slot).unwrap();
        assert!(legacy.feed(&batch(0, &["a"]), &slot).is_err());
        let mut s = Stream::new_mode(false, false, true);
        assert!(s.feed(&batch(0, &["a"]), &slot).is_err());
        let mut s = Stream::new_mode(false, false, true);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        assert!(s.feed(&batch(0, &["a"]), &slot).is_err());
        let mut s = Stream::new_mode(false, false, true);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        s.feed(
            &snapshot(Progress {
                visited: 1,
                found: 1,
                ..Default::default()
            }),
            &slot,
        )
        .unwrap();
        assert!(
            s.feed(
                &wire(Response::SearchBatch {
                    id: 1,
                    version: 2,
                    batch: files_core::search_batch::Batch {
                        offset: 0,
                        hits: vec![named("a")]
                    }
                }),
                &slot
            )
            .is_err()
        );
        let mut s = Stream::new_mode(false, false, true);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        s.feed(
            &snapshot(Progress {
                visited: 1,
                found: 1,
                ..Default::default()
            }),
            &slot,
        )
        .unwrap();
        assert!(
            s.feed(
                &wire(Response::Search {
                    id: 1,
                    report: Report {
                        hits: vec![named("a")],
                        visited: 1,
                        complete: true,
                        ..Default::default()
                    }
                }),
                &slot
            )
            .is_err()
        );
    }
    #[test]
    fn terminal_error_discards_candidates_and_never_returns_a_partial_success() {
        let slot = Mutex::new(None);
        let mut s = Stream::new_mode(false, false, true);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        s.feed(
            &snapshot(Progress {
                visited: 1,
                found: 1,
                ..Default::default()
            }),
            &slot,
        )
        .unwrap();
        s.feed(&batch(0, &["a"]), &slot).unwrap();
        assert!(!s.candidates.as_ref().unwrap().hits().is_empty());
        s.feed(
            &wire(Response::Error {
                id: 1,
                message: "root replaced".into(),
            }),
            &slot,
        )
        .unwrap();
        assert!(s.candidates.is_none());
        assert!(s.finish().is_err());
    }
    #[test]
    fn content_reply_must_match_requested_mode() {
        let slot = Mutex::new(None);
        for expected in [false, true] {
            let mut s = Stream::new(false).expect_content(expected);
            s.feed(&snapshot(Progress::default()), &slot).unwrap();
            let report = Report {
                complete: true,
                content: Some(Default::default()),
                ..Default::default()
            };
            let result = s.feed(&wire(Response::Search { id: 1, report }), &slot);
            assert_eq!(result.is_ok(), expected);
            if expected {
                assert!(s.finish().unwrap().content.is_some());
            }
        }
    }
    #[test]
    fn fragmented_progress_is_latest_only_and_final_matches_after_complete_frame() {
        let slot = Mutex::new(None);
        let mut s = Stream::new(false);
        let mut bytes = snapshot(Progress::default());
        let p = Progress {
            visited: 256,
            ..Default::default()
        };
        bytes.extend(snapshot(p));
        bytes.extend(wire(Response::Search {
            id: 1,
            report: Report {
                visited: 256,
                complete: true,
                ..Default::default()
            },
        }));
        for chunk in bytes.chunks(7) {
            s.feed(chunk, &slot).unwrap();
        }
        assert_eq!(*slot.lock().unwrap(), Some(p));
        assert!(s.line.is_empty() && s.line.capacity() == 0);
        let r = s.finish().unwrap();
        assert!(r.complete && r.visited == 256);
    }
    #[test]
    fn rejects_regression_wrong_id_missing_final_and_inconsistent_summary() {
        let slot = Mutex::new(None);
        let mut s = Stream::new(false);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        s.feed(
            &snapshot(Progress {
                visited: 3,
                ..Default::default()
            }),
            &slot,
        )
        .unwrap();
        assert!(
            s.feed(
                &snapshot(Progress {
                    visited: 2,
                    ..Default::default()
                }),
                &slot
            )
            .is_err()
        );
        let mut s = Stream::new(false);
        assert!(
            s.feed(
                &wire(Response::SearchProgress {
                    id: 2,
                    progress: Progress::default()
                }),
                &slot
            )
            .is_err()
        );
        let mut s = Stream::new(false);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        assert!(s.finish().is_err());
        let mut s = Stream::new(false);
        s.feed(&snapshot(Progress::default()), &slot).unwrap();
        assert!(
            s.feed(
                &wire(Response::Search {
                    id: 1,
                    report: Report {
                        visited: 1,
                        complete: true,
                        ..Default::default()
                    }
                }),
                &slot
            )
            .is_err()
        );
        let mut s = Stream::new(false);
        assert!(
            s.feed(
                &wire(Response::Search {
                    id: 1,
                    report: Report {
                        complete: true,
                        ..Default::default()
                    }
                }),
                &slot
            )
            .is_err()
        );
    }
    #[test]
    fn terminal_error_and_protocol_caps_are_strict() {
        let slot = Mutex::new(None);
        let mut s = Stream::new(false);
        s.feed(
            &wire(Response::Error {
                id: 1,
                message: "é".repeat(1000),
            }),
            &slot,
        )
        .unwrap();
        assert!(s.finish().unwrap_err().to_string().len() <= 512);
        let mut s = Stream::new(false);
        s.feed(
            &wire(Response::Error {
                id: 1,
                message: "root replaced".into(),
            }),
            &slot,
        )
        .unwrap();
        assert!(s.feed(b"\n", &slot).is_err());
        let mut s = Stream::new(false);
        assert!(s.feed(&vec![b' '; MAX_LINE + 1], &slot).is_err());
        assert_eq!(s.line.capacity(), 0);
        let mut s = Stream::new(false);
        for _ in 0..MAX_MESSAGES {
            s.feed(&snapshot(Progress::default()), &slot).unwrap();
        }
        assert!(s.feed(&snapshot(Progress::default()), &slot).is_err());
        let mut s = Stream::new(false);
        for _ in 0..256 {
            s.feed(&vec![b' '; 8191], &slot).unwrap();
            assert!(s.line.capacity() <= MAX_LINE);
        }
        assert!(s.finish().is_err());
    }
}
