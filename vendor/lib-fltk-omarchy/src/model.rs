//! Keyed collection models: change one item, a range or a snapshot and the
//! views redraw only the rows that changed.
//!
//! A [`ListModel`] keeps items in display order, each with a stable key
//! from the app (a thread id, a path) and a stable [`ItemId`] given by the
//! model. Operations are granular (insert, remove, move, upsert, patch) and
//! listeners receive [`Changes`]: the inserted / removed / moved ranges and
//! the updated IDs, never "everything changed". Views built with
//! [`crate::list::List::from_model`], [`crate::table::Table::from_model`] and
//! [`crate::table::Tree::from_model`] invalidate only the visible rows whose
//! content changed, keep the scroll position when items are inserted above
//! the viewport, and keep the cursor and selection by ID.
//!
//! ```no_run
//! use fltk_omarchy::{list::{List, ListRow}, model::ListModel};
//! #[derive(Clone, PartialEq)]
//! struct Thread { id: String, title: String, status: String }
//! let threads = ListModel::new(|t: &Thread| t.id.clone());
//! threads.push(Thread { id: "a".into(), title: "Port to fltk".into(), status: "idle".into() });
//! let list = List::from_model(&threads, |t| ListRow::new(&t.title).detail(&t.status));
//! threads.patch(&"a".to_string(), |t| t.status = "running".into()); // one row redraws
//! ```
//!
//! **Protocols that send whole snapshots**: [`ListModel::sync_from`] diffs
//! the snapshot against the model by key and applies the minimal
//! operations (removals, moves along a longest increasing subsequence,
//! inserts, and replacements only for items that are not `==`). An
//! unchanged snapshot costs one comparison per item and notifies nobody.
//!
//! **Background threads** never touch the model: they push [`Diff`]s into
//! a [`Feed`] (`Send + Sync + Clone`); the UI thread applies everything
//! pushed since the last loop iteration in one transaction (one
//! notification, one redraw), optionally capped with [`Feed::max_rate`].
//!
//! **Many operations, one notification**: [`ListModel::transaction`] (or
//! [`crate::state::batch`], which also covers States).

use crate::panel::{ItemId, KeyboardPanel};
use crate::state::{self, FeedSlot, SlotCell, State, Subscription};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// More structural operations than this in one notification become a
/// reset (still keyed: IDs, cursor and selection survive).
const OPS_LIMIT: usize = 256;
/// `sync_from` moves at most this many items one by one; more reorders
/// the model in one pass and reports a reset.
const MOVE_LIMIT: usize = 64;

// ================================================================ Changes

/// One structural operation, with indices in the model as it was right
/// before this operation (replay [`Changes::ops`] in order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// `len` items inserted at `at`.
    Inserted { at: usize, len: usize },
    /// `len` items removed from `at`.
    Removed { at: usize, len: usize },
    /// One item removed at `from`, then inserted at `to` (in the list
    /// without it).
    Moved { from: usize, to: usize },
}

/// What changed in one notification of a [`ListModel`].
#[derive(Debug, Clone, Default)]
pub struct Changes {
    ops: Vec<Change>,
    reset: bool,
    inserted: Vec<ItemId>,
    removed: Vec<ItemId>,
    updated: Vec<ItemId>,
    new_or_updated: HashSet<ItemId>,
}

impl Changes {
    /// Structural operations in order (empty after a reset).
    pub fn ops(&self) -> &[Change] {
        &self.ops
    }

    /// The order changed in a way not listed in `ops` (a sort, a large
    /// reorder, many operations at once). IDs are still stable: re-read the
    /// order and look items up by ID.
    pub fn is_reset(&self) -> bool {
        self.reset
    }

    /// Anything other than updates in place (rows moved, appeared or went).
    pub fn is_structural(&self) -> bool {
        self.reset || !self.ops.is_empty()
    }

    /// IDs of items that are new in this notification.
    pub fn inserted(&self) -> &[ItemId] {
        &self.inserted
    }

    /// IDs of items that are gone (drop per-item caches). An item inserted
    /// and removed within one batch appears in both lists.
    pub fn removed(&self) -> &[ItemId] {
        &self.removed
    }

    /// IDs of existing items whose value changed (each once; new items are
    /// not repeated here).
    pub fn updated(&self) -> &[ItemId] {
        &self.updated
    }

    pub fn is_empty(&self) -> bool {
        !self.reset
            && self.ops.is_empty()
            && self.updated.is_empty()
            && self.removed.is_empty()
            && self.inserted.is_empty()
    }

    /// Where the item that was at `old` (before these changes) is now.
    /// None when it was removed, or after a reset (use IDs then).
    pub fn map_index(&self, old: usize) -> Option<usize> {
        if self.reset {
            return None;
        }
        let mut i = old;
        for op in &self.ops {
            match *op {
                Change::Inserted { at, len } => {
                    if i >= at {
                        i += len;
                    }
                }
                Change::Removed { at, len } => {
                    if i >= at + len {
                        i -= len;
                    } else if i >= at {
                        return None;
                    }
                }
                Change::Moved { from, to } => {
                    if i == from {
                        i = to;
                    } else {
                        if from < i {
                            i -= 1;
                        }
                        if to <= i {
                            i += 1;
                        }
                    }
                }
            }
        }
        Some(i)
    }

    fn op(&mut self, op: Change) {
        if self.reset {
            return;
        }
        // Runs of pushes and of removals at one place stay one range.
        match (self.ops.last_mut(), op) {
            (Some(Change::Inserted { at, len }), Change::Inserted { at: a2, len: l2 })
                if a2 == *at + *len =>
            {
                *len += l2
            }
            (Some(Change::Removed { at, len }), Change::Removed { at: a2, len: l2 })
                if a2 == *at =>
            {
                *len += l2
            }
            (Some(Change::Removed { at, len }), Change::Removed { at: a2, len: l2 })
                if a2 + l2 == *at =>
            {
                *at = a2;
                *len += l2;
            }
            _ => self.ops.push(op),
        }
        if self.ops.len() > OPS_LIMIT {
            self.set_reset();
        }
    }

    fn set_reset(&mut self) {
        self.reset = true;
        self.ops.clear();
    }

    fn insert_id(&mut self, id: ItemId) {
        self.inserted.push(id);
        self.new_or_updated.insert(id);
    }

    fn update_id(&mut self, id: ItemId) {
        if self.new_or_updated.insert(id) {
            self.updated.push(id);
        }
    }
}

// ================================================================== Diff

/// A patch closure sent from a thread.
pub type PatchFn<T> = Box<dyn FnOnce(&mut T) + Send>;

/// One operation on a [`ListModel`], for [`ListModel::apply`] and [`Feed`].
pub enum Diff<K, T> {
    /// Replaces the item with the same key in place, or appends it.
    Upsert(T),
    /// Inserts at an index; an existing item with that key is replaced and
    /// moved there.
    UpsertAt(usize, T),
    /// Removes the item with this key (nothing if absent).
    Remove(K),
    /// Changes the item with this key in place (nothing if absent).
    Patch(K, PatchFn<T>),
    /// Moves the item with this key to an index (clamped).
    Move(K, usize),
    /// A full snapshot: [`ListModel::sync_from`].
    Sync(Vec<T>),
    /// Removes everything.
    Clear,
}

