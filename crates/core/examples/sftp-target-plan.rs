//! Inert unless --plan; emits bounded argv, does not spawn/connect/mount.
#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStrExt;
    let args: Vec<_> = std::env::args_os().skip(1).take(8).collect();
    if args.len() != 6 || args[0] != "--plan" {
        println!("Usage: sftp-target-plan --plan HOST USER PORT DIRECTORY MOUNTPOINT (argv only)");
        return Ok(());
    }
    let text = |i: usize| {
        args[i]
            .to_str()
            .ok_or(std::io::Error::other("Target fields require UTF8"))
    };
    let target = files_core::sftp_target::Target::parse(text(1)?, text(2)?, text(3)?, text(4)?)
        .map_err(std::io::Error::other)?;
    let argv = target
        .foreground_args(std::path::Path::new(&args[5]))
        .map_err(std::io::Error::other)?;
    println!(
        "{}",
        serde_json::json!({"source":target.source(),"argv":argv.iter().map(|s|s.as_bytes()).collect::<Vec<_>>()})
    );
    Ok(())
}
#[cfg(not(unix))]
fn main() {
    eprintln!("This argv diagnostic is Unix-only");
}
