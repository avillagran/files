//! Planificación pura: no modifica el disco. El ejecutor debe revalidar identidades
//! y destinos, y usar nombres temporales exclusivos para ciclos y cambios de caso.
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

#[path = "rename_regex.rs"]
mod rename_regex;

const MAX_NAME_BYTES: usize = 4096;
const MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;

fn consume_bytes(budget: &mut usize, bytes: usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(bytes)
        .ok_or("El lote excede el presupuesto de nombres")?;
    Ok(())
}

fn replace_bounded(stem: &str, from: &str, to: &str) -> Result<String, String> {
    let count = stem.matches(from).count();
    let size = stem
        .len()
        .checked_sub(count * from.len())
        .and_then(|base| {
            count
                .checked_mul(to.len())
                .and_then(|extra| base.checked_add(extra))
        })
        .filter(|size| *size <= MAX_NAME_BYTES)
        .ok_or("El reemplazo genera un nombre demasiado largo")?;
    let mut result = String::with_capacity(size);
    let mut end = 0;
    for (start, matched) in stem.match_indices(from) {
        result.push_str(&stem[end..start]);
        result.push_str(to);
        end = start + matched.len();
    }
    result.push_str(&stem[end..]);
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Preserve,
    Lower,
    Upper,
}

#[derive(Clone, Debug)]
pub struct RenameOptions {
    /// Tokens: {name}, {ext}, {n}, {n:03}; {{ y }} representan llaves literales.
    /// {dotext} incluye el punto sólo si existe extensión. {ext} no incluye el punto. {name} es el nombre sin su última extensión.
    pub pattern: String,
    pub start: u64,
    pub step: u64,
    /// Reemplazo en el nombre sin extensión, antes de aplicar el patrón.
    pub replace: Option<(String, String)>,
    /// Opt-in regex. Capturas $1/${nombre}, $$ literal; ausentes expanden vacío.
    /// El modo literal no interpreta dólares ni metacaracteres.
    pub regex: bool,
    pub case: Case,
    /// Comparación conservadora Unicode lowercase; no sustituye reglas del FS.
    pub case_insensitive: bool,
    pub portable_names: bool,
}

impl Default for RenameOptions {
    fn default() -> Self {
        Self {
            pattern: "{name}{dotext}".into(),
            start: 1,
            step: 1,
            replace: None,
            regex: false,
            case: Case::Preserve,
            case_insensitive: false,
            portable_names: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    pub from: String,
    pub to: String,
}

fn key(name: &str, insensitive: bool) -> String {
    if insensitive {
        name.to_lowercase()
    } else {
        name.into()
    }
}

/// Valida un componente para crear o renombrar. El límite del filesystem se
/// comprueba al ejecutar; `portable` aplica además las restricciones de nombres
/// compartidas por la UI. No comprueba existencia ni permisos.
pub fn valid_name(name: &str, portable: bool) -> Result<(), String> {
    if name.len() > MAX_NAME_BYTES {
        return Err("Nombre demasiado largo".into());
    }
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
        return Err(format!("Nombre inválido: {name:?}"));
    }
    if portable {
        if name
            .chars()
            .any(|c| c.is_control() || "\\<>:\"|?*".contains(c))
            || name.ends_with(['.', ' '])
        {
            return Err(format!("Nombre no portable: {name:?}"));
        }
        let base = name.split('.').next().unwrap_or_default().to_uppercase();
        let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ["COM", "LPT"].iter().any(|p| {
                base.strip_prefix(p).is_some_and(|n| {
                    matches!(
                        n,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                })
            });
        if reserved {
            return Err(format!("Nombre reservado: {name:?}"));
        }
    }
    Ok(())
}

fn render(pattern: &str, name: &str, ext: &str, number: u64) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                result.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                result.push('}');
            }
            '}' => return Err("Llave de cierre sin apertura".into()),
            '{' => {
                let mut token = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    token.push(c);
                }
                if !closed {
                    return Err("Token sin cerrar".into());
                }
                match token.as_str() {
                    "name" => result.push_str(name),
                    "ext" => result.push_str(ext),
                    "dotext" => {
                        if !ext.is_empty() {
                            result.push('.');
                            result.push_str(ext);
                        }
                    }
                    "n" => result.push_str(&number.to_string()),
                    _ => {
                        let width = token
                            .strip_prefix("n:")
                            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                            .and_then(|s| s.parse::<usize>().ok())
                            .filter(|w| (1..=20).contains(w))
                            .ok_or_else(|| format!("Token desconocido: {{{token}}}"))?;
                        result.push_str(&format!("{number:0width$}"));
                    }
                }
            }
            _ => result.push(c),
        }
        if result.len() > 4096 {
            return Err("Nombre generado demasiado largo".into());
        }
    }
    Ok(result)
}

