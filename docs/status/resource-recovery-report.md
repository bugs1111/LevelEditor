# Resource reload, relocation and pixel cache verification

## Implemented behavior

`src/editor/resources.rs` now retains reload failures in `resource_missing` and exposes:

- `changed_document_files() -> Vec<PathBuf>`: compares actual main/external document identities against the saved filesystem snapshot. An enum `.json` is a resource, while a main `.json` is a document. Native Save uses this API.
- `resource_dialog(ctx)`: displays the failed path/error, Locate, Retry and the original dismissal choices. Missing files use “Fix that later”; decode errors use “Ignore”. Choosing a different filename requires confirmation, matching `LostFile.hx`.
- `retry_resource(ctx, path)`: clears the selected failure, invalidates its cached decode generation, and requests loading again. Enum sources retry synchronization instead of image decoding.
- `relocate_resource(ctx, old, new) -> bool`: changes every matching tileset, level background, derived layer atlas path and external-enum source, including loaded external levels, as an undoable recovery action. It reloads/synchronizes the new source and preserves enum identities. Invalid destinations leave the document unchanged.
- `ignore_resource(path)`: dismisses only that failure without modifying the project or its broken resource reference.

Initial resource scanning includes backgrounds and missing enum files. Repeated failures merge by normalized path. A successful decode clears the corresponding failure. Read-only backup inspection does not offer relocation for missing files. Root integration clears failures on project switch and calls the dialog globally.

Resource retries preserve unresolved document conflicts instead of accepting a new baseline over external disk edits. Existing save-completion replay of changed resources remains covered by the original three tests, which were retained.

## Same-size artwork and source pixel data

Successful atlas decoding regenerates original `cachedPixelData` using the already decoded asset cache: precise opaque bits and alpha-weighted RMS colors, rounded to original ARGB4444 channel values. This fixes same-size artwork changes leaving stale world colors or auto-rule culling. No additional image decode is performed. Unchanged cache data skips document edits. Pixel-only changes rebuild auto layers only in levels using that atlas; geometry changes use the existing remap API.

Original source quirks are preserved: a partial edge cell retains opaque=true and color zero because `_parseTilePixels` does not enter the full-cell pixel loop. `intToHex3_ARGB` rounds each channel to the nearest value in 0–15, rather than truncating its high nibble. Border cropping now applies padding trimming once in the core remapper, rather than once in native code and again in core.

The source audit also found that native `Project::add_tileset` incorrectly rejected non-multiple image dimensions. With root authorization, only that function was changed: positive dimensions/grid are required, oversized grid is clamped to the smaller image dimension, and derived cell counts use overflow-safe ceiling division. This matches `TilesetDef.get_cWid/get_cHei` and `importAtlasImage`/`getMaxTileGridSize`. The old rejection test now uses an actually invalid zero-sized image.

## Source references and tests

Pinned upstream:

- `ui/modal/dialog/LostFile.hx`: Locate, different-name confirmation, “Fix that later”, recoverable relocation.
- `ui/modal/dialog/Retry.hx`: Retry and Ignore.
- `page/Editor.hx` around 298 and 436–464: missing enums, missing atlas/failed decode, backup restrictions.
- `data/def/TilesetDef.hx` lines 30/33, 84–85, 281, 657–693: ceiling dimensions, grid clamp, original pixel data.
- Installed original 1.5.3 `dn_legacy_Color.intToHex3_ARGB`: channel rounding and lowercase hex.

Five new resource behavior tests failed before implementation while the three existing tests passed (`/private/tmp/resource-missing-red.log`). The same-size cache regression independently failed with stale opaque bits “11” instead of “10” (`/private/tmp/resource-pixels-red.log`). The new core import regression failed on the incorrect multiple-of-grid restriction (`/private/tmp/atlas-import-red.log`).

Final targeted runs pass **14 native resource tests** (`/private/tmp/resources-final-all.log`) and **13 core definition tests** (`/private/tmp/atlas-import-green.log`). Coverage includes missing background/atlas/enum, decode retry after repairing a file, duplicate failures, identity-preserving relocation and undo, actual main/external conflict classification, preserving pending conflicts during retry, exact ARGB/edge behavior, single padding trim, and an opacity change causing auto-layer output to grow from one tile to two. Native file chooser interaction itself has not been driven in a real OS dialog; the selected-path operation and resulting document behavior are tested directly.

All-target workspace strict Clippy passed (`/private/tmp/resources-workspace-clippy.log`). The broader workspace test attempt stopped at the concurrently added core naming regression `names_are_unique_across_worlds_and_external_publication_keeps_iid_and_stub_in_sync`; its owning agents subsequently reported a fix and targeted pass. Root owns the final consolidated workspace run. Additional targeted resource verification now passes 14 tests, including opacity-driven auto-layer recompilation. No whole-workspace pass is inferred from the targeted runs.

The native-only full suite after resource integration passed 115 tests with 4 explicit ignores (`/private/tmp/resources-native-full.log`), before subsequent moving-entity renderer work.
