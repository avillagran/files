//! Explicit local-calendar presets resolved once off-UI into fixed UTC bounds.
//! No environment mutation, new dependency, clock polling or per-result timezone work.
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Today,
    Yesterday,
    LastSevenDays,
    LastHour,
    Last24Hours,
    Last168Hours,
}
impl Preset {
    fn rolling_seconds(self) -> Option<i64> {
        match self {
            Self::LastHour => Some(3600),
            Self::Last24Hours => Some(86400),
            Self::Last168Hours => Some(7 * 86400),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::LastHour => "Última hora · segundos completos, UTC fijado",
            Self::Last24Hours => "Últimas24h · segundos completos, UTC fijado",
            Self::Last168Hours => "Últimas168h · segundos completos, UTC fijado",
            Self::Today => "Hoy local · límites UTC fijados",
            Self::Yesterday => "Ayer local · límites UTC fijados",
            Self::LastSevenDays => "Hoy y seis días previos · límites UTC fijados",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub after: String,
    pub before: String,
    pub reference: i64,
    pub start: i64,
    pub end: i64,
    pub preset: Preset,
}
impl Range {
    pub fn matches_request(&self, preset: Preset, reference: SystemTime) -> bool {
        reference_seconds(reference).ok() == Some(self.reference)
            && self.preset == preset
            && self.start < self.end
            && preset.rolling_seconds().is_none_or(|span| {
                self.end == self.reference && self.end.checked_sub(span) == Some(self.start)
            })
            && self
                .end
                .checked_sub(self.start)
                .is_some_and(|span| span <= 8 * 86400)
            && utc_seconds(&self.after) == Some(self.start)
            && utc_seconds(&self.before) == Some(self.end)
    }
}
/// Date-only remains midnight UTC. Timestamp extension is strict YYYY-MM-DDTHH:MM:SSZ.
pub fn utc_seconds(s: &str) -> Option<i64> {
    if s.len() == 10 {
        return crate::search_predicate::date(s);
    }
    let b = s.as_bytes();
    if b.len() != 20 || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    if !b[11..13]
        .iter()
        .chain(&b[14..16])
        .chain(&b[17..19])
        .all(u8::is_ascii_digit)
    {
        return None;
    }
    let day = crate::search_predicate::date(s.get(..10)?)?;
    let hour = s.get(11..13)?.parse::<i64>().ok()?;
    let minute = s.get(14..16)?.parse::<i64>().ok()?;
    let second = s.get(17..19)?.parse::<i64>().ok()?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    day.checked_add(hour * 3600 + minute * 60 + second)
}
fn reference_seconds(time: SystemTime) -> Result<i64, String> {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::from(d.as_secs()),
        Err(e) => {
            -i128::from(e.duration().as_secs()) - i128::from(e.duration().subsec_nanos() != 0)
        }
    };
    i64::try_from(seconds).map_err(|_| "Reloj fuera del rango de fechas".into())
}
#[cfg(unix)]
mod local {
    use super::*;
    fn broken(seconds: i64, local: bool) -> Result<libc::tm, String> {
        let time = seconds as libc::time_t;
        if time as i64 != seconds {
            return Err("Fecha fuera del rango de la plataforma".into());
        }
        let mut out = std::mem::MaybeUninit::<libc::tm>::uninit();
        let ptr = unsafe {
            if local {
                libc::localtime_r(&time, out.as_mut_ptr())
            } else {
                libc::gmtime_r(&time, out.as_mut_ptr())
            }
        };
        if ptr.is_null() {
            return Err("No se pudo resolver el calendario local".into());
        }
        let value = unsafe { out.assume_init() };
        if !(-1899..=8099).contains(&value.tm_year) {
            return Err("Fecha fuera de años1–9999".into());
        }
        Ok(value)
    }
    fn ymd(tm: &libc::tm) -> (i32, i32, i32) {
        (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
    }
    fn day(tm: &libc::tm) -> Result<i64, String> {
        let (y, m, d) = ymd(tm);
        if !(1..=9999).contains(&y) {
            return Err("Calendario local fuera de años1–9999".into());
        }
        crate::search_predicate::date(&format!("{y:04}-{m:02}-{d:02}"))
            .ok_or_else(|| "Fecha local no válida".into())
    }
    fn boundary(civil_utc: i64) -> Result<i64, String> {
        let civil = broken(civil_utc, false)?;
        if !(1..=9999).contains(&(civil.tm_year + 1900)) {
            return Err("Límite local fuera de años1–9999".into());
        }
        let wanted = ymd(&civil);
        let mut candidates = [None; 3];
        for (i, dst) in [-1, 0, 1].into_iter().enumerate() {
            let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
            tm.tm_year = civil.tm_year;
            tm.tm_mon = civil.tm_mon;
            tm.tm_mday = civil.tm_mday;
            tm.tm_isdst = dst;
            let seconds = unsafe { libc::mktime(&mut tm) } as i64;
            let actual = broken(seconds, true)?;
            // A missing midnight may normalize forward to the first valid time.
            // A skipped date/backward normalization must not silently change dates.
            if ymd(&actual) == wanted {
                candidates[i] = Some(seconds);
            }
        }
        let first = candidates
            .into_iter()
            .flatten()
            .min()
            .ok_or("La fecha local no existe en esta zona horaria")?;
        let before = first.checked_sub(1).ok_or("Límite local fuera del rango")?;
        if ymd(&broken(before, true)?) == wanted {
            return Err("Inicio de día local ambiguo; usa límites UTC explícitos".into());
        }
        Ok(first)
    }
    pub(super) fn format(seconds: i64) -> Result<String, String> {
        let tm = broken(seconds, false)?;
        let (y, m, d) = ymd(&tm);
        if !(1..=9999).contains(&y) {
            return Err("Límite UTC fuera de años1–9999".into());
        }
        Ok(format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            tm.tm_hour, tm.tm_min, tm.tm_sec
        ))
    }
    pub(super) fn resolve(preset: Preset, reference: i64) -> Result<Range, String> {
        if let Some(span) = preset.rolling_seconds() {
            let start = reference
                .checked_sub(span)
                .ok_or("Intervalo fuera del rango")?;
            let after = format(start)?;
            let before = format(reference)?;
            if utc_seconds(&after) != Some(start) || utc_seconds(&before) != Some(reference) {
                return Err("Calendario UTC no compatible; usa límites explícitos".into());
            }
            return Ok(Range {
                after,
                before,
                reference,
                start,
                end: reference,
                preset,
            });
        }
        let now = broken(reference, true)?;
        let today = day(&now)?;
        let (start_day, end_day) = match preset {
            Preset::Today => (today, today + 86400),
            Preset::Yesterday => (today - 86400, today),
            Preset::LastSevenDays => (today - 6 * 86400, today + 86400),
            Preset::LastHour | Preset::Last24Hours | Preset::Last168Hours => {
                unreachable!("handled above")
            }
        };
        let start = boundary(start_day)?;
        let end = boundary(end_day)?;
        if start >= end || end - start > 8 * 86400 {
            return Err("Intervalo de calendario local no válido".into());
        }
        let again = broken(reference, true)?;
        if ymd(&again) != ymd(&now) || again.tm_hour != now.tm_hour || again.tm_min != now.tm_min {
            return Err("Zona horaria cambió durante el cálculo; elige de nuevo".into());
        }
        let after = format(start)?;
        let before = format(end)?;
        if utc_seconds(&after) != Some(start) || utc_seconds(&before) != Some(end) {
            return Err("Calendario UTC no compatible; usa límites explícitos".into());
        }
        Ok(Range {
            after,
            before,
            reference,
            start,
            end,
            preset,
        })
    }
}
pub fn resolve(preset: Preset, reference: SystemTime) -> Result<Range, String> {
    let reference = reference_seconds(reference)?;
    #[cfg(unix)]
    {
        local::resolve(preset, reference)
    }
    #[cfg(not(unix))]
    {
        let _ = (preset, reference);
        Err("Presets locales no disponibles en esta plataforma".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(unix)]
    fn rolling_ranges_have_exact_elapsed_seconds_and_reject_forged_bounds() {
        for reference in [
            UNIX_EPOCH - std::time::Duration::from_nanos(1),
            UNIX_EPOCH
                + std::time::Duration::from_secs(1793538000)
                + std::time::Duration::from_nanos(999_999_999),
        ] {
            for (preset, span) in [
                (Preset::LastHour, 3600),
                (Preset::Last24Hours, 86400),
                (Preset::Last168Hours, 604800),
            ] {
                let r = resolve(preset, reference).unwrap();
                assert_eq!(r.end, reference_seconds(reference).unwrap());
                assert_eq!(r.end - r.start, span);
                assert!(r.matches_request(preset, reference));
                assert!(!r.matches_request(Preset::Today, reference));
                let mut forged = r.clone();
                forged.start += 1;
                forged.after = local::format(forged.start).unwrap();
                assert!(!forged.matches_request(preset, reference));
                let mut forged = r.clone();
                forged.end += 1;
                forged.before = local::format(forged.end).unwrap();
                assert!(!forged.matches_request(preset, reference));
            }
        }
        let earliest = UNIX_EPOCH - std::time::Duration::from_secs(62135596800);
        assert!(resolve(Preset::LastHour, earliest).is_err());
    }
    #[test]
    fn strict_timestamp_keeps_date_only_and_negative_epoch_semantics() {
        assert_eq!(utc_seconds("1970-01-01"), Some(0));
        assert_eq!(utc_seconds("1969-12-31T23:59:59Z"), Some(-1));
        assert_eq!(
            utc_seconds("2000-02-29T12:34:56Z"),
            utc_seconds("2000-02-29").map(|x| x + 45296)
        );
        for s in [
            "2026-02-30T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:60Z",
            "2026-01-01T00:00:00+00:00",
            "2026-01-01T00:00:00z",
            "2026-01-01T00:00:00Zx",
            "２０２６-01-01T00:00:00Z",
        ] {
            assert!(utc_seconds(s).is_none(), "{s}");
        }
    }
    #[test]
    fn forged_interval_overflow_and_reference_mismatch_reject_without_panicking() {
        let r = Range {
            after: "1970-01-01T00:00:00Z".into(),
            before: "1970-01-02T00:00:00Z".into(),
            reference: 0,
            start: i64::MIN,
            end: i64::MAX,
            preset: Preset::Today,
        };
        assert!(!r.matches_request(Preset::Today, UNIX_EPOCH));
        #[cfg(unix)]
        assert!(local::resolve(Preset::Today, i64::MAX).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn isolated_calendar_case() {
        let Ok(case) = std::env::var("FILES_TEST_CALENDAR_CASE") else {
            return;
        };
        if case == "apia-skip" {
            let reference = utc_seconds("2011-12-31T00:00:00Z").unwrap();
            assert!(
                resolve(
                    Preset::Yesterday,
                    UNIX_EPOCH + std::time::Duration::from_secs(reference as u64)
                )
                .is_err()
            );
            return;
        }
        let (reference, preset, after, before) = match case.as_str() {
            "utc-leap" => (
                "2024-03-01T12:00:00Z",
                Preset::Yesterday,
                "2024-02-29T00:00:00Z",
                "2024-03-01T00:00:00Z",
            ),
            "new-york-spring" => (
                "2026-03-08T16:00:00Z",
                Preset::Today,
                "2026-03-08T05:00:00Z",
                "2026-03-09T04:00:00Z",
            ),
            "havana-fold" => (
                "2026-11-01T16:00:00Z",
                Preset::Today,
                "2026-11-01T04:00:00Z",
                "2026-11-02T05:00:00Z",
            ),
            "new-york-fall" => (
                "2026-11-01T17:00:00Z",
                Preset::Today,
                "2026-11-01T04:00:00Z",
                "2026-11-02T05:00:00Z",
            ),
            "santiago-gap" => (
                "2026-09-06T16:00:00Z",
                Preset::Today,
                "2026-09-06T04:00:00Z",
                "2026-09-07T03:00:00Z",
            ),
            "santiago-yesterday" => (
                "2026-04-05T16:00:00Z",
                Preset::Yesterday,
                "2026-04-04T03:00:00Z",
                "2026-04-05T04:00:00Z",
            ),
            "seven" => (
                "2026-03-10T16:00:00Z",
                Preset::LastSevenDays,
                "2026-03-04T05:00:00Z",
                "2026-03-11T04:00:00Z",
            ),
            _ => panic!("Unknown owned calendar fixture"),
        };
        let t = utc_seconds(reference).unwrap();
        let range = resolve(
            preset,
            UNIX_EPOCH + std::time::Duration::from_secs(t as u64),
        )
        .unwrap();
        assert_eq!(range.after, after);
        assert_eq!(range.before, before);
        for (preset, span) in [
            (Preset::LastHour, 3600),
            (Preset::Last24Hours, 86400),
            (Preset::Last168Hours, 604800),
        ] {
            let clock = UNIX_EPOCH + std::time::Duration::from_secs(t as u64);
            let elapsed = resolve(preset, clock).unwrap();
            assert_eq!(elapsed.end - elapsed.start, span);
            assert_eq!(elapsed.end, t);
        }

        assert_eq!(range.reference, t);
    }
    #[test]
    #[cfg(unix)]
    fn calendar_presets_use_isolated_process_timezones_and_dst() {
        for (zone, case) in [
            ("UTC", "utc-leap"),
            ("America/Havana", "havana-fold"),
            ("Pacific/Apia", "apia-skip"),
            ("America/New_York", "new-york-spring"),
            ("America/New_York", "new-york-fall"),
            ("America/Santiago", "santiago-gap"),
            ("America/Santiago", "santiago-yesterday"),
            ("America/New_York", "seven"),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "search_dates::tests::isolated_calendar_case",
                    "--nocapture",
                ])
                .env("TZ", zone)
                .env("FILES_TEST_CALENDAR_CASE", case)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{zone}/{case}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }
}
