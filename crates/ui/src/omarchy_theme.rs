//! Translate one validated toolkit theme into the existing Painter palette.
use files_core::theme::{Color, Corners};
use fltk_omarchy::theme::{self, Rgb};

fn color(rgb: Rgb) -> Color {
    Color(rgb.r(), rgb.g(), rgb.b())
}

pub fn corners(value: Corners) -> theme::Corners {
    match value {
        Corners::Omarchy => theme::Corners::Omarchy,
        Corners::Square => theme::Corners::Square,
        Corners::Rounded => theme::Corners::Rounded,
    }
}

pub fn apply(model: &mut crate::model::Model) {
    theme::with(|theme| {
        let p = &theme.palette;
        // Keep surfaces tied to the same validated theme after live reloads.
        // The toolkit does not yet expose the extended colors.toml tokens.
        let palette = files_core::theme::Palette {
            background: color(p.background),
            darker_background: color(theme.popups.fill(p.background)),
            lighter_background: color(theme.fill(&theme.controls.hover, p.background)),
            foreground: color(p.foreground),
            bright_foreground: color(p.foreground),
            dark_foreground: color(p.dim),
            accent: color(p.accent),
            selection: color(p.selection),
            muted: color(p.muted),
            red: color(p.red),
            green: color(p.green),
            yellow: color(p.yellow),
            blue: color(p.blue),
            cyan: color(p.cyan),
            magenta: color(p.magenta),
        };
        model.system_palette = palette;
        // Painter radii are expressed as a boolean scale throughout Files.
        model.radius = i32::from(theme.radius > 0);
        model.badge_radius = theme.badge_radius();
    });
    model.apply_theme();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_rgb_and_every_corner_preference() {
        assert_eq!(color(Rgb(0x12abef)), Color(0x12, 0xab, 0xef));
        assert_eq!(corners(Corners::Omarchy), theme::Corners::Omarchy);
        assert_eq!(corners(Corners::Square), theme::Corners::Square);
        assert_eq!(corners(Corners::Rounded), theme::Corners::Rounded);
    }
}
