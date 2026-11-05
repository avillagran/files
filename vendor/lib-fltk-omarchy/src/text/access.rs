//! Screen readers (feature `a11y`): a [`RichText`] is a read-only
//! Document whose virtual children are only the blocks near the view.
//!
//! Each block is a Paragraph, Heading (with its level), ListItem ("3 of
//! 5", checked for tasks), Code (with a Copy button) or Table (Row, Cell,
//! ColumnHeader). Its text is its name and one text run per laid out
//! line, so the Document has a Text interface over everything exposed.
//! Links are Link children with their URL; Click opens them like Enter.
//!
//! Children are placed in content coordinates and the Document carries
//! the scroll offset: scrolling changes one node, and a stream that grows
//! the last block changes that block's nodes only.

use super::*;
use crate::a11y::{self, Action, Node, Request, Role, Value};
use crate::text::store::ShapeKind;

/// Keys of text runs (`RUN + line`) and of a code block's copy button.
/// Link keys are their first run index (below `RUN`).
const RUN: u64 = 1 << 32;
const COPY_KEY: u64 = 1 << 33;
/// Blocks exposed at most (the view and half a view around it).
const MAX_BLOCKS: usize = 200;
/// Text runs (lines) exposed per block at most: a huge block exposes the
/// lines near the view.
const MAX_RUNS: usize = 400;
/// Longest name of a block, in bytes (its runs still carry the text).
const NAME_MAX: usize = 4000;
/// How far a list is scanned to count its items.
const LIST_SCAN: usize = 2000;
/// Keys of List nodes: `LIST + index of the list's first block`.
const LIST: u64 = 1 << 34;

pub(super) fn install(rt: &RichText) {
    let (inn, w) = (rt.inner.clone(), rt.widget.clone());
    a11y::describe(&rt.widget, move |n| {
        if let Ok(i) = inn.try_borrow() {
            describe(&w, &i, n);
        }
    });
    let (inn, w, link) = (rt.inner.clone(), rt.widget.clone(), rt.link.clone());
    a11y::on_action(&rt.widget, move |r| act(&w, &inn, &link, r));
}

/// One line of a block: bytes of the text and its rectangle (content
/// coordinates) when laid out.
struct Ln {
    start: usize,
    end: usize,
    bounds: Option<(i32, i32, i32, i32)>,
}

fn lines(i: &Inner, k: usize, bx: i32, by: i32) -> Vec<Ln> {
    let text = i.store.text(k);
    if let Some((_, l)) = i.laid(k) {
        return l
            .lines
            .iter()
            .map(|ln| {
                let fr = l.frags_of(ln);
                let (x0, x1) = match (fr.first(), fr.last()) {
                    (Some(a), Some(b)) => (a.x, b.x + b.w),
                    _ => (0.0, 0.0),
                };
                Ln {
                    start: ln.start as usize,
                    end: ln.end as usize,
                    bounds: Some((
                        bx + ln.x + x0.floor() as i32,
                        by + ln.y,
                        (x1 - x0).ceil().max(1.0) as i32,
                        ln.h,
                    )),
                }
            })
            .collect();
    }
    let mut at = 0;
    let mut out: Vec<Ln> = text
        .split_inclusive('\n')
        .map(|s| {
            let r = Ln {
                start: at,
                end: at + s.len(),
                bounds: None,
            };
            at += s.len();
            r
        })
        .collect();
    if out.is_empty() {
        out.push(Ln {
            start: 0,
            end: 0,
            bounds: None,
        });
    }
    out
}

