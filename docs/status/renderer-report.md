# Native renderer and library verification — 2026-11-16

## Implemented and integrated

The native renderer uses the pinned original LDtk display/data source and the installed original 1.5.3 compiled Heaps functions. It keeps canonical project JSON read-only while drawing.

- Original bitmap font XML advances, glyph bearings and PNG glyph quads. Heaps word/CJK wrapping (unbroken Latin words overflow), DPI font selection, outlines and rotated labels.
- Entity Rectangle/Ellipse/Cross/Tile, pivots, instance dimensions, smart colors, smart tiles from field/default values, tile opacity and original inactive opacity. Shared palette entity previews use the same geometry and actual original atlas pixels.
- Stretch, FitInside, Cover, FullSizeCropped, FullSizeUncropped, Repeat and NineSlice tile modes, including cropped edge fragments and unscaled nine-slice corners.
- Layer offsets, parallax/scale, opacity, tile flips, original asymmetric grid-tile Y pivot, generated tile alpha, IntGrid colors, hidden layers, grid/guide/ruler overlays and tile enum tags.
- Background unscaled/crop/fit/repeat geometry calculated from current editable settings instead of trusting stale exported __bgPos.
- Field name/value modes, prefixes/suffixes, quoted strings, array counts/icons/colors/vertical arrays, display scale, table/outline layout, names and smart colors; radii, points/stars/paths/loops; all five reference-link styles, outgoing and cross-level incoming links, cross-world/depth cut endings. Invalid hidden/default fields retain original validation labels, errors recolor the corresponding field group, and table/outline colors follow the original. A lazy per-frame reference index replaces per-reference project scans.
- Source world tile-average ARGB4444 colors and regeneration from alpha-weighted RMS decoded pixels, topmost-stack selection and top-layer cell occlusion including pure IntGrid, level-tile overrides, depth sorting/fading/outlines, level smart colors/identifier plates and level-field flows, world grids/axes, original striped background and 90px title font, rotated horizontal identifier plates, nearby edge tiles and simplified separated neighbors through the original 300px boundary.
- GPU inactive-layer filter: capture each composed layer, use a private egui-wgpu renderer, apply original colorization matrix and Heaps paired-tap separable Gaussian, then composite. Two shared scratch surfaces plus one retained output per inactive layer; old targets are retired and disabling the mode drops filter resources. Newly uploaded textures can use ordinary shapes for their first frame until GPU handles exist.
- Field-flow outlines use the original Heaps Outline2D shader, with 60 radial samples, two passes, full alpha, original premultiplied formula and per-field base color. Labels, value flows (including icons/color swatches), and entity identifiers are cached as GPU textures; unchanged flows skip compositing/filter passes. The retained cache targets 64 MiB, evicting older frames while retaining the current frame working set. The shared scratch renderer submits each cache miss before reusing its buffers. Nested inactive-layer RGBA targets propagate their format rather than assuming the window BGRA format. Headless/no-GPU drawing retains the simpler geometric fallback; newly uploaded textures can show unfiltered text for one frame.
- Asset decoding uses the core image loader (including Aseprite), two workers and bounded queues, generation-safe invalidation, a 128 MiB texture/raw-pixel/alpha-mask cache, visible reloadable atlas errors, cached precise tile-opacity queries, average-color memoization and one-shot successful decode-size events for metadata reload handling. An unassigned atlas is distinguished from an explicitly embedded atlas.
- Native library: original logo/home header, all bundled examples, recent projects and recent directories; searchable/grouped IntGrid values and entity definitions, source palette colors, allowed/excluded entity-tag filtering, actual previews, embedded/external atlas picker, rectangular drag selection and tile tags, random/stack/flip controls, optional auto-rule groups.

Integration APIs:

- `canvas::configure_gpu(cc.wgpu_render_state.as_ref())` at app creation.
- `canvas::entity_preview(ctx, painter, rect, project, definition, assets)`.
- `canvas::{world_to_layer, layer_to_world}(project,key,index,viewport,point)` use the exact same offset/parallax/scale transform as rendering.
- `canvas::draw_selection_ghost(ctx,painter,canvas,project,viewport,key,group,delta_px,flips,assets)` draws mixed clipboard cells/tiles/entities/field points without cloning the project.
- `Assets::{texture, embedded, builtin, invalidate, clear, stats, tile_is_opaque, embedded_tile_is_opaque, average_tile_color, take_loaded_images}`.
- `EditorApp::{library_palette, atlas_picker, library_home}`; `library::entity_allowed` for keyboard selection filtering.

## Verification evidence

