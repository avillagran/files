# Actual synchronization metadata values

`directory_sync_metadata` acquires the values required by a resolved physical
blueprint. It streams one bounded value record at a time, checks selected hardlink
cohort compatibility and returns a non-decodable live acquisition receipt after
final verification. This is SOURCE ONLY, with no executor, operator approval,
owned durable metadata store, journal, staging/publication, recovery or UI/install.
C02 and the full explorer goal remain active; the installed dry-run is unchanged.

## Fresh acquisition

The sync-only Inventory extension reopens each raw path component from its pinned
parent. It uses O_PATH/NOFOLLOW, checks captured identity, full revision and mount
for every prefix and the current name, then opens regular/directory data with
NOATIME. Literal symlinks use the existing no-follow attribute reader. The whole
path is reopened again after reading attributes and root/parent/original-alias
bindings are checked before and after. Special types and changed/unowned objects
are rejected; no data read follows a symlink target.

The values include kind, permission bits, uid/gid, access/modified time and every
captured raw xattr name/value, including ACL/security/capability attributes.
Their recomputed full-value digest must match the original captured evidence.
This reuses the EXDEV attribute reader, without altering legacy capture/copy
behavior or its wire format. Per object:64 attributes,128KiB combined name+NUL
and value bytes,255-byte nonempty names, canonical strict raw order. Symlinks
inherit the existing canonical captured access-time policy: readlink can alter
atime, so later symlink atime alone is not a fresh-change detector. Regular and
directory access time must match captured evidence.

A complete final Plan.bind rehashes and verifies the original evidence after the
last consumer write, including the trailer. A sink that changes a source or
requests cancellation at the last callback cannot obtain Acquired. There is no
atomic snapshot, locking out writers, syscall timeout or lasting freshness proof.
Before effects the future worker must revalidate and manage its own changes.

## Recipes and real compatibility

Full recipes acquire the actual metadata of the chosen source or retained old
destination. Basic recipes acquire the old destination's complete attributes,
owner and access time, then overlay source permissions and non-directory mtime.
Kind-changing/new objects use full source values. Hardlink cohort members compare
complete resulting metadata digests, including actual attribute values, rather
than the old opaque hashes. Old hashes that differ can converge after Basic;
truly incompatible final values reject preparation and require a new explicit
choice/policy review. No group is silently split and no member wins arbitrarily.

Basic chmod-equivalent ACL adjustment preserves named principal entries and
changes owner, group mask (or group-object without a mask) and other permissions.
Linux UAPI ACL v2 header/tag/permission/id shape, ordering, required entries and
mask requirements are validated before modification; malformed or unsupported
ACL overlay fails before a complete receipt. Default ACLs are retained.
The semantics follow [acl(5)](https://man7.org/linux/man-pages/man5/acl.5.html),
“Correspondence between ACL entries and file permission bits”; binary layout is
verified against `/usr/include/linux/posix_acl_xattr.h` on NODE. No library or new
preview decoder is added.

Shared-directory changes emit two distinct records: complete old values for undo
and desired values. Basic desired-directory records explicitly carry an
AfterOwnChildren mtime policy; their stored old mtime is an audit baseline, not an
instruction to revert natural mtime changes made by child publication. Exact
source metadata uses literal captured times. Directory policy still needs an
own-effect ledger and checked application/rollback. Merely acquiring attributes
does not prove the filesystem permits reproducing them on a new staged object.

## Bounded stream and historical inspection

FLSMETv1 carries evidence, logical and physical seals, nonzero16-byte job ID,
stream byte budget, derived record count, ordered typed value records and a
SHA256 trailer over `Files:sync-metadata-values:v1\0` plus every preceding byte.
Stage records are followed by each directory's undo/desired pair. Individual
values are raw little-endian records, maximum131584B, with exact end validation;
no JSON escaping or lossy paths. Numeric indices refer to the original evidence.

The caller supplies a Write sink. Default stream allowance is64MiB, configurable
up to8GiB, with at most32770 records. This is a streamed artifact allowance, not
an in-memory buffer or an ordinary helper stdout response allowance. Header and
all records/trailer count against the budget; failure leaves partial/unapproved
output. The implementation retains only current bounded values/encoding and
bounded per-cohort hashes, never all attribute payloads. The caller can choose
to retain bytes, and must provide its own bound if it does so.

`inspect` streams historical data without opening original paths. It checks
complete header/seals/job/limits/count/order/flags, canonical value grammar,
full-value proof matches where available, Basic scalar/ACL consistency,
directory undo→desired recipe correspondence, cohort equality, trailer and EOF.
Visitor records remain provisional until the entire call succeeds. Even after
success they are historical claims, never permission to modify a filesystem.

In particular a Basic leaf's post-overlay user attribute claim cannot be proven
from its old opaque digest alone. A well-formed resealed forged history can pass
inspection but cannot match the live acquisition receipt's complete stream seal.
The seal is not authentication or provenance. StoredValues.matches only compares
job/seal/length/count to Acquired; it is not fresh evidence or proof of fsync.

Acquired borrows the physical plan and Prepared owners. It certifies actual
checked reads, compatible recipes and completion of Write calls at that point;
a sink may discard bytes. It does not prove the caller retained output, created
an owned file, verified readback, synced it or obtained operator approval. A
valid-looking complete stream may remain after a final sink mutation rejected
Acquired; such data is history only.

## Next stage and acceptance

Create an owned metadata/staging journal store with identity/readback/durable
ACK tied to job/evidence/logical/physical/value seals. Then explicit new visible
approval and live helper ownership/pins/ACK; own namespace/revision/nlink/ctime
ledger; global source staging before destination effects, recoverable no-clobber
preservation/publication, barriers, bounded queue/pause/cancel/close ACK and
crash/reopen/recovery/undo. Full untouched topology reconciliation still needs
explicit review. No unattended Apply on personal folders.

Final NODE tests, Clippy/fmt and installed-hash checks are recorded in
`evidence/directory-sync-metadata/assessment.json`. Fixtures cover actual raw
attributes, real Linux ACL/chmod correspondence, convergent/incompatible cohorts,
undo, symlinks, stale/foreign/cancel/sink/budget/corrupt history and full retained
inventories. Planning/acquisition tests are not stage application, fsync or crash
acceptance. PSS/Swap/CPU/startup remain frozen; new previews paused and native/
global gates open. Byte/count bounds are not resource measurements.

NODE final validation:1465 workspace tests (558core/112helper/795UI), three
historical ignored,15 new metadata tests, all-target Clippy-Dwarnings and fmt
PASS normal. Initial15 focused and first1465 full passed; first Clippy reported
four style lints (two manual modulo checks, fixed chunks API and nested if).
Equivalent standard API/conditional changes resolve them without suppression.
The final run also strengthens deep ancestor replacement and includes a real
non-UTF8 xattr name. No new release build or installation.

The subsequent SOURCE ONLY DIRECTORY-SYNC-STORE.md supplies the owned artifact,
complete saved evidence/choices/budget, readback and fresh file/namespace fsync
barriers. Acquired alone still makes none of those storage promises. The effect
journal, operator approval and actual destination changes remain pending.

Later source checkpoint: [stage metadata reproduction](DIRECTORY-SYNC-STAGE-METADATA.md)
now applies exact desired raw values to pinned staged objects and checks readback;
existing owned rename fixtures use it before the global staging barrier. This
adds no staged tree creator/native approval/installed Apply and explicitly rejects
symlink xattr changes when no safe descriptor mutation route is available.