/// `selected` fija el orden de la secuencia; `existing` debe contener TODOS los
/// nombres del directorio, incluidos ocultos. No convertir OsString con pérdida:
/// la UI debe rechazar/omitir explícitamente nombres no UTF-8 antes de llamar.
/// Se permiten ciclos; el ejecutor NO debe aplicar las filas secuencialmente.
/// Máximo 100k entradas por lista, 4096 bytes por nombre y 8 MiB de nombres
/// agregados de entrada/salida por separado. El presupuesto de entrada cuenta
/// ambas listas; no representa la memoria total del proceso. Un error no entrega
/// un plan parcial: la UI debe mostrarlo y permitir reducir el lote.
pub fn plan(
    selected: &[String],
    existing: &[String],
    options: &RenameOptions,
) -> Result<Vec<Rename>, String> {
    plan_with_check(selected, existing, options, || Ok(()))
}

/// Planifica con cancelación cooperativa entre entradas en todas las fases.
/// Nunca devuelve filas parciales ni toca el filesystem. La cancelación no
/// interrumpe una asignación individual ni el procesamiento de un nombre acotado.
pub fn plan_cancellable(
    selected: &[String],
    existing: &[String],
    options: &RenameOptions,
    cancelled: &AtomicBool,
) -> Result<Vec<Rename>, String> {
    plan_with_check(selected, existing, options, || {
        if cancelled.load(Ordering::Acquire) {
            Err("Planificación cancelada".into())
        } else {
            Ok(())
        }
    })
}

