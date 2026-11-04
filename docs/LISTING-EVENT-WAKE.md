# Large listing wake-up and M2 evaluation

2026-11-04 (runs began2026-11-03). Sourceff7af56 improves the existing bounded worker/event-loop
handshake. Consuming a response wakes the producer with an unpark token; sending
one wakes FLTK's event loop. Previously a full producer queue retried on a2ms
timer and the UI relied on its5ms fallback poll. No additional callback queue,
thread, filesystem parallelism, sorting exception or omitted entry is introduced.
Response capacity8, batch256, pump64, generation/cancellation and limiter4 remain.
The defensive producer timeout remains; earlier notifications are not lost.

A new4097-entry fixture saturates the response queue, verifies every ordered
entry arrives, then repeats with restart to ensure old queued generations never
reach the new listing. NODE validation:1612workspace tests passed
(704core/112helper/796UI,three historical ignored),7focused worker tests,
Clippy workspace/all-targets with warnings denied and format check passed.

## NODE comparison

Four valid runs per app/count, alternating AB/BA, same native Wayland655x836
window, warm filesystem cache/new process,List/name sort, same10k/100k emptytxt
fixtures and private profiles as the [original comparison](FLEA-0.4-COMPARISON.md).
No builds or other own benchmarks during timed passes. Flea0.4 official x86_64
asset still uses VulkanRTX3080Ti. No new code in Flea or changed UI readiness.

| Entries | App | Median listing-state readiness | Median stable tree RAM |
|---|---|---:|---:|
|10k|Files optimized|98ms|18.8MiB|
|10k|Flea0.4|407ms|102.2MiB|
|100k|Files optimized|206ms|32.6MiB|
|100k|Flea0.4|408ms|104.6MiB|

The previous Files100k median was603ms: the new run takes≈66% less time,
≈49% less than Flea in this run, retaining≈69% less stable tree RAM.
The producer-only pilot was565ms, both-direction pilot224ms; both are single
investigative runs, not statistical acceptance. Final ranking uses16 valid runs.

## Same source on M2

The user explicitly requested M2 evaluation. Authoritative development stays
NODE. A native ARM release was built offline/locked in an isolated M2 test tree;
worker SHA matches NODE exactly. No installed executable, default association,
file chooser or ordinary settings were replaced. HTML support was copied
unchanged from the existing ARM installation solely into the private trial.

M2 has Asahi and two outputs. Comparison clients were moved/resized through the
existing benchmark geometry helper to1100x700 on HDMIscale1, with readback.
Startup geometry before that transition is retained. Timing includes geometry
setup and is not directly comparable to NODE. First Files run initially mapped
on internal2x; subsequent rows record their actual initial geometry. Default
row densities differ. No emulated native input or displayed-frame latency claim.

| Entries | App | Median listing-state readiness | Median stable tree RAM |
|---|---|---:|---:|
|10k|Files|97ms|33.8MiB|
|10k|Flea0.4|197ms|146.7MiB|
|100k|Files|213ms|43.0MiB|
|100k|Flea0.4|195ms|149.5MiB|

Flea ARM logs Vulkan **llvmpipe**, a software renderer, not hardware Asahi Vulkan.
This describes the current M2 setup, not GPUI hardware performance in general.
Files RAM advantage remains;100k readiness is close, and its≈18ms disadvantage
is smaller than the polling granularity. No Files speed victory is established
on M2. First Flea launch also includes cache warm-up; all four rows are retained.

## M2 minimum checks

- First-draw test:10/10 below100ms; median55.3ms,max/p9577.5ms. This is the
  existing external draw-marker method, executable-only fadvise; system/font/
  shared-library caches were not globally evicted. Not proof of truly cold
  startup, compositor presentation or input readiness.
-10k Groups baseline at verified1920x1080 physical (960x540 logical,scale2):
  **FAIL**, peak58.46MB decimal vs50MB goal; stable45.29MiB. The historical40MiB
  analysis threshold also fails; no baseline/global acceptance is granted.
  The existing harness samples the entire launch, including the initial tiled
  geometry before resize: the peak is that launch envelope, not proof that every
  sample was already at a fixed1080p surface. This remains unaccepted.
- An earlier1920x1080 logical request on2x measured3840x2160, invalid for the1080p
  target. Its≈102MB peak and geometry failure are preserved, not normalized.
- Cua0.28.2 returned zero native Wayland windows; no keyboard/pointer fallback
  was invented. Native keyboard,scroll,clipboard,AT-SPI and2x sharpness remain
  unaccepted. The driver started by this test was stopped.

Initial M2 packaging attempts missed vendoredjpeg-decoder and bundledtheme
includes; exact files were copied, then native release passed. Initial legacy
monitor-dispatch setup failed; installed LuaAPI was read and used for the next
run. All diagnostics remain in evidence; they are not hidden app failures.
Both hosts' retained timed identities exited; existing installations preserved.

On M2, manually try the private build with:

```sh
~/.cache/files-100k-m2-check/try-files
# Or append a folder path.
```

The launcher uses separate Files preferences and disables session restoration.
It reads the user's requested folder normally; it does not set Files as default.
The default file-chooser integration remains pending.

## Decision and evidence

NODE's observed100k bottleneck is improved and its scoped memory/speed comparison
now favors Files. **Universal superiority is not established.** M2 passes the
limited first-draw test but still fails the memory minimum, and does not show a
100k speed advantage. Keep those distinct from the complete explorer goal.
Resource/startup work here is limited to the user's authorized comparison,
optimization and M2 checks; other frozen experiments/new preview formats stay paused.

[Hashes,32 comparative passes,minimum checks and raw logs](evidence/listing-event-wake/assessment.json).

Post-timing M2 List/Groups captures verify10k populated rows and visible metadata
on the internal2x output. They were taken after every measured pass and do not
validate native input or the full2x sharpness gate; footerPSS was not used.
