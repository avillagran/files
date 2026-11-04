//! Explicit debug-only process lab; optional durable registration, no product UI.
#[cfg(all(debug_assertions, target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use files_core::{
        sftp_connection::{Connection, Snapshot},
        sftp_lifecycle::Phase as LivePhase,
        sftp_registry::{Phase, Process, Publication, Record, Store},
    };
    use std::io::{self, BufRead, Read, Write};
    type Journal = Option<(Store, Record)>;
    fn durable(store: &mut Store, record: &mut Record) -> Result<(), String> {
        match store.publish(record).map_err(|e| e.to_string())? {
            Publication::Durable => Ok(()),
            other => Err(format!("Registro no durable: {other:?}")),
        }
    }
    fn process(old: Option<Process>, pid: Option<u32>) -> Result<Option<Process>, String> {
        pid.map(|pid| {
            if old.is_some_and(|p| p.pid == pid) {
                Ok(old.unwrap())
            } else {
                Process::capture(pid).map_err(|e| e.to_string())
            }
        })
        .transpose()
    }
    fn persist(
        connection: &Connection,
        snapshot: &Snapshot,
        journal: &mut Journal,
    ) -> Result<(), String> {
        let Some((store, record)) = journal else {
            return Ok(());
        };
        let server = if snapshot.phase != LivePhase::Absent && snapshot.server_pid.is_none() {
            record.server
        } else {
            process(record.server, snapshot.server_pid)?
        };
        let unmount = process(record.unmount, snapshot.unmount_pid)?;
        let mount = if snapshot.phase == LivePhase::Absent {
            None
        } else {
            connection.observed_mount().cloned()
        };
        let phase = match snapshot.phase {
            LivePhase::Ready => Phase::Verified,
            LivePhase::Absent => Phase::Absent,
            LivePhase::Disconnecting if mount.is_some() => Phase::Disconnecting,
            LivePhase::Verifying if server.is_some() => Phase::Spawned,
            _ => Phase::Uncertain,
        };
        if record.phase != phase
            || record.server != server
            || record.unmount != unmount
            || record.mount != mount
        {
            record.phase = phase;
            record.server = server;
            record.unmount = unmount;
            record.mount = mount;
            durable(store, record)?;
        }
        Ok(())
    }
    fn intent(journal: &mut Journal, phase: Phase) -> Result<(), String> {
        if let Some((store, record)) = journal {
            record.phase = phase;
            durable(store, record)?;
        }
        Ok(())
    }
    fn emit(snapshot: Snapshot, error: Option<String>, journal: &Journal) -> io::Result<()> {
        println!(
            "{}",
            serde_json::json!({"phase":format!("{:?}",snapshot.phase),"point":snapshot.point.as_os_str().as_encoded_bytes(),"diagnostic":snapshot.diagnostic,"server_pid":snapshot.server_pid,"unmount_pid":snapshot.unmount_pid,"request_error":error,"registry_generation":journal.as_ref().map(|(_,r)|r.generation()),"registry_phase":journal.as_ref().map(|(_,r)|format!("{:?}",r.phase))})
        );
        io::stdout().flush()
    }
    let args: Vec<_> = std::env::args_os().skip(1).take(9).collect();
    let registered = args.len() == 8 && args[7] == "--registry";
    if !(args.len() == 7 || registered) || args[0] != "--attempt" {
        println!(
            "Usage: sftp-connection-probe --attempt PRIVATE_PARENT SSHFS UNMOUNT HOST DIRECTORY CONFIG [--registry]; commands poll/cancel/disconnect/exit"
        );
        return Ok(());
    }
    let target = files_core::sftp_target::Target::parse(
        args[4].to_str().ok_or("Host UTF8")?,
        "",
        "",
        args[5].to_str().ok_or("Directory UTF8")?,
    )?;
    let root = std::path::Path::new(&args[1]);
    let store = if registered {
        let (store, existing) = Store::open(root)?;
        if existing.is_some() || !store.writable() {
            return Err("Lab requires empty writable registry".into());
        }
        Some(store)
    } else {
        None
    };
    let point = files_core::sftp_mount_point::Parent::open(root)?.create()?;
    let mut journal = if let Some(mut store) = store {
        let mut record = Record::prepared(target.clone(), &point);
        durable(&mut store, &mut record)?;
        Some((store, record))
    } else {
        None
    };
    let mut connection = Connection::start_lab(
        target,
        point,
        std::path::Path::new(&args[2]),
        std::path::Path::new(&args[3]),
        std::path::Path::new(&args[6]),
    )?;
    let snapshot = connection.snapshot();
    let error = persist(&connection, &snapshot, &mut journal).err();
    emit(snapshot, error, &journal)?;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    for _ in 0..1024 {
        let mut line = Vec::new();
        reader.by_ref().take(65).read_until(b'\n', &mut line)?;
        if line.len() > 64 || !line.ends_with(b"\n") {
            return Err("Invalid bounded command".into());
        }
        let error = match line.as_slice() {
            b"poll\n" => None,
            b"cancel\n" => match intent(&mut journal, Phase::Uncertain) {
                Ok(()) => {
                    connection.cancel();
                    None
                }
                Err(e) => Some(e),
            },
            b"disconnect\n" => match intent(&mut journal, Phase::Disconnecting) {
                Ok(()) => connection.disconnect().err().map(|e| e.to_string()),
                Err(e) => Some(e),
            },
            b"exit\n" => {
                emit(connection.snapshot(), None, &journal)?;
                return Ok(());
            }
            _ => return Err("Unknown command".into()),
        };
        let snapshot = connection.poll();
        let persisted = persist(&connection, &snapshot, &mut journal).err();
        emit(snapshot, error.or(persisted), &journal)?;
    }
    Err("Command limit".into())
}
#[cfg(not(all(debug_assertions, target_os = "linux")))]
fn main() {
    println!("Linux debug laboratory only; inert elsewhere");
}
