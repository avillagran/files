//! Bounded streaming UTF8 validation and exact literal matching, no file reopen.
use files_core::search_content::{MAX_FILE, Match};
use std::{
    io::{self, Read},
    time::Instant,
};
#[cfg(unix)]
pub fn unchanged(
    revision: files_core::media_source::Revision,
    file: &std::fs::File,
    path: &std::path::Path,
) -> bool {
    revision.revalidate(file, path).is_ok()
}
pub enum Outcome {
    Text(Option<Found>),
    Binary,
    Limited,
    Time,
}
pub struct Found {
    pub line: u64,
    pub byte: u64,
    pub regex_end: Option<u64>,
    pub snippet: Option<files_core::search_snippet::Snippet>,
}
const HISTORY: usize = files_core::search_content::MAX_LITERAL + 64;
struct Context {
    history: [u8; HISTORY],
    len: usize,
    head: usize,
    raw: Vec<u8>,
    start: u64,
}
impl Context {
    fn new() -> Self {
        Self {
            history: [0; HISTORY],
            len: 0,
            head: 0,
            raw: Vec::with_capacity(files_core::search_snippet::MAX_BYTES),
            start: 0,
        }
    }
    fn remember(&mut self, bytes: &[u8]) {
        if bytes.len() >= HISTORY {
            self.history
                .copy_from_slice(&bytes[bytes.len() - HISTORY..]);
            self.len = HISTORY;
            self.head = 0;
        } else {
            for &byte in bytes {
                self.history[self.head] = byte;
                self.head += 1;
                if self.head == HISTORY {
                    self.head = 0;
                }
                self.len = (self.len + 1).min(HISTORY);
            }
        }
    }
    fn begin(&mut self, matched: Match, bytes: &[u8], offset: u64) {
        self.start = matched.byte.saturating_sub(64);
        if self.start < offset {
            let history_start = offset - self.len as u64;
            let skip = (self.start - history_start) as usize;
            let oldest = (self.head + HISTORY - self.len) % HISTORY;
            for i in skip..self.len {
                if self.raw.len() == files_core::search_snippet::MAX_BYTES {
                    break;
                }
                self.raw.push(self.history[(oldest + i) % HISTORY]);
            }
        }
        self.append(bytes, offset);
    }
    fn append(&mut self, bytes: &[u8], offset: u64) {
        let next = self.start + self.raw.len() as u64;
        let skip = next.saturating_sub(offset) as usize;
        if skip < bytes.len() {
            let n =
                (files_core::search_snippet::MAX_BYTES - self.raw.len()).min(bytes.len() - skip);
            self.raw.extend_from_slice(&bytes[skip..skip + n]);
        }
    }
    fn finish(
        self,
        matched: Match,
        needle: usize,
        total: u64,
    ) -> io::Result<files_core::search_snippet::Snippet> {
        let mut begin = 0;
        while begin < self.raw.len() && self.raw[begin] & 0xc0 == 0x80 {
            begin += 1;
        }
        let raw = &self.raw[begin..];
        let text = match std::str::from_utf8(raw) {
            Ok(s) => s,
            Err(e) if e.error_len().is_none() => {
                std::str::from_utf8(&raw[..e.valid_up_to()]).map_err(io::Error::other)?
            }
            Err(e) => return Err(io::Error::other(e)),
        };
        let start = self.start + begin as u64;
        let end = start + text.len() as u64;
        let matched_end = matched.byte + needle as u64;
        let mut normalized = String::with_capacity(text.len());
        let mut mark = None;
        let mut mark_end = 0;
        for (byte, c) in text.char_indices() {
            let source = start + byte as u64;
            if source == matched.byte {
                mark = Some(normalized.len());
            }
            normalized.push(if c.is_control() || c.is_whitespace() {
                ' '
            } else {
                c
            });
            if source + c.len_utf8() as u64 <= matched_end {
                mark_end = normalized.len();
            }
        }
        let snippet = files_core::search_snippet::Snippet {
            text: normalized,
            start: mark
                .ok_or_else(|| io::Error::other("Fragmento perdió el inicio de coincidencia"))?,
            end: mark_end,
            leading: start > 0,
            trailing: end < total,
            match_clipped: matched_end > end,
        };
        if !snippet.valid() {
            return Err(io::Error::other("Fragmento inválido"));
        }
        Ok(snippet)
    }
}
pub struct Needle<'a> {
    bytes: &'a [u8],
    failure: Vec<usize>,
    lines: u64,
}
impl<'a> Needle<'a> {
    pub fn new(text: &'a str) -> Self {
        let bytes = text.as_bytes();
        let mut failure = vec![0; bytes.len()];
        let mut k = 0;
        for i in 1..bytes.len() {
            while k > 0 && bytes[i] != bytes[k] {
                k = failure[k - 1];
            }
            if bytes[i] == bytes[k] {
                k += 1;
            }
            failure[i] = k;
        }
        Self {
            bytes,
            failure,
            lines: bytes.iter().filter(|&&b| b == b'\n').count() as u64,
        }
    }
    pub fn scan(
        &self,
        reader: &mut impl Read,
        budget: &mut u64,
        deadline: Instant,
    ) -> io::Result<(Outcome, u64)> {
        let mut buf = [0u8; 32768 + 3];
        let mut context = Context::new();
        let (mut carry, mut matched, mut prefix, mut line, mut offset) =
            (0, None::<Match>, 0, 1u64, 0u64);
        loop {
            if Instant::now() >= deadline {
                return Ok((Outcome::Time, offset));
            }
            if *budget == 0 {
                return Ok((Outcome::Limited, offset));
            }
            let count = (32768usize).min((*budget).min(MAX_FILE + 1 - offset) as usize);
            let n = match reader.read(&mut buf[carry..carry + count]) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                v => v?,
            };
            *budget -= n as u64;
            if n == 0 {
                return Ok((
                    if carry == 0 {
                        Outcome::Text(match matched {
                            Some(m) => Some(Found {
                                line: m.line,
                                byte: m.byte,
                                regex_end: None,
                                snippet: Some(context.finish(m, self.bytes.len(), offset)?),
                            }),
                            None => None,
                        })
                    } else {
                        Outcome::Binary
                    },
                    offset,
                ));
            }
            if offset + n as u64 > MAX_FILE {
                return Ok((Outcome::Limited, offset + n as u64));
            }
            let fresh = &buf[carry..carry + n];
            if fresh.contains(&0) {
                return Ok((Outcome::Binary, offset + n as u64));
            }
            let was_matched = matched.is_some();
            if matched.is_none() {
                for (i, &b) in fresh.iter().enumerate() {
                    if b == b'\n' {
                        line += 1;
                    }
                    while prefix > 0 && b != self.bytes[prefix] {
                        prefix = self.failure[prefix - 1];
                    }
                    if b == self.bytes[prefix] {
                        prefix += 1;
                    }
                    if prefix == self.bytes.len() {
                        matched = Some(Match {
                            line: line - self.lines,
                            byte: offset + i as u64 + 1 - self.bytes.len() as u64,
                            regex_end: None,
                        });
                        break;
                    }
                }
            }
            if !was_matched {
                if let Some(m) = matched {
                    context.begin(m, fresh, offset);
                } else {
                    context.remember(fresh);
                }
            } else {
                context.append(fresh, offset);
            }
            offset += n as u64;
            let end = carry + n;
            carry = match std::str::from_utf8(&buf[..end]) {
                Ok(_) => 0,
                Err(e) if e.error_len().is_none() => {
                    let left = end - e.valid_up_to();
                    buf.copy_within(e.valid_up_to()..end, 0);
                    left
                }
                Err(_) => return Ok((Outcome::Binary, offset)),
            };
        }
    }
}
pub enum Scanner<'a> {
    Literal(Needle<'a>),
    Regex(&'a files_core::search_regex::Matcher),
}
impl Scanner<'_> {
    pub fn scan(
        &self,
        reader: &mut impl Read,
        budget: &mut u64,
        deadline: Instant,
    ) -> io::Result<(Outcome, u64)> {
        match self {
            Self::Literal(needle) => needle.scan(reader, budget, deadline),
            Self::Regex(regex) => scan_regex(regex, reader, budget, deadline),
        }
    }
}
fn scan_regex(
    regex: &files_core::search_regex::Matcher,
    reader: &mut impl Read,
    budget: &mut u64,
    deadline: Instant,
) -> io::Result<(Outcome, u64)> {
    let mut text = Vec::new();
    text.try_reserve_exact(MAX_FILE as usize)
        .map_err(io::Error::other)?;
    let mut buf = [0u8; 32768];
    loop {
        if Instant::now() >= deadline {
            return Ok((Outcome::Time, text.len() as u64));
        }
        if *budget == 0 {
            return Ok((Outcome::Limited, text.len() as u64));
        }
        let count = buf
            .len()
            .min((*budget).min(MAX_FILE + 1 - text.len() as u64) as usize);
        let n = match reader.read(&mut buf[..count]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        *budget -= n as u64;
        let bytes = text.len() as u64 + n as u64;
        if bytes > MAX_FILE {
            return Ok((Outcome::Limited, bytes));
        }
        if n == 0 {
            break;
        }
        if buf[..n].contains(&0) {
            return Ok((Outcome::Binary, bytes));
        }
        text.extend_from_slice(&buf[..n]);
    }
    let bytes = text.len() as u64;
    let Ok(text) = std::str::from_utf8(&text) else {
        return Ok((Outcome::Binary, bytes));
    };
    let matched = match regex.find(text, &mut || Instant::now() < deadline) {
        Ok(m) => m,
        Err(files_core::search_regex::Error::Cancelled) => return Ok((Outcome::Time, bytes)),
        Err(error) => return Err(io::Error::other(format!("Regex: {error:?}"))),
    };
    let found = if let Some(m) = matched {
        let position = Match {
            line: m.line,
            byte: m.start as u64,
            regex_end: Some(m.end as u64),
        };
        let snippet = if m.start == m.end {
            None
        } else {
            let mut context = Context::new();
            context.begin(position, text.as_bytes(), 0);
            Some(context.finish(position, m.end - m.start, bytes)?)
        };
        Some(Found {
            line: m.line,
            byte: m.start as u64,
            regex_end: Some(m.end as u64),
            snippet,
        })
    } else {
        None
    };
    if Instant::now() >= deadline {
        return Ok((Outcome::Time, bytes));
    }
    Ok((Outcome::Text(found), bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run(bytes: &[u8], text: &str) -> Outcome {
        let mut budget = 64 * 1024 * 1024;
        Needle::new(text)
            .scan(
                &mut &bytes[..],
                &mut budget,
                Instant::now() + std::time::Duration::from_secs(1),
            )
            .unwrap()
            .0
    }
    fn regex_run(pattern: &str, bytes: &[u8], mut budget: u64) -> (Outcome, u64) {
        let regex = files_core::search_regex::Matcher::compile(pattern, &mut || true).unwrap();
        scan_regex(
            &regex,
            &mut &bytes[..],
            &mut budget,
            Instant::now() + std::time::Duration::from_secs(2),
        )
        .unwrap()
    }
    #[test]
    fn regex_validates_complete_utf8_and_handles_chunk_crossing_spans_and_empty_eof() {
        let mut text = vec![b'x'; 32767];
        text.extend_from_slice("á\n日本\nend42".as_bytes());
        let (Outcome::Text(Some(m)), bytes) = regex_run(r"(?s)á.*end[0-9]+", &text, MAX_FILE * 2)
        else {
            panic!()
        };
        assert_eq!(bytes, text.len() as u64);
        assert_eq!(m.byte, 32767);
        assert_eq!(m.regex_end, Some(bytes));
        assert!(m.snippet.unwrap().valid());
        for data in [b"".as_slice(), "á\n".as_bytes()] {
            let (Outcome::Text(Some(m)), bytes) = regex_run(r"\z", data, MAX_FILE * 2) else {
                panic!()
            };
            assert_eq!(m.byte, bytes);
            assert_eq!(m.regex_end, Some(bytes));
            assert!(m.snippet.is_none());
        }
        for data in [b"hit\0".as_slice(), b"hit\xff", b"hit\xc3"] {
            assert!(matches!(
                regex_run("hit", data, MAX_FILE * 2).0,
                Outcome::Binary
            ));
        }
    }
    #[test]
    fn regex_limits_deadlines_and_full_validation_do_not_report_false_absence() {
        assert!(matches!(
            regex_run("hit", b"hit and more", 3).0,
            Outcome::Limited
        ));
        assert!(matches!(
            regex_run("x", &vec![b'x'; MAX_FILE as usize + 1], MAX_FILE * 2).0,
            Outcome::Limited
        ));
        let regex = files_core::search_regex::Matcher::compile("x", &mut || true).unwrap();
        assert!(matches!(
            scan_regex(&regex, &mut b"x".as_slice(), &mut 20, Instant::now())
                .unwrap()
                .0,
            Outcome::Time
        ));
        let (Outcome::Text(Some(m)), n) =
            regex_run(r"\z", &vec![b'x'; MAX_FILE as usize], MAX_FILE * 2)
        else {
            panic!()
        };
        assert_eq!(m.byte, MAX_FILE);
        assert_eq!(n, MAX_FILE);
    }
    #[test]
    fn exact_match_line_unicode_cross_chunk_and_overlapping_prefix() {
        let mut data = vec![b'x'; 32767];
        data.extend_from_slice("á\nabababaca".as_bytes());
        let Outcome::Text(Some(m)) = run(&data, "á\naba") else {
            panic!()
        };
        assert_eq!(m.line, 1);
        assert_eq!(m.byte, 32767);
        let Outcome::Text(Some(m)) = run(&data, "ababaca") else {
            panic!()
        };
        assert_eq!(m.line, 2);
        assert_eq!(m.byte, 32772);
        assert!(matches!(run(b"Case", "case"), Outcome::Text(None)));
    }
    #[test]
    fn prefix_match_never_accepts_later_binary_invalid_utf8_or_file_limit() {
        for data in [
            b"needle\0".as_slice(),
            b"needle\xff".as_slice(),
            b"needle\xc3".as_slice(),
        ] {
            assert!(matches!(run(data, "needle"), Outcome::Binary));
        }
        let data = vec![b'x'; MAX_FILE as usize + 1];
        assert!(matches!(run(&data, "x"), Outcome::Limited));
    }
    #[test]
    fn total_budget_deadline_and_empty_file_are_explicit() {
        let n = Needle::new("x");
        let now = Instant::now();
        let mut budget = 2;
        let (result, bytes) = n
            .scan(
                &mut b"xxxx".as_slice(),
                &mut budget,
                now + std::time::Duration::from_secs(1),
            )
            .unwrap();
        assert!(matches!(result, Outcome::Limited));
        assert_eq!((bytes, budget), (2, 0));
        assert!(matches!(
            n.scan(&mut b"x".as_slice(), &mut budget, now).unwrap().0,
            Outcome::Time
        ));
        assert!(matches!(run(b"", "x"), Outcome::Text(None)));
    }
    #[test]
    fn snippet_crosses_utf8_chunks_normalizes_controls_and_marks_exact_text() {
        let mut data = vec![b'x'; 32767];
        data.extend_from_slice("á\nneedle\tfin\r\n".as_bytes());
        let Outcome::Text(Some(found)) = run(&data, "á\nneedle") else {
            panic!()
        };
        let s = found.snippet.unwrap();
        assert!(s.valid());
        assert!(s.leading && !s.trailing && !s.match_clipped);
        assert_eq!(&s.text[s.start..s.end], "á needle");
        assert!(s.text.ends_with(" fin  "));
        assert_eq!(found.byte, 32767);
    }
    #[test]
    fn long_literal_is_matched_fully_even_when_display_fragment_is_clipped() {
        let text = "ñ".repeat(512);
        let Outcome::Text(Some(found)) = run(text.as_bytes(), &text) else {
            panic!()
        };
        let s = found.snippet.unwrap();
        assert!(s.valid() && s.match_clipped && s.trailing && !s.leading);
        assert_eq!(s.start, 0);
        assert_eq!(s.end, s.text.len());
        assert!(s.text.len() <= 256);
    }
    #[test]
    fn one_byte_reads_keep_prefix_context_and_first_match_only() {
        struct Tiny<'a>(&'a [u8]);
        impl Read for Tiny<'_> {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.0.is_empty() {
                    return Ok(0);
                }
                out[0] = self.0[0];
                self.0 = &self.0[1..];
                Ok(1)
            }
        }
        let bytes = "previo @ á\naguja después aguja".as_bytes();
        let mut budget = MAX_FILE;
        let (outcome, _) = Needle::new("aguja")
            .scan(
                &mut Tiny(bytes),
                &mut budget,
                Instant::now() + std::time::Duration::from_secs(1),
            )
            .unwrap();
        let Outcome::Text(Some(found)) = outcome else {
            panic!()
        };
        let s = found.snippet.unwrap();
        assert!(s.valid());
        assert_eq!(&s.text[s.start..s.end], "aguja");
        assert!(s.text.contains("después aguja"));
        assert_eq!(found.line, 2);
        assert!(!s.leading && !s.trailing);
    }
}