- Earlier native binary verification passed 51 tests with 3 explicit ignores. The expanded renderer verification and current integration status are recorded below.
- Strict native Clippy passed after the initial renderer/library fixes. The latest run is blocked only by the concurrently edited root helper `selection_world_delta` being unused; it reports no owned renderer warnings.
- Four original examples render real tessellated meshes in both level and world modes without JSON mutations or asset errors: Entities, Mosaic, Biomes and Typical TopDown. Last recorded level-mode visible counts: 1,807 / 852 / 3,265 / 1,497. These are rendering checks, not whole-image pixel equivalence assertions.
- Actual Metal GPU pixel test passed. It composites a red rectangle, applies the original matrix/Gaussian, reads back the texture, and checks interior RGB, transparent exterior and blur crossing the original rectangle boundary. Output: `/private/tmp/ldtk-gaussian-gpu.png`.
- Geometry regressions cover all tile modes, clipping, original grid-tile asymmetric pivot, repeating background alignment, bitmap font metrics/wrapping, default-field visibility, short arrow links, entity preview alpha, atlas hit-testing/drag direction and tag filtering.
- Asset tests cover embedded atlas reuse/invalidation, stale decoder results, alpha 255 versus 254/0, outside-image queries and uncached images.
- Native Metal screenshots: `native-entities.png`, `native-single-layer.png`, `native-large-project.png`. Entities and TopDown were viewed and confirmed to contain the actual project/tiles/fields/palettes; the replacement large capture was viewed and confirmed to show world1 / Level_0, 43,002 visible cells and 2.94 ms displayed UI CPU time.

### Corrected smoke acceptance

The first large-project screenshot was a loading spinner: the old harness captured at frame 60 regardless of readiness. That artifact and the claim that its 0.571 s run proved rendering were invalid. The harness now waits for `capture_ready()` and **60 consecutive ready frames**, requests the screenshot once, and fails with a nonzero exit code after 60 seconds. A successful capture exits without triggering migrated-document save prompts. Home capture is explicitly identified by absence of a project argument. `LDTK_SMOKE_OUTPUT` selects a per-fixture file.

The replacement large screenshot was captured at 2.117 s after startup, including load/migration/rebuild. This total is not a frame-time benchmark. The currently retained `native-large-project.png` is the replacement, not the spinner.

### Independent timings

Release, original `tests/lotsOfEntities.ldtk`: 30 warmed headless draw frames in 129.6 ms, approximately **4.32 ms/frame**. This measures CPU scene/mesh construction, not GPU presentation.

The real native Metal run after outline integration recorded 44 warmed `app.update` CPU samples: **median 3.798 ms, p95 3.876 ms**. Startup-to-capture was 1.142 s, including load. `native-lots-of-entities.png` was viewed and contains the loaded entity grid and labels. These CPU measurements are not GPU frame rates.

Release, original 13,272,803-byte `tests/largeLevels/world1.ldtk`: load 62.8 ms; explicit migration 143.4 ms; five level auto-layer rebuilds 53.2 / 246.9 / 334.6 / 467.8 / 314.1 ms. About 1.42 s of rebuild work was avoidable for reusable saved 1.5 caches. A dedicated ignored core benchmark records these stages independently.

Load recommendation: preserve saved autoLayerTiles for source version >=1.5 when the relevant instances have arrays; an empty array is valid. Rebuild older/missing caches, or after rule/source edits. Migration's boolean also includes non-render defaults and is not by itself proof that caches require recompilation. Preserving an authored cache does not prove that cache was fresh when its author saved it.

## Follow-up verification

The four previously listed renderer omissions are implemented: world background/title and separated neighbors; missing average cache regeneration; invalid field/error presentation; and the rotated identifier plate. Additional source auditing corrected transparent topmost tile-stack handling, layer-instance offset contribution to parallax, and the original Outline quality-versus-alpha distinction.

