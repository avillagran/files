//! Private lifecycle lab, inert without --attempt. Holds provenance FDs until exit.
use std::io::{self, BufRead, Read, Write};
fn table() -> Result<Vec<files_core::mount_table::Mount>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open("/proc/self/mountinfo")?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    Ok(files_core::mount_table::parse(&bytes)?)
}
fn emit(value: serde_json::Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).take(3).collect();
    if args.len() != 2 || args[0] != "--attempt" {
        println!(
            "Usage: mount-point-probe --attempt PRIVATE_PARENT; commands empty/parent/exit; creates and retains one empty directory"
        );
        return Ok(());
    }
    let mut point =
        files_core::sftp_mount_point::Parent::open(std::path::Path::new(&args[1]))?.create()?;
    point.verify_unmounted_empty(&table()?)?;
    emit(serde_json::json!({"point":point.path().as_os_str().as_encoded_bytes()}))?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    for _ in 0..8 {
        let mut line = Vec::new();
        reader.by_ref().take(65).read_until(b'\n', &mut line)?;
        if line.len() > 64 || !line.ends_with(b"\n") {
            return Err("Invalid bounded command".into());
        }
        let result = match line.as_slice() {
            b"empty\n" => point.verify_unmounted_empty(&table()?),
            b"parent\n" => point.verify_parent(),
            b"exit\n" => {
                emit(serde_json::json!({"ok":true}))?;
                return Ok(());
            }
            _ => return Err("Unknown command".into()),
        };
        emit(match result {
            Ok(()) => serde_json::json!({"ok":true}),
            Err(e) => serde_json::json!({"ok":false,"error":e.to_string()}),
        })?;
    }
    Err("Command limit".into())
}
