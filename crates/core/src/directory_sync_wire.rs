//! Versioned fresh-review transport. Proposals are derived from validated
//! observations locally, never trusted as executable instructions from a frame.
use crate::{
    directory_compare::{self as compare, Progress},
    directory_compare_wire as observation,
    directory_sync::{self as sync, Mode, Review},
    relative_source::DirectoryIdentity,
};
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Component, PathBuf},
    sync::atomic::AtomicBool,
};

pub const VERSION: u8 = 1;
pub const MAX_FRAME: usize = observation::MAX_FRAME;
// Each observation frame gains a <=128-byte version/mode wrapper. A page has
// >=1 row; at most 2*MAX_ITEMS pages, MAX_UPDATES progress and one terminal.
pub const MAX_STREAM: usize =
    observation::MAX_STREAM + 128 * (2 * compare::MAX_ITEMS + observation::MAX_UPDATES + 1);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub version: u8,
    pub id: u64,
    #[serde(with = "crate::protocol::path_wire")]
    pub left: PathBuf,
    #[serde(with = "crate::protocol::path_wire")]
    pub right: PathBuf,
    pub expected: Option<[DirectoryIdentity; 2]>,
    pub mode: Mode,
    pub read_limit: u64,
}
impl Intent {
    /// Admission only, no filesystem I/O. Canonical topology is checked by
    /// fresh preparation in the helper; never on the UI thread.
    pub fn validate(&self) -> io::Result<()> {
        for path in [&self.left, &self.right] {
            crate::navigation_history::validate(path).map_err(|_| bad())?;
            if path.components().any(|c| matches!(c, Component::ParentDir)) {
                return Err(bad());
            }
        }
        if self.version != VERSION
            || self.read_limit > compare::MAX_READ
            || self.left == self.right
            || self.expected.is_some_and(|ids| ids[0] == ids[1])
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub version: u8,
    pub mode: Mode,
    pub observation: observation::Frame,
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Revisión de sincronización inválida",
    )
}
/// Finish only after EOF, exit0 and reap. Validation proves wire/intent
/// coherence, not freshness against later writers, atomic snapshots or Apply
/// authority. A caller synthesizing frames cannot gain mutation permissions.
pub struct Collector {
    intent: Intent,
    observations: observation::Collector,
    poisoned: bool,
}
impl Collector {
    pub fn new(intent: Intent) -> io::Result<Self> {
        intent.validate()?;
        Ok(Self {
            observations: observation::Collector::new(
                intent.id,
                intent.read_limit,
                intent.expected,
            )?,
            intent,
            poisoned: false,
        })
    }
    pub fn accept(&mut self, frame: Frame) -> io::Result<Option<Progress>> {
        if self.poisoned {
            return Err(bad());
        }
        let result = if frame.version != VERSION || frame.mode != self.intent.mode {
            Err(bad())
        } else {
            self.observations.accept(frame.observation)
        };
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn finish(self) -> Result<Review, String> {
        if self.poisoned {
            return Err(bad().to_string());
        }
        let report = self.observations.finish()?;
        if report.roots[0] == report.roots[1] {
            return Err(bad().to_string());
        }
        sync::classify(
            [&self.intent.left, &self.intent.right],
            self.intent.mode,
            report,
            &AtomicBool::new(false),
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        directory_compare::{Difference, Kind, Row},
        directory_compare_wire::Event,
        directory_sync::{Choice, Conflict, Proposal, Side},
    };
    fn roots() -> [DirectoryIdentity; 2] {
        [
            DirectoryIdentity {
                device: 1,
                inode: 2,
            },
            DirectoryIdentity {
                device: 1,
                inode: 3,
            },
        ]
    }
    fn intent(mode: Mode) -> Intent {
        Intent {
            version: VERSION,
            id: 7,
            left: "/left".into(),
            right: "/right".into(),
            expected: Some(roots()),
            mode,
            read_limit: 0,
        }
    }
    fn frame(mode: Mode, event: Event) -> Frame {
        Frame {
            version: VERSION,
            mode,
            observation: observation::Frame { id: 7, event },
        }
    }
    fn rows() -> Vec<Row> {
        vec![
            Row {
                path: "a".into(),
                left: Some(Kind::File),
                right: None,
                difference: Difference::OnlyLeft,
                metadata_differs: false,
            },
            Row {
                path: "b".into(),
                left: None,
                right: Some(Kind::File),
                difference: Difference::OnlyRight,
                metadata_differs: false,
            },
            Row {
                path: "content".into(),
                left: Some(Kind::File),
                right: Some(Kind::File),
                difference: Difference::Content,
                metadata_differs: false,
            },
        ]
    }
    fn done(mode: Mode) -> Frame {
        frame(
            mode,
            Event::Done {
                roots: roots(),
                progress: Progress {
                    phase: 3,
                    entries: [2, 2],
                    compared: 3,
                    different: 3,
                    ..Progress::default()
                },
            },
        )
    }
    #[test]
    fn every_mode_derives_proposals_and_pending_choices_from_validated_rows() {
        for mode in [
            Mode::Merge,
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
        ] {
            let mut c = Collector::new(intent(mode)).unwrap();
            c.accept(frame(
                mode,
                Event::Page {
                    offset: 0,
                    rows: rows(),
                },
            ))
            .unwrap();
            c.accept(done(mode)).unwrap();
            let r = c.finish().unwrap();
            assert_eq!(r.mode(), mode);
            assert_eq!(r.roots(), roots());
            assert!(!r.blocked());
            assert_eq!(
                r.entries()[2].proposal(),
                Proposal::Review(Conflict::Content)
            );
            assert!(r.entries().iter().all(|e| e.choice() == Choice::Pending));
            let (a, b) = match mode {
                Mode::Merge => (
                    Proposal::CopyFrom(Side::Left),
                    Proposal::CopyFrom(Side::Right),
                ),
                Mode::Update(Side::Left) => (Proposal::CopyFrom(Side::Left), Proposal::Retain),
                Mode::Update(Side::Right) => (Proposal::Retain, Proposal::CopyFrom(Side::Right)),
                Mode::Mirror(Side::Left) => (
                    Proposal::CopyFrom(Side::Left),
                    Proposal::Review(Conflict::DestinationOnly),
                ),
                Mode::Mirror(Side::Right) => (
                    Proposal::Review(Conflict::DestinationOnly),
                    Proposal::CopyFrom(Side::Right),
                ),
            };
            assert_eq!(
                (r.entries()[0].proposal(), r.entries()[1].proposal()),
                (a, b)
            );
        }
    }
    #[test]
    fn wrong_mode_version_id_poison_even_after_an_accepted_terminal() {
        let mode = Mode::Merge;
        for after_terminal in [false, true] {
            for case in 0..3 {
                let mut c = Collector::new(intent(mode)).unwrap();
                if after_terminal {
                    c.accept(frame(
                        mode,
                        Event::Page {
                            offset: 0,
                            rows: rows(),
                        },
                    ))
                    .unwrap();
                    c.accept(done(mode)).unwrap();
                }
                let mut f = frame(
                    mode,
                    Event::Progress {
                        progress: Progress::default(),
                    },
                );
                match case {
                    0 => f.mode = Mode::Mirror(Side::Right),
                    1 => f.version = 2,
                    _ => f.observation.id = 8,
                }
                assert!(c.accept(f).is_err());
                assert!(c.accept(done(mode)).is_err());
                assert!(c.finish().is_err());
            }
        }
    }
    #[test]
    fn incomplete_invalid_observations_and_root_mismatch_cannot_publish() {
        let mode = Mode::Merge;
        assert!(Collector::new(intent(mode)).unwrap().finish().is_err());
        for case in 0..4 {
            let mut c = Collector::new(intent(mode)).unwrap();
            let mut rs = rows();
            if case == 0 {
                rs[0].path = "../escape".into();
            }
            if case == 1 {
                rs.swap(0, 1);
            }
            let page = c.accept(frame(
                mode,
                Event::Page {
                    offset: 0,
                    rows: rs,
                },
            ));
            let mut f = done(mode);
            if let Event::Done { roots, progress } = &mut f.observation.event {
                if case == 2 {
                    roots[1].inode = 99;
                }
                if case == 3 {
                    progress.compared = 1;
                }
            }
            let terminal = c.accept(f);
            assert!(page.is_err() || terminal.is_err());
            assert!(c.finish().is_err());
        }
        let mut input = intent(mode);
        input.expected = None;
        let mut c = Collector::new(input).unwrap();
        c.accept(frame(
            mode,
            Event::Page {
                offset: 0,
                rows: rows(),
            },
        ))
        .unwrap();
        let mut f = done(mode);
        if let Event::Done { roots, .. } = &mut f.observation.event {
            roots[1] = roots[0];
        }
        c.accept(f).unwrap();
        assert!(c.finish().is_err());
    }
    #[test]
    fn partial_wire_observations_block_every_row_including_known_unique_names() {
        let mode = Mode::Mirror(Side::Left);
        let mut c = Collector::new(intent(mode)).unwrap();
        let mut rs = rows();
        rs[2].difference = Difference::Unknown;
        c.accept(frame(
            mode,
            Event::Page {
                offset: 0,
                rows: rs,
            },
        ))
        .unwrap();
        c.accept(frame(
            mode,
            Event::Done {
                roots: roots(),
                progress: Progress {
                    phase: 3,
                    entries: [2, 2],
                    compared: 3,
                    different: 2,
                    unknown: 1,
                    reasons: compare::READ_LIMIT,
                    ..Progress::default()
                },
            },
        ))
        .unwrap();
        let r = c.finish().unwrap();
        assert!(r.blocked());
        assert!(
            r.entries()
                .iter()
                .all(|e| e.proposal() == Proposal::Blocked)
        );
    }
    #[test]
    fn strict_intent_and_envelope_schema_reject_foreign_instructions() {
        let mode = Mode::Merge;
        for case in 0..6 {
            let mut i = intent(mode);
            match case {
                0 => i.version = 2,
                1 => i.left = "relative".into(),
                2 => i.left = "/a/../b".into(),
                3 => i.right = i.left.clone(),
                4 => i.read_limit = compare::MAX_READ + 1,
                _ => i.expected = Some([roots()[0]; 2]),
            };
            assert!(Collector::new(i).is_err());
        }
        let mut i = serde_json::to_value(intent(mode)).unwrap();
        i["apply"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Intent>(i).is_err());
        let mut f = serde_json::to_value(done(mode)).unwrap();
        f["proposals"] = serde_json::json!(["remove"]);
        assert!(serde_json::from_value::<Frame>(f).is_err());
        let mut f = serde_json::to_value(done(mode)).unwrap();
        f["observation"]["event"]["execute"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Frame>(f).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn raw_intent_roundtrip_binds_mode_roots_and_protocol_command() {
        use std::os::unix::ffi::OsStringExt;
        let mut i = intent(Mode::Mirror(Side::Right));
        i.left = std::ffi::OsString::from_vec(b"/raw\xff\n".to_vec()).into();
        let b = serde_json::to_vec(&crate::protocol::Request::PrepareDirectorySync {
            intent: i.clone(),
        })
        .unwrap();
        let crate::protocol::Request::PrepareDirectorySync { intent: r } =
            serde_json::from_slice(&b).unwrap()
        else {
            panic!()
        };
        assert_eq!(r.left, i.left);
        assert_eq!(r.mode, i.mode);
        assert_eq!(r.expected, i.expected);
        r.validate().unwrap();
    }
    #[test]
    fn full_retained_control_namespace_fits_wrapped_stream_without_four_mib_truncation() {
        let mode = Mode::Mirror(Side::Right);
        let mut c = Collector::new(intent(mode)).unwrap();
        let mut rows = Vec::new();
        for side in 0..2 {
            for n in 0..compare::MAX_ITEMS {
                rows.push(Row {
                    path: format!(
                        "{}{n:04x}{}",
                        if side == 0 { 'a' } else { 'b' },
                        "\u{1}".repeat(123)
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
        let mut total = 0;
        let mut offset = 0;
        while offset < rows.len() {
            let end = observation::page_end(&rows, offset);
            assert!(end > offset);
            let f = frame(
                mode,
                Event::Page {
                    offset,
                    rows: rows[offset..end].to_vec(),
                },
            );
            let b = serde_json::to_vec(&f).unwrap();
            assert!(b.len() <= MAX_FRAME);
            total += b.len() + 1;
            c.accept(serde_json::from_slice(&b).unwrap()).unwrap();
            offset = end;
        }
        let f = frame(
            mode,
            Event::Done {
                roots: roots(),
                progress: Progress {
                    phase: 3,
                    entries: [compare::MAX_ITEMS as u64; 2],
                    compared: rows.len() as u64,
                    different: rows.len() as u64,
                    ..Progress::default()
                },
            },
        );
        total += serde_json::to_vec(&f).unwrap().len() + 1;
        c.accept(f).unwrap();
        // Reserve all possible progress frames, even though this fixture did not
        // need them. Wrapper itself is bounded independently for every mode.
        for mode in [
            Mode::Merge,
            Mode::Update(Side::Left),
            Mode::Mirror(Side::Right),
        ] {
            let plain = observation::Frame {
                id: u64::MAX,
                event: Event::Progress {
                    progress: Progress {
                        phase: 2,
                        entries: [compare::MAX_ENTRIES; 2],
                        bytes_read: compare::MAX_READ,
                        ..Progress::default()
                    },
                },
            };
            let plain_len = serde_json::to_vec(&plain).unwrap().len();
            let wrapped_len = serde_json::to_vec(&Frame {
                version: VERSION,
                mode,
                observation: plain,
            })
            .unwrap()
            .len();
            assert!(wrapped_len - plain_len <= 128);
        }
        total += observation::MAX_UPDATES * (512 + 128);
        assert!(total > 4 * 1024 * 1024);
        assert!(total <= MAX_STREAM);
        assert_eq!(c.finish().unwrap().entries().len(), 2 * compare::MAX_ITEMS);
    }
}