- `renderer-oracle.json` contains 64 pixel-average cases, 64 level-bound distance cases and 9 validation cases generated by **executing methods extracted directly from installed original 1.5.3 renderer.js**. All 137 vectors match the Rust implementation. Original renderer.js SHA256: `5edc5e13ec407897f741c04975497958b445b50b8794b47eb94088602508655d`. Regenerate with `node docs/status/renderer-oracle.cjs /path/to/extracted/renderer.js`.
- Source outline shader: [Heaps Outline2D](https://github.com/HeapsIO/heaps/blob/master/h3d/shader/Outline2D.hx). Its algorithm agrees with the installed compiled shader/pass parameters. The actual Metal regression checks opaque interior, full-alpha two-pixel outline, transparent exterior, rounded corner coverage, cached flow reuse, and RGBA composition under a BGRA outer window.
- Original striped resource is the `largeStripes` slice (0,0,234,234) of the pinned `res/atlas/appElements.aseprite`, copied losslessly to `assets/img/world-large-stripes.png`. Title glyphs use the copied original `noto_sans_display_semicondensed_extralight_90` XML/PNG.
- The expanded native test run passed **77 tests with 4 explicit ignores** before subsequent root additions. The latest shared-workspace run passed 77 tests and failed the root’s in-progress resource/save-baseline regression; all renderer tests passed. A source-only renderer run passed 37 tests before adding the GPU outline test. Final inclusive core autotile verification passes all 23 tests, including 57,682 official saved tiles and 66 incremental/full checks; strict core Clippy passes.
- Updated original corpus check passed: Entities 1,807/120 mesh batches; Mosaic 852/8; Biomes 3,265/8; TopDown 1,497/42. Both level and world modes draw meshes without mutating project JSON or producing asset errors.
- Real `native-world.png` was viewed and shows loaded TopDown world levels, striped backdrop and original title. `native-entities.png` and `native-single-layer.png` were viewed after GPU outline integration; single-layer capture verifies outlined fields nested inside Gaussian layer composition. The first nested run exposed a RGBA/BGRA pipeline mismatch and failed; the subsequent format propagation fix passed both the explicit Metal regression and the real app rerun.
- `native-entity-def.png` opens the real E shortcut after readiness, at 1440×900 logical / 2880×1800 physical pixels. `native-home.png` is the corrected, viewed content capture. The first home capture was blank because the parent had consumed its layout cursor; a failing regression reproduced it and the home child-UI fix passed before recapture.
- `LDTK_SMOKE_KEY` supports E/P/C/L/T/U/H/Backtick once after readiness. The harness waits 60 stable frames after input and records warmed app.update CPU samples separately from startup duration.

These checks establish implemented source behavior and real GPU execution, not exhaustive whole-image equality for every editor state. Original/native screenshots still have different camera/chrome arrangements; panel alignment and the overall acceptance audit remain coordinated by the root agent. Strict whole-bin Clippy currently reports only a concurrently edited root file; the renderer-only strict run passed and the new shader files add no reported warnings.

Latest logs: `/private/tmp/renderer-final3-tests.log` (one root regression failure), `/private/tmp/renderer-final3-corpus.log` (pass), `/private/tmp/renderer-final3-clippy.log` (root unused helper), `/private/tmp/autotile-final-integration.log` (pass), `/private/tmp/autotile-final-clippy.log` (pass), `/private/tmp/native-outline-lots.log` (native CPU samples).

Earlier logs: `/private/tmp/renderer-complete-tests2.log`, `/private/tmp/renderer-remaining-corpus.log`, `/private/tmp/outline-tests.log`, `/private/tmp/outline-gpu-nested.log`, `/private/tmp/native-outline-single-fixed.log`, `/private/tmp/atlas-event-green.log`, `/private/tmp/rule-move-green.log`. Earlier timing logs remain `/private/tmp/render-release-perf.log` and `/private/tmp/auto-large-perf.log`.

## Selection and brush ghost source audit

`GhostMode::{Move, Copy, Brush}` and `draw_group_ghost(..., mode, assets)` distinguish original `GenericLevelElementGroup` ghosts from `ui/Cursor` previews. `draw_selection_ghost` remains a Move-mode wrapper. No moving-entity visibility flag remains: source `onMoveStart` builds the ghost, and `showGhost` hides only selection decoration, leaving the original elements visible. Root owns deferred entity movement and movement-arrow interaction.

- Source selection grid/tile ghosts have alpha 1; entity **core plus pivot** has alpha 0.5. Cursor entity previews use 0.33. Names and ordinary field flows are excluded from these cores.
- Brush IntGrid cells use the original outer colored and inset black strokes, without a filled cell. Selection IntGrid cells remain filled.
- Cursor tiles use atlas center-ratio pivots and the rounded grid-difference scale. Selection-group tile bitmaps have no tile pivot and retain source bitmap size. Their flipped anchors and UVs are now handled separately from ordinary LayerRender geometry.
- Geometry-aware flips mirror inside each entry's bounds, matching the core selection transformation instead of dropping its flip positioning.
- Selected point ghosts use full-color outer/inner circles. Move mode draws original straight point-link segments between moved entities, selected points, and stationary neighboring points; Copy suppresses those links. Source PointPath's selected-adjacent-point `pxParallaxX`-for-Y quirk is preserved explicitly.

The new `render_ghost.rs` contains these shared drawing details. Five focused egui render tests pass (`/private/tmp/ghost-tests-final.log`), including exact entity alpha in all three modes, exclusion of name meshes, preservation of the normal scene, distinct brush/selection tile bounds and flipped UVs, geometry-aware flips, outline-only brush cells and Move/Copy point-link behavior. The initial grid-alpha regression failed at 153 versus 255 before correction (`/private/tmp/ghost-alpha-red.log`). Tests inspect real shapes/vertices and preserve canonical project JSON. Root owns the final consolidated full suite; the local strict Clippy attempt after helper fixes is blocked only by the concurrently edited level-resize `filter(...).next_back()` diagnostic (`/private/tmp/ghost-clippy-final.log`).
