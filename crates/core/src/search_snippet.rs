//! A normalized, bounded display fragment; not a file snapshot or mutation proof.
use serde::{Deserialize, Serialize};
pub const MAX_BYTES: usize = 256;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snippet {
    #[serde(deserialize_with = "bounded_text")]
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub leading: bool,
    pub trailing: bool,
    pub match_clipped: bool,
}
fn bounded_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Text;
    impl<'de> serde::de::Visitor<'de> for Text {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("fragmento UTF8 de hasta256 bytes")
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<String, E> {
            if s.len() > MAX_BYTES {
                return Err(E::custom("Fragmento demasiado largo"));
            }
            Ok(s.to_owned())
        }
    }
    d.deserialize_str(Text)
}
impl Snippet {
    pub fn valid(&self) -> bool {
        !self.text.is_empty()
            && self.text.len() <= MAX_BYTES
            && self.text.capacity() <= MAX_BYTES
            && !self.text.chars().any(char::is_control)
            && self.start < self.end
            && self.end <= self.text.len()
            && self.text.is_char_boundary(self.start)
            && self.text.is_char_boundary(self.end)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn snippet() -> Snippet {
        Snippet {
            text: "antes á después".into(),
            start: 6,
            end: 8,
            leading: false,
            trailing: false,
            match_clipped: false,
        }
    }
    #[test]
    fn utf8_marks_and_wire_size_are_strict() {
        let good = snippet();
        assert!(good.valid());
        let wire = serde_json::to_vec(&good).unwrap();
        let copy: Snippet = serde_json::from_slice(&wire).unwrap();
        assert_eq!(good, copy);
        for (start, end) in [(7, 8), (6, 7), (8, 6), (6, 999), (6, 6)] {
            let mut s = snippet();
            s.start = start;
            s.end = end;
            assert!(!s.valid());
        }
        let mut s = snippet();
        s.text = "x".repeat(257);
        assert!(!s.valid());
        assert!(serde_json::from_slice::<Snippet>(&serde_json::to_vec(&s).unwrap()).is_err());
        let mut s = snippet();
        s.text = "a\nb".into();
        s.start = 0;
        s.end = 1;
        assert!(!s.valid());
    }
    #[test]
    fn worst_case_valid_snippets_and_paths_keep_existing_wire_caps() {
        use crate::{
            listing::Kind,
            protocol::Response,
            search::{Hit, Report},
            search_content::{Match, Stats},
        };
        for regex in [false, true] {
            let hits = (0..500)
                .map(|i| Hit {
                    relative: format!("{i:03}{}", "\u{1}".repeat(509)).into(),
                    kind: Kind::File,
                    content: Some(Match {
                        line: 1,
                        byte: 0,
                        regex_end: regex.then_some(crate::search_content::MAX_FILE),
                    }),
                    snippet: Some(Snippet {
                        text: "\\".repeat(256),
                        start: 0,
                        end: 1,
                        leading: true,
                        trailing: true,
                        match_clipped: true,
                    }),
                })
                .collect();
            let report = Report {
                hits,
                visited: 500,
                content: Some(Stats {
                    regex,
                    files: 500,
                    bytes: if regex {
                        crate::search_content::MAX_TOTAL
                    } else {
                        500
                    },
                    ..Default::default()
                }),
                complete: true,
                ..Default::default()
            };
            crate::search::validate(&report).unwrap();
            let mut total = 256 * 512; // conservative allowance for bounded progress frames
            for (chunk, batch) in report
                .hits
                .chunks(crate::search_batch::MAX_BATCH)
                .enumerate()
            {
                let bytes =
                    serde_json::to_vec(&crate::search_batch::Frame::new(1, chunk * 32, batch))
                        .unwrap();
                assert!(bytes.len() < 2 * 1024 * 1024);
                total += bytes.len() + 1;
            }
            let final_bytes = serde_json::to_vec(&Response::Search { id: 1, report }).unwrap();
            assert!(final_bytes.len() < 2 * 1024 * 1024);
            total += final_bytes.len() + 1;
            assert!(total < 4 * 1024 * 1024);
        }
    }
}
