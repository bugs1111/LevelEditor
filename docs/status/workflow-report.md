# Data editing workflow implementation evidence

## Native integration requirements

- `History::begin(project,key)` now snapshots all potentially affected documents; use `begin_project(project)` for definition, world, level and cross-level commands. `commit`, `undo`, `redo`, `can_undo`, `can_redo` retain their signatures. `cancel(&mut Project)->bool` restores a pending gesture or form edit without adding an action.
- IntGrid drawing/flood/rectangle hot paths should call `paint_int_grid_cells(key,layer,&[([x,y],value)])`. It validates once, deduplicates with last-write-wins, stages only changed cells, and runs one area rebuild. It does not clone the project. On rule errors both CSV and generated output remain unchanged. `paint_int_grid` delegates a one-cell batch.
- Manual tile hot paths can call `paint_tile_stacked(key,layer,x,y,tile_id,src,flips,erase,stack)`. It deduplicates equal ID+flip before raising to the top; stacked erase removes only the top tile. The caller supplies opacity-dependent stacking policy as in the original TileTool. `paint_tile` retains its original signature and uses non-stacking mode. Killer-layer caches rebuild atomically.
- Entity add/remove methods in `edit.rs` now use full field construction, project reference cleanup, TOC regeneration and cross-level count-limit moves. Native history must encompass the project for those operations.
- `add_world` enables canonical original MultiWorlds serialization: legacy root levels become `worlds[0]`, with `dummyWorldIid` retained. Existing `LevelKey.world=None` selections must become `Some(0)`; the first new world returns index 1. `enable_multiworlds()->Result<bool>` exposes migration independently.
- Geometry previews use exported allocation-free helpers `constrained_entity_size(def,requested,initial,snap)` and `snapped_entity_position(def,local,grid,snap)`. They apply original resize axes/aspect/ranges and entity placement pivot/coordinate rounding. `resize_entity` uses the same constraints. Native right/bottom resize handles must also compensate pivot-1 anchors when size changes.
- Definition panels can use exported `TilesetRemap::{TileIds,AtlasCoordinates,Clear}` with `assign_tileset_with_policy(layer_uid,Option<tileset_uid>,mode,clear_overrides,allow_loss)`, `set_layer_tileset_override(key,index,Option<uid>,mode,allow_loss)`, and `update_tileset_definition_with_policy(uid,patch,mode,allow_loss)`. Each returns a discarded-reference count. Default assignment keeps IDs/overrides; default geometry edits retain atlas cell coordinates. Loss requires an explicit choice, and original null holes in auto-rule stamps remain holes.
- Pointer entity dragging can update `px` inside a pending project history gesture and call `finish_entity_transform(key,layer,index)` at release. It returns `true` if the touched entity was removed, uses source offset/scaled inclusive layer bounds, tidies only that entity's Point bounds and repairs references/TOC. Clear the selected entity index when it returns true. Calling transactional `move_entity` for every pointer event clones the project; that low-level numeric setter itself does not finalize a gesture.
- Mixed selection gestures should retain the returned group from `move_selection_group_with_result` / `paste_selection_group_with_result`; their result carries exact selected cell masks, entity identities and independent Point indices. Moving resolves live selected values first, so form edits since capture are retained. Copying requires `refresh_selection_group` first when the source level remains live; clipboard snapshots themselves intentionally remain transferable.
- For camera-dependent layer geometry, use `copy_selection_group_from_regions(key,&regions,bounds_px)`, then `move_selection_group_with_geometry` / `paste_selection_group_with_geometry`. Regions are `{layerIndex,rect:[cx,cy,cw,ch],geometry:{origin:[worldX,worldY],scale},all?:true}`. Native code computes each cell rectangle with `world_to_layer`; geometry origin is the world position of local `[0,0]`, and scale is `layer_to_world([1,0]).x-layer_to_world([0,0]).x`. Pass current `[{layerDefUid,origin:[worldX,worldY],scale}]` to geometry move/paste. No layer is implicitly excluded for parallax; caller applies original visibility/`canSelectWhenInactive` policy.
- Definition duplication uses `duplicate_definition(kind,uid,name)` with an already chosen unique name; supported kinds are `layers`, `entities`, `enums`, `tilesets`. Copies go immediately after their source. `reorder_definition(kind,uid,index)` additionally dispatches layer/level-field reorder. Layer copies get fresh empty instances, entity fields and auto-rule groups/rules get fresh global UIDs, scoped IntGrid palette group IDs stay intact as in the source.

