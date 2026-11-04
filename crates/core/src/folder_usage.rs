//! Versioned opt-in regular-file allocation result; legacy usage stays separate.
use serde::{Deserialize, Serialize};

pub const VERSION: u8 = 1;
pub const MAX_IDENTITIES: usize = 65_536;
pub const MAX_ENTRIES: u64 = 1_000_000;
pub const MAX_FRAME_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Apparent,
    Allocated,
}

/// In-process result. Allocation absence denotes an apparent-only request,
/// while typed unavailable values remain completed allocation observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub bytes: u64,
    pub entries: u64,
    pub allocation: Option<Observation>,
}
impl Measurement {
    pub fn apparent(bytes: u64, entries: u64) -> Self {
        Self {
            bytes,
            entries,
            allocation: None,
        }
    }
    pub fn validate(&self, mode: Mode) -> Result<(), &'static str> {
        if self.entries > MAX_ENTRIES {
            return Err("Límite de entradas de cálculo inválido");
        }
        match (mode, self.allocation) {
            (Mode::Apparent, None) => Ok(()),
            (Mode::Allocated, Some(allocation)) => Report {
                version: VERSION,
                bytes: self.bytes,
                entries: self.entries,
                allocation,
            }
            .validate(),
            _ => Err("El modo del resultado no coincide con el cálculo solicitado"),
        }
    }
}
impl TryFrom<Report> for Measurement {
    type Error = &'static str;
    fn try_from(report: Report) -> Result<Self, Self::Error> {
        report.validate()?;
        Ok(Self {
            bytes: report.bytes,
            entries: report.entries,
            allocation: Some(report.allocation),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Overflow,
    IdentityLimit,
    ChangedIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Bytes(u64),
    Unavailable(Reason),
}
impl Value {
    pub fn label(self) -> String {
        match self {
            Self::Bytes(bytes) => format!("{bytes} bytes"),
            Self::Unavailable(reason) => format!(
                "NO DISPONIBLE · {}",
                match reason {
                    Reason::Overflow => "desbordamiento",
                    Reason::IdentityLimit => "límite de65.536identidades",
                    Reason::ChangedIdentity => "identidad cambió durante el recorrido",
                }
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    /// Regular-file allocation per observed name, including hardlink aliases.
    pub by_name: Value,
    /// Once per observed device/inode within one root, not across reports.
    pub by_identity: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u8,
    pub bytes: u64,
    pub entries: u64,
    pub allocation: Observation,
}
impl Report {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != VERSION || self.entries > MAX_ENTRIES {
            return Err("Versión o límite de entradas inválido");
        }
        let Observation {
            by_name,
            by_identity,
        } = self.allocation;
        for value in [by_name, by_identity] {
            if matches!(value, Value::Bytes(n) if n % 512 != 0) {
                return Err("Asignación sin unidades de512B");
            }
        }
        if matches!(by_name, Value::Unavailable(reason) if reason != Reason::Overflow) {
            return Err("Motivo de asignación por nombre inválido");
        }
        if let (Value::Bytes(names), Value::Bytes(unique)) = (by_name, by_identity)
            && unique > names
        {
            return Err("Asignación única superior a la suma por nombre");
        }
        if matches!(by_identity, Value::Unavailable(Reason::Overflow))
            && !matches!(by_name, Value::Unavailable(Reason::Overflow))
        {
            return Err("Desbordamiento único incompatible con suma por nombre");
        }
        if matches!(by_identity, Value::Unavailable(Reason::IdentityLimit))
            && self.entries <= MAX_IDENTITIES as u64
        {
            return Err("Límite de identidades incompatible con entradas");
        }
        if matches!(by_identity, Value::Unavailable(Reason::ChangedIdentity)) && self.entries < 2 {
            return Err("Conflicto de identidad sin dos observaciones");
        }
        if self.entries == 0
            && (self.bytes != 0 || by_name != Value::Bytes(0) || by_identity != Value::Bytes(0))
        {
            return Err("Resultado vacío incompatible con totales");
        }
        Ok(())
    }
}

/// Strict allocated-mode reader vocabulary. A legacy Usage final is invalid.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Frame {
    UsageProgress { id: u64, bytes: u64, entries: u64 },
    AllocatedUsage { id: u64, report: Report },
    Error { id: u64, message: String },
    Unsupported { id: u64, message: String },
}

/// Semantic ledger shared by bounded stream readers; no filesystem operations.
pub struct Ledger {
    id: u64,
    progress: Option<(u64, u64)>,
    terminal: Option<Result<Report, String>>,
}
impl Ledger {
    pub fn new(id: u64) -> Result<Self, &'static str> {
        if id == 0 {
            return Err("Identidad de cálculo inválida");
        }
        Ok(Self {
            id,
            progress: None,
            terminal: None,
        })
    }
    pub fn progress(&self) -> Option<(u64, u64)> {
        self.progress
    }
    pub fn accept(&mut self, frame: Frame) -> Result<(), String> {
        let result = self.accept_frame(frame);
        if let Err(message) = &result {
            self.terminal = Some(Err(message.clone()));
        }
        result
    }
    fn accept_frame(&mut self, frame: Frame) -> Result<(), String> {
        if self.terminal.is_some() {
            return Err("Datos después del resultado final".into());
        }
        match frame {
            Frame::UsageProgress { id, bytes, entries } if id == self.id => {
                let previous = self.progress.unwrap_or((0, 0));
                if entries > MAX_ENTRIES
                    || bytes < previous.0
                    || entries < previous.1
                    || self.progress.is_none() && (bytes, entries) != (0, 0)
                {
                    return Err("Progreso de asignación inválido".into());
                }
                self.progress = Some((bytes, entries));
            }
            Frame::AllocatedUsage { id, report } if id == self.id => {
                report.validate()?;
                if self.progress != Some((report.bytes, report.entries)) {
                    return Err("El resultado no coincide con el progreso final".into());
                }
                self.terminal = Some(Ok(report));
            }
            Frame::Error { id, message } | Frame::Unsupported { id, message } if id == self.id => {
                if message.is_empty() || message.len() > 512 {
                    return Err("Diagnóstico de cálculo inválido".into());
                }
                self.terminal = Some(Err(message));
            }
            _ => return Err("Identidad de cálculo inesperada".into()),
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Report, String> {
        self.terminal
            .unwrap_or_else(|| Err("Cálculo sin resultado completo".into()))
    }
}

/// Newline-framed reader with a hard bound before buffering. Parsing is supplied
/// by the caller so core does not require a JSON runtime dependency.
pub struct Reader {
    ledger: Ledger,
    line: Vec<u8>,
    failure: Option<String>,
}
impl Reader {
    pub fn new(id: u64) -> Result<Self, &'static str> {
        Ok(Self {
            ledger: Ledger::new(id)?,
            line: Vec::new(),
            failure: None,
        })
    }
    pub fn progress(&self) -> Option<(u64, u64)> {
        self.ledger.progress()
    }
    pub fn feed(
        &mut self,
        data: &[u8],
        mut parse: impl FnMut(&[u8]) -> Result<Frame, String>,
    ) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result: Result<(), String> = (|| {
            for &byte in data {
                if byte == b'\n' {
                    let frame = parse(&self.line)?;
                    self.ledger.accept(frame)?;
                    self.line.clear();
                } else {
                    if self.line.len() == MAX_FRAME_BYTES {
                        return Err("Línea de asignación superior a4096B".into());
                    }
                    self.line.push(byte);
                }
            }
            Ok(())
        })();
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    pub fn finish(self) -> Result<Report, String> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if !self.line.is_empty() {
            return Err("Respuesta de asignación truncada".into());
        }
        self.ledger.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> Report {
        Report {
            version: VERSION,
            bytes: 7,
            entries: 2,
            allocation: Observation {
                by_name: Value::Bytes(8192),
                by_identity: Value::Bytes(4096),
            },
        }
    }
    #[test]
    fn in_process_measurement_preserves_mode_and_unavailability_and_validates_version_before_conversion()
     {
        let apparent = Measurement::apparent(7, 2);
        assert!(apparent.validate(Mode::Apparent).is_ok());
        assert!(apparent.validate(Mode::Allocated).is_err());
        let allocated = Measurement::try_from(report()).unwrap();
        assert!(allocated.validate(Mode::Allocated).is_ok());
        assert!(allocated.validate(Mode::Apparent).is_err());
        assert!(
            Measurement::try_from(Report {
                version: VERSION + 1,
                ..report()
            })
            .is_err()
        );
        assert!(
            Measurement::apparent(0, MAX_ENTRIES + 1)
                .validate(Mode::Apparent)
                .is_err()
        );
        let unavailable = Report {
            entries: MAX_IDENTITIES as u64 + 1,
            allocation: Observation {
                by_name: Value::Bytes(512),
                by_identity: Value::Unavailable(Reason::IdentityLimit),
            },
            ..report()
        };
        let value = Measurement::try_from(unavailable).unwrap();
        assert_eq!(value.allocation, Some(unavailable.allocation));
        assert!(value.validate(Mode::Allocated).is_ok());
    }
    #[test]
    fn protocol_serialization_matches_strict_frame_and_legacy_shape_is_unchanged() {
        use crate::protocol::{Request, Response};
        let bytes = serde_json::to_vec(&Response::AllocatedUsage {
            id: 7,
            report: report(),
        })
        .unwrap();
        let Frame::AllocatedUsage {
            id,
            report: decoded,
        } = parse(&bytes).unwrap()
        else {
            panic!("wrong mode")
        };
        assert_eq!((id, decoded), (7, report()));
        assert_eq!(
            serde_json::to_string(&Response::Usage {
                id: 7,
                bytes: 3,
                entries: 2
            })
            .unwrap(),
            r#"{"status":"usage","id":7,"bytes":3,"entries":2}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::UsageProgress {
                id: 7,
                bytes: 3,
                entries: 2
            })
            .unwrap(),
            r#"{"status":"usage_progress","id":7,"bytes":3,"entries":2}"#
        );
        assert_eq!(
            serde_json::to_string(&Request::MeasureDirectory {
                id: 7,
                path: "/fixture".into()
            })
            .unwrap(),
            r#"{"command":"measure_directory","id":7,"path":"/fixture"}"#
        );
        let request = serde_json::to_vec(&Request::MeasureDirectoryAllocated {
            id: 7,
            version: VERSION,
            path: "/fixture".into(),
        })
        .unwrap();
        assert!(matches!(
            serde_json::from_slice::<Request>(&request).unwrap(),
            Request::MeasureDirectoryAllocated {
                id: 7,
                version: VERSION,
                ..
            }
        ));
    }
    #[test]
    fn strict_report_vocabulary_rejects_unknown_fields_reasons_and_missing_values() {
        for value in [
            r#"{"state":"bytes","value":0,"extra":1}"#,
            r#"{"state":"bytes"}"#,
            r#"{"state":"unavailable","value":"future"}"#,
            r#"{"state":"unavailable","value":"identity_limit","extra":1}"#,
        ] {
            assert!(serde_json::from_str::<Value>(value).is_err());
        }
        let mut value = serde_json::to_value(report()).unwrap();
        value["allocation"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Report>(value).is_err());
        for message in [String::new(), "x".repeat(513)] {
            let mut ledger = Ledger::new(7).unwrap();
            assert!(ledger.accept(Frame::Error { id: 7, message }).is_err());
            assert!(ledger.finish().is_err());
        }
        let mut ledger = Ledger::new(7).unwrap();
        ledger
            .accept(Frame::Unsupported {
                id: 7,
                message: "no disponible".into(),
            })
            .unwrap();
        assert_eq!(ledger.finish().unwrap_err(), "no disponible");
    }
    fn parse(line: &[u8]) -> Result<Frame, String> {
        serde_json::from_slice(line).map_err(|e| e.to_string())
    }
    fn wire() -> Vec<u8> {
        let frames = [
            Frame::UsageProgress {
                id: 7,
                bytes: 0,
                entries: 0,
            },
            Frame::UsageProgress {
                id: 7,
                bytes: 7,
                entries: 2,
            },
            Frame::AllocatedUsage {
                id: 7,
                report: report(),
            },
        ];
        let mut bytes = Vec::new();
        for frame in frames {
            bytes.extend(serde_json::to_vec(&frame).unwrap());
            bytes.push(b'\n');
        }
        bytes
    }
    #[test]
    fn arbitrary_chunk_boundaries_preserve_exact_final_and_progress() {
        let wire = wire();
        for split in 0..=wire.len() {
            let mut reader = Reader::new(7).unwrap();
            reader.feed(&wire[..split], parse).unwrap();
            reader.feed(&wire[split..], parse).unwrap();
            assert_eq!(reader.progress(), Some((7, 2)));
            assert_eq!(reader.finish().unwrap(), report());
        }
        let mut reader = Reader::new(7).unwrap();
        for byte in wire {
            reader.feed(&[byte], parse).unwrap();
        }
        assert_eq!(reader.finish().unwrap(), report());
    }
    #[test]
    fn malformed_legacy_unknown_duplicate_and_wrong_identity_are_terminal_failures() {
        for message in [
            r#"{"status":"usage","id":7,"bytes":7,"entries":2}"#,
            r#"{"status":"usage_progress","id":8,"bytes":0,"entries":0}"#,
            r#"{"status":"usage_progress","id":7,"bytes":0,"entries":0,"extra":1}"#,
            r#"{"status":"usage_progress","id":7,"bytes":0,"bytes":1,"entries":0}"#,
            r#"{"status":"usage_progress","id":7,"bytes":-1,"entries":0}"#,
            "{}",
            "not json",
            "",
        ] {
            let mut reader = Reader::new(7).unwrap();
            assert!(
                reader
                    .feed(format!("{message}\n").as_bytes(), parse)
                    .is_err()
            );
            assert!(reader.feed(&wire(), parse).is_err());
            assert!(reader.finish().is_err());
        }
        assert!(Reader::new(0).is_err());
    }
    #[test]
    fn bounds_truncation_terminal_errors_and_extra_data_do_not_return_success() {
        let mut reader = Reader::new(7).unwrap();
        assert!(
            reader
                .feed(&vec![b'x'; MAX_FRAME_BYTES + 1], parse)
                .is_err()
        );
        assert_eq!(reader.line.len(), MAX_FRAME_BYTES);
        assert!(reader.finish().is_err());
        let wire = wire();
        for cut in 0..wire.len() {
            let mut reader = Reader::new(7).unwrap();
            reader.feed(&wire[..cut], parse).unwrap();
            assert!(reader.finish().is_err());
        }
        let mut reader = Reader::new(7).unwrap();
        reader.feed(&wire, parse).unwrap();
        assert!(reader.feed(b"\n", parse).is_err());
        assert!(reader.finish().is_err());
        let mut reader = Reader::new(7).unwrap();
        reader
            .feed(
                b"{\"status\":\"error\",\"id\":7,\"message\":\"rechazado\"}\n",
                parse,
            )
            .unwrap();
        assert_eq!(reader.finish().unwrap_err(), "rechazado");
    }
    #[test]
    fn progress_requires_zero_monotonic_bounds_and_exact_terminal() {
        for values in [
            vec![(1, 0)],
            vec![(0, 0), (7, 2), (6, 2)],
            vec![(0, 0), (7, 2), (7, 1)],
            vec![(0, 0), (0, MAX_ENTRIES + 1)],
        ] {
            let mut ledger = Ledger::new(7).unwrap();
            assert!(values.into_iter().any(|(bytes, entries)| {
                ledger
                    .accept(Frame::UsageProgress {
                        id: 7,
                        bytes,
                        entries,
                    })
                    .is_err()
            }));
            assert!(ledger.finish().is_err());
        }
        for progress in [None, Some((0, 0)), Some((7, 1)), Some((8, 2))] {
            let mut ledger = Ledger::new(7).unwrap();
            ledger
                .accept(Frame::UsageProgress {
                    id: 7,
                    bytes: 0,
                    entries: 0,
                })
                .unwrap();
            if let Some((bytes, entries)) = progress {
                ledger
                    .accept(Frame::UsageProgress {
                        id: 7,
                        bytes,
                        entries,
                    })
                    .unwrap();
            }
            assert!(
                ledger
                    .accept(Frame::AllocatedUsage {
                        id: 7,
                        report: report()
                    })
                    .is_err()
            );
            assert!(ledger.finish().is_err());
        }
    }
    #[test]
    fn report_rejects_impossible_metrics_and_future_versions() {
        let valid = report();
        assert!(valid.validate().is_ok());
        for value in [
            Report {
                version: 2,
                ..valid
            },
            Report {
                entries: MAX_ENTRIES + 1,
                ..valid
            },
            Report {
                entries: 0,
                ..valid
            },
            Report {
                allocation: Observation {
                    by_name: Value::Bytes(1),
                    ..valid.allocation
                },
                ..valid
            },
            Report {
                allocation: Observation {
                    by_identity: Value::Bytes(16384),
                    ..valid.allocation
                },
                ..valid
            },
            Report {
                allocation: Observation {
                    by_name: Value::Unavailable(Reason::IdentityLimit),
                    ..valid.allocation
                },
                ..valid
            },
            Report {
                allocation: Observation {
                    by_identity: Value::Unavailable(Reason::Overflow),
                    ..valid.allocation
                },
                ..valid
            },
            Report {
                allocation: Observation {
                    by_identity: Value::Unavailable(Reason::IdentityLimit),
                    ..valid.allocation
                },
                ..valid
            },
            Report {
                entries: 1,
                allocation: Observation {
                    by_identity: Value::Unavailable(Reason::ChangedIdentity),
                    ..valid.allocation
                },
                ..valid
            },
        ] {
            assert!(value.validate().is_err(), "{value:?}");
        }
        for allocation in [
            Observation {
                by_name: Value::Unavailable(Reason::Overflow),
                by_identity: Value::Bytes(512),
            },
            Observation {
                by_name: Value::Unavailable(Reason::Overflow),
                by_identity: Value::Unavailable(Reason::Overflow),
            },
            Observation {
                by_name: Value::Bytes(512),
                by_identity: Value::Unavailable(Reason::ChangedIdentity),
            },
        ] {
            assert!(
                Report {
                    allocation,
                    ..valid
                }
                .validate()
                .is_ok()
            );
        }
    }
}
