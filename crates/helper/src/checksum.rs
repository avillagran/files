//! Un lote, archivos secuenciales y progreso acotado; la cancelación del padre mata/recolecta el helper.
use files_core::checksum::{self, Event, Frame, Name};
use std::{
    io::{self, Write},
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
fn send(out: &mut impl Write, id: u64, event: Event) -> io::Result<()> {
    serde_json::to_writer(&mut *out, &Frame { id, event })?;
    out.write_all(b"\n")?;
    out.flush()
}
fn short(mut value: String) -> String {
    if value.len() > 256 {
        let mut end = 253;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
        value.push('…');
    }
    value
}
pub fn run(id: u64, root: &Path, names: &[Name], mut out: impl Write) -> io::Result<()> {
    checksum::validate(root, names)?;
    let cancel = AtomicBool::new(false);
    let mut root_identity = None;
    let mut extras = 0;
    for (index, name) in names.iter().enumerate() {
        let mut last = Instant::now();
        let result =
            checksum::calculate(root, &name.path, root_identity, &cancel, |bytes, total| {
                if bytes == 0 || (extras < 256 && last.elapsed() >= Duration::from_millis(250)) {
                    if bytes > 0 {
                        extras += 1;
                    }
                    send(
                        &mut out,
                        id,
                        Event::Progress {
                            index,
                            bytes,
                            total,
                        },
                    )?;
                    last = Instant::now();
                }
                Ok(())
            });
        if let Ok(observation) = &result {
            root_identity = Some(observation.proof.root);
        }
        send(
            &mut out,
            id,
            Event::Item {
                index,
                result: result.map_err(|e| short(e.to_string())),
            },
        )?;
    }
    send(&mut out, id, Event::Done { count: names.len() })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_batch_has_no_output_and_no_file_io() {
        let mut out = Vec::new();
        assert!(run(9, Path::new("/does-not-exist"), &[], &mut out).is_err());
        assert!(out.is_empty());
    }
}