/// Lines of a block exposed as runs: all of them, or the ones near the
/// view for a huge block.
fn window(lines: &[Ln], view: (i64, i64)) -> Range<usize> {
    if lines.len() <= MAX_RUNS {
        return 0..lines.len();
    }
    let first = lines
        .iter()
        .position(|l| {
            l.bounds
                .is_some_and(|(_, y, _, h)| (y + h) as i64 >= view.0)
        })
        .unwrap_or(0)
        .min(lines.len() - MAX_RUNS);
    first..first + MAX_RUNS
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// Byte offset of character `c` of `s`.
fn byte_of(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

fn clip_name(t: &str) -> String {
    if t.len() <= NAME_MAX {
        return t.to_string();
    }
    let mut e = NAME_MAX;
    while !t.is_char_boundary(e) {
        e -= 1;
    }
    format!("{}…", &t[..e])
}

/// Content coordinates of the view: block `k`'s top and the text column.
struct Geo {
    bx: i32,
    wy: i32,
    width: i32,
    /// Visible content range (y from the content top, window units).
    view: (i64, i64),
}

impl Geo {
    fn block_y(&self, i: &Inner, k: usize) -> i32 {
        (self.wy as i64 + i.pad as i64 + i.heights.prefix(k)) as i32 + i.gap(k)
    }
}

/// Blocks to expose: the view and half a view around it, then the blocks
/// that hold the focus target and the selection ends.
fn exposed(i: &Inner, h: i32) -> Vec<usize> {
    let n = i.store.len();
    if n == 0 {
        return Vec::new();
    }
    let vh = if i.view_h > 0 { i.view_h } else { h } as i64;
    let y0 = i.top - i.pad as i64;
    let first = i.heights.find((y0 - vh / 2).max(0));
    let last = i.heights.find((y0 + vh + vh / 2).max(0));
    let mut v: Vec<usize> = (first..=last.min(n - 1)).take(MAX_BLOCKS).collect();
    let extra = [
        i.focus.map(|t| t.block()),
        i.sel_range().map(|(a, _)| a.block),
        i.sel_range().map(|(_, b)| b.block),
    ];
    for k in extra.into_iter().flatten() {
        if k < n && !v.contains(&k) {
            v.push(k);
        }
    }
    v.sort_unstable();
    v
}

/// Position of list item `k` among its siblings, when the list is short
/// enough to count.
fn list_position(i: &Inner, k: usize) -> Option<(usize, usize)> {
    let me = i.store.shape(k);
    let ordered = matches!(i.store.marker(k), Some(ListMarker::Ordered(_)));
    // A sibling: an item at the same depth of the same kind of list.
    // Anything less indented ends the list; deeper blocks belong to items.
    let step = |j: usize| -> Option<bool> {
        let s = i.store.shape(j);
        if s.indent < me.indent || s.quote != me.quote {
            return None;
        }
        if s.indent > me.indent || !s.marker {
            return Some(false);
        }
        let o = matches!(i.store.marker(j), Some(ListMarker::Ordered(_)));
        (o == ordered).then_some(true)
    };
    let (mut before, mut after) = (0usize, 0usize);
    let mut j = k;
    let mut scanned = 0;
    while j > 0 {
        j -= 1;
        scanned += 1;
        if scanned > LIST_SCAN {
            return None;
        }
        match step(j) {
            Some(true) => before += 1,
            Some(false) => {}
            None => break,
        }
    }
    j = k + 1;
    scanned = 0;
    while j < i.store.len() {
        scanned += 1;
        if scanned > LIST_SCAN {
            return None;
        }
        match step(j) {
            Some(true) => after += 1,
            Some(false) => {}
            None => break,
        }
        j += 1;
    }
    Some((before + 1, before + 1 + after))
}

fn describe(w: &Widget, i: &Inner, n: &mut Node) {
    n.role(Role::Document).read_only(true).focusable();
    let vh = if i.view_h > 0 { i.view_h } else { w.h() };
    n.scroll(i.top as f64, i.max_top() as f64);
    let geo = Geo {
        bx: w.x() + i.pad,
        wy: w.y(),
        width: if i.width > 0 {
            i.width
        } else {
            (w.w() - 2 * i.pad).max(40)
        },
        view: (
            w.y() as i64 + i.top - vh as i64 / 2,
            w.y() as i64 + i.top + vh as i64 + vh as i64 / 2,
        ),
    };
    let focus = if crate::focus::is_focused(w) {
        i.focus
    } else {
        None
    };
    let blocks: Vec<usize> = exposed(i, w.h())
        .into_iter()
        .filter(|&k| i.store.shape(k).kind != ShapeKind::Rule)
        .collect();
    let mut cx = Emit {
        i,
        geo: &geo,
        focus,
        sel: i.sel_range().map(|_| i.sel.unwrap_or_default()),
        paths: [None, None],
    };
    cx.seq(&blocks, 0, &[], n);
    if let [Some(a), Some(f)] = &cx.paths {
        n.text_selection_in((&a.0, a.1), (&f.0, f.1));
    }
}

/// Builds the children: blocks at `depth`, and a List node for each run
/// of blocks nested deeper (list items and what belongs to them).
struct Emit<'a> {
    i: &'a Inner,
    geo: &'a Geo,
    focus: Option<Target>,
    sel: Option<(TextPos, TextPos)>,
    /// Key paths of the selection ends (from the document).
    paths: [Option<(Vec<u64>, usize)>; 2],
}

impl Emit<'_> {
    fn seq(&mut self, blocks: &[usize], depth: u8, prefix: &[u64], n: &mut Node) {
        let i = self.i;
        let mut j = 0;
        // Position of the last list item at this depth (and its kind): the
        // next sibling is one further, without scanning the list again.
        let mut pos: Option<(bool, usize, usize, usize, usize)> = None;
        while j < blocks.len() {
            let k = blocks[j];
            if i.store.shape(k).indent > depth {
                // A list: the following deeper blocks, until a list of the
                // other kind (ordered / bullets) starts at this depth.
                let item = |b: usize| -> Option<bool> {
                    let s = i.store.shape(b);
                    (s.indent == depth + 1 && s.marker)
                        .then(|| matches!(i.store.marker(b), Some(ListMarker::Ordered(_))))
                };
                let mut kind = None;
                let mut run = Vec::new();
                for &b in &blocks[j..] {
                    if i.store.shape(b).indent <= depth {
                        break;
                    }
                    if let Some(o) = item(b) {
                        if kind.is_some_and(|x| x != o) {
                            break;
                        }
                        kind = Some(o);
                    }
                    run.push(b);
                }
                j += run.len();
                // Keyed by where the list starts, so scrolling keeps its node.
                let mut start = k;
                let mut scanned = 0;
                while start > 0 && scanned < LIST_SCAN {
                    let p = start - 1;
                    if i.store.shape(p).indent <= depth
                        || item(p).is_some_and(|o| kind.is_some_and(|x| x != o))
                    {
                        break;
                    }
                    start = p;
                    scanned += 1;
                }
                let key = LIST + start as u64;
                let path = [prefix, &[key]].concat();
                n.child(key, |l| {
                    l.role(Role::List);
                    self.seq(&run, depth + 1, &path, l);
                });
                continue;
            }
            j += 1;
            let shape = i.store.shape(k);
            let item = shape.marker && shape.kind == ShapeKind::Paragraph;
            let ordered = matches!(i.store.marker(k), Some(ListMarker::Ordered(_)));
            let here = if !item {
                None
            } else {
                // The next sibling with no gap in between: count on;
                // otherwise scan.
                let next = match pos {
                    Some((o, p, of, jj, kk)) if o == ordered && p < of && k - kk == j - 1 - jj => {
                        Some((p + 1, of))
                    }
                    _ => list_position(i, k),
                };
                pos = next.map(|(p, of)| (ordered, p, of, j - 1, k));
                next
            };
            let path = [prefix, &[k as u64]].concat();
            n.child(k as u64, |b| {
                block(i, k, self.geo, self.focus, here, b);
                if let Some((a, f)) = self.sel {
                    for (slot, p) in self.paths.iter_mut().zip([a, f]) {
                        if p.block == k {
                            *slot = place(i, k, self.geo, p.offset)
                                .map(|(rest, c)| ([&path[..], &rest].concat(), c));
                        }
                    }
                }
            });
        }
    }
}

