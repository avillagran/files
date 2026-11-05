# Files versus Flea 0.4.0 — NODE, 2026-11-03

The user authorized lifting the resource/startup freeze solely to decide
whether Files is worth continuing for being lighter **and** faster than
Flea 0.4. No functionality was added, no app was installed and no associations were changed.

**Result: a clear RAM advantage for Files; listing speed is mixed. The
strict condition of winning on both dimensions is not met in this test.**
This does not mean Flea always wins or that Files should be deleted. Do not keep
adding features out of inertia: this evidence informs the user's decision.

## Results

Medians of four valid runs per app/size (16 total), alternating AB/BA.
Steady memory of the process tree: sum of PSS+SwapPss, including the observed helpers.
GPU memory is recorded separately and is not blindly added to PSS.

| Files | App | Steady RAM MiB | Sampled peak MiB | Listing ready state ms | Window registered ms |
|---|---|---:|---:|---:|---:|
| 10,000 | Files | 18.9 | 18.9 | 97 | 97 |
| 10,000 | Flea 0.4 | 107.9 | 108.6 | 403 | 399 |
| 100,000 | Files | 32.6 | 32.6 | 603 | 99 |
| 100,000 | Flea 0.4 | 110.5 | 111.3 | 409 | 405 |

Files cuts the tree's RAM by about 82.5%/70.5% in these two folders.
With 100k, Files' listing state takes about 47.4% longer than Flea's.
A registered window does not prove it is already usable. No column measures native
input→presented frame nor allows claiming overall superior responsiveness.

## Protocol and traceability

- NODE x86_64, AMD Ryzen 9 5900X; real Wayland Hyprland 0.56.2, 1920x1080 screen,
  scale 1. Clients 655x836 at the same position/geometry in every run;
  each application keeps its default row density and style.
- Flea tag v0.4.0/22fa922, official linux-x86_64 binary. The tarball's SHA256 verified
  against the published file: efa466c4916228cad6e374f9f33b216ef236f53515758fddc502c6972f5c50d7.
  Each log confirms Vulkan NVIDIA GeForce RTX 3080 Ti, not a software fallback.
- Files release built on NODE from 5a8f995, normal Cargo/original idle gate,
  build finished before measuring. FLTK/fltk-rs 1.5.23; not installed.
  SHAs of the four executables in assessment.json.
- Own btrfs folders with 10k/100k empty .txt files, same names, List and
  sort by name. Private HOME/XDG profiles; no usual preferences/caches.
  Directory metadata pre-warmed before each size; fresh processes.
  **These are not real cold-cache tests**: the machine's global cache was not dropped.
- Tree sampling every ≈40–59 ms, five seconds per run and at least three after
  readiness. Steady median from max(ready+1 s, 3 s). Sampled peaks are lower
  bounds; children born/dying between samples may escape. Zero sampler errors
  in 16 valid runs. Files' footer PSS counter was not used.
- Files readiness: FILES_METRICS_READY→scene_ready, loaded, error-free, exact count.
  Flea: IPC listingWindowState exact total/loaded>0, state ready, drawnCount>0 from the
  mounted view adapter. They are different observers; external latency from
  polling (tens of ms), with Flea's IPC overhead included. Files does not wait for stat
  of every row in List. Later captures verify the visible listing and the
  100k count in both, outside the timed runs.
- GPU: nvidia-smi records Flea's G/C+G memory on two GPUs; it is not interpreted
  as an independent total that can be added to PSS nor used to win the comparison.
- Everyday desktop with other apps: no unrelated processes were closed or killed.
  Alternating reduces order bias; it is not an idle-machine laboratory.
- An initial readiness parser failure in Flea 100k: the IPC did not return JSON yet;
  the app did not fail, the harness did. The full log is kept; it was fixed to not-yet-ready and
  repeated as 2-retry. Four valid runs per group; the failed one explicitly excluded.
- Only process groups created by the test were terminated with SIGTERM. This
  is not a test of normal close, recovery or the absence of long-term leaks.

## What is missing for a broader conclusion

Not measured: a truly cold start, search/navigation/scroll through native
input, presentation latency, equivalent thumbnails, sustained use,
CPU/energy or other GPUs/machines. No computer-use driver was available;
keyboard injection was not improvised and no handler was passed off as OS input.
The apps do not have full feature parity either: this test covers List with
empty files. The RAM advantage does exist in this scenario; universal speed
superiority is not shown and the 100k case favors Flea.

[Evidence, hashes, raw files and limits](evidence/flea-0.4-comparison-20261103/assessment.json).
[Chart](evidence/flea-0.4-comparison-20261103/comparison.html).
Official sources: [release](https://github.com/thisisgm/flea/releases/tag/v0.4.0),
[method](https://github.com/thisisgm/flea/blob/v0.4.0/docs/benchmarks.md).
The tag's README still shows 0.3.10 metrics: they were not used as 0.4 results.
The exceptional authorization does not lift the freeze for other experiments.

## Subsequent optimization

The original results above remain historical evidence. Source ff7af56 subsequently
improved worker/UI wake-ups: NODE 100k readiness 206 ms vs Flea 408 ms, with retained
RAM advantage. M2 was separately evaluated and still has open memory/native
acceptance. See [LISTING-EVENT-WAKE.md](LISTING-EVENT-WAKE.md); this does not claim
universal superiority or erase the earlier mixed result.
