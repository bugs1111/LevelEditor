# Full parity execution ledger

Baseline: `562f7aa`, clean worktree; 52 tests pass. Upstream source commit: `2b7b5512f2f88fa281cb6492debbb26ead7838d4`.

User instruction: continue until the complete Rust port is finished; use original icons, fonts and examples directly. No scope reduction or additional approval gate.

Implementation and review have been split among core/format, native UI/workflow, rendering/acceptance and independent review workers. File ownership is handed back before integration; reports below identify verification boundaries.

## Implemented and verified during continuation

- Deterministic auto-layer engine, incremental updates, rule/group CRUD. Current official caches match all 57,682 tiles exactly; independent original compiled RNG/noise vectors and incremental/full rebuild comparisons pass. Evidence: `autotile-report.md`. Historical stale/versioned caches are identified separately.
- Full fields/defaults/arrays/enums/references, entity limits and geometry, world/level/definition workflows, project-wide history, clipboard regions, and mixed-layer selections. Official field/entity/TOC/neighbour exports were compared. Evidence: `workflow-report.md` and core integration tests.
- Save As resource rebasing, external levels, backups/restore/rename/conflict snapshots; original exporters and Ogmo/external-enum import. Actual sample outputs were generated, including source-expected exporter errors. Evidence: `formats-report.md`.
- Native forms call transactional core APIs; open/save/export are asynchronous. Native tests caught and fixed a null tile field being assigned merely by viewing it, stacked flood-fill membership/erase differences, and palette navigation incorrectly panning the camera. The early 55-test snapshot has been superseded by the verification runs below.
- Original bitmap fonts/icons/examples/atlases are bundled. Actual Metal Gaussian/color-transform pixel readback and rendered sample captures pass. Corrected large-project screenshot contains 43,002 visible cells; the earlier spinner capture was rejected. Evidence: `renderer-report.md`.
- A separate original LDtk 1.5.3 process runs with an isolated profile/fixture copy for live UI comparison. Real reference screenshots: `original-entities.png`, `original-entity-def.png`, 1440×900 logical pixels at 2× DPI. These exposed substantial chrome/panel differences; no claim of aligned pixel equality has been made.

## Integrated native workflows

- Source-shaped 300-point sidebar, Home with full original samples and thumbnails, compact color picker and tile popout, original help affordances, persistent form state, right-side entity inspector with move/resize fading, connector and constrained dimension inputs.
- Mixed-layer sparse selection, ghosts, raw/parallax geometry, all entity link styles, auto-chain preview, Point and EntityRef picking/following across worlds, source world menus and guarded world forms.
- Every field type, indexed array operations preserving untouched default markers and wrapper extensions, ten-language native text modal, line numbers, typed/pasted/IME event distinctions, balanced pairs/XML tags, external text and read-only application logs.
- Commit-before-save/close/search/navigation, including same-frame final input and owner changes. Regression tests cover the previously reproduced stale entity-index and pending-input failures.
- Background new-project/save/import/export, source hooks and command streaming/cancellation/trust, file/resource recovery and conflict handling, backup Home routes bound to the actual original path. Repeated Ogmo imports deliberately choose an unused destination rather than overwrite an existing project.
- Native macOS document-open AppleEvents, startup FIFO and runtime wake, Unicode paths, app/document icons, alternate file registration, signed bundle. Original default application association is preserved.
- Actual secondary-button press/move/release erases entities and Point slots as one undo, without leaving a stale inspector index. A reproduced right-click stamp paste was corrected and its real egui event regression passes.
- Source bleep feedback now includes reference hover/follow, delayed repeated command search, Point picking, removal, selection, error diagnostics and undo/redo bounds. Bounds are recorded as four coordinates inside existing history transactions; no full-level diff is added for highlighting. Real hover regressions exposed and fixed a context-lock reentry and a shared-reference widget ownership bug.
- Source-measured sidebar geometry now places the TopDown tile palette at exactly y=281 logical points: 36-point buttons, four-point bottom margin, 31.5-point layer rows with one-point margins, source guide bounds and 29-point tile toolbar. The sidebar remains 300 points wide. Tests exercise the actual egui layout, rather than compensating with an overlay offset.

## Verification before final packaging

- The complete core reference run passed all 13 explicitly ignored corpus/oracle/performance checks. JSON roundtrip covered 49 projects and 3 external levels; current official auto-layer caches matched 57,682 tiles exactly; all 66 incremental edits matched full rebuilds. Original RNG/noise vectors, migration idempotence, exporters/importers and large-project operations were included.
- Native test coverage has grown beyond 270 tests per app target, including actual egui input and modal flows. Final integrated counts and commands are recorded after the last source-feedback integration; earlier intermediate counts are not final-build claims.
- Windows GNU and Linux GNU workspace/all-target checks passed with local target standard libraries and locked dependencies. These are cross-compilation checks, not runtime or packaging verification on those operating systems.
- After adding bounds recording, the release large-level history check passed again: warm mean begin+commit 3.782091 ms. The source geometry/lifecycle tests also pass; this local timing is not a universal latency guarantee.
- Real Metal captures compare equal camera/viewport states against an isolated original LDtk 1.5.3. ColorSync normalization was required for the original embedded color profile. The level-edge/shadow sample at 15 distances differs by at most 2/255 per channel. This is localized evidence, not proof of whole-application pixel equality.
- Loading, 13 MB scenes, 2,048 entities and actual paint strokes have measured warmed app-update CPU timings. The final harness also separates eframe CPU rendering and frame-start intervals; these are not presented FPS or input-to-photon latency. Invalid loading/focus/input-contaminated captures were rejected and replaced.

Transient feedback and sidebar source geometry have been verified. The final field presentation, interaction captures, release GPU checks and latest bundle verification are still being finalized. See `acceptance-report.md`, `bleep-feedback.md` and the final verification record for their outcomes. Remaining acceptance limits are tracked in `parity.md`; an unavailable Rust release-update feed and untested platform runtime must not be represented as completed upstream-equivalent behavior.