impl<K, T> Diff<K, T> {
    pub fn patch(key: K, f: impl FnOnce(&mut T) + Send + 'static) -> Diff<K, T> {
        Diff::Patch(key, Box::new(f))
    }
}

impl<K: std::fmt::Debug, T: std::fmt::Debug> std::fmt::Debug for Diff<K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::Upsert(t) => f.debug_tuple("Upsert").field(t).finish(),
            Diff::UpsertAt(i, t) => f.debug_tuple("UpsertAt").field(i).field(t).finish(),
            Diff::Remove(k) => f.debug_tuple("Remove").field(k).finish(),
            Diff::Patch(k, _) => f.debug_tuple("Patch").field(k).finish(),
            Diff::Move(k, i) => f.debug_tuple("Move").field(k).field(i).finish(),
            Diff::Sync(v) => f.debug_tuple("Sync").field(&v.len()).finish(),
            Diff::Clear => f.write_str("Clear"),
        }
    }
}

// ================================================================= model

struct Entry<K, T> {
    id: ItemId,
    key: K,
    item: T,
}

struct Data<K, T> {
    items: Vec<Entry<K, T>>,
    ids: HashMap<K, ItemId>,
    /// Always valid: index of every ID.
    pos: HashMap<ItemId, usize>,
    next_id: ItemId,
}

impl<K: Clone + Eq + Hash, T> Data<K, T> {
    fn reindex(&mut self, range: Range<usize>) {
        for i in range {
            if let Some(e) = self.items.get(i) {
                self.pos.insert(e.id, i);
            }
        }
    }

    fn reindex_all(&mut self) {
        self.pos.clear();
        self.pos.reserve(self.items.len());
        for (i, e) in self.items.iter().enumerate() {
            self.pos.insert(e.id, i);
        }
    }

    fn index_of_key(&self, key: &K) -> Option<usize> {
        self.ids.get(key).and_then(|id| self.pos.get(id)).copied()
    }

    fn new_entry(&mut self, key: K, item: T) -> Entry<K, T> {
        self.next_id += 1;
        let id = self.next_id;
        self.ids.insert(key.clone(), id);
        Entry { id, key, item }
    }

    fn remove_at(&mut self, i: usize) -> Entry<K, T> {
        let e = self.items.remove(i);
        self.ids.remove(&e.key);
        self.pos.remove(&e.id);
        let n = self.items.len();
        self.reindex(i..n);
        e
    }

    fn move_entry(&mut self, from: usize, to: usize) {
        let e = self.items.remove(from);
        self.items.insert(to, e);
        self.reindex(from.min(to)..from.max(to) + 1);
    }
}

type ModelListener<K, T> = Rc<RefCell<dyn FnMut(&ListModel<K, T>, &Changes) -> bool>>;

struct MInner<K: 'static, T: 'static> {
    data: RefCell<Data<K, T>>,
    key: Box<dyn Fn(&T) -> K>,
    listeners: RefCell<Vec<(u64, ModelListener<K, T>)>>,
    next_sub: Cell<u64>,
    pending: RefCell<Changes>,
    queued: Cell<bool>,
    notifying: Cell<bool>,
    notifications: Cell<u64>,
}

/// An ordered, keyed collection with granular change events (UI thread,
/// `Rc`-based; cloning shares it). `K` is the app's stable key, `T` the
/// item. Duplicate keys are not kept: inserting an existing key replaces
/// (and moves) that item.
pub struct ListModel<K: 'static, T: 'static> {
    inner: Rc<MInner<K, T>>,
}

impl<K, T> Clone for ListModel<K, T> {
    fn clone(&self) -> Self {
        ListModel {
            inner: self.inner.clone(),
        }
    }
}

impl<K, T> PartialEq for ListModel<K, T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<K, T> std::fmt::Debug for ListModel<K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ListModel({} items)",
            self.inner.data.borrow().items.len()
        )
    }
}

impl<K: Clone + Eq + Hash + 'static, T: Clone + PartialEq + 'static> ListModel<K, T> {
    /// An empty model; `key` gives each item's stable key.
    pub fn new(key: impl Fn(&T) -> K + 'static) -> ListModel<K, T> {
        ListModel {
            inner: Rc::new(MInner {
                data: RefCell::new(Data {
                    items: Vec::new(),
                    ids: HashMap::new(),
                    pos: HashMap::new(),
                    next_id: 0,
                }),
                key: Box::new(key),
                listeners: RefCell::new(Vec::new()),
                next_sub: Cell::new(0),
                pending: RefCell::new(Changes::default()),
                queued: Cell::new(false),
                notifying: Cell::new(false),
                notifications: Cell::new(0),
            }),
        }
    }

    /// A model holding `items` (duplicate keys: the first one is kept).
    pub fn with_items(
        key: impl Fn(&T) -> K + 'static,
        items: impl IntoIterator<Item = T>,
    ) -> ListModel<K, T> {
        let m = ListModel::new(key);
        {
            let mut d = m.inner.data.borrow_mut();
            for t in items {
                let k = (m.inner.key)(&t);
                if !d.ids.contains_key(&k) {
                    let e = d.new_entry(k, t);
                    d.items.push(e);
                }
            }
            d.reindex_all();
        }
        m
    }

    // ------------------------------------------------------------ reading

