# Linux process-tree sampler

`TreeSampler(pid)` retains the existing, default `discovery='scan'`: enumerate
`/proc`, read process identities, then follow parent relationships. Historical
reports are unchanged. The optional API is:

```python
sampler = TreeSampler(pid, discovery='children')
row = sampler.sample()
```

`children` enumerates `/proc/PID/task/` for **every thread**, reads each TID's
`stat` and `children`, and recursively visits discovered processes. Already
observed `(PID, starttime)` identities remain in scope after reparenting. It
avoids reading unrelated processes' `stat` files; it does not change memory,
CPU, descriptor or thread accounting. CPU remains an observed interval value,
not a whole-lifetime total. No speed or app-budget claim follows from unit tests.

The containing process identity is checked before enumeration and again after
all its thread reads, before newly listed child PID files are opened. Each TID
is checked before and after its children read. Live/unverifiable errors in
process stat, task enumeration, task stat or children invalidate the sample.
Permission/parse/I/O failures are never accepted as an empty children list. A
listed child whose first stat fails is also an error, even if never previously
observed. Such unresolved discoveries are retained until a current relationship
can be verified or the PID directory is confirmed gone; reparenting cannot
silently remove that uncertainty. PID reuse cannot count an unrelated new
process as the previously observed member.

Both modes still use the same PSS + SwapPss and PID/starttime verification while
reading memory and descriptors. `complete` means the surviving, readable
identities and the selected discovery reads were verifiable. It **does not mean
whole-lifetime coverage**. A confirmed exit with unread memory is an explicit
hole, not zero memory. `exited_during_read` records process holes; `start` may be
`None` when a newly listed process vanished before its first identity read.
`discovery_events` separately records vanished/reused TIDs with PID, process
starttime, TID/task starttime when available, phase, reason and
`memory_available=false`. Consumers must retain both event arrays and warnings
when aggregating a report. The `discovery` field states the selected algorithm.

Neither mode makes a simultaneous snapshot. Tasks/children can appear and exit
between enumeration and reads, and short-lived processes can escape a sample
entirely. A departed task with an unread children list is a discovery lifetime
gap even when the remaining reads validate. Peaks remain sampled lower bounds.
A fast requested interval is not a guarantee: report actual sample intervals,
all known helper identities and any identities never sampled. The children
mode does not enumerate unrelated/reparented processes that were never seen
under an observed owner. It requires Linux task/children support and sufficient
procfs permissions. Missing/denied task data is explicit `complete=false`.

There is no hard sample-time bound: procfs enumeration and nested stat/children,
smaps and fd syscalls are synchronous. More threads, descendants or retained
identities increase work; a kernel/procfs stall can delay the caller. Discovery
has no background thread or intentional blind interval. GPU memory remains
outside this sampler and must be reported separately.

Synthetic tests (no app, decoder, Cargo or resource benchmark):

```sh
python3 -m unittest discover -s scripts -p test_process_metrics.py
```

The legacy scan tests run unchanged, and the same contract is exercised with
children discovery. Added fixtures cover nonleader-thread children, recursive
and reparented descendants, PID/TID reuse, unknown unreadable children,
unresolved identities after reparenting/parent exit, task disappearance, live
permission/parse/ENOENT/ESRCH errors, parent identity changing during discovery,
empty task enumeration and no global `/proc` enumeration. This validates the
instrument's contract, not helper lifetime coverage or files' memory budget.
