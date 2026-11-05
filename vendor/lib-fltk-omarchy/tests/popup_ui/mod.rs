//! UI checks of `PopupMenu` submenus: keys through `focus::press`, the
//! pointer through `focus::hover_at` / `click_at` (fltk's event path), the
//! hover delay on the frozen animation clock, placement in the window,
//! right to left, pixels of offscreen snapshots (animations off) and the
//! accessible tree.
use super::{check, pixel, pump_until};
use fltk::prelude::*;
use fltk_omarchy::anim::clock;
use fltk_omarchy::focus::{self, press};
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::popups::{Dismiss, PopupHandle, PopupItem, PopupMenu};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{i18n, theme, App, AppWindow};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Check<'a> = &'a dyn Fn(&Path) -> Result<(), String>;
type Rect = (i32, i32, i32, i32);

/// 0 Open, 1 Edit ▸ (Cut disabled, Copy, Paste, More ▸ (Paste special
/// disabled, Duplicate, Rename)), 2 Share ▸ (Email, Link), 3 Properties
/// (disabled), 4 Delete.
fn items() -> Vec<PopupItem> {
    vec![
        PopupItem::new(glyph::FOLDER, "Open").id("open"),
        PopupItem::submenu(
            glyph::FILE,
            "Edit",
            vec![
                PopupItem::new("", "Cut").disabled().id("cut"),
                PopupItem::new("", "Copy").shortcut("Mod+C").id("copy"),
                PopupItem::new("", "Paste").id("paste"),
                PopupItem::submenu(
                    "",
                    "More",
                    vec![
                        PopupItem::new("", "Paste special").disabled(),
                        PopupItem::new("", "Duplicate").id("duplicate"),
                        PopupItem::new("", "Rename").id("rename"),
                    ],
                ),
            ],
        ),
        PopupItem::submenu(
            glyph::SEARCH,
            "Share",
            vec![
                PopupItem::new("", "Email").id("email"),
                PopupItem::new("", "Link").id("link"),
            ],
        ),
        PopupItem::new("", "Properties").disabled(),
        PopupItem::new(glyph::TRASH, "Delete").id("delete"),
    ]
}

struct Ui {
    win: AppWindow,
    anchor: Button,
    kids: i32,
}

fn ui(w: i32, h: i32, shots: &Path) -> Ui {
    let mut win = AppWindow::new("t", w, h);
    let mut body = win.body();
    let anchor = Button::new("Menu");
    body.fit(&*anchor);
    win.end();
    let _ = win.snapshot(shots.join("submenu-layout.png")); // lays out
    let kids = win.children();
    Ui { win, anchor, kids }
}

impl Ui {
    fn focus_anchor(&self) {
        let _ = self.anchor.clone().take_focus();
    }
    fn shot(&self, shots: &Path, name: &str) -> Result<PathBuf, String> {
        let p = shots.join(name);
        self.win.clone().snapshot(&p).map_err(|e| e.to_string())?;
        Ok(p)
    }
    /// Layers left after closing (deleted on the next loop turn).
    fn layers(&self) -> i32 {
        App::pump();
        self.win.children() - self.kids
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        fltk::window::DoubleWindow::delete((*self.win).clone());
    }
}

/// Inner padding of a menu card and its row height.
fn metrics() -> (i32, i32) {
    let t = theme::current();
    (2 + t.spacing.xxs, t.spacing.popup_row_height)
}

/// Center of entry `i` of a card (no dividers in these menus).
fn row(r: Rect, i: i32) -> (i32, i32) {
    let (pad, rh) = metrics();
    (r.0 + r.2 / 2, r.1 + pad + i * rh + rh / 2)
}

fn pump_for(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        App::pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn advance(ms: u64) {
    clock::advance(Duration::from_millis(ms));
}

/// Picks and dismissals of one menu.
#[derive(Clone, Default)]
struct Log {
    paths: Rc<RefCell<Vec<Vec<usize>>>>,
    reasons: Rc<RefCell<Vec<Dismiss>>>,
}

impl Log {
    fn open(&self, u: &Ui) -> PopupHandle {
        u.focus_anchor();
        let (p, r) = (self.paths.clone(), self.reasons.clone());
        PopupMenu::open_path(&*u.anchor, items(), move |path| p.borrow_mut().push(path))
            .on_dismiss(move |d| r.borrow_mut().push(d))
    }
    fn last_path(&self) -> Option<Vec<usize>> {
        self.paths.borrow().last().cloned()
    }
}

/// Pixels in a box that differ from `reference` by more than a faint tint.
fn ink(
    png: &Path,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    reference: [u8; 3],
) -> Result<usize, String> {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = pixel(png, x, y)?;
            if (0..3)
                .map(|i| (p[i] as i32 - reference[i] as i32).abs())
                .sum::<i32>()
                > 60
            {
                n += 1;
            }
        }
    }
    Ok(n)
}

