//! Read-only FD-anchored search; content reads are explicit and bounded.
use files_core::{
    listing::{Entry, Kind},
    name_filter::Filter,
    search::{Hit, MAX_PATH_BYTES, MAX_RESULTS, MAX_VISITS, Progress, Report},
};
use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
pub struct Limits {
    pub visits: u64,
    pub results: usize,
    pub depth: usize,
    pub elapsed: Duration,
    pub content_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            visits: MAX_VISITS,
            results: MAX_RESULTS,
            depth: 64,
            elapsed: Duration::from_secs(10),
            content_bytes: files_core::search_content::MAX_TOTAL,
        }
    }
}
#[cfg(target_os = "linux")]
pub fn scan(root: &Path, query: &str, hidden: bool, limits: Limits) -> io::Result<Report> {
    scan_inner(
        root,
        query,
        hidden,
        limits,
        (false, false),
        |_, _| Ok(()),
        None,
    )
}
#[cfg(target_os = "linux")]
pub fn scan_progress(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    mut publish: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Report> {
    scan_updates(root, query, hidden, limits, false, |progress, _| {
        publish(progress)
    })
}
/// Borrowed provisional prefix, in discovery order. No cloning or second scan.
/// A successful callback does not imply a valid final root: errors after the last
/// callback still invalidate every provisional observation. The returned Report
/// remains the only terminal result and is sorted after root revalidation.
#[cfg(target_os = "linux")]
pub fn scan_updates(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    eager_first_hit: bool,
    publish: impl FnMut(Progress, &[Hit]) -> io::Result<()>,
) -> io::Result<Report> {
    scan_inner(
        root,
        query,
        hidden,
        limits,
        (true, eager_first_hit),
        publish,
        None,
    )
}
/// A single FD-anchored walk. The callback borrows one page and must return only
/// after the consumer explicitly admits continuation. No rescan or all-hit vector.
#[cfg(target_os = "linux")]
pub fn scan_pages(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    mut page: impl FnMut(u64, u64, bool, &Report) -> io::Result<()>,
) -> io::Result<()> {
    scan_inner(
        root,
        query,
        hidden,
        limits,
        (false, false),
        |_, _| Ok(()),
        Some(&mut page),
    )?;
    Ok(())
}
#[cfg(target_os = "linux")]
type PageSink<'a> = dyn FnMut(u64, u64, bool, &Report) -> io::Result<()> + 'a;
#[cfg(target_os = "linux")]
fn scan_inner(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    tracking: (bool, bool),
    mut publish: impl FnMut(Progress, &[Hit]) -> io::Result<()>,
    mut page: Option<&mut PageSink<'_>>,
) -> io::Result<Report> {
    let (tracking, eager_first_hit) = tracking;
    use std::{
        fs::{self, File, OpenOptions, ReadDir},
        os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
        path::PathBuf,
    };
    fn open(path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    }
    fn mount_id(file: &File) -> io::Result<u64> {
        let mut stat = std::mem::MaybeUninit::<libc::statx>::zeroed();
        if unsafe {
            libc::statx(
                file.as_raw_fd(),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH,
                libc::STATX_MNT_ID,
                stat.as_mut_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let stat = unsafe { stat.assume_init() };
        if stat.stx_mask & libc::STATX_MNT_ID == 0 || stat.stx_mnt_id == 0 {
            return Err(io::Error::other(
                "No se pudo verificar el límite de montaje",
            ));
        }
        Ok(stat.stx_mnt_id)
    }
    struct Frame {
        _file: File,
        entries: ReadDir,
        relative: PathBuf,
    }
    fn frame(file: File, relative: PathBuf) -> io::Result<Frame> {
        let entries = fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
        Ok(Frame {
            _file: file,
            entries,
            relative,
        })
    }
    let bytes = root.as_os_str().as_encoded_bytes();
    if !root.is_absolute() || bytes.len() > 4096 || bytes.contains(&0) {
        return Err(io::Error::other(
            "La raíz de búsqueda debe ser absoluta y de hasta 4096 bytes",
        ));
    }
    let start = Instant::now();
    let filter = Filter::for_search(query);
    if query.trim().is_empty() {
        return Err(io::Error::other(
            "Escribe un nombre, patrón o filtro antes de buscar",
        ));
    }
    if let Some(error) = filter.error() {
        return Err(io::Error::other(error));
    }
    if limits.visits > MAX_VISITS
        || limits.results > MAX_RESULTS
        || limits.results == 0
        || limits.depth == 0
        || limits.depth > 64
        || limits.content_bytes == 0
        || limits.content_bytes > files_core::search_content::MAX_TOTAL
    {
        return Err(io::Error::other("Límites de búsqueda inválidos"));
    }
    let metadata_requested = filter.needs_metadata();
    let needle = filter.content().map(|text| match filter.regex() {
        Some(regex) => crate::search_content::Scanner::Regex(regex),
        None => crate::search_content::Scanner::Literal(crate::search_content::Needle::new(text)),
    });
    let mut content_budget = limits.content_bytes;
    let file = open(root)?;
    let mount = mount_id(&file)?;
    let root_proof = file.try_clone()?;
    let check_root = || -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let actual = open(root)?;
        let before = root_proof.metadata()?;
        let after = actual.metadata()?;
        if before.dev() != after.dev() || before.ino() != after.ino() || mount_id(&actual)? != mount
        {
            return Err(io::Error::other(
                "La raíz de búsqueda fue reemplazada; vuelve a buscar",
            ));
        }
        Ok(())
    };
    let mut deadline = start
        .checked_add(limits.elapsed)
        .ok_or_else(|| io::Error::other("Límite de tiempo fuera del rango"))?;
    let mut sequence = 0u64;
    let mut found_total = 0u64;

    let mut stack = vec![frame(file, PathBuf::new())?];
    let mut report = Report {
        metadata_requested,
        content: needle.as_ref().map(|_| files_core::search_content::Stats {
            regex: filter.regex().is_some(),
            ..Default::default()
        }),
        complete: true,
        ..Default::default()
    };
    let mut retained = 0usize;
    let mut last_progress = Instant::now();
    let mut positive_published = false;
    let mut hits_published = false;
    if tracking {
        publish(Progress::default(), &[])?;
    }
    while !stack.is_empty() {
        if tracking
            && report.visited > 0
            && ((eager_first_hit && !hits_published && !report.hits.is_empty())
                || (report.visited.is_multiple_of(128)
                    && (!positive_published
                        || last_progress.elapsed() >= Duration::from_millis(100))))
        {
            publish(Progress::from_report(&report), &report.hits)?;
            positive_published = true;
            hits_published |= !report.hits.is_empty();
            last_progress = Instant::now();
        }
        if Instant::now() >= deadline {
            report.reason = Some("Límite de tiempo; resultados parciales".into());
            break;
        }
        let Some(item) = stack.last_mut().unwrap().entries.next() else {
            stack.pop();
            continue;
        };
        if report.visited == limits.visits {
            report.reason = Some("Límite de entradas visitadas; resultados parciales".into());
            break;
        }
        report.visited += 1;
        let item = match item {
            Ok(v) => v,
            Err(_) => {
                report.errors += 1;
                continue;
            }
        };
        let name = item.file_name();
        if !files_core::visibility::accepts(&name, hidden) {
            continue;
        }
        let kind = match item.file_type() {
            Ok(t) => {
                if t.is_symlink() {
                    Kind::Symlink
                } else if t.is_dir() {
                    Kind::Directory
                } else if t.is_file() {
                    Kind::File
                } else {
                    Kind::Other
                }
            }
            Err(_) => {
                report.errors += 1;
                continue;
            }
        };
        let parent = &stack.last().unwrap().relative;
        let length =
            parent.as_os_str().len() + usize::from(!parent.as_os_str().is_empty()) + name.len();
        // Bound paths before allocating, including the absolute path a result
        // will eventually open. The report never authorizes an FS mutation.
        if length + root.as_os_str().len() + 1 > 4096 {
            report.errors += 1;
            continue;
        }
        let mut relative = PathBuf::with_capacity(length);
        relative.push(parent);
        relative.push(&name);
        let mut entry = Entry {
            name,
            kind,
            metadata: None,
            stat_attempted: false,
        };
        let candidate = filter.candidate_score(&entry).is_some();
        if candidate && metadata_requested && needle.is_none() {
            report.metadata_reads += 1;
            match fs::symlink_metadata(item.path()).and_then(|m| {
                let observed = if m.is_symlink() {
                    Kind::Symlink
                } else if m.is_dir() {
                    Kind::Directory
                } else if m.is_file() {
                    Kind::File
                } else {
                    Kind::Other
                };
                if observed != kind {
                    return Err(io::Error::other("Entrada cambió durante búsqueda"));
                }
                Ok(files_core::listing::Metadata {
                    size: m.len(),
                    modified: m.modified()?,
                })
            }) {
                Ok(metadata) => entry.metadata = Some(metadata),
                Err(_) => report.errors += 1,
            }
        }
        let mut content_match = None;
        let mut snippet = None;
        let mut accepted = candidate && (!metadata_requested || filter.score(&entry).is_some());
        if candidate && let Some(needle) = &needle {
            accepted = false;
            let observed = (|| -> io::Result<_> {
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                    .open(item.path())?;
                let metadata = file.metadata()?;
                let revision = files_core::media_source::Revision::from_metadata(&metadata)?;
                if mount_id(&file)? != mount {
                    return Ok(None);
                }
                if metadata_requested {
                    report.metadata_reads += 1;
                    entry.metadata = Some(files_core::listing::Metadata {
                        size: metadata.len(),
                        modified: metadata.modified()?,
                    });
                    if filter.score(&entry).is_none() {
                        return Ok(Some((file, revision, false)));
                    }
                }
                Ok(Some((file, revision, true)))
            })();
            match observed {
                Err(_) => report.errors += 1,
                Ok(None) => report.mounts_excluded += 1,
                Ok(Some((mut file, revision, true))) => {
                    report.content.as_mut().unwrap().files += 1;
                    if revision.bytes > files_core::search_content::MAX_FILE {
                        report.content.as_mut().unwrap().limited += 1;
                    } else {
                        let result = needle.scan(&mut file, &mut content_budget, deadline);
                        report.content.as_mut().unwrap().bytes =
                            limits.content_bytes - content_budget;
                        use crate::search_content::Outcome;
                        match result {
                            Err(_) => report.errors += 1,
                            Ok((Outcome::Binary, _)) => {
                                report.content.as_mut().unwrap().binary += 1
                            }
                            Ok((Outcome::Limited, _)) => {
                                report.content.as_mut().unwrap().limited += 1
                            }
                            Ok((Outcome::Time, _)) => {
                                report.reason = Some("Parcial: tiempo leyendo texto".into());
                                break;
                            }
                            Ok((Outcome::Text(found), _)) => {
                                if !crate::search_content::unchanged(revision, &file, &item.path())
                                {
                                    report.content.as_mut().unwrap().changed += 1;
                                } else {
                                    accepted = found.is_some();
                                    content_match =
                                        found.as_ref().map(|m| files_core::search_content::Match {
                                            line: m.line,
                                            byte: m.byte,
                                            regex_end: m.regex_end,
                                        });
                                    snippet = found.and_then(|m| m.snippet);
                                }
                            }
                        }
                    }
                }
                Ok(Some((_, _, false))) => {}
            }
            if content_budget == 0 {
                report.reason = Some("Parcial: presupuesto de lectura".into());
                break;
            }
        }
        if accepted {
            if retained + relative.capacity() > MAX_PATH_BYTES {
                report.reason = Some("Presupuesto de rutas agotado; resultados parciales".into());
                break;
            }
            retained += relative.capacity();
            report.hits.push(Hit {
                relative: relative.clone(),
                kind,
                content: content_match,
                snippet,
            });
            found_total += 1;
            if (report.hits.len() == limits.results || retained > MAX_PATH_BYTES - 4096)
                && let Some(page) = page.as_mut()
            {
                check_root()?;
                report.complete = false;
                report.reason =
                    Some("Página completa; fin del recorrido todavía no comprobado".into());
                report.hits.sort_by(|a, b| a.relative.cmp(&b.relative));
                files_core::search::validate(&report)?;
                let waiting = Instant::now();
                page(sequence, found_total, true, &report)?;
                deadline = deadline
                    .checked_add(waiting.elapsed())
                    .ok_or_else(|| io::Error::other("Espera fuera del rango"))?;
                check_root()?;
                sequence += 1;
                report.hits.clear();
                retained = 0;
                report.reason = None;
            } else if report.hits.len() == limits.results {
                report.reason =
                    Some("Límite de resultados; reduce el alcance o añade filtros".into());
                break;
            }
        }
        if kind == Kind::Directory {
            if stack.len() == limits.depth {
                report.errors += 1;
                continue;
            }
            match open(&item.path()).and_then(|file| Ok((mount_id(&file)?, file))) {
                Ok((id, file)) if id == mount => match frame(file, relative) {
                    Ok(f) => stack.push(f),
                    Err(_) => report.errors += 1,
                },
                Ok(_) => report.mounts_excluded += 1,
                Err(_) => report.errors += 1,
            }
        }
    }
    if tracking {
        publish(Progress::from_report(&report), &report.hits)?;
    }
    if report.reason.is_none() && report.errors > 0 {
        report.reason =
            Some("Algunas entradas no se pudieron recorrer; resultados parciales".into());
    }
    if report.reason.is_none()
        && report
            .content
            .is_some_and(|s| s.limited > 0 || s.changed > 0)
    {
        report.reason = Some("Parcial: texto limitado/cambiado".into());
    }
    report.complete = report.reason.is_none();
    check_root()?;
    report.hits.sort_by(|a, b| a.relative.cmp(&b.relative));
    files_core::search::validate(&report)?;
    if let Some(page) = page.as_mut() {
        page(sequence, found_total, false, &report)?;
    }
    Ok(report)
}
#[cfg(not(target_os = "linux"))]
pub fn scan_pages(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    _page: impl FnMut(u64, u64, bool, &Report) -> io::Result<()>,
) -> io::Result<()> {
    scan(root, query, hidden, limits).map(|_| ())
}
#[cfg(not(target_os = "linux"))]
pub fn scan(_: &Path, _: &str, _: bool, _: Limits) -> io::Result<Report> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Búsqueda segura pendiente en esta plataforma",
    ))
}

