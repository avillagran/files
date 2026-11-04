use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
pub struct ThemeWatch {
    watcher: Option<RecommendedWatcher>,
    rx: Receiver<()>,
    last: Option<Instant>,
    paths: Vec<PathBuf>,
    file: Option<PathBuf>,
    toolkit: bool,
}
impl ThemeWatch {
    pub fn new(toolkit: bool) -> Self {
        Self::with_path(None, toolkit)
    }
    fn with_path(file: Option<PathBuf>, toolkit: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let watcher = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
            if r.is_ok_and(|e| {
                matches!(
                    e.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                )
            }) {
                let _ = tx.try_send(());
            }
        })
        .map_err(|e| eprintln!("No se pudo vigilar el tema: {e}"))
        .ok();
        let mut w = Self {
            watcher,
            rx,
            last: None,
            paths: Vec::new(),
            file,
            toolkit,
        };
        w.attach();
        w
    }
    fn attach(&mut self) {
        if let Some(w) = self.watcher.as_mut() {
            for p in self.paths.drain(..) {
                let _ = w.unwatch(&p);
            }
            let file = self
                .file
                .clone()
                .unwrap_or_else(files_core::theme::theme_path);
            let mut paths = Vec::new();
            if let Some(parent) = file.parent() {
                paths.push(parent.to_path_buf());
                if let Some(p) = parent.parent() {
                    paths.push(p.to_path_buf());
                }
                if let Some(p) = parent.parent().and_then(|p| p.parent()) {
                    paths.push(p.to_path_buf());
                }
            }
            if let Ok(real) = file.canonicalize()
                && let Some(p) = real.parent()
            {
                paths.push(p.to_path_buf());
            }
            if self.toolkit {
                let state = fltk_omarchy::theme::omarchy_state_dir();
                let theme = fltk_omarchy::theme::theme_dir();
                let shell = fltk_omarchy::theme::user_shell_toml();
                paths.push(state);
                paths.push(theme.clone());
                if let Ok(real) = theme.canonicalize() {
                    paths.push(real);
                }
                if let Some(parent) = shell.parent() {
                    paths.push(parent.to_path_buf());
                }
            }
            // Omarchy's window transparency toggles (Glassy windows) are
            // followed even with FILES_THEME: they are not part of the theme.
            // The directory appears only once a toggle is set, so the nearest
            // existing ancestor is watched until it does.
            if let Some(dir) = fltk_omarchy::glass::toggles_dir()
                .ancestors()
                .take(3)
                .find(|p| p.is_dir())
            {
                paths.push(dir.to_path_buf());
            }
            paths.sort();
            paths.dedup();
            for p in paths {
                if w.watch(&p, RecursiveMode::NonRecursive).is_ok() {
                    self.paths.push(p);
                }
            }
        }
    }
    pub fn changed(&mut self) -> bool {
        while self.rx.try_recv().is_ok() {
            self.last = Some(Instant::now());
        }
        if self
            .last
            .is_some_and(|i| i.elapsed() >= Duration::from_millis(100))
        {
            self.last = None;
            self.attach();
            true
        } else {
            false
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink};
    fn wait(w: &mut ThemeWatch) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            if w.changed() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("El watcher no notificó el cambio");
    }
    #[test]
    fn atomic_symlink_and_palette_replacement() {
        let base = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!("watch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(base.clone());
        for name in ["one", "two"] {
            fs::create_dir(base.join(name)).unwrap();
            fs::write(
                base.join(name).join("colors.toml"),
                "background='#010203'\nforeground='#ffffff'",
            )
            .unwrap();
        }
        symlink(base.join("one"), base.join("theme")).unwrap();
        let mut w = ThemeWatch::with_path(Some(base.join("theme/colors.toml")), false);
        assert!(w.watcher.is_some());
        symlink(base.join("two"), base.join("next")).unwrap();
        fs::rename(base.join("next"), base.join("theme")).unwrap();
        wait(&mut w);
        fs::write(
            base.join("two/new.toml"),
            "background='#aabbcc'\nforeground='#eeeeee'",
        )
        .unwrap();
        fs::rename(base.join("two/new.toml"), base.join("two/colors.toml")).unwrap();
        wait(&mut w);
        let palette = files_core::theme::Palette::parse(
            &fs::read_to_string(base.join("theme/colors.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(palette.background, files_core::theme::Color(170, 187, 204));
    }
}