pub fn checks(run: &mut dyn FnMut(&str, Check)) {
    run(
        "popup submenus: keys open and close levels, nested paths, ids, disabled entries skipped",
        &|shots| {
            let u = ui(640, 420, shots);
            let log = Log::default();

            // Right on a submenu entry opens it on its first enabled entry (Cut is disabled).
            let h = log.open(&u);
            press("Down");
            press("Right");
            check(
                h.submenu_path() == vec![1] && u.layers() == 2,
                &format!(
                    "Right opens Edit: {:?}, {} layers",
                    h.submenu_path(),
                    u.layers()
                ),
            )?;
            press("Enter");
            check(
                log.last_path() == Some(vec![1, 1]),
                &format!("Copy picked as [1, 1]: {:?}", log.paths.borrow()),
            )?;
            check(
                h.dismissed() == Some(Dismiss::Picked(1))
                    && h.picked_path() == Some(vec![1, 1])
                    && *log.reasons.borrow() == vec![Dismiss::Picked(1)],
                &format!("Picked(top-level 1) once: {:?}", log.reasons.borrow()),
            )?;
            check(
                !h.is_open() && u.layers() == 0 && focus::is_focused(&*u.anchor),
                "every card closed, focus back on the anchor",
            )?;

            // Enter opens; Left and Esc close only that level, the cursor stays on its entry.
            let h = log.open(&u);
            press("Down");
            press("Enter");
            check(
                h.submenu_path() == vec![1],
                "Enter on a submenu entry opens it",
            )?;
            press("Left");
            check(
                h.submenu_path().is_empty() && h.is_open() && u.layers() == 1,
                "Left closes the submenu, the menu stays",
            )?;
            press("Right");
            press("Escape");
            check(
                h.submenu_path().is_empty() && h.is_open() && h.dismissed().is_none(),
                "Esc in a submenu closes only it",
            )?;
            press("Enter");
            check(h.submenu_path() == vec![1], "the cursor went back to Edit")?;
            // Up from Copy skips the disabled Cut and wraps to More; depth 2.
            press("Up");
            press("Right");
            check(
                h.submenu_path() == vec![1, 3],
                &format!("More opened at depth 2: {:?}", h.submenu_path()),
            )?;
            press("Left");
            check(h.submenu_path() == vec![1], "Left closes depth 2 only")?;
            press("Right");
            press("Enter");
            check(
                log.last_path() == Some(vec![1, 3, 1]),
                &format!(
                    "Duplicate (Paste special disabled) as [1, 3, 1]: {:?}",
                    log.last_path()
                ),
            )?;

            // Esc walks back level by level, then closes the menu (once).
            log.reasons.borrow_mut().clear();
            let h = log.open(&u);
            press("Down");
            press("Right");
            press("Up");
            press("Right");
            check(h.submenu_path() == vec![1, 3], "two levels open")?;
            press("Escape");
            check(h.submenu_path() == vec![1], "Esc: depth 2 closed")?;
            press("Escape");
            check(
                h.submenu_path().is_empty() && h.is_open(),
                "Esc: depth 1 closed",
            )?;
            press("Escape");
            check(
                !h.is_open() && *log.reasons.borrow() == vec![Dismiss::Escape] && u.layers() == 0,
                &format!("Esc on the root: Escape once: {:?}", log.reasons.borrow()),
            )?;
            check(focus::is_focused(&*u.anchor), "focus back after Esc")?;

            // Space acts like Enter; Right on an entry inside a submenu does nothing.
            let h = log.open(&u);
            press("Down");
            press("Space");
            check(h.submenu_path() == vec![1], "Space opens the submenu")?;
            press("Down");
            press("Right");
            check(
                h.is_open() && h.submenu_path() == vec![1],
                "Right on Paste: nothing",
            )?;
            press("Space");
            check(
                log.last_path() == Some(vec![1, 2]),
                &format!("Space picks Paste: {:?}", log.last_path()),
            )?;

            // open_id gives the entry's id; open gives the top-level index.
            let id = Rc::new(RefCell::new(String::new()));
            let i2 = id.clone();
            u.focus_anchor();
            PopupMenu::open_id(&*u.anchor, items(), move |s| {
                *i2.borrow_mut() = s.to_string()
            });
            for k in ["Down", "Right", "Up", "Right", "Down", "Enter"] {
                press(k);
            }
            check(
                *id.borrow() == "rename",
                &format!("open_id: {:?}", id.borrow()),
            )?;
            let top = Rc::new(RefCell::new(None));
            let t2 = top.clone();
            u.focus_anchor();
            PopupMenu::open(&*u.anchor, items(), move |i| *t2.borrow_mut() = Some(i));
            for k in ["Down", "Down", "Right", "Down", "Enter"] {
                press(k);
            }
            check(
                *top.borrow() == Some(2),
                &format!(
                    "open: Link reports its top-level entry 2: {:?}",
                    top.borrow()
                ),
            )?;

            // Right on a root entry without a submenu is not a menu key (as before).
            let h = log.open(&u);
            press("Right");
            check(
                !h.is_open() && h.dismissed() == Some(Dismiss::Closed),
                "Right on a root leaf closes the menu (Closed)",
            )?;
            Ok(())
        },
    );

    run(
        "popup submenus: hover opens after 200 ms (animation clock), aim keeps it, clicks",
        &|shots| {
            clock::freeze();
            let r = (|| -> Result<(), String> {
                let u = ui(640, 420, shots);
                let (pad, rh) = metrics();
                let log = Log::default();
                let h = log.open(&u);
                let r = h.rect().ok_or("rect")?;
                let (ex, ey) = row(r, 1);
                focus::hover_at(&*u.win, ex, ey);
                pump_for(40);
                check(h.submenu_path().is_empty(), "not at once")?;
                advance(150);
                pump_for(40);
                check(h.submenu_path().is_empty(), "not after 150 ms")?;
                advance(60);
                pump_until(|| h.submenu_path() == vec![1]);
                check(h.submenu_path() == vec![1], "open after 200 ms")?;

                // Straight down to Share: after the delay its submenu replaces Edit's.
                let (sx, sy) = row(r, 2);
                focus::hover_at(&*u.win, sx, sy);
                advance(100);
                pump_for(40);
                check(h.submenu_path() == vec![1], "Edit's stays during the delay")?;
                advance(150);
                pump_until(|| h.submenu_path() == vec![2]);
                check(
                    h.submenu_path() == vec![2],
                    &format!("Share's replaced it: {:?}", h.submenu_path()),
                )?;

                // Aim: from Share toward its card, across Properties: it stays open
                // past the hover delay, and the pointer reaches it.
                let sub = *h.submenu_rects().first().ok_or("share card")?;
                let (fx, fy) = (r.0 + r.2 - 30, sy);
                focus::hover_at(&*u.win, fx, fy);
                focus::hover_at(&*u.win, fx + 20, fy + rh);
                advance(250);
                pump_for(40);
                check(
                    h.submenu_path() == vec![2],
                    "aiming: Share's stays open past the hover delay",
                )?;
                focus::hover_at(&*u.win, sub.0 + sub.2 / 2, sub.1 + pad + rh / 2);
                advance(400);
                pump_for(40);
                check(h.submenu_path() == vec![2], "inside the submenu: it stays")?;
                // The grace ends where the pointer rests (Properties): it closes.
                focus::hover_at(&*u.win, fx, fy);
                focus::hover_at(&*u.win, fx + 20, fy + rh);
                advance(350);
                pump_until(|| h.submenu_path().is_empty());
                check(
                    h.submenu_path().is_empty() && h.is_open(),
                    "after the aim grace the entry under the pointer wins",
                )?;

                // A click opens at once; on its entry it stays; on a disabled parent entry it closes.
                let (cx, cy) = row(r, 1);
                focus::click_at(&*u.win, cx, cy);
                check(h.submenu_path() == vec![1], "click opens at once")?;
                focus::click_at(&*u.win, cx, cy);
                check(h.submenu_path() == vec![1], "click on its entry keeps it")?;
                let (px, py) = row(r, 3);
                focus::click_at(&*u.win, px, py);
                check(
                    h.submenu_path().is_empty() && h.is_open(),
                    "click elsewhere on the parent closes the submenu",
                )?;
                // A click on an entry of a submenu picks its path.
                focus::click_at(&*u.win, sx, sy);
                let sub = *h.submenu_rects().first().ok_or("share card")?;
                let (lx, ly) = row(sub, 1);
                focus::click_at(&*u.win, lx, ly);
                check(
                    log.last_path() == Some(vec![2, 1])
                        && h.dismissed() == Some(Dismiss::Picked(2)),
                    &format!("Link clicked: {:?}", log.last_path()),
                )?;
                check(u.layers() == 0, "all cards gone")?;

                // A click outside every card closes all of them.
                let h = log.open(&u);
                focus::click_at(&*u.win, cx, cy);
                check(h.submenu_path() == vec![1], "open")?;
                focus::click_at(&*u.win, 630, 410);
                check(
                    h.dismissed() == Some(Dismiss::ClickOutside) && u.layers() == 0,
                    "click outside: ClickOutside, every card closed",
                )?;
                let picks = log.paths.borrow().len();
                check(picks == 1, "no pick")
            })();
            clock::resume();
            r
        },
    );

    run("popup submenus: beside the entry, flip near the right edge, clamp to the window, right to left", &|shots| {
        let (pad, rh) = metrics();
        let u = ui(520, 300, shots);
        let none = |_: Vec<usize>| {};
        let h = PopupMenu::open_at_path(&*u.win, 20, 20, items(), none);
        press("Down");
        press("Right");
        let r = h.rect().ok_or("rect")?;
        let s = *h.submenu_rects().first().ok_or("card")?;
        check(s.0 >= r.0 + r.2 - 6 && s.0 <= r.0 + r.2 + 2, &format!("on the right of {r:?}: {s:?}"))?;
        check((s.1 + pad - (r.1 + pad + rh)).abs() <= 2, &format!("first entry level with Edit: {s:?} {r:?}"))?;
        let w0 = r.2;
        h.close();
        check(u.layers() == 0, "close() closes the submenus too")?;

        // Near the right edge it opens on the left.
        let h = PopupMenu::open_at_path(&*u.win, 520 - w0 - 4, 20, items(), none);
        press("Down");
        press("Right");
        let r = h.rect().ok_or("rect")?;
        let s = *h.submenu_rects().first().ok_or("card")?;
        check(s.0 + s.2 <= r.0 + 6 && s.0 >= 0, &format!("flipped to the left of {r:?}: {s:?}"))?;
        h.close();

        // Near the bottom: a tall submenu is clamped and scrolls; a short one moves up.
        let long: Vec<PopupItem> = (0..14).map(|i| PopupItem::new("", &format!("Item {i}"))).collect();
        let tall = || vec![
            PopupItem::new("", "A"),
            PopupItem::new("", "B"),
            PopupItem::submenu("", "Many", long.clone()),
            PopupItem::submenu("", "Share", vec![PopupItem::new("", "Email"), PopupItem::new("", "Link"), PopupItem::new("", "Chat")]),
        ];
        let got = Rc::new(RefCell::new(None));
        let g2 = got.clone();
        let y0 = 300 - (4 * rh + 2 * pad) - 4;
        let h = PopupMenu::open_at_path(&*u.win, 20, y0, tall(), move |p| *g2.borrow_mut() = Some(p));
        let r = h.rect().ok_or("rect")?;
        press("Down");
        press("Down");
        press("Right");
        let s = *h.submenu_rects().first().ok_or("card")?;
        check(s.1 >= 0 && s.1 + s.3 <= 300 && s.3 < 14 * rh, &format!("tall submenu clamped to 300 px: {s:?}"))?;
        press("End");
        press("Enter");
        check(*got.borrow() == Some(vec![2, 13]), &format!("the last of 14 reached by scrolling: {:?}", got.borrow()))?;
        let h = PopupMenu::open_at_path(&*u.win, 20, y0, tall(), none);
        press("Up");
        press("Right");
        let s = *h.submenu_rects().first().ok_or("card")?;
        check(s.1 + s.3 <= 298 && s.1 < r.1 + 3 * rh, &format!("short submenu moved up to fit: {s:?} (root {r:?})"))?;
        h.close();

        // Right to left: grows left of the point, submenus open on the left;
        // Left opens, Right closes.
        i18n::set_locale("ar");
        let res = (|| -> Result<(), String> {
            let picked = Rc::new(RefCell::new(None));
            let p2 = picked.clone();
            let h = PopupMenu::open_at_path(&*u.win, 400, 20, items(), move |p| *p2.borrow_mut() = Some(p));
            let r = h.rect().ok_or("rect")?;
            check((r.0 + r.2 - 400).abs() <= 2, &format!("grows to the left of the point: {r:?}"))?;
            press("Down");
            press("Left");
            check(h.submenu_path() == vec![1], "Left opens right to left")?;
            let s = *h.submenu_rects().first().ok_or("card")?;
            check(s.0 + s.2 <= r.0 + 6, &format!("on the left of {r:?}: {s:?}"))?;
            u.shot(shots, "submenu-rtl.png")?;
            press("Right");
            check(h.submenu_path().is_empty() && h.is_open(), "Right closes right to left")?;
            press("Left");
            press("Enter");
            check(*picked.borrow() == Some(vec![1, 1]), &format!("picked {:?}", picked.borrow()))?;
            // Near the left edge it flips to the right.
            let h = PopupMenu::open_at_path(&*u.win, w0 + 4, 20, items(), none);
            press("Down");
            press("Left");
            let r = h.rect().ok_or("rect")?;
            let s = *h.submenu_rects().first().ok_or("card")?;
            check(s.0 >= r.0 + r.2 - 6, &format!("flipped to the right of {r:?}: {s:?}"))?;
            h.close();
            Ok(())
        })();
        i18n::set_locale("en");
        res
    });

    run(
        "popup submenus: the card is drawn (animations off), chevrons on the trailing side",
        &|shots| {
            let u = ui(640, 420, shots);
            let (pad, rh) = metrics();
            let t = theme::current();
            let padx = t.spacing.control_padding_x;
            let cw = (14.0 * t.font.base as f64 / 12.0).round() as i32;
            let log = Log::default();
            let h = log.open(&u);
            let r = h.rect().ok_or("rect")?;
            let before = u.shot(shots, "submenu-0.png")?;
            press("Down");
            press("Right");
            let after = u.shot(shots, "submenu-1-open.png")?;
            let s = *h.submenu_rects().first().ok_or("card")?;
            // Its far border (the fill is the window background in this theme).
            let mut diff = 0;
            for x in s.0 + s.2 - 3..s.0 + s.2 {
                let (a, b) = (
                    pixel(&before, x, s.1 + s.3 / 2)?,
                    pixel(&after, x, s.1 + s.3 / 2)?,
                );
                diff = diff.max(
                    (0..3)
                        .map(|i| (a[i] as i32 - b[i] as i32).abs())
                        .sum::<i32>(),
                );
            }
            check(
                diff > 60,
                &format!(
                    "submenu card border drawn at x {}: difference {diff}",
                    s.0 + s.2 - 1
                ),
            )?;
            // Copy (the cursor) has the hover fill, Cut (disabled) not.
            let hover = pixel(&after, s.0 + s.2 - 30, s.1 + pad + rh + 2)?;
            let plain = pixel(&after, s.0 + s.2 - 30, s.1 + pad + 2)?;
            check(
                hover != plain,
                &format!("cursor on Copy: {hover:?} vs {plain:?}"),
            )?;
            // A chevron in the trailing column of Share, nothing in Delete's.
            let col = |i: i32| (r.1 + pad + i * rh + 4, r.1 + pad + (i + 1) * rh - 4);
            let fill = pixel(
                &after,
                r.0 + r.2 - padx - cw - 24,
                r.1 + pad + 2 * rh + rh / 2,
            )?;
            let (y0, y1) = col(2);
            let share = ink(
                &after,
                r.0 + r.2 - padx - cw,
                r.0 + r.2 - padx,
                y0,
                y1,
                fill,
            )?;
            let (y0, y1) = col(4);
            let delete = ink(
                &after,
                r.0 + r.2 - padx - cw,
                r.0 + r.2 - padx,
                y0,
                y1,
                fill,
            )?;
            check(
                share > 4 && delete == 0,
                &format!("chevron ink: Share {share}, Delete {delete}"),
            )?;
            h.close();

            // Right to left: the chevron is on the left.
            i18n::set_locale("ar");
            let res = (|| -> Result<(), String> {
                let h = log.open(&u);
                let r = h.rect().ok_or("rect")?;
                let png = u.shot(shots, "submenu-2-rtl.png")?;
                let fill = pixel(&png, r.0 + padx + cw + 24, r.1 + pad + 2 * rh + rh / 2)?;
                let (y0, y1) = col_rtl(r, 2);
                let share = ink(&png, r.0 + padx, r.0 + padx + cw, y0, y1, fill)?;
                let right = ink(&png, r.0 + r.2 - padx - cw, r.0 + r.2 - padx, y0, y1, fill)?;
                h.close();
                check(
                    share > 4,
                    &format!("ar: chevron on the left ({share} px; trailing-right {right})"),
                )
            })();
            i18n::set_locale("en");
            res
        },
    );
}

