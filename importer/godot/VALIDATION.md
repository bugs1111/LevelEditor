# Godot importer validation — 2026-11-22

Host: macOS. Engine: the user's existing `/Applications/Godot.app/Contents/MacOS/Godot`, `4.7.2.stable.official.ed1daf0bf`. No copied Godot executable is required or retained. Upstream source: `0ecab8d1cc6531801b487157848059c3b8971aa0`.

## Reproduce

```sh
python3 importer/godot/tests/run.py
```

The final run passed six scenarios / twelve engine invocations:

| Scenario | Actual checks |
| --- | --- |
| LDtk 1.5.4 | Real editor import, PackedScene instantiation, tile atlas/coordinates/flip, entity identifier/local position, typed scalar values and nullable arrays. |
| Reimport | Source changed from 2 to 1 tile and health 25 to 50; loaded scene contains the new values. |
| LDtk 1.5.3 | Same actual import/scene checks against the older format version. |
| External level | Main file has null layerInstances; separate `.ldtkl` supplies the actual level and entities. |
| Multi-world | Both worlds and their actual levels survive scene packing in source order. |
| Multi-world tileset override | Second world's 16px layer uses an 8px atlas override; imported cells use the correct source and atlas coordinates. |

Every project is installed in a temporary directory with spaces by copying only the addon plus test project resources. Source `.ldtk`, `.ldtkl` and PNG hashes are unchanged by import. Import and load subprocesses must exit successfully, without SCRIPT ERROR, ERROR, or invalid-UID warnings. The real Entity Post-Import callback checks non-null and empty enum arrays as `Array[String]`; scalar metadata records that result in the packed scene.

The test runner also starts the default `demo.tscn` scene in its own headless Godot invocation after the first import. Use `just demo-godot` to open the demo project in the editor and `F5` to run that scene.

Raw logs and machine-readable results are at `.tools/godot-importer/`; the directory is intentionally not committed. The test fixture also loaded successfully through the existing LevelEditor MCP `Project::load` and exposed its definitions; this is a document-load check, not a second Rust build or an exhaustive format validation.

## Failures observed before fixes

- No addon: `.ldtk` has no resource loader and cannot instantiate as a PackedScene.
- Upstream array conversion: null Point/Color/Tile/EntityRef entries raised errors; unconverted integer arrays retained JSON number types.
- Temporary generated node trees were not freed, producing leaked CanvasItems/resources at editor exit.
- Generated resource UIDs were unresolved and fell back to text paths.
- Multiple worlds tried to access a nonexistent `depth` property; after removing that access, packed scenes still omitted level nodes until ownership was corrected.
- The local generic-array fix lost upstream `Array[String]` during Post-Import. A failing real callback test preceded restoration of typed non-null and empty local-enum arrays.
- Multi-world override collection scanned only root levels, producing `TileSetAtlasSource missing`. The failing second-world override case preceded traversal of all worlds' levels.

One independent read-only review found the enum and multi-world override issues plus installation/documentation gaps. Both code findings were fixed with observed failing→passing regressions and the full suite passed afterward.

## Decisions and remaining boundaries

- An attempted temporary self-contained Godot copy triggered macOS verification. All those executable copies were deleted, and all accepted test evidence uses the original installed Godot. No security exception or Gatekeeper change was needed.
- Generated resource names retain upstream behavior. Each LDtk project must have its own directory to avoid collisions; unrelated game projects were not modified.
- External-only `.ldtkl` edits require manually reimporting the parent `.ldtk` or saving the parent again. No new file watcher was added.
- Multi-world output remains an aggregate scene. Separate per-level output is single-world-only; upstream cross-level EntityRef limitations remain documented.
- A separate minimal plain Node/PackedScene test showed Godot 4.7.2 binary scene serialization changing nested `Array[String]` type metadata from 4 to 0. Values survive; tests assert the plugin's original typed contract in actual Post-Import and check loaded values separately. Runtime typed arrays may need `assign()`.
- Deferred minor: first persistent editor opening can generate normal `.gd.uid` files. These were not pre-generated for the pinned upstream copy; developers may version them with their scripts.
- No visual/GPU performance comparison, all-platform Godot support, every upstream option, Aseprite or embedded-atlas compatibility is claimed by these small PNG fixtures.
