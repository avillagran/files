//! `RichLabel`: one paragraph of inline Markdown (`**bold**`, `*italic*`,
//! `` `code` ``, links) for headers, rows and toasts.

use super::doc::*;
use super::layout::{layout_block, FltkMetrics, Layout, Metrics, Typo};
use super::view::{draw_block, BlockCtx, Target};
use crate::state::State;
use crate::theme;
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::rc::Rc;

struct Data {
    block: Block,
    color: Option<ColorRole>,
    cache: Option<(i32, Typo, Layout)>,
    hover: Option<usize>,
}

type LinkFn = Rc<RefCell<Option<Box<dyn FnMut(&str)>>>>;

/// A rich one-paragraph label. It takes its natural one-line size (like
/// `Label`) and wraps when its container makes it narrower.
///
/// ```no_run
/// use fltk_omarchy::text::RichLabel;
/// let l = RichLabel::new("Saved **3 files** to `~/Documents` — [undo](app://undo)")
///     .on_link(|url| println!("{url}"));
/// ```
///
/// With links it is a Tab stop; Enter opens the first link.
#[derive(Clone)]
pub struct RichLabel {
    inner: Widget,
    data: Rc<RefCell<Data>>,
    link: LinkFn,
}
fltk::widget_extends!(RichLabel, Widget, inner);

impl RichLabel {
    pub fn new(md: &str) -> RichLabel {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let data = Rc::new(RefCell::new(Data {
            block: Block::new(BlockKind::Paragraph),
            color: None,
            cache: None,
            hover: None,
        }));
        let mut l = RichLabel {
            inner,
            data,
            link: Rc::default(),
        };
        l.install();
        #[cfg(feature = "a11y")]
        l.describe();
        l.set_markdown(md);
        l
    }

