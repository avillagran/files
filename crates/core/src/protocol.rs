use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    PrepareDirectorySync {
        intent: crate::directory_sync_wire::Intent,
    },
    CompareDirectories {
        id: u64,
        #[serde(with = "path_wire")]
        left: std::path::PathBuf,
        #[serde(with = "path_wire")]
        right: std::path::PathBuf,
        expected: Option<[crate::relative_source::DirectoryIdentity; 2]>,
        read_limit: u64,
    },
    MeasureDirectoryAllocated {
        id: u64,
        version: u8,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    FindDuplicates {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
    },
    HashFiles {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
        names: Vec<crate::checksum::Name>,
    },
    MeasureDirectory {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    Search {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        query: String,
        hidden: bool,
    },
    SearchStream {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        query: String,
        hidden: bool,
    },
    SearchPages {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        query: String,
        hidden: bool,
    },
    SearchBatches {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        query: String,
        hidden: bool,
    },
    SpaceMap {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    PreviewOffice {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        revision: crate::media_source::Revision,
    },
    PreviewArchive {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    PreviewDirectory {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    Du {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    PreviewText {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
    PreviewRelativeText {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
        #[serde(with = "path_wire")]
        relative: std::path::PathBuf,
    },
    PreviewRelativeImage {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
        #[serde(with = "path_wire")]
        relative: std::path::PathBuf,
    },
    ProbeRelativeFile {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
        #[serde(with = "path_wire")]
        relative: std::path::PathBuf,
        expected_root: crate::relative_source::DirectoryIdentity,
    },
    PreviewRelativeImageChecked {
        id: u64,
        #[serde(with = "path_wire")]
        root: std::path::PathBuf,
        #[serde(with = "path_wire")]
        relative: std::path::PathBuf,
        expected: crate::relative_source::Proof,
    },
    PreviewThumbnail {
        id: u64,
        raw_path: Vec<u8>,
        revision: crate::media_source::Revision,
        max_dimension: u32,
    },
    PreviewImageChecked {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
        revision: crate::media_source::Revision,
    },
    PreviewImage {
        id: u64,
        #[serde(with = "path_wire")]
        path: std::path::PathBuf,
    },
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    AllocatedUsage {
        id: u64,
        report: crate::folder_usage::Report,
    },
    UsageProgress {
        id: u64,
        bytes: u64,
        entries: u64,
    },
    SearchProgress {
        id: u64,
        progress: crate::search::Progress,
    },
    SearchBatch {
        id: u64,
        version: u8,
        batch: crate::search_batch::Batch,
    },
    Search {
        id: u64,
        report: crate::search::Report,
    },
    RelativeSource {
        id: u64,
        proof: crate::relative_source::Proof,
    },
    SpaceMap {
        id: u64,
        report: crate::space_map::SpaceReport,
    },
    Error {
        id: u64,
        message: String,
    },
    Unsupported {
        id: u64,
        message: String,
    },
    Usage {
        id: u64,
        bytes: u64,
        entries: u64,
    },
    /// One JSON header followed by exactly byte_len raw RGB8 bytes.
    Image {
        id: u64,
        width: u32,
        height: u32,
        byte_len: usize,
        source_width: u32,
        source_height: u32,
        format: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<crate::media_source::Revision>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relative_source: Option<crate::relative_source::Proof>,
    },
    OfficeText {
        id: u64,
        text: String,
        truncated: bool,
        source: crate::media_source::Revision,
    },
    Text {
        id: u64,
        text: String,
        encoding: String,
        truncated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relative_source: Option<crate::relative_source::Proof>,
    },
}
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const PROTOCOL_VERSION: u32 = 1;

/// Legacy Unicode paths stay JSON strings. Opaque Unix paths use bounded bytes;
/// relative paths are preserved too, validation belongs to the command/openat.
pub(crate) mod path_wire {
    use serde::{
        Deserializer, Serializer,
        de::{self, SeqAccess, Visitor},
        ser::{Error as _, SerializeSeq},
    };
    use std::{
        fmt,
        path::{Path, PathBuf},
    };
    const LIMIT: usize = 4096;
    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = path.as_os_str().as_encoded_bytes();
        if bytes.len() > LIMIT || bytes.contains(&0) {
            return Err(S::Error::custom("Ruta fuera del límite de4096B o con NUL"));
        }
        if let Some(text) = path.to_str() {
            return serializer.serialize_str(text);
        }
        #[cfg(unix)]
        {
            let mut seq = serializer.serialize_seq(Some(bytes.len()))?;
            for byte in bytes {
                seq.serialize_element(byte)?;
            }
            seq.end()
        }
        #[cfg(not(unix))]
        {
            Err(S::Error::custom(
                "Ruta no Unicode pendiente en esta plataforma",
            ))
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        struct PathVisitor;
        impl<'de> Visitor<'de> for PathVisitor {
            type Value = PathBuf;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("ruta Unicode o bytes Unix acotados")
            }
            fn visit_str<E: de::Error>(self, text: &str) -> Result<PathBuf, E> {
                if text.len() > LIMIT || text.as_bytes().contains(&0) {
                    return Err(E::custom("Ruta fuera del límite de4096B o con NUL"));
                }
                Ok(PathBuf::from(text))
            }
            fn visit_string<E: de::Error>(self, text: String) -> Result<PathBuf, E> {
                if text.len() > LIMIT || text.as_bytes().contains(&0) {
                    return Err(E::custom("Ruta fuera del límite de4096B o con NUL"));
                }
                Ok(PathBuf::from(text.into_boxed_str().into_string()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<PathBuf, A::Error> {
                if seq.size_hint().is_some_and(|n| n > LIMIT) {
                    return Err(de::Error::custom("Ruta raw demasiado larga"));
                }
                let mut bytes = Vec::new();
                while let Some(byte) = seq.next_element::<u8>()? {
                    if bytes.len() == LIMIT || byte == 0 {
                        return Err(de::Error::custom("Ruta raw fuera del límite o con NUL"));
                    }
                    bytes.push(byte);
                }
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStringExt;
                    // Array decoding grows geometrically. Retain only the actual
                    // bytes so a valid wire page cannot exceed its capacity budget
                    // solely because of receiver-side spare path capacity.
                    let bytes = bytes.into_boxed_slice().into_vec();
                    Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
                }
                #[cfg(not(unix))]
                {
                    let _ = bytes;
                    Err(de::Error::custom("Ruta raw pendiente en esta plataforma"))
                }
            }
        }
        deserializer.deserialize_any(PathVisitor)
    }
}
#[cfg(all(test, unix))]
mod path_tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
    #[test]
    fn opaque_search_path_capacity_roundtrip_preserves_a_valid_500_hit_budget() {
        let mut report = crate::search::Report {
            visited: 500,
            complete: true,
            ..Default::default()
        };
        for i in 0..500 {
            let mut bytes = vec![b'x'; 513];
            bytes[170] = b'/';
            bytes[341] = b'/';
            bytes[512] = 0xff;
            bytes[509..512].copy_from_slice(format!("{i:03}").as_bytes());
            report.hits.push(crate::search::Hit {
                relative: PathBuf::from(OsString::from_vec(bytes)),
                kind: crate::listing::Kind::File,
                content: None,
                snippet: None,
            });
        }
        crate::search::validate(&report).unwrap();
        let wire = serde_json::to_vec(&report).unwrap();
        let decoded: crate::search::Report = serde_json::from_slice(&wire).unwrap();
        crate::search::validate(&decoded).unwrap();
        assert!(
            decoded
                .hits
                .iter()
                .all(|h| h.relative.capacity() == 513 && h.relative.as_os_str().len() == 513)
        );
        assert_eq!(decoded.hits[499].relative, report.hits[499].relative);
    }
    #[test]
    fn request_paths_preserve_opaque_bytes_and_legacy_unicode() {
        for path in [
            PathBuf::from("/fixture/área.png"),
            PathBuf::from(OsString::from_vec(b"/fixture/raw\xff.png".to_vec())),
        ] {
            let expected = path.clone();
            let raw = path.to_str().is_none();
            let wire = serde_json::to_vec(&Request::PreviewImage { id: 1, path }).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&wire).unwrap();
            assert_eq!(value["path"].is_array(), raw);
            let Request::PreviewImage { path, .. } = serde_json::from_slice(&wire).unwrap() else {
                panic!("wrong command")
            };
            assert_eq!(path, expected);
        }
    }
    #[test]
    fn relative_resource_root_and_name_also_roundtrip_opaque_bytes() {
        let root =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/fixture/root\xfe".to_vec()));
        let relative =
            std::path::PathBuf::from(std::ffi::OsString::from_vec(b"nested/raw\xff.png".to_vec()));
        let wire = serde_json::to_vec(&Request::PreviewRelativeImage {
            id: 1,
            root: root.clone(),
            relative: relative.clone(),
        })
        .unwrap();
        let Request::PreviewRelativeImage {
            root: r,
            relative: p,
            ..
        } = serde_json::from_slice(&wire).unwrap()
        else {
            panic!("wrong command")
        };
        assert_eq!((r, p), (root, relative));
    }
    #[test]
    fn relative_paths_are_not_normalized_or_url_decoded() {
        let value = serde_json::json!({"command":"preview_relative_text","id":1,"root":"/fixture","relative":"sub/%2e%2e/../a"});
        let Request::PreviewRelativeText { relative, .. } = serde_json::from_value(value).unwrap()
        else {
            panic!("wrong command")
        };
        assert_eq!(relative, PathBuf::from("sub/%2e%2e/../a"));
    }
    #[test]
    fn path_bounds_and_nul_reject_without_partial_request() {
        for value in [
            serde_json::json!(vec![255; 4097]),
            serde_json::json!([47, 0, 65]),
            serde_json::json!([47, 256]),
            serde_json::json!("x".repeat(4097)),
            serde_json::json!("/bad\u{0}path"),
        ] {
            let input = serde_json::json!({"command":"preview_text","id":1,"path":value});
            assert!(serde_json::from_value::<Request>(input).is_err());
        }
        let path = PathBuf::from(OsString::from_vec(vec![255; 4097]));
        assert!(serde_json::to_vec(&Request::PreviewText { id: 1, path }).is_err());
    }
}
