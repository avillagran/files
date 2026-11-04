//! Inspect explicit private journal; may create/acquire its lock, never adopt or spawn transport.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let args: Vec<_> = std::env::args_os().skip(1).take(3).collect();
    if args.len() != 2 || args[0] != "--inspect" {
        println!(
            "Usage: sftp-registry-probe --inspect PRIVATE_ROOT; may create/acquire advisory lock, never adopts/unmounts"
        );
        return Ok(());
    }
    let (store, record) = files_core::sftp_registry::Store::open(std::path::Path::new(&args[1]))?;
    let mut table = Vec::new();
    std::fs::File::open("/proc/self/mountinfo")?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut table)?;
    let table = files_core::mount_table::parse(&table)?;
    let observation = record
        .as_ref()
        .map(|r| store.observe(r, &table))
        .transpose()?;
    println!(
        "{}",
        serde_json::json!({"writable":store.writable(),"phase":record.as_ref().map(|r|format!("{:?}",r.phase)),"generation":record.as_ref().map(|r|r.generation()),"server":record.as_ref().and_then(|r|r.server).map(|p|(p.pid,p.start_ticks)),"recovery":observation.map(|v|format!("{v:?}")),"point":record.as_ref().map(|r|r.point.as_os_str().as_encoded_bytes()),"port":record.as_ref().and_then(|r|r.target.port())})
    );
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    println!("Linux laboratory only");
}
