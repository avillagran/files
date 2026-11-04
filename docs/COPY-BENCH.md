# Copy comparison

Serialized smoke16MiB/1000files/one repetition and a64MiB/1000files/three
repetition series passed contents/SHA256/mode/mtime/source-integrity checks on
2026-10-26. These are isolated core timings, not app memory acceptance.

```sh
cargo build --release -p files-core --example copy_bench
python3 scripts/copy-bench.py --mib 16 --small-files 1000 --repetitions 1
```

The normal comparison defaults to 64 MiB, 1000 small files and three repetitions.
Each scenario rotates these modes: engine auto, cp reflink-auto, engine stream,
and cp reflink-never/sparse-never. The latter may still use kernel copy_file_range;
it is not claimed to be a userspace-only cp implementation.

Fixtures are created exclusively below ~/.cache/files-test and deleted after use.
SHA256, regular-file sizes, rwx permissions and modification timestamps are
compared after timing, including verification that the source is unchanged.
Directory root mode/mtime are checked too. No personal files are used.

External duration includes process startup; the example also reports engine-only
duration and actual transfer-method counts. Reflink performance represents logical
cloning, not transfer of all logical bytes. These are warm-cache runs without
fsync, not physical-storage throughput or durable-completion latency. Times from
one small smoke run must not be used to claim general performance superiority.

Results go to docs/evidence/copy/throughput.json. The instrument deliberately
records resource_gate=NOT_MEASURED: PSS+Swap for the app and operation worker must
be measured in the separate integrated scenario. Do not run alongside builds,
UI capture or other benchmarks.

Series evidence: evidence/copy/throughput-1793025220744794064.json; initial smoke
throughput.json remains intact. External medians in milliseconds (three runs):

| Fixture | engine auto | cp auto | engine stream | cp no-reflink |
|---|---:|---:|---:|---:|
|64MiB file|16.357|13.064|25.982|29.838|
|1000small files|23.611|40.381|53.117|53.413|

Variance is large (bulk cp-auto5.926–27.458ms; engine-auto11.418–19.796ms).
No general speed advantage is claimed. Auto used reflink for every regular file;
stream used the stream fallback. Kernel-copy fallback is unit tested, not exercised
by this measured filesystem's auto path. All24series rows passed integrity checks.
Full process/worker resources are separate in evidence/0.0.4/copy-resources.json;
resource_gate here deliberately stays NOT_MEASURED. The UI baseline still FAILs.