/// Animations on with a frozen clock, back to the test defaults afterwards.
struct Motion;
impl Motion {
    fn on() -> Motion {
        fltk_omarchy::anim::set_enabled(true);
        clock::freeze();
        Motion
    }
}
impl Drop for Motion {
    fn drop(&mut self) {
        clock::resume();
        fltk_omarchy::anim::set_enabled(false);
    }
}

pub fn motion_checks(run: &mut dyn FnMut(&str, Check)) {
    run(
        "popup submenus: with animations on a submenu fades in and out; keys keep working",
        &|shots| {
            let _m = Motion::on();
            let u = ui(640, 420, shots);
            let log = Log::default();
            let h = log.open(&u);
            advance(300);
            press("Down");
            press("Right");
            let s = *h.submenu_rects().first().ok_or("card")?;
            let (bx, by) = (s.0 + s.2 - 1, s.1 + s.3 / 2);
            let t0 = u.shot(shots, "submenu-fade-0.png")?;
            advance(300);
            let t1 = u.shot(shots, "submenu-fade-1.png")?;
            let d = |a: [u8; 3], b: [u8; 3]| {
                (0..3)
                    .map(|i| (a[i] as i32 - b[i] as i32).abs())
                    .sum::<i32>()
            };
            let (a, b) = (pixel(&t0, bx, by)?, pixel(&t1, bx, by)?);
            check(
                d(a, b) > 60,
                &format!("the border fades in: {a:?} -> {b:?}"),
            )?;
            press("Left");
            check(
                h.submenu_path().is_empty() && h.is_open(),
                "Left closes it (fading out)",
            )?;
            press("Down");
            press("Down");
            press("Enter");
            check(
                log.last_path() == Some(vec![4]),
                &format!("keys reach the root card at once: {:?}", log.last_path()),
            )?;
            advance(300);
            pump_until(|| u.layers() == 0);
            check(
                u.layers() == 0 && focus::is_focused(&*u.anchor),
                "every layer gone after the fade, focus on the anchor",
            )
        },
    );
}

