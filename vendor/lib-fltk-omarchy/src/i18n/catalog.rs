//! Flat translation catalogs in o3's format:
//! `{ "<locale>": { "<English source>": "<translation>" } }`.
//!
//! Plural forms are extra keys with a `|category` suffix next to the plain
//! key, which stays the `other` form, so every value is still a string and
//! a catalog without plurals is unchanged:
//!
//! ```json
//! { "en": { "{count} files": "{count} files", "{count} files|one": "{count} file" },
//!   "ru": { "{count} files": "{count} файла", "{count} files|one": "{count} файл",
//!           "{count} files|few": "{count} файла", "{count} files|many": "{count} файлов" } }
//! ```

use super::locale;
use super::plural::{self, Plural};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

/// Locale → (source → translation). Locale keys are normalized (`fr-ca`).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Catalog {
    locales: BTreeMap<String, HashMap<String, String>>,
}

/// A catalog that is not o3's flat format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError(pub String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "translation catalog: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog::default()
    }

    /// Parses o3's flat JSON. Every value must be a string.
    pub fn from_json(json: &str) -> Result<Catalog, CatalogError> {
        let v: Value = serde_json::from_str(json).map_err(|e| CatalogError(e.to_string()))?;
        let Value::Object(root) = v else {
            return Err(CatalogError("the root must be an object of locales".into()));
        };
        let mut c = Catalog::new();
        for (loc, strings) in root {
            let Value::Object(strings) = strings else {
                return Err(CatalogError(format!("locale \"{loc}\" must be an object")));
            };
            let tag = locale::normalize(&loc);
            if tag.is_empty() {
                return Err(CatalogError(format!("\"{loc}\" is not a locale")));
            }
            let map = c.locales.entry(tag).or_default();
            for (k, val) in strings {
                let Value::String(s) = val else {
                    return Err(CatalogError(format!(
                        "\"{loc}\" / \"{k}\" must be a string"
                    )));
                };
                map.insert(k, s);
            }
        }
        Ok(c)
    }

    /// Reads and parses a catalog file.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Catalog, CatalogError> {
        let p = path.as_ref();
        let s = std::fs::read_to_string(p)
            .map_err(|e| CatalogError(format!("{}: {e}", p.display())))?;
        Catalog::from_json(&s).map_err(|e| CatalogError(format!("{}: {}", p.display(), e.0)))
    }

    /// Adds `other` on top: its strings replace the ones here.
    pub fn merge(&mut self, other: &Catalog) {
        for (loc, strings) in &other.locales {
            let map = self.locales.entry(loc.clone()).or_default();
            for (k, v) in strings {
                map.insert(k.clone(), v.clone());
            }
        }
    }

    pub fn insert(&mut self, locale: &str, source: &str, translation: &str) {
        self.locales
            .entry(locale::normalize(locale))
            .or_default()
            .insert(source.into(), translation.into());
    }

    /// Locales present (normalized, sorted).
    pub fn locales(&self) -> Vec<String> {
        self.locales.keys().cloned().collect()
    }

    pub fn has_locale(&self, tag: &str) -> bool {
        self.locales.contains_key(tag)
    }

    /// Exact entry (empty strings count as missing, like o3).
    pub fn get(&self, locale: &str, key: &str) -> Option<&str> {
        self.locales
            .get(locale)?
            .get(key)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    /// Keys of one locale.
    pub fn keys(&self, locale: &str) -> Vec<String> {
        let mut k: Vec<String> = self
            .locales
            .get(locale)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        k.sort();
        k
    }

    /// Lookup order for a requested list (`fr_CA.UTF-8:en`): each entry and
    /// its shorter forms (`fr-ca`, `fr`), then English.
    pub fn chain(requested: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in locale::parse_list(requested)
            .iter()
            .map(|t| locale::fallbacks(t))
            .chain([vec!["en".to_string()]])
        {
            for l in t {
                if !out.contains(&l) {
                    out.push(l);
                }
            }
        }
        out
    }

    /// The template for `source` along `chain` and the locale it came from;
    /// with `count`, the `source|category` form of that locale first.
    pub fn lookup<'a>(
        &'a self,
        chain: &'a [String],
        source: &str,
        count: Option<i64>,
    ) -> Option<(&'a str, &'a str)> {
        Catalog::lookup_in(&[self], chain, source, count)
    }

    /// [`Catalog::lookup`] over layered catalogs, first one winning, with
    /// the same result as looking up in their merge (last merged = first here).
    pub fn lookup_in<'a>(
        layers: &[&'a Catalog],
        chain: &'a [String],
        source: &str,
        count: Option<i64>,
    ) -> Option<(&'a str, &'a str)> {
        for l in chain {
            let find = |key: &str| layers.iter().find_map(|c| c.get(l, key));
            if let Some(n) = count {
                let cat = plural::category(l, n);
                if cat != Plural::Other {
                    if let Some(s) = find(&format!("{source}|{}", cat.as_str())) {
                        return Some((l.as_str(), s));
                    }
                }
            }
            if let Some(s) = find(source) {
                return Some((l.as_str(), s));
            }
        }
        None
    }

    /// o3's `translate`: the string for the first locale in `requested` (a
    /// POSIX or BCP 47 list) that has it, by itself or by its language, then
    /// English, then `source` itself.
    pub fn translate(&self, requested: &str, source: &str) -> String {
        let chain = Catalog::chain(requested);
        self.lookup(&chain, source, None)
            .map(|(_, s)| s.to_string())
            .unwrap_or_else(|| source.to_string())
    }

    /// The first requested locale the catalog has (itself or its language),
    /// else the first requested, else `en`. Same choice as o3's `resolve`.
    pub fn resolve(&self, requested: &str) -> String {
        Catalog::resolve_in(requested, |t| self.locales.contains_key(t))
    }

    /// [`Catalog::resolve`] for catalogs that have the locales `has` accepts.
    pub fn resolve_in(requested: &str, has: impl Fn(&str) -> bool) -> String {
        let list = locale::parse_list(requested);
        list.iter()
            .find(|t| locale::fallbacks(t).iter().any(|f| has(f)))
            .or(list.first())
            .cloned()
            .unwrap_or_else(|| "en".into())
    }

    /// Consistency report against `reference` (usually `en`): missing and
    /// extra keys, placeholder mismatches, missing or unused plural forms.
    pub fn check(&self, reference: &str) -> Report {
        let mut issues = Vec::new();
        let empty = HashMap::new();
        let refm = self.locales.get(reference).unwrap_or(&empty);
        if refm.is_empty() {
            issues.push(Issue {
                locale: reference.into(),
                key: String::new(),
                kind: IssueKind::Missing,
            });
        }
        let split = |k: &str| -> (String, Option<Plural>) {
            match k
                .rsplit_once('|')
                .and_then(|(b, c)| Plural::parse(c).map(|p| (b.to_string(), p)))
            {
                Some((b, p)) => (b, Some(p)),
                None => (k.to_string(), None),
            }
        };
        let base_keys: BTreeSet<String> = refm.keys().map(|k| split(k).0).collect();
        // A key is plural when the reference has any `|category` form of it.
        let plural_keys: BTreeSet<String> = refm
            .keys()
            .filter_map(|k| match split(k) {
                (b, Some(_)) => Some(b),
                _ => None,
            })
            .collect();
        for (loc, map) in &self.locales {
            let cats = plural::categories(loc);
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                let v = &map[k];
                let (base, cat) = split(k);
                if !refm.contains_key(&base) {
                    issues.push(Issue {
                        locale: loc.clone(),
                        key: k.clone(),
                        kind: IssueKind::Extra,
                    });
                    continue;
                }
                if v.is_empty() {
                    issues.push(Issue {
                        locale: loc.clone(),
                        key: k.clone(),
                        kind: IssueKind::Empty,
                    });
                    continue;
                }
                if let Some(c) = cat {
                    if !cats.contains(&c) || c == Plural::Other {
                        issues.push(Issue {
                            locale: loc.clone(),
                            key: k.clone(),
                            kind: IssueKind::UnusedPlural(c),
                        });
                    }
                }
                let want = placeholders(&base);
                let got = placeholders(v);
                // A plural form may spell the number out ("عنصر واحد"): {count} is optional there.
                let missing: Vec<String> = want
                    .difference(&got)
                    .filter(|p| !(cat.is_some() && p.as_str() == "count"))
                    .cloned()
                    .collect();
                let unknown: Vec<String> = got.difference(&want).cloned().collect();
                if !missing.is_empty() || !unknown.is_empty() {
                    issues.push(Issue {
                        locale: loc.clone(),
                        key: k.clone(),
                        kind: IssueKind::Placeholders { missing, unknown },
                    });
                }
            }
            for k in &base_keys {
                if !map.contains_key(k) {
                    issues.push(Issue {
                        locale: loc.clone(),
                        key: k.clone(),
                        kind: IssueKind::Missing,
                    });
                } else if plural_keys.contains(k) {
                    for c in cats.iter().filter(|c| **c != Plural::Other) {
                        if !map.contains_key(&format!("{k}|{}", c.as_str())) {
                            issues.push(Issue {
                                locale: loc.clone(),
                                key: k.clone(),
                                kind: IssueKind::MissingPlural(*c),
                            });
                        }
                    }
                }
            }
        }
        Report { issues }
    }
}

