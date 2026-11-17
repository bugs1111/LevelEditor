# Native GUI/GPU platform acceptance

This supplements the compile/test/package matrix in `platform-acceptance.json`.
The manually dispatched **Native GUI and GPU acceptance** workflow runs native
windows on `macos-15` (ARM), `macos-15-intel`, `windows-latest` and `ubuntu-24.04`.
Creating the workflow does not establish that those GUI runs have passed.

Each runner builds the release `native_smoke` example and application test
binary, then runs six sequential native-window scenarios:

| Scenario | Required evidence |
| --- | --- |
| Home | No project opened; focus mask dismissed; nonblank PNG |
| Entities | Loaded resources; active Entities layer with entities; nonblank PNG |
| Entity form | Real injected `E` shortcut opens the Entities form |
| Held entity drag | A real press/move starts `EntityMove`, opens the entity inspector and preserves entity count/dirty state while held |
| World | Checked-in free-layout project loaded with world view active |
| Paint | `F1` selects the basic AutoLayers IntGrid layer; a real press/move/release stroke adds cells and changes its checksum |

The held drag reuses the recorded `entity_move` actions from
`acceptance-input-scenarios.json`: PlayerStart at world `(72,360)`, then move by
`(+64,-32)` without releasing. Its camera is recentered on that entity so the
press is at the actual canvas center, clear of the left toolbar on narrow
windows. Acceptance requires
`gesture: null → EntityMove`, `stroke: false → true`, `panel: Entity`, an active
Entities layer, unchanged positive entity count and unchanged dirty state.
The screenshot captures a held, uncommitted gesture. It does not establish
committed entity coordinates or an eventual release result. Color-picker and
tile-popout remain outside the automatic pass matrix because current capture
state cannot distinguish opened from missed popup actions.

All scenarios require an adapter log, successful process exit, screenshot
completion, zero surface acquisition errors, finite positive logical viewport
size/DPI metadata matching the PNG dimensions, and at least 32 distinct RGB colors
in a valid native screenshot. Project captures also require `capture_ready()`.
Each has a fresh `LDTK_CONFIG_DIR`, at least one second settling and two seconds
of sampling, and a 90-second process limit. Fixture SHA-256 hashes must remain
unchanged. The script continues collecting the remaining scenarios after a
failure and exits nonzero if any scenario fails. It never substitutes a
headless egui render or treats a missing adapter as a skip.

Three adapter-dependent ignored tests run separately, including after a scene failure:

- `canvas::blur::gpu_tests::gaussian_composites_original_layer_then_blurs_on_gpu`
- `canvas::outline::tests::field_outline_uses_two_sixty_sample_passes_and_reuses_cached_flow`
- `app::gpu_surface_tests::surface_error_submits_pending_uploads_without_a_present`

All request an adapter/device and fail if unavailable. Blur verifies GPU texture
readback; outline verifies its GPU pass/cache behavior; the surface-error
regression checks that a failed acquire still submits pending uploads. Their exact names avoid
selecting the separate renderer tests that require untracked `.reference/ldtk`
resources. Each step requires exactly one passed test with none ignored, so
a renamed test cannot silently pass by matching zero tests. Each test step has
a three-minute limit.

Artifacts upload even on failure as `gui-acceptance-<target>`. They contain
`results.json` with source commit, platform, adapter details/classification,
state before/after input, surface counters and timing lines; six PNG/log pairs;
isolated profiles; the three GPU test logs; and `native-harness.zip` containing
the executable, standalone GPU test executable, resource assets/examples, runner and source revision for
physical-platform follow-up. A screenshot is evidence of a
rendered scene and the recorded state, not pixel parity with the original app.
Injected events traverse the real application input hook; they do not prove
OS-level foreground ownership, mouse delivery or input-to-photon latency.

Linux explicitly uses Xvfb and Mesa lavapipe Vulkan. Setup resolves exactly
one `lvp_icd*.json` manifest, accepting Mesa package filename changes while
failing if its driver is absent or ambiguous. Windows may select a
software adapter such as WARP; macOS must obtain an available Metal adapter.
The adapter actually returned is recorded for each scene. `software`, `virtual`
and `unknown` adapters remain distinct from reported integrated/discrete GPU
adapters. Even a hosted runner reporting hardware does not establish acceptance
on a user's physical Windows/Linux/Intel Mac hardware. Missing graphics/display
capability is a failed CI result to investigate, not a platform success.
Windows RSS may be unavailable because the current harness samples with `ps`;
RSS is preserved as an observation and is not a GUI pass criterion. These short
captures also do not resolve the separate long forced-redraw Home memory issue.

Run locally after building, with no other acceptance/editor process active:

```sh
scripts/cargo build --release --example native_smoke --locked
python3 scripts/verify-gui-acceptance.py --output /tmp/ldtk-gui-acceptance
```