    pub fn len(&self) -> usize {
        self.inner.data.borrow().items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A copy of the item at an index.
    pub fn get(&self, index: usize) -> Option<T> {
        self.with_at(index, T::clone)
    }

    /// Reads the item at an index without cloning it.
    pub fn with_at<R>(&self, index: usize, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.inner
            .data
            .borrow()
            .items
            .get(index)
            .map(|e| f(&e.item))
    }

    /// Reads the item with a key without cloning it.
    pub fn with_key<R>(&self, key: &K, f: impl FnOnce(&T) -> R) -> Option<R> {
        let d = self.inner.data.borrow();
        let i = d.index_of_key(key)?;
        Some(f(&d.items[i].item))
    }

    /// Reads the item with a model ID without cloning it.
    pub fn with_id<R>(&self, id: ItemId, f: impl FnOnce(&T) -> R) -> Option<R> {
        let d = self.inner.data.borrow();
        let i = *d.pos.get(&id)?;
        Some(f(&d.items[i].item))
    }

    /// A copy of the item with a key.
    pub fn get_key(&self, key: &K) -> Option<T> {
        self.with_key(key, T::clone)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.data.borrow().ids.contains_key(key)
    }

    pub fn key_at(&self, index: usize) -> Option<K> {
        self.inner
            .data
            .borrow()
            .items
            .get(index)
            .map(|e| e.key.clone())
    }

    /// The stable ID at an index (what panels and views keep).
    pub fn id_at(&self, index: usize) -> Option<ItemId> {
        self.inner.data.borrow().items.get(index).map(|e| e.id)
    }

    /// Current index of an ID.
    pub fn index_of(&self, id: ItemId) -> Option<usize> {
        self.inner.data.borrow().pos.get(&id).copied()
    }

    pub fn index_of_key(&self, key: &K) -> Option<usize> {
        self.inner.data.borrow().index_of_key(key)
    }

    pub fn id_of(&self, key: &K) -> Option<ItemId> {
        self.inner.data.borrow().ids.get(key).copied()
    }

    pub fn key_of(&self, id: ItemId) -> Option<K> {
        let d = self.inner.data.borrow();
        d.pos.get(&id).map(|&i| d.items[i].key.clone())
    }

    /// Visits every item in order without cloning.
    pub fn for_each(&self, mut f: impl FnMut(usize, &T)) {
        for (i, e) in self.inner.data.borrow().items.iter().enumerate() {
            f(i, &e.item);
        }
    }

    /// A copy of every item, in order.
    pub fn to_vec(&self) -> Vec<T> {
        self.inner
            .data
            .borrow()
            .items
            .iter()
            .map(|e| e.item.clone())
            .collect()
    }

    /// Notifications sent so far (tests, benchmarks).
    pub fn notifications(&self) -> u64 {
        self.inner.notifications.get()
    }

    // ----------------------------------------------------------- writing

    /// Appends (or replaces in place when the key exists).
    pub fn push(&self, item: T) {
        self.upsert(item);
    }

    /// Appends many items; existing keys are replaced in place.
    pub fn extend(&self, items: impl IntoIterator<Item = T>) {
        self.transaction(|m| {
            for t in items {
                m.upsert(t);
            }
        });
    }

    /// Replaces the item with the same key in place (notifies only if it
    /// differs), or appends it. Returns its index.
    pub fn upsert(&self, item: T) -> usize {
        let key = (self.inner.key)(&item);
        let mut d = self.inner.data.borrow_mut();
        if let Some(i) = d.index_of_key(&key) {
            let changed = d.items[i].item != item;
            let id = d.items[i].id;
            if changed {
                d.items[i].item = item;
            }
            drop(d);
            if changed {
                self.record(|c| c.update_id(id));
            }
            return i;
        }
        let e = d.new_entry(key, item);
        let (id, at) = (e.id, d.items.len());
        d.items.push(e);
        d.pos.insert(id, at);
        drop(d);
        self.record(|c| {
            c.op(Change::Inserted { at, len: 1 });
            c.insert_id(id);
        });
        at
    }

    /// Inserts at `index` (clamped). An existing item with the same key is
    /// replaced and moved to `index` instead.
    pub fn insert(&self, index: usize, item: T) {
        self.upsert_at(index, item);
    }

    /// [`ListModel::insert`]; returns the final index.
    pub fn upsert_at(&self, index: usize, item: T) -> usize {
        let key = (self.inner.key)(&item);
        let existing = self.index_of_key(&key);
        if let Some(i) = existing {
            self.transaction(|m| {
                let at = m.upsert(item);
                debug_assert_eq!(at, i);
                m.move_to(i, index)
            })
        } else {
            let mut d = self.inner.data.borrow_mut();
            let at = index.min(d.items.len());
            let e = d.new_entry(key, item);
            let id = e.id;
            d.items.insert(at, e);
            let n = d.items.len();
            d.reindex(at..n);
            drop(d);
            self.record(|c| {
                c.op(Change::Inserted { at, len: 1 });
                c.insert_id(id);
            });
            at
        }
    }

    /// Removes the item at an index.
    pub fn remove(&self, index: usize) -> Option<T> {
        let mut d = self.inner.data.borrow_mut();
        if index >= d.items.len() {
            return None;
        }
        let e = d.remove_at(index);
        drop(d);
        self.record(|c| {
            c.op(Change::Removed { at: index, len: 1 });
            c.removed.push(e.id);
        });
        Some(e.item)
    }

    /// Removes the item with a key.
    pub fn remove_key(&self, key: &K) -> Option<T> {
        let i = self.index_of_key(key)?;
        self.remove(i)
    }

    /// Removes a range of indices (clamped); one `Removed` change.
    pub fn remove_range(&self, range: Range<usize>) -> Vec<T> {
        let mut d = self.inner.data.borrow_mut();
        let end = range.end.min(d.items.len());
        let start = range.start.min(end);
        if start == end {
            return Vec::new();
        }
        let gone: Vec<Entry<K, T>> = d.items.drain(start..end).collect();
        for e in &gone {
            d.ids.remove(&e.key);
            d.pos.remove(&e.id);
        }
        let n = d.items.len();
        d.reindex(start..n);
        drop(d);
        let ids: Vec<ItemId> = gone.iter().map(|e| e.id).collect();
        self.record(|c| {
            c.op(Change::Removed {
                at: start,
                len: end - start,
            });
            c.removed.extend(ids);
        });
        gone.into_iter().map(|e| e.item).collect()
    }

    /// Keeps only the items for which `f` is true (one notification).
    pub fn retain(&self, mut f: impl FnMut(&T) -> bool) {
        self.transaction(|m| {
            let mut i = m.len();
            while i > 0 {
                i -= 1;
                let keep = m.with_at(i, &mut f).unwrap_or(true);
                if !keep {
                    m.remove(i);
                }
            }
        });
    }

    /// Removes everything.
    pub fn clear(&self) {
        let n = self.len();
        self.remove_range(0..n);
    }

    /// Moves the item at `from` to `to` (clamped). Returns the final index.
    pub fn move_to(&self, from: usize, to: usize) -> usize {
        let mut d = self.inner.data.borrow_mut();
        let n = d.items.len();
        if from >= n {
            return from;
        }
        let to = to.min(n - 1);
        if from == to {
            return to;
        }
        d.move_entry(from, to);
        drop(d);
        self.record(|c| c.op(Change::Moved { from, to }));
        to
    }

    /// Moves the item with a key to an index (e.g. 0: bump to the top).
    pub fn move_key(&self, key: &K, to: usize) -> Option<usize> {
        let i = self.index_of_key(key)?;
        Some(self.move_to(i, to))
    }

    /// Changes the item with a key in place; notifies only if it changed
    /// (the item is cloned once to compare). The key may change too.
    pub fn patch(&self, key: &K, f: impl FnOnce(&mut T)) -> bool {
        self.patch_with(key, |t| {
            let before = t.clone();
            f(t);
            *t != before
        })
    }

    /// Like [`ListModel::patch`] without the clone: `f` returns whether it
    /// changed anything.
    pub fn patch_with(&self, key: &K, f: impl FnOnce(&mut T) -> bool) -> bool {
        let mut d = self.inner.data.borrow_mut();
        let Some(i) = d.index_of_key(key) else {
            return false;
        };
        let changed = f(&mut d.items[i].item);
        if !changed {
            return false;
        }
        let id = d.items[i].id;
        let new_key = (self.inner.key)(&d.items[i].item);
        let mut dropped = None;
        if new_key != d.items[i].key {
            // Re-keyed: the item keeps its ID; another item with the new key goes.
            if let Some(j) = d.index_of_key(&new_key) {
                let e = d.remove_at(j);
                dropped = Some((j, e.id));
            }
            let old = std::mem::replace(
                &mut d
                    .items
                    .iter_mut()
                    .find(|e| e.id == id)
                    .expect("patched item")
                    .key,
                new_key.clone(),
            );
            d.ids.remove(&old);
            d.ids.insert(new_key, id);
        }
        drop(d);
        self.record(|c| {
            if let Some((j, gone)) = dropped {
                c.op(Change::Removed { at: j, len: 1 });
                c.removed.push(gone);
            }
            c.update_id(id);
        });
        true
    }

    /// Sorts (stable). Notifies a reset, only if the order changed; IDs,
    /// cursor and selection survive.
    pub fn sort_by(&self, mut cmp: impl FnMut(&T, &T) -> std::cmp::Ordering) {
        let mut d = self.inner.data.borrow_mut();
        let before: Vec<ItemId> = d.items.iter().map(|e| e.id).collect();
        d.items.sort_by(|a, b| cmp(&a.item, &b.item));
        if d.items.iter().map(|e| e.id).eq(before.iter().copied()) {
            return;
        }
        d.reindex_all();
        drop(d);
        self.record(|c| c.set_reset());
    }

    /// Makes the model equal to a full snapshot (in its order), with the
    /// minimal changes found by key: removals, moves (the longest
    /// increasing subsequence stays put), inserts, and replacements only
    /// for items that are not `==`. Only the window between the common key
    /// prefix and suffix is hashed, so a status change or a bump near the
    /// top of a long list stays cheap. Duplicate keys in the snapshot: one
    /// is kept. Unchanged snapshot: no notification.
    pub fn sync_from(&self, snapshot: impl IntoIterator<Item = T>) {
        let new: Vec<(K, T)> = snapshot
            .into_iter()
            .map(|t| ((self.inner.key)(&t), t))
            .collect();
        let mut changes = Changes::default();
        {
            let mut d = self.inner.data.borrow_mut();
            sync_data(&mut d, new, &mut changes);
        }
        if !changes.is_empty() {
            self.record(|c| merge(c, changes));
        }
    }

    /// Applies diffs in order, as one transaction.
    pub fn apply(&self, diffs: impl IntoIterator<Item = Diff<K, T>>) {
        self.transaction(|m| {
            for d in diffs {
                m.apply_one(d);
            }
        });
    }

    fn apply_one(&self, diff: Diff<K, T>) {
        match diff {
            Diff::Upsert(t) => {
                self.upsert(t);
            }
            Diff::UpsertAt(i, t) => {
                self.upsert_at(i, t);
            }
            Diff::Remove(k) => {
                self.remove_key(&k);
            }
            Diff::Patch(k, f) => {
                self.patch(&k, f);
            }
            Diff::Move(k, i) => {
                self.move_key(&k, i);
            }
            Diff::Sync(v) => self.sync_from(v),
            Diff::Clear => self.clear(),
        }
    }

    /// Runs `f` as one transaction: listeners get one notification with
    /// every change (joins an enclosing [`state::batch`]).
    pub fn transaction<R>(&self, f: impl FnOnce(&Self) -> R) -> R {
        state::batch(|| f(self))
    }

    // ------------------------------------------------------- notification

    fn record(&self, f: impl FnOnce(&mut Changes)) {
        f(&mut self.inner.pending.borrow_mut());
        if self.inner.notifying.get() {
            return; // the running flush loop delivers it next
        }
        if state::in_batch() {
            if !self.inner.queued.replace(true) {
                let m = self.clone();
                state::defer(Box::new(move || {
                    m.inner.queued.set(false);
                    m.flush();
                }));
            }
            return;
        }
        self.flush();
    }

    fn flush(&self) {
        loop {
            let changes = std::mem::take(&mut *self.inner.pending.borrow_mut());
            if changes.is_empty() {
                return;
            }
            self.inner
                .notifications
                .set(self.inner.notifications.get() + 1);
            let listeners: Vec<(u64, ModelListener<K, T>)> = self
                .inner
                .listeners
                .borrow()
                .iter()
                .map(|(i, l)| (*i, l.clone()))
                .collect();
            self.inner.notifying.set(true);
            let mut dead = Vec::new();
            for (id, l) in listeners {
                if let Ok(mut f) = l.try_borrow_mut() {
                    if !f(self, &changes) {
                        dead.push(id);
                    }
                }
            }
            self.inner.notifying.set(false);
            if !dead.is_empty() {
                self.inner
                    .listeners
                    .borrow_mut()
                    .retain(|(i, _)| !dead.contains(i));
            }
        }
    }

    /// Calls `f(model, changes)` after every change (once per transaction
    /// or batch). `f` may read the model; changes it makes are delivered
    /// in a following notification.
    pub fn subscribe(
        &self,
        mut f: impl FnMut(&ListModel<K, T>, &Changes) + 'static,
    ) -> Subscription {
        let id = self.listen(move |m, c| {
            f(m, c);
            true
        });
        let weak: Weak<MInner<K, T>> = Rc::downgrade(&self.inner);
        Subscription::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.listeners.borrow_mut().retain(|(i, _)| *i != id);
            }
        })
    }

    /// Internal listener that unregisters itself by returning false.
    pub(crate) fn listen(
        &self,
        f: impl FnMut(&ListModel<K, T>, &Changes) -> bool + 'static,
    ) -> u64 {
        let id = self.inner.next_sub.get() + 1;
        self.inner.next_sub.set(id);
        self.inner
            .listeners
            .borrow_mut()
            .push((id, Rc::new(RefCell::new(f))));
        id
    }

    pub fn listener_count(&self) -> usize {
        self.inner.listeners.borrow().len()
    }

    /// A `State` that follows the number of items (labels, badges).
    pub fn count_state(&self) -> State<usize> {
        let s = State::new(self.len());
        let w = s.clone();
        self.subscribe(move |m, c| {
            if c.is_structural() {
                w.set(m.len());
            }
        })
        .forever();
        s
    }

    /// Makes `panel` follow this model: its items are the model's IDs in
    /// order; cursor, anchor and selection survive reorders and drop
    /// removed items. The host redraws only when they lose an item.
    pub fn bind_panel(&self, panel: &KeyboardPanel) {
        let set = |m: &ListModel<K, T>, p: &KeyboardPanel, quiet: bool| {
            let (a, b) = (m.clone(), m.clone());
            p.set_items_with(
                m.len(),
                move |i| a.id_at(i).unwrap_or(0),
                move |id| b.index_of(id),
                quiet,
            );
        };
        set(self, panel, false);
        let weak = panel.downgrade();
        self.listen(move |m, c| {
            let Some(p) = weak.upgrade() else {
                return false;
            };
            if c.is_structural() || !c.removed().is_empty() {
                set(m, &p, true);
            }
            true
        });
    }
}

