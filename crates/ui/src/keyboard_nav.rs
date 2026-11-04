//! Browser key navigation. No event globals, filesystem work or per-key order clone.
use crate::model::{Model, View};
use fltk::enums::Key;
/// Context-sensitive F5, shared by the real event dispatcher and owned fixtures.
pub fn f5_action(m: &Model) -> crate::model::Action {
    if crate::selection_tools::is_map(m) {
        crate::model::Action::SpaceRefresh
    } else {
        crate::model::Action::CopyFiles
    }
}
pub fn f5(m: &mut Model) {
    m.action(f5_action(m));
}
/// Listing focus: parent navigation, even when a retained filter is nonempty.
/// Focused input consumes Backspace before reaching this browser handler.
pub fn backspace(m: &mut Model) {
    if let Some(parent) = m.path.parent() {
        m.navigate(parent.to_path_buf());
    }
}
/// Saturating rank movement; an unfocused Down starts at the first row, Up at last.
pub fn target(order: &[usize], selected: Option<usize>, delta: i32) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    if delta == i32::MIN {
        return Some(0);
    }
    if delta == i32::MAX {
        return Some(order.len() - 1);
    }
    Some(
        match selected.and_then(|i| order.iter().position(|&j| j == i)) {
            Some(p) => (p as i64 + i64::from(delta)).clamp(0, order.len() as i64 - 1) as usize,
            None => {
                if delta < 0 {
                    order.len() - 1
                } else {
                    0
                }
            }
        },
    )
}
pub fn key(m: &mut Model, key: Key, ctrl: bool, shift: bool, w: i32, h: i32) -> bool {
    if !matches!(
        key,
        Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    ) {
        return false;
    }
    if m.editing || m.closing || m.selection_refreshing() {
        return true;
    }
    if m.view == View::Columns
        && !m.dual.enabled
        && !ctrl
        && !shift
        && matches!(key, Key::Left | Key::Right)
    {
        m.column_step(key == Key::Right);
        return true;
    }
    let grid = m.view == View::Grid && !m.dual.enabled;
    let groups = m.view == View::Groups && !m.dual.enabled;
    let folder = groups
        && m.selected
            .and_then(|i| m.entries.get(i))
            .is_some_and(|e| e.is_dir());
    let metrics =
        crate::grid_view::Metrics::new(crate::grid_view::width(m, w), m.grid_zoom, m.order.len());
    let columns = if grid {
        metrics.columns as i32
    } else if folder {
        let rail = if m.rail { 170 } else { 52 };
        let inspector = if m.inspector && w >= 800 { 240 } else { 0 };
        ((w - inspector - rail - 32) / 240).max(1)
    } else {
        1
    };
    let h = crate::operation_bar::browser_height(m, h);
    let page = if m.view == View::SpaceMap && !m.dual.enabled {
        m.space_geometry.page_rows()
    } else if grid {
        columns * ((h - 170) / metrics.cell_height).max(1)
    } else if m.dual.enabled || m.view == View::Columns {
        ((h - 230) / 36).max(1)
    } else if folder {
        columns * ((h - 162) / 82).max(1)
    } else {
        ((h - 164) / 38).max(1)
    };
    let mut delta = match key {
        Key::Home => i32::MIN,
        Key::End => i32::MAX,
        Key::Up => -columns,
        Key::Down => columns,
        Key::Left => -1,
        Key::Right => 1,
        Key::PageUp => -page,
        Key::PageDown => page,
        _ => unreachable!(),
    };
    if folder && matches!(key, Key::Down | Key::PageDown) {
        let count = m.order.partition_point(|&i| m.entries[i].is_dir());
        if let Some(rank) = m
            .selected
            .and_then(|i| m.order.iter().position(|&j| j == i))
            && count < m.order.len()
            && rank.saturating_add(delta as usize) >= count
        {
            delta = (count - rank) as i32;
        }
    }
    m.move_browser_focus(delta, shift, ctrl);
    crate::view::reveal_selection(m, w, h);
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_focus_boundaries_and_extremes_never_skip_or_overflow() {
        let a = [4, 2, 7];
        assert_eq!(target(&[], None, 1), None);
        assert_eq!(target(&a, None, 1), Some(0));
        assert_eq!(target(&a, None, -1), Some(2));
        assert_eq!(target(&a, Some(2), i32::MIN), Some(0));
        assert_eq!(target(&a, Some(2), i32::MAX), Some(2));
        assert_eq!(target(&a, Some(4), -200), Some(0));
        assert_eq!(target(&a, Some(7), 200), Some(2));
    }
}
