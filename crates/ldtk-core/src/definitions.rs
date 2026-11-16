use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn list<'a>(project: &'a Project, kind: &str) -> Result<&'a Vec<Value>> {
    project
        .json
        .get("defs")
        .and_then(|v| v.get(kind))
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(&format!("defs.{kind} must be an array")))
}
fn valid_identifier(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}
fn check_identifier(project: &Project, kind: &str, identifier: &str) -> Result<()> {
    if !valid_identifier(identifier) {
        return Err(invalid(
            "identifier must start with a letter or underscore and contain only letters, numbers or underscores",
        ));
    }
    if list(project, kind)?
        .iter()
        .any(|d| d["identifier"] == identifier)
    {
        return Err(invalid("identifier already exists"));
    }
    Ok(())
}
fn next_uid(project: &Project) -> Result<i64> {
    fn max_uid(value: &Value) -> i64 {
        match value {
            Value::Object(map) => map
                .values()
                .map(max_uid)
                .max()
                .unwrap_or(-1)
                .max(map.get("uid").and_then(Value::as_i64).unwrap_or(-1)),
            Value::Array(values) => values.iter().map(max_uid).max().unwrap_or(-1),
            _ => -1,
        }
    }
    let uid = project
        .json
        .get("nextUid")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(
            max_uid(&project.json)
                .checked_add(1)
                .ok_or_else(|| invalid("UID overflow"))?,
        );
    if uid < 0 || uid >= i32::MAX as i64 {
        return Err(invalid("UID exceeds LDtk integer range"));
    }
    Ok(uid)
}
fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Project {
    pub fn add_layer(&mut self, identifier: &str, kind: &str, grid: i32) -> Result<i64> {
        check_identifier(self, "layers", identifier)?;
        if !["IntGrid", "Tiles", "Entities", "AutoLayer"].contains(&kind) {
            return Err(invalid("unknown layer type"));
        }
        if grid <= 0 {
            return Err(invalid("grid size must be positive"));
        }
        let uid = next_uid(self)?;
        let mut instances = Vec::new();
        for key in self.level_keys() {
            let level = self
                .level(key)
                .ok_or_else(|| invalid("external level has not been loaded"))?;
            let width = level["pxWid"]
                .as_i64()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("invalid level width"))?;
            let height = level["pxHei"]
                .as_i64()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("invalid level height"))?;
            let cell_width = width
                .checked_add(i64::from(grid) - 1)
                .ok_or_else(|| invalid("layer size overflow"))?
                / i64::from(grid);
            let cell_height = height
                .checked_add(i64::from(grid) - 1)
                .ok_or_else(|| invalid("layer size overflow"))?
                / i64::from(grid);
            let cells = cell_width
                .checked_mul(cell_height)
                .filter(|v| *v <= i32::MAX as i64)
                .ok_or_else(|| invalid("layer cell count overflow"))?;
            level
                .get("layerInstances")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("level layerInstances must be an array"))?;
            let csv = if kind == "IntGrid" {
                let mut csv = Vec::new();
                csv.try_reserve_exact(cells as usize)
                    .map_err(|_| invalid("not enough memory for IntGrid layer"))?;
                csv.resize(cells as usize, json!(0));
                csv
            } else {
                vec![]
            };
            instances.push((key,json!({
                "__identifier":identifier,"__type":kind,"__cWid":cell_width,"__cHei":cell_height,"__gridSize":grid,"__opacity":1,
                "__pxTotalOffsetX":0,"__pxTotalOffsetY":0,"__tilesetDefUid":null,"__tilesetRelPath":null,
                "iid":new_uuid(),"levelId":level["uid"],"layerDefUid":uid,"pxOffsetX":0,"pxOffsetY":0,"visible":true,
                "optionalRules":[],"intGridCsv":csv,"autoLayerTiles":[],"seed":0,"overrideTilesetUid":null,"gridTiles":[],"entityInstances":[]
            })));
        }
        let definition = json!({
            "__type":kind,"identifier":identifier,"type":kind,"uid":uid,"doc":null,"uiColor":null,"gridSize":grid,
            "guideGridWid":0,"guideGridHei":0,"displayOpacity":1,"inactiveOpacity":1,"hideInList":false,
            "hideFieldsWhenInactive":false,"canSelectWhenInactive":true,"renderInWorldView":true,"pxOffsetX":0,"pxOffsetY":0,
            "parallaxFactorX":0,"parallaxFactorY":0,"parallaxScaling":true,"requiredTags":[],"excludedTags":[],
            "autoTilesKilledByOtherLayerUid":null,"uiFilterTags":[],"useAsyncRender":false,
            "intGridValues":if kind=="IntGrid" {json!([{"value":1,"identifier":null,"color":"#000000","tile":null,"groupUid":0}])}else{json!([])},
            "intGridValuesGroups":[],"autoRuleGroups":[],"autoSourceLayerDefUid":null,"tilesetDefUid":null,"tilePivotX":0,"tilePivotY":0,"biomeFieldUid":null
        });
        for (key, instance) in instances {
            self.level_mut(key).expect("validated level")["layerInstances"]
                .as_array_mut()
                .expect("validated layer array")
                .insert(0, instance);
        }
        self.json["defs"]["layers"]
            .as_array_mut()
            .expect("validated definitions")
            .insert(0, definition);
        self.json["nextUid"] = json!(uid + 1);
        Ok(uid)
    }

    pub fn add_tileset(&mut self, path: &Path, grid: i32, width: i32, height: i32) -> Result<i64> {
        list(self, "tilesets")?;
        if grid <= 0 || width <= 0 || height <= 0 {
            return Err(invalid("atlas dimensions and grid size must be positive"));
        }
        // TilesetDef.importAtlasImage clamps the grid and retains partial edge cells.
        let grid = grid.min(width).min(height);
        let columns = 1 + (width - 1) / grid;
        let rows = 1 + (height - 1) / grid;
        if path.as_os_str().is_empty() {
            return Err(invalid("tileset path is empty"));
        }
        let uid = next_uid(self)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Tileset");
        let mut identifier: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if !valid_identifier(&identifier) {
            identifier = format!("Tileset_{identifier}");
        }
        if list(self, "tilesets")?
            .iter()
            .any(|d| d["identifier"] == identifier)
        {
            identifier = format!("{identifier}_{uid}");
        }
        let asset_path = if path.is_absolute() {
            self.path
                .as_ref()
                .and_then(|p| p.parent())
                .and_then(|base| relative_path(base, path))
                .unwrap_or_else(|| path.to_path_buf())
        } else {
            path.to_path_buf()
        };
        let rel = asset_path
            .to_str()
            .ok_or_else(|| invalid("tileset path must be valid UTF-8"))?
            .replace('\\', "/");
        let definition = json!({"__cWid":columns,"__cHei":rows,"identifier":identifier,"uid":uid,"relPath":rel,"embedAtlas":null,
            "pxWid":width,"pxHei":height,"tileGridSize":grid,"spacing":0,"padding":0,"tags":[],"tagsSourceEnumUid":null,
            "enumTags":[],"customData":[],"savedSelections":[],"cachedPixelData":null});
        self.json["defs"]["tilesets"]
            .as_array_mut()
            .expect("validated definitions")
            .push(definition);
        self.json["nextUid"] = json!(uid + 1);
        Ok(uid)
    }

    pub fn add_entity_def(&mut self, identifier: &str, width: i32, height: i32) -> Result<i64> {
        check_identifier(self, "entities", identifier)?;
        if width <= 0 || height <= 0 {
            return Err(invalid("entity dimensions must be positive"));
        }
        let uid = next_uid(self)?;
        let definition = json!({"identifier":identifier,"uid":uid,"tags":[],"exportToToc":false,"allowOutOfBounds":false,"doc":null,
            "width":width,"height":height,"resizableX":false,"resizableY":false,"minWidth":null,"maxWidth":null,"minHeight":null,"maxHeight":null,
            "keepAspectRatio":false,"tileOpacity":1,"fillOpacity":1,"lineOpacity":1,"hollow":false,"color":"#FFCC00","renderMode":"Rectangle",
            "showName":true,"tilesetId":null,"tileRenderMode":"FitInside","tileRect":null,"uiTileRect":null,"nineSliceBorders":[],"maxCount":0,
            "limitScope":"PerLevel","limitBehavior":"MoveLastOne","pivotX":0.5,"pivotY":1.0,"fieldDefs":[]});
        self.json["defs"]["entities"]
            .as_array_mut()
            .expect("validated definitions")
            .push(definition);
        self.json["nextUid"] = json!(uid + 1);
        Ok(uid)
    }

    pub fn assign_tileset(&mut self, layer_uid: i64, tileset_uid: i64) -> Result<()> {
        self.assign_tileset_with_policy(
            layer_uid,
            Some(tileset_uid),
            TilesetRemap::TileIds,
            false,
            false,
        )
        .map(|_| ())
    }
}