/// Key path (below block `k`'s node) and character in the run that holds
/// byte `off` of the block.
fn place(i: &Inner, k: usize, geo: &Geo, off: usize) -> Option<(Vec<u64>, usize)> {
    let text = i.store.text(k);
    let by = geo.block_y(i, k);
    let ls = lines(i, k, geo.bx, by);
    let off = floor_char(text, off);
    let li = ls
        .iter()
        .position(|l| off >= l.start && off < l.end)
        .unwrap_or(ls.len().saturating_sub(1));
    let ln = ls.get(li)?;
    let c = chars(&text[ln.start..off.max(ln.start)]);
    if let Some(t) = i.store.table(k) {
        let laid = i.laid(k).is_some();
        for (r, row) in t.rows.iter().enumerate() {
            for (ci, cell) in row.iter().enumerate() {
                if off < cell.start || off > cell.end {
                    continue;
                }
                let path = vec![r as u64, ci as u64];
                if !laid {
                    let c = chars(&text[cell.start..off]);
                    return Some(([path, vec![RUN]].concat(), c));
                }
                let (li, ln) = cell_lines(&ls, cell)
                    .take_while(|(_, l)| l.start <= off)
                    .last()?;
                let c = chars(&text[ln.start..off]);
                return Some(([path, vec![RUN + li as u64]].concat(), c));
            }
        }
        return None;
    }
    let key = RUN + li as u64;
    window(&ls, geo.view).contains(&li).then(|| (vec![key], c))
}

