//! Bounded UTF-8 content regex foundation. No filesystem work or wire format.
//! Uses the existing std-only regex engine, including empty matches and EOF.
//! Cancellation is cooperative before/after one compile or first-match search;
//! it cannot interrupt the engine internally. Limits are local, not PSS claims.
use regex::{Regex, RegexBuilder};

pub const MAX_PATTERN: usize = crate::search_content::MAX_LITERAL;
pub const MAX_NFA_BYTES: usize = 1024 * 1024;
pub const MAX_DFA_BYTES: usize = 256 * 1024;
pub const MAX_NESTING: u32 = 64;
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    PatternLimit,
    TextLimit,
    Invalid(String),
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// UTF-8 byte offsets; start == end represents a zero-width match.
    pub start: usize,
    pub end: usize,
    /// One-based line containing start, including the final empty line at EOF.
    pub line: u64,
}
#[derive(Debug)]
pub struct Matcher(Regex);
impl Matcher {
    pub fn compile(pattern: &str, keep_going: &mut impl FnMut() -> bool) -> Result<Self, Error> {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        if pattern.len() > MAX_PATTERN {
            return Err(Error::PatternLimit);
        }
        let expression = RegexBuilder::new(pattern)
            .size_limit(MAX_NFA_BYTES)
            .dfa_size_limit(MAX_DFA_BYTES)
            .nest_limit(MAX_NESTING)
            .build();
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        expression.map(Self).map_err(|error| {
            let mut text = error.to_string();
            if text.len() > 1024 {
                let mut end = 1021;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                text.push('…');
            }
            Error::Invalid(text)
        })
    }
    /// The caller must validate the complete file and its revision separately.
    /// A successful None says only that this complete supplied text did not match.
    pub fn find(
        &self,
        text: &str,
        keep_going: &mut impl FnMut() -> bool,
    ) -> Result<Option<Match>, Error> {
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        if text.len() as u64 > crate::search_content::MAX_FILE {
            return Err(Error::TextLimit);
        }
        let found = self.0.find(text).map(|m| Match {
            start: m.start(),
            end: m.end(),
            line: 1 + text.as_bytes()[..m.start()]
                .iter()
                .filter(|&&b| b == b'\n')
                .count() as u64,
        });
        if !keep_going() {
            return Err(Error::Cancelled);
        }
        Ok(found)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn find(pattern: &str, text: &str) -> Option<Match> {
        Matcher::compile(pattern, &mut || true)
            .unwrap()
            .find(text, &mut || true)
            .unwrap()
    }
    #[test]
    fn first_match_uses_utf8_offsets_and_supports_file_spanning_patterns() {
        let text = "á\n日本\nend42";
        let m = find(r"(?s)日本.*[0-9]+", text).unwrap();
        assert_eq!(&text[m.start..m.end], "日本\nend42");
        assert_eq!(
            m,
            Match {
                start: 3,
                end: text.len(),
                line: 2
            }
        );
        assert_eq!(find(r"(?m)^end[0-9]+$", text).unwrap().line, 3);
        assert_eq!(find(r"end[0-9]+", "end1 end2").unwrap().end, 4);
        assert!(find("absent", text).is_none());
    }
    #[test]
    fn zero_width_and_eof_are_real_matches_including_empty_files() {
        assert_eq!(
            find("^$", ""),
            Some(Match {
                start: 0,
                end: 0,
                line: 1
            })
        );
        assert_eq!(
            find(r"\z", "á\n"),
            Some(Match {
                start: 3,
                end: 3,
                line: 2
            })
        );
        assert_eq!(
            find(r"(?-u:\b)", "word"),
            Some(Match {
                start: 0,
                end: 0,
                line: 1
            })
        );
        assert_eq!(find(r"a*", "bbb").unwrap().start, 0);
    }
    #[test]
    fn compile_and_input_limits_fail_explicitly_without_false_absence() {
        assert!(matches!(
            Matcher::compile(&"a".repeat(MAX_PATTERN + 1), &mut || true),
            Err(Error::PatternLimit)
        ));
        for pattern in ["(", r"\p{L}", r"(?i)a", r"(a)\1", "a{999999}"] {
            let Err(Error::Invalid(message)) = Matcher::compile(pattern, &mut || true) else {
                panic!("accepted {pattern}")
            };
            assert!(!message.is_empty() && message.len() <= 1024);
        }
        let nested = format!("{}a{}", "(".repeat(70), ")".repeat(70));
        assert!(matches!(
            Matcher::compile(&nested, &mut || true),
            Err(Error::Invalid(_))
        ));
        let diagnostic = "日".repeat(340) + "(";
        let Err(Error::Invalid(message)) = Matcher::compile(&diagnostic, &mut || true) else {
            panic!("expected bounded diagnostic");
        };
        assert!(message.len() <= 1024 && message.ends_with('…'));
        let regex = Matcher::compile("z", &mut || true).unwrap();
        let text = "x".repeat(crate::search_content::MAX_FILE as usize);
        assert_eq!(regex.find(&text, &mut || true), Ok(None));
        assert_eq!(
            find(r"\z", &text),
            Some(Match {
                start: text.len(),
                end: text.len(),
                line: 1
            })
        );
        assert_eq!(
            regex.find(&(text + "x"), &mut || true),
            Err(Error::TextLimit)
        );
    }
    #[test]
    fn cancellation_before_and_after_compile_and_search_discards_results() {
        assert!(matches!(
            Matcher::compile("a", &mut || false),
            Err(Error::Cancelled)
        ));
        let mut calls = 0;
        assert!(matches!(
            Matcher::compile("a", &mut || {
                calls += 1;
                calls == 1
            }),
            Err(Error::Cancelled)
        ));
        let regex = Matcher::compile("a", &mut || true).unwrap();
        assert_eq!(regex.find("a", &mut || false), Err(Error::Cancelled));
        calls = 0;
        assert_eq!(
            regex.find("a", &mut || {
                calls += 1;
                calls == 1
            }),
            Err(Error::Cancelled)
        );
    }
}
