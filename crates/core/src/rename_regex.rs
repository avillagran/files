//! Pure, bounded regex replacement for the rename planner, compiled once per
//! batch. Std/UTF-8 scalar matching only: regex's Unicode data table features
//! (separate Unicode-DFS license), DFA/backtracking/perf engines are disabled.
//! Unicode literals/dot/ranges still work; Unicode properties/perl classes and
//! case folding return an explicit compile error. Use [0-9] or (?-u:\w) for ASCII.
//! NFA size/nesting/capture caps contain this engine's state, not the
//! whole process's PSS. One bounded compile/search cannot be interrupted inside
//! regex; cooperative cancellation is checked before/after and between matches.
use regex::{Regex, RegexBuilder};
use std::fmt::{self, Write};

const MAX_BYTES: usize = 4096;
const MAX_NFA_BYTES: usize = 1024 * 1024;
const MAX_DFA_BYTES: usize = 256 * 1024;
const MAX_CAPTURES: usize = 64;
const MAX_PARTS: usize = 256;
const MAX_ERROR_BYTES: usize = 1024;

struct Diagnostic(String);
impl Write for Diagnostic {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = (MAX_ERROR_BYTES - 3).saturating_sub(self.0.len());
        let mut end = remaining.min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&value[..end]);
        if end == value.len() {
            Ok(())
        } else {
            Err(fmt::Error)
        }
    }
}
fn compile_error(error: impl fmt::Display) -> String {
    let mut message = Diagnostic(String::new());
    if write!(&mut message, "Regex no válida o fuera del límite: {error}").is_err() {
        message.0.push('…');
    }
    message.0
}
#[derive(Debug)]
enum Part<'a> {
    Literal(&'a str),
    Capture(Option<usize>),
}
pub(super) struct Prepared<'a> {
    expression: Regex,
    parts: Vec<Part<'a>>,
}
impl<'a> Prepared<'a> {
    pub(super) fn compile(
        pattern: &str,
        replacement: &'a str,
        check: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<Self, String> {
        check()?;
        if pattern.len() > MAX_BYTES || replacement.len() > MAX_BYTES {
            return Err("Regex o reemplazo demasiado largo".into());
        }
        let expression = RegexBuilder::new(pattern)
            .size_limit(MAX_NFA_BYTES)
            .dfa_size_limit(MAX_DFA_BYTES)
            .nest_limit(64)
            .build()
            .map_err(compile_error)?;
        check()?;
        if expression.captures_len() > MAX_CAPTURES + 1 {
            return Err("La regex supera 64 grupos de captura".into());
        }
        let mut parts = Vec::new();
        let mut remaining = replacement;
        while !remaining.is_empty() {
            check()?;
            if parts.len() == MAX_PARTS {
                return Err("El reemplazo supera 256 segmentos".into());
            }
            let Some(dollar) = remaining.find('$') else {
                parts.push(Part::Literal(remaining));
                break;
            };
            if dollar > 0 {
                parts.push(Part::Literal(&remaining[..dollar]));
                remaining = &remaining[dollar..];
                continue;
            }
            if remaining.starts_with("$$") {
                parts.push(Part::Literal("$"));
                remaining = &remaining[2..];
                continue;
            }
            let reference = if let Some(braced) = remaining.strip_prefix("${") {
                braced.find('}').map(|end| (&braced[..end], end + 3))
            } else {
                let length = remaining.as_bytes()[1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
                    .count();
                (length > 0).then(|| (&remaining[1..1 + length], length + 1))
            };
            let Some((name, consumed)) = reference else {
                parts.push(Part::Literal("$"));
                remaining = &remaining[1..];
                continue;
            };
            // Match Rust regex replacement semantics: unbraced references take
            // the longest ASCII alphanumeric/underscore sequence. Unknown or
            // unmatched captures expand to empty; use ${1}_suffix to delimit.
            let index = name
                .parse::<usize>()
                .ok()
                .filter(|n| *n < expression.captures_len())
                .or_else(|| {
                    expression
                        .capture_names()
                        .position(|known| known == Some(name))
                });
            parts.push(Part::Capture(index));
            remaining = &remaining[consumed..];
        }
        check()?;
        Ok(Self { expression, parts })
    }
    fn emit(
        &self,
        stem: &str,
        check: &mut impl FnMut() -> Result<(), String>,
        mut append: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut end = 0;
        check()?;
        for captures in self.expression.captures_iter(stem) {
            check()?;
            let whole = captures
                .get(0)
                .ok_or("Falta coincidencia completa de regex")?;
            append(&stem[end..whole.start()])?;
            for part in &self.parts {
                check()?;
                match part {
                    Part::Literal(value) => append(value)?,
                    Part::Capture(Some(index)) => {
                        if let Some(value) = captures.get(*index) {
                            append(value.as_str())?;
                        }
                    }
                    Part::Capture(None) => {}
                }
            }
            end = whole.end();
        }
        check()?;
        append(&stem[end..])
    }
    pub(super) fn replace(
        &self,
        stem: &str,
        check: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        if stem.len() > MAX_BYTES {
            return Err("Nombre de entrada demasiado largo".into());
        }
        let mut length = 0usize;
        // First pass visits borrowed slices only: reject expansion BEFORE any
        // output String is allocated, not after regex::Captures::expand grows it.
        self.emit(stem, check, |piece| {
            length = length
                .checked_add(piece.len())
                .filter(|n| *n <= MAX_BYTES)
                .ok_or("El reemplazo genera un nombre demasiado largo")?;
            Ok(())
        })?;
        check()?;
        let mut output = String::with_capacity(length);
        self.emit(stem, check, |piece| {
            // Recheck before each append; no change to the input or compiled
            // expression can normally differ between these two bounded passes.
            if output
                .len()
                .checked_add(piece.len())
                .is_none_or(|n| n > length)
            {
                return Err("La expansión regex cambió entre pasadas".into());
            }
            output.push_str(piece);
            Ok(())
        })?;
        check()?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn replace(pattern: &str, replacement: &str, value: &str) -> Result<String, String> {
        Prepared::compile(pattern, replacement, &mut || Ok(()))?.replace(value, &mut || Ok(()))
    }
    #[test]
    fn unicode_named_numbered_optional_and_literal_dollar_captures() {
        assert_eq!(
            replace(r"(?P<word>.+)-([0-9]+)", "${word}-$2-$$", "área-12").unwrap(),
            "área-12-$"
        );
        assert_eq!(
            replace(r"(a)?b", "$1-X-${missing}-$99", "b").unwrap(),
            "-X--"
        );
        assert_eq!(replace(r"(a)", "${1}_$1_x_$", "a").unwrap(), "a_$");
        assert_eq!(replace(r"(?P<名字>.+)", "${名字}", "漢字").unwrap(), "漢字");
    }
    #[test]
    fn empty_matches_keep_unicode_boundaries_and_do_not_skip_text() {
        assert_eq!(replace("", "-", "á漢").unwrap(), "-á-漢-");
        assert_eq!(replace("^|$", "_", "abc").unwrap(), "_abc_");
        assert_eq!(replace("a*", "X", "aáb").unwrap(), "XáXbX");
    }
    #[test]
    fn unicode_scalar_matching_works_without_unlicensed_property_tables() {
        assert_eq!(replace("[á-ÿ]", "X", "área").unwrap(), "Xrea");
        assert_eq!(replace(r"(?-u:\w+)", "X", "área").unwrap(), "áX");
        for pattern in [r"\p{L}", r"\d", r"\w", r"\b", "(?i:á)"] {
            assert!(replace(pattern, "X", "área1").is_err(), "{pattern}");
        }
    }
    #[test]
    fn expansion_and_engine_caps_fail_closed() {
        assert!(replace("a", &"x".repeat(4096), &"a".repeat(4096)).is_err());
        assert_eq!(replace("a", &"x".repeat(4096), "a").unwrap().len(), 4096);
        assert!(replace("(a)", &"$1".repeat(257), "a").is_err());
        for pattern in [
            "[".to_owned(),
            format!("{}a{}", "(".repeat(65), ")".repeat(65)),
            "(a)".repeat(65),
            "a{10000000}".to_owned(),
        ] {
            assert!(replace(&pattern, "x", "a").is_err(), "{pattern}");
        }
        assert!(replace(&"a".repeat(4097), "x", "a").is_err());
    }
    #[test]
    fn diagnostics_are_utf8_bounded_and_cancellation_is_checked_between_matches() {
        assert!(compile_error("漢".repeat(4096)).len() <= MAX_ERROR_BYTES);
        let expression = Prepared::compile("a", "$0$0", &mut || Ok(())).unwrap();
        let expected = expression.replace("aaa", &mut || Ok(())).unwrap();
        let mut checkpoints = 0;
        expression
            .replace("aaa", &mut || {
                checkpoints += 1;
                Ok(())
            })
            .unwrap();
        for stop in 0..checkpoints {
            let mut count = 0;
            assert_eq!(
                expression
                    .replace("aaa", &mut || {
                        let cancel = count == stop;
                        count += 1;
                        if cancel {
                            Err("cancelled".into())
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_err(),
                "cancelled"
            );
        }
        assert_eq!(expression.replace("aaa", &mut || Ok(())).unwrap(), expected);
    }
}
