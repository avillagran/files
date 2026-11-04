use crate::theme::{config_dir, home};
use std::path::PathBuf;
#[derive(Clone, Debug)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
}
pub fn xdg_places() -> Vec<Place> {
    let h = home();
    let mut out = vec![Place {
        label: "Inicio".into(),
        path: h.clone(),
    }];
    let config = config_dir().parent().unwrap().join("user-dirs.dirs");
    let text = std::fs::read_to_string(config).unwrap_or_default();
    for (key, label, fallback) in [
        ("DOWNLOAD", "Descargas", "Downloads"),
        ("DOCUMENTS", "Documentos", "Documents"),
        ("PICTURES", "Imágenes", "Pictures"),
        ("MUSIC", "Música", "Music"),
        ("VIDEOS", "Vídeos", "Videos"),
        ("DESKTOP", "Escritorio", "Desktop"),
    ] {
        let prefix = format!("XDG_{key}_DIR=");
        let path = text
            .lines()
            .find_map(|l| l.strip_prefix(&prefix))
            .map(|v| PathBuf::from(v.trim_matches('"').replace("$HOME", &h.to_string_lossy())))
            .unwrap_or_else(|| h.join(fallback));
        if path.is_dir() && path != h {
            out.push(Place {
                label: label.into(),
                path,
            });
        }
    }
    out
}
#[derive(Clone, Debug)]
pub struct Mount {
    pub source: String,
    pub path: PathBuf,
    pub fs: String,
}
fn unescape(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some(Mount {
                source: unescape(f.next()?),
                path: PathBuf::from(unescape(f.next()?)),
                fs: f.next()?.into(),
            })
        })
        .collect()
}
pub fn mounts() -> Vec<Mount> {
    parse_mounts(&std::fs::read_to_string("/proc/mounts").unwrap_or_default())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escaped_mount() {
        let m = parse_mounts("/dev/sda /media/My\\040Disk ext4 rw 0 0");
        assert_eq!(m[0].path, PathBuf::from("/media/My Disk"));
    }
}
