//! Inert by default; explicit owned-fixture filesystem execution contract probe.
//! No resource sampling/daemon/desktop input. Not a product UI or installed tool.
#[cfg(target_os = "linux")]
fn main() {
    use files_core::{archive_extract::Budget, archive_extract_fs::Prepared, copy_engine::Control};
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::Path};
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() == 1 || args.get(1).is_some_and(|v| v == "--help") {
        println!(
            "Usage: archive-extract-probe --prepare|--extract SOURCE DEST_PARENT NEW_NAME [--umask-0777] [--cancel-after-progress]"
        );
        return;
    }
    let mode = args[1].to_str();
    if !matches!(mode, Some("--prepare" | "--extract"))
        || args.len() < 5
        || args[5..]
            .iter()
            .any(|v| v != "--umask-0777" && v != "--cancel-after-progress")
    {
        eprintln!("Explicit --prepare|--extract and three paths required");
        std::process::exit(2);
    }
    let control = Control::default();
    let prepared = match Prepared::capture(
        Path::new(&args[2]),
        Path::new(&args[3]),
        OsStr::new(&args[4]),
        Budget::default(),
        &control,
    ) {
        Ok(p) => p,
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({"ok":false,"phase":"prepare","error":e.to_string(),"resource_gate":"NOT_MEASURED"})
            );
            std::process::exit(1);
        }
    };
    if mode == Some("--prepare") {
        println!(
            "{}",
            serde_json::json!({"ok":true,"phase":"prepared_readonly","files":prepared.plan().files,"directories":prepared.plan().directories,"declared_bytes":prepared.plan().bytes,"retained_fds":prepared.pinned_fds(),"accounted_retained_bytes":prepared.retained_bytes(),"auxiliary_reserve":prepared.execution_auxiliary_reserve(),"resource_gate":"NOT_MEASURED"})
        );
        return;
    }
    if args[5..].iter().any(|v| v == "--umask-0777") {
        unsafe {
            libc::umask(0o777);
        }
    }
    let cancel = args[5..].iter().any(|v| v == "--cancel-after-progress");
    let result = prepared.execute(&control, |p| {
        if cancel && p.bytes > 0 {
            control.cancel()
        }
    });
    match result {
        Ok(r) => println!(
            "{}",
            serde_json::json!({"ok":true,"phase":"published_synced","files":r.files,"directories":r.directories,"bytes":r.bytes,"destination_raw":r.destination.as_os_str().as_bytes(),"resource_gate":"NOT_MEASURED"})
        ),
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({"ok":false,"phase":"execute","error":e.cause.to_string(),"publication":format!("{:?}",e.publication),"cleanup":format!("{:?}",e.cleanup),"rescue_raw":e.rescue.as_ref().map(|p|p.as_os_str().as_bytes()),"candidate_raw":e.candidate.as_ref().map(|p|p.as_os_str().as_bytes()),"provisional_bytes":e.provisional_bytes,"cleanup_error":e.cleanup_error,"can_retry_sync":e.can_retry_sync(),"can_retry_cleanup":e.can_retry_cleanup(),"resource_gate":"NOT_MEASURED"})
            );
            std::process::exit(1);
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    println!("Archive extraction execution is not implemented on this platform");
}
