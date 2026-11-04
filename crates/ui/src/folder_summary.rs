//! Cheap direct-child observations; deliberately no byte or recursive total.
use std::io;
pub const LIMIT: usize = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Complete,
    Limited,
    Partial,
    Unavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Summary {
    pub entries: usize,
    pub status: Status,
}
const _: () = assert!(std::mem::size_of::<Summary>() <= 32);
impl Summary {
    pub fn label(self) -> String {
        match self.status {
            Status::Complete => format!("{} directas · sin calcular", self.entries),
            Status::Limited => format!("≥{} directas · límite", self.entries),
            Status::Partial if self.entries == 0 => "Recuento incompleto".into(),
            Status::Partial => format!("≥{} directas · parcial", self.entries),
            Status::Unavailable => "Recuento no disponible".into(),
        }
    }
}
pub fn scan(
    entries: io::Result<impl Iterator<Item = io::Result<()>>>,
    cancelled: impl Fn() -> bool,
) -> Option<Summary> {
    if cancelled() {
        return None;
    }
    let Ok(mut entries) = entries else {
        return Some(Summary {
            entries: 0,
            status: Status::Unavailable,
        });
    };
    for count in 0..LIMIT {
        if cancelled() {
            return None;
        }
        match entries.next() {
            None => {
                return Some(Summary {
                    entries: count,
                    status: Status::Complete,
                });
            }
            Some(Err(_)) => {
                return Some(Summary {
                    entries: count,
                    status: Status::Partial,
                });
            }
            Some(Ok(())) => {}
        }
    }
    // No extra peek or false EOF claim: exactly LIMIT is also a lower bound.
    Some(Summary {
        entries: LIMIT,
        status: Status::Limited,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn empty_is_distinct_from_open_or_midstream_failure() {
        assert_eq!(
            scan(Ok(std::iter::empty()), || false).unwrap(),
            Summary {
                entries: 0,
                status: Status::Complete
            }
        );
        let no_access: io::Result<std::iter::Empty<io::Result<()>>> =
            Err(io::ErrorKind::PermissionDenied.into());
        assert_eq!(
            scan(no_access, || false).unwrap().status,
            Status::Unavailable
        );
        let rows = vec![Ok(()), Err(io::ErrorKind::PermissionDenied.into()), Ok(())];
        assert_eq!(
            scan(Ok(rows.into_iter()), || false).unwrap(),
            Summary {
                entries: 1,
                status: Status::Partial
            }
        );
    }
    #[test]
    fn arbitrarily_large_stream_is_not_exhausted_or_peeked() {
        let reads = Cell::new(0);
        let rows = std::iter::from_fn(|| {
            reads.set(reads.get() + 1);
            Some(Ok(()))
        });
        let observed = scan(Ok(rows), || false).unwrap();
        assert_eq!(reads.get(), LIMIT);
        assert_eq!(observed.status, Status::Limited);
        assert_eq!(observed.entries, LIMIT);
        assert!(observed.label().starts_with('≥'));
    }
    #[test]
    fn cancelled_generation_stops_before_next_entry_and_publishes_nothing() {
        let reads = Cell::new(0);
        let rows = std::iter::from_fn(|| {
            reads.set(reads.get() + 1);
            Some(Ok(()))
        });
        assert!(scan(Ok(rows), || reads.get() == 3).is_none());
        assert_eq!(reads.get(), 3);
    }
}
