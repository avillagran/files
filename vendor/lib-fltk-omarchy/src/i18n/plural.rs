//! CLDR plural categories for whole numbers.
//!
//! Only integer counts (file counts, items, minutes): the fraction rules
//! and the compact "many" of French/Spanish/Italian/Portuguese millions are
//! left out, those fall back to the plain key (the `other` form).

use super::locale;

/// A CLDR plural category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Plural {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl Plural {
    pub const ALL: [Plural; 6] = [
        Plural::Zero,
        Plural::One,
        Plural::Two,
        Plural::Few,
        Plural::Many,
        Plural::Other,
    ];

    /// Name used in catalog keys: `"{count} files|one"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Plural::Zero => "zero",
            Plural::One => "one",
            Plural::Two => "two",
            Plural::Few => "few",
            Plural::Many => "many",
            Plural::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Plural> {
        Plural::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// Category of the whole number `n` in the language of `tag` (`ru`, `pt-pt`…).
pub fn category(tag: &str, n: i64) -> Plural {
    let n = n.unsigned_abs();
    let (m10, m100) = (n % 10, n % 100);
    let slavic_few = (2..=4).contains(&m10) && !(12..=14).contains(&m100);
    match locale::language(tag) {
        "ja" | "ko" | "zh" | "th" | "vi" | "id" | "ms" | "lo" | "my" | "km" => Plural::Other,
        // European Portuguese counts 0 as plural; Brazilian (the default) does not.
        "pt" if tag == "pt-pt" => one_if(n == 1),
        "fr" | "pt" | "fa" | "hi" | "bn" | "am" | "zu" => one_if(n <= 1),
        "ru" | "uk" | "be" => {
            if m10 == 1 && m100 != 11 {
                Plural::One
            } else if slavic_few {
                Plural::Few
            } else {
                Plural::Many
            }
        }
        "pl" => {
            if n == 1 {
                Plural::One
            } else if slavic_few {
                Plural::Few
            } else {
                Plural::Many
            }
        }
        "cs" | "sk" => match n {
            1 => Plural::One,
            2..=4 => Plural::Few,
            _ => Plural::Other,
        },
        "ar" => match (n, m100) {
            (0, _) => Plural::Zero,
            (1, _) => Plural::One,
            (2, _) => Plural::Two,
            (_, 3..=10) => Plural::Few,
            (_, 11..=99) => Plural::Many,
            _ => Plural::Other,
        },
        "he" => match n {
            1 => Plural::One,
            2 => Plural::Two,
            _ => Plural::Other,
        },
        _ => one_if(n == 1),
    }
}

fn one_if(c: bool) -> Plural {
    if c {
        Plural::One
    } else {
        Plural::Other
    }
}

/// Categories a whole number can take in the language of `tag`, `Other`
/// last. A catalog provides each one but `Other` as a `key|category` entry.
pub fn categories(tag: &str) -> Vec<Plural> {
    let mut seen: Vec<Plural> = Vec::new();
    // Every rule above repeats within 0..200.
    for n in 0..200 {
        let c = category(tag, n);
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.retain(|c| *c != Plural::Other);
    seen.sort();
    seen.push(Plural::Other);
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use Plural::*;

    fn run(tag: &str, ns: &[i64]) -> Vec<Plural> {
        ns.iter().map(|n| category(tag, *n)).collect()
    }

    #[test]
    fn slavic_arabic_and_cjk() {
        let ns = [0, 1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111, 112, 122];
        assert_eq!(
            run("ru", &ns),
            [Many, One, Few, Few, Many, Many, Many, Many, One, Few, Many, One, Many, Many, Few]
        );
        assert_eq!(run("uk", &ns), run("ru", &ns));
        assert_eq!(
            run("pl", &ns),
            [Many, One, Few, Few, Many, Many, Many, Many, Many, Few, Many, Many, Many, Many, Few]
        );
        assert_eq!(
            run("cs", &[0, 1, 2, 4, 5, 21]),
            [Other, One, Few, Few, Other, Other]
        );
        assert_eq!(
            run("ar", &[0, 1, 2, 3, 10, 11, 99, 100, 102, 103, 111]),
            [Zero, One, Two, Few, Few, Many, Many, Other, Other, Few, Many]
        );
        assert_eq!(run("ja", &[0, 1, 2]), [Other, Other, Other]);
        assert_eq!(run("zh-hans", &[1]), [Other]);
        assert_eq!(run("fr", &[0, 1, 2]), [One, One, Other]);
        assert_eq!(run("pt", &[0, 1, 2]), [One, One, Other]);
        assert_eq!(run("pt-pt", &[0, 1, 2]), [Other, One, Other]);
        assert_eq!(run("en", &[0, 1, 2, -1]), [Other, One, Other, One]);
        assert_eq!(run("nb", &[1, 2]), [One, Other]);
        assert_eq!(categories("ru"), [One, Few, Many, Other]);
        assert_eq!(categories("ar"), [Zero, One, Two, Few, Many, Other]);
        assert_eq!(categories("ja"), [Other]);
        assert_eq!(categories("cs"), [One, Few, Other]);
        assert_eq!(categories("tr"), [One, Other]);
        assert_eq!(Plural::parse("few"), Some(Few));
    }
}
