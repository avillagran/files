//! Bounded, filesystem-free local name filter. Paths and operations keep the
//! original OsString; lossy display text is used only for matching.
use crate::listing::{Entry, Kind};

pub const MAX_BYTES: usize = 4096;
pub const MAX_TERMS: usize = 32;
#[derive(Clone, Copy)]
enum Pattern<'a> {
    Fuzzy(&'a str),
    Literal(&'a str),
    Glob(&'a str),
    Extension(&'a str),
    Kind(Option<Kind>),
    Metadata(crate::search_predicate::Predicate),
    Content,
}
#[derive(Clone, Copy)]
struct Term<'a> {
    pattern: Pattern<'a>,
    exclude: bool,
}
pub struct Filter<'a> {
    terms: [Option<Term<'a>>; MAX_TERMS],
    error: Option<&'static str>,
    content: Option<String>,
    regex: Option<crate::search_regex::Matcher>,
    regex_error: Option<String>,
}
impl<'a> Filter<'a> {
    pub fn new(query: &'a str) -> Self {
        Self::parse(query, false)
    }
    pub fn for_search(query: &'a str) -> Self {
        Self::parse(query, true)
    }
    fn parse(query: &'a str, metadata: bool) -> Self {
        let mut result = Self {
            terms: [None; MAX_TERMS],
            error: None,
            content: None,
            regex: None,
            regex_error: None,
        };
        if query.len() > MAX_BYTES {
            result.error = Some("Filtrar: máximo 4096 bytes");
            return result;
        }
        let mut rest = query.trim();
        let mut count = 0;
        while !rest.is_empty() {
            if count == MAX_TERMS {
                result.error = Some("Filtrar: máximo 32 términos");
                break;
            }
            let head = rest.trim_start_matches(['!', '-']);
            let content_token = head.starts_with("content:") || head.starts_with("regex:");
            let mut quoted = false;
            let mut escaped = false;
            let end = rest
                .char_indices()
                .find_map(|(i, c)| {
                    if content_token && quoted && escaped {
                        escaped = false;
                        return None;
                    }
                    if content_token && quoted && c == '\\' {
                        escaped = true;
                        return None;
                    }
                    if c == '"' {
                        quoted = !quoted;
                    }
                    (c.is_whitespace() && !quoted).then_some(i)
                })
                .unwrap_or(rest.len());
            let mut text = &rest[..end];
            rest = rest[end..].trim_start();
            let exclude = text.len() > 1 && (text.starts_with('!') || text.starts_with('-'));
            if exclude {
                text = &text[1..];
            }
            let facet = text
                .split_once(':')
                .filter(|(p, _)| matches!(*p, "size" | "after" | "before"));
            let pattern = if let Some(value) = text
                .strip_prefix("content:")
                .or_else(|| text.strip_prefix("regex:"))
            {
                if !metadata {
                    result.error =
                        Some("Contenido: usa Buscar (Ctrl+Shift+F); name: busca el nombre literal");
                }
                if exclude || result.content.is_some() {
                    result.error = Some("Usa un solo content: o regex: positivo");
                }
                match crate::search_content::literal(value) {
                    Ok(value) => {
                        if text.starts_with("regex:") {
                            match crate::search_regex::Matcher::compile(&value, &mut || true) {
                                Ok(regex) => result.regex = Some(regex),
                                Err(crate::search_regex::Error::Invalid(message)) => {
                                    let reason = message
                                        .lines()
                                        .rev()
                                        .find(|line| !line.trim().is_empty())
                                        .unwrap_or("expresión no válida");
                                    result.regex_error = Some(format!(
                                        "Regex: {}",
                                        reason.trim().trim_start_matches("error: ")
                                    ))
                                }
                                Err(_) => {
                                    result.regex_error =
                                        Some("Regex fuera de los límites admitidos".into())
                                }
                            }
                        }
                        result.content = Some(value);
                    }
                    Err(error) => result.error = Some(error),
                }
                Pattern::Content
            } else if let Some((prefix, value)) = facet {
                if !metadata {
                    result.error = Some(
                        "Tamaño/fecha: usa Buscar (Ctrl+Shift+F); name: busca el nombre literal",
                    );
                }
                match crate::search_predicate::Predicate::parse(prefix, value) {
                    Some(p) => Pattern::Metadata(p),
                    None => {
                        result.error = Some(
                            "Usa size:>10MiB o size:1MB..2MB; after:/before:AAAA-MM-DD o AAAA-MM-DDTHH:MM:SSZ (UTC)",
                        );
                        Pattern::Literal("")
                    }
                }
            } else if let Some(kind) = text.strip_prefix("type:") {
                let kind = match kind {
                    "dir" | "folder" | "carpeta" => Some(Kind::Directory),
                    "file" | "archivo" => Some(Kind::File),
                    "link" | "enlace" => Some(Kind::Symlink),
                    "other" | "otro" => Some(Kind::Other),
                    _ => None,
                };
                if kind.is_none() {
                    result.error = Some("type: usa dir, file, link u other");
                }
                Pattern::Kind(kind)
            } else if let Some(ext) = text.strip_prefix("ext:") {
                Pattern::Extension(ext.trim_matches('"'))
            } else {
                text = text.strip_prefix("name:").unwrap_or(text);
                if text.starts_with('"') {
                    Pattern::Literal(text.trim_matches('"'))
                } else if text.contains(['*', '?']) {
                    Pattern::Glob(text)
                } else {
                    Pattern::Fuzzy(text)
                }
            };
            result.terms[count] = Some(Term { pattern, exclude });
            count += 1;
        }
        result
    }
    pub fn error(&self) -> Option<&str> {
        self.error.or(self.regex_error.as_deref())
    }
    pub fn regex(&self) -> Option<&crate::search_regex::Matcher> {
        self.regex.as_ref()
    }
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }
    pub fn needs_metadata(&self) -> bool {
        self.terms
            .iter()
            .flatten()
            .any(|t| matches!(t.pattern, Pattern::Metadata(_)))
    }
    pub fn candidate_score(&self, entry: &Entry) -> Option<i32> {
        self.evaluate(entry, true)
    }
    pub fn score(&self, entry: &Entry) -> Option<i32> {
        self.evaluate(entry, false)
    }
    fn evaluate(&self, entry: &Entry, candidate: bool) -> Option<i32> {
        if self.error().is_some() {
            return None;
        }
        if self.content.is_some() && entry.kind != Kind::File {
            return None;
        }
        let name = entry.name.to_string_lossy();
        let mut total = 0i32;
        for term in self.terms.iter().flatten() {
            if let Pattern::Metadata(predicate) = term.pattern {
                if predicate.is_size() && entry.kind != Kind::File {
                    return None;
                }
                if candidate {
                    continue;
                }
                let metadata = entry.metadata.as_ref()?;
                let matched = predicate.matches(metadata.size, metadata.modified);
                if matched == term.exclude {
                    return None;
                }
                continue;
            }
            let score = match term.pattern {
                Pattern::Content => continue,
                Pattern::Metadata(_) => unreachable!(),
                Pattern::Fuzzy(text) => crate::listing::fuzzy_score(&name, text),
                Pattern::Literal(text) => contains_folded(&name, text).then_some(100),
                Pattern::Glob(pattern) => glob(&name, pattern).then_some(100),
                Pattern::Extension(exts) => name
                    .rsplit_once('.')
                    .filter(|(stem, _)| !stem.is_empty())
                    .filter(|(_, ext)| {
                        exts.split(',')
                            .any(|q| equal_folded(ext, q.trim_start_matches('.')))
                    })
                    .map(|_| 100),
                Pattern::Kind(kind) => (kind == Some(entry.kind)).then_some(0),
            };
            if term.exclude {
                score.is_none().then_some(())?;
            } else {
                total = total.saturating_add(score?);
            }
        }
        Some(total)
    }
}
fn equal_folded(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}
fn contains_folded(name: &str, text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    let mut chars = name.chars().flat_map(char::to_lowercase);
    while chars.clone().next().is_some() {
        let mut candidate = chars.clone();
        if text
            .chars()
            .flat_map(char::to_lowercase)
            .all(|c| candidate.next() == Some(c))
        {
            return true;
        }
        chars.next();
    }
    false
}
/// Full-name glob: '?' consumes one Unicode scalar, '*' any sequence. No regex,
/// recursion, DP table or per-file allocation. ASCII folding matches shell names;
/// literal Unicode characters compare case-insensitively too.
fn glob(mut name: &str, mut pattern: &str) -> bool {
    let mut star: Option<(&str, &str)> = None;
    while !name.is_empty() {
        let p = pattern.chars().next();
        if p == Some('*') {
            pattern = &pattern[1..];
            star = Some((name, pattern));
            continue;
        }
        let c = name.chars().next().unwrap();
        if p.is_some_and(|p| p == '?' || p.to_lowercase().eq(c.to_lowercase())) {
            name = &name[c.len_utf8()..];
            pattern = &pattern[p.unwrap().len_utf8()..];
        } else if let Some((retry, suffix)) = star {
            if retry.is_empty() {
                return false;
            }
            let retry = &retry[retry.chars().next().unwrap().len_utf8()..];
            name = retry;
            pattern = suffix;
            star = Some((retry, suffix));
        } else {
            return false;
        }
    }
    pattern.chars().all(|c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(name: &str, kind: Kind) -> Entry {
        Entry {
            name: name.into(),
            kind,
            metadata: None,
            stat_attempted: false,
        }
    }
    fn matches(name: &str, query: &str) -> bool {
        Filter::new(query).score(&entry(name, Kind::File)).is_some()
    }
    #[test]
    fn regex_content_is_explicit_compiled_once_and_rejects_ambiguous_modes() {
        let filter = Filter::for_search(r#"ext:txt regex:"(?m)^error[0-9]+$""#);
        assert!(filter.error().is_none());
        assert_eq!(filter.content(), Some("(?m)^error[0-9]+$"));
        assert!(filter.regex().is_some());
        assert!(Filter::for_search("content:a.*b").regex().is_none());
        for q in [
            "regex:(",
            r"regex:\p{L}",
            "!regex:a",
            "content:a regex:b",
            "regex:a content:b",
            "regex:a regex:b",
        ] {
            assert!(Filter::for_search(q).error().is_some(), "{q}");
        }
        let invalid = Filter::for_search("regex:(");
        assert!(invalid.error().unwrap().contains("unclosed group"));
        assert!(!invalid.error().unwrap().contains('\n'));
        assert!(Filter::new("regex:a").error().is_some());
        assert!(Filter::for_search("name:regex:a").content().is_none());
    }
    #[test]
    fn content_is_search_only_regular_exact_bounded_and_combines_with_facets() {
        let f = Filter::for_search(r#"ext:txt size:>1B content:"Hola \"mundo\"""#);
        assert!(f.error().is_none(), "{:?}", f.error());
        assert_eq!(f.content(), Some("Hola \"mundo\""));
        assert!(f.needs_metadata());
        assert!(Filter::new("content:needle").error().is_some());
        for bad in [
            "!content:a",
            "content:a content:b",
            r#"content:"""#,
            r#"content:"open"#,
            r#"content:"bad\q""#,
        ] {
            assert!(Filter::for_search(bad).error().is_some(), "{bad}");
        }
        assert!(
            Filter::for_search(&format!("content:{}", "a".repeat(1025)))
                .error()
                .is_some()
        );
        assert!(Filter::for_search("name:content:a").content().is_none());
    }
    #[test]
    fn search_metadata_requires_observation_even_for_negation() {
        use crate::listing::Metadata;
        use std::time::{Duration, UNIX_EPOCH};
        let mut e = entry("report.pdf", Kind::File);
        for query in ["size:>=1KiB", "!size:<1KiB", "after:1970-01-01"] {
            let f = Filter::for_search(query);
            assert!(f.error().is_none());
            assert!(f.candidate_score(&e).is_some());
            assert!(f.score(&e).is_none());
            assert!(Filter::new(query).error().is_some());
        }
        e.metadata = Some(Metadata {
            size: 1024,
            modified: UNIX_EPOCH + Duration::from_secs(86400),
        });
        assert!(
            Filter::for_search("ext:pdf size:1KiB..2KiB after:1970-01-02 before:1970-01-03")
                .score(&e)
                .is_some()
        );
        assert!(Filter::for_search("before:1970-01-02").score(&e).is_none());
        e.kind = Kind::Directory;
        assert!(
            Filter::for_search("!size:<1KiB")
                .candidate_score(&e)
                .is_none()
        );
        assert!(Filter::new("name:size:>1MiB").error().is_none());
    }
    #[test]
    fn words_are_independent_and_negations_exclude() {
        assert!(matches("2026-final-report.pdf", "report 2026 !draft"));
        assert!(!matches("draft-report-2026.pdf", "report 2026 !draft"));
        assert!(matches("file-00123.txt", "f123"));
        assert!(matches("anything", "   "));
    }
    #[test]
    fn extensions_kinds_quotes_and_unicode_are_composable() {
        assert!(matches(
            "Informe anual.PDF",
            "\"informe anual\" ext:pdf,md type:file"
        ));
        assert!(!matches("informe-muy-anual.pdf", "\"informe anual\""));
        assert!(matches("Árbol日本.rs", "árbol ext:rs"));
        assert!(
            Filter::new("type:dir !name:cache")
                .score(&entry("Documents", Kind::Directory))
                .is_some()
        );
        assert!(!matches(".bashrc", "ext:bashrc"));
        assert!(Filter::new("type:banana").error().is_some());
    }
    #[test]
    fn full_name_globs_use_unicode_boundaries_and_backtrack() {
        assert!(matches("note-日本.txt", "note-??.txt"));
        assert!(!matches("note-日本.txt.old", "*.txt"));
        assert!(matches("ababab-end.txt", "*ab*end.?xt"));
        assert!(matches("README.MD", "*.md"));
        assert!(matches("photo.jpg", "* !*.tmp"));
        assert!(!matches("photo.tmp", "* !*.tmp"));
    }
    #[test]
    fn query_bounds_fail_closed_before_allocating_terms() {
        assert!(Filter::new(&"x".repeat(4097)).error().is_some());
        assert!(
            Filter::new(&"a ".repeat(33))
                .score(&entry("a", Kind::File))
                .is_none()
        );
        assert!(Filter::new(&"a ".repeat(32)).error().is_none());
    }
    #[test]
    fn unfinished_quote_is_useful_while_typing() {
        assert!(matches("annual report.txt", "name:\"annual report"));
        assert!(!matches("annual unrelated report.txt", "\"annual report"));
    }
    #[cfg(unix)]
    #[test]
    fn matching_keeps_opaque_names_intact() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let e = Entry {
            name: std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec()),
            ..entry("", Kind::File)
        };
        assert!(Filter::new("raw ext:txt").score(&e).is_some());
        assert_eq!(e.name.as_bytes(), b"raw-\xff.txt");
    }
}