fn block(
    i: &Inner,
    k: usize,
    geo: &Geo,
    focus: Option<Target>,
    pos: Option<(usize, usize)>,
    b: &mut Node,
) {
    let shape = i.store.shape(k);
    let text = i.store.text(k);
    let by = geo.block_y(i, k);
    b.bounds(geo.bx, by, geo.width, i.body_h(k));
    match shape.kind {
        ShapeKind::Heading(l) => {
            b.role(Role::Heading).level(l as usize);
        }
        ShapeKind::Code => {
            b.role(Role::Code).description(i.store.lang(k));
        }
        ShapeKind::Table(_) => return table(i, k, geo, by, focus, b),
        _ if shape.marker => {
            b.role(Role::ListItem).level(shape.indent as usize);
            if let Some(ListMarker::Task(done)) = i.store.marker(k) {
                b.toggled(done);
            }
            if let Some((p, of)) = pos {
                b.position(p, of);
            }
        }
        _ => {
            b.role(Role::Paragraph);
        }
    }
    b.name(&clip_name(text));
    let ls = lines(i, k, geo.bx, by);
    for li in window(&ls, geo.view) {
        let l = &ls[li];
        let t = &text[l.start..l.end];
        // The last line ends the block: the document's text gets a break.
        if li + 1 == ls.len() && !t.ends_with('\n') {
            b.text_run(RUN + li as u64, &format!("{t}\n"), l.bounds);
        } else {
            b.text_run(RUN + li as u64, t, l.bounds);
        }
    }
    links(i, k, 0..text.len(), (geo.bx, by), focus, b);
    if shape.is_code() {
        let laid = i.laid(k).map(|(_, l)| copy_rect(l, &i.typo, geo.bx, by));
        b.child(COPY_KEY, |c| {
            c.role(Role::Button)
                .name(&crate::i18n::tr("Copy"))
                .action(Action::Click)
                .focusable();
            if let Some((x, y, w, h)) = laid {
                c.bounds(x, y, w, h);
            }
            if focus == Some(Target::Copy { block: k }) {
                c.focused();
            }
        });
    }
}