## API contracts

All operations return `ldtk_core::Result<T>` unless noted. `LevelKey` remains `{ world:Option<usize>, level:usize }`.

| Family | Public methods |
|---|---|
| Transactions | `edit_document(FnOnce(&mut Project)->Result<T>)->Result<T>`; `refresh_derived_data()->Result<()>` |
| Worlds | `enable_multiworlds()->bool`; `add_world(identifier)->usize`; `duplicate_world(index,identifier)->usize`; `remove_world(index)`; `set_world_layout(world,layout,grid_w,grid_h)` |
| Levels | `add_level(world,identifier,width,height)->LevelKey`; `duplicate_level(key,identifier)->LevelKey`; `remove_level(key)`; `move_level(key,x,y,depth)`; `move_level_to_world(key,world,index)->LevelKey`; `resize_level(key,width,height,offset_x,offset_y)` |
| Layers | `reorder_layer(uid,index)`; `update_layer_properties(uid,&patch)`; `change_layer_grid_size(uid,new_grid)`; `convert_layer_type(uid,kind,allow_loss)` |
| Definition changes | `rename_definition(kind,uid,identifier)`; `remove_definition(kind,uid)`; `update_definition(kind,uid,&patch)`; `update_entity_definition(uid,&patch)`; `update_tileset_definition(uid,&patch)`; `duplicate_definition(kind,uid,name)->i64`; `reorder_definition(kind,uid,index)` |
| Entity edits | `move_entity(key,layer,index,x,y)`; `resize_entity(key,layer,index,width,height)`; `transfer_entity(from,from_layer,index,to,to_layer,x,y)`; `remove_entity_with_references(key,layer,index)->bool`; `finish_entity_transform(key,layer,index)->bool`; existing `add_entity` and `remove_entity` signatures preserved |
| Fields | `make_field_instance(&definition)->Value`; `set_field_value(key,entity_iid:Option<&str>,field_uid,value:Value)`; `reset_field_value(key,entity_iid,field_uid)`; `add_field_definition(entity_uid:Option<i64>,identifier,kind,is_array)->i64`; `update_field_definition(uid,&patch)`; `remove_field_definition(uid)`; `reorder_field_definition(uid,index)`; `set_field_default(uid,value)`; `convert_field_type(uid,kind,is_array,round)` |
| Enums | `add_enum_definition(identifier,&[&str])->i64`; `update_enum_definition(uid,&patch)`; `rename_enum_value(uid,old,new)`; `remove_enum_value(uid,value)`; generic definition rename/removal propagate into dependent field definitions/instances |
| Region clipboard | `copy_region(key,layer,[x,y,width,height])->Value`; `cut_region(...) -> Value`; `paste_region(key,layer,&clipboard,x,y,flip_x,flip_y,stack)->usize`; `move_region(key,layer,rect,dx,dy,flip_x,flip_y)->usize` |
| Mixed selections | `copy_selection_group(key,&[layer_indices],rect_px)`; `select_all_group(key,&layers)`; `refresh_selection_group(key,&group)`; `delete_selection_group(key,&group)`; `cut_selection_group(key,&layers,rect_px)`; `move_selection_group_with_result(key,&group,delta_px,flip_x,flip_y)->Value`; `paste_selection_group_with_result(key,&group,origin_px,flip_x,flip_y,stack)->Value`; count-returning `move_selection_group` / `paste_selection_group` retained |
| Camera-dependent selections | `copy_selection_group_from_regions(key,&regions,bounds_px)->Value`; `refresh_selection_group_geometry(key,&group,&geometries)->Value`; `move_selection_group_with_geometry(key,&group,delta_world:[f64;2],&geometries,flip_x,flip_y)->Value`; `paste_selection_group_with_geometry(key,&group,origin_world:[f64;2],&geometries,flip_x,flip_y,stack)->Value` |

`None` as a field entity IID means a level field. Public field kinds are `Int`, `Float`, `Bool`, `Color`, `String`, `Multilines`, `FilePath`, `Point`, `Tile`, `EntityRef`, `LocalEnum.Name`, `ExternEnum.Name`. Arrays are selected independently with `is_array`.

Clipboard coordinates and region rectangles use cells. Clipboard JSON is:

```json
{"type":"Tiles","gridSize":16,"width":2,"height":1,"origin":[0,0],"tilesetUid":9,"gridTiles":[],"intGridCsv":[],"entityInstances":[]}
```

