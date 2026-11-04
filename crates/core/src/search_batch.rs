//! Bounded discovery-order candidates. Only exact terminal reconciliation accepts a result.
use crate::{
    listing::Kind,
    search::{self, Hit, Report},
};
use serde::{Deserialize, Serialize};
use std::{io, path::Component};
pub const MAX_BATCH: usize = 32;
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub offset: usize,
    #[serde(deserialize_with = "search::bounded_batch_hits")]
    pub hits: Vec<Hit>,
}
/// Serialize the same payload directly from a scanner prefix, without path clones.
#[derive(Serialize)]
pub struct Borrowed<'a> {
    pub offset: usize,
    pub hits: &'a [Hit],
}
#[derive(Serialize)]
pub struct Frame<'a> {
    status: &'static str,
    id: u64,
    version: u8,
    batch: Borrowed<'a>,
}
impl<'a> Frame<'a> {
    pub fn new(id: u64, offset: usize, hits: &'a [Hit]) -> Self {
        Self {
            status: "search_batch",
            id,
            version: 1,
            batch: Borrowed { offset, hits },
        }
    }
}
pub struct Candidates {
    content: bool,
    regex: bool,
    paths: usize,
    hits: Vec<Hit>,
}
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Lote de búsqueda inválido o fuera de presupuesto",
    )
}
impl Candidates {
    pub fn new(content: bool) -> Self {
        Self {
            content,
            regex: false,
            paths: 0,
            hits: Vec::new(),
        }
    }
    pub fn expect_regex(mut self, regex: bool) -> Self {
        self.regex = regex;
        self
    }
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
    pub fn push(&mut self, batch: Batch) -> io::Result<()> {
        if batch.offset != self.hits.len()
            || batch.hits.is_empty()
            || batch.hits.len() > MAX_BATCH
            || self
                .hits
                .len()
                .checked_add(batch.hits.len())
                .is_none_or(|n| n > search::MAX_RESULTS)
        {
            return Err(invalid());
        }
        let mut paths = self.paths;
        for (index, hit) in batch.hits.iter().enumerate() {
            let raw = hit.relative.as_os_str().as_encoded_bytes();
            if hit
                .snippet
                .as_ref()
                .is_some_and(|s| hit.content.is_none() || !s.valid())
                || raw.is_empty()
                || raw.len() > 4096
                || raw.contains(&0)
                || hit
                    .relative
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                || hit.content.is_some() != self.content
                || hit.content.is_some_and(|m| {
                    hit.kind != Kind::File
                        || !m.valid(self.regex, crate::search_content::MAX_FILE)
                        || (m.zero_width() && hit.snippet.is_some())
                })
                || self.hits.iter().any(|h| h.relative == hit.relative)
                || batch.hits[..index]
                    .iter()
                    .any(|h| h.relative == hit.relative)
            {
                return Err(invalid());
            }
            paths = paths
                .checked_add(hit.relative.capacity())
                .filter(|&n| n <= search::MAX_PATH_BYTES)
                .ok_or_else(invalid)?;
        }
        // Validate the entire batch before changing visible candidate state.
        self.hits
            .try_reserve_exact(batch.hits.len())
            .map_err(io::Error::other)?;
        self.hits.extend(batch.hits);
        self.paths = paths;
        Ok(())
    }
    /// Final sorting may differ from discovery order. Compare every field/byte,
    /// never only a digest or count. Consumes the provisional owner even on error.
    pub fn reconcile(mut self, report: &Report) -> io::Result<()> {
        search::validate(report)?;
        if report.content.is_some() != self.content
            || report.content.is_some_and(|s| s.regex != self.regex)
            || report.hits.len() != self.hits.len()
            || report.hits.len() as u64 > report.visited
        {
            return Err(invalid());
        }
        self.hits
            .sort_unstable_by(|a, b| a.relative.cmp(&b.relative));
        let mut final_order: Vec<_> = report.hits.iter().collect();
        final_order.sort_unstable_by(|a, b| a.relative.cmp(&b.relative));
        if self.hits.iter().zip(final_order).any(|(a, b)| a != b) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hit(name: &str) -> Hit {
        Hit {
            relative: name.into(),
            kind: Kind::File,
            content: None,
            snippet: None,
        }
    }
    fn batch(offset: usize, names: &[&str]) -> Batch {
        Batch {
            offset,
            hits: names.iter().map(|n| hit(n)).collect(),
        }
    }
    #[test]
    fn regex_candidates_require_opt_in_and_reconcile_exact_zero_width_span() {
        let mut h = hit("empty");
        h.content = Some(crate::search_content::Match {
            line: 1,
            byte: 0,
            regex_end: Some(0),
        });
        let b = || Batch {
            offset: 0,
            hits: vec![h.clone()],
        };
        let mut literal = Candidates::new(true);
        assert!(literal.push(b()).is_err());
        assert!(literal.hits().is_empty());
        let mut regex = Candidates::new(true).expect_regex(true);
        regex.push(b()).unwrap();
        let mut report = Report {
            hits: vec![h],
            visited: 1,
            content: Some(crate::search_content::Stats {
                regex: true,
                files: 1,
                ..Default::default()
            }),
            complete: true,
            ..Default::default()
        };
        regex.reconcile(&report).unwrap();
        let mut mismatch = Candidates::new(true).expect_regex(true);
        mismatch
            .push(Batch {
                offset: 0,
                hits: report.hits.clone(),
            })
            .unwrap();
        report.content.as_mut().unwrap().regex = false;
        assert!(mismatch.reconcile(&report).is_err());
    }
    #[test]
    fn offsets_duplicates_and_malformed_batch_leave_previous_candidates_intact() {
        let mut c = Candidates::new(false);
        c.push(batch(0, &["b"])).unwrap();
        for b in [
            batch(0, &["a"]),
            batch(1, &["a", "a"]),
            batch(1, &["ok", "../escape"]),
            batch(1, &["b"]),
            batch(1, &[]),
        ] {
            assert!(c.push(b).is_err());
            assert_eq!(c.hits(), [hit("b")]);
        }
        c.push(batch(1, &["a"])).unwrap();
        c.reconcile(&Report {
            hits: vec![hit("a"), hit("b")],
            visited: 2,
            complete: true,
            ..Default::default()
        })
        .unwrap();
    }
    #[test]
    fn terminal_same_count_different_path_or_kind_is_rejected() {
        for different in [
            hit("other"),
            Hit {
                kind: Kind::Symlink,
                ..hit("a")
            },
        ] {
            let mut c = Candidates::new(false);
            c.push(batch(0, &["a"])).unwrap();
            assert!(
                c.reconcile(&Report {
                    hits: vec![different],
                    visited: 1,
                    complete: true,
                    ..Default::default()
                })
                .is_err()
            );
        }
    }
    #[test]
    fn serde_rejects_33rd_hit_and_unknown_fields() {
        let b = Batch {
            offset: 0,
            hits: (0..33).map(|i| hit(&i.to_string())).collect(),
        };
        assert!(serde_json::from_slice::<Batch>(&serde_json::to_vec(&b).unwrap()).is_err());
        assert!(serde_json::from_str::<Batch>(r#"{"offset":0,"hits":[],"extra":true}"#).is_err());
    }
    #[test]
    fn cumulative_limit_and_content_mode_are_not_inferred_from_first_hit() {
        let mut c = Candidates::new(false);
        for offset in (0..480).step_by(32) {
            c.push(Batch {
                offset,
                hits: (offset..offset + 32).map(|i| hit(&i.to_string())).collect(),
            })
            .unwrap();
        }
        c.push(Batch {
            offset: 480,
            hits: (480..500).map(|i| hit(&i.to_string())).collect(),
        })
        .unwrap();
        assert!(c.push(batch(500, &["extra"])).is_err());
        assert_eq!(c.hits().len(), 500);
        let mut c = Candidates::new(true);
        assert!(c.push(batch(0, &["a"])).is_err());
        assert!(c.hits().is_empty());
    }
    #[test]
    fn cumulative_path_capacity_rejects_whole_batch_before_publication() {
        let mut c = Candidates::new(false);
        let mut index = 0;
        while index < search::MAX_RESULTS {
            let offset = c.hits().len();
            let hits = (index..index + MAX_BATCH)
                .map(|n| hit(&format!("{n:03}{}", "a".repeat(1000))))
                .collect();
            let before: Vec<_> = c.hits().iter().map(|h| h.relative.clone()).collect();
            if c.push(Batch { offset, hits }).is_err() {
                assert_eq!(
                    c.hits()
                        .iter()
                        .map(|h| h.relative.clone())
                        .collect::<Vec<_>>(),
                    before
                );
                assert!(offset < search::MAX_RESULTS);
                return;
            }
            index += MAX_BATCH;
        }
        panic!("path cap was not applied");
    }
    #[test]
    #[cfg(unix)]
    fn raw_wire_roundtrip_and_content_position_must_match_exactly() {
        use std::os::unix::ffi::OsStringExt;
        let raw = std::ffi::OsString::from_vec(b"dir/raw-\xff".to_vec());
        let b = Batch {
            offset: 0,
            hits: vec![Hit {
                relative: raw.clone().into(),
                ..hit("unused")
            }],
        };
        let wire = serde_json::to_vec(&b).unwrap();
        let b: Batch = serde_json::from_slice(&wire).unwrap();
        assert_eq!(b.hits[0].relative.as_os_str(), raw);
        let mut c = Candidates::new(false);
        c.push(b).unwrap();
        c.reconcile(&Report {
            hits: vec![Hit {
                relative: raw.into(),
                ..hit("unused")
            }],
            visited: 1,
            complete: true,
            ..Default::default()
        })
        .unwrap();
        let mut c = Candidates::new(true);
        let mut h = hit("text");
        h.content = Some(crate::search_content::Match {
            line: 1,
            byte: 2,
            regex_end: None,
        });
        c.push(Batch {
            offset: 0,
            hits: vec![h],
        })
        .unwrap();
        let mut h = hit("text");
        h.content = Some(crate::search_content::Match {
            line: 1,
            byte: 3,
            regex_end: None,
        });
        assert!(
            c.reconcile(&Report {
                hits: vec![h],
                visited: 1,
                content: Some(crate::search_content::Stats {
                    files: 1,
                    bytes: 10,
                    ..Default::default()
                }),
                complete: true,
                ..Default::default()
            })
            .is_err()
        );
    }
}