For Linux use the workflow's Mesa/display setup and prefix the Python command
with `xvfb-run -a -s '-screen 0 1600x1000x24'`. The harness requests 1440×900 logical content with scale-1 editor settings.
The runner reads Home's actual logical viewport/DPI and translates canvas input
by half the size difference; the sidebar remains 300 logical points wide.
Paint therefore targets the same empty cells, while the held drag targets the
recentered entity at `(width/2+150,height/2)`. Each scene records actual viewport,
DPI and injected controls. Missing/invalid metadata or a PNG mismatch fails
acceptance, rather than inferring logical coordinates from physical pixels.
No physical-platform result is claimed until the relevant run is recorded.

The portable ZIP runs without a checkout after extraction (on Unix, restore the
executable bit with `chmod +x native_smoke` if the extractor did not preserve it):

```sh
python3 scripts/verify-gui-acceptance.py --binary ./native_smoke \
  --fixtures resources/examples/original --output-dir /tmp/ldtk-gui-physical \
  --source-commit "$(cat source-commit.txt)"
```

On Windows use `python` and `--binary ./native_smoke.exe`. A physical Linux run
should inherit its authorized display and native Vulkan driver, omitting the
CI-specific Xvfb/lavapipe environment. Coordinate the display before starting a
window; the runner always simulates focus and routes injected input through the
editor. Its output directory and profile remain independent of CI evidence.

The same ZIP includes `gpu-tests` (`gpu-tests.exe` on Windows), discovered from
Cargo's `compiler-artifact` JSON rather than an assumed target directory. It can
run the three exact GPU tests offscreen without a display. On a Linux NVIDIA host,
set `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json` to exclude software
drivers and record the driver identity alongside the result:

```sh
chmod +x gpu-tests
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json ./gpu-tests \
  canvas::blur::gpu_tests::gaussian_composites_original_layer_then_blurs_on_gpu \
  --ignored --exact --nocapture --test-threads=1
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json ./gpu-tests \
  canvas::outline::tests::field_outline_uses_two_sixty_sample_passes_and_reuses_cached_flow \
  --ignored --exact --nocapture --test-threads=1
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/nvidia_icd.json ./gpu-tests \
  app::gpu_surface_tests::surface_error_submits_pending_uploads_without_a_present \
  --ignored --exact --nocapture --test-threads=1
```

Each command must report exactly one passed test. Passing these tests proves
GPU execution/readback and the tested effects, while native-window presentation
and GUI input still require their separate successful scene captures.

## Recorded local validation — 2026-11-17

Source `7b2ef29a9eb09793001c9cc8a6fbf874ba613d6d`, macOS 27.2.1 ARM64,
Apple M6 integrated GPU, Metal: all five native scenarios passed with 2880×1800
captures and zero surface acquisition errors. The paint scene added 13 cells
(484 → 497) and changed the IntGrid checksum. Both exact GPU tests reported
`1 passed; 0 failed; 0 ignored`. The integration owner visually inspected all
five screenshots. Local evidence is `.tools/gui-acceptance-local/results.json`,
its five PNG/log pairs and `gpu-blur.log` / `gpu-outline.log`. This is local
Apple ARM/Metal evidence; the other CI and physical platforms remain pending.

### Six-scene v0.1.1 follow-up

The fixed v0.1.1 release harness passed all six scenes on Apple M6/Metal in
`.tools/gui-acceptance-fixed-six/results.json`; all six 2880×1800 screenshots
were visually inspected. Held drag transitioned from no gesture/stroke to
`EntityMove`/active stroke with the Entity inspector, preserving nine entities
and `dirty=false`. Paint again added 13 IntGrid cells (484 → 497). All scenes
reported zero surface errors. These normal captures complement the separate
surface-error regression and forced-timeout memory stress; they do not inject
failures themselves.

### Final logical viewport contract recheck

After the explicit dark-theme correction and viewport metadata log, all six
scenes passed again in `.tools/gui-acceptance-final-contract/results.json` with
logical size 1440×900, 2 pixels per point, matching 2880×1800 PNGs and zero
surface errors. All six PNGs were visually inspected. The recentered held drag
retained nine entities and `dirty=false` while showing `EntityMove`, the ghost,
arrow and faded inspector. Paint retained its checksum/cell-change requirement.
A separate protocol-only check exercised 1024×768/DPI1 coordinate conversion,
including paint's 13 initially empty row-5 cells; it is not Windows GPU evidence.
Its script, geometry and validator output are preserved under
`.tools/gui-viewport-contract-probe`. Twenty-four gate cases pass, including
negative missing/invalid viewport/DPI and mismatched PNG dimensions. Timing
lines from the final run are observations only; other verification was running
concurrently, so this run is not a performance comparison.
