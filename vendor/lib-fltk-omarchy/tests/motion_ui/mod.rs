//! UI checks of motion (anim, layout motion, Sidebar, popups, tabs, rows).
//! Animations are forced on with a frozen clock: the first frame shows the
//! old state, a middle frame differs from both ends, the last frame is
//! pixel-identical to the same UI built with animations off.
use super::{check, png_rgb, pump_until};
use fltk::prelude::*;
use fltk_omarchy::anim::{self, clock};
use fltk_omarchy::focus::{self, press};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::model::ListModel;
use fltk_omarchy::panel::KeyboardPanel;
use fltk_omarchy::popups::{PopupItem, PopupMenu};
use fltk_omarchy::table::{Tree, TreeNode};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{i18n, App, AppWindow, State};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Check<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

fn shot(win: &AppWindow, dir: &Path, name: &str) -> Result<PathBuf, String> {
    let p = dir.join(name);
    win.clone().snapshot(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Same pixels, within 2 levels per channel: Windows' GDI blit rounds
/// blended (veiled, transparent) colors by one.
fn same(a: &Path, b: &Path) -> Result<bool, String> {
    let (a, b) = (png_rgb(a)?, png_rgb(b)?);
    Ok(close(&a.raw, &b.raw))
}

fn close(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 2)
}

/// Same pixels from row `y0` down.
fn same_from(a: &Path, b: &Path, y0: i32) -> Result<bool, String> {
    let (a, b) = (png_rgb(a)?, png_rgb(b)?);
    let line = a.w * a.depth + 1;
    let from = (y0.max(0) as usize * line).min(a.raw.len());
    Ok(a.raw.len() == b.raw.len() && close(&a.raw[from..], &b.raw[from..]))
}

fn ms(n: u64) {
    clock::advance(Duration::from_millis(n));
}

/// Animations on, frozen clock; back to the test defaults afterwards.
struct Motion;
impl Motion {
    fn on() -> Motion {
        anim::set_enabled(true);
        clock::freeze();
        Motion
    }
}
impl Drop for Motion {
    fn drop(&mut self) {
        clock::resume();
        anim::set_enabled(false);
    }
}

fn delete(win: AppWindow) {
    fltk::window::DoubleWindow::delete((*win).clone());
}

struct Shell {
    win: AppWindow,
    side: Sidebar,
    button: Button,
}

/// A sidebar beside a column of content.
fn shell(mode: SidebarMode) -> Shell {
    let mut win = AppWindow::new("Motion", 560, 300);
    let mut body = win.body();
    body.set_margin(0);
    let view = State::new(0usize);
    let mut row = layout::row();
    let side = Sidebar::new(
        vec![
            SidebarItem::new(glyph::FOLDER, "Files").badge("12"),
            SidebarItem::new(glyph::SEARCH, "Search"),
            SidebarItem::new(glyph::SETTINGS, "Settings").footer(),
        ],
        &view,
    )
    .mode(mode);
    row.fit(&*side);
    let mut col = layout::column();
    let button = Button::new("Content").bordered();
    col.fit(&*button);
    col.end();
    row.end();
    win.end();
    Shell { win, side, button }
}

pub fn checks(run: &mut dyn FnMut(&str, Check)) {
    run(
        "motion: a sidebar's width slides (190 ms), ends on FLTK's layout; off jumps",
        &|shots| {
            // Reference: the expanded layout with animations off.
            anim::set_enabled(false);
            let reference = shell(SidebarMode::Push);
            reference.side.expand();
            let want = shot(&reference.win, shots, "motion-side-ref.png")?;
            let (wide, wide_x) = (reference.side.w(), reference.button.x());
            delete(reference.win);

            let _m = Motion::on();
            let s = shell(SidebarMode::Push);
            let before = shot(&s.win, shots, "motion-side-0.png")?;
            let (rail, rail_x) = (s.side.w(), s.button.x());
            s.side.expand();
            let first = shot(&s.win, shots, "motion-side-1-t0.png")?;
            check(
                s.side.w() == rail && same(&first, &before)?,
                &format!("first frame is the old layout: {}", s.side.w()),
            )?;
            check(layout::animating(), "moving")?;
            ms(60);
            let mid = shot(&s.win, shots, "motion-side-2-t60.png")?;
            let (mw, mx) = (s.side.w(), s.button.x());
            check(
                rail < mw && mw < wide && rail_x < mx && mx < wide_x,
                &format!(
                    "60 ms: width {rail} < {mw} < {wide}, content x {rail_x} < {mx} < {wide_x}"
                ),
            )?;
            check(
                !same(&mid, &before)? && !same(&mid, &want)?,
                "a middle frame differs from both ends",
            )?;
            // OutCubic: past halfway at a third of the time.
            check(
                (mw - rail) as f64 > 0.5 * (wide - rail) as f64,
                &format!("OutCubic: {mw} past halfway at 60 of 190 ms"),
            )?;
            ms(200);
            let end = shot(&s.win, shots, "motion-side-3-end.png")?;
            check(
                s.side.w() == wide && s.button.x() == wide_x,
                &format!("ends at {} / {}", s.side.w(), s.button.x()),
            )?;
            check(
                same(&end, &want)?,
                "last frame = the layout without animations",
            )?;
            check(!layout::animating(), "stopped")?;
            // Off: at once.
            anim::set_enabled(false);
            s.side.collapse();
            shot(&s.win, shots, "motion-side-4-off.png")?;
            check(
                s.side.w() == rail && !layout::animating(),
                &format!("off: jumps to {}", s.side.w()),
            )?;
            delete(s.win);
            Ok(())
        },
    );
    run(
        "motion: a window resize mid-way snaps to the final layout and keeps fixed sizes",
        &|shots| {
            let _m = Motion::on();
            let s = shell(SidebarMode::Push);
            shot(&s.win, shots, "motion-resize-0.png")?;
            s.side.expand();
            shot(&s.win, shots, "motion-resize-1.png")?;
            ms(60);
            shot(&s.win, shots, "motion-resize-2.png")?;
            let mid = s.side.w();
            let mut w = s.win.clone();
            let (wx, wy) = (w.x(), w.y());
            w.resize(wx, wy, 640, 320);
            shot(&s.win, shots, "motion-resize-3.png")?;
            let wide = theme_wide(&s.side);
            check(
                mid < wide && s.side.w() == wide,
                &format!("resized at {mid}: final width {} (want {wide})", s.side.w()),
            )?;
            check(!layout::animating(), "the resize is not animated")?;
            let gap = s.button.x() - (s.side.x() + s.side.w());
            check(
                (0..=16).contains(&gap),
                &format!("content follows: gap {gap}"),
            )?;
            delete(s.win);
            Ok(())
        },
    );
    run("motion: set_visible collapses and fades out, show grows in; first layout and RTL flips are not animated", &|shots| {
        let build = || {
            let mut win = AppWindow::new("t", 360, 240);
            let mut body = win.body();
            let a = Button::new("First").bordered();
            body.fit(&*a);
            let b = Button::new("Second").bordered();
            body.fit(&*b);
            let c = Button::new("Third").bordered();
            body.fit(&*c);
            win.end();
            (win, a, b, c)
        };
        anim::set_enabled(false);
        let (rw, _, rb, _) = build();
        layout::set_visible(&*rb, false);
        let want = shot(&rw, shots, "motion-hide-ref.png")?;
        delete(rw);

        let _m = Motion::on();
        let (win, _a, b, c) = build();
        let before = shot(&win, shots, "motion-hide-0.png")?;
        check(!layout::animating(), "first layout: no motion")?;
        let (by, cy) = (b.y(), c.y());
        layout::set_visible(&*b, false);
        check(!b.visible(), "hidden at once (FLTK state is final)")?;
        let first = shot(&win, shots, "motion-hide-1-t0.png")?;
        check(same(&first, &before)?, "first frame: the old picture (the hidden button is a ghost)")?;
        ms(80);
        let mid = shot(&win, shots, "motion-hide-2-t80.png")?;
        check(c.y() > by && c.y() < cy && !same(&mid, &want)?, &format!("80 ms: third at {} between {by} and {cy}", c.y()))?;
        ms(200);
        let end = shot(&win, shots, "motion-hide-3-end.png")?;
        check(c.y() == by && same(&end, &want)?, "end: the gap is closed, same picture as without animations")?;
        layout::set_visible(&*b, true);
        shot(&win, shots, "motion-show-0.png")?;
        ms(60);
        let grow = shot(&win, shots, "motion-show-1-t60.png")?;
        check(b.visible() && c.y() > by && c.y() < cy && !same(&grow, &before)?, &format!("showing: third pushed to {}", c.y()))?;
        ms(200);
        let back = shot(&win, shots, "motion-show-2-end.png")?;
        check(same(&back, &before)?, "shown again: the first picture")?;
        // A direction flip re-lays out every row: no motion.
        i18n::set_locale("ar");
        shot(&win, shots, "motion-rtl.png")?;
        let flipped = !layout::animating();
        i18n::set_locale("en");
        shot(&win, shots, "motion-ltr.png")?;
        check(flipped && !layout::animating(), "direction flips are not animated")?;
        delete(win);
        Ok(())
    });
    run(
        "motion: popups fade in (180 ms) and out (120 ms); focus comes back at once",
        &|shots| {
            let _m = Motion::on();
            let mut win = AppWindow::new("t", 400, 300);
            let mut body = win.body();
            let anchor = Button::new("Menu").bordered();
            body.fit(&*anchor);
            win.end();
            let before = shot(&win, shots, "motion-pop-0.png")?;
            let kids = win.children();
            let _ = anchor.clone().take_focus();
            let h = PopupMenu::open(
                &*anchor,
                vec![PopupItem::new("", "One"), PopupItem::new("", "Two")],
                |_| {},
            );
            let (x, y, w, _) = h.rect().ok_or("menu rect")?;
            let (px, py) = (x + w - 12, y + 6);
            let t0 = shot(&win, shots, "motion-pop-1-t0.png")?;
            ms(60);
            let mid = shot(&win, shots, "motion-pop-2-t60.png")?;
            ms(200);
            let open = shot(&win, shots, "motion-pop-3-open.png")?;
            let (a, m, z) = (
                super::pixel(&t0, px, py)?,
                super::pixel(&mid, px, py)?,
                super::pixel(&open, px, py)?,
            );
            let bg = super::pixel(&before, px, py)?;
            let d = |p: [u8; 3], q: [u8; 3]| {
                (0..3)
                    .map(|i| (p[i] as i32 - q[i] as i32).abs())
                    .sum::<i32>()
            };
            check(
                d(a, bg) <= 3,
                &format!("starts at the background: {a:?} vs {bg:?}"),
            )?;
            check(
                d(m, bg) > 0 && d(m, z) > 0,
                &format!("60 ms: between {bg:?} and {z:?}: {m:?}"),
            )?;
            check(d(z, bg) > 3, &format!("open: the card {z:?}"))?;
            h.close();
            check(focus::is_focused(&*anchor), "focus is back at once")?;
            check(
                win.children() == kids + 1,
                "the layer stays while it fades out",
            )?;
            let fading = shot(&win, shots, "motion-pop-4-closing.png")?;
            check(
                !same(&fading, &before)?,
                "first closing frame still shows it",
            )?;
            // It is deleted after the fade even with the clock frozen (a timer).
            let t = Instant::now();
            pump_until(|| win.children() == kids);
            check(
                win.children() == kids && t.elapsed() >= Duration::from_millis(100),
                &format!("deleted after {:?}", t.elapsed()),
            )?;
            let after = shot(&win, shots, "motion-pop-5-closed.png")?;
            check(same(&after, &before)?, "closed: the window as before")?;
            delete(win);
            Ok(())
        },
    );
    run("motion: the tab indicator slides (160 ms)", &|shots| {
        let _m = Motion::on();
        let mut win = AppWindow::new("t", 480, 120);
        let mut body = win.body();
        let v = State::new(0usize);
        let tabs = TabBar::new(&["One", "Two", "Three"], &v);
        body.fit(&*tabs);
        win.end();
        shot(&win, shots, "motion-tab-0.png")?;
        let acc = fltk_omarchy::theme::current().palette.accent;
        let accent_xs = |p: &Path| -> Result<Vec<i32>, String> {
            let y = tabs.y() + tabs.h() - 2;
            let mut xs = Vec::new();
            for x in tabs.x()..tabs.x() + tabs.w() {
                if super::pixel(p, x, y)? == [acc.r(), acc.g(), acc.b()] {
                    xs.push(x);
                }
            }
            Ok(xs)
        };
        v.set(2);
        let a = accent_xs(&shot(&win, shots, "motion-tab-1-t0.png")?)?;
        ms(50);
        let b = accent_xs(&shot(&win, shots, "motion-tab-2-t50.png")?)?;
        ms(200);
        let c = accent_xs(&shot(&win, shots, "motion-tab-3-end.png")?)?;
        let (a0, b0, c0) = (
            a.first().copied().unwrap_or(-1),
            b.first().copied().unwrap_or(-1),
            c.first().copied().unwrap_or(-1),
        );
        check(
            a0 >= 0 && a0 < b0 && b0 < c0,
            &format!("accent line starts at x {a0} → {b0} → {c0}"),
        )?;
        delete(win);
        Ok(())
    });
    run(
        "motion: list rows (model) make room, arrive and leave; last frame = no animation",
        &|shots| {
            let build = |items: &[&str]| {
                let model = ListModel::new(|s: &String| s.clone());
                for i in items {
                    model.push(i.to_string());
                }
                let mut win = AppWindow::new("t", 320, 300);
                let mut body = win.body();
                let list = List::from_model(&model, |s: &String| ListRow::new(s));
                body.fit(&*list);
                body.fixed(&*list, 240);
                win.end();
                (win, model, list)
            };
            anim::set_enabled(false);
            let (rw, _, _) = build(&["new", "a", "b", "c", "d"]);
            let want_insert = shot(&rw, shots, "motion-list-ref-insert.png")?;
            delete(rw);
            let (rw, _, _) = build(&["new", "a", "c", "d"]);
            let want_remove = shot(&rw, shots, "motion-list-ref-remove.png")?;
            delete(rw);

            let _m = Motion::on();
            let (win, model, _list) = build(&["a", "b", "c", "d"]);
            let before = shot(&win, shots, "motion-list-0.png")?;
            model.insert(0, "new".to_string());
            let t0 = shot(&win, shots, "motion-list-1-t0.png")?;
            check(
                same(&t0, &before)?,
                "first frame: rows where they were, the new one transparent",
            )?;
            ms(200);
            let mid = shot(&win, shots, "motion-list-2-t200.png")?;
            check(
                !same(&mid, &before)? && !same(&mid, &want_insert)?,
                "middle frame differs from both ends",
            )?;
            ms(600);
            let end = shot(&win, shots, "motion-list-3-end.png")?;
            check(same(&end, &want_insert)?, "last frame = no animation")?;
            model.remove_key(&"b".to_string());
            let r0 = shot(&win, shots, "motion-list-4-remove-t0.png")?;
            check(
                same(&r0, &end)?,
                "removed: first frame shows it where it was",
            )?;
            ms(150);
            let r1 = shot(&win, shots, "motion-list-5-remove-mid.png")?;
            check(
                !same(&r1, &end)? && !same(&r1, &want_remove)?,
                "leaving: a middle frame",
            )?;
            ms(600);
            let r2 = shot(&win, shots, "motion-list-6-remove-end.png")?;
            check(same(&r2, &want_remove)?, "gone: same as without animations")?;
            delete(win);
            Ok(())
        },
    );
    run(
        "motion: tree branches open and close (200 ms); last frame = no animation",
        &|shots| {
            let build = |open: bool| {
                let mut win = AppWindow::new("t", 320, 300);
                let mut body = win.body();
                let panel = KeyboardPanel::new("tree");
                let tree = Tree::new(&panel, |p| match p {
                    None => vec![
                        TreeNode::new(1, "src").branch(),
                        TreeNode::new(2, "docs"),
                        TreeNode::new(3, "tests"),
                    ],
                    Some(1) => vec![TreeNode::new(10, "lib.rs"), TreeNode::new(11, "anim.rs")],
                    _ => vec![],
                });
                body.fit(&*tree);
                body.fixed(&*tree, 240);
                win.end();
                if open {
                    tree.expand(1);
                }
                (win, tree)
            };
            anim::set_enabled(false);
            let (rw, _) = build(true);
            let want = shot(&rw, shots, "motion-tree-ref.png")?;
            delete(rw);
            let _m = Motion::on();
            let (win, tree) = build(false);
            let before = shot(&win, shots, "motion-tree-0.png")?;
            tree.expand(1);
            let t0 = shot(&win, shots, "motion-tree-1-t0.png")?;
            // The chevron turns at once; the rows below have not moved yet.
            let below = tree.y() + fltk_omarchy::theme::current().spacing.control_height;
            check(
                same_from(&t0, &before, below)?,
                "first frame: rows below where they were",
            )?;
            ms(100);
            let mid = shot(&win, shots, "motion-tree-2-t100.png")?;
            check(!same(&mid, &before)? && !same(&mid, &want)?, "middle frame")?;
            ms(500);
            let end = shot(&win, shots, "motion-tree-3-end.png")?;
            check(same(&end, &want)?, "opened: same as without animations")?;
            tree.collapse(1);
            let c0 = shot(&win, shots, "motion-tree-4-closing.png")?;
            check(
                same_from(&c0, &end, below)?,
                "closing: first frame keeps the rows",
            )?;
            ms(600);
            let closed = shot(&win, shots, "motion-tree-5-closed.png")?;
            check(same(&closed, &before)?, "closed: the first picture")?;
            delete(win);
            Ok(())
        },
    );
    run("sidebar: hover intent (350 ms), leave grace, keyboard (focus expands, arrows, Enter, Esc), pin", &|shots| {
        anim::set_enabled(false);
        let s = shell(SidebarMode::Push);
        let picked = Rc::new(RefCell::new(Vec::new()));
        let p2 = picked.clone();
        let side = s.side.clone().on_activate(move |i| p2.borrow_mut().push(i));
        shot(&s.win, shots, "sidebar-0.png")?;
        let rail = side.w();
        let t = Instant::now();
        side.hover(true);
        while t.elapsed() < Duration::from_millis(200) {
            App::pump();
            std::thread::sleep(Duration::from_millis(5));
        }
        check(!side.is_expanded(), "not before the hover intent")?;
        pump_until(|| side.is_expanded());
        check(side.is_expanded() && t.elapsed() >= Duration::from_millis(340), &format!("expanded after {:?}", t.elapsed()))?;
        shot(&s.win, shots, "sidebar-1-hover.png")?;
        check(side.w() > rail, "wider")?;
        side.hover(false);
        pump_until(|| !side.is_expanded());
        check(!side.is_expanded(), "collapses after the pointer leaves")?;
        // Keyboard: focus inside expands at once.
        let _ = side.clone().take_focus();
        check(side.is_expanded(), "focus expands")?;
        press("Down");
        press("Enter");
        check(*picked.borrow() == vec![1] && side.cursor() == 1, &format!("Down + Enter picks 1: {:?}", picked.borrow()))?;
        press("Escape");
        check(!side.is_expanded() && focus::is_focused(&*side), "Esc collapses, focus stays")?;
        side.set_pinned(true);
        check(side.is_expanded(), "pinned: expanded")?;
        side.hover(false);
        press("Escape");
        check(side.is_expanded(), "pinned: Esc and leaving keep it open")?;
        side.set_pinned(false);
        delete(s.win);
        // Overlay: the rail keeps its width, a layer covers the content.
        let o = shell(SidebarMode::Overlay);
        shot(&o.win, shots, "sidebar-2-overlay-0.png")?;
        let (rail, bx) = (o.side.w(), o.button.x());
        o.side.expand();
        shot(&o.win, shots, "sidebar-3-overlay-open.png")?;
        check(o.side.w() == rail && o.button.x() == bx && o.side.overlay_width() > rail, "overlay: content does not move")?;
        o.side.collapse();
        shot(&o.win, shots, "sidebar-4-overlay-closed.png")?;
        pump_until(|| o.side.overlay_width() == 0);
        check(o.side.overlay_width() == 0, "overlay closed")?;
        delete(o.win);
        // Right to left: the rail is on the right.
        i18n::set_locale("ar");
        let r = shell(SidebarMode::Push);
        shot(&r.win, shots, "sidebar-5-rtl.png")?;
        let ok = r.side.x() > r.button.x();
        delete(r.win);
        i18n::set_locale("en");
        check(ok, "rtl: rail on the right")
    });
}

fn theme_wide(s: &Sidebar) -> i32 {
    // The expanded width (default 240, wider for large fonts).
    let _ = s;
    240.max(fltk_omarchy::theme::current().font.body * 20)
}

/// Screen readers: the sidebar is a tab list with its expanded state.
#[cfg(feature = "a11y")]
pub fn a11y_checks(run: &mut dyn FnMut(&str, Check)) {
    run(
        "a11y: sidebar is a tab list with its expanded state and selected item",
        &|shots| {
            anim::set_enabled(false);
            let s = shell(SidebarMode::Push);
            shot(&s.win, shots, "a11y-sidebar.png")?;
            let snap = fltk_omarchy::a11y::snapshot(&*s.win).ok_or("no tree")?;
            let collapsed = snap
                .lines()
                .any(|l| l.contains("TabList \"Sidebar\"") && l.contains("collapsed"));
            let files = snap.lines().any(|l| {
                l.contains("Tab \"Files\"") && l.contains("selected") && l.contains("1 of 3")
            });
            s.side.expand();
            shot(&s.win, shots, "a11y-sidebar-open.png")?;
            let snap2 = fltk_omarchy::a11y::snapshot(&*s.win).ok_or("no tree")?;
            let expanded = snap2
                .lines()
                .any(|l| l.contains("TabList \"Sidebar\"") && l.contains("expanded"));
            delete(s.win);
            check(collapsed && files && expanded, &format!("collapsed {collapsed}, Files selected {files}, expanded {expanded}:\n{snap}\n{snap2}"))
        },
    );
}
