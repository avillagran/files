//! Complete read-only ZIP32 namespace/local-record plan. No destination IO,
//! payload integrity proof, filesystem authority, extraction or publication yet.
use crate::{
    archive_list::{self, Listing},
    archive_member::{self, Limits, Span},
};
use std::{
    collections::BTreeMap,
    io::{self, Read, Seek},
    sync::atomic::{AtomicBool, Ordering},
};
pub const MAX_OUTPUT: u64 = 1024 * 1024 * 1024;
pub const MAX_COMPONENTS: usize = 64;
pub const MAX_NODES: usize = 20000;
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub total: u64,
    pub member: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            total: MAX_OUTPUT,
            member: MAX_OUTPUT,
        }
    }
}
#[derive(Debug)]
pub struct Plan {
    listing: Listing,
    spans: Vec<Span>,
    pub bytes: u64,
    pub files: usize,
    pub directories: usize,
}
impl Plan {
    pub fn listing(&self) -> &Listing {
        &self.listing
    }
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }
    /// Conservative retained allocation accounting, not process PSS or FD memory.
    pub fn retained_bytes(&self) -> Option<usize> {
        let entries = &self.listing.entries;
        let mut size = std::mem::size_of::<Self>()
            .checked_add(
                entries
                    .capacity()
                    .checked_mul(std::mem::size_of::<crate::archive_list::Entry>())?,
            )?
            .checked_add(
                self.spans
                    .capacity()
                    .checked_mul(std::mem::size_of::<Span>())?,
            )?;
        for e in entries {
            size = size.checked_add(e.name.capacity())?.checked_add(64)?;
        }
        size.checked_add(128)
    }
    pub fn into_listing(self) -> Listing {
        self.listing
    }
}
#[derive(Clone, Copy)]
enum Node {
    File,
    Directory { explicit: bool },
}
fn invalid(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Plan ZIP cancelado",
        ))
    } else {
        Ok(())
    }
}
/// Validates the whole archive's names/types/quotas/local metadata before caller
/// creates even its staging root. Payload CRCs remain the execution phase's duty.
pub fn prepare(
    input: &mut (impl Read + Seek),
    budget: Budget,
    cancel: &AtomicBool,
) -> io::Result<Plan> {
    if budget.total > MAX_OUTPUT || budget.member > MAX_OUTPUT {
        return Err(invalid("Presupuesto ZIP supera el máximo de1GiB"));
    }
    check(cancel)?;
    let listing = archive_list::read(input)?;
    check(cancel)?;
    let limits = Limits {
        compressed: u64::from(u32::MAX - 1),
        output: budget.member,
    };
    let mut bytes = 0u64;
    let (mut files, mut directories) = (0, 0);
    {
        // Prefix keys borrow the one retained raw-name payload, not cloned paths.
        let mut nodes = BTreeMap::<&[u8], Node>::new();
        for e in &listing.entries {
            check(cancel)?;
            archive_member::validate(e, limits)?;
            bytes = bytes
                .checked_add(u64::from(e.uncompressed_bytes))
                .filter(|&n| n <= budget.total)
                .ok_or_else(|| invalid("Expansión total ZIP fuera del presupuesto"))?;
            let name = if e.directory {
                e.name.strip_suffix(b"/").unwrap_or(&e.name)
            } else {
                &e.name
            };
            let mut components = 0usize;
            for part in name.split(|&b| b == b'/') {
                components += 1;
                if part.is_empty()
                    || part == b"."
                    || part == b".."
                    || part.len() > 255
                    || components > MAX_COMPONENTS
                {
                    return Err(invalid(
                        "Ruta ZIP vacía, dot, componente largo o profundidad fuera de límite",
                    ));
                }
            }
            if name.is_empty() {
                return Err(invalid("Ruta ZIP vacía"));
            }
            for end in name
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| (b == b'/').then_some(i))
            {
                let prefix = &name[..end];
                match nodes.get(prefix) {
                    Some(Node::File) => {
                        return Err(invalid("Archivo ZIP también es ancestro de otra ruta"));
                    }
                    Some(Node::Directory { .. }) => {}
                    None => {
                        if nodes.len() == MAX_NODES {
                            return Err(invalid("ZIP supera20000 nodos de destino"));
                        }
                        nodes.insert(prefix, Node::Directory { explicit: false });
                    }
                }
            }
            match (nodes.get_mut(name), e.directory) {
                (Some(Node::Directory { explicit }), true) if !*explicit => {
                    *explicit = true;
                }
                (Some(_), _) => return Err(invalid("Colisión de nombres/tipos ZIP en el destino")),
                (None, directory) => {
                    if nodes.len() == MAX_NODES {
                        return Err(invalid("ZIP supera20000 nodos de destino"));
                    }
                    nodes.insert(
                        name,
                        if directory {
                            Node::Directory { explicit: true }
                        } else {
                            Node::File
                        },
                    );
                }
            }
        }
        for node in nodes.values() {
            match node {
                Node::File => files += 1,
                Node::Directory { .. } => directories += 1,
            }
        }
    }
    let mut spans = Vec::with_capacity(listing.entries.len());
    for index in 0..listing.entries.len() {
        check(cancel)?;
        spans.push(archive_member::inspect_cancellable(
            input, &listing, index, limits, cancel,
        )?);
    }
    // Sort a compact index, not the authoritative central-entry ordering.
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_unstable_by_key(|&i| spans[i].local_start);
    for pair in order.windows(2) {
        if spans[pair[1]].local_start < spans[pair[0]].record_end {
            return Err(invalid("Registros locales ZIP se solapan"));
        }
    }
    check(cancel)?;
    Ok(Plan {
        listing,
        spans,
        bytes,
        files,
        directories,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive_member::fixtures::{Spec, zip};
    use std::io::Cursor;
    fn plan(members: &[Spec<'_>]) -> io::Result<Plan> {
        prepare(
            &mut Cursor::new(zip(members)),
            Budget::default(),
            &AtomicBool::new(false),
        )
    }
    fn dir(name: &[u8]) -> Spec<'_> {
        let mut e = Spec::file(name, b"");
        e.mode = 0o040700;
        e.method = 0;
        e
    }
    #[test]
    fn raw_names_implicit_directories_and_out_of_order_explicit_directory_are_preserved() {
        let members = [
            Spec::file(b"a/opaque-\xff", b"one"),
            dir(b"a/"),
            Spec::file("ñ".as_bytes(), b"two"),
        ];
        let p = plan(&members).unwrap();
        assert_eq!((p.files, p.directories, p.bytes), (2, 1, 6));
        assert_eq!(p.listing.entries[0].name, b"a/opaque-\xff");
        assert_eq!(p.spans.len(), 3);
    }
    #[test]
    fn lexical_escape_empty_dot_drive_backslash_control_and_utf8_lies_are_rejected() {
        for name in [
            &b"../outside"[..],
            b"/absolute",
            b"C:/drive",
            b"a\\b",
            b"a//b",
            b"a/./b",
            b".",
            b"a/../b",
            b"control\n",
        ] {
            assert!(plan(&[Spec::file(name, b"x")]).is_err(), "{name:?}");
        }
        let mut e = Spec::file(b"raw-\xff", b"x");
        e.flags = 0x800;
        assert!(plan(&[e]).is_err());
    }
    #[test]
    fn duplicate_canonical_directory_and_file_parent_aliases_fail_in_any_order() {
        assert!(plan(&[dir(b"a/"), dir(b"a")]).is_err());
        assert!(plan(&[Spec::file(b"a", b"x"), Spec::file(b"a/b", b"y")]).is_err());
        assert!(plan(&[Spec::file(b"a/b", b"y"), Spec::file(b"a", b"x")]).is_err());
        assert!(plan(&[Spec::file(b"a", b"x"), dir(b"a/")]).is_err());
        assert!(plan(&[Spec::file(b"a", b"x"), Spec::file(b"a", b"x")]).is_err());
    }
    #[test]
    fn links_special_types_nonempty_directory_unsupported_flags_and_method_fail() {
        for mode in [0o120777, 0o010600, 0o020600, 0o060600, 0o140600] {
            let mut e = Spec::file(b"node", b"x");
            e.mode = mode;
            assert!(plan(&[e]).is_err());
        }
        let mut e = dir(b"a/");
        e.data = b"not empty";
        assert!(plan(&[e]).is_err());
        for flags in [1, 0x40, 0x20] {
            let mut e = Spec::file(b"flags", b"x");
            e.flags = flags;
            assert!(plan(&[e]).is_err());
        }
        let mut e = Spec::file(b"method", b"x");
        e.method = 12;
        assert!(plan(&[e]).is_err());
    }
    #[test]
    fn declared_quota_and_zero_output_empty_tree_are_checked_without_destination() {
        let bytes = zip(&[Spec::file(b"a", b"123"), Spec::file(b"b", b"456")]);
        assert!(
            prepare(
                &mut Cursor::new(&bytes),
                Budget {
                    total: 5,
                    member: 3
                },
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert!(
            prepare(
                &mut Cursor::new(&bytes),
                Budget {
                    total: 6,
                    member: 2
                },
                &AtomicBool::new(false)
            )
            .is_err()
        );
        let bytes = zip(&[dir(b"a/"), Spec::file(b"a/empty", b"")]);
        let p = prepare(
            &mut Cursor::new(bytes),
            Budget {
                total: 0,
                member: 0,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(p.bytes, 0);
    }
    #[test]
    fn max_components_and_component_bytes_are_rejected_before_output() {
        let deep = std::iter::repeat_n("a", 65).collect::<Vec<_>>().join("/");
        assert!(plan(&[Spec::file(deep.as_bytes(), b"")]).is_err());
        let long = vec![b'x'; 256];
        assert!(plan(&[Spec::file(&long, b"")]).is_err());
        let accepted = std::iter::repeat_n("a", 64).collect::<Vec<_>>().join("/");
        assert!(plan(&[Spec::file(accepted.as_bytes(), b"")]).is_ok());
    }
    #[test]
    fn overlapping_valid_local_records_are_rejected_not_just_bad_names() {
        let mut embedded = vec![0; 30];
        embedded[..4].copy_from_slice(b"PK\x03\x04");
        embedded[4..6].copy_from_slice(&20u16.to_le_bytes());
        embedded[26..28].copy_from_slice(&1u16.to_le_bytes());
        embedded.push(b'b');
        let mut payload = b"pre".to_vec();
        payload.extend_from_slice(&embedded);
        payload.extend_from_slice(b"post");
        let mut a = Spec::file(b"a", &payload);
        a.method = 0;
        let mut b = Spec::file(b"b", b"");
        b.method = 0;
        let mut bytes = zip(&[a, b]);
        let central =
            u32::from_le_bytes(bytes[bytes.len() - 6..bytes.len() - 2].try_into().unwrap())
                as usize;
        let second = central + 47;
        bytes[second + 42..second + 46].copy_from_slice(&34u32.to_le_bytes());
        let error = prepare(
            &mut Cursor::new(bytes),
            Budget::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.to_string().contains("solapan"), "{error}");
    }
    #[test]
    fn local_name_size_flags_and_descriptor_contradictions_are_preflight_errors() {
        for at in [6usize, 8, 14, 18, 22, 30] {
            let mut bytes = zip(&[Spec::file(b"a", b"x")]);
            bytes[at] ^= 1;
            assert!(
                prepare(
                    &mut Cursor::new(bytes),
                    Budget::default(),
                    &AtomicBool::new(false)
                )
                .is_err(),
                "byte{at}"
            );
        }
        let mut e = Spec::file(b"a", b"x");
        e.descriptor = 2;
        let mut bytes = zip(&[e]);
        let central =
            u32::from_le_bytes(bytes[bytes.len() - 6..bytes.len() - 2].try_into().unwrap())
                as usize;
        bytes[central - 1] ^= 1;
        assert!(
            prepare(
                &mut Cursor::new(bytes),
                Budget::default(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
    }
    #[test]
    fn node_metadata_budget_is_independent_of_entry_count_and_raw_name_bytes() {
        let names: Vec<_> = (0..7000)
            .map(|i| format!("r{i}/a/b").into_bytes())
            .collect();
        let members: Vec<_> = names
            .iter()
            .map(|name| {
                let mut e = Spec::file(name, b"");
                e.method = 0;
                e
            })
            .collect();
        let error = plan(&members).unwrap_err();
        assert!(error.to_string().contains("20000"), "{error}");
    }
    #[test]
    fn cancellation_and_empty_archive_are_explicit() {
        let bytes = zip(&[]);
        let error = prepare(
            &mut Cursor::new(&bytes),
            Budget::default(),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        let p = prepare(
            &mut Cursor::new(bytes),
            Budget::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!((p.files, p.directories, p.bytes), (0, 0, 0));
    }
}
