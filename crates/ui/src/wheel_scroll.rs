//! Adapts the toolkit's shared motion to the bounded canvas scroll offsets.
use crate::{
    columns::MAX_COLUMNS,
    model::{Model, View},
};
use fltk::{frame::Frame, prelude::*};
use fltk_omarchy::{anim, scroll::Scroll};

const SLOTS: usize = MAX_COLUMNS + 3;
#[derive(PartialEq, Eq)]
struct Scope {
    generation: u64,
    view: View,
    active: u64,
    other: Option<(u64, u64)>,
    dual: bool,
    column: usize,
    columns: [u64; MAX_COLUMNS],
}

#[derive(Default)]
pub struct State {
    scope: Option<Scope>,
    slots: [Scroll; SLOTS],
}
impl State {
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
    fn sync(&mut self, m: &Model) {
        let scope = Scope {
            generation: m.generation,
            view: m.view,
            active: m.dual.active_id,
            other: m.dual.other.as_ref().map(|b| (b.id, b.revision)),
            dual: m.dual.enabled,
            column: m.columns.active,
            columns: std::array::from_fn(|i| m.columns.columns.get(i).map_or(0, |c| c.id)),
        };
        if self.scope.as_ref() != Some(&scope) {
            self.cancel();
            self.scope = Some(scope);
        }
    }
}

fn offsets(m: &Model, h: i32) -> [Option<(i32, i32)>; SLOTS] {
    let mut out = [None; SLOTS];
    let browser_h = crate::operation_bar::browser_height(m, h);
    if m.dual.enabled {
        let available = (browser_h - 226).max(0);
        out[0] = Some((m.scroll, (m.order.len() as i32 * 36 - available).max(0)));
        if let Some(b) = &m.dual.other {
            out[1] = Some((b.scroll, (b.order.len() as i32 * 36 - available).max(0)));
        }
    } else if m.view == View::Columns {
        let available = (browser_h - 228).max(0);
        for (i, c) in m.columns.columns.iter().take(MAX_COLUMNS).enumerate() {
            let (top, count) = if i == m.columns.active {
                (m.scroll, m.order.len())
            } else {
                (c.data.scroll, c.data.order.len())
            };
            out[2 + i] = Some((top, (count as i32 * 36 - available).max(0)));
        }
        out[SLOTS - 1] = Some((
            m.columns.horizontal,
            (m.columns.columns.len() as i32 * crate::columns_view::WIDTH - m.columns.area.w).max(0),
        ));
    } else {
        let max = if m.view == View::SpaceMap {
            m.space_geometry.max_scroll()
        } else {
            (m.content_height - (browser_h - 134).max(0)).max(0)
        };
        out[0] = Some((m.scroll, max));
    }
    out
}
fn set(m: &mut Model, slot: usize, value: i32) {
    match slot {
        0 => m.scroll = value,
        1 => {
            if let Some(b) = &mut m.dual.other {
                b.scroll = value;
            }
        }
        s if s == SLOTS - 1 => m.columns.horizontal = value,
        s => {
            let i = s - 2;
            if i == m.columns.active {
                m.scroll = value;
            } else if let Some(c) = m.columns.columns.get_mut(i) {
                c.data.scroll = value;
            }
        }
    }
}

pub fn wheel(m: &mut Model, x: i32, delta: i32, horizontal: bool, h: i32) {
    if delta == 0 {
        return;
    }
    let (slot, step) = if m.dual.enabled {
        let left = if m.rail { 186 } else { 68 };
        let on_right = x >= left + m.dual_width.max(0) / 2;
        (usize::from(on_right != m.dual.right_active), 36)
    } else if m.view == View::Columns && horizontal {
        (SLOTS - 1, 60)
    } else if m.view == View::Columns {
        let i = ((x - m.columns.area.x + m.columns.horizontal) / crate::columns_view::WIDTH).max(0)
            as usize;
        if i >= MAX_COLUMNS {
            return;
        }
        (2 + i, 36)
    } else {
        (0, if m.view == View::SpaceMap { 28 } else { 38 })
    };
    let Some((position, max)) = offsets(m, h)[slot] else {
        return;
    };
    let mut state = std::mem::take(&mut m.wheel_scroll);
    state.sync(m);
    let value =
        state.slots[slot].wheel(position as i64, delta as f64 * step as f64, max as i64) as i32;
    set(m, slot, value);
    m.wheel_scroll = state;
}

pub fn advance(m: &mut Model, canvas: &Frame) {
    let mut state = std::mem::take(&mut m.wheel_scroll);
    state.sync(m);
    let offsets = offsets(m, canvas.h());
    for (slot, offset) in offsets.into_iter().enumerate() {
        let Some((position, max)) = offset else {
            continue;
        };
        let value = state.slots[slot].position(position as i64, max as i64) as i32;
        set(m, slot, value);
        if state.slots[slot].running() {
            anim::request_redraw(canvas);
        }
    }
    m.wheel_scroll = state;
}