/// One problem found by [`Catalog::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub locale: String,
    pub key: String,
    pub kind: IssueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueKind {
    /// The key is in the reference locale but not here (or the reference is empty).
    Missing,
    /// The key is not in the reference locale.
    Extra,
    /// Empty translation (the lookup skips it).
    Empty,
    /// `{name}`s that differ from the source key.
    Placeholders {
        missing: Vec<String>,
        unknown: Vec<String>,
    },
    /// A plural key lacks this category for the locale's language.
    MissingPlural(Plural),
    /// A `|category` the locale's language never uses for whole numbers.
    UnusedPlural(Plural),
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (l, k) = (&self.locale, &self.key);
        match &self.kind {
            IssueKind::Missing if k.is_empty() => {
                write!(f, "{l}: reference locale missing or empty")
            }
            IssueKind::Missing => write!(f, "{l}: missing \"{k}\""),
            IssueKind::Extra => write!(f, "{l}: extra \"{k}\" (not in the reference)"),
            IssueKind::Empty => write!(f, "{l}: empty \"{k}\""),
            IssueKind::Placeholders { missing, unknown } => {
                write!(f, "{l}: \"{k}\" placeholders")?;
                if !missing.is_empty() {
                    write!(f, " missing {{{}}}", missing.join("}, {"))?;
                }
                if !unknown.is_empty() {
                    write!(f, " unknown {{{}}}", unknown.join("}, {"))?;
                }
                Ok(())
            }
            IssueKind::MissingPlural(c) => write!(
                f,
                "{l}: \"{k}\" lacks the plural form \"{k}|{}\"",
                c.as_str()
            ),
            IssueKind::UnusedPlural(c) => write!(
                f,
                "{l}: \"{k}\" — \"{}\" is not a plural form of this language",
                c.as_str()
            ),
        }
    }
}