fn merge(into: &mut Changes, from: Changes) {
    if from.reset {
        into.set_reset();
    }
    for op in from.ops {
        into.op(op);
    }
    for id in from.inserted {
        into.insert_id(id);
    }
    into.removed.extend(from.removed);
    for id in from.updated {
        into.update_id(id);
    }
}

/// The diff of `sync_from`, on the data, recording into `ch`. Items in the
/// common key prefix and suffix are only compared; the window between
/// them is reconciled by key.
fn sync_data<K: Clone + Eq + Hash, T: PartialEq>(
    d: &mut Data<K, T>,
    mut new: Vec<(K, T)>,
    ch: &mut Changes,
) {
    let (old_len, new_len) = (d.items.len(), new.len());
    let mut p = 0;
    while p < old_len && p < new_len && d.items[p].key == new[p].0 {
        p += 1;
    }
    let mut s = 0;
    while s < old_len - p
        && s < new_len - p
        && d.items[old_len - 1 - s].key == new[new_len - 1 - s].0
    {
        s += 1;
    }
    let tail = new.split_off(new_len - s);
    let mid = new.split_off(p);
    for (e, (_, t)) in d.items[..p].iter_mut().zip(new) {
        if e.item != t {
            e.item = t;
            ch.update_id(e.id);
        }
    }
    for (e, (_, t)) in d.items[old_len - s..].iter_mut().zip(tail) {
        if e.item != t {
            e.item = t;
            ch.update_id(e.id);
        }
    }
    if p + s == old_len && mid.is_empty() {
        return; // same keys in the same order
    }
    let window: Vec<Entry<K, T>> = d.items.drain(p..old_len - s).collect();
    let out = sync_window(d, window, mid, p, ch);
    let len = out.len();
    d.items.splice(p..p, out);
    if d.items.len() == old_len {
        d.reindex(p..p + len);
    } else {
        let n = d.items.len();
        d.reindex(p..n);
    }
}

