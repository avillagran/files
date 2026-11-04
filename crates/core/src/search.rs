//! Bounded filename search wire. Results are observations, never mutation proofs.
use crate::listing::Kind;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, PathBuf},
};

pub const MAX_RESULTS: usize = 500;
pub const MAX_PATH_BYTES: usize = 256 * 1024;
pub const MAX_VISITS: u64 = 250_000;
/// Provisional counters only: no paths or executable result authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub visited: u64,
    pub found: u64,
    pub errors: u64,
    pub mounts_excluded: u64,
    pub metadata_reads: u64,
}
impl Progress {
    pub fn from_report(r: &Report) -> Self {
        Self {
            visited: r.visited,
            found: r.hits.len() as u64,
            errors: r.errors,
            mounts_excluded: r.mounts_excluded,
            metadata_reads: r.metadata_reads,
        }
    }
    pub fn validate_after(self, previous: Self) -> io::Result<()> {
        if self.visited > MAX_VISITS
            || self.found > MAX_RESULTS as u64
            || self.found > self.visited
            || self.errors > MAX_VISITS * 2
            || self.mounts_excluded > self.visited
            || self.metadata_reads > self.visited
            || self.visited < previous.visited
            || self.found < previous.found
            || self.errors < previous.errors
            || self.mounts_excluded < previous.mounts_excluded
            || self.metadata_reads < previous.metadata_reads
        {
            return Err(io::Error::other("Contadores de búsqueda inválidos"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    #[serde(with = "crate::protocol::path_wire")]
    pub relative: PathBuf,
    pub kind: Kind,
    #[serde(default)]
    pub content: Option<crate::search_content::Match>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<crate::search_snippet::Snippet>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Report {
    #[serde(deserialize_with = "bounded_hits")]
    pub hits: Vec<Hit>,
    pub visited: u64,
    #[serde(default)]
    pub content: Option<crate::search_content::Stats>,
    #[serde(default)]
    pub metadata_reads: u64,
    #[serde(default)]
    pub metadata_requested: bool,
    pub errors: u64,
    pub mounts_excluded: u64,
    pub complete: bool,
    pub reason: Option<String>,
}
fn bounded_hits<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Hit>, D::Error> {
    bounded_hits_limit::<D, MAX_RESULTS>(d)
}
pub(crate) fn bounded_batch_hits<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Hit>, D::Error> {
    bounded_hits_limit::<D, 32>(d)
}
fn bounded_hits_limit<'de, D: serde::Deserializer<'de>, const N: usize>(
    d: D,
) -> Result<Vec<Hit>, D::Error> {
    struct Visitor<const N: usize>;
    impl<'de, const N: usize> serde::de::Visitor<'de> for Visitor<N> {
        type Value = Vec<Hit>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "hasta {N} resultados acotados")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let mut hits = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(N));
            let mut bytes = 0usize;
            loop {
                if hits.len() == N {
                    if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                        return Err(A::Error::custom("Demasiados resultados"));
                    }
                    break;
                }
                let Some(hit) = seq.next_element::<Hit>()? else {
                    break;
                };
                bytes = bytes.saturating_add(hit.relative.capacity());
                if bytes > MAX_PATH_BYTES {
                    return Err(A::Error::custom("Resultados exceden presupuesto"));
                }
                hits.push(hit);
            }
            Ok(hits)
        }
    }
    d.deserialize_seq(Visitor::<N>)
}
pub fn validate(report: &Report) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Resultados de búsqueda inválidos o fuera del presupuesto",
        )
    };
    if report.hits.len() > MAX_RESULTS
        || report.visited > MAX_VISITS
        || report.metadata_reads > report.visited
        || (!report.metadata_requested && report.metadata_reads != 0)
        || report.reason.as_ref().is_some_and(|s| s.len() > 512)
        || (report.complete && (report.errors != 0 || report.reason.is_some()))
        || (!report.complete && report.reason.is_none())
        || report
            .hits
            .iter()
            .map(|h| h.relative.capacity())
            .sum::<usize>()
            > MAX_PATH_BYTES
    {
        return Err(invalid());
    }
    if let Some(s) = report.content
        && ((s.files == 0 && s.bytes != 0)
            || report.hits.len() as u64 > s.files
            || s.files > report.visited
            || s.bytes > crate::search_content::MAX_TOTAL
            || s.binary.saturating_add(s.limited).saturating_add(s.changed) > s.files
            || (report.complete && (s.limited > 0 || s.changed > 0)))
    {
        return Err(invalid());
    }
    for h in &report.hits {
        if h.snippet
            .as_ref()
            .is_some_and(|s| h.content.is_none() || !s.valid())
            || (report.content.is_some() != h.content.is_some())
            || h.content.is_some_and(|m| {
                h.kind != Kind::File
                    || !m.valid(
                        report.content.is_some_and(|s| s.regex),
                        report.content.map_or(0, |s| s.bytes),
                    )
                    || (m.zero_width() && h.snippet.is_some())
            })
        {
            return Err(invalid());
        }
        let raw = h.relative.as_os_str().as_encoded_bytes();
        if raw.is_empty()
            || raw.len() > 4096
            || raw.contains(&0)
            || h.relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regex_empty_eof_spans_are_mode_bound_and_literal_bounds_stay_strict() {
        use crate::search_content::{MAX_FILE, Match, Stats};
        let mut report = Report {
            hits: vec![Hit {
                relative: "empty".into(),
                kind: Kind::File,
                content: Some(Match {
                    line: 1,
                    byte: 0,
                    regex_end: Some(0),
                }),
                snippet: None,
            }],
            visited: 1,
            content: Some(Stats {
                regex: true,
                files: 1,
                ..Default::default()
            }),
            complete: true,
            ..Default::default()
        };
        validate(&report).unwrap();
        let wire = serde_json::to_vec(&report).unwrap();
        validate(&serde_json::from_slice(&wire).unwrap()).unwrap();
        report.content.as_mut().unwrap().regex = false;
        assert!(validate(&report).is_err());
        report.hits[0].content.as_mut().unwrap().regex_end = None;
        assert!(validate(&report).is_err()); // empty literal cannot match
        report.content.as_mut().unwrap().regex = true;
        assert!(validate(&report).is_err()); // missing regex span
        report.content.as_mut().unwrap().bytes = MAX_FILE;
        report.hits[0].content = Some(Match {
            line: MAX_FILE + 1,
            byte: MAX_FILE,
            regex_end: Some(MAX_FILE),
        });
        validate(&report).unwrap();
        report.hits[0].content.as_mut().unwrap().regex_end = Some(MAX_FILE + 1);
        assert!(validate(&report).is_err());
        report.hits[0].content.as_mut().unwrap().regex_end = Some(MAX_FILE - 1);
        assert!(validate(&report).is_err());
        report.hits[0].content = Some(Match {
            line: 1,
            byte: 0,
            regex_end: Some(0),
        });
        report.hits[0].snippet = Some(crate::search_snippet::Snippet {
            text: "a".into(),
            start: 0,
            end: 1,
            leading: false,
            trailing: false,
            match_clipped: false,
        });
        assert!(validate(&report).is_err()); // no invented highlighted character
    }
    #[test]
    fn legacy_literal_wire_omits_regex_extensions_and_roundtrips_strictly() {
        let m = crate::search_content::Match {
            line: 1,
            byte: 0,
            regex_end: None,
        };
        assert_eq!(
            serde_json::to_value(m).unwrap(),
            serde_json::json!({"line":1,"byte":0})
        );
        let s = crate::search_content::Stats::default();
        assert!(serde_json::to_value(s).unwrap().get("regex").is_none());
        let old: crate::search_content::Match =
            serde_json::from_str(r#"{"line":1,"byte":0}"#).unwrap();
        assert!(old.valid(false, 1));
        assert!(!old.valid(true, 1));
    }
    #[test]
    fn content_report_checks_presence_counters_kind_line_and_offsets() {
        use crate::search_content::{MAX_FILE, Match, Stats};
        let mut r = Report {
            visited: 1,
            complete: true,
            content: Some(Stats {
                files: 1,
                bytes: 12,
                ..Default::default()
            }),
            hits: vec![Hit {
                relative: "file".into(),
                kind: Kind::File,
                content: Some(Match {
                    line: 2,
                    byte: 3,
                    regex_end: None,
                }),
                snippet: None,
            }],
            ..Default::default()
        };
        validate(&r).unwrap();
        r.content.as_mut().unwrap().limited = 1;
        assert!(validate(&r).is_err());
        r.content.as_mut().unwrap().limited = 0;
        r.hits[0].content = Some(Match {
            line: 0,
            byte: 3,
            regex_end: None,
        });
        assert!(validate(&r).is_err());
        r.hits[0].content = Some(Match {
            line: 1,
            byte: MAX_FILE,
            regex_end: None,
        });
        assert!(validate(&r).is_err());
        r.hits[0].content = None;
        assert!(validate(&r).is_err());
        r.content = None;
        validate(&r).unwrap();
    }
    #[test]
    fn progress_validates_bounds_and_all_counter_regressions() {
        let p = Progress {
            visited: 128,
            found: 2,
            errors: 3,
            mounts_excluded: 1,
            metadata_reads: 10,
        };
        p.validate_after(Progress::default()).unwrap();
        for bad in [
            Progress { visited: 127, ..p },
            Progress { found: 1, ..p },
            Progress { errors: 2, ..p },
            Progress {
                mounts_excluded: 0,
                ..p
            },
            Progress {
                metadata_reads: 9,
                ..p
            },
            Progress { found: 501, ..p },
            Progress {
                visited: MAX_VISITS + 1,
                ..p
            },
        ] {
            assert!(bad.validate_after(p).is_err());
        }
    }
    #[test]
    fn rejects_escape_absolute_and_mislabelled_partial_reports() {
        for name in ["../escape", "/absolute", "", "a/../escape"] {
            let r = Report {
                hits: vec![Hit {
                    relative: name.into(),
                    kind: Kind::File,
                    content: None,
                    snippet: None,
                }],
                complete: true,
                ..Default::default()
            };
            assert!(validate(&r).is_err());
        }
        assert!(
            validate(&Report {
                errors: 1,
                complete: true,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            validate(&Report {
                complete: false,
                ..Default::default()
            })
            .is_err()
        );
    }
    #[test]
    fn wire_prevents_unbounded_results_and_preserves_raw_paths() {
        use crate::protocol::Response;
        let mut r = Report {
            complete: true,
            ..Default::default()
        };
        r.hits = (0..=MAX_RESULTS)
            .map(|i| Hit {
                relative: format!("file{i}").into(),
                kind: Kind::File,
                content: None,
                snippet: None,
            })
            .collect();
        let wire = serde_json::to_vec(&Response::Search { id: 1, report: r }).unwrap();
        assert!(serde_json::from_slice::<Response>(&wire).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let name = std::ffi::OsString::from_vec(b"nested/raw\xff.rs".to_vec());
            let r = Report {
                complete: true,
                hits: vec![Hit {
                    relative: PathBuf::from(name.clone()),
                    kind: Kind::File,
                    content: None,
                    snippet: None,
                }],
                ..Default::default()
            };
            let wire = serde_json::to_vec(&Response::Search { id: 1, report: r }).unwrap();
            let Response::Search { report, .. } = serde_json::from_slice(&wire).unwrap() else {
                panic!()
            };
            validate(&report).unwrap();
            assert_eq!(report.hits[0].relative.as_os_str(), name);
        }
    }
}