fn relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    if base.first() != target.first() {
        return None;
    }
    let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut relative = PathBuf::new();
    for _ in shared..base.len() {
        relative.push("..");
    }
    for component in &target[shared..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

/// How existing atlas references behave when choosing or resizing a tileset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilesetRemap {
    TileIds,
    AtlasCoordinates,
    Clear,
}
#[derive(Clone, Copy)]
struct AtlasGeometry {
    grid: i64,
    padding: i64,
    spacing: i64,
    cols: i64,
    rows: i64,
}
fn geometry(d: &Value) -> Result<AtlasGeometry> {
    let grid = d["tileGridSize"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("invalid atlas grid size"))?;
    let padding = d["padding"].as_i64().unwrap_or(0);
    let spacing = d["spacing"].as_i64().unwrap_or(0);
    let width = d["pxWid"]
        .as_i64()
        .ok_or_else(|| invalid("invalid atlas width"))?;
    let height = d["pxHei"]
        .as_i64()
        .ok_or_else(|| invalid("invalid atlas height"))?;
    if padding < 0
        || spacing < 0
        || padding > i64::from(i32::MAX)
        || spacing > i64::from(i32::MAX)
        || width <= 2 * padding
        || height <= 2 * padding
        || width > i64::from(i32::MAX)
        || height > i64::from(i32::MAX)
        || grid > i64::from(i32::MAX)
    {
        return Err(invalid("invalid atlas geometry"));
    }
    let stride = grid + spacing;
    Ok(AtlasGeometry {
        grid,
        padding,
        spacing,
        cols: (width - 2 * padding + stride - 1) / stride,
        rows: (height - 2 * padding + stride - 1) / stride,
    })
}
fn remap_id(
    id: i64,
    old: Option<AtlasGeometry>,
    new: Option<AtlasGeometry>,
    mode: TilesetRemap,
) -> Option<i64> {
    if mode == TilesetRemap::Clear {
        return None;
    }
    let Some(new) = new else { return Some(id) };
    if id < 0 {
        return None;
    }
    if mode == TilesetRemap::AtlasCoordinates {
        let cols = old.map_or(new.cols, |a| a.cols);
        let x = id % cols;
        let y = id / cols;
        (x < new.cols && y < new.rows).then_some(y * new.cols + x)
    } else {
        (id < new.cols * new.rows).then_some(id)
    }
}
fn remap_ids(
    values: &mut Vec<Value>,
    old: Option<AtlasGeometry>,
    new: Option<AtlasGeometry>,
    mode: TilesetRemap,
    allow_loss: bool,
) -> Result<usize> {
    let mut removed = 0;
    let mut output = Vec::with_capacity(values.len());
    for value in values.iter() {
        let mapped = value.as_i64().and_then(|id| remap_id(id, old, new, mode));
        if let Some(id) = mapped {
            output.push(json!(id));
        } else if !allow_loss && mode != TilesetRemap::Clear {
            return Err(invalid(
                "atlas remap would discard tile references; select allow_loss explicitly",
            ));
        } else {
            removed += 1;
        }
    }
    *values = output;
    Ok(removed)
}
fn remap_tiles(
    values: &mut Vec<Value>,
    old: Option<AtlasGeometry>,
    new: Option<AtlasGeometry>,
    mode: TilesetRemap,
    allow_loss: bool,
) -> Result<usize> {
    let mut removed = 0;
    let mut output = Vec::with_capacity(values.len());
    for tile in values.iter() {
        let mapped = tile["t"]
            .as_i64()
            .and_then(|id| remap_id(id, old, new, mode));
        if let Some(id) = mapped {
            let mut tile = tile.clone();
            tile["t"] = json!(id);
            tile["src"] = new.map_or_else(
                || json!([-1, -1]),
                |a| {
                    json!([
                        a.padding + (id % a.cols) * (a.grid + a.spacing),
                        a.padding + (id / a.cols) * (a.grid + a.spacing)
                    ])
                },
            );
            output.push(tile);
        } else if !allow_loss && mode != TilesetRemap::Clear {
            return Err(invalid(
                "atlas remap would discard tiles; select allow_loss explicitly",
            ));
        } else {
            removed += 1;
        }
    }
    *values = output;
    Ok(removed)
}
fn remap_rules(
    d: &mut Value,
    old: Option<AtlasGeometry>,
    new: Option<AtlasGeometry>,
    mode: TilesetRemap,
    allow_loss: bool,
) -> Result<usize> {
    let mut removed = 0;
    if let Some(groups) = d["autoRuleGroups"].as_array_mut() {
        for group in groups {
            if let Some(rules) = group["rules"].as_array_mut() {
                for rule in rules {
                    if let Some(ids) = rule["tileIds"].as_array_mut() {
                        removed += remap_rule_ids(ids, old, new, mode, allow_loss)?;
                    }
                    if let Some(rects) = rule["tileRectsIds"].as_array_mut() {
                        for rect in rects.iter_mut() {
                            let ids = rect
                                .as_array_mut()
                                .ok_or_else(|| invalid("rule tile rectangle must be an array"))?;
                            removed += remap_rule_ids(ids, old, new, mode, allow_loss)?;
                        }
                    }
                }
            }
        }
    }
    Ok(removed)
}
impl Project {
    /// Default source selection keeps logical tile IDs and preserves per-level overrides.
    pub fn assign_tileset_with_policy(
        &mut self,
        layer_uid: i64,
        tileset_uid: Option<i64>,
        mode: TilesetRemap,
        clear_overrides: bool,
        allow_loss: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            let definition = p
                .layer_def(layer_uid)
                .ok_or_else(|| invalid("layer definition not found"))?
                .clone();
            let kind = definition["type"].as_str().unwrap_or("");
            if !["Tiles", "IntGrid", "AutoLayer"].contains(&kind) {
                return Err(invalid("entity layers do not have a layer tileset"));
            }
            let new = tileset_uid
                .map(|uid| {
                    p.tileset(uid)
                        .ok_or_else(|| invalid("tileset definition not found"))
                        .and_then(geometry)
                })
                .transpose()?;
            let old = definition["tilesetDefUid"]
                .as_i64()
                .and_then(|uid| p.tileset(uid))
                .map(geometry)
                .transpose()?;
            let atlases = p.json["defs"]["tilesets"]
                .as_array()
                .ok_or_else(|| invalid("tileset definitions missing"))?
                .iter()
                .map(|d| {
                    Ok((
                        d["uid"]
                            .as_i64()
                            .ok_or_else(|| invalid("tileset UID missing"))?,
                        geometry(d)?,
                    ))
                })
                .collect::<Result<std::collections::BTreeMap<_, _>>>()?;
            let mut removed = 0;
            for key in p.level_keys() {
                let layers = p
                    .level_mut(key)
                    .and_then(|l| l["layerInstances"].as_array_mut())
                    .ok_or_else(|| invalid("level layers not loaded"))?;
                for layer in layers {
                    if layer["layerDefUid"] != layer_uid {
                        continue;
                    }
                    let override_geometry = layer["overrideTilesetUid"]
                        .as_i64()
                        .and_then(|uid| atlases.get(&uid))
                        .copied();
                    if !clear_overrides && !layer["overrideTilesetUid"].is_null() {
                        if let Some(tiles) = layer["gridTiles"].as_array_mut() {
                            removed += remap_tiles(
                                tiles,
                                override_geometry,
                                override_geometry,
                                TilesetRemap::TileIds,
                                allow_loss,
                            )?;
                        }
                        continue;
                    }
                    let old = override_geometry.or(old);
                    if clear_overrides {
                        layer["overrideTilesetUid"] = Value::Null;
                    }
                    if let Some(tiles) = layer["gridTiles"].as_array_mut() {
                        removed += remap_tiles(tiles, old, new, mode, allow_loss)?;
                    }
                }
            }
            let d = p.json["defs"]["layers"]
                .as_array_mut()
                .expect("validated layer definitions")
                .iter_mut()
                .find(|d| d["uid"] == layer_uid)
                .expect("validated layer");
            removed += remap_rules(d, old, new, mode, allow_loss)?;
            d["tilesetDefUid"] = json!(tileset_uid);
            if kind == "Tiles"
                && let Some(atlas) = new
                && definition["gridSize"].as_i64() != Some(atlas.grid)
            {
                p.change_layer_grid_size(layer_uid, atlas.grid as i32)?;
            }
            p.refresh_auto_and_derived()?;
            Ok(removed)
        })
    }
    pub fn set_layer_tileset_override(
        &mut self,
        key: crate::LevelKey,
        index: usize,
        tileset_uid: Option<i64>,
        mode: TilesetRemap,
        allow_loss: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            let layer = p
                .layer(key, index)
                .ok_or_else(|| invalid("layer not found"))?;
            if !["Tiles", "IntGrid", "AutoLayer"].contains(&layer["__type"].as_str().unwrap_or(""))
            {
                return Err(invalid("entity layers do not have a layer tileset"));
            }
            let default = p
                .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
                .and_then(|d| d["tilesetDefUid"].as_i64());
            let old = layer["overrideTilesetUid"]
                .as_i64()
                .or(default)
                .and_then(|uid| p.tileset(uid))
                .map(geometry)
                .transpose()?;
            let new = tileset_uid
                .or(default)
                .map(|uid| {
                    p.tileset(uid)
                        .ok_or_else(|| invalid("tileset definition not found"))
                        .and_then(geometry)
                })
                .transpose()?;
            let layer = p.layer_mut(key, index).expect("validated layer");
            layer["overrideTilesetUid"] = json!(tileset_uid);
            let removed = remap_tiles(
                layer["gridTiles"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("gridTiles must be an array"))?,
                old,
                new,
                mode,
                allow_loss,
            )?;
            p.refresh_derived_data()?;
            p.rebuild_auto_layers(key)?;
            Ok(removed)
        })
    }
    /// Returns the number of discarded tile references when an explicit loss policy is selected.
    pub fn update_tileset_definition_with_policy(
        &mut self,
        uid: i64,
        patch: &Value,
        mode: TilesetRemap,
        allow_loss: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            let old_def = p
                .tileset(uid)
                .ok_or_else(|| invalid("tileset definition not found"))?
                .clone();
            let old = geometry(&old_def)?;
            let fields = patch
                .as_object()
                .ok_or_else(|| invalid("tileset patch must be an object"))?;
            if fields.contains_key("uid") {
                return Err(invalid("tileset UID is immutable"));
            }
            if let Some(name) = patch["identifier"].as_str() {
                p.rename_definition("tilesets", uid, name)?;
            }
            let d = p.json["defs"]["tilesets"]
                .as_array_mut()
                .expect("validated tilesets")
                .iter_mut()
                .find(|d| d["uid"] == uid)
                .expect("validated tileset");
            for (k, v) in fields {
                d[k] = v.clone();
            }
            // Original image reload recognizes a border-only crop and trims its padding.
            let ow = old_def["pxWid"].as_i64().unwrap_or(0);
            let oh = old_def["pxHei"].as_i64().unwrap_or(0);
            let nw = d["pxWid"].as_i64().unwrap_or(0);
            let nh = d["pxHei"].as_i64().unwrap_or(0);
            let pad = d["padding"].as_i64().unwrap_or(0);
            if pad > 0
                && ow > nw
                && oh > nh
                && (ow - nw) <= 2 * pad
                && (oh - nh) <= 2 * pad
                && nw % 2 == 0
                && nh % 2 == 0
            {
                d["padding"] = json!(pad - (ow - nw).min(oh - nh) / 2);
            }
            let new = geometry(d)?;
            d["__cWid"] = json!(new.cols);
            d["__cHei"] = json!(new.rows);
            let mut removed = 0;
            if let Some(selections) = d["savedSelections"].as_array_mut() {
                for selection in selections {
                    if let Some(ids) = selection["ids"].as_array_mut() {
                        removed += remap_ids(ids, Some(old), Some(new), mode, allow_loss)?;
                    }
                }
            }
            if let Some(tags) = d["enumTags"].as_array_mut() {
                for tag in tags {
                    if let Some(ids) = tag["tileIds"].as_array_mut() {
                        removed += remap_ids(ids, Some(old), Some(new), mode, allow_loss)?;
                    }
                }
            }
            if let Some(custom) = d["customData"].as_array_mut() {
                let mut out = vec![];
                for item in custom.iter() {
                    let id = item["tileId"]
                        .as_i64()
                        .and_then(|id| remap_id(id, Some(old), Some(new), mode));
                    if let Some(id) = id {
                        let mut item = item.clone();
                        item["tileId"] = json!(id);
                        out.push(item);
                    } else if !allow_loss && mode != TilesetRemap::Clear {
                        return Err(invalid("atlas remap would discard custom tile data"));
                    } else {
                        removed += 1;
                    }
                }
                *custom = out;
            }
            let layer_defs = p.json["defs"]["layers"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for key in p.level_keys() {
                let layers = p
                    .level_mut(key)
                    .and_then(|l| l["layerInstances"].as_array_mut())
                    .ok_or_else(|| invalid("level layers not loaded"))?;
                for layer in layers {
                    let effective = layer["overrideTilesetUid"].as_i64().or_else(|| {
                        layer_defs
                            .iter()
                            .find(|d| d["uid"] == layer["layerDefUid"])
                            .and_then(|d| d["tilesetDefUid"].as_i64())
                    });
                    if effective == Some(uid)
                        && let Some(tiles) = layer["gridTiles"].as_array_mut()
                    {
                        removed += remap_tiles(tiles, Some(old), Some(new), mode, allow_loss)?;
                    }
                }
            }
            for layer in p.json["defs"]["layers"]
                .as_array_mut()
                .expect("validated layer definitions")
            {
                if layer["tilesetDefUid"] == uid {
                    removed += remap_rules(layer, Some(old), Some(new), mode, allow_loss)?;
                }
            }
            p.refresh_auto_and_derived()?;
            Ok(removed)
        })
    }
}
fn remap_rule_ids(
    values: &mut [Value],
    old: Option<AtlasGeometry>,
    new: Option<AtlasGeometry>,
    mode: TilesetRemap,
    allow_loss: bool,
) -> Result<usize> {
    let mut removed = 0;
    for value in values {
        if value.is_null() {
            continue;
        }
        if let Some(mapped) = value.as_i64().and_then(|id| remap_id(id, old, new, mode)) {
            *value = json!(mapped);
        } else if !allow_loss && mode != TilesetRemap::Clear {
            return Err(invalid(
                "atlas remap would discard auto-rule tiles; select allow_loss explicitly",
            ));
        } else {
            *value = Value::Null;
            removed += 1;
        }
    }
    Ok(removed)
}