/// Reconciles the items of a window (starting at index `base`) with the
/// snapshot's window; returns the window's new entries in order.
fn sync_window<K: Clone + Eq + Hash, T: PartialEq>(
    d: &mut Data<K, T>,
    mut items: Vec<Entry<K, T>>,
    new: Vec<(K, T)>,
    base: usize,
    ch: &mut Changes,
) -> Vec<Entry<K, T>> {
    let mut target: HashMap<K, usize> = HashMap::with_capacity(new.len());
    for (i, (k, _)) in new.iter().enumerate() {
        target.entry(k.clone()).or_insert(i);
    }
    // 1. Removals, from the end so earlier indices stay valid.
    let mut i = items.len();
    while i > 0 {
        i -= 1;
        if !target.contains_key(&items[i].key) {
            let mut start = i;
            while start > 0 && !target.contains_key(&items[start - 1].key) {
                start -= 1;
            }
            let gone: Vec<Entry<K, T>> = items.drain(start..=i).collect();
            for e in &gone {
                d.ids.remove(&e.key);
                d.pos.remove(&e.id);
                ch.removed.push(e.id);
            }
            ch.op(Change::Removed {
                at: base + start,
                len: gone.len(),
            });
            i = start;
        }
    }
    // 2. Moves: items on the longest increasing subsequence of target
    //    positions stay; every other one moves right after its predecessor.
    let order: Vec<usize> = items.iter().map(|e| target[&e.key]).collect();
    let stay = lis_mask(&order);
    let movers = stay.iter().filter(|s| !**s).count();
    if movers > MOVE_LIMIT {
        items.sort_by_key(|e| target[&e.key]);
        ch.set_reset();
    } else if movers > 0 {
        let mut sorted = order.clone();
        sorted.sort_unstable();
        let mut moving: Vec<usize> = order
            .iter()
            .zip(&stay)
            .filter(|(_, s)| !**s)
            .map(|(t, _)| *t)
            .collect();
        moving.sort_unstable();
        for t in moving {
            let rank = sorted.binary_search(&t).expect("target present");
            let from = items
                .iter()
                .position(|e| target[&e.key] == t)
                .expect("mover present");
            let to = if rank == 0 {
                0
            } else {
                let pred = sorted[rank - 1];
                let p = items
                    .iter()
                    .position(|e| target[&e.key] == pred)
                    .expect("predecessor present");
                if from < p {
                    p
                } else {
                    p + 1
                }
            };
            if from != to {
                let e = items.remove(from);
                items.insert(to, e);
                ch.op(Change::Moved {
                    from: base + from,
                    to: base + to,
                });
            }
        }
    }
    // 3. Inserts and replacements, in one pass building the final order.
    //    A key already in the model outside this window, or seen earlier in
    //    it, is a duplicate in the snapshot: skipped.
    let mut old = items.into_iter().peekable();
    let mut out: Vec<Entry<K, T>> = Vec::with_capacity(new.len());
    let mut run: Option<(usize, usize)> = None;
    for (k, t) in new {
        if old.peek().is_some_and(|e| e.key == k) {
            if let Some((at, len)) = run.take() {
                ch.op(Change::Inserted { at, len });
            }
            let mut e = old.next().expect("peeked");
            if e.item != t {
                e.item = t;
                ch.update_id(e.id);
            }
            out.push(e);
        } else if !d.ids.contains_key(&k) {
            let at = base + out.len();
            let e = d.new_entry(k, t);
            ch.insert_id(e.id);
            out.push(e);
            run = Some(run.map_or((at, 1), |(a, len)| (a, len + 1)));
        }
    }
    if let Some((at, len)) = run {
        ch.op(Change::Inserted { at, len });
    }
    debug_assert!(old.next().is_none());
    out
}