/// Link children for the links inside `range` of block `k` (block at
/// `bx, by`).
fn links(
    i: &Inner,
    k: usize,
    range: Range<usize>,
    (bx, by): (i32, i32),
    focus: Option<Target>,
    n: &mut Node,
) {
    let text = i.store.text(k);
    let laid = i.laid(k);
    for (bytes, runs, url) in i.store.links(k) {
        if bytes.start < range.start || bytes.start >= range.end.max(range.start + 1) {
            continue;
        }
        // The rectangle of the link on its first line.
        let rect = laid.and_then(|(_, l)| {
            let ln = l.lines.iter().find(|ln| (ln.end as usize) > bytes.start)?;
            let fr = l.frags_of(ln);
            let a = fr.iter().find(|f| runs.contains(&(f.run as usize)))?;
            let z = fr.iter().rev().find(|f| runs.contains(&(f.run as usize)))?;
            Some((
                bx + ln.x + a.x.floor() as i32,
                by + ln.y,
                (z.x + z.w - a.x).ceil() as i32,
                ln.h,
            ))
        });
        n.child(runs.start as u64, |c| {
            c.role(Role::Link)
                .name(&text[bytes.clone()])
                .url(url)
                .action(Action::Click)
                .focusable();
            if let Some((x, y, w, h)) = rect {
                c.bounds(x, y, w, h);
            }
            if matches!(focus, Some(Target::Link { block, run }) if block == k && runs.contains(&run)) {
                c.focused();
            }
        });
    }
}

fn table(i: &Inner, k: usize, geo: &Geo, by: i32, focus: Option<Target>, b: &mut Node) {
    let Some(t) = i.store.table(k) else { return };
    let text = i.store.text(k);
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    b.role(Role::Table).counts(t.rows.len(), cols);
    let ls = lines(i, k, geo.bx, by);
    let geom = i.laid(k).and_then(|(_, l)| l.table.clone());
    if let Some(g) = &geom {
        b.bounds(
            geo.bx + g.cols.first().map_or(0, |c| c.0),
            by,
            geo.width,
            i.body_h(k),
        );
    }
    for (r, row) in t.rows.iter().enumerate() {
        let header = r < t.header_rows;
        b.child(r as u64, |rn| {
            rn.role(Role::Row).row_index(r);
            if let Some((y, h)) = geom.as_ref().and_then(|g| g.rows.get(r)) {
                rn.bounds(geo.bx, by + y, geo.width, *h);
            }
            for (c, cell) in row.iter().enumerate() {
                rn.child(c as u64, |cn| {
                    cn.role(if header {
                        Role::ColumnHeader
                    } else {
                        Role::Cell
                    })
                    .name(&text[cell.clone()])
                    .row_index(r)
                    .column_index(c);
                    if let (Some(g), Some((y, h))) =
                        (&geom, geom.as_ref().and_then(|g| g.rows.get(r)))
                    {
                        if let Some((x, w)) = g.cols.get(c) {
                            cn.bounds(geo.bx + x, by + y, *w, *h);
                        }
                    }
                    // Cells end with a tab, rows with a break (document text).
                    let sep = if c + 1 == row.len() { "\n" } else { "\t" };
                    match &geom {
                        Some(_) => {
                            let cl: Vec<_> = cell_lines(&ls, cell).collect();
                            for (n, (li, l)) in cl.iter().enumerate() {
                                let e = l.end.min(cell.end).max(l.start);
                                let t = text[l.start..e].trim_end_matches(['\t', '\n']);
                                let t = if n + 1 == cl.len() {
                                    format!("{t}{sep}")
                                } else {
                                    t.to_string()
                                };
                                cn.text_run(RUN + *li as u64, &t, l.bounds);
                            }
                        }
                        None => {
                            cn.text_run(RUN, &format!("{}{sep}", &text[cell.clone()]), None);
                        }
                    }
                    links(i, k, cell.clone(), (geo.bx, by), focus, cn);
                });
            }
        });
    }
}

