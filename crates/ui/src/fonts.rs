use fltk::enums::Font;
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
static FAMILY: OnceLock<String> = OnceLock::new();
pub fn family() -> &'static str {
    FAMILY.get().map(String::as_str).unwrap_or("monospace")
}
static NERD: AtomicBool = AtomicBool::new(false);
/// Una consulta de fontconfig, sin enumerar ni rasterizar el catálogo de fuentes.
pub fn configure() -> String {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{family}", "monospace"])
        .output()
        .ok();
    let candidate = output
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let family = candidate.split(',').next().unwrap_or("").trim();
    let (family, nerd) = if family.contains("Nerd") {
        (family.to_owned(), true)
    } else {
        let fallback = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "FiraCode Nerd Font"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        if fallback.contains("Nerd") {
            (
                fallback
                    .split(',')
                    .next()
                    .unwrap_or("monospace")
                    .trim()
                    .to_owned(),
                true,
            )
        } else {
            ("monospace".into(), false)
        }
    };
    let _ = FAMILY.set(family.clone());
    NERD.store(nerd, Ordering::Relaxed);
    // FLTK's Cairo backend parses the Pango style suffix. Register only these
    // eight faces; never enumerate/rasterize the complete installed catalogue.
    apply_family(&family);
    eprintln!("FILES_FONT={family} nerd={nerd}");
    family
}
pub fn reapply() {
    apply_family(family());
}
fn apply_family(family: &str) {
    for (f, suffix) in [
        (Font::Helvetica, ""),
        (Font::Courier, ""),
        (Font::HelveticaBold, " Bold"),
        (Font::CourierBold, " Bold"),
        (Font::HelveticaItalic, " Italic"),
        (Font::CourierItalic, " Italic"),
        (Font::HelveticaBoldItalic, " Bold Italic"),
        (Font::CourierBoldItalic, " Bold Italic"),
    ] {
        Font::set_font(f, &format!("{family}{suffix}"));
    }
}
pub fn icon(glyph: &'static str, fallback: &'static str) -> &'static str {
    if NERD.load(Ordering::Relaxed) {
        glyph
    } else {
        fallback
    }
}
