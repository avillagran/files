use std::{env, fs, path::PathBuf};
fn comparable_header(path: &std::path::Path) -> String {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        // These strings are build-directory installation destinations, never
        // layout/ABI flags. Distinct host/target artifacts legitimately differ.
        .filter(|line| {
            !line.starts_with("#define FLTK_DATADIR ") && !line.starts_with("#define FLTK_DOCDIR ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn main() {
    println!("cargo:rerun-if-changed=native/window_gestures.cpp");
    println!("cargo:rerun-if-changed=native/file_transfer.cpp");
    println!("cargo:rerun-if-env-changed=FILES_FLTK_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=FILES_FLTK_CONFIG_DIR");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }
    // fltk-sys is also a build dependency so its generated headers exist before
    // this adapter is compiled. An explicit override supports vendored sources.
    let source = env::var_os("FILES_FLTK_SOURCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = env::var_os("CARGO_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env::var_os("HOME").expect("HOME or CARGO_HOME required"))
                        .join(".cargo")
                });
            let registry = home.join("registry/src");
            let mut candidates = fs::read_dir(registry)
                .expect("Cargo registry sources unavailable; set FILES_FLTK_SOURCE_DIR")
                .filter_map(Result::ok)
                .map(|e| e.path().join("fltk-sys-1.5.23/cfltk/fltk"))
                .filter(|p| {
                    p.join("src/drivers/Wayland/Fl_Wayland_Window_Driver.H")
                        .is_file()
                })
                .collect::<Vec<_>>();
            candidates.sort();
            // The same crate unpacked under two registry indexes (another
            // project or toolchain) is not ambiguous when the sources this
            // adapter compiles against are byte-identical.
            let key = |root: &PathBuf| {
                [
                    "src/drivers/Wayland/Fl_Wayland_Window_Driver.H",
                    "src/drivers/Wayland/Fl_Wayland_Screen_Driver.H",
                    "src/drivers/Wayland/fl_wayland_clipboard_dnd.cxx",
                    "FL/Fl_Window.H",
                    "FL/platform.H",
                ]
                .iter()
                .map(|f| fs::read(root.join(f)).unwrap_or_default())
                .collect::<Vec<_>>()
            };
            assert!(
                !candidates.is_empty() && candidates.iter().all(|c| key(c) == key(&candidates[0])),
                "Ambiguous/missing pinned FLTK sources; set FILES_FLTK_SOURCE_DIR"
            );
            candidates.remove(0)
        });
    let build = PathBuf::from(env::var_os("OUT_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let config = env::var_os("FILES_FLTK_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut paths = fs::read_dir(&build)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("fltk-sys-"))
                .map(|e| e.path().join("out/build/fltk"))
                .filter(|p| p.join("config.h").is_file())
                .collect::<Vec<_>>();
            paths.sort();
            let first = paths
                .first()
                .expect("FLTK generated headers missing; set FILES_FLTK_CONFIG_DIR");
            for path in &paths {
                for file in ["config.h", "FL/fl_config.h"] {
                    assert!(
                        comparable_header(&first.join(file)) == comparable_header(&path.join(file)),
                        "Ambiguous FLTK configurations; set FILES_FLTK_CONFIG_DIR for this build"
                    );
                }
            }
            first.clone()
        });
    for path in [&source, &config] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let repo = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for path in [
        "vendor/fltk-wayland-transfer",
        "native/fltk",
        "scripts/cmake-files-v2",
    ] {
        println!("cargo:rerun-if-changed={}", repo.join(path).display());
    }
    let configs = fs::read_dir(&build)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("fltk-sys-"))
        .map(|e| e.path().join("out/build/fltk"))
        .filter(|p| p.join("config.h").is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(";");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let status = std::process::Command::new("cmake")
        .arg(format!("-DFILES_REPO={}", repo.display()))
        .arg(format!("-DFILES_CONFIGS={configs}"))
        .arg(format!(
            "-DFILES_HEADER={}",
            out.join("files_fltk_dnd_build.h").display()
        ))
        .arg("-P")
        .arg(repo.join("native/fltk/guard-v1.cmake"))
        .status()
        .expect("cmake required to verify pinned DnD build inputs");
    assert!(status.success(), "Pinned FLTK DnD input guard failed");
    // The header names a STRONG content-addressed native symbol. A historical
    // build directory passing the source guard cannot silently validate the
    // wrong library: final linkage must resolve that exact compiled stamp.
    let mut compiler = cc::Build::new();
    compiler
        .cpp(true)
        .std("c++17")
        .file("native/window_gestures.cpp")
        .file("native/file_transfer.cpp")
        .include(&out)
        .include(&config)
        .include(&source);
    compiler.compile("files_window_gestures");
}
