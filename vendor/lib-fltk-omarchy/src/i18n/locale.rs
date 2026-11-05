//! Locale tags and the user's preferred languages per OS.
//!
//! The OS calls live in `platform::languages`. Every function falls back instead of failing: no panics, an
//! empty list means "no preference" (callers use English).
//!
//! - Linux/BSD and other Unix: gettext's order. `LC_ALL`, else
//!   `LC_MESSAGES`, else `LANG` is the locale; `C`/`POSIX` there means no
//!   translation. Otherwise `LANGUAGE` (a `:` list) comes first.
//! - macOS: an explicit `LC_ALL`/`LC_MESSAGES`/`LANGUAGE` wins (terminals set
//!   `LANG` to whatever, so `LANG` alone does not); then CoreFoundation's
//!   `CFLocaleCopyPreferredLanguages` (System Settings > Language); then `LANG`.
//! - Windows: an explicit `LC_ALL`/`LC_MESSAGES`/`LANGUAGE`/`LANG` wins; then
//!   `GetUserPreferredUILanguages`, then `GetUserDefaultLocaleName`.

/// Normalized tag: `fr_CA.UTF-8@euro` → `fr-ca`, `zh-Hans-CN` → `zh-hans-cn`.
/// `C`, `POSIX` and empty give `""`. Only the first entry of a `:` list.
pub fn normalize(tag: &str) -> String {
    let first = tag.split(':').next().unwrap_or("");
    let t = first
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .trim()
        .replace('_', "-")
        .to_lowercase();
    match t.as_str() {
        "c" | "posix" => String::new(),
        _ => t,
    }
}

/// Language of a normalized tag, with Norwegian Bokmål/Nynorsk as `no`
/// (the catalogs use `no`) and the old Hebrew code `iw` as `he`.
pub fn language(tag: &str) -> &str {
    let b = tag.split('-').next().unwrap_or("");
    match b {
        "nb" | "nn" => "no",
        "iw" => "he",
        _ => b,
    }
}

/// A `:` or `,` separated list (`LANGUAGE`, `fr_CA.UTF-8:en`), normalized,
/// without empty, `C`/`POSIX` or repeated entries.
pub fn parse_list(list: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in list.split([':', ',']) {
        let n = normalize(part.trim());
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Lookup order for one tag: itself, then shorter prefixes, then the
/// language: `zh-hans-cn` → `zh-hans-cn`, `zh-hans`, `zh`; `nb-no` →
/// `nb-no`, `nb`, `no`.
pub fn fallbacks(tag: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut t = tag;
    while !t.is_empty() {
        if !out.iter().any(|o| o == t) {
            out.push(t.to_string());
        }
        t = match t.rfind('-') {
            Some(i) => &t[..i],
            None => "",
        };
    }
    let lang = language(tag);
    if !lang.is_empty() && !out.iter().any(|o| o == lang) {
        out.push(lang.to_string());
    }
    out
}

/// True for right-to-left languages (Arabic, Hebrew, Persian, Urdu, and a
/// few others written in Arabic or Hebrew script).
pub fn is_rtl(tag: &str) -> bool {
    matches!(
        language(tag),
        "ar" | "he" | "fa" | "ur" | "ps" | "sd" | "ug" | "yi" | "ckb" | "dv"
    )
}

/// Preferred languages from environment variables, gettext order. `get`
/// reads a variable (injectable for tests).
pub fn from_env_with(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let set = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    let primary = set("LC_ALL")
        .or_else(|| set("LC_MESSAGES"))
        .or_else(|| set("LANG"));
    if let Some(p) = &primary {
        if normalize(p).is_empty() {
            // C/POSIX: untranslated (gettext ignores LANGUAGE then).
            return Vec::new();
        }
    }
    let mut out = set("LANGUAGE").map(|l| parse_list(&l)).unwrap_or_default();
    if let Some(p) = primary {
        for t in parse_list(&p) {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// The OS list, normalized, without empties and duplicates.
fn os_languages() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in crate::platform::languages::preferred() {
        let t = normalize(&t);
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

/// The user's preferred languages, best first (may be empty).
pub fn system() -> Vec<String> {
    use crate::platform::{Os, OS};
    match OS {
        Os::MacOs => {
            // An explicit choice in the environment; LANG alone is not one.
            let explicit = from_env_with(|k| if k == "LANG" { None } else { env(k) });
            if !explicit.is_empty() {
                return explicit;
            }
            let os = os_languages();
            if !os.is_empty() {
                return os;
            }
            from_env_with(env)
        }
        Os::Windows => {
            let explicit = from_env_with(env);
            if !explicit.is_empty() {
                return explicit;
            }
            os_languages()
        }
        _ => from_env_with(env),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn normalizes_posix_and_bcp47_tags() {
        assert_eq!(normalize("fr_CA.UTF-8:en"), "fr-ca");
        assert_eq!(normalize("de_DE@euro"), "de-de");
        assert_eq!(normalize("zh-Hans-CN"), "zh-hans-cn");
        assert_eq!(normalize("C.UTF-8"), "");
        assert_eq!(normalize("POSIX"), "");
        assert_eq!(normalize(""), "");
        assert_eq!(language("nb-no"), "no");
        assert_eq!(language("nn"), "no");
        assert_eq!(language("iw-il"), "he");
        assert_eq!(
            parse_list("fr_CA.UTF-8:en,es::C"),
            vec!["fr-ca", "en", "es"]
        );
        assert_eq!(fallbacks("zh-hans-cn"), vec!["zh-hans-cn", "zh-hans", "zh"]);
        assert_eq!(fallbacks("nb-no"), vec!["nb-no", "nb", "no"]);
        assert!(
            is_rtl("ar-eg")
                && is_rtl("he")
                && is_rtl("fa")
                && is_rtl("ur")
                && !is_rtl("en")
                && !is_rtl("ja")
        );
    }

    #[test]
    fn environment_follows_gettext() {
        assert_eq!(
            from_env_with(env_of(&[("LANG", "es_CL.UTF-8")])),
            vec!["es-cl"]
        );
        assert_eq!(
            from_env_with(env_of(&[
                ("LANG", "es_CL.UTF-8"),
                ("LC_MESSAGES", "de_DE.UTF-8")
            ])),
            vec!["de-de"]
        );
        assert_eq!(
            from_env_with(env_of(&[
                ("LANG", "es_CL.UTF-8"),
                ("LC_ALL", "ja_JP.UTF-8"),
                ("LC_MESSAGES", "de_DE")
            ])),
            vec!["ja-jp"]
        );
        assert_eq!(
            from_env_with(env_of(&[("LANG", "pt_BR.UTF-8"), ("LANGUAGE", "uk:ru")])),
            vec!["uk", "ru", "pt-br"]
        );
        // C/POSIX turns translation off, LANGUAGE included.
        assert!(from_env_with(env_of(&[("LC_ALL", "C"), ("LANGUAGE", "fr")])).is_empty());
        assert_eq!(from_env_with(env_of(&[("LANGUAGE", "fr")])), vec!["fr"]);
        assert!(from_env_with(env_of(&[])).is_empty());
        // Never panics, whatever the OS gives.
        let _ = system();
    }
}
