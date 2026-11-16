# Progress — full native Rust LDtk port

2026-11-16: User selected the complete native Rust route and explicitly asked to begin immediately. Working in the initially empty LevelEditor directory on a new `rust-port` branch, rather than creating a second worktree of an empty repository.

Baseline downloaded: `2b7b5512f2f88fa281cb6492debbb26ead7838d4`. Toolchain is being installed locally because the host has no Rust.

Execution decision: independent core implementation is delegated while the primary agent implements native rendering/integration. Files have explicit owners; one fresh reviewer checks the assembled result. Full parity remains the objective, not a claim about the first build.

Pre-flight: core and desktop share `Project`/`LevelKey`/`History` only; desktop does not implement a second serializer or maintain a second authoritative document.

## Implemented initial stage

- Core: complete JSON preservation, external levels, staged saves, basic grid/tile/entity edits, bounded level history, definition creation and atlas assignment. Advanced edits with unsafe derived data are explicitly refused; see core-report.md.
- Desktop: native Rust window, original color/font/icon assets, sidebar and palettes, GPU atlas batches, world preview, coordinate tools, native project dialogs, paint/erase/fill, grouped history and scalar fields.
- Viewport 5 tests were observed red then green. Scalar wrappers and field-definition bounds had observed red/green cycles. Headless egui input regressions reproduced real unsafe editing behind dialogs/loading and stale undo snapshots, then passed after fixes. Normal canvas input and app save/reopen flow are covered as positive controls.
- Fresh native review and full implementation review were completed. Confirmed issues with external Save As, undefined IntGrid values, scalar bounds, entity reference deletion, property history, modal input and per-cell quadratic validation were fixed with regression tests.

## Actual native runtime evidence

The first real window test exposed a missing native wgpu backend, despite successful compilation. Added platform backend selection and an observed failing→passing regression. Rebuilt runtime test then used the **Apple M6 / Metal** adapter and saved `native-smoke.png`: 64 frames in 1.047 seconds for the official TopDown sample; the captured frame reports 1,300 visible tiles and approximately 0.51 ms of application CPU time. This is a small-sample smoke test, not an original-versus-port latency benchmark or a full fluidity guarantee.

Native desktop automation successfully inspected installed LDtk 1.5.3 and opened its TopDown example. Launching the new app through that automation interface timed out; the subsequent application-owned runtime harness provided actual Metal rendering evidence. Complete side-by-side visual and manual interaction comparison remains pending.

## Decisions and remaining limits

- The user explicitly requested immediate execution, so design/plan recording did not introduce another approval pause.
- Existing property forms clear prior brush history after changes. This prevents stale whole-level snapshots from destroying later property edits; undoable property transactions remain pending.
- Save As involving external-level path changes or relative asset rebasing fails explicitly. Full copying/rebasing remains pending.
- Existing rule-generated tiles display, but edits requiring deterministic rule regeneration fail explicitly until the rules engine is ported.
- Native screenshot is evidence of actual rendering, not evidence of pixel-identical visual fidelity. The full objective remains open in parity.md.

## Final verification of this stage

- `scripts/cargo test --workspace --locked --offline`: 52 passed (36 core + 16 desktop), optional corpus ignored in that default run.
- Explicit upstream corpus: 49 project documents and 3 external levels retained all JSON values.
- Strict workspace/all-target Clippy and rustfmt passed. One upstream `block` dependency emits a Rust future-compatibility notice; current builds and lints pass.
- Release binary and ad-hoc signed local macOS app bundle built; deep/strict signature verification passed.
- Full objective remains incomplete. Next major work is deterministic autotile computation, complete editing/clipboard/definition workflows and exports, then visual and large-project latency comparison.
