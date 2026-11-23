# Upstream provenance

- Project: [heygleeson/godot-ldtk-importer](https://github.com/heygleeson/godot-ldtk-importer)
- Pinned commit: [`0ecab8d1cc6531801b487157848059c3b8971aa0`](https://github.com/heygleeson/godot-ldtk-importer/commit/0ecab8d1cc6531801b487157848059c3b8971aa0)
- Original author: Andrew Gleeson
- License: MIT, Copyright (c) 2023 Andrew Gleeson. The complete notice is retained in [`addons/ldtk-importer/LICENSE`](addons/ldtk-importer/LICENSE) and must accompany redistributed plugin copies.
- Import method: upstream `addons/ldtk-importer/` source files are included directly; no Git submodule or automatic upstream update is required.

The upstream plugin name, author, node classes, install path and Post-Import interfaces are retained. This is an integrated copy with local compatibility fixes, not an upstream release.

## Local changes

- `src/util/field-util.gd`: convert array elements using the same parser as scalar fields, preserving nullable Color/Point/Tile/EntityRef values and integer element types; retain upstream `Array[String]` for non-null and empty local-enum arrays.
- `ldtk-importer.gd`: free temporary source and imported node trees after packing; complete reference cleanup for multi-world imports; register generated level resource UIDs.
- `src/world.gd`: preserve the input world order instead of accessing a nonexistent world `depth` property; assign generated nodes to the packed multi-world scene so levels are not silently omitted.
- `src/util/util.gd` and `src/tileset.gd`: register generated resource UIDs before loading saved scenes, preventing invalid-UID path fallback warnings on a fresh import; collect tileset overrides from every world's levels.
- `src/level.gd`: removed whitespace on blank lines.

The small test map derives its document/default field structure from the repository's LDtk TopDown sample; the original LDtk MIT notice is retained at the repository root. The two-color test PNG is generated specifically for this integration and does not contain sample artwork. The source map, validation scripts, project configuration and integration documentation are local additions.
