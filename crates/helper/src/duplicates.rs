//! Un recorrido y un lector a la vez, con progreso limitado y un informe final acotado.
use files_core::duplicates::{self, Event, Frame};
use std::{
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn send(out: &mut impl Write, id: u64, event: Event) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(&Frame { id, event })?;
    if bytes.len() > 256 * 1024 {
        return Err(io::Error::other("Informe de duplicados superior a256KiB"));
    }
    bytes.push(b'\n');
    out.write_all(&bytes)?;
    out.flush()
}
pub fn run(id: u64, root: &Path, mut out: impl Write) -> io::Result<()> {
    let mut updates = 0;
    let mut phase = u8::MAX;
    let mut last = Instant::now();
    let report = duplicates::scan_all(root, &AtomicBool::new(false), |progress| {
        if updates < 512
            && (progress.phase != phase || last.elapsed() >= Duration::from_millis(250))
        {
            updates += 1;
            phase = progress.phase;
            last = Instant::now();
            send(&mut out, id, Event::Progress { progress })?;
        }
        Ok(())
    });
    match report {
        Ok(mut report) => {
            let mut start = 0;
            while start < report.rows.len() {
                let end = duplicates::page_end(&report.rows, start);
                if end == start {
                    return Err(io::Error::other("Página sin avance"));
                }
                send(
                    &mut out,
                    id,
                    Event::Page {
                        offset: start,
                        rows: report.rows[start..end].to_vec(),
                    },
                )?;
                start = end;
            }
            report.rows.clear();
            send(&mut out, id, Event::Done { report })
        }
        Err(error) => {
            let mut message = error.to_string();
            if message.len() > 256 {
                let mut end = 253;
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
                message.push('…');
            }
            send(&mut out, id, Event::Error { message })
        }
    }
}