impl Project {
    /// Duplicate a definition after its source; new layers have fresh empty instances.
    pub fn duplicate_definition(&mut self, kind: &str, uid: i64, name: &str) -> Result<i64> {
        use crate::workflow::{alloc_uid, valid_identifier};
        if !matches!(kind, "layers" | "entities" | "enums" | "tilesets") {
            return Err(invalid("definition kind cannot be duplicated"));
        }
        self.edit_document(|p| {
            if !valid_identifier(name) {
                return Err(invalid("invalid definition identifier"));
            }
            let defs = p.json["defs"][kind]
                .as_array()
                .ok_or_else(|| invalid("definition list missing"))?;
            if defs.iter().any(|d| d["identifier"] == name) {
                return Err(invalid("definition identifier already exists"));
            }
            let source_index = defs
                .iter()
                .position(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("definition not found"))?;
            let mut copy = defs[source_index].clone();
            let new_uid = if kind == "layers" {
                p.add_layer(
                    name,
                    copy["type"]
                        .as_str()
                        .ok_or_else(|| invalid("layer type missing"))?,
                    copy["gridSize"]
                        .as_i64()
                        .ok_or_else(|| invalid("layer grid missing"))? as i32,
                )?
            } else {
                alloc_uid(p)?
            };
            copy["uid"] = json!(new_uid);
            copy["identifier"] = json!(name);
            if kind == "entities" {
                for field in copy["fieldDefs"].as_array_mut().into_iter().flatten() {
                    field["uid"] = json!(alloc_uid(p)?);
                }
            }
            if kind == "layers" {
                for group in copy["autoRuleGroups"].as_array_mut().into_iter().flatten() {
                    group["uid"] = json!(alloc_uid(p)?);
                    for rule in group["rules"].as_array_mut().into_iter().flatten() {
                        rule["uid"] = json!(alloc_uid(p)?);
                    }
                }
            }
            let defs = p.json["defs"][kind]
                .as_array_mut()
                .expect("validated definition list");
            if kind == "layers" {
                let target = defs
                    .iter_mut()
                    .find(|d| d["uid"] == new_uid)
                    .expect("new layer definition");
                *target = copy;
                p.reorder_layer(new_uid, source_index + 1)?;
            } else {
                defs.insert(source_index + 1, copy);
            }
            p.refresh_auto_and_derived()?;
            Ok(new_uid)
        })
    }
    pub fn reorder_definition(&mut self, kind: &str, uid: i64, index: usize) -> Result<()> {
        if kind == "layers" {
            return self.reorder_layer(uid, index);
        }
        if kind == "levelFields" {
            return self.reorder_field_definition(uid, index);
        }
        if !matches!(kind, "entities" | "enums" | "tilesets") {
            return Err(invalid("definition kind cannot be reordered"));
        }
        self.edit_document(|p| {
            let defs = p.json["defs"][kind]
                .as_array_mut()
                .ok_or_else(|| invalid("definition list missing"))?;
            let old = defs
                .iter()
                .position(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("definition not found"))?;
            if index >= defs.len() {
                return Err(invalid("definition index outside list"));
            }
            let value = defs.remove(old);
            defs.insert(index, value);
            p.refresh_derived_data()
        })
    }
}
