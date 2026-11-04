//! Desktop applications and default handlers (freedesktop Desktop Entry,
//! MIME Applications Associations), for "Open" and "Open with".
//!
//! Read-only except [`set_default`], which rewrites only the user's
//! `$XDG_CONFIG_HOME/mimeapps.list` `[Default Applications]` (and adds the
//! association) atomically, keeping every other line. Scans are bounded.
use std::{
    collections::HashSet,
    ffi::OsStr,
    io::{self, Read},
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 4096;
const MAX_FILE: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    /// Desktop file ID (`org.gnome.Foo.desktop`).
    pub id: String,
    pub name: String,
    pub mimes: Vec<String>,
    /// Hidden from menus (still valid as a handler).
    pub no_display: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Catalog {
    pub apps: Vec<App>,
    pub limited: bool,
}

fn home() -> PathBuf {
    crate::theme::home()
}
fn env_dir(var: &str, default: PathBuf) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or(default)
}
fn env_list(var: &str, default: &str) -> Vec<PathBuf> {
    let raw = std::env::var(var).ok().filter(|v| !v.is_empty());
    raw.as_deref()
        .unwrap_or(default)
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .collect()
}
pub fn data_home() -> PathBuf {
    env_dir("XDG_DATA_HOME", home().join(".local/share"))
}
pub fn config_home() -> PathBuf {
    env_dir("XDG_CONFIG_HOME", home().join(".config"))
}
/// Application directories, most important first.
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![data_home().join("applications")];
    for d in env_list("XDG_DATA_DIRS", "/usr/local/share:/usr/share") {
        dirs.push(d.join("applications"));
    }
    dirs
}
/// mimeapps.list files, most important first (spec order).
fn mimeapps_files() -> Vec<PathBuf> {
    let mut files = vec![config_home().join("mimeapps.list")];
    for d in env_list("XDG_CONFIG_DIRS", "/etc/xdg") {
        files.push(d.join("mimeapps.list"));
    }
    files.push(data_home().join("applications/mimeapps.list"));
    for d in env_list("XDG_DATA_DIRS", "/usr/local/share:/usr/share") {
        files.push(d.join("applications/mimeapps.list"));
    }
    files
}

fn read_small(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_FILE).read_to_string(&mut text).ok()?;
    Some(text)
}

fn locale_keys() -> Vec<String> {
    let lang = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    let base = lang.split(['.', '@']).next().unwrap_or("").to_string();
    let mut keys = Vec::new();
    if !base.is_empty() && base != "C" && base != "POSIX" {
        keys.push(format!("Name[{base}]"));
        if let Some((l, _)) = base.split_once('_') {
            keys.push(format!("Name[{l}]"));
        }
    }
    keys.push("Name".into());
    keys
}

