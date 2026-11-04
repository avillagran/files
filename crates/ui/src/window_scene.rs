//! Opt-in fixture commands. Never calls native move/resize or injects OS events.
use crate::{
    model::{Action, Model},
    window_chrome::{self, Zone},
};
use files_core::theme::WindowMode;

pub fn command(m: &mut Model, text: &str) -> Result<(), &'static str> {
    let action = match text {
        "preferences" => Some(Action::Settings),
        "actions" => Some(Action::Actions),
        "normal" => Some(Action::WindowMode(WindowMode::Normal)),
        "fullscreen" => Some(Action::WindowMode(WindowMode::Fullscreen)),
        "menus-close" => {
            m.settings = false;
            m.actions_menu = false;
            None
        }
        "close" => {
            m.closing = true;
            None
        }
        _ => return Err("Comando de ventana desconocido"),
    };
    if let Some(action) = action {
        let hit = m
            .hits
            .iter()
            .rev()
            .find(|hit| match (&action, &hit.action) {
                (Action::Settings, Action::Settings) | (Action::Actions, Action::Actions) => true,
                (Action::WindowMode(a), Action::WindowMode(b)) => a == b,
                _ => false,
            })
            .ok_or("El control no tiene un target visible")?;
        let (x, y) = (hit.rect.x + hit.rect.w / 2, hit.rect.y + hit.rect.h / 2);
        m.pointer_down(x, y, false, false, false);
    }
    Ok(())
}

pub fn validate(m: &Model, w: i32, h: i32) {
    assert!(!(m.settings && m.actions_menu));
    let is_file_action = |a: &Action| {
        matches!(
            a,
            Action::Trash
                | Action::Properties
                | Action::NewEntry(_)
                | Action::Rename
                | Action::CopyFiles
                | Action::ExtractArchive
                | Action::CreateArchive
                | Action::Undo(_)
                | Action::Open
                | Action::Preview
        )
    };
    let is_preference = |a: &Action| {
        matches!(
            a,
            Action::WindowMode(_) | Action::Theme(_) | Action::Corner(_)
        )
    };
    if m.settings {
        assert!(m.hits.iter().any(|h| is_preference(&h.action)));
        // Inspector has Open/Preview controls independently; only menu targets
        // in the menu rectangle participate in the separation contract.
        assert!(!m.hits.iter().any(|hit| hit.rect.x >= w - 330
            && hit.rect.y >= 58
            && hit.rect.y < 432
            && hit.rect.w == 276
            && is_file_action(&hit.action)));
    }
    if m.actions_menu {
        assert!(!m.hits.iter().any(|hit| is_preference(&hit.action)));
    }
    // No clipped breadcrumb can invisibly capture the free header gap.
    for hit in &m.hits {
        if hit.rect.y < 58 && matches!(hit.action, Action::Navigate(_)) {
            assert!(hit.rect.x + hit.rect.w <= crate::view::omni_right(w));
        }
    }
    // Every active header control (including breadcrumbs) excludes movement.
    for hit in &m.hits {
        if hit.rect.y < 58 && hit.rect.w > 0 && hit.rect.h > 0 {
            assert_eq!(
                window_chrome::at(
                    m,
                    w,
                    h,
                    hit.rect.x + hit.rect.w / 2,
                    hit.rect.y + hit.rect.h / 2
                ),
                None
            );
        }
    }
    if !window_chrome::blocked(m) && m.window_mode == WindowMode::Normal {
        for (x, y, z) in [
            (2, 2, Zone::TopLeft),
            (w - 3, 2, Zone::TopRight),
            (2, h - 3, Zone::BottomLeft),
            (w - 3, h - 3, Zone::BottomRight),
        ] {
            assert_eq!(window_chrome::at(m, w, h, x, y), Some(z));
        }
        assert_eq!(window_chrome::at(m, w, h, w / 2, 53), Some(Zone::Move));
        assert_eq!(window_chrome::at(m, w, h, w / 2, 100), None);
    }
}