fn plan_with_check(
    selected: &[String],
    existing: &[String],
    options: &RenameOptions,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<Vec<Rename>, String> {
    check()?;
    if selected.len() > MAX_ENTRIES || existing.len() > MAX_ENTRIES {
        return Err("Lote demasiado grande".into());
    }
    let mut input_budget = MAX_BATCH_BYTES;
    for name in selected.iter().chain(existing) {
        check()?;
        if name.len() > MAX_NAME_BYTES {
            return Err("Nombre de entrada demasiado largo".into());
        }
        consume_bytes(&mut input_budget, name.len())?;
    }
    if options.pattern.len() > 4096 {
        return Err("Patrón demasiado largo".into());
    }
    if !options.regex
        && options
            .replace
            .as_ref()
            .is_some_and(|(from, _)| from.is_empty())
    {
        return Err("El texto a reemplazar no puede estar vacío".into());
    }
    if options
        .replace
        .as_ref()
        .is_some_and(|(from, to)| from.len() > MAX_NAME_BYTES || to.len() > MAX_NAME_BYTES)
    {
        return Err("Texto de reemplazo demasiado largo".into());
    }
    let regex = if options.regex {
        match &options.replace {
            Some((from, to)) => Some(rename_regex::Prepared::compile(from, to, &mut check)?),
            None => None,
        }
    } else {
        None
    };
    let mut all = HashSet::new();
    for name in existing {
        check()?;
        all.insert(name.as_str());
    }
    let mut sources = HashSet::new();
    let mut exact_sources = HashSet::new();
    for source in selected {
        check()?;
        valid_name(source, false)?;
        if !all.contains(source.as_str()) {
            return Err(format!("Origen inexistente: {source}"));
        }
        if !sources.insert(key(source, options.case_insensitive)) {
            return Err(format!("Origen duplicado o ambiguo: {source}"));
        }
        exact_sources.insert(source.as_str());
    }
    let mut occupied = HashSet::new();
    for name in existing {
        check()?;
        if !exact_sources.contains(name.as_str()) {
            occupied.insert(key(name, options.case_insensitive));
        }
    }
    let mut destinations = HashSet::new();
    let mut result = Vec::with_capacity(selected.len());
    let mut output_budget = MAX_BATCH_BYTES;
    for (index, source) in selected.iter().enumerate() {
        check()?;
        let number = (index as u64)
            .checked_mul(options.step)
            .and_then(|offset| options.start.checked_add(offset))
            .ok_or("La secuencia excede el rango permitido")?;
        let (stem, ext) = match source.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, ext),
            _ => (source.as_str(), ""),
        };
        let stem = match (&options.replace, &regex) {
            (_, Some(regex)) => regex.replace(stem, &mut check)?,
            (Some((from, to)), None) => replace_bounded(stem, from, to)?,
            (None, None) => stem.to_owned(),
        };
        let stem = match options.case {
            Case::Preserve => stem,
            Case::Lower => stem.to_lowercase(),
            Case::Upper => stem.to_uppercase(),
        };
        let target = render(&options.pattern, &stem, ext, number)?;
        consume_bytes(&mut output_budget, source.len() + target.len())?;
        valid_name(&target, options.portable_names)?;
        let target_key = key(&target, options.case_insensitive);
        if occupied.contains(&target_key) || !destinations.insert(target_key) {
            return Err(format!("Colisión en destino: {target}"));
        }
        result.push(Rename {
            from: source.clone(),
            to: target,
        });
    }
    check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regex_replacement_is_opt_in_and_preserves_extension_and_literal_dollars() {
        let source = names(&["área-12.JPG"]);
        let mut options = RenameOptions {
            regex: true,
            replace: Some((r"(?P<word>.+)-([0-9]+)".into(), "${word}-$2-$$".into())),
            ..Default::default()
        };
        assert_eq!(
            plan(&source, &source, &options).unwrap()[0].to,
            "área-12-$.JPG"
        );
        let source = names(&["a.b.txt"]);
        options.regex = false;
        options.replace = Some((".".into(), "$1".into()));
        assert_eq!(plan(&source, &source, &options).unwrap()[0].to, "a$1b.txt");
        options.replace = Some(("".into(), "-".into()));
        assert!(plan(&source, &source, &options).is_err());
        options.regex = true;
        assert_eq!(
            plan(&source, &source, &options).unwrap()[0].to,
            "-a-.-b-.txt"
        );
    }
    #[test]
    fn regex_still_rejects_collisions_and_keeps_cycles_as_a_plan() {
        let source = names(&["a", "b"]);
        let mut options = RenameOptions {
            regex: true,
            replace: Some(("[ab]".into(), "x".into())),
            ..Default::default()
        };
        assert!(
            plan(&source, &source, &options)
                .unwrap_err()
                .contains("Colisión")
        );
        options.replace = Some(("x".into(), "occupied".into()));
        let source = names(&["x"]);
        assert!(plan(&source, &names(&["x", "occupied"]), &options).is_err());
        options.pattern = "{n}".into();
        options.replace = Some(("[12]".into(), "$0".into()));
        let source = names(&["2", "1"]);
        let result = plan(&source, &source, &options).unwrap();
        assert_eq!((result[0].to.as_str(), result[1].to.as_str()), ("1", "2"));
    }
    #[test]
    fn regex_plans_cancel_between_matches_and_retry_without_partial_result() {
        let source = names(&["áá.txt", "bbbb.txt"]);
        let options = RenameOptions {
            regex: true,
            replace: Some((".".into(), "${0}_".into())),
            pattern: "{n}-{name}{dotext}".into(),
            ..Default::default()
        };
        let expected = plan(&source, &source, &options).unwrap();
        let mut checkpoints = 0;
        plan_with_check(&source, &source, &options, || {
            checkpoints += 1;
            Ok(())
        })
        .unwrap();
        for stop in 0..checkpoints {
            let mut count = 0;
            assert_eq!(
                plan_with_check(&source, &source, &options, || {
                    let cancel = count == stop;
                    count += 1;
                    if cancel {
                        Err("regex cancelled".into())
                    } else {
                        Ok(())
                    }
                })
                .unwrap_err(),
                "regex cancelled"
            );
        }
        assert_eq!(plan(&source, &source, &options).unwrap(), expected);
    }
    #[test]
    fn cancellation_never_returns_partial_plans_and_allows_retry() {
        let selected = names(&["área.jpg", "segundo.txt", ".config"]);
        let existing = names(&["área.jpg", "segundo.txt", ".config", "ocupado"]);
        let options = RenameOptions {
            pattern: "nuevo-{n:03}{dotext}".into(),
            ..Default::default()
        };
        let expected = plan(&selected, &existing, &options).unwrap();
        let original = (selected.clone(), existing.clone());
        let mut checkpoints = 0;
        assert_eq!(
            plan_with_check(&selected, &existing, &options, || {
                checkpoints += 1;
                Ok(())
            })
            .unwrap(),
            expected
        );
        // Inyectar cancelación en cada frontera, incluidas las posteriores a
        // filas ya generadas; el resultado público sigue siendo sólo un error.
        for stop in 0..checkpoints {
            let mut current = 0;
            let error = plan_with_check(&selected, &existing, &options, || {
                let cancel = current == stop;
                current += 1;
                if cancel {
                    Err("cancelled by test".into())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert_eq!(error, "cancelled by test");
            assert_eq!((&selected, &existing), (&original.0, &original.1));
        }
        let cancelled = AtomicBool::new(true);
        assert_eq!(
            plan_cancellable(&selected, &existing, &options, &cancelled).unwrap_err(),
            "Planificación cancelada"
        );
        cancelled.store(false, Ordering::Release);
        assert_eq!(
            plan_cancellable(&selected, &existing, &options, &cancelled).unwrap(),
            expected
        );
        cancelled.store(true, Ordering::Release);
        assert!(plan_cancellable(&[], &[], &options, &cancelled).is_err());
    }

    #[test]
    fn shared_name_validation_preserves_portability_rules() {
        for name in [
            "CON.txt",
            "nul",
            "LPT1",
            "COM².txt",
            "a/b",
            "..",
            "trailing.",
        ] {
            assert!(valid_name(name, true).is_err(), "{name}");
        }
        assert!(valid_name(&"x".repeat(MAX_NAME_BYTES + 1), false).is_err());
        assert!(valid_name("CON.txt", false).is_ok());
        for name in ["informe.txt", ".config", "área", "CONTEXT.txt"] {
            assert!(valid_name(name, true).is_ok(), "{name}");
        }
    }

    #[test]
    fn expansion_and_aggregate_limits_are_checked() {
        assert!(replace_bounded(&"a".repeat(4096), "a", &"b".repeat(4096)).is_err());
        assert_eq!(replace_bounded("área área", "área", "x").unwrap(), "x x");
        assert_eq!(replace_bounded("aaaaa", "aa", "b").unwrap(), "bba");
        let mut budget = 4;
        consume_bytes(&mut budget, 4).unwrap();
        assert!(consume_bytes(&mut budget, 1).is_err());
        assert_eq!(budget, 0);
        let names = vec!["a".repeat(MAX_NAME_BYTES + 1)];
        assert!(plan(&names, &names, &RenameOptions::default()).is_err());
        let names = vec!["a".repeat(MAX_NAME_BYTES); MAX_BATCH_BYTES / MAX_NAME_BYTES + 1];
        assert!(
            plan(&[], &names, &RenameOptions::default())
                .unwrap_err()
                .contains("presupuesto")
        );
    }
    #[test]
    fn defaults_preserve_names_and_cycles_can_be_planned() {
        let sources = names(&["archivo.txt", "README", ".gitignore"]);
        assert!(
            plan(&sources, &sources, &RenameOptions::default())
                .unwrap()
                .iter()
                .all(|row| row.from == row.to)
        );
        let cycle = names(&["2", "1"]);
        let options = RenameOptions {
            pattern: "{n}".into(),
            ..Default::default()
        };
        let rows = plan(&cycle, &cycle, &options).unwrap();
        assert_eq!(
            rows[0],
            Rename {
                from: "2".into(),
                to: "1".into()
            }
        );
        assert_eq!(
            rows[1],
            Rename {
                from: "1".into(),
                to: "2".into()
            }
        );
    }
    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }
    #[test]
    fn sequence_unicode_and_extension() {
        let sources = names(&["área.jpg", "niño.png"]);
        let options = RenameOptions {
            pattern: "foto-{n:03}-{name}.{ext}".into(),
            start: 9,
            step: 2,
            ..Default::default()
        };
        let result = plan(&sources, &sources, &options).unwrap();
        assert_eq!(result[0].to, "foto-009-área.jpg");
        assert_eq!(result[1].to, "foto-011-niño.png");
    }
    #[test]
    fn refuses_collision_with_hidden_or_unselected_file() {
        let sources = names(&["a"]);
        let existing = names(&["a", ".hidden"]);
        let options = RenameOptions {
            pattern: ".hidden".into(),
            ..Default::default()
        };
        assert!(plan(&sources, &existing, &options).is_err());
    }
    #[test]
    fn rejects_duplicate_destinations_and_case_collisions() {
        let sources = names(&["a", "b"]);
        let options = RenameOptions {
            pattern: "{n}".into(),
            step: 0,
            ..Default::default()
        };
        assert!(plan(&sources, &sources, &options).is_err());
        let options = RenameOptions {
            pattern: "A".into(),
            case_insensitive: true,
            ..Default::default()
        };
        assert!(plan(&names(&["b"]), &sources, &options).is_err());
    }
    #[test]
    fn preserves_dotfile_and_handles_literal_braces() {
        let sources = names(&[".config"]);
        let options = RenameOptions {
            pattern: "{{{name}}}".into(),
            ..Default::default()
        };
        assert_eq!(
            plan(&sources, &sources, &options).unwrap()[0].to,
            "{.config}"
        );
    }
    #[test]
    fn rejects_traversal_reserved_names_and_bad_tokens() {
        let sources = names(&["a"]);
        for pattern in [
            "../x",
            "CON.txt",
            "a/b",
            "x\\y",
            "{unknown}",
            "{n:999999}",
            "{name",
            "}",
        ] {
            let options = RenameOptions {
                pattern: pattern.into(),
                ..Default::default()
            };
            assert!(plan(&sources, &sources, &options).is_err(), "{pattern}");
        }
    }
    #[test]
    fn sequence_overflow_is_error_not_wraparound() {
        let sources = names(&["a", "b"]);
        let options = RenameOptions {
            start: u64::MAX,
            pattern: "{n}".into(),
            ..Default::default()
        };
        assert!(plan(&sources, &sources, &options).is_err());
    }
    #[test]
    fn literal_replace_and_case_preserve_extension() {
        let sources = names(&["foto-verano.JPG"]);
        let options = RenameOptions {
            pattern: "{name}.{ext}".into(),
            replace: Some(("foto".into(), "álbum".into())),
            case: Case::Upper,
            ..Default::default()
        };
        assert_eq!(
            plan(&sources, &sources, &options).unwrap()[0].to,
            "ÁLBUM-VERANO.JPG"
        );
    }
}