    fn install(&self) {
        let d = self.data.clone();
        self.inner.clone().draw(move |w| {
            let mut d = d.borrow_mut();
            theme::with(|t| {
                let mut m = FltkMetrics::default();
                ensure(&mut d, w.w(), &mut m);
                let Some((_, ty, l)) = d.cache.as_ref() else {
                    return;
                };
                let ring = crate::focus::show_ring(w);
                let first_link = d
                    .block
                    .content
                    .runs()
                    .iter()
                    .position(|r| r.style.link.is_some());
                let ctx = BlockCtx {
                    t,
                    ty,
                    under: t.palette.background,
                    sel: None,
                    hover: d.hover.map(|run| Target::Link { block: 0, run }),
                    focus: if ring {
                        first_link.map(|run| Target::Link { block: 0, run })
                    } else {
                        None
                    },
                    copied: false,
                    prev_quote: 0,
                    gap: 0,
                    clip: (w.y(), w.y() + w.h()),
                    color: d.color.map(|c| c.rgb(t)),
                };
                fltk::draw::push_clip(w.x(), w.y(), w.w(), w.h());
                let by = w.y() + (w.h() - l.height).max(0) / 2;
                draw_block(&ctx, &d.block, l, w.x(), by, &mut m);
                fltk::draw::pop_clip();
            });
        });
        let (d, link) = (self.data.clone(), self.link.clone());
        self.inner.clone().handle(move |w, ev| {
            let run_at = |w: &Widget, d: &Data| -> Option<usize> {
                let (_, _, l) = d.cache.as_ref()?;
                let by = w.y() + (w.h() - l.height).max(0) / 2;
                let (rx, ry) = ((app::event_x() - w.x()) as f32, app::event_y() - by);
                for ln in &l.lines {
                    if ry < ln.y || ry >= ln.y + ln.h {
                        continue;
                    }
                    for f in l.frags_of(ln) {
                        let fx = ln.x as f32 + f.x;
                        if rx >= fx
                            && rx < fx + f.w
                            && d.block
                                .content
                                .runs()
                                .get(f.run as usize)
                                .is_some_and(|r| r.style.link.is_some())
                        {
                            return Some(f.run as usize);
                        }
                    }
                }
                None
            };
            let fire = |run: usize, d: &Rc<RefCell<Data>>| {
                let url = d
                    .borrow()
                    .block
                    .content
                    .runs()
                    .get(run)
                    .and_then(|r| r.style.link.clone());
                if let (Some(url), Some(f)) = (url, link.borrow_mut().as_mut()) {
                    f(&url);
                }
            };
            match ev {
                Event::Enter => true,
                Event::Focus | Event::Unfocus => {
                    w.redraw();
                    true
                }
                Event::Move => {
                    let h = run_at(w, &d.borrow());
                    if h != d.borrow().hover {
                        d.borrow_mut().hover = h;
                        if let Some(mut win) = w.window() {
                            win.set_cursor(if h.is_some() {
                                Cursor::Hand
                            } else {
                                Cursor::Default
                            });
                        }
                        w.redraw();
                    }
                    true
                }
                Event::Leave => {
                    d.borrow_mut().hover = None;
                    if let Some(mut win) = w.window() {
                        win.set_cursor(Cursor::Default);
                    }
                    w.redraw();
                    true
                }
                Event::Push => run_at(w, &d.borrow()).is_some(),
                Event::Released => {
                    if let Some(run) = run_at(w, &d.borrow()) {
                        fire(run, &d);
                    }
                    true
                }
                Event::KeyDown
                    if matches!(app::event_key(), Key::Enter | Key::KPEnter)
                        || app::event_key() == Key::from_char(' ') =>
                {
                    let first = d
                        .borrow()
                        .block
                        .content
                        .runs()
                        .iter()
                        .position(|r| r.style.link.is_some());
                    match first {
                        Some(run) => {
                            fire(run, &d);
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            }
        });
    }

    /// Screen readers: a Label with its text, and a Link child per link
    /// (Click opens it, like Enter).
    #[cfg(feature = "a11y")]
    fn describe(&self) {
        use crate::a11y::{self, Action, Role};
        let (d, w) = (self.data.clone(), self.inner.clone());
        a11y::describe(&self.inner, move |n| {
            let Ok(d) = d.try_borrow() else { return };
            let text = d.block.text();
            n.role(Role::Label).name(text);
            let lay = d.cache.as_ref().map(|(_, _, l)| l);
            let by = w.y() + lay.map_or(0, |l| (w.h() - l.height).max(0) / 2);
            let mut first = true;
            for (bytes, runs, url) in link_groups(&d.block) {
                n.child(runs.start as u64, |c| {
                    c.role(Role::Link)
                        .name(&text[bytes.clone()])
                        .url(&url)
                        .action(Action::Click)
                        .focusable();
                    let rect = lay.and_then(|l| {
                        let ln = l.lines.iter().find(|ln| ln.end as usize > bytes.start)?;
                        let fr = l.frags_of(ln);
                        let a = fr.iter().find(|f| runs.contains(&(f.run as usize)))?;
                        let z = fr.iter().rev().find(|f| runs.contains(&(f.run as usize)))?;
                        Some((w.x() + ln.x + a.x as i32, by + ln.y, (z.x + z.w - a.x).ceil() as i32, ln.h))
                    });
                    if let Some((x, y, ww, h)) = rect {
                        c.bounds(x, y, ww, h);
                    }
                    // Enter opens the first link.
                    if first && crate::focus::is_focused(&w) {
                        c.focused();
                    }
                });
                first = false;
            }
        });
        let (d, link, w) = (self.data.clone(), self.link.clone(), self.inner.clone());
        a11y::on_action(&self.inner, move |r| {
            let Some(run) = r.key().map(|k| k as usize) else {
                return false;
            };
            match r.action {
                Action::Click => {
                    let url = d.borrow().block.content.runs().get(run).and_then(|r| r.style.link.clone());
                    if let (Some(url), Some(f)) = (url, link.borrow_mut().as_mut()) {
                        f(&url);
                    }
                    true
                }
                Action::Focus => w.clone().take_focus().is_ok(),
                _ => false,
            }
        });
    }

    /// Base text color (runs with their own role keep it).
    pub fn color(self, role: ColorRole) -> Self {
        self.data.borrow_mut().color = Some(role);
        self.inner.clone().redraw();
        self
    }

    pub fn on_link(self, f: impl FnMut(&str) + 'static) -> Self {
        *self.link.borrow_mut() = Some(Box::new(f));
        self
    }

    pub fn set_markdown(&mut self, md: &str) {
        let content = super::markdown::parse_inline(md);
        self.set_inline(content);
    }

    /// Sets already styled text.
    pub fn set_inline(&mut self, content: Inline) {
        let has_links = content.runs().iter().any(|r| r.style.link.is_some());
        let natural = {
            let mut d = self.data.borrow_mut();
            d.block.content = content;
            d.cache = None;
            d.hover = None;
            let mut m = FltkMetrics::default();
            theme::with(|t| {
                let ty = Typo::from_theme(t);
                let base = ty.base(&d.block);
                let w: f32 = d
                    .block
                    .content
                    .runs()
                    .iter()
                    .map(|r| m.width(base.with(&r.style), &d.block.text()[r.range.clone()]))
                    .sum();
                (w.ceil() as i32 + 2, ty.line_h(ty.body))
            })
        };
        if has_links {
            self.inner.set_visible_focus();
        } else {
            self.inner.clear_visible_focus();
        }
        if !crate::layout::refit(&self.inner, natural) {
            self.inner.set_size(natural.0, natural.1);
        }
        self.inner.redraw();
    }

    pub fn text(&self) -> String {
        self.data.borrow().block.text().to_string()
    }

    pub fn content(&self) -> Inline {
        self.data.borrow().block.content.clone()
    }

    pub fn bind(self, s: &State<String>) -> Self {
        let mut me = self.clone();
        crate::state::bind_widget(&self.inner, s, true, move |v| me.set_markdown(v));
        self
    }
}

/// Links of a block: byte range, run range and URL of each (runs of one
/// link joined).
#[cfg(feature = "a11y")]
fn link_groups(b: &Block) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>, std::sync::Arc<str>)> {
    let mut out: Vec<(std::ops::Range<usize>, std::ops::Range<usize>, std::sync::Arc<str>)> = Vec::new();
    let mut prev: Option<&std::sync::Arc<str>> = None;
    for (ri, r) in b.content.runs().iter().enumerate() {
        match (&r.style.link, out.last_mut()) {
            (Some(u), Some(last)) if prev == Some(u) => {
                last.0.end = r.range.end;
                last.1.end = ri + 1;
            }
            (Some(u), _) => out.push((r.range.clone(), ri..ri + 1, u.clone())),
            _ => {}
        }
        prev = r.style.link.as_ref();
    }
    out
}

fn ensure(d: &mut Data, width: i32, m: &mut dyn Metrics) {
    let ty = theme::with(Typo::from_theme);
    if d.cache
        .as_ref()
        .is_some_and(|(w, t, _)| *w == width && *t == ty)
    {
        return;
    }
    let l = layout_block(&d.block, width.max(1), &ty, m, None);
    d.cache = Some((width, ty, l));
}
