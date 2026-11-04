//! Checked, allocation-free metadata predicates. Calendar boundaries are UTC.
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone, Copy)]
pub(crate) enum Predicate {
    Size { low: u64, high: u64 },
    After(i64),
    Before(i64),
}
impl Predicate {
    pub(crate) fn parse(prefix: &str, value: &str) -> Option<Self> {
        match prefix {
            "size" => {
                let (low, high) = if let Some((a, b)) = value.split_once("..") {
                    (
                        if a.is_empty() { 0 } else { bytes(a)? },
                        if b.is_empty() { u64::MAX } else { bytes(b)? },
                    )
                } else if let Some(v) = value.strip_prefix(">=") {
                    (bytes(v)?, u64::MAX)
                } else if let Some(v) = value.strip_prefix("<=") {
                    (0, bytes(v)?)
                } else if let Some(v) = value.strip_prefix('>') {
                    (bytes(v)?.checked_add(1)?, u64::MAX)
                } else if let Some(v) = value.strip_prefix('<') {
                    (0, bytes(v)?.checked_sub(1)?)
                } else {
                    let n = bytes(value.strip_prefix('=').unwrap_or(value))?;
                    (n, n)
                };
                (low <= high).then_some(Self::Size { low, high })
            }
            "after" => Some(Self::After(crate::search_dates::utc_seconds(value)?)),
            "before" => Some(Self::Before(crate::search_dates::utc_seconds(value)?)),
            _ => None,
        }
    }
    pub(crate) fn is_size(self) -> bool {
        matches!(self, Self::Size { .. })
    }
    pub(crate) fn matches(self, size: u64, modified: SystemTime) -> bool {
        match self {
            Self::Size { low, high } => (low..=high).contains(&size),
            Self::After(bound) => seconds(modified) >= i128::from(bound),
            Self::Before(bound) => seconds(modified) < i128::from(bound),
        }
    }
}
fn seconds(t: SystemTime) -> i128 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::from(d.as_secs()),
        Err(e) => {
            -i128::from(e.duration().as_secs()) - i128::from(e.duration().subsec_nanos() != 0)
        }
    }
}
fn bytes(s: &str) -> Option<u64> {
    let end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    let (number, unit) = s.split_at(end);
    let power = [
        ("", (1u64, 0)),
        ("b", (1, 0)),
        ("kb", (1000, 1)),
        ("mb", (1000, 2)),
        ("gb", (1000, 3)),
        ("tb", (1000, 4)),
        ("kib", (1024, 1)),
        ("mib", (1024, 2)),
        ("gib", (1024, 3)),
        ("tib", (1024, 4)),
    ]
    .into_iter()
    .find(|(name, _)| unit.eq_ignore_ascii_case(name))?
    .1;
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || fraction.len() > 3
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || number.ends_with('.')
    {
        return None;
    }
    let scale = 10u64.pow(fraction.len() as u32);
    let n = whole
        .parse::<u64>()
        .ok()?
        .checked_mul(scale)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse().ok()?
        })?;
    let product = u128::from(n) * u128::from(power.0.pow(power.1));
    if product % u128::from(scale) != 0 {
        return None;
    }
    u64::try_from(product / u128::from(scale)).ok()
}
pub(crate) fn date(s: &str) -> Option<i64> {
    if s.len() != 10
        || s.as_bytes()[4] != b'-'
        || s.as_bytes()[7] != b'-'
        || !s
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let m: i64 = s[5..7].parse().ok()?;
    let d: i64 = s[8..].parse().ok()?;
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if y == 0 || !(1..=days).contains(&d) {
        return None;
    }
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yo = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    Some((era * 146097 + yo * 365 + yo / 4 - yo / 100 + doy - 719468) * 86400)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_units_ranges_and_overflow() {
        assert_eq!(bytes("1.5MiB"), Some(1572864));
        assert_eq!(bytes("1MB"), Some(1000000));
        for s in [
            "-1",
            "1.0001MB",
            "0.1B",
            "18446744073709551616",
            "1XB",
            "1.",
        ] {
            assert!(bytes(s).is_none(), "{s}");
        }
        assert!(Predicate::parse("size", "2..1").is_none());
        assert!(Predicate::parse("size", "<0").is_none());
        assert!(
            Predicate::parse("size", "1KiB..2KiB")
                .unwrap()
                .matches(1024, UNIX_EPOCH)
        );
        assert!(
            !Predicate::parse("size", ">1KiB")
                .unwrap()
                .matches(1024, UNIX_EPOCH)
        );
    }
    #[test]
    fn timestamp_bounds_are_inclusive_after_exclusive_before_with_subsecond_floor() {
        let after = Predicate::parse("after", "1970-01-01T00:00:01Z").unwrap();
        let before = Predicate::parse("before", "1970-01-01T00:00:01Z").unwrap();
        let just_before = UNIX_EPOCH + std::time::Duration::from_nanos(999999999);
        let exact = UNIX_EPOCH + std::time::Duration::from_secs(1);
        assert!(!after.matches(0, just_before) && before.matches(0, just_before));
        assert!(after.matches(0, exact) && !before.matches(0, exact));
        let negative = Predicate::parse("before", "1970-01-01T00:00:00Z").unwrap();
        assert!(negative.matches(0, UNIX_EPOCH - std::time::Duration::from_nanos(1)));
    }
    #[test]
    fn calendar_boundaries_and_negative_epoch() {
        assert_eq!(date("1970-01-01"), Some(0));
        assert_eq!(date("1969-12-31"), Some(-86400));
        assert_eq!(
            date("2000-03-01").unwrap() - date("2000-02-28").unwrap(),
            172800
        );
        for s in [
            "1900-02-29",
            "2026-02-30",
            "2026-13-01",
            "0000-01-01",
            "２０２６-01",
        ] {
            assert!(date(s).is_none());
        }
        assert!(
            Predicate::parse("after", "1970-01-01")
                .unwrap()
                .matches(0, UNIX_EPOCH)
        );
        assert!(
            !Predicate::parse("before", "1970-01-01")
                .unwrap()
                .matches(0, UNIX_EPOCH)
        );
    }
}
