//! Change hints for the folders shown on screen.
//!
//! The listing watch observes the open folder itself; a file added *inside*
//! a visible subfolder changes nothing there, so its "N directas" count, date
//! and measured size went stale. This watches the visible subfolders (not
//! recursive, at most [`MAX_WATCHES`]) from its own thread (attaching can
//! block on slow mounts) and reports which names changed. The model debounces
//! the hints and only re-reads those rows: idle cost is zero, nothing polls.
use notify::{EventKind, RecursiveMode, Watcher};
use std::{
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

/// inotify watches are a per-user resource: keep the visible window small.
pub const MAX_WATCHES: usize = 64;
/// Most hints kept between polls; extra changes collapse to a full re-read.
const MAX_HINTS: usize = 256;

enum Command {
    Set(PathBuf, Vec<OsString>),
}

pub struct ChildWatch {
    tx: Option<Sender<Command>>,
    rx: Option<Receiver<(PathBuf, OsString)>>,
    /// What the thread was last asked to watch.
    current: (PathBuf, Vec<OsString>),
}
impl Default for ChildWatch {
    fn default() -> Self {
        Self {
            tx: None,
            rx: None,
            current: (PathBuf::new(), Vec::new()),
        }
    }
}

impl ChildWatch {
    fn start(&mut self) {
        let (tx, commands) = mpsc::channel::<Command>();
        let (out, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("files-child-watch".into())
            .spawn(move || run(commands, out));
        if spawned.is_ok() {
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
    }
    /// Visible subfolders of `parent` (only sent when the set changes).
    pub fn set(&mut self, parent: &Path, mut names: Vec<OsString>) {
        names.truncate(MAX_WATCHES);
        names.sort();
        if self.current.0 == parent && self.current.1 == names {
            return;
        }
        if self.tx.is_none() {
            if names.is_empty() {
                return;
            }
            self.start();
        }
        self.current = (parent.to_path_buf(), names.clone());
        if let Some(tx) = &self.tx
            && tx.send(Command::Set(parent.to_path_buf(), names)).is_err()
        {
            self.tx = None;
            self.rx = None;
        }
    }
    /// Names of `parent`'s subfolders that changed since the last poll.
    pub fn poll(&mut self, parent: &Path) -> Vec<OsString> {
        let mut out: Vec<OsString> = Vec::new();
        let Some(rx) = &self.rx else { return out };
        loop {
            match rx.try_recv() {
                Ok((p, name)) => {
                    if p == parent && !out.contains(&name) && out.len() < MAX_HINTS {
                        out.push(name);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.tx = None;
                    self.rx = None;
                    break;
                }
            }
        }
        out
    }
    #[cfg(test)]
    pub fn watching(&self) -> usize {
        self.current.1.len()
    }
}

fn run(commands: Receiver<Command>, out: Sender<(PathBuf, OsString)>) {
    let (events_tx, events) = mpsc::channel::<PathBuf>();
    let mut watcher = match notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
        if let Ok(e) = r
            && matches!(
                e.kind,
                EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
            )
        {
            for p in e.paths {
                let _ = events_tx.send(p);
            }
        }
    }) {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut parent = PathBuf::new();
    let mut watched: HashSet<OsString> = HashSet::new();
    loop {
        // Commands first, then events; block briefly so the thread idles.
        match commands.recv_timeout(std::time::Duration::from_millis(250)) {
            Ok(Command::Set(p, names)) => {
                let wanted: HashSet<OsString> = names.into_iter().collect();
                if p != parent {
                    for n in watched.drain() {
                        let _ = watcher.unwatch(&parent.join(&n));
                    }
                    parent = p;
                }
                for n in watched.difference(&wanted).cloned().collect::<Vec<_>>() {
                    let _ = watcher.unwatch(&parent.join(&n));
                    watched.remove(&n);
                }
                for n in wanted {
                    if !watched.contains(&n)
                        && watcher
                            .watch(&parent.join(&n), RecursiveMode::NonRecursive)
                            .is_ok()
                    {
                        watched.insert(n);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(path) = events.try_recv() {
            // An event names the changed child; the watched folder is its
            // parent (or the folder itself for attribute changes).
            let folder = |p: &Path| p.parent() == Some(parent.as_path());
            let name = if folder(&path) {
                path.file_name().map(|n| n.to_os_string())
            } else if path.parent().and_then(|p| p.parent()) == Some(parent.as_path()) {
                path.parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_os_string())
            } else {
                None
            };
            if let Some(n) = name.filter(|n| watched.contains(n))
                && out.send((parent.clone(), n)).is_err()
            {
                return;
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn reports_changes_inside_visible_subfolders_only() {
        let d = std::env::temp_dir().join(format!("files-child-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for n in ["a", "b", "c"] {
            std::fs::create_dir_all(d.join(n)).unwrap();
        }
        let mut w = ChildWatch::default();
        w.set(&d, vec!["a".into(), "b".into()]);
        assert_eq!(w.watching(), 2);
        std::thread::sleep(std::time::Duration::from_millis(600));
        std::fs::write(d.join("a/new.txt"), b"x").unwrap();
        std::fs::write(d.join("c/ignored.txt"), b"x").unwrap();
        let mut seen = Vec::new();
        for _ in 0..40 {
            seen.extend(w.poll(&d));
            if !seen.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(100));
                seen.extend(w.poll(&d));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        seen.sort();
        seen.dedup();
        assert_eq!(seen, vec![OsString::from("a")]);
        // Same set: no new command; empty set unwatches.
        w.set(&d, vec!["b".into(), "a".into()]);
        w.set(&d, Vec::new());
        assert_eq!(w.watching(), 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Debounce before re-reading a changed row (bursts of writes coalesce).
const QUIET: std::time::Duration = std::time::Duration::from_millis(1500);
/// A measured folder size is recomputed at most this often.
const SIZE_EVERY: std::time::Duration = std::time::Duration::from_secs(60);
/// The visible set is re-sent at most this often while scrolling.
const SET_EVERY: std::time::Duration = std::time::Duration::from_millis(300);

/// A folder leaving the view keeps its watch this long: reloads reorder
/// rows for a moment and must not open a gap where changes are missed.
const LINGER: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Default)]
pub struct Pending {
    changed: std::collections::HashMap<OsString, std::time::Instant>,
    sized: std::collections::HashMap<OsString, std::time::Instant>,
    /// Watched names and when they were last on screen.
    seen: std::collections::HashMap<OsString, std::time::Instant>,
    /// Names whose watch was dropped (a later watch re-reads the row).
    dropped: HashSet<OsString>,
    last_set: Option<std::time::Instant>,
    path: PathBuf,
}

impl crate::model::Model {
    /// Watches visible subfolders and refreshes the rows that changed:
    /// count, date and (throttled) a measured size. True when repainting.
    pub fn pump_child_watch(&mut self) -> bool {
        use std::time::Instant;
        let now = Instant::now();
        if self.child_pending.path != self.path {
            self.child_pending = Pending {
                path: self.path.clone(),
                ..Default::default()
            };
        }
        // A reload of the open folder (e.g. after creating a link) is not a
        // reason to stop watching: changes made meanwhile would be lost.
        if self.closing || self.view == crate::model::View::Columns {
            let path = self.path.clone();
            self.child_watch.set(&path, Vec::new());
            return false;
        }
        if self.loaded
            && self.visible_generation == self.generation
            && self
                .child_pending
                .last_set
                .is_none_or(|t| now - t >= SET_EVERY)
        {
            let visible: Vec<OsString> = self
                .visible
                .iter()
                .filter_map(|i| self.entries.get(*i))
                .filter(|e| e.is_dir())
                .take(MAX_WATCHES)
                .map(|e| e.name.clone())
                .collect();
            let pending = &mut self.child_pending;
            for name in visible {
                if pending.seen.insert(name.clone(), now).is_none() && pending.dropped.remove(&name)
                {
                    // Watched again after a gap: changes may have been missed.
                    pending
                        .changed
                        .entry(name)
                        .or_insert(now.checked_sub(QUIET).unwrap_or(now));
                }
            }
            let expired: Vec<OsString> = pending
                .seen
                .iter()
                .filter(|(_, t)| now - **t > LINGER)
                .map(|(n, _)| n.clone())
                .collect();
            for name in expired {
                pending.seen.remove(&name);
                pending.dropped.insert(name);
            }
            while pending.seen.len() > MAX_WATCHES {
                let oldest = pending
                    .seen
                    .iter()
                    .min_by_key(|(_, t)| **t)
                    .map(|(n, _)| n.clone())
                    .expect("non-empty");
                pending.seen.remove(&oldest);
                pending.dropped.insert(oldest);
            }
            if pending.dropped.len() > 4096 {
                pending.dropped.clear();
            }
            let names: Vec<OsString> = pending.seen.keys().cloned().collect();
            pending.last_set = Some(now);
            let path = self.path.clone();
            if std::env::var_os("FILES_CHILD_WATCH_TRACE").is_some() {
                eprintln!("FILES_CHILD_WATCH_SET {:?}", names);
            }
            self.child_watch.set(&path, names);
        }
        let path = self.path.clone();
        for name in self.child_watch.poll(&path) {
            if std::env::var_os("FILES_CHILD_WATCH_TRACE").is_some() {
                eprintln!("FILES_CHILD_WATCH_EVENT {name:?}");
            }
            self.child_pending.changed.insert(name, now);
        }
        let due: Vec<OsString> = self
            .child_pending
            .changed
            .iter()
            .filter(|(_, t)| now - **t >= QUIET)
            .map(|(n, _)| n.clone())
            .collect();
        if !self.loaded {
            return false;
        }
        let mut dirty = false;
        for name in due {
            let Some(i) = self.entries.iter().position(|e| e.name == name) else {
                self.child_pending.changed.remove(&name);
                continue;
            };
            // Count and date: the visible-row request path re-reads them.
            self.requested.remove(&i);
            self.stats_requested.remove(&i);
            self.entries[i].stat_attempted = false;
            // Measured size: requeue slowly (the size queue runs one at a time).
            let measured = self.sizes.measured_mode(&path, &name);
            let recent = self
                .child_pending
                .sized
                .get(&name)
                .is_some_and(|t| now - *t < SIZE_EVERY);
            if let Some(mode) = measured {
                if recent {
                    // Keep the hint; it is retried once the minute has passed.
                    continue;
                }
                if self
                    .sizes
                    .enqueue_mode(&path, std::iter::once(name.as_os_str()), mode)
                    .is_ok()
                {
                    self.child_pending.sized.insert(name.clone(), now);
                }
            }
            self.child_pending.changed.remove(&name);
            eprintln!(
                "FILES_CHILD_REFRESH name={:?} size_requeued={}",
                name,
                measured.is_some()
            );
            dirty = true;
        }
        dirty
    }
}