/// Result of [`Catalog::check`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in &self.issues {
            writeln!(f, "{i}")?;
        }
        Ok(())
    }
}

/// Names of the `{name}` placeholders in `s` (`[A-Za-z_][A-Za-z0-9_]*`).
pub fn placeholders(s: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        let tail = &rest[open + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        let name = &tail[..end];
        if valid_name(name) && tail[end..].starts_with('}') {
            out.insert(name.to_string());
        }
        rest = tail;
    }
    out
}

fn valid_name(n: &str) -> bool {
    !n.is_empty() && !n.starts_with(|c: char| c.is_ascii_digit())
}

/// Replaces `{name}` with the matching argument; unknown names and
/// malformed braces stay as written. With `isolate`, each value is wrapped
/// in U+2068/U+2069 (first-strong isolate) so a Latin name or a number
/// keeps its own direction inside right-to-left text.
pub fn format(template: &str, args: &[(&str, &dyn fmt::Display)], isolate: bool) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        let name = &tail[..end];
        let hit = (valid_name(name) && tail[end..].starts_with('}'))
            .then(|| args.iter().find(|(k, _)| *k == name))
            .flatten();
        match hit {
            Some((_, v)) => {
                if isolate {
                    out.push('\u{2068}');
                }
                use fmt::Write;
                let _ = write!(out, "{v}");
                if isolate {
                    out.push('\u{2069}');
                }
                rest = &tail[end + 1..];
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Keys used in Rust source: the string literals passed to `tr`, `tr!`,
/// `tr_with`, `trn`, `trn!` and `i18n::text` (for "unlisted string" checks).
/// Comment lines (doc examples) and `#[cfg(test)]` items are skipped.
pub fn source_keys(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut kept = Vec::new();
    // Inside a #[cfg(test)] item: (seen its opening brace, brace depth).
    let mut test: Option<(bool, i32)> = None;
    for l in src.lines() {
        let t = l.trim_start();
        if let Some((opened, depth)) = test.as_mut() {
            *depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
            *opened |= l.contains('{');
            if *opened && *depth <= 0 {
                test = None;
            }
            continue;
        }
        if t.starts_with("#[cfg(test)]") {
            test = Some((false, 0));
        } else if !t.starts_with("//") {
            kept.push(l);
        }
    }
    let code = kept.join("\n");
    for pat in [
        "tr(\"",
        "tr!(\"",
        "tr_with(\"",
        "trn(\"",
        "trn!(\"",
        "i18n::text(\"",
    ] {
        let mut rest = code.as_str();
        while let Some(i) = rest.find(pat) {
            let prev = rest[..i].chars().last();
            rest = &rest[i + pat.len()..];
            if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let mut s = String::new();
            let mut chars = rest.chars();
            while let Some(ch) = chars.next() {
                match ch {
                    '"' => break,
                    '\\' => match chars.next() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('u') => {
                            // \u{2068}
                            let hex: String =
                                chars.by_ref().skip(1).take_while(|c| *c != '}').collect();
                            if let Some(c) =
                                u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                            {
                                s.push(c)
                            }
                        }
                        Some(o) => s.push(o),
                        None => {}
                    },
                    c => s.push(c),
                }
            }
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const O3: &str = r#"{"en":{"Close":"Close","Tasks":"Tasks"},"fr":{"Close":"Fermer"},"no":{"Close":"Lukk"},"es":{"Close":""}}"#;

    #[test]
    fn o3_lookup_fallbacks() {
        // The cases of o3's vendor/t3-protocol/src/i18n.rs.
        let c = Catalog::from_json(O3).unwrap();
        assert_eq!(c.translate("fr_CA.UTF-8", "Close"), "Fermer");
        assert_eq!(c.translate("zz:fr_CA.UTF-8", "Close"), "Fermer");
        assert_eq!(c.translate("fr", "Tasks"), "Tasks");
        assert_eq!(c.translate("zz", "Unknown"), "Unknown");
        assert_eq!(c.translate("nb_NO.UTF-8", "Close"), "Lukk");
        assert_eq!(c.translate("nn", "Close"), "Lukk");
        // Empty translations count as missing.
        assert_eq!(c.translate("es", "Close"), "Close");
        assert_eq!(c.resolve("fr_CA.UTF-8:en"), "fr-ca");
        assert_eq!(c.resolve("zz:fr"), "fr");
        assert_eq!(c.resolve("zz"), "zz");
        assert_eq!(c.resolve(""), "en");
        assert_eq!(
            Catalog::chain("fr_CA.UTF-8:de"),
            vec!["fr-ca", "fr", "de", "en"]
        );
    }

    #[test]
    fn rejects_other_formats_and_merges() {
        assert!(Catalog::from_json("[]").is_err());
        assert!(Catalog::from_json(r#"{"en":["x"]}"#).is_err());
        assert!(Catalog::from_json(r#"{"en":{"a":{"one":"x"}}}"#).is_err());
        let mut tk =
            Catalog::from_json(r#"{"en":{"Cancel":"Cancel"},"es":{"Cancel":"Cancelar"}}"#).unwrap();
        let app = Catalog::from_json(
            r#"{"es":{"Cancel":"Anular","Open":"Abrir"},"ES_cl":{"Open":"Abrir po"}}"#,
        )
        .unwrap();
        tk.merge(&app);
        assert_eq!(tk.translate("es", "Cancel"), "Anular");
        assert_eq!(tk.translate("es_CL.UTF-8", "Open"), "Abrir po");
        assert_eq!(tk.translate("es_AR", "Open"), "Abrir");
        assert_eq!(tk.locales(), vec!["en", "es", "es-cl"]);
    }

    #[test]
    fn placeholders_and_isolates() {
        let n = 2;
        assert_eq!(format("{n} working", &[("n", &n)], false), "2 working");
        assert_eq!(format("{n} working", &[], false), "{n} working");
        assert_eq!(
            format(
                "Not {x} {{y}} {1a} {",
                &[("x", &1.5), ("y", &"[1,2]"), ("1a", &"no")],
                false
            ),
            "Not 1.5 {[1,2]} {1a} {"
        );
        assert_eq!(
            format("{a}{a}{b}", &[("a", &"x"), ("b", &"y")], false),
            "xxy"
        );
        assert_eq!(
            format("حذف {n}", &[("n", &3)], true),
            "حذف \u{2068}3\u{2069}"
        );
        assert_eq!(
            placeholders("{a} {b_2} {3c} {d"),
            ["a", "b_2"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn plural_lookup() {
        let c = Catalog::from_json(r#"{
            "en": {"{count} files": "{count} files", "{count} files|one": "{count} file"},
            "ru": {"{count} files": "{count} файла", "{count} files|one": "{count} файл", "{count} files|few": "{count} файла", "{count} files|many": "{count} файлов"},
            "pl": {"{count} files": "{count} pliku", "{count} files|one": "{count} plik", "{count} files|few": "{count} pliki", "{count} files|many": "{count} plików"},
            "cs": {"{count} files": "{count} souborů", "{count} files|one": "{count} soubor", "{count} files|few": "{count} soubory"},
            "ar": {"{count} files": "{count} ملف", "{count} files|zero": "لا ملفات", "{count} files|one": "ملف واحد", "{count} files|two": "ملفان", "{count} files|few": "{count} ملفات", "{count} files|many": "{count} ملفًا"},
            "ja": {"{count} files": "{count} 個のファイル"},
            "de": {"{count} files": "{count} Dateien"}
        }"#).unwrap();
        let t = |loc: &str, n: i64| {
            let chain = Catalog::chain(loc);
            let (_, s) = c.lookup(&chain, "{count} files", Some(n)).unwrap();
            format(s, &[("count", &n)], false)
        };
        assert_eq!(t("en", 1), "1 file");
        assert_eq!(t("en", 0), "0 files");
        assert_eq!(
            [1, 3, 5, 21, 22, 111].map(|n| t("ru", n)),
            [
                "1 файл",
                "3 файла",
                "5 файлов",
                "21 файл",
                "22 файла",
                "111 файлов"
            ]
        );
        assert_eq!(
            [1, 2, 5, 12, 22, 25].map(|n| t("pl", n)),
            [
                "1 plik",
                "2 pliki",
                "5 plików",
                "12 plików",
                "22 pliki",
                "25 plików"
            ]
        );
        assert_eq!(
            [1, 3, 5].map(|n| t("cs", n)),
            ["1 soubor", "3 soubory", "5 souborů"]
        );
        assert_eq!(
            [0, 1, 2, 3, 11, 100].map(|n| t("ar", n)),
            [
                "لا ملفات",
                "ملف واحد",
                "ملفان",
                "3 ملفات",
                "11 ملفًا",
                "100 ملف"
            ]
        );
        assert_eq!(
            [1, 7].map(|n| t("ja", n)),
            ["1 個のファイル", "7 個のファイル"]
        );
        // A locale without the plural form uses its plain key (the `other` form).
        assert_eq!(t("de", 1), "1 Dateien");
        // The report sees the German gap and nothing else.
        let r = c.check("en");
        assert_eq!(
            r.issues,
            vec![Issue {
                locale: "de".into(),
                key: "{count} files".into(),
                kind: IssueKind::MissingPlural(Plural::One)
            }],
            "{r}"
        );
    }

    #[test]
    fn finds_keys_in_source() {
        let src = r#"let a = tr("Open"); let b = tr!("Delete {name}?", name = n); let c = i18n::text("Save \"x\" \u{2026}");
                     // tr("comment")
                     #[cfg(test)]
                     mod tests {
                         fn x() { tr("test"); }
                     }
                     let d = trn!("{count} files", 3); let e = str("no"); let f = attr("no");"#;
        assert_eq!(
            source_keys(src),
            ["Open", "Delete {name}?", "{count} files", "Save \"x\" …"]
        );
    }

    #[test]
    fn check_reports_missing_extra_and_placeholders() {
        let c = Catalog::from_json(
            r#"{
            "en": {"Open": "Open", "Delete {name}": "Delete {name}"},
            "es": {"Open": "Abrir", "Delete {nombre}": "x", "Delete {name}": "Eliminar {nombre}"},
            "ja": {"Open": "開く", "Delete {name}": "{name} を削除", "Open|one": "x"}
        }"#,
        )
        .unwrap();
        let r = c.check("en");
        let s = r.to_string();
        assert!(s.contains("es: extra \"Delete {nombre}\""), "{s}");
        assert!(
            s.contains("es: \"Delete {name}\" placeholders missing {name} unknown {nombre}"),
            "{s}"
        );
        assert!(s.contains("ja: \"Open|one\""), "{s}");
        assert_eq!(r.issues.len(), 3, "{s}");
        let mut c2 = c.clone();
        c2.insert("fr", "Open", "Ouvrir");
        assert!(c2
            .check("en")
            .issues
            .iter()
            .any(|i| i.locale == "fr" && i.key == "Delete {name}" && i.kind == IssueKind::Missing));
    }
}
