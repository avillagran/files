//! Row motion for virtual lists and trees, by stable item ID (o3's thread
//! rows): rows that make room slide (442 ms OutCubic, `RowDisplaced`), a
//! row moved elsewhere flies there lifted (520 ms InOutCubic,
//! `T3RowFlight`), new rows arrive (360 ms: fade in and settle) and removed
//! rows leave (300 ms InCubic: fade out and drift), as `T3ThreadRow`.
//!
//! Only rows in view are tracked (a few dozen): a list of a million rows
//! costs the same. The owner calls [`RowMotion::changed`] when its data
//! changes, then each draw [`RowMotion::begin`] with the rows in view,
//! draws each row at [`RowMotion::at`] and ends with [`RowMotion::end`].

use crate::anim::{self, ms, Ease, Tween};
use crate::panel::ItemId;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Displaced,
    Moved,
    Arrive,
}

#[derive(Debug, Clone, Copy)]
struct RowAnim {
    dy0: f64,
    tween: Tween,
    kind: Kind,
}

impl RowAnim {
    /// (offset, opacity, lift 0..1).
    fn sample(&self, drift: f64) -> (i32, f64, f64) {
        let t = self.tween.t();
        match self.kind {
            Kind::Displaced => (
                (self.dy0 * (1.0 - Ease::OutCubic.apply(t))).round() as i32,
                1.0,
                0.0,
            ),
            Kind::Moved => (
                (self.dy0 * (1.0 - Ease::InOutCubic.apply(t))).round() as i32,
                1.0,
                (std::f64::consts::PI * t).sin(),
            ),
            Kind::Arrive => {
                // o3: a fifth of the time waiting, then fade and settle.
                let k = ((t - 0.2) / 0.8).clamp(0.0, 1.0);
                let o = Ease::OutCubic.apply(k);
                let settle = Ease::OutBack(1.2).apply(k);
                (((1.0 - settle) * drift).round() as i32, o, 0.0)
            }
        }
    }
}

/// A row that left while in view, drawn fading out where it was.
pub(crate) struct Ghost<R> {
    pub row: R,
    pub y: i32,
    tween: Tween,
}

impl<R> Ghost<R> {
    /// (offset, opacity).
    pub fn sample(&self, drift: f64) -> (i32, f64) {
        let k = Ease::InCubic.apply(self.tween.t());
        ((k * drift).round() as i32, 1.0 - k)
    }
}

pub(crate) struct RowMotion<R> {
    gen: u64,
    seen: u64,
    primed: bool,
    // Rows drawn last time: id, y on screen (relative to the view), payload.
    last: Vec<(ItemId, i32, R)>,
    anims: HashMap<ItemId, RowAnim>,
    pub ghosts: Vec<Ghost<R>>,
    inserted: HashSet<ItemId>,
    moved: HashSet<ItemId>,
    /// Arrivals for every row that is new in view (tree branches opening).
    all_new_arrive: bool,
    displaced_ms: u32,
}

impl<R: Clone> Default for RowMotion<R> {
    fn default() -> Self {
        RowMotion {
            gen: 0,
            seen: 0,
            primed: false,
            last: Vec::new(),
            anims: HashMap::new(),
            ghosts: Vec::new(),
            inserted: HashSet::new(),
            moved: HashSet::new(),
            all_new_arrive: false,
            displaced_ms: ms::ROW_DISPLACED,
        }
    }
}

impl<R: Clone> RowMotion<R> {
    /// The data changed: `inserted` rows arrive, `moved` rows fly.
    pub fn changed(&mut self, inserted: &[ItemId], moved: &[ItemId]) {
        self.gen += 1;
        self.inserted.extend(inserted.iter().copied());
        self.moved.extend(moved.iter().copied());
    }

    /// Rows were shown or hidden in place (a tree branch): new rows arrive,
    /// the others slide over `ms`.
    pub fn reflowed(&mut self, ms: u32) {
        self.gen += 1;
        self.all_new_arrive = true;
        self.displaced_ms = ms;
    }

