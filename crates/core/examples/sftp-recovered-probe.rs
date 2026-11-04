//! Explicit Linux recovered-control lab, inert without --attempt.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, BufRead, Read, Write};
    let args: Vec<_> = std::env::args_os().skip(1).take(4).collect();
    if args.len() != 3 || args[0] != "--attempt" {
        println!(
            "Usage: sftp-recovered-probe --attempt PRIVATE_ROOT UNMOUNT; commands poll/disconnect/reconcile/sync/exit"
        );
        return Ok(());
    }
    let mut controller = files_core::sftp_recovered::Controller::open(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
    )?;
    fn emit(s: files_core::sftp_recovered::Snapshot, result: Option<String>) -> io::Result<()> {
        println!(
            "{}",
            serde_json::json!({"phase":format!("{:?}",s.phase),"point":s.point.as_os_str().as_encoded_bytes(),"writable":s.writable,"pending_pid":s.pending_pid,"generation":s.generation,"saved_phase":format!("{:?}",s.saved_phase),"diagnostic":s.diagnostic,"result":result})
        );
        io::stdout().flush()
    }
    emit(controller.snapshot(), None)?;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    for _ in 0..1024 {
        let mut line = Vec::new();
        input.by_ref().take(65).read_until(b'\n', &mut line)?;
        if line.len() > 64 || !line.ends_with(b"\n") {
            return Err("Invalid bounded command".into());
        }
        let result = match line.as_slice() {
            b"poll\n" => None,
            b"disconnect\n" => Some(format!("{:?}", controller.disconnect())),
            b"reconcile\n" => Some(format!("{:?}", controller.reconcile())),
            b"sync\n" => Some(format!("{:?}", controller.retry_sync())),
            b"exit\n" => {
                emit(controller.snapshot(), None)?;
                return Ok(());
            }
            _ => return Err("Unknown command".into()),
        };
        emit(controller.poll(), result)?;
    }
    Err("Command limit".into())
}
#[cfg(not(target_os = "linux"))]
fn main() {
    println!("Linux laboratory only");
}