/// Marks the elements on one longest strictly increasing subsequence.
fn lis_mask(v: &[usize]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new(); // index into v of the tail of each length
    let mut prev: Vec<Option<usize>> = vec![None; v.len()];
    for (i, &x) in v.iter().enumerate() {
        let k = tails.partition_point(|&j| v[j] < x);
        if k > 0 {
            prev[i] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut mask = vec![false; v.len()];
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        mask[i] = true;
        cur = prev[i];
    }
    mask
}

// ================================================================== Feed

/// Pushes [`Diff`]s into a [`ListModel`] from any thread (`Send + Sync +
/// Clone`). Everything pushed between two event-loop iterations is applied
/// on the UI thread in one transaction: one notification, one redraw. A
/// snapshot (`sync`) or `clear` drops the diffs queued before it. Create it
/// on the UI thread with [`ListModel::feed`].
///
/// ```no_run
/// # use fltk_omarchy::model::{ListModel, Diff};
/// # #[derive(Clone, PartialEq)] struct Thread { id: u64, status: &'static str }
/// let threads = ListModel::new(|t: &Thread| t.id);
/// let feed = threads.feed().max_rate(60.0);
/// std::thread::spawn(move || {
///     feed.upsert(Thread { id: 7, status: "running" });
///     feed.patch(7, |t| t.status = "done");
///     feed.move_to(7, 0);
/// });
/// ```
pub struct Feed<K: Send + 'static, T: Send + 'static> {
    slot: Arc<FeedSlot<Vec<Diff<K, T>>>>,
}

impl<K: Send + 'static, T: Send + 'static> Clone for Feed<K, T> {
    fn clone(&self) -> Self {
        Feed {
            slot: self.slot.clone(),
        }
    }
}

impl<K: Send + 'static, T: Send + 'static> Feed<K, T> {
    /// Thread-safe.
    pub fn push(&self, diff: Diff<K, T>) {
        self.slot.push(|q| {
            if matches!(diff, Diff::Sync(_) | Diff::Clear) {
                q.clear();
            }
            q.push(diff);
        });
    }

    /// Thread-safe: several diffs at once.
    pub fn push_all(&self, diffs: impl IntoIterator<Item = Diff<K, T>>) {
        self.slot.push(|q| {
            for diff in diffs {
                if matches!(diff, Diff::Sync(_) | Diff::Clear) {
                    q.clear();
                }
                q.push(diff);
            }
        });
    }

    pub fn upsert(&self, item: T) {
        self.push(Diff::Upsert(item));
    }

    pub fn upsert_at(&self, index: usize, item: T) {
        self.push(Diff::UpsertAt(index, item));
    }

    pub fn remove(&self, key: K) {
        self.push(Diff::Remove(key));
    }

    pub fn patch(&self, key: K, f: impl FnOnce(&mut T) + Send + 'static) {
        self.push(Diff::patch(key, f));
    }

    pub fn move_to(&self, key: K, index: usize) {
        self.push(Diff::Move(key, index));
    }

    /// A full snapshot (diffed on the UI thread with `sync_from`).
    pub fn sync(&self, items: Vec<T>) {
        self.push(Diff::Sync(items));
    }

    pub fn clear(&self) {
        self.push(Diff::Clear);
    }

    /// Applies at most `hz` times per second (e.g. the display rate):
    /// pushes in between wait for one fltk timeout. 0 = every loop
    /// iteration (default). Thread-safe.
    pub fn max_rate(self, hz: f64) -> Self {
        self.set_max_rate(hz);
        self
    }

    pub fn set_max_rate(&self, hz: f64) {
        self.slot.set_max_rate(hz);
    }

    /// Diffs pushed and not applied yet.
    pub fn queued(&self) -> usize {
        self.slot.pending.lock().unwrap().len()
    }

    /// False once the model is gone (pushes are then dropped).
    pub fn is_connected(&self) -> bool {
        self.slot.alive.load(Ordering::Relaxed)
    }
}

impl<K, T> ListModel<K, T>
where
    K: Clone + Eq + Hash + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
{
    /// A thread-safe [`Feed`] into this model. Call on the UI thread.
    pub fn feed(&self) -> Feed<K, T> {
        let weak = Rc::downgrade(&self.inner);
        let cell: SlotCell<Vec<Diff<K, T>>> = Rc::default();
        let c2 = cell.clone();
        let sink = crate::task::register_sink(move || {
            let Some(slot) = c2.borrow().clone() else {
                return true;
            };
            let Some(inner) = weak.upgrade() else {
                slot.alive.store(false, Ordering::Relaxed);
                return false;
            };
            let diffs = slot.take();
            if !diffs.is_empty() {
                ListModel { inner }.apply(diffs);
            }
            true
        });
        let slot = FeedSlot::new(sink);
        *cell.borrow_mut() = Some(slot.clone());
        Feed { slot }
    }
}

// ============================================================ view helper

/// How a view's rows were invalidated by model changes (tests, benchmarks).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Invalidations {
    /// Notifications that redrew the whole view.
    pub full: usize,
    /// Single rows invalidated (partial redraws).
    pub rows: usize,
    /// Notifications that needed no row redraw at all.
    pub none: usize,
}

/// IDs drawn in each visible row slot at the last draw, so a change can
/// tell which slots now show something else.
#[derive(Debug, Default)]
pub(crate) struct RowSlots {
    top: i64,
    ids: Vec<Option<ItemId>>,
    valid: bool,
}

impl RowSlots {
    /// Start of a draw at offset `top` with `visible` slots.
    pub(crate) fn begin(&mut self, top: i64, visible: usize) {
        if !self.valid || top != self.top {
            self.ids.clear();
            self.top = top;
            self.valid = true;
        }
        self.ids.resize(visible, None);
    }

    pub(crate) fn drawn(&mut self, slot: usize, id: Option<ItemId>) {
        if let Some(s) = self.ids.get_mut(slot) {
            *s = id;
        }
    }

    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
    }

    /// The ID drawn in the first slot, if the slots still describe `top`.
    pub(crate) fn anchor(&self, top: i64) -> Option<ItemId> {
        (self.valid && self.top == top)
            .then(|| self.ids.first().copied().flatten())
            .flatten()
    }
}

/// What a view must redraw after `changes`.
pub(crate) struct Refit {
    /// New scroll offset (anchored on the first visible item).
    pub top: i64,
    pub full: bool,
    /// Slots (0 = first visible row) to redraw.
    pub slots: Vec<usize>,
    /// The scrollbar may have changed.
    pub bar: bool,
}

