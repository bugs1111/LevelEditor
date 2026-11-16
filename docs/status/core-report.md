# Lossless Rust core verification

The core preserves complete JSON values, including unrecognized fields. It does not implement the migrations, exports or complete editor behavior of LDtk 1.5.3.

## Implemented

- New project from upstream empty-project template, with fresh UUIDs.
- Legacy root levels, multiple worlds, separately loaded external `.ldtkl` documents.
- Unknown JSON fields survive load/save; external levels remain external. Edited canonical external-level summaries update their root stubs without copying external-only extension fields or inlining layers.
- Save honors `minifyJson`, serializes and stages every output before publishing, replaces individual files with rename, and attempts rollback if a publication fails. The project path changes only after a successful save. A multi-file save is not a crash-atomic transaction.
- Bounds and shape checks before cell edits. IntGrid validates the touched cell without scanning the entire CSV and rejects nonzero replacement values absent from the target definition palette. Tiles validate assigned atlas IDs and source coordinates and preserve unrelated tile metadata.
- Entity placement uses definition dimensions, pivot, tags and basic field defaults. Per-layer, per-level and per-world count limits are checked. Same-level move-last preserves the entity IID and existing custom fields. Cross-level movement/discard requiring broader history is rejected.
- Entity deletion/discard refuses inbound scalar/array EntityRefs, including editor-value wrappers when exported values are stale. Referenced movement across layers is refused until reference metadata can be updated transactionally. Unloaded external levels prevent unchecked deletion.
- Undo groups edits between begin/commit, snapshots only the active level, retains thirty actions, preserves external level data, and leaves unrelated project/level changes alone.
- Layer creation (IntGrid/Tiles/Entities) instantiates every loaded level transactionally using upstream schema defaults. Tileset and entity definitions can be created. Compatible atlas assignment updates definitions and layer instances while preserving tile metadata.

## Explicit limitations

- Save As to any different project path is rejected when external levels exist, including a filename change within the same directory, until level files can be copied/remapped without modifying the original project. Save As into a different directory is also rejected when relative image/enum paths or FilePath fields/defaults require rebasing. Unsaved projects may retain absolute asset paths when their final location is unknown.
- Active autotile rules, including separate AutoLayers sourcing the edited IntGrid, reject grid edits until deterministic recomputation is implemented.
- Editing entities marked `exportToToc` is rejected until derived table-of-contents data can be regenerated while preserving unknown records and exact world coordinates. The UI must apply the same restriction to direct property/field changes.
- Full field/editor display behavior, legacy definition migration, tag-to-smart-tile selection, and advanced entity constraints are incomplete.
- Backup/recovery, external file conflict detection and crash-consistent multi-file transactions are pending.
- Atlas import currently requires dimensions divisible by its cell size; assignment requires matching layer/atlas grid sizes and rejects instance overrides or existing tiles requiring coordinate remapping.

## Verification evidence

The API scaffold first ran sixteen semantic tests: fourteen failed on missing behavior, two error-preflight tests passed trivially. Creation/assignment, minification/summary sync, relative FilePath rebasing detection, atlas validation, reference safety and toc restrictions each had observed failing assertions before implementation. Final review regressions also observed two failures before fixing same-directory external Save As isolation and undefined IntGrid palette values.

Final commands on the actual workspace:

- `scripts/cargo test -p ldtk-core`: **36 passed**, one upstream corpus test ignored by default.
- `scripts/cargo clippy -p ldtk-core --all-targets -- -D warnings`: passed.
- `scripts/cargo test -p ldtk-core --test upstream_corpus -- --ignored --nocapture`: passed; all **49 original project documents and 3 external levels** retained complete JSON semantics (8.94 seconds in the final debug run).

The corpus test deliberately clears the source path to test serialization at a temporary destination; it does not copy image assets or establish relocation fidelity. It requires the pinned checkout at `.reference/ldtk`; `LDTK_REFERENCE_TESTS` can select that test directory explicitly.

The empty-project fixture/template is copied from LDtk commit `2b7b5512f2f88fa281cb6492debbb26ead7838d4`; upstream MIT notices apply.