#[cfg(not(target_os = "linux"))]
pub fn scan_progress(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    _: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Report> {
    scan(root, query, hidden, limits)
}

#[cfg(not(target_os = "linux"))]
pub fn scan_updates(
    root: &Path,
    query: &str,
    hidden: bool,
    limits: Limits,
    _: bool,
    _: impl FnMut(Progress, &[Hit]) -> io::Result<()>,
) -> io::Result<Report> {
    scan(root, query, hidden, limits)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static ID: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "files-search-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn recursive_regex_reports_empty_eof_and_partial_limits_with_original_paths() {
        let f = Fixture::new();
        fs::write(f.0.join("empty.txt"), b"").unwrap();
        fs::write(f.0.join("good.txt"), b"before\nerror42\nafter").unwrap();
        fs::write(f.0.join("binary.txt"), b"error42\0").unwrap();
        let r = scan(
            &f.0,
            r#"regex:"(?m)^error[0-9]+$""#,
            false,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].relative, std::path::Path::new("good.txt"));
        assert_eq!(r.hits[0].content.unwrap().regex_end, Some(14));
        assert!(r.content.unwrap().regex);
        let r = scan(&f.0, r"regex:\z", false, Limits::default()).unwrap();
        files_core::search::validate(&r).unwrap();
        assert_eq!(r.hits.len(), 2);
        assert!(
            r.hits
                .iter()
                .all(|h| h.content.unwrap().zero_width() && h.snippet.is_none())
        );
        fs::write(
            f.0.join("large.txt"),
            vec![b'x'; files_core::search_content::MAX_FILE as usize + 1],
        )
        .unwrap();
        let r = scan(&f.0, r"regex:x", false, Limits::default()).unwrap();
        assert!(!r.complete);
        assert_eq!(r.content.unwrap().limited, 1);
    }
    #[test]
    fn paged_walk_keeps_all_1207_raw_hits_once_in_bounded_windows() {
        let f = Fixture::new();
        for i in 0..1206 {
            fs::write(f.0.join(format!("file-{i}.txt")), b"x").unwrap();
        }
        fs::write(
            f.0.join(std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec())),
            b"x",
        )
        .unwrap();
        let mut names = std::collections::HashSet::new();
        let mut windows = Vec::new();
        scan_pages(
            &f.0,
            "ext:txt",
            false,
            Limits::default(),
            |sequence, found, more, r| {
                assert_eq!(sequence, windows.len() as u64);
                assert!(r.hits.len() <= MAX_RESULTS);
                assert!(
                    r.hits.iter().map(|h| h.relative.capacity()).sum::<usize>() <= MAX_PATH_BYTES
                );
                for hit in &r.hits {
                    assert!(names.insert(hit.relative.clone()));
                }
                assert_eq!(found, names.len() as u64);
                windows.push((r.hits.len(), more, r.complete));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            windows,
            [(500, true, false), (500, true, false), (207, false, true)]
        );
        assert_eq!(names.len(), 1207);
    }
    #[test]
    fn long_relative_names_fill_path_budget_before_500_without_losing_the_next_hit() {
        let f = Fixture::new();
        let mut parent = f.0.clone();
        for _ in 0..14 {
            parent.push("d".repeat(240));
            fs::create_dir(&parent).unwrap();
        }
        for i in 0..80 {
            fs::write(parent.join(format!("file-{i:03}.txt")), b"x").unwrap();
        }
        let mut windows = 0;
        let mut found = 0;
        scan_pages(
            &f.0,
            "ext:txt",
            false,
            Limits::default(),
            |_, total, _, r| {
                windows += 1;
                found += r.hits.len();
                assert_eq!(total, found as u64);
                assert!(r.hits.len() < MAX_RESULTS);
                assert!(
                    r.hits.iter().map(|h| h.relative.capacity()).sum::<usize>() <= MAX_PATH_BYTES
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(found, 80);
        assert!(windows >= 2);
    }
    #[test]
    fn directory_matching_the_page_boundary_still_descends_after_credit() {
        let f = Fixture::new();
        fs::create_dir_all(f.0.join("parent/child/grandchild")).unwrap();
        let mut names = Vec::new();
        let mut terminal = false;
        scan_pages(
            &f.0,
            "type:dir",
            false,
            Limits {
                results: 1,
                ..Limits::default()
            },
            |_, _, more, r| {
                names.extend(r.hits.iter().map(|h| h.relative.clone()));
                terminal = !more;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            names,
            vec![
                PathBuf::from("parent"),
                "parent/child".into(),
                "parent/child/grandchild".into()
            ]
        );
        assert!(terminal);
    }
    #[test]
    fn root_replaced_during_credit_wait_rejects_before_another_page() {
        let f = Fixture::new();
        fs::write(f.0.join("a"), b"x").unwrap();
        fs::write(f.0.join("b"), b"x").unwrap();
        let old = f.0.with_extension("held-root");
        let mut calls = 0;
        let result = scan_pages(
            &f.0,
            "type:file",
            false,
            Limits {
                results: 1,
                ..Limits::default()
            },
            |_, _, more, _| {
                calls += 1;
                assert!(more);
                fs::rename(&f.0, &old)?;
                fs::create_dir(&f.0)?;
                Ok(())
            },
        );
        fs::remove_dir_all(old).unwrap();
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
    #[test]
    fn visits_and_content_limits_are_session_wide_not_renewed_per_page() {
        let f = Fixture::new();
        for i in 0..8 {
            fs::write(f.0.join(format!("{i}.txt")), b"text").unwrap();
        }
        let mut terminal = None;
        scan_pages(
            &f.0,
            "type:file",
            false,
            Limits {
                results: 1,
                visits: 3,
                ..Limits::default()
            },
            |_, found, more, r| {
                assert!(r.visited <= 3 && found <= 3);
                if !more {
                    terminal = Some((found, r.complete, r.reason.clone()));
                }
                Ok(())
            },
        )
        .unwrap();
        let (found, complete, reason) = terminal.unwrap();
        assert_eq!(found, 3);
        assert!(!complete && reason.unwrap().contains("visitadas"));
        let mut terminal = None;
        scan_pages(
            &f.0,
            "content:text",
            false,
            Limits {
                results: 1,
                content_bytes: 12,
                ..Limits::default()
            },
            |_, _, more, r| {
                assert!(r.content.unwrap().bytes <= 12);
                if !more {
                    terminal = Some((r.content.unwrap().bytes, r.complete));
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(terminal, Some((12, false)));
    }
    #[test]
    fn credit_error_stops_walk_and_waiting_does_not_spend_active_time_budget() {
        let f = Fixture::new();
        for i in 0..3 {
            fs::write(f.0.join(format!("{i}.txt")), b"x").unwrap();
        }
        let mut calls = 0;
        let result = scan_pages(
            &f.0,
            "ext:txt",
            false,
            Limits {
                results: 1,
                ..Limits::default()
            },
            |_, _, _, _| {
                calls += 1;
                Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"))
            },
        );
        assert!(result.is_err());
        assert_eq!(calls, 1);
        let mut complete = false;
        scan_pages(
            &f.0,
            "ext:txt",
            false,
            Limits {
                results: 1,
                elapsed: Duration::from_secs(1),
                ..Limits::default()
            },
            |_, _, more, r| {
                if more {
                    std::thread::sleep(Duration::from_millis(1100));
                } else {
                    complete = r.complete;
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(complete);
    }
    #[test]
    fn borrowed_updates_preserve_prefix_raw_names_and_final_observations() {
        let f = Fixture::new();
        for i in 0..260 {
            fs::write(f.0.join(format!("{i:03}.txt")), b"x").unwrap();
        }
        let opaque = std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec());
        fs::write(f.0.join(&opaque), b"raw").unwrap();
        let mut previous = Vec::new();
        let mut progress = Progress::default();
        let mut positive_before_final = false;
        let mut first_hit_visit = None;
        let r = scan_updates(
            &f.0,
            "ext:txt",
            false,
            Limits::default(),
            true,
            |p, hits| {
                p.validate_after(progress)?;
                assert_eq!(p.found as usize, hits.len());
                if !hits.is_empty() && first_hit_visit.is_none() {
                    first_hit_visit = Some(p.visited);
                }
                assert!(hits.len() <= files_core::search::MAX_RESULTS);
                let names: Vec<_> = hits.iter().map(|h| h.relative.clone()).collect();
                assert!(names.starts_with(&previous));
                if p.visited < 261 && !hits.is_empty() {
                    positive_before_final = true;
                }
                previous = names;
                progress = p;
                Ok(())
            },
        )
        .unwrap();
        assert!(positive_before_final && r.complete);
        assert_eq!(first_hit_visit, Some(1));
        previous.sort();
        assert_eq!(
            previous,
            r.hits
                .iter()
                .map(|h| h.relative.clone())
                .collect::<Vec<_>>()
        );
        assert!(previous.iter().any(|p| p.as_os_str() == opaque));
        assert_eq!(progress, Progress::from_report(&r));
    }
    #[test]
    fn provisional_prefix_does_not_override_terminal_root_replacement_error() {
        let f = Fixture::new();
        let root = f.0.join("root");
        fs::create_dir(&root).unwrap();
        for i in 0..260 {
            fs::write(root.join(format!("{i}.txt")), b"x").unwrap();
        }
        let mut replaced = false;
        let result = scan_updates(
            &root,
            "ext:txt",
            false,
            Limits::default(),
            true,
            |p, hits| {
                if !replaced && p.found > 0 {
                    assert!(!hits.is_empty());
                    fs::rename(&root, f.0.join("preserved")).unwrap();
                    fs::create_dir(&root).unwrap();
                    replaced = true;
                }
                Ok(())
            },
        );
        assert!(replaced && result.is_err());
        assert_eq!(fs::read_dir(f.0.join("preserved")).unwrap().count(), 260);
    }
    #[test]
    fn failed_provisional_consumer_stops_scanning_without_terminal_success() {
        let f = Fixture::new();
        for i in 0..260 {
            fs::write(f.0.join(format!("{i}.txt")), b"x").unwrap();
        }
        let mut count = 0;
        let result = scan_updates(&f.0, "ext:txt", false, Limits::default(), true, |p, _| {
            count += 1;
            if p.found > 0 {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "consumer closed"));
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(count, 2);
    }
    #[test]
    fn content_regular_utf8_binary_limits_raw_symlinks_and_metadata_are_explicit() {
        let f = Fixture::new();
        fs::write(f.0.join("yes.txt"), "first\nHola mundo\n").unwrap();
        fs::write(f.0.join("case.txt"), "hola mundo").unwrap();
        fs::write(f.0.join("binary.txt"), b"Hola mundo\0").unwrap();
        fs::write(f.0.join("invalid.txt"), b"Hola mundo\xff").unwrap();
        fs::write(f.0.join(".hidden.txt"), "Hola mundo").unwrap();
        let raw = std::ffi::OsString::from_vec(b"raw-\xff.txt".to_vec());
        fs::write(f.0.join(&raw), "Hola mundo").unwrap();
        symlink("yes.txt", f.0.join("link.txt")).unwrap();
        let r = scan(
            &f.0,
            r#"ext:txt content:"Hola mundo" size:>1B"#,
            false,
            Limits::default(),
        )
        .unwrap();
        assert!(r.complete);
        assert_eq!(r.hits.len(), 2);
        assert_eq!(r.content.unwrap().binary, 2);
        assert!(r.metadata_requested);
        assert!(
            r.hits
                .iter()
                .all(|h| h.kind == Kind::File && h.content.is_some())
        );
        assert_eq!(
            r.hits
                .iter()
                .find(|h| h.relative == Path::new("yes.txt"))
                .unwrap()
                .content
                .unwrap()
                .line,
            2
        );
        assert!(r.hits.iter().any(|h| h.relative.as_os_str() == raw));
        assert_eq!(
            scan(&f.0, r#"content:"Hola mundo""#, true, Limits::default())
                .unwrap()
                .hits
                .len(),
            3
        );
        fs::write(
            f.0.join("large.txt"),
            vec![b'x'; files_core::search_content::MAX_FILE as usize + 1],
        )
        .unwrap();
        let r = scan(&f.0, r#"content:"Hola mundo""#, false, Limits::default()).unwrap();
        assert!(!r.complete);
        assert_eq!(r.content.unwrap().limited, 1);
        assert_eq!(r.hits.len(), 2);
        let r = scan(
            &f.0,
            r#"name:yes content:"Hola mundo""#,
            false,
            Limits {
                content_bytes: 2,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!r.complete && r.hits.is_empty());
        assert_eq!(r.content.unwrap().bytes, 2);
    }
    #[test]
    fn matching_read_cannot_publish_after_same_name_replacement_or_inplace_change() {
        use files_core::media_source::Revision;
        use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
        let f = Fixture::new();
        let path = f.0.join("file");
        fs::write(&path, b"needle").unwrap();
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        let before = Revision::from_file(&file).unwrap();
        let mut budget = 100;
        let found = crate::search_content::Needle::new("needle")
            .scan(
                &mut file,
                &mut budget,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        assert!(matches!(
            found.0,
            crate::search_content::Outcome::Text(Some(_))
        ));
        fs::write(f.0.join("replacement"), b"needle").unwrap();
        fs::rename(f.0.join("replacement"), &path).unwrap();
        assert!(!crate::search_content::unchanged(before, &file, &path));
        let current = fs::File::open(&path).unwrap();
        let before = Revision::from_file(&current).unwrap();
        fs::write(&path, b"other!").unwrap();
        assert!(!crate::search_content::unchanged(before, &current, &path));
    }
    #[test]
    fn streamed_counters_are_monotonic_and_root_replacement_never_returns_paths() {
        let f = Fixture::new();
        for i in 0..260 {
            fs::write(f.0.join(format!("{i}.txt")), b"x").unwrap();
        }
        let mut frames = Vec::new();
        let r = scan_progress(&f.0, "size:>1MiB", false, Limits::default(), |p| {
            frames.push(p);
            Ok(())
        })
        .unwrap();
        assert_eq!(frames[0], Progress::default());
        assert_eq!(*frames.last().unwrap(), Progress::from_report(&r));
        assert!(frames.iter().any(|p| p.visited == 128));
        for pair in frames.windows(2) {
            pair[1].validate_after(pair[0]).unwrap();
        }
        assert_eq!(r.metadata_reads, 260);
        assert!(r.complete && r.hits.is_empty());
        let saved = f.0.with_extension("preserved");
        let mut replaced = false;
        let result = scan_progress(&f.0, "ext:txt", false, Limits::default(), |_| {
            if !replaced {
                fs::rename(&f.0, &saved)?;
                fs::create_dir(&f.0)?;
                replaced = true;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(saved.join("0.txt").is_file());
        fs::remove_dir_all(saved).unwrap();
    }
    #[test]
    fn metadata_facets_are_lazy_nofollow_and_preserve_sources() {
        use std::time::{Duration, UNIX_EPOCH};
        let f = Fixture::new();
        fs::write(f.0.join("old.bin"), [7; 1024]).unwrap();
        fs::write(f.0.join("new.bin"), [8; 2048]).unwrap();
        fs::create_dir(f.0.join("folder.bin")).unwrap();
        symlink("old.bin", f.0.join("link.bin")).unwrap();
        let old = fs::OpenOptions::new()
            .write(true)
            .open(f.0.join("old.bin"))
            .unwrap();
        old.set_modified(UNIX_EPOCH + Duration::from_secs(86400))
            .unwrap();
        let plain = scan(&f.0, "ext:bin", false, Limits::default()).unwrap();
        assert_eq!(plain.metadata_reads, 0);
        assert!(!plain.metadata_requested);
        let sized = scan(
            &f.0,
            "ext:bin size:1KiB..2KiB before:1970-01-03",
            false,
            Limits::default(),
        )
        .unwrap();
        assert!(sized.complete && sized.metadata_requested);
        assert_eq!(sized.metadata_reads, 2); // No stat for directory or symlink.
        assert_eq!(sized.hits.len(), 1);
        assert_eq!(sized.hits[0].relative, PathBuf::from("old.bin"));
        let links = scan(
            &f.0,
            "type:link before:1970-01-03",
            false,
            Limits::default(),
        )
        .unwrap();
        assert!(links.complete && links.hits.is_empty()); // Link metadata, not old target.
        assert_eq!(links.metadata_reads, 1);
        let rejected = scan(&f.0, "nomatch size:>1B", false, Limits::default()).unwrap();
        assert_eq!(rejected.metadata_reads, 0);
        assert_eq!(fs::read(f.0.join("old.bin")).unwrap(), [7; 1024]);
        assert!(scan(&f.0, "size:wat", false, Limits::default()).is_err());
    }
    #[test]
    fn nested_filters_raw_hidden_and_symlink_cycles() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("nested")).unwrap();
        fs::create_dir(f.0.join(".hidden")).unwrap();
        let raw = std::ffi::OsString::from_vec(b"final-2026-\xff.rs".to_vec());
        fs::write(f.0.join("nested").join(&raw), b"ok").unwrap();
        fs::write(f.0.join("nested/draft-2026.rs"), b"no").unwrap();
        fs::write(f.0.join(".hidden/final-2026.rs"), b"hidden").unwrap();
        symlink(&f.0, f.0.join("nested/cycle")).unwrap();
        let r = scan(&f.0, "2026 ext:rs !draft", false, Limits::default()).unwrap();
        assert!(r.complete);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].relative, PathBuf::from("nested").join(raw));
        let r = scan(&f.0, "type:link", false, Limits::default()).unwrap();
        assert_eq!(r.hits.len(), 1);
        let r = scan(&f.0, "2026 ext:rs !draft", true, Limits::default()).unwrap();
        assert_eq!(r.hits.len(), 2);
    }
    #[test]
    fn explicit_partial_limits_never_claim_complete_or_not_found() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("sub")).unwrap();
        fs::write(f.0.join("sub/a.rs"), b"a").unwrap();
        fs::write(f.0.join("b.rs"), b"b").unwrap();
        for limits in [
            Limits {
                results: 1,
                ..Limits::default()
            },
            Limits {
                visits: 1,
                ..Limits::default()
            },
            Limits {
                depth: 1,
                ..Limits::default()
            },
            Limits {
                elapsed: Duration::ZERO,
                ..Limits::default()
            },
        ] {
            let r = scan(&f.0, "*.rs", false, limits).unwrap();
            assert!(!r.complete);
            assert!(r.reason.is_some());
        }
        assert!(scan(&f.0, "", false, Limits::default()).is_err());
        assert!(scan(&f.0, "type:bad", false, Limits::default()).is_err());
    }
    #[test]
    fn invalid_root_and_final_symlink_are_rejected() {
        let f = Fixture::new();
        fs::write(f.0.join("file"), b"a").unwrap();
        symlink(&f.0, f.0.join("link")).unwrap();
        assert!(scan(&f.0.join("link"), "*", false, Limits::default()).is_err());
        assert!(scan(&f.0.join("file"), "*", false, Limits::default()).is_err());
        let r = scan(&f.0, "not-here", false, Limits::default()).unwrap();
        assert!(r.complete && r.hits.is_empty());
    }
}
