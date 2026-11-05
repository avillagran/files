//! Portable theme watcher: compares the size and modification time of the
//! few files the theme is made of (and the OS dark/light setting) once a
//! second. Used where there is no inotify (macOS, Windows, other Unixes).
//! Cost when idle: a handful of `stat` calls per second.

#![cfg_attr(target_os = "linux", allow(dead_code))]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// How often the files are compared.
pub(crate) const INTERVAL: Duration = Duration::from_secs(1);
/// Quiet time after a change before reloading (a theme switch writes several files).
const SETTLE: Duration = Duration::from_millis(150);

/// Files and directories whose change means "reload the theme".
pub(crate) fn theme_paths() -> Vec<PathBuf> {
    let state = crate::theme::omarchy_state_dir();
    let theme = crate::theme::theme_dir();
    let toggles = crate::glass::toggles_dir();
    vec![
        state.join("theme.name"),
        theme.clone(),
        theme.join("colors.toml"),
        theme.join("shell.toml"),
        theme.join("alacritty.toml"),
        crate::theme::user_shell_toml(),
        crate::tooltip::settings_path(),
        toggles.join("window-opaque.lua"),
        toggles.join("window-glassy.lua"),
    ]
}

/// What was seen: per path (modified, length) or None when absent, plus
/// the OS appearance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    files: Vec<Option<(Option<SystemTime>, u64)>>,
    dark: Option<bool>,
}

impl Snapshot {
    pub(crate) fn take(paths: &[PathBuf], dark: Option<bool>) -> Snapshot {
        let files = paths
            .iter()
            .map(|p| {
                std::fs::metadata(p)
                    .ok()
                    .map(|m| (m.modified().ok(), if m.is_dir() { 0 } else { m.len() }))
            })
            .collect();
        Snapshot { files, dark }
    }

    fn now() -> Snapshot {
        Snapshot::take(&theme_paths(), super::prefers_dark())
    }
}

/// Sleeps `d` in short slices; false when `stop` was set meanwhile.
fn nap(stop: &AtomicBool, d: Duration) -> bool {
    let slice = Duration::from_millis(250);
    let mut left = d;
    while !left.is_zero() {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        let s = left.min(slice);
        std::thread::sleep(s);
        left -= s;
    }
    !stop.load(Ordering::Relaxed)
}

/// Blocks until `stop`, calling `changed` once per settled change.
pub(crate) fn watch(stop: Arc<AtomicBool>, changed: &dyn Fn()) {
    let mut last = Snapshot::now();
    while nap(&stop, INTERVAL) {
        let mut now = Snapshot::now();
        if now == last {
            continue;
        }
        // Wait until the files stop changing (theme switch in progress).
        loop {
            if !nap(&stop, SETTLE) {
                return;
            }
            let again = Snapshot::now();
            if again == now {
                break;
            }
            now = again;
        }
        last = now;
        changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_sees_created_changed_and_removed_files() {
        let d = std::env::temp_dir().join(format!("fltk-omarchy-poll-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("colors.toml");
        let paths = vec![f.clone()];
        let a = Snapshot::take(&paths, None);
        std::fs::write(&f, "background = \"#000000\"\n").unwrap();
        let b = Snapshot::take(&paths, None);
        assert_ne!(a, b, "created");
        std::fs::write(&f, "background = \"#000000\"\nforeground = \"#ffffff\"\n").unwrap();
        let c = Snapshot::take(&paths, None);
        assert_ne!(b, c, "longer");
        assert_ne!(c, Snapshot::take(&paths, Some(true)), "OS appearance");
        std::fs::remove_file(&f).unwrap();
        assert_eq!(a, Snapshot::take(&paths, None), "removed again");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn watch_stops_promptly() {
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let t0 = std::time::Instant::now();
        let h = std::thread::spawn(move || watch(s2, &|| {}));
        std::thread::sleep(Duration::from_millis(50));
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        assert!(t0.elapsed() < Duration::from_secs(2));
    }
}
