//! One bounded page from a retained walk; paths remain observations, not mutation proofs.
use crate::search::{self, Report};
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub version: u8,
    pub id: u64,
    pub sequence: u64,
    pub found_total: u64,
    pub more: bool,
    pub report: Report,
}
#[derive(Serialize)]
pub struct PageRef<'a> {
    pub version: u8,
    pub id: u64,
    pub sequence: u64,
    pub found_total: u64,
    pub more: bool,
    pub report: &'a Report,
}
/// The sender retains the borrowed page until its exact consumer credit arrives.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Next {
    pub version: u8,
    pub id: u64,
    pub sequence: u64,
}
impl Next {
    pub fn read(
        input: &mut impl Read,
        id: u64,
        sequence: u64,
        decode: impl FnOnce(&[u8]) -> io::Result<Self>,
    ) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(256);
        loop {
            let mut byte = [0];
            input.read_exact(&mut byte)?;
            if byte[0] == b'\n' {
                break;
            }
            if bytes.len() == 256 {
                return Err(io::Error::other("Crédito de búsqueda demasiado grande"));
            }
            bytes.push(byte[0]);
        }
        let credit = decode(&bytes)?;
        if credit.version != 1 || credit.id != id || credit.sequence != sequence {
            return Err(io::Error::other("Crédito de página obsoleto o inválido"));
        }
        Ok(())
    }
}
pub struct Ledger {
    id: u64,
    next: u64,
    found: u64,
    visited: u64,
    errors: u64,
    mounts: u64,
    metadata: u64,
    terminal: bool,
    wants_metadata: bool,
    regex: bool,
    content: Option<crate::search_content::Stats>,
}
impl Ledger {
    pub fn new(id: u64, metadata: bool, content: bool) -> Self {
        Self {
            id,
            next: 0,
            found: 0,
            visited: 0,
            errors: 0,
            mounts: 0,
            metadata: 0,
            terminal: false,
            wants_metadata: metadata,
            regex: false,
            content: content.then(Default::default),
        }
    }

    pub fn expect_regex(mut self, regex: bool) -> Self {
        self.regex = regex;
        self
    }

    pub fn accept(&mut self, page: &Page) -> io::Result<()> {
        search::validate(&page.report)?;
        let r = &page.report;
        search::Progress::from_report(r).validate_after(Default::default())?;
        let content_valid = match (self.content, r.content) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                b.regex == self.regex
                    && b.files >= a.files
                    && b.bytes >= a.bytes
                    && b.binary >= a.binary
                    && b.limited >= a.limited
                    && b.changed >= a.changed
            }
            _ => false,
        };

        if self.terminal
            || !content_valid
            || r.metadata_requested != self.wants_metadata
            || page.version != 1
            || page.id != self.id
            || page.sequence != self.next
            || self.found.checked_add(r.hits.len() as u64) != Some(page.found_total)
            || page.found_total > r.visited
            || r.visited < self.visited
            || r.errors < self.errors
            || r.mounts_excluded < self.mounts
            || r.metadata_reads < self.metadata
            || (page.more && (r.hits.is_empty() || r.complete))
        {
            return Err(io::Error::other("Página de búsqueda incoherente"));
        }
        self.content = r.content;
        self.next += 1;
        self.found = page.found_total;
        self.visited = r.visited;
        self.errors = r.errors;
        self.mounts = r.mounts_excluded;
        self.metadata = r.metadata_reads;
        self.terminal = !page.more;
        Ok(())
    }
}