    /// Before drawing: `rows` are the ids in view with their y (relative to
    /// the view). `alive` tells removed items from rows scrolled away.
    pub fn begin(&mut self, rows: &[(ItemId, i32)], alive: impl Fn(ItemId) -> bool) {
        // Finished motion goes away.
        self.anims.retain(|_, a| !a.tween.done());
        self.ghosts.retain(|g| !g.tween.done());
        if self.gen == self.seen {
            return;
        }
        self.seen = self.gen;
        let (inserted, moved) = (
            std::mem::take(&mut self.inserted),
            std::mem::take(&mut self.moved),
        );
        let all_new = std::mem::replace(&mut self.all_new_arrive, false);
        let displaced_ms = std::mem::replace(&mut self.displaced_ms, ms::ROW_DISPLACED);
        if !self.primed || !anim::enabled() {
            self.anims.clear();
            self.ghosts.clear();
            return;
        }
        let before: HashMap<ItemId, i32> = self.last.iter().map(|(id, y, _)| (*id, *y)).collect();
        let now: HashSet<ItemId> = rows.iter().map(|r| r.0).collect();
        let mut fresh = Vec::new();
        for &(id, y) in rows {
            match before.get(&id) {
                Some(&old) if old != y => {
                    let kind = if moved.contains(&id) {
                        Kind::Moved
                    } else {
                        Kind::Displaced
                    };
                    let ms = if kind == Kind::Moved {
                        ms::ROW_MOVE
                    } else {
                        displaced_ms
                    };
                    self.anims.insert(
                        id,
                        RowAnim {
                            dy0: (old - y) as f64,
                            tween: Tween::start(ms, Ease::Linear),
                            kind,
                        },
                    );
                }
                Some(_) => {}
                None if inserted.contains(&id) || all_new || moved.contains(&id) => {
                    self.anims.insert(
                        id,
                        RowAnim {
                            dy0: 0.0,
                            tween: Tween::start(ms::ROW_ARRIVE, Ease::Linear),
                            kind: Kind::Arrive,
                        },
                    );
                }
                None => fresh.push(id),
            }
        }
        // Rows that came into view because others moved: they move with
        // their neighbour (the nearest row above that moves, else below).
        for id in fresh {
            let pos = rows.iter().position(|r| r.0 == id).unwrap_or(0);
            let near = rows[..pos]
                .iter()
                .rev()
                .chain(rows[pos + 1..].iter())
                .find_map(|(n, _)| {
                    self.anims
                        .get(n)
                        .filter(|a| a.kind == Kind::Displaced)
                        .map(|a| a.dy0)
                });
            if let Some(dy0) = near {
                self.anims.insert(
                    id,
                    RowAnim {
                        dy0,
                        tween: Tween::start(displaced_ms, Ease::Linear),
                        kind: Kind::Displaced,
                    },
                );
            }
        }
        for (id, y, row) in &self.last {
            if !now.contains(id) && !alive(*id) {
                self.ghosts.push(Ghost {
                    row: row.clone(),
                    y: *y,
                    tween: Tween::start(ms::ROW_DEPART, Ease::Linear),
                });
            }
        }
    }

    /// Offset, opacity and lift (0..1, moved rows mid-flight) of a row.
    pub fn at(&self, id: ItemId, drift: f64) -> (i32, f64, f64) {
        self.anims.get(&id).map_or((0, 1.0, 0.0), |a| {
            if a.tween.done() {
                (0, 1.0, 0.0)
            } else {
                a.sample(drift)
            }
        })
    }

    /// After drawing: the rows in view as drawn (y with its offset).
    pub fn end(&mut self, drawn: Vec<(ItemId, i32, R)>) {
        self.primed = true;
        self.last = drawn;
    }

    /// True while something moves (the owner schedules a frame).
    pub fn running(&self) -> bool {
        self.anims.values().any(|a| !a.tween.done()) || self.ghosts.iter().any(|g| !g.tween.done())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn displaced_moved_arriving_and_leaving_rows() {
        anim::set_enabled(true);
        anim::clock::freeze();
        let mut m: RowMotion<&'static str> = RowMotion::default();
        m.begin(&[(1, 0), (2, 30), (3, 60)], |_| true);
        m.end(vec![(1, 0, "a"), (2, 30, "b"), (3, 60, "c")]);
        // 9 inserted at the top, 2 removed, 3 moved up.
        m.changed(&[9], &[3]);
        m.begin(&[(9, 0), (1, 30), (3, 60)], |id| id != 2);
        assert_eq!(m.at(9, 22.0).1, 0.0, "arrival starts transparent");
        assert_eq!(m.at(1, 22.0).0, -30, "displaced row starts where it was");
        assert_eq!(m.at(3, 22.0).0, 0, "same place: no motion");
        assert_eq!(m.ghosts.len(), 1, "removed row leaves a ghost");
        assert!(m.running());
        anim::clock::advance(Duration::from_millis(600));
        assert_eq!(m.at(1, 22.0), (0, 1.0, 0.0));
        assert_eq!(m.at(9, 22.0), (0, 1.0, 0.0));
        assert!(!m.running());
        anim::clock::resume();
        anim::set_enabled(None);
    }

    #[test]
    fn off_or_first_draw_jumps() {
        anim::set_enabled(false);
        let mut m: RowMotion<()> = RowMotion::default();
        m.begin(&[(1, 0)], |_| true);
        m.end(vec![(1, 0, ())]);
        m.changed(&[], &[]);
        m.begin(&[(1, 30)], |_| true);
        assert_eq!(m.at(1, 0.0), (0, 1.0, 0.0));
        anim::set_enabled(None);
    }
}
