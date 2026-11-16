# LDtk native Rust port

The user explicitly chose a complete Rust rewrite, including the UI, retaining LDtk's full feature set, visual appearance, workflow and responsiveness, and instructed execution without further design approval. This document records implementation decisions, not a reduction of that request.

## Baseline and approaches

Reference repository: https://github.com/deepnight/ldtk at immutable commit `2b7b5512f2f88fa281cb6492debbb26ead7838d4`. Its app version is 1.5.3; master also contains additions beyond the published release. The local checkout under `.reference/ldtk` is reference material only and is not shipped or executed.

Alternatives discussed: Rust core with retained HTML/CSS frontend (easier visual reuse), and an entirely native Rust application (requires reproducing widgets and rendering). The user chose the latter. No Electron, embedded webview, JavaScript or Haxe runtime in the product.

## Architecture decisions

- Native window and GPU rendering through pinned eframe/egui 0.33 and wgpu. Use upstream colors, fonts, icons, left sidebar and panel organization as reference, rather than redesigning the editor.
- A separate `ldtk-core` crate owns project I/O and editing operations, independent of the UI and GPU.
- Keep complete project JSON values, including unknown fields, rather than using the simplified game importer format. Edits touch specific fields only. Separate `.ldtkl` files retain their external-file representation.
- Render original atlases with nearest-neighbor sampling, layer order, tile flips, offsets and opacity. Cache textures and avoid reloading/decoding them each frame. Clip rendering to the viewport and request redraw only on input/background work.
- First establish verifiable project round trips and a working native editor; then extend coverage using the feature ledger. A runnable initial build is explicitly not full parity.
- Default targets are macOS, Windows and Linux. Only platforms actually tested may be reported as verified.

## Full acceptance scope

Project creation/open/save/save-as, backup/recovery, external levels, migrations; all world layouts and multiworld; IntGrid/Tiles/AutoLayer/Entities; tilesets, stamps, random modes and tags; complete autotile rules including seeded randomness and Perlin; entities, all custom field types, enums and external enum sync; paint/erase/fill/selection/copy/paste/transforms; complete keyboard/mouse interaction, undo/redo and modal workflow; resource reload and watchers; all exports/imports and external commands; settings, accessibility and help; any baseline-specific embedding/update integrations need an explicit equivalent or recorded decision, never silent omission.

## Verification

- Original → Rust save → original: compare full JSON semantics, IDs, fields and references, including external levels and unknown extension values. Maintain fixture attribution.
- Fixed rule inputs and seeds must generate identical tile IDs, coordinates, flips and ordering. Preserve the upstream algorithm; generic random/noise substitutes do not establish parity.
- Compare screens at fixed window size, DPI, active tool and zoom, using upstream colors/fonts and explicit image tolerances. Screenshot fidelity remains unverified until an original executable can be run side by side.
- Replay actual editing flows and undo/redo; compare resulting project data. Unit tests alone do not establish GUI parity.
- Measure release-mode brush response, viewport motion, loading/saving and memory on the same hardware/corpus as original. Target interactive frames within 16.7 ms at 60 Hz; report percentiles and dataset sizes, never infer fluidity from language choice.

## Delivery rules

Track implemented, partially implemented and pending features honestly in `docs/status/parity.md`. Keep build/run commands reproducible. No disabled UI control masquerades as a completed feature. No full-parity or performance guarantee until the associated comparisons pass.