Tiles/entities retain the original LDtk object shape and unknown keys, with `px` relative to the region origin. Unused arrays are empty. `origin` supports translating entity Point fields on duplication. Atlas stamps can directly construct `gridTiles` using relative pixels and the normal `t/src/f/a/d` keys. Manual tile stacking retains distinct tiles and raises repeated ID+flip entries instead of duplicating them.

Mixed groups retain `{type:"SelectionGroup",levelIid,origin:[pxX,pxY],size:[pxW,pxH],gridSize,compatibleGrids,elementCount,emptySpace,layers:[...]}`. Each layer entry retains `layerDefUid`, `identifier`, `layerIndex`, raw layer-space `origin`, cell `rect`, the per-layer `clipboard` above, `cellMask` for selected nonzero CSV cells, and `points:[{entityIid,fieldDefUid,arrayIndex,isArray,value:{cx,cy}}]`. Geometry-aware groups additionally retain `geometryAware:true`, `scaledGridSize`, each entry's `geometry:{origin,scale}`, and `worldOrigin`. Stable entity IIDs and explicit Point indices prevent a moved selection from collecting unrelated objects inside its new bounding rectangle.

## Source-backed behavior

Pinned original source is `.reference/ldtk` at the plan's upstream commit.

- `data/inst/FieldInstance.hx`: `ValueWrapper` serialization, two string escaping layers in editor values, public scalar/array formats, Points, Tile rectangles, entity reference metadata, default/null distinctions and reverse symmetric references.
- `data/def/FieldDef.hx`: primitive and enum defaults, numeric clamping with rounded Int bounds, reversed range swapping, regular-expression filtering, array bounds and field display metadata.
- `misc/FieldTypeConverter.hx`: supported scalar conversions and scalar-to-array conversion. Float-to-Int supports truncation or rounding. Numeric target wrappers are normalized rather than retaining an incompatible source wrapper.
- `data/def/TilesetDef.hx:295-398`: coordinate-preserving atlas geometry remap across manual tiles, saved selections, auto-rule rectangles (including null holes), enum tag tile lists and custom data; border-only crop padding adjustment. Cell counts use the original ceil formula and support partial edge tiles.
- `data/inst/LayerInstance.hx:169-184` and `ui/modal/panel/EditLayerDefs.hx:450,860`: per-level override priority, normal ID-preserving assignment and Tiles grid adoption.
- `tool/ResizeTool.hx:210-284`, `tool/lt/EntityTool.hx:68-76` and `misc/Coords.hx:40`: cheap constrained geometry previews and rounded pivot-relative placements.
- `data/Project.hx:640-683`: original single-world/MultiWorlds JSON representation and migration preserving the implicit world IID.
- `data/World.hx`: duplicate level IID/UID/layer/entity remapping; out-of-copy entity references detached; GridVania/linear layout transitions.
- `data/inst/LayerInstance.hx:829`: grid-size remapping keeps IntGrid/Tiles cell coordinates; entity anchors and custom sizes scale. `applyNewBounds` retains sub-grid offsets and translates Point fields.
- `data/Level.hx`: out-of-bounds entity cleanup during resize; world neighbour ordering/directions.
- `data/Project.hx:updateTableOfContent`: modern instancesData, deprecated instances flag, exported custom fields, effective linear-layout positions.
- `tool/lt/TileTool.hx` and `LayerInstance.addGridTile`: distinct tile stacking, raise-to-top and topmost erase.
- `GenericLevelElementGroup.hx:306-333,609-807`: largest scaled-grid selection space, raw-grid compatibility, element-specific movement, copied entity Point paths, and scoped count-limit checks. Grid cells move by integer smart-grid steps times integer scaled-grid ratio; entity raw pixels move by truncated world delta; Point cells move by truncated world delta / their layer scaled grid. These distinct source behaviors are deliberately retained. The native caller supplies the camera parallax origin used at capture/release.
- `data/inst/EntityInstance.hx:237`: entity out-of-layer bounds include the layer offsets and scaled grid size; the maximum anchor coordinate is inclusive. `finish_entity_transform` does not sweep unrelated entities.
- `data/Definitions.hx:201-230,361-388,614-632,717-735`: duplicate placement, scoped/global UID policies, and empty layer instance construction through project tidy.

## Verification evidence