/// Anchors the scroll offset on the first visible item and lists the row
/// slots whose item or content changed. Inserting or removing above the
/// viewport (with `top > 0`) keeps what is on screen; at the very top new
/// items show up.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refit(
    slots: &mut RowSlots,
    top: i64,
    row_h: i32,
    view_h: i32,
    len: usize,
    changes: &Changes,
    id_at: &dyn Fn(usize) -> Option<ItemId>,
    index_of: &dyn Fn(ItemId) -> Option<usize>,
) -> Refit {
    let rh = row_h.max(1) as i64;
    let structural = changes.is_structural();
    let fresh = slots.valid && slots.top == top;
    let mut new_top = top;
    if structural && top > 0 {
        let first = (top / rh) as usize;
        let off = top - first as i64 * rh;
        let anchor = if fresh {
            slots.ids.first().copied().flatten()
        } else {
            None
        };
        let moved = anchor
            .and_then(index_of)
            .or_else(|| changes.map_index(first));
        if let Some(nf) = moved {
            new_top = nf as i64 * rh + off;
        }
    }
    let max_top = (len as i64 * rh - view_h as i64).max(0);
    new_top = new_top.clamp(0, max_top);
    let visible = (view_h.max(0) / rh as i32 + 2) as usize;
    let aligned = fresh && new_top.rem_euclid(rh) == top.rem_euclid(rh);
    if !aligned {
        slots.invalidate();
        return Refit {
            top: new_top,
            full: true,
            slots: Vec::new(),
            bar: true,
        };
    }
    let first = (new_top / rh) as usize;
    let mut dirty: Vec<usize> = Vec::new();
    if structural {
        for k in 0..visible {
            let idx = first + k;
            let now = if idx < len { id_at(idx) } else { None };
            if slots.ids.get(k).copied().flatten() != now {
                dirty.push(k);
            }
        }
    }
    for &id in changes.updated() {
        if let Some(idx) = index_of(id) {
            if idx >= first && idx < first + visible {
                dirty.push(idx - first);
            }
        }
    }
    dirty.sort_unstable();
    dirty.dedup();
    slots.top = new_top;
    if dirty.len() > visible / 2 + 1 {
        slots.invalidate();
        return Refit {
            top: new_top,
            full: true,
            slots: Vec::new(),
            bar: true,
        };
    }
    Refit {
        top: new_top,
        full: false,
        slots: dirty,
        bar: structural,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct It {
        k: u32,
        v: u32,
    }
    fn it(k: u32, v: u32) -> It {
        It { k, v }
    }
    fn model(keys: &[u32]) -> ListModel<u32, It> {
        ListModel::with_items(|t: &It| t.k, keys.iter().map(|&k| it(k, 0)))
    }
    fn keys(m: &ListModel<u32, It>) -> Vec<u32> {
        (0..m.len()).map(|i| m.key_at(i).unwrap()).collect()
    }
    fn log(m: &ListModel<u32, It>) -> Rc<RefCell<Vec<Changes>>> {
        let seen: Rc<RefCell<Vec<Changes>>> = Rc::default();
        let s2 = seen.clone();
        m.subscribe(move |_, c| s2.borrow_mut().push(c.clone()))
            .forever();
        seen
    }

    /// Replays the structural ops on the old key list (and checks they
    /// produce the new one).
    fn replay(old: &[u32], new: &ListModel<u32, It>, c: &Changes) -> Vec<u32> {
        let mut v: Vec<Option<u32>> = old.iter().map(|&k| Some(k)).collect();
        for op in c.ops() {
            match *op {
                Change::Inserted { at, len } => {
                    for j in 0..len {
                        v.insert(at + j, None);
                    }
                }
                Change::Removed { at, len } => {
                    v.drain(at..at + len);
                }
                Change::Moved { from, to } => {
                    let x = v.remove(from);
                    v.insert(to, x);
                }
            }
        }
        // Inserted slots are filled from the model; kept ones must match it.
        v.iter()
            .enumerate()
            .map(|(i, k)| k.unwrap_or_else(|| new.key_at(i).unwrap()))
            .collect()
    }

    #[test]
    fn granular_ops_and_ids() {
        let m = model(&[1, 2, 3]);
        let seen = log(&m);
        let id2 = m.id_of(&2).unwrap();
        m.push(it(4, 0));
        m.insert(0, it(0, 0));
        m.move_key(&4, 1);
        assert_eq!(keys(&m), vec![0, 4, 1, 2, 3]);
        assert!(m.patch(&2, |t| t.v = 9));
        assert!(!m.patch(&2, |t| t.v = 9), "equal patch does not notify");
        m.upsert(it(3, 0)); // unchanged
        m.remove_key(&1);
        assert_eq!(m.id_of(&2), Some(id2), "IDs are stable");
        assert_eq!(m.index_of(id2), Some(2));
        let s = seen.borrow();
        assert_eq!(
            s.len(),
            5,
            "one notification per operation outside a transaction"
        );
        assert_eq!(s[0].ops(), &[Change::Inserted { at: 3, len: 1 }]);
        assert_eq!(s[2].ops(), &[Change::Moved { from: 4, to: 1 }]);
        assert_eq!(s[3].updated(), &[id2]);
        assert_eq!(s[4].ops(), &[Change::Removed { at: 2, len: 1 }]);
        // Inserting an existing key replaces and moves it.
        drop(s);
        m.insert(0, it(3, 5));
        assert_eq!((keys(&m), m.get_key(&3).unwrap().v), (vec![3, 0, 4, 2], 5));
    }

    #[test]
    fn transaction_coalesces_into_one_notification() {
        let m = model(&[]);
        let seen = log(&m);
        m.transaction(|m| {
            for k in 0..1000 {
                m.push(it(k, 0));
            }
            for k in 0..1000 {
                m.patch(&k, |t| t.v += 1);
            }
            m.remove_range(10..20);
        });
        let s = seen.borrow();
        assert_eq!(s.len(), 1);
        assert_eq!(
            s[0].ops(),
            &[
                Change::Inserted { at: 0, len: 1000 },
                Change::Removed { at: 10, len: 10 }
            ]
        );
        assert!(
            s[0].updated().is_empty(),
            "new items are not also reported as updated"
        );
        assert_eq!(s[0].inserted().len(), 1000);
        assert_eq!(s[0].removed().len(), 10);
        drop(s);
        // Many scattered moves collapse into a reset.
        m.transaction(|m| {
            for k in 0..300 {
                m.move_key(&(k * 3), 0);
            }
        });
        assert!(seen.borrow()[1].is_reset());
    }

    #[test]
    fn sync_from_is_minimal_and_short_circuits() {
        let m = model(&[1, 2, 3, 4, 5]);
        let seen = log(&m);
        m.sync_from(vec![it(1, 0), it(2, 0), it(3, 0), it(4, 0), it(5, 0)]);
        assert_eq!(m.notifications(), 0, "same snapshot: nothing");
        m.sync_from(vec![it(1, 0), it(2, 7), it(3, 0), it(4, 0), it(5, 0)]);
        assert_eq!(seen.borrow()[0].updated(), &[m.id_of(&2).unwrap()]);
        assert!(!seen.borrow()[0].is_structural());
        // Bump 5 to the top: one move, not five.
        m.sync_from(vec![it(5, 0), it(1, 0), it(2, 7), it(3, 0), it(4, 0)]);
        assert_eq!(seen.borrow()[1].ops(), &[Change::Moved { from: 4, to: 0 }]);
        // Rotate left: one move (1 to the end).
        m.sync_from(vec![it(1, 0), it(2, 7), it(3, 0), it(4, 0), it(5, 0)]);
        assert_eq!(seen.borrow()[2].ops().len(), 1);
        // Remove 3, insert 9 after 4, change 1.
        let id4 = m.id_of(&4);
        m.sync_from(vec![it(1, 1), it(2, 7), it(4, 0), it(9, 0), it(5, 0)]);
        let c = seen.borrow()[3].clone();
        assert_eq!(
            c.ops(),
            &[
                Change::Removed { at: 2, len: 1 },
                Change::Inserted { at: 3, len: 1 }
            ]
        );
        assert_eq!(c.updated(), &[m.id_of(&1).unwrap()]);
        assert_eq!(m.id_of(&4), id4);
    }

    /// Tiny xorshift so the randomized check needs no dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n.max(1)
        }
    }

    #[test]
    fn sync_from_randomized_matches_snapshot_and_replays() {
        let mut rng = Rng(0x9e3779b97f4a7c15);
        for round in 0..400 {
            let n = rng.below(40) as u32;
            let mut cur: Vec<It> = (0..n).map(|k| it(k, 0)).collect();
            let m = ListModel::with_items(|t: &It| t.k, cur.clone());
            let seen = log(&m);
            for _step in 0..6 {
                let before = keys(&m);
                let ids: HashMap<u32, ItemId> =
                    before.iter().map(|k| (*k, m.id_of(k).unwrap())).collect();
                // Random edit of the snapshot: removals, inserts, moves, value changes.
                let mut next = cur.clone();
                for _ in 0..rng.below(5) {
                    if !next.is_empty() {
                        let i = rng.below(next.len() as u64) as usize;
                        next.remove(i);
                    }
                }
                for _ in 0..rng.below(5) {
                    let k = 1000 + rng.below(100_000) as u32;
                    let i = rng.below(next.len() as u64 + 1) as usize;
                    if !next.iter().any(|t| t.k == k) {
                        next.insert(i, it(k, 0));
                    }
                }
                let moves = if round % 10 == 0 { 100 } else { rng.below(4) };
                for _ in 0..moves {
                    if next.len() > 1 {
                        let a = rng.below(next.len() as u64) as usize;
                        let t = next.remove(a);
                        let b = rng.below(next.len() as u64 + 1) as usize;
                        next.insert(b, t);
                    }
                }
                for t in next.iter_mut() {
                    if rng.below(8) == 0 {
                        t.v += 1;
                    }
                }
                let changed_values: Vec<u32> = next
                    .iter()
                    .filter(|t| cur.iter().any(|c| c.k == t.k && c.v != t.v))
                    .map(|t| t.k)
                    .collect();
                let notes = seen.borrow().len();
                m.sync_from(next.clone());
                consistent(&m);
                assert_eq!(m.to_vec(), next, "round {round}: model equals snapshot");
                let after = keys(&m);
                // Survivors keep their IDs.
                for k in &after {
                    if let Some(id) = ids.get(k) {
                        assert_eq!(m.id_of(k), Some(*id));
                    }
                }
                if before == after && changed_values.is_empty() {
                    assert_eq!(seen.borrow().len(), notes, "unchanged: no notification");
                } else {
                    let c = seen.borrow().last().unwrap().clone();
                    if !c.is_reset() {
                        assert_eq!(
                            replay(&before, &m, &c),
                            after,
                            "round {round}: ops replay to the new order"
                        );
                        // Moves are minimal: kept items not on an LIS are the only ones moved.
                        let kept: Vec<usize> = before
                            .iter()
                            .filter(|k| after.contains(k))
                            .map(|k| after.iter().position(|a| a == k).unwrap())
                            .collect();
                        let lis = lis_mask(&kept).iter().filter(|x| **x).count();
                        let moved = c
                            .ops()
                            .iter()
                            .filter(|o| matches!(o, Change::Moved { .. }))
                            .count();
                        assert_eq!(moved, kept.len() - lis, "round {round}: minimal moves");
                        // map_index follows survivors.
                        for (oi, k) in before.iter().enumerate() {
                            assert_eq!(
                                c.map_index(oi),
                                after.iter().position(|a| a == k),
                                "round {round}: map_index"
                            );
                        }
                    }
                    let mut upd: Vec<u32> = c
                        .updated()
                        .iter()
                        .map(|id| m.key_of(*id).unwrap())
                        .collect();
                    upd.sort_unstable();
                    let mut want = changed_values.clone();
                    want.sort_unstable();
                    assert_eq!(
                        upd, want,
                        "round {round}: exactly the changed items are updated"
                    );
                }
                cur = next;
            }
        }
    }

    fn consistent(m: &ListModel<u32, It>) {
        let d = m.inner.data.borrow();
        assert_eq!(d.ids.len(), d.items.len(), "one key per item");
        assert_eq!(d.pos.len(), d.items.len(), "one position per item");
        for (i, e) in d.items.iter().enumerate() {
            assert_eq!(d.ids.get(&e.key), Some(&e.id));
            assert_eq!(d.pos.get(&e.id), Some(&i));
        }
    }

    #[test]
    fn sync_from_with_duplicate_keys_keeps_one() {
        let mut rng = Rng(0xdeadbeef12345);
        for _ in 0..300 {
            let n = rng.below(20) as u32;
            let m = model(&(0..n).collect::<Vec<_>>());
            let mut snap: Vec<It> = (0..n)
                .filter(|_| rng.below(4) != 0)
                .map(|k| it(k, 0))
                .collect();
            for _ in 0..rng.below(6) {
                let k = rng.below(n as u64 + 10) as u32;
                let at = rng.below(snap.len() as u64 + 1) as usize;
                snap.insert(at, it(k, 0));
            }
            m.sync_from(snap.clone());
            consistent(&m);
            let mut want: Vec<u32> = snap.iter().map(|t| t.k).collect();
            want.sort_unstable();
            want.dedup();
            let mut got = keys(&m);
            got.sort_unstable();
            assert_eq!(got, want, "every key once");
        }
    }

    #[test]
    fn panel_selection_survives_reorder_by_key() {
        let m = model(&[1, 2, 3, 4, 5]);
        let p = KeyboardPanel::new("t");
        m.bind_panel(&p);
        p.click(m.id_of(&2).unwrap(), false, false);
        p.click(m.id_of(&4).unwrap(), false, true);
        let sel: Vec<ItemId> = [2, 3, 4].iter().map(|k| m.id_of(k).unwrap()).collect();
        assert_eq!(p.selection(), sel);
        m.sync_from(vec![it(4, 0), it(5, 0), it(3, 0), it(1, 0), it(2, 0)]);
        assert_eq!(p.cursor(), m.id_of(&4), "cursor follows its item");
        assert_eq!(p.cursor_index(), Some(0));
        let mut sel_keys: Vec<u32> = p
            .selection()
            .iter()
            .map(|id| m.key_of(*id).unwrap())
            .collect();
        sel_keys.sort_unstable();
        assert_eq!(sel_keys, vec![2, 3, 4]);
        m.remove_key(&3);
        assert_eq!(p.selection_len(), 2, "removed items leave the selection");
        assert_eq!(p.len(), 4);
        m.sort_by(|a, b| a.k.cmp(&b.k));
        assert_eq!(
            p.cursor_index(),
            Some(2),
            "after a sort (reset) the cursor is still item 4"
        );
    }

    #[test]
    fn refit_anchors_and_lists_dirty_slots() {
        // 100 rows of 10 px, view 50 px, scrolled to row 20 (+3 px).
        let m = model(&(0..100).collect::<Vec<_>>());
        let mut slots = RowSlots::default();
        let top = 203;
        slots.begin(top, 7);
        for k in 0..7 {
            slots.drawn(k, m.id_at(20 + k));
        }
        let changes = Rc::new(RefCell::new(None));
        let c2 = changes.clone();
        m.subscribe(move |_, c| *c2.borrow_mut() = Some(c.clone()))
            .forever();
        let (a, b) = (m.clone(), m.clone());
        let id_at = move |i| a.id_at(i);
        let index_of = move |id| b.index_of(id);
        // Insert 3 above the viewport: the view moves by 3 rows, nothing redraws.
        m.transaction(|m| {
            for k in 0..3 {
                m.insert(0, it(500 + k, 0));
            }
        });
        let c = changes.borrow().clone().unwrap();
        let r = refit(&mut slots, top, 10, 50, m.len(), &c, &id_at, &index_of);
        assert_eq!(
            (r.top, r.full, r.slots.clone(), r.bar),
            (233, false, vec![], true)
        );
        // Patch a visible item and an invisible one: one slot.
        m.transaction(|m| {
            m.patch(&22, |t| t.v = 1);
            m.patch(&90, |t| t.v = 1);
        });
        let c = changes.borrow().clone().unwrap();
        let r = refit(&mut slots, 233, 10, 50, m.len(), &c, &id_at, &index_of);
        assert_eq!((r.full, r.slots.clone(), r.bar), (false, vec![2], false));
        // Remove a visible item: the slots below it change.
        m.remove_key(&24);
        let c = changes.borrow().clone().unwrap();
        let r = refit(&mut slots, 233, 10, 50, m.len(), &c, &id_at, &index_of);
        assert_eq!(r.top, 233);
        assert_eq!(r.slots, vec![4, 5, 6]);
        // At the very top, an insert at 0 shows up (no anchoring).
        let mut s0 = RowSlots::default();
        s0.begin(0, 7);
        for k in 0..7 {
            s0.drawn(k, m.id_at(k));
        }
        m.insert(0, it(900, 0));
        let c = changes.borrow().clone().unwrap();
        let r = refit(&mut s0, 0, 10, 50, m.len(), &c, &id_at, &index_of);
        assert!(r.top == 0 && (r.full || r.slots.len() == 7));
    }
}
