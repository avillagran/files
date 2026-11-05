//! Default font family per OS and the FLTK face names for a family.
//!
//! Omarchy draws its whole UI in the monospace font. Linux asks fontconfig
//! for `monospace` (what `omarchy-font-set` changes); macOS and Windows do
//! not have that alias, so their bundled monospace fonts are used.
//! `OMARCHY_FLTK_FONT` overrides the family everywhere.

use super::{Os, OS};

/// The UI family when nothing else is configured.
pub fn default_family() -> &'static str {
    match OS {
        Os::MacOs => "Menlo",
        Os::Windows => "Consolas",
        _ => "monospace",
    }
}

/// FLTK face names (regular, bold) for `family`, in the form the OS font
/// backend expects: Pango descriptions on Linux, PostScript or full names
/// on macOS (Core Text), GDI names with FLTK's style prefix on Windows.
pub fn faces(family: &str) -> (String, String) {
    match OS {
        Os::MacOs if family == "Menlo" => ("Menlo-Regular".into(), "Menlo-Bold".into()),
        Os::Windows => (format!(" {family}"), format!("B{family}")),
        _ => (family.to_string(), format!("{family} Bold")),
    }
}

/// FLTK face names (italic, bold italic) for `family`, same backends as
/// [`faces`] (Windows: FLTK's `I`/`P` style prefixes).
pub fn italic_faces(family: &str) -> (String, String) {
    match OS {
        Os::MacOs if family == "Menlo" => ("Menlo-Italic".into(), "Menlo-BoldItalic".into()),
        Os::Windows => (format!("I{family}"), format!("P{family}")),
        _ => (format!("{family} Italic"), format!("{family} Bold Italic")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_follow_the_os_backend() {
        let (r, b) = faces("JetBrains Mono");
        match OS {
            Os::Windows => assert_eq!(
                (r.as_str(), b.as_str()),
                (" JetBrains Mono", "BJetBrains Mono")
            ),
            _ => assert_eq!(
                (r.as_str(), b.as_str()),
                ("JetBrains Mono", "JetBrains Mono Bold")
            ),
        }
        if OS == Os::Linux {
            assert_eq!(default_family(), "monospace");
        }
    }
}
