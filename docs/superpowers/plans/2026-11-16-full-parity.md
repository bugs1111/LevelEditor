# Complete native LDtk parity implementation plan

> Continue the existing all-Rust implementation autonomously. The user explicitly requested all remaining features, original assets/examples, and verification until complete. No design or execution approval pause is needed.

**Goal:** Complete the remaining functional, visual, workflow and responsiveness parity against upstream commit `2b7b5512f2f88fa281cb6492debbb26ead7838d4`, retaining an honest acceptance ledger.

**Architecture:** Keep authoritative complete JSON in `ldtk-core`. Independent core modules provide deterministic rules, document/selection/field workflows, and filesystem/export/import services. The desktop uses these APIs rather than reimplementing serialization. Native UI uses original assets and follows original templates.

**Requirements and acceptance scope:** the user's full-port instructions and `docs/status/parity.md`.

## Global constraints

- Entire application including UI is Rust. Upstream code/resources are references/data; no upstream JS/Haxe runtime is shipped.
- Reuse upstream icons, fonts, atlases and all examples, preserving notices.
- Data integrity, stable IDs/references and meaningful undo/redo apply to every new edit.
- Fixed seed rule output must match upstream behavior; screenshots must be actual program renders.
- Do not mark a feature complete solely because code or unit tests exist. Record original-versus-port evidence.
- Independent workers own separate files; the primary agent owns module registration, integration and desktop files. Workers do not commit or spawn further workers.

## Tasks and contracts

### A. Deterministic autotile engine

Files: `crates/ldtk-core/src/autotile.rs`, `tests/autotile.rs`, optional narrowly related helper files named `autotile_*`.

API: `Project::rebuild_auto_layers(key: LevelKey) -> Result<()>`; `Project::rebuild_auto_layers_area(key: LevelKey, source_uid: i64, rect: [i32;4]) -> Result<()>`; deterministic RNG/noise helpers as needed. Consume full layer definitions, source IntGrid, optional groups, biome fields and existing atlas metadata. Produce canonical `autoLayerTiles` with correct IDs/coordinates/flips/order/alpha. Do not edit existing `edit.rs`; primary integration removes the earlier guard only after engine tests pass.

- [x] Port matching, groups/biomes/optional rules, modulo/checker, flip symmetry, break-on-match, random tile rectangles/offsets and Perlin from original.
- [x] Observe failing behavior tests; validate against official examples and original compiled implementation or fixture outputs, including boundary cases.
- [x] Add incremental area invalidation and record correctness/performance evidence.

### B. Complete data editing workflows

Files: `crates/ldtk-core/src/workflow.rs`, `src/field_edit.rs`, `src/selection.rs` (all under the core crate), `src/history.rs`, tests with corresponding names. Primary owns existing `edit.rs` and `definitions.rs` integration.

APIs: transactional `Project::edit_document` helpers; world/level add/duplicate/remove/move/resize; layer reorder/remove/rename/property updates; full custom field definition/value conversion; entity ref and toc regeneration; region clipboard/selection APIs; project-wide history with `History::begin_project(&Project)` alongside the compatible existing begin/commit API. Export intuitive exact signatures in the worker report before desktop wiring.

- [x] Build each family test-first from actual original data/source, preserving unknown values.
- [x] Cover every field type and arrays, defaults/nulls/ranges/editor wrappers, enum definitions and propagation.
- [x] Cover reference updates and project-wide undo/redo, move/delete, region clipboard/stamps/flips.
- [x] Cover world/level and definition workflows and remove old explicit limitations only after integration tests.

### C. Project filesystem and format services

Files: `crates/ldtk-core/src/project.rs`, new `exports.rs`, `imports.rs`, `enum_sync.rs`; core Cargo.toml if image dependency is needed; corresponding tests. Do not edit other workers' files or `lib.rs`.

APIs: preserve `Project::load/save` signatures; support independent external-level Save As, relative resource rebasing/copying, backups/retention, external changes/conflicts; `Project::export_*` for original formats (PNG/image exports, CSV, simplified JSON, TMX, GameMaker); Ogmo import; external enum synchronization. Report signatures before desktop wiring.

- [x] Test failures and rollback, resource resolution, original format options and expected output artifacts.
- [x] Port original exporters/importers rather than inventing simplified incompatible formats.
- [x] Verify outputs using original examples/reference fixtures and actual file round trips.

### D. Native editor integration and visual/workflow fidelity

Files: desktop `src/`, assets/examples and packaging scripts. Owned by primary.

- [x] Wire core services into real native forms for project/world/level/layer/entity/field/enum/tileset/rule workflows.
- [x] Add rectangle/stamp/random/stacking, selection/clipboard/transforms, entity resize/move and original shortcuts/navigation.
- [ ] Port complete rendering of backgrounds/parallax/entity tiles/text/fields/overlays and upstream layout/styling/assets.
- [x] Add command palette/search, settings, file changes and actual import/export actions.
- [x] Keep desktop headless event-flow tests and real GPU screenshot harnesses; exercise files through actual save/reopen workflows.

### E. Acceptance and release

- [x] Independent review and resolve concrete findings with regression tests.
- [ ] Run full workspace tests, original corpus, format/lint/build checks and native runtime tests.
- [ ] Compare original and Rust screens at the same viewport/DPI/zoom, replay editing flows and compare saved JSON/exports.
- [ ] Measure release-mode input/render/save/load/memory on original large fixtures; improve measured regressions.
- [ ] Verify platform builds and applicable integrations. Every outstanding row in parity.md must have actual evidence before claiming complete.

## Progress ledger

Use `docs/status/full-parity-progress.md` for task evidence, worker ownership, exact API contracts, decisions and unresolved differences. The previous initial-stage completion is not completion of this plan or the user's objective.