/// Laid out lines of a table cell (with their index in the block).
fn cell_lines<'a>(
    ls: &'a [Ln],
    cell: &'a Range<usize>,
) -> impl Iterator<Item = (usize, &'a Ln)> + 'a {
    ls.iter()
        .enumerate()
        .filter(move |(_, l)| l.start >= cell.start && l.start < cell.end.max(cell.start + 1))
}

// ---------------------------------------------------------------- actions

fn act(w: &Widget, inn: &Rc<RefCell<Inner>>, link: &LinkFn, r: &Request) -> bool {
    let mut w = w.clone();
    // Below the List nodes: [block, ..].
    let path: Vec<u64> = r.path.iter().copied().skip_while(|k| *k >= LIST).collect();
    if path.is_empty() && !r.path.is_empty() {
        return false;
    }
    let Some(&k) = path.first() else {
        return match (r.action, &r.value) {
            (Action::SetTextSelection, Some(Value::TextRange { anchor, focus })) => {
                select(&w, inn, anchor, focus)
            }
            _ => false,
        };
    };
    let k = k as usize;
    if k >= inn.borrow().store.len() {
        return false;
    }
    // The target a link or copy key names (tables: [block, row, cell, link]).
    let target = match path.last().copied() {
        Some(COPY_KEY) => Some(Target::Copy { block: k }),
        Some(run) if path.len() != 1 && path.len() != 3 && run < RUN => Some(Target::Link {
            block: k,
            run: run as usize,
        }),
        _ => None,
    };
    match (r.action, target) {
        (Action::Click, Some(t)) => {
            inn.borrow_mut().focus = Some(t);
            activate_focus(&mut w, inn, link)
        }
        (Action::Focus, Some(t)) => {
            let _ = w.take_focus();
            let mut i = inn.borrow_mut();
            i.focus = Some(t);
            reveal_block(&mut i, k, w.h());
            reveal(&mut i, t, &w);
            drop(i);
            w.redraw();
            true
        }
        (Action::Focus | Action::ScrollIntoView, _) => {
            if r.action == Action::Focus {
                let _ = w.take_focus();
            }
            reveal_block(&mut inn.borrow_mut(), k, w.h());
            w.redraw();
            true
        }
        (Action::SetTextSelection, _) => match &r.value {
            Some(Value::TextRange { anchor, focus }) => select(&w, inn, anchor, focus),
            _ => false,
        },
        _ => false,
    }
}

/// Scrolls so block `k` is in view (its top, when it is taller).
fn reveal_block(i: &mut Inner, k: usize, view: i32) {
    let top_y = i.heights.prefix(k) + i.gap(k) as i64;
    let h = i.body_h(k) as i64;
    let view = view as i64 - 2 * i.pad as i64;
    let at = i.top - i.pad as i64;
    if top_y < at || h > view {
        i.top = top_y;
    } else if top_y + h > at + view {
        i.top = top_y + h - view;
    }
    let max = i.max_top();
    i.top = i.top.clamp(0, max);
    let top = i.top;
    i.motion.jump(top);
    i.stuck = i.stick && top >= max;
}

/// A selection from the AT: key paths of the run holders and characters.
fn select(
    w: &Widget,
    inn: &Rc<RefCell<Inner>>,
    a: &(Vec<u64>, usize),
    f: &(Vec<u64>, usize),
) -> bool {
    let pos = |(path, c): &(Vec<u64>, usize)| -> Option<TextPos> {
        let path: Vec<u64> = path.iter().copied().skip_while(|k| *k >= LIST).collect();
        let i = inn.borrow();
        let k = *path.first()? as usize;
        if k >= i.store.len() {
            return None;
        }
        let text = i.store.text(k);
        let base = match path.as_slice() {
            [_] => {
                let geo = Geo {
                    bx: w.x() + i.pad,
                    wy: w.y(),
                    width: i.width,
                    view: (
                        w.y() as i64 + i.top - i.view_h as i64 / 2,
                        w.y() as i64 + i.top + i.view_h as i64 * 3 / 2,
                    ),
                };
                let ls = lines(&i, k, geo.bx, geo.block_y(&i, k));
                ls.get(window(&ls, geo.view).start).map_or(0, |l| l.start)
            }
            [_, r, cell] => {
                i.store
                    .table(k)?
                    .rows
                    .get(*r as usize)?
                    .get(*cell as usize)?
                    .start
            }
            _ => return None,
        };
        Some(TextPos {
            block: k,
            offset: base + byte_of(&text[base..], *c),
        })
    };
    let (Some(a), Some(f)) = (pos(a), pos(f)) else {
        return false;
    };
    inn.borrow_mut().sel = Some((a, f));
    w.clone().redraw();
    true
}
