# LDtk Native Foundation Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans for integration, with one isolated core implementer and a fresh whole-change reviewer. The user explicitly instructed execution; no further approval pause.

**Goal:** Start the complete port with a runnable all-Rust editor and a verifiable, lossless project core; retain the full parity roadmap.

**Architecture:** `ldtk-core` preserves complete LDtk documents and owns data editing and history. The native desktop app renders those documents using cached GPU textures and custom egui panels. No web runtime.

**Tech Stack:** Rust 2024, eframe/egui 0.33.0, wgpu, serde_json, image, rfd.

**Spec:** `docs/superpowers/specs/2026-11-16-rust-port-design.md`

## Global Constraints

- Baseline commit: `2b7b5512f2f88fa281cb6492debbb26ead7838d4`.
- Entire application, including UI, is native Rust.
- Do not silently discard unknown JSON fields or inline externally stored levels on save.
- Original assets retain their notices; sample art has independent attribution.
- Do not claim complete feature, visual, interaction or performance parity without evidence.

## Review Focus

1. External `.ldtkl` documents must remain external after editing and saving.
2. Saving must not destroy existing project files on serialization or write failure.
3. Long pointer drags must form one undo action and fill skipped grid cells.
4. Pointer zoom must preserve the world position beneath the cursor.
5. File changes, missing assets and unsupported operations must be visible rather than silently corrupting data.

### Task 1: Lossless project core and editing history

**Files:** `crates/ldtk-core/src/{lib,project,edit,history}.rs`, `crates/ldtk-core/tests/`, `tests/fixtures/`.

**Interfaces:** `Project { pub json: Value, pub path: Option<PathBuf>, pub external_levels: BTreeMap<String, Value> }`; `LevelKey { world: Option<usize>, level: usize }`; `Project::{new,from_json,load,save,level_keys,level,level_mut,layer,layer_mut,layer_def,tileset,paint_int_grid,paint_tile,add_entity}`; `History::{begin,commit,undo,redo,can_undo,can_redo}`. Core implementer owns these files; desktop consumes them.

- [x] Write tests for unknown-field round trips, separate levels, bounds-safe IntGrid and tile edits, entity placement, dirty/save behavior and stroke history. Use real temporary files and original fixtures.
- [x] Run `cargo test -p ldtk-core`, record meaningful failing assertions before implementation.
- [x] Implement minimum correct behavior without UI dependencies. Atomic file replacement; explicit errors for malformed documents.
- [x] Run the core tests and original fixture corpus; record coverage and limitations.

### Task 2: Native window, viewport and input

**Files:** `src/{main,app,canvas,theme}.rs`, `tests/viewport.rs`, `assets/`, `scripts/`.

**Interfaces:** Consume Task 1 data API. `Viewport` provides world/screen conversion, cursor-anchored zoom and visible bounds. App state owns active level/layer, tool, palettes and texture cache.

- [x] Write viewport tests for coordinate inverses, cursor-anchored zoom, clipping and continuous brush interpolation; observe failures.
- [x] Implement native GPU window, upstream colors/font, left sidebar, tool buttons, project dialogs and cached atlas rendering.
- [x] Add level/world navigation, layer visibility, tile/IntGrid/entity palettes, draw/erase, grouped history, fit/pan/zoom, grid and shortcuts. Unsupported workflows remain pending in the ledger.
- [x] Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --release`.
- [x] Launch the executable, inspect the rendered window and exercise a saved/reopened edit. Capture a screenshot if available.

### Task 3: Coverage, review and reproducible delivery

**Files:** `README.md`, `docs/status/{parity,progress}.md`, `THIRD_PARTY_NOTICES.md`.

- [x] Record exact implemented/pending matrix and build/runtime measurements.
- [x] Obtain a fresh code review of data integrity and native input/render integration; fix material findings and rerun affected checks.
- [x] Record actual verification evidence and remaining parity work. Keep the objective open until full acceptance is satisfied.

## Initial-stage verification outcome

52 workspace tests passed; the optional upstream corpus also passed for 49 projects and 3 external level documents. Strict Clippy, rustfmt, release build and local macOS bundle signature verification passed. The real Metal runtime harness generated a screenshot. Full side-by-side visual comparison and full original feature parity remain pending in `docs/status/parity.md`; completing this bounded foundation plan does not complete the overall port.