/// Incremental one-page slot. The transport supplies JSON decoding; no runtime
/// codec dependency or unbounded stream collection is required here.
pub struct Reader {
    line: Vec<u8>,
    ledger: Ledger,
    pending: Option<Page>,
    awaiting: Option<u64>,
    terminal: bool,
    failed: bool,
}
impl Reader {
    pub const MAX_LINE: usize = 2 * 1024 * 1024;
    pub fn new(id: u64, metadata: bool, content: bool) -> Self {
        Self {
            line: Vec::new(),
            ledger: Ledger::new(id, metadata, content),
            pending: None,
            awaiting: None,
            terminal: false,
            failed: false,
        }
    }
    pub fn expect_regex(mut self, regex: bool) -> Self {
        self.ledger = self.ledger.expect_regex(regex);
        self
    }
    pub fn feed(
        &mut self,
        bytes: &[u8],
        mut decode: impl FnMut(&[u8]) -> io::Result<Page>,
    ) -> io::Result<()> {
        let result = (|| {
            if self.failed {
                return Err(io::Error::other("Lector de páginas inválido"));
            }
            for part in bytes.split_inclusive(|b| *b == b'\n') {
                if self.awaiting.is_some() || self.pending.is_some() || self.terminal {
                    return Err(io::Error::other("Página sin crédito o después del final"));
                }
                let len = self
                    .line
                    .len()
                    .checked_add(part.len())
                    .filter(|&n| n <= Self::MAX_LINE)
                    .ok_or_else(|| io::Error::other("Página JSON superior a2MiB"))?;
                if len > self.line.capacity() {
                    let target = self
                        .line
                        .capacity()
                        .saturating_mul(2)
                        .max(len)
                        .min(Self::MAX_LINE);
                    self.line
                        .try_reserve_exact(target - self.line.len())
                        .map_err(io::Error::other)?;
                }
                self.line.extend_from_slice(part);
                if part.last() == Some(&b'\n') {
                    let page = decode(&self.line[..self.line.len() - 1])?;
                    self.ledger.accept(&page)?;
                    self.awaiting = page.more.then_some(page.sequence);
                    self.terminal = !page.more;
                    self.pending = Some(page);
                    self.line.clear();
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
            self.pending = None;
            self.line = Vec::new();
        }
        result
    }
    pub fn take(&mut self) -> Option<Page> {
        if self.failed {
            None
        } else {
            self.pending.take()
        }
    }
    pub fn waiting(&self) -> Option<u64> {
        if self.failed { None } else { self.awaiting }
    }
    pub fn credit(&mut self, sequence: u64) -> io::Result<()> {
        if self.failed || self.pending.is_some() || self.awaiting != Some(sequence) || self.terminal
        {
            return Err(io::Error::other("Crédito obsoleto o página sin consumir"));
        }
        self.awaiting = None;
        Ok(())
    }
    pub fn finish(&self) -> io::Result<()> {
        if self.failed || !self.terminal || !self.line.is_empty() {
            Err(io::Error::other("Búsqueda paginada sin final válido"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(sequence: u64, found: u64, more: bool) -> Page {
        Page {
            version: 1,
            id: 7,
            sequence,
            found_total: found,
            more,
            report: Report {
                hits: if more {
                    vec![search::Hit {
                        relative: format!("{sequence}").into(),
                        kind: crate::listing::Kind::File,
                        content: None,
                        snippet: None,
                    }]
                } else {
                    vec![]
                },
                visited: found,
                complete: !more,
                reason: more.then(|| "Page boundary".into()),
                ..Default::default()
            },
        }
    }
    fn wire(page: &Page) -> Vec<u8> {
        let mut b = serde_json::to_vec(page).unwrap();
        b.push(b'\n');
        b
    }
    fn decode(bytes: &[u8]) -> io::Result<Page> {
        serde_json::from_slice(bytes).map_err(io::Error::other)
    }
    #[test]
    fn paged_regex_zero_width_requires_expected_mode_through_terminal_page() {
        let make = || Page {
            version: 1,
            id: 7,
            sequence: 0,
            found_total: 1,
            more: false,
            report: search::Report {
                hits: vec![search::Hit {
                    relative: "empty".into(),
                    kind: crate::listing::Kind::File,
                    content: Some(crate::search_content::Match {
                        line: 1,
                        byte: 0,
                        regex_end: Some(0),
                    }),
                    snippet: None,
                }],
                visited: 1,
                content: Some(crate::search_content::Stats {
                    regex: true,
                    files: 1,
                    ..Default::default()
                }),
                complete: true,
                ..Default::default()
            },
        };
        assert!(Ledger::new(7, false, true).accept(&make()).is_err());
        Ledger::new(7, false, true)
            .expect_regex(true)
            .accept(&make())
            .unwrap();
        let mut wire = serde_json::to_vec(&make()).unwrap();
        wire.push(b'\n');
        let mut reader = Reader::new(7, false, true).expect_regex(true);
        reader
            .feed(&wire, |bytes| {
                serde_json::from_slice(bytes).map_err(io::Error::other)
            })
            .unwrap();
        assert!(reader.finish().is_ok());
    }
    #[test]
    fn page_reader_fragments_exact_credit_and_explicit_terminal() {
        let mut reader = Reader::new(7, false, false);
        let bytes = wire(&page(0, 1, true));
        for byte in bytes {
            reader.feed(&[byte], decode).unwrap();
        }
        assert_eq!(reader.waiting(), Some(0));
        assert!(reader.credit(0).is_err());
        assert_eq!(reader.take().unwrap().sequence, 0);
        assert!(reader.credit(1).is_err());
        assert_eq!(reader.waiting(), Some(0));
        reader.credit(0).unwrap();
        assert!(reader.credit(0).is_err());
        reader.feed(&wire(&page(1, 1, false)), decode).unwrap();
        reader.finish().unwrap();
        assert!(reader.take().unwrap().report.hits.is_empty());
        assert!(reader.credit(1).is_err());
        assert!(reader.feed(b"extra", decode).is_err());
    }
    #[test]
    fn page_reader_rejects_unsolicited_duplicate_and_missing_terminal_stickily() {
        let mut r = Reader::new(7, false, false);
        r.feed(&wire(&page(0, 1, true)), decode).unwrap();
        r.take();
        assert!(r.finish().is_err());
        assert!(r.feed(&wire(&page(1, 2, true)), decode).is_err());
        assert!(r.take().is_none() && r.feed(b"", decode).is_err());
        let mut r = Reader::new(7, false, false);
        r.feed(b"{", decode).unwrap();
        assert!(r.finish().is_err());
        assert!(r.feed(&vec![b'x'; Reader::MAX_LINE], decode).is_err());
    }
    #[test]
    fn exact_sequence_counts_and_terminal_empty_are_transactional() {
        let mut ledger = Ledger::new(7, false, false);
        let first = page(0, 1, true);
        ledger.accept(&first).unwrap();
        assert!(ledger.accept(&first).is_err());
        let mut wrong = page(1, 2, true);
        wrong.id = 8;
        assert!(ledger.accept(&wrong).is_err());
        assert!(ledger.accept(&page(1, 3, true)).is_err());
        ledger.accept(&page(1, 2, true)).unwrap();
        ledger.accept(&page(2, 2, false)).unwrap();
        assert!(ledger.accept(&page(3, 2, false)).is_err());
    }
    #[test]
    fn counter_regression_mode_changes_and_false_complete_more_reject() {
        let mut ledger = Ledger::new(7, true, false);
        let mut p = page(0, 1, true);
        p.report.metadata_requested = true;
        p.report.metadata_reads = 1;
        ledger.accept(&p).unwrap();
        for mode in 0..4 {
            let mut next = page(1, 2, true);
            next.report.metadata_requested = true;
            next.report.metadata_reads = 1;
            match mode {
                0 => next.report.metadata_reads = 0,
                1 => next.report.metadata_requested = false,
                2 => {
                    next.report.complete = true;
                    next.report.reason = None
                }
                _ => next.report.content = Some(Default::default()),
            }
            assert!(ledger.accept(&next).is_err());
        }
        let mut next = page(1, 2, true);
        next.report.metadata_requested = true;
        next.report.metadata_reads = 2;
        ledger.accept(&next).unwrap();
    }
    #[test]
    fn borrowed_frame_matches_owned_wire_and_credit_is_bounded_exact_and_does_not_prefetch() {
        let p = page(0, 1, true);
        let raw = serde_json::to_vec(&PageRef {
            version: 1,
            id: 7,
            sequence: 0,
            found_total: 1,
            more: true,
            report: &p.report,
        })
        .unwrap();
        let decoded: Page = serde_json::from_slice(&raw).unwrap();
        Ledger::new(7, false, false).accept(&decoded).unwrap();
        let credit = b"{\"version\":1,\"id\":7,\"sequence\":0}\nTAIL";
        let mut input = &credit[..];
        Next::read(&mut input, 7, 0, |bytes| {
            serde_json::from_slice(bytes).map_err(io::Error::other)
        })
        .unwrap();
        assert_eq!(input, b"TAIL");
        for invalid in [
            b"{\"version\":1,\"id\":8,\"sequence\":0}\n".as_slice(),
            b"{\"version\":1,\"id\":7,\"sequence\":1}\n",
            b"{}\n",
            b"{\"version\":1,\"id\":7,\"sequence\":0,\"extra\":1}\n",
        ] {
            assert!(
                Next::read(&mut &invalid[..], 7, 0, |b| serde_json::from_slice(b)
                    .map_err(io::Error::other))
                .is_err()
            );
        }
        assert!(
            Next::read(&mut &vec![b'x'; 257][..], 7, 0, |_| panic!(
                "oversize decoded"
            ))
            .is_err()
        );
        assert!(Next::read(&mut &b""[..], 7, 0, |_| panic!("EOF decoded")).is_err());
    }
}
