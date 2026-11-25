# Home release memory investigation — 2026-11-17

Follow-up confirmed and fixed an acquisition-error staging-buffer accumulation in the installed egui-wgpu 0.33.3. A controlled Metal surface fault reproduced high memory, and the production recovery passed repeated real-timeout stress tests. See the follow-up section below and [machine-readable evidence](home-memory-followup.json). The exact OS event in the initial historical run was not recorded and cannot be reconstructed from its memory samples alone.

The original observations and controls below are retained as history: later runs of the **same unchanged binary** returned to approximately 120 updates/s and about 252 MiB RSS at screenshot capture. Those lower readings were not a fix. The eventual fix does not introduce frame pacing or `device.poll(Wait)`.

All runs used the Apple M6 / Metal backend, the isolated profile `/private/tmp/ldtk-acceptance-settings`, UI scale 1, and a 1440×900 logical viewport. The recent list retained the 13 MB `world1.ldtk` project and the many-entity fixture. Capture state remained Home, with no project loaded, no gesture, and `dirty=false`.

## Recorded results

RSS values below are KiB from the harness. “Ready” is sampled at ready frame 20, **before** the settling interval; “capture” includes screenshot readback allocations. CPU update timing and frame intervals do not measure completed GPU work or input latency.

| Run | Settling + sampling | Total updates | Sampled updates | Ready / capture RSS | Log |
| --- | --- | ---: | ---: | ---: | --- |
| Initial release observation | 3 s + 2.000 s | 10,273 / 5.462 s | 3,648 | 202,096 / 1,246,720 | `.tools/home-streaming-release.log` |
| Longer unchanged-binary observation | 10 s + 2.000 s | 20,329 / 12.463 s | 3,651 | 197,648 / 1,249,104 | `.tools/home-rss-diagnosis-smoke.log` |
| Original-path recheck | 3 s + 2.001 s | 607 / 5.161 s | 241 | 194,560 / 257,968 | `.tools/home-rss-path-recheck-root.log` |
| Byte-identical copy recheck | 3 s + 2.000 s | 606 / 5.127 s | 241 | 195,776 / 258,432 | `.tools/home-rss-path-recheck-copied.log` |
| Own-PID activation request | 3 s + 2.008 s | 607 / 5.142 s | 242 | 193,648 / 256,256 | `.tools/home-rss-os-active.log` |
| Own-PID hide, then restore | 6 s + 2.000 s | 966 / 8.140 s | 241 | 195,344 / 257,824 | `.tools/home-rss-os-hide-restore.log` |

The two recheck executables had identical SHA-256: `f2d909e32a7896b81803e04df4f9db9f6c4ec84979b595fa69e6c0506f49f34e`. The original executable was not rebuilt or replaced between these controls. This rules out a code change as the explanation for the later lower readings; it does not identify the native-state trigger.

In the longer high-update-rate run, external `ps` samples were 517,488 KiB at 0.51 s, 1,209,120 KiB at 3.01 s, and 1,209,504 KiB at 7.01 s. RSS had reached a plateau over that observed interval, rather than continuing in proportion to the total update count. Raw samples: `.tools/home-rss-samples.json`.

The later `vmmap -summary` snapshot reported a 2.7 GiB physical footprint, including approximately 2.0 GiB in `IOAccelerator (graphics)` across 29,395 regions, 80.5 MiB resident `IOSurface` memory across 710 regions, and 523.1 MiB resident `Malloc Small` memory. These are different accounting categories from `ps` RSS and were not sampled atomically. The region counts alone do not identify which renderer objects were retained. Snapshots: `.tools/home-rss-vmmap-0.5.log` and `.tools/home-rss-vmmap-7.0.log`.

## Controls and their limits

An actual release `level-editor` process, without the harness's continuous repaint request, measured 197,024 KiB at 3 s and 197,104 KiB at 7 s. Its graphics snapshot contained 4,528 KiB of `IOAccelerator (graphics)` memory and seven `IOSurface` regions. This process was inspected through `ps`/`vmmap`; its OS window was not captured, so it is not a visual acceptance result. Logs: `.tools/home-rss-idle-samples.json`, `.tools/home-rss-idle-vmmap.log`.

