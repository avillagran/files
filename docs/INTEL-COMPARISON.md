# Files and Flea 0.4 on the older Intel

2026-11-04. User explicitly authorized direct SSH to intel@192.168.1.70.
The previously pending T3 link was not needed. No target installation or default
association was changed. Same optimized Files sourceff7af56 and exact native
x86_64 binaries as the prior NODE run, plus official Flea0.4.0, were unpacked
under /home/intel/.cache/files-flea-comparison-20261104.

The user's clarification is authoritative: **50MB is an internal objective,
not a project rejection threshold.** The memory reduction is valuable. Earlier
numerical gates remain historical observations, not a negative product verdict.

## Hardware and protocol

- mbp-intel: Corei5-5257U2.70GHz,2cores/4threads, about8GB RAM.
- Intel Iris6100(BroadwellGT3),i915. Every Flea launch confirms hardware Vulkan
  on this GPU, not the M2 llvmpipe software path.
- Omarchy, nativeWayland/Hyprland, internal2560x1600 outputscale2. Every timed
  client1256x750logical=2512x1500physical, identical geometry/position for both
  apps in all16runs. Default row density/styles differ.
- Four runs per app/count, alternatingAB/BA;10k and100k empty.txt files with
  identical names,name-sortedList, fresh processes/warm directorymetadata.
  Private HOME/XDG profiles; no global cache drop, target builds, screenshots
  or other own benchmarks during measurements. Background apps untouched.
- Process-tree PSS+SwapPss with observed helpers; sampled peaks lower bounds,
  short-lived children can escape. Different app readiness observers and polling
  of tens of ms; not keyboard/scroll/presentation latency. Mapped is separate.
- Initial Flea run had slower startup/cache initialization; all valid runs remain
  in results. No failures or sampler errors in this campaign. Own groups only
  terminated; retained identities/windows verified exited. No orderly-close claim.

## Results

Medianas; MiB means1024²bytes. The RAM column is stable process-tree memory,
not a resident GPU total.

| Files | App | Listing-state ready | Stable tree RAM | Sampled peak RAM |
|---|---|---:|---:|---:|
|10.000|Files|159ms|39.0MiB|39.0MiB|
|10.000|Flea0.4|279ms|56.4MiB|57.1MiB|
|100.000|Files|1.420ms|51.8MiB|51.8MiB|
|100.000|Flea0.4|271ms|58.8MiB|59.6MiB|

Files uses≈31%/12% less stable tree RAM in these two scenarios. It completes the
reported listing state sooner at10k. At100k it takes≈5.2times as long as Flea;
Files1.414–1.429s vsFlea0.267–0.339s, well beyond polling granularity.
This exposes a real large-list weakness on this machine/display configuration;
it does not identify whether sorting, painting or another stage dominates.

## GPU accounting

Flea's i915 fdinfo reports79.796875MiB resident across unique DRM clients at the
end of each run, consistent across both folder sizes. Duplicate FDs are
coalesced by PCIdevice/clientID, taking the maximum observation per client;
system/stolen resident regions are then combined. This is an end observation,
not a GPU lifetime peak. Files has no own DRM client in these observations;
its compositor-side graphics cost is outside the measured process tree.

**Do not blindly add these resident GPU bytes to PSS:** driver mapping and
accounting may overlap. Raw fdinfo and aggregation are retained. The additional
GPU observation provides useful memory context without inventing a combined
physical-memory total or treating the compositor as free for Files.

## Interpretation

The memory benefit is real and should not be rejected for missing an arbitrary
50MB target. This older Intel adds another useful result:10k favors Files on
both metrics,100k favors Files on process RAM but Flea on listing latency.
There is no universal winner. NODE's100k win and M2's close timing remain separate
experiments: different geometry,pixel counts,driver/backend and workloads.
Do not attribute this Intel result solely to CPU age without controlled profiling.

The next performance investigation can target100k listing on this older Intel
at2x, separating ordering,queue work and software painting. This report changes
no application code and does not claim a diagnosis or speed fix. The complete
explorer goal and pending file-chooser integration remain preserved.

[Raw results,hashes,host manifest and method](evidence/intel-comparison-20261104/assessment.json).
