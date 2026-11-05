//! A small, dependency-free code highlighter: diffs by line, and for
//! common languages comments, strings, numbers and keywords. It is a
//! lexer, not a grammar: good enough for chat transcripts, replaceable
//! through [`MarkdownOptions::highlight`](super::markdown::MarkdownOptions).

use super::doc::ColorRole;
use std::ops::Range;

const KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "def",
    "default",
    "defer",
    "del",
    "do",
    "elif",
    "else",
    "enum",
    "except",
    "export",
    "extends",
    "extern",
    "false",
    "finally",
    "fn",
    "for",
    "from",
    "func",
    "function",
    "go",
    "if",
    "impl",
    "import",
    "in",
    "interface",
    "is",
    "let",
    "loop",
    "match",
    "mod",
    "module",
    "mut",
    "new",
    "nil",
    "None",
    "not",
    "null",
    "or",
    "and",
    "package",
    "pass",
    "pub",
    "raise",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "switch",
    "this",
    "throw",
    "trait",
    "true",
    "True",
    "False",
    "try",
    "type",
    "typeof",
    "undefined",
    "unsafe",
    "use",
    "var",
    "void",
    "where",
    "while",
    "with",
    "yield",
    "local",
    "then",
    "end",
    "elseif",
    "fi",
    "esac",
    "done",
    "echo",
    "select",
    "insert",
    "update",
    "delete",
    "into",
    "values",
    "create",
    "table",
    "begin",
    "private",
    "protected",
    "public",
];

#[derive(Clone, Copy, PartialEq)]
enum Comments {
    Slash,
    Hash,
    Dash,
    Semi,
}

/// Highlights `code` written in `lang` (the fence info word). Unknown or
/// empty languages get no colors.
pub fn basic(lang: &str, code: &str) -> Vec<(Range<usize>, ColorRole)> {
    let lang = lang.to_ascii_lowercase();
    if matches!(lang.as_str(), "diff" | "patch") {
        return diff(code);
    }
    let comments = match lang.as_str() {
        "rust" | "rs" | "c" | "h" | "cpp" | "c++" | "cc" | "hpp" | "js" | "javascript" | "jsx"
        | "ts" | "typescript" | "tsx" | "go" | "java" | "kotlin" | "kt" | "swift" | "cs"
        | "csharp" | "scala" | "dart" | "zig" | "php" | "qml" | "json" | "jsonc" | "css"
        | "scss" | "glsl" | "wgsl" | "proto" => Comments::Slash,
        "python" | "py" | "sh" | "bash" | "zsh" | "fish" | "shell" | "console" | "ruby" | "rb"
        | "yaml" | "yml" | "toml" | "perl" | "r" | "make" | "makefile" | "dockerfile" | "nix"
        | "conf" | "elixir" | "ex" | "cmake" | "ps1" | "powershell" => Comments::Hash,
        "sql" | "lua" | "haskell" | "hs" | "elm" => Comments::Dash,
        "lisp" | "clojure" | "clj" | "scheme" | "asm" | "ini" => Comments::Semi,
        _ => return Vec::new(),
    };
    let rust = matches!(lang.as_str(), "rust" | "rs");
    let backticks = matches!(
        lang.as_str(),
        "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx" | "go" | "sh" | "bash" | "zsh"
    );
    lex(code, comments, rust, backticks)
}

fn diff(code: &str) -> Vec<(Range<usize>, ColorRole)> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in code.split_inclusive('\n') {
        let role = if line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            Some(ColorRole::Dim)
        } else if line.starts_with('+') {
            Some(ColorRole::Green)
        } else if line.starts_with('-') {
            Some(ColorRole::Red)
        } else if line.starts_with("@@") {
            Some(ColorRole::Cyan)
        } else {
            None
        };
        let body = line.trim_end_matches('\n').len();
        if let Some(r) = role {
            out.push((at..at + body, r));
        }
        at += line.len();
    }
    out
}

fn lex(
    code: &str,
    comments: Comments,
    rust: bool,
    backticks: bool,
) -> Vec<(Range<usize>, ColorRole)> {
    let b = code.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    let line_end = |i: usize| code[i..].find('\n').map_or(n, |e| i + e);
    while i < n {
        let c = b[i];
        // Comments.
        let line_comment = match comments {
            Comments::Slash => c == b'/' && b.get(i + 1) == Some(&b'/'),
            Comments::Hash => c == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace()),
            Comments::Dash => c == b'-' && b.get(i + 1) == Some(&b'-'),
            Comments::Semi => c == b';',
        };
        if line_comment {
            let e = line_end(i);
            out.push((i..e, ColorRole::Faint));
            i = e;
            continue;
        }
        if comments == Comments::Slash && c == b'/' && b.get(i + 1) == Some(&b'*') {
            let e = code[i + 2..].find("*/").map_or(n, |e| i + 2 + e + 2);
            out.push((i..e, ColorRole::Faint));
            i = e;
            continue;
        }
        // Strings.
        if c == b'"' || (c == b'`' && backticks) || c == b'\'' {
            // A Rust lifetime ('a) is not a char literal.
            if c == b'\'' && rust {
                let close = code[i + 1..]
                    .char_indices()
                    .take(4)
                    .find(|(_, ch)| *ch == '\'');
                if close.is_none() || b.get(i + 1) == Some(&b'\'') {
                    i += 1;
                    continue;
                }
            }
            let mut j = i + 1;
            while j < n && b[j] != c && (c == b'`' || b[j] != b'\n') {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let e = (j + 1).min(n);
            out.push((i..e, ColorRole::Green));
            i = e;
            continue;
        }
        // Numbers and words.
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'.' || b[j] == b'_') {
                j += 1;
            }
            out.push((i..j, ColorRole::Yellow));
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let mut j = i;
            while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let w = &code[i..j];
            if KEYWORDS.contains(&w) {
                out.push((i..j, ColorRole::Magenta));
            } else if w.as_bytes()[0].is_ascii_uppercase() && w.len() > 1 {
                out.push((i..j, ColorRole::Cyan));
            } else if b.get(j) == Some(&b'(') || (rust && b.get(j) == Some(&b'!')) {
                out.push((i..j, ColorRole::Blue));
            }
            i = j;
            continue;
        }
        // Skip one UTF-8 char.
        i += code[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lines_and_code_tokens() {
        let d = basic("diff", "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same");
        assert_eq!(
            d.iter().map(|(_, c)| *c).collect::<Vec<_>>(),
            vec![
                ColorRole::Dim,
                ColorRole::Dim,
                ColorRole::Cyan,
                ColorRole::Red,
                ColorRole::Green
            ]
        );
        let code = "fn main() { let s = \"hi\"; // note\n 42 }";
        let r = basic("rust", code);
        let role = |s: &str| {
            r.iter()
                .find(|(rg, _)| &code[rg.clone()] == s)
                .map(|(_, c)| *c)
        };
        assert_eq!(role("fn"), Some(ColorRole::Magenta));
        assert_eq!(role("\"hi\""), Some(ColorRole::Green));
        assert_eq!(role("// note"), Some(ColorRole::Faint));
        assert_eq!(role("42"), Some(ColorRole::Yellow));
        assert_eq!(role("main"), Some(ColorRole::Blue));
        assert!(basic("", code).is_empty() && basic("brainfuck", code).is_empty());
        // Lifetimes are not strings; ranges stay on char boundaries.
        let r = basic("rust", "fn f<'a>(x: &'a str) -> char { 'é' }");
        assert!(r
            .iter()
            .any(|(rg, c)| *c == ColorRole::Green && rg.len() == 4));
    }
}