A temporary copy of the full native harness used the actual Cargo dependency/features and `app::native_gpu_config`, with optional blank-UI and GPU-wait branches. Its dependency fingerprints matched the original for eframe, wgpu, egui_extras, and objc2. During these later approximately 120-update/s runs, normal Home captured at 257,424 KiB, blank UI at 177,632 KiB, and Home with GPU wait at 238,128 KiB. All three were already in the lower-memory regime; this does **not** establish GPU wait as a remedy for the earlier regime. Logs: `.tools/home-rss-exact-{app,blank,wait}.log`; samples: `.tools/home-rss-exact-results.json`. Temporary probe resources used a symlinked manifest root, producing one recent-folder tint difference; the byte-identical original/copy controls avoid that build difference.

Smaller eframe probes also tested focus injection, repeated title updates, and the project's macOS document-event installation. They remained near 108,000 KiB during the later runs. They did not reproduce the earlier native regime, so they cannot individually clear those paths as possible contributors under other conditions.

The Swift helper controlled only the diagnostic PID through `NSRunningApplication`. The activation request did **not** produce `isActive=true`; a true foreground comparison was therefore not established. Hiding did produce `isHidden=true` and remove the window's onscreen flag, and restoring reversed those flags. That hide/restore sequence retained the lower-memory regime. The harness continued to simulate `input.focused=true`, which is distinct from OS activation. Exact helper output, window bounds, and RSS samples are in `.tools/home-rss-os-results.json`.

## Reproduction commands

Equivalent child launch for the initial scenario, from the repository root:

```sh
env LDTK_CONFIG_DIR=/private/tmp/ldtk-acceptance-settings \
  LDTK_SMOKE_SETTLE_SECONDS=3 \
  LDTK_SMOKE_OUTPUT=/private/tmp/ldtk-home-rss.png \
  target/release/examples/native_smoke
```

The longer observation used `LDTK_SMOKE_SETTLE_SECONDS=10`. External inspection used:

```sh
ps -p PID -o rss=,vsz=,etime=
vmmap -summary PID
```

Using the current product name in the example path, the byte-identical control was copied with Python `shutil.copy2` to `/private/tmp/level-editor-rss-renamed/original/native_smoke`, then launched with the same environment and working directory. Hashes and paths are recorded in `.tools/home-rss-path-recheck-results.json`.

The own-PID helper was built and invoked as follows; `activate` first unhides the target, then requests activation. The recorded `isActive` result, rather than the request alone, determines whether activation succeeded.

```sh
swiftc -module-cache-path /private/tmp/ldtk-swift-module-cache \
  /private/tmp/ldtk-own-app-state.swift -o /private/tmp/ldtk-own-app-state
/private/tmp/ldtk-own-app-state PID activate
/private/tmp/ldtk-own-app-state PID hide
/private/tmp/ldtk-own-app-state PID activate
```

The temporary diagnostic harness lives at `/private/tmp/ldtk-home-rss-probe`, outside the repository. Its build command was:

```sh
scripts/cargo build --manifest-path /private/tmp/ldtk-home-rss-probe/Cargo.toml \
  --target-dir /Users/bugs/Projects/LevelEditor/target --release --offline
```

Its executable is `target/release/ldtk-home-rss-probe`. `LDTK_RSS_BLANK=1` selects blank UI; `LDTK_RSS_WAIT=1` waits for prior GPU submissions. These diagnostic switches are absent from production source. Build log: `.tools/home-rss-exact-probe-build.log`.

## Original diagnostic boundary

Source review found a concrete mechanism to instrument if the high-update-rate regime returns: egui-wgpu 0.33.3 uploads buffers **before** acquiring the surface. A surface-acquisition error returns before queue submission. wgpu's pending staging writes are drained by submission; polling completed work is not a replacement for that submission. Repeated acquisition failures could therefore retain pending allocations. No surface-error counter was collected in the high-RSS run, so this remains a hypothesis for that run, not its established cause. The active default error handler returns `SkipFrame`; repeated surface reconfiguration is not an established path in this configuration.

The next discriminating capture should record surface-acquisition error counts, successful submissions/presents, OS activation/visibility, and memory categories during the **same** high-update-rate interval. A subsequent harness-only counter in `examples/native_smoke.rs` now prints `Surface acquire errors (all frames)` while preserving the configured error action; the runs above predate that instrumentation. The existing observations do not justify changing the production frame policy or asserting a Home leak has been fixed.