Behavior tests were added/updated first and observed failing for absent workflows, old explicit guards, incorrect negative-grid floor division, stale field defaults/appearance caches, source range normalization, copied Point paths, stacked erase, and stale killer-layer output before the corresponding implementation change.

- `tests/field_edit.rs::official_example_field_exports_match_original_saved_values` loads every original sample and compares **337** saved public field values and field tiles exactly after regeneration.
- `tests/workflow.rs::official_toc_neighbours_and_entity_caches_match_original_saved_exports` compares **4** original TOC entries, **178** entity export caches and **156** neighbour entries exactly across original samples.
- Default-value fixture test compares **45** original saved default-backed fields across six original historical fixtures.
- Targeted workflow/field/selection/history tests cover transactional rollback, unknown keys, IDs, subgrid resize/Point translation, out-of-bounds cleanup, definition propagation, external-level changes, cross-level references, global undo/redo, world duplication/transfer, region overlap/flips/stacking, regex lookarounds/backreferences, and batch painting rollback.
- Strict core Clippy (`scripts/cargo clippy --offline -p ldtk-core --all-targets -- -D warnings`) passed after edit/hot-path integration.
- Full workspace/all-targets suite passed after mixed-selection geometry and definition duplication integration: main native test binary **72 passed / 3 ignored**, smoke test binary **71 passed / 3 ignored**; all core test binaries passed. A subsequent enum-values-patch regression was observed failing, corrected, and passed separately. A final whole-core run after that change encountered the autotile worker’s in-progress `apply_auto_rule_wizard` TDD compile failure; it is not reported as a pass. The final owned suite below passed.
- Final owned suite: **97 passed / 3 explicitly ignored / 0 failed** across `definitions`, `field_edit`, `history`, `references`, `selection`, `selection_group`, and `workflow`. Final strict core Clippy passed and owned-file `git diff --check` passed.
- Mixed-selection tests cover 16 behaviors: compatible/incompatible differing grids, Point-only selection, source-layer offsets, selected-zero masks, erase-arrival policy, unknown metadata, exact result selection persistence, copied cross-layer reference remapping, stale-live-object refresh, scaled movement, and copied Point continued dragging. Native resize module adds five source geometry tests (also built in the smoke binary).
- The 2 historical-fixture tests are explicitly ignored in default runs because `.reference` is not shipped, and both passed in an explicit `--ignored` run. Official sample comparisons use bundled `examples/original`.
- Release-mode `batch_intgrid_release_performance` painted 65,536 cells in **20.006 ms** on this host, with no auto rules; it stages touched cells instead of cloning the project. This is one measured full-region operation, not a p95 interactive performance claim.

## Limits and remaining acceptance work

These APIs are implemented backend workflows, not evidence that every original editor gesture or panel has visual/workflow parity.

- Mixed-layer and independent Point backend APIs now exist, including explicit camera geometry. Complete original gesture replay is still required: unit tests validate source-derived algorithms, and no compiled-original mixed-selection gesture oracle was captured. The `SelectionGroup` JSON is internal native state, not a promise of cross-process compatibility with the upstream clipboard serialization.
- Region/group mirroring and Point translation are tested internally; compiled-original equivalence for every independent-Point mirroring gesture has not been established. The geometry group path applies flips within each captured layer rectangle; source multi-layer mirroring behavior still needs a direct original UI acceptance case.
- `convert_layer_type(...,allow_loss=true)` explicitly discards incompatible content and can flatten generated tiles. The original bake dialog's create-copy/keep-source/remove-source choices need native orchestration.
- Field conversion accepts the original supported conversion families. Array-to-scalar conversion is rejected because it would discard values and the source converter does not expose it. Full compiled-original conversion replay has not been captured, although original saved field export comparison is exact.
- JSON strings cannot represent isolated UTF-16 surrogate values; a regex producing such a value returns an explicit error rather than silently replacing it. ECMAScript regex matching otherwise uses native Rust `regress` with UTF-16 input.
- History stores at most 30 actions and 64 MiB of encoded before/after payload. A single action exceeding the memory cap clears undo history. Stable IID/UID/defUid array paths preserve later unrelated siblings; arrays without stable identifiers are restored as values when their shape changes.
- General transactional structural methods clone complete project documents for rollback. Hot IntGrid batches avoid this; interactive structural/per-entity-heavy operations still need release profiling on large original fixtures.
- Native UI interaction replay, real rendering comparisons, filesystem export acceptance, packaging and platform integration are outside this worker's ownership and remain in the global acceptance ledger.