fn col_rtl(r: Rect, i: i32) -> (i32, i32) {
    let (pad, rh) = metrics();
    (r.1 + pad + i * rh + 4, r.1 + pad + (i + 1) * rh - 4)
}

#[cfg(feature = "a11y")]
pub fn a11y_checks(run: &mut dyn FnMut(&str, Check)) {
    use fltk_omarchy::a11y::{self, testing};
    use testing::accesskit::{Action as A, Role as R};
    run("a11y: submenu entries have a popup and expanded state; the submenu is a menu named after its entry", &|shots| {
        let u = ui(640, 420, shots);
        let rec = testing::record(&*u.win);
        let log = Log::default();
        let h = log.open(&u);
        rec.sync();
        let snap = a11y::snapshot(&*u.win).ok_or("no tree")?;
        check(snap.contains("MenuItem \"Edit\" [has popup, collapsed, 2 of 5]"), &format!("collapsed submenu entry:\n{snap}"))?;
        press("Down");
        press("Right");
        rec.sync();
        let snap = a11y::snapshot(&*u.win).ok_or("no tree")?;
        for want in ["MenuItem \"Edit\" [has popup, expanded, 2 of 5]", "Menu \"Edit\"", "MenuItem \"Cut\" [disabled, 1 of 4]",
                     "MenuItem \"Copy\" [focused, 2 of 4]", "MenuItem \"More\" [has popup, collapsed, 4 of 4]"] {
            check(snap.contains(want), &format!("has {want:?}:\n{snap}"))?;
        }
        rec.act(rec.find(R::MenuItem, "More").ok_or("More")?, A::Expand, None);
        check(h.submenu_path() == vec![1, 3], &format!("AT expands More: {:?}", h.submenu_path()))?;
        rec.act(rec.find(R::MenuItem, "Duplicate").ok_or("Duplicate")?, A::Click, None);
        check(log.last_path() == Some(vec![1, 3, 1]) && !h.is_open(), &format!("AT picks Duplicate: {:?}", log.last_path()))
    });
}