All diagnostic native processes had exited at the final check; exact executable-path matches were zero. Evidence: `.tools/home-rss-process-cleanup-check.log`. The actual release app inspected for the idle comparison was explicitly terminated after sampling. Temporary probes and helpers remain outside tracked source; no diagnostic probe is part of the release.

## Controlled reproduction and production fix — 2026-11-17

Three unchanged-binary Home rechecks remained near 195 MiB before capture, about 120 updates/s, and zero acquisition errors. Explicit FIFO/immediate configuration, an optional GPU-wait control, hiding, minimizing and off-screen placement also failed to reproduce the high-memory regime. These negative controls did not justify a pacing or cache-size change.

Source inspection identified the pending-write lifetime: the installed `egui-wgpu 0.33.3` painter calls `update_texture` / `update_buffers` before `get_current_texture`; `SurfaceErrorAction::SkipFrame` returns without queue submission. `wgpu` retains staging buffers for those writes until submission. The upstream [egui fix #7928](https://github.com/emilk/egui/pull/7928) likewise uses an empty queue submission on painter exits without a normal submit. That fix is absent from the installed version.

To make this path repeatable without modifying another application or the display, a disposable full native Home probe temporarily cleared only its own `CAMetalLayer` device, then restored it. This produced actual `SurfaceError::Timeout` results. The same probe measured GPU buffer object counts, queue completion callbacks, acquisition errors and external `ps` RSS. The fault is deliberate; it does not identify the original historical OS event.

| Case | Timeout count | Maximum sampled GPU buffers | Maximum sampled RSS KiB | Recovery capture |
| --- | ---: | ---: | ---: | --- |
| Original error handler, 1 s fault | 9,435 | 18,881 | 1,878,000 | Passed after restoring the surface |
| Diagnostic empty-submit control, 1 s | 7,011 | 8 | 245,536 | Passed |
| Production fix, 1 s | 6,975 | 8 | 197,312 | Passed |
| Production fix, 5 s | 36,637 | 10 | 197,040 | Passed |
| Production fix, 10 s | 73,388 | 10 | 198,624 | Passed |

The fixed production runs used the application callback itself, with the diagnostic flush override unset. Texture counts stayed at 40. Metal's byte counters returned zero in this probe; they are not used as memory evidence. RSS sampling and screenshot-time allocations are separate: fixed capture RSS was 258,976–260,064 KiB. The transient two additional buffers reflect outstanding writes, rather than an allocation growing with every failed frame.

The production change is limited to `app::native_gpu_config` and the existing canvas GPU state: on acquisition errors, submit pending queue writes before returning the original error handler's action. Successful frames add no submission, wait, sleep or frame cap. This avoids a renderer fork or broad dependency upgrade. The existing eframe painter keeps one render state across its viewports; `EditorApp::new` configures that state before rendering.

The regression `app::gpu_surface_tests::surface_error_submits_pending_uploads_without_a_present` performs actual GPU readback through the production error callback. It failed before the fix with `[0,0,0,0]` instead of the queued `[1,2,3,4]`, and passed after the fix. Polling without submission did not make the failing test pass. The default SkipFrame result is asserted. Run explicitly with a GPU:

```sh
scripts/cargo test --bin level-editor gpu_surface_tests::surface_error_submits_pending_uploads_without_a_present \
  --locked -- --ignored --nocapture --test-threads=1
```

Full workspace tests passed (native app 316 passed, 6 ignored); the native example target passed 320 tests, 6 ignored. Strict all-target Clippy and formatting passed. The newly ignored GPU regression was run separately and passed, and is an explicit step in the four-platform GUI/GPU workflow. A read-only independent review found no blocking defect in the queue ownership or error policy.

Raw reproduction, controls, readback RED/GREEN results and stress samples are under `.tools/home-memory-followup/`; the disposable probe source remains `/private/tmp/ldtk-home-memory-followup/probe.rs`. [home-memory-followup.json](home-memory-followup.json) records the measurements and limits. This establishes and fixes the repeatable acquisition-error memory defect; it is not a universal assertion that every cause of high memory or every physical display transition has been tested.
