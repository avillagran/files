//! Checks translation catalogs (o3's flat JSON), like o3's
//! tools/check-translations.cjs: missing and extra keys per locale,
//! `{placeholder}` mismatches, missing or unused plural forms, and with
//! `--sources` the strings the code translates that the catalog lacks.
//! Exits 1 when anything is found.
//!
//!   cargo run --example i18n_check                         # the toolkit catalog
//!   cargo run --example i18n_check -- i18n/app.json --sources src
//!   cargo run --example i18n_check -- a.json b.json --reference en --with-toolkit
use fltk_omarchy::i18n::{self, catalog, Catalog};
use std::path::{Path, PathBuf};

fn main() {
    let mut files = Vec::new();
    let mut sources: Vec<PathBuf> = Vec::new();
    let mut reference = "en".to_string();
    let mut with_toolkit = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--sources" => sources.push(args.next().expect("--sources DIR").into()),
            "--reference" => reference = args.next().expect("--reference LOCALE"),
            "--with-toolkit" => with_toolkit = true,
            "-h" | "--help" => {
                println!("i18n_check [catalog.json ...] [--reference en] [--with-toolkit] [--sources DIR ...]");
                return;
            }
            _ => files.push(a),
        }
    }
    let mut cat = if files.is_empty() || with_toolkit {
        Catalog::from_json(i18n::TOOLKIT_JSON).expect("toolkit catalog")
    } else {
        Catalog::new()
    };
    for f in &files {
        match Catalog::from_file(f) {
            Ok(c) => cat.merge(&c),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    }
    let report = cat.check(&reference);
    let mut problems = report.issues.len();
    print!("{report}");
    for dir in &sources {
        let mut unlisted = Vec::new();
        scan(dir, &mut |path, src| {
            for key in catalog::source_keys(src) {
                if cat.get(&reference, &key).is_none() && !unlisted.iter().any(|(_, k)| *k == key) {
                    unlisted.push((path.display().to_string(), key));
                }
            }
        });
        for (p, k) in &unlisted {
            println!("{p}: unlisted UI string \"{k}\" (not in \"{reference}\")");
        }
        problems += unlisted.len();
    }
    let keys = cat
        .keys(&reference)
        .iter()
        .filter(|k| !k.contains('|'))
        .count();
    println!(
        "{} locales × {keys} keys: {}",
        cat.locales().len(),
        if problems == 0 {
            "no issues".to_string()
        } else {
            format!("{problems} issue(s)")
        }
    );
    std::process::exit(if problems == 0 { 0 } else { 1 });
}

fn scan(dir: &Path, f: &mut dyn FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            scan(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs") {
            if let Ok(src) = std::fs::read_to_string(&p) {
                f(&p, &src);
            }
        }
    }
}