/// Parses the `[Desktop Entry]` group; None for non-applications.
pub fn parse_entry(id: &str, text: &str, names: &[String]) -> Option<App> {
    let mut in_group = false;
    let mut values: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            values.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let get = |k: &str| {
        values
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    if get("Type") != Some("Application")
        || get("Hidden") == Some("true")
        || get("Exec").is_none_or(str::is_empty)
    {
        return None;
    }
    let name = names.iter().find_map(|k| get(k)).unwrap_or(id).to_string();
    let mimes = get("MimeType")
        .unwrap_or("")
        .split(';')
        .filter(|m| !m.is_empty() && m.len() <= 255)
        .map(str::to_string)
        .collect();
    Some(App {
        id: id.to_string(),
        name,
        mimes,
        no_display: get("NoDisplay") == Some("true"),
    })
}

/// All applications; a user entry hides a system entry with the same ID.
pub fn scan() -> Catalog {
    scan_dirs(&application_dirs())
}
pub fn scan_dirs(dirs: &[PathBuf]) -> Catalog {
    let names = locale_keys();
    let mut seen = HashSet::new();
    let mut catalog = Catalog::default();
    for dir in dirs {
        let mut stack = vec![(dir.clone(), String::new())];
        while let Some((d, prefix)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for entry in rd.flatten() {
                if catalog.apps.len() + seen.len() > 2 * MAX_ENTRIES {
                    catalog.limited = true;
                    break;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() && prefix.matches('-').count() < 4 {
                    stack.push((entry.path(), format!("{prefix}{name}-")));
                    continue;
                }
                if !name.ends_with(".desktop") {
                    continue;
                }
                let id = format!("{prefix}{name}");
                if !seen.insert(id.clone()) {
                    continue;
                }
                if let Some(app) =
                    read_small(&entry.path()).and_then(|t| parse_entry(&id, &t, &names))
                {
                    catalog.apps.push(app);
                }
            }
        }
    }
    catalog.apps.sort_by_key(|a| a.name.to_lowercase());
    catalog
}

/// Values of `key` in `group` of a mimeapps.list text.
fn list_values(text: &str, group: &str, key: &str) -> Vec<String> {
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == group;
            continue;
        }
        if in_group
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return v
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

/// Default application ID for `mime`, checking that it is installed.
pub fn default_for(mime: &str, catalog: &Catalog) -> Option<String> {
    for file in mimeapps_files() {
        let Some(text) = read_small(&file) else {
            continue;
        };
        for id in list_values(&text, "[Default Applications]", mime) {
            if catalog.apps.iter().any(|a| a.id == id) {
                return Some(id);
            }
        }
    }
    // No explicit default: the first added association, then any handler.
    recommended(mime, catalog).into_iter().next()
}

/// Applications that declare `mime` (text/* also accepts text/plain
/// handlers, as the shared-mime-info subclassing does), plus user-added
/// associations; removed associations excluded.
pub fn recommended(mime: &str, catalog: &Catalog) -> Vec<String> {
    let mut added = Vec::new();
    let mut removed = HashSet::new();
    for file in mimeapps_files() {
        if let Some(text) = read_small(&file) {
            added.extend(list_values(&text, "[Added Associations]", mime));
            removed.extend(list_values(&text, "[Removed Associations]", mime));
        }
    }
    let wants = |m: &String| m == mime || (mime.starts_with("text/") && m == "text/plain");
    let mut out: Vec<String> = Vec::new();
    for id in added.into_iter().chain(
        catalog
            .apps
            .iter()
            .filter(|a| a.mimes.iter().any(wants))
            .map(|a| a.id.clone()),
    ) {
        if !removed.contains(&id) && !out.contains(&id) && catalog.apps.iter().any(|a| a.id == id) {
            out.push(id);
        }
    }
    out
}

/// MIME type by name, then by content for unknown names (text vs binary).
pub fn detect_mime(path: &Path) -> String {
    if path.is_dir() {
        return "inode/directory".into();
    }
    let by_name = crate::mime::mime(path);
    if by_name != "application/octet-stream" {
        return by_name.into();
    }
    let mut head = [0u8; 4096];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let head = &head[..n];
    if n == 0 {
        return "text/plain".into(); // empty files open as text, like Nautilus
    }
    if head.starts_with(b"%PDF") {
        return "application/pdf".into();
    }
    if head.starts_with(b"\x7fELF") {
        return "application/x-executable".into();
    }
    if !head.contains(&0) {
        return "text/plain".into();
    }
    by_name.into()
}

/// Makes `id` the user's default for `mime` (and an added association).
pub fn set_default(mime: &str, id: &str) -> io::Result<()> {
    set_default_in(&config_home().join("mimeapps.list"), mime, id)
}
pub fn set_default_in(file: &Path, mime: &str, id: &str) -> io::Result<()> {
    let ok =
        |s: &str| !s.is_empty() && s.len() <= 255 && !s.contains(['\n', '\r', '=', ';', '[', ']']);
    if !ok(mime) || !ok(id) || !id.ends_with(".desktop") {
        return Err(io::Error::other("tipo o aplicación no válidos"));
    }
    let old = std::fs::read_to_string(file).or_else(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Ok(String::new())
        } else {
            Err(e)
        }
    })?;
    let mut text = upsert(&old, "[Default Applications]", mime, |_| id.to_string());
    text = upsert(&text, "[Added Associations]", mime, |current| {
        let mut ids: Vec<&str> = current
            .split(';')
            .filter(|s| !s.is_empty() && *s != id)
            .collect();
        ids.insert(0, id);
        format!("{};", ids.join(";"))
    });
    let dir = file
        .parent()
        .ok_or_else(|| io::Error::other("sin carpeta"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".mimeapps.list.files-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Sets `key` in `group` (creating the group at the end if missing).
fn upsert(text: &str, group: &str, key: &str, value: impl Fn(&str) -> String) -> String {
    let mut out = Vec::new();
    let mut in_group = false;
    let mut found_group = false;
    let mut done = false;
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_group && !done {
                out.push(format!("{key}={}", value("")));
                done = true;
            }
            in_group = t == group;
            found_group |= in_group;
        } else if in_group
            && !done
            && let Some((k, v)) = t.split_once('=')
            && k.trim() == key
        {
            out.push(format!("{key}={}", value(v.trim())));
            done = true;
            continue;
        }
        out.push(line.to_string());
        if in_group && !done && i == lines.len() - 1 {
            out.push(format!("{key}={}", value("")));
            done = true;
        }
    }
    if !found_group {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push(group.to_string());
        out.push(format!("{key}={}", value("")));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// The application with this ID, if installed.
pub fn find<'a>(catalog: &'a Catalog, id: &str) -> Option<&'a App> {
    catalog.apps.iter().find(|a| a.id == id)
}

/// A desktop ID as a single argument for a launcher (validated).
pub fn launch_id(id: &str) -> Option<&OsStr> {
    (id.ends_with(".desktop")
        && !id.contains(['/', '\0'])
        && !id.starts_with('-')
        && id.len() <= 255)
        .then(|| OsStr::new(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_entries_and_localized_names() {
        let names = vec!["Name[es_CL]".to_string(), "Name[es]".into(), "Name".into()];
        let a = parse_entry(
            "imv.desktop",
            "[Desktop Entry]\nType=Application\nName=Image viewer\nName[es]=Visor\nExec=imv %F\nMimeType=image/png;image/jpeg;\n[Desktop Action x]\nName=Other\n",
            &names,
        )
        .unwrap();
        assert_eq!(
            (a.name.as_str(), a.mimes.len(), a.no_display),
            ("Visor", 2, false)
        );
        assert!(parse_entry("l.desktop", "[Desktop Entry]\nType=Link\nExec=x\n", &names).is_none());
        assert!(
            parse_entry(
                "h.desktop",
                "[Desktop Entry]\nType=Application\nHidden=true\nExec=x\n",
                &names
            )
            .is_none()
        );
    }

    #[test]
    fn scan_prefers_user_entries_and_subdirectory_ids() {
        let d = crate::persistence::test_dir("apps-scan");
        let _ = fs::remove_dir_all(&d);
        let (user, sys) = (d.join("user"), d.join("sys"));
        fs::create_dir_all(user.join("kde")).unwrap();
        fs::create_dir_all(&sys).unwrap();
        fs::write(
            user.join("ed.desktop"),
            "[Desktop Entry]\nType=Application\nName=Mine\nExec=ed\nMimeType=text/plain;\n",
        )
        .unwrap();
        fs::write(
            sys.join("ed.desktop"),
            "[Desktop Entry]\nType=Application\nName=System\nExec=ed\n",
        )
        .unwrap();
        fs::write(
            user.join("kde/view.desktop"),
            "[Desktop Entry]\nType=Application\nName=Kview\nExec=v\nMimeType=image/png;\n",
        )
        .unwrap();
        let c = scan_dirs(&[user, sys]);
        assert_eq!(c.apps.len(), 2);
        assert_eq!(find(&c, "ed.desktop").unwrap().name, "Mine");
        assert!(find(&c, "kde-view.desktop").is_some());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn set_default_keeps_other_lines_and_adds_association() {
        let d = crate::persistence::test_dir("apps-default");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("mimeapps.list");
        fs::write(&f, "# mine\n[Default Applications]\ntext/html=chromium.desktop\nimage/png=old.desktop\n\n[Added Associations]\nimage/png=old.desktop;\n").unwrap();
        set_default_in(&f, "image/png", "imv.desktop").unwrap();
        let t = fs::read_to_string(&f).unwrap();
        assert!(t.contains("# mine") && t.contains("text/html=chromium.desktop"));
        assert_eq!(
            list_values(&t, "[Default Applications]", "image/png"),
            vec!["imv.desktop"]
        );
        assert_eq!(
            list_values(&t, "[Added Associations]", "image/png"),
            vec!["imv.desktop", "old.desktop"]
        );
        set_default_in(&f, "text/x-new", "ed.desktop").unwrap();
        let t = fs::read_to_string(&f).unwrap();
        assert_eq!(
            list_values(&t, "[Default Applications]", "text/x-new"),
            vec!["ed.desktop"]
        );
        let fresh = d.join("new/mimeapps.list");
        set_default_in(&fresh, "a/b", "x.desktop").unwrap();
        assert!(
            fs::read_to_string(&fresh)
                .unwrap()
                .contains("[Default Applications]\na/b=x.desktop")
        );
        assert!(set_default_in(&f, "a/b", "bad").is_err());
        assert!(set_default_in(&f, "a\nb", "x.desktop").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn content_detection_for_unknown_names() {
        let d = crate::persistence::test_dir("apps-mime");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("README"), b"hola\n").unwrap();
        fs::write(d.join("blob"), [0u8, 1, 2, 3]).unwrap();
        fs::write(d.join("doc"), b"%PDF-1.7").unwrap();
        assert_eq!(detect_mime(&d.join("README")), "text/plain");
        assert_eq!(detect_mime(&d.join("blob")), "application/octet-stream");
        assert_eq!(detect_mime(&d.join("doc")), "application/pdf");
        assert_eq!(detect_mime(&d.join("x.png")), "image/png");
        assert_eq!(detect_mime(&d), "inode/directory");
        assert!(
            launch_id("imv.desktop").is_some()
                && launch_id("-x.desktop").is_none()
                && launch_id("a/b.desktop").is_none()
        );
        let _ = fs::remove_dir_all(&d);
    }
}
