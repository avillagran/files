//! Read-only diagnostic of this process's Linux mount namespace. No path/source output.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open("/proc/self/mountinfo")?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let mounts = files_core::mount_table::parse(&bytes).map_err(std::io::Error::other)?;
    let args: Vec<_> = std::env::args_os().skip(1).take(4).collect();
    if args.len() == 3 && args[0] == "--point" {
        use std::os::unix::ffi::OsStrExt;
        let point = std::path::Path::new(&args[1]);
        if mounts.iter().all(|m| m.point != point) {
            println!("{}", serde_json::json!({"present":false}));
        } else {
            let m = files_core::mount_table::unique_sshfs(&mounts, point, &args[2])
                .map_err(std::io::Error::other)?;
            println!(
                "{}",
                serde_json::json!({"present":true,"id":m.id,"parent":m.parent,"device":m.device,"point":m.point.as_os_str().as_bytes(),"root":m.root.as_os_str().as_bytes(),"source":m.source.as_bytes()})
            );
        }
    } else if args.is_empty() {
        println!(
            "mountinfo: {} records parsed; read-only; no mounts changed",
            mounts.len()
        );
    } else {
        return Err(std::io::Error::other(
            "Usage: mount-table-probe [--point ABSOLUTE EXPECTED_SOURCE]",
        )
        .into());
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This read-only mountinfo diagnostic is Linux-only");
}
