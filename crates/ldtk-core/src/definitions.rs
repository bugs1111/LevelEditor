use crate::{Error, Project, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BakeMode {
    Keep,
    Delete,
    Empty,
}

impl Project {
    pub fn validate_auto_layer_bake(&self, uid: i64) -> Result<()> {
        let defs = list(self, "layers")?;
        let def = defs
            .iter()
            .find(|d| d["uid"] == uid)
            .ok_or_else(|| invalid("Layer definition not found."))?;
        let auto = def["type"] == "AutoLayer";
        let has_tileset = list(self, "tilesets")?
            .iter()
            .any(|ts| ts["uid"].is_i64() && ts["uid"] == def["tilesetDefUid"]);
        if !has_tileset
            || !(auto || def["type"] == "IntGrid")
            || (auto && def["autoSourceLayerDefUid"].is_null())
        {
            return Err(invalid(
                "Errors in current layer settings prevent rules to be applied. It can't be baked now.",
            ));
        }
        if defs.iter().any(|d| d["autoSourceLayerDefUid"] == uid) {
            return Err(invalid(
                "This layer cannot be baked, as at least one other auto-layer rely on it as 'source' for its data.",
            ));
        }
        Ok(())
    }

    pub fn bake_auto_layer(
        &mut self,
        uid: i64,
        mode: BakeMode,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<i64> {
        self.validate_auto_layer_bake(uid)?;
        self.edit_document(|p| {
            let defs = list(p, "layers")?;
            let index = defs.iter().position(|d| d["uid"] == uid).unwrap();
            let original = defs[index].clone();
            if mode == BakeMode::Empty && original["type"] != "IntGrid" {
                return Err(invalid(
                    "Only an IntGrid layer can be emptied after baking.",
                ));
            }
            let identifier = crate::naming::unique_name(
                &format!(
                    "{}_baked",
                    original["identifier"].as_str().unwrap_or("Layer")
                ),
                p.json["identifierStyle"].as_str().unwrap_or("Capitalize"),
                &defs
                    .iter()
                    .filter_map(|d| d["identifier"].as_str().map(str::to_owned))
                    .collect(),
            );
            let grid = original["gridSize"]
                .as_i64()
                .and_then(|grid| i32::try_from(grid).ok())
                .filter(|&grid| grid > 0)
                .ok_or_else(|| invalid("Invalid layer grid size."))?;
            let atlas = list(p, "tilesets")?
                .iter()
                .find(|ts| ts["uid"] == original["tilesetDefUid"])
                .unwrap()
                .clone();
            let layout = geometry(&atlas)?;
            let cached = atlas["cachedPixelData"]["opaqueTiles"].as_str();
            let image = if cached.is_some() {
                None
            } else if atlas["embedAtlas"] == "LdtkIcons" {
                crate::exports::embedded_atlas().ok()
            } else {
                atlas["relPath"]
                    .as_str()
                    .and_then(|path| p.make_absolute_file_path(path).ok())
                    .and_then(|path| crate::exports::load_image_rgba(&path).ok())
            };
            let mut opacity = HashMap::new();

            let uid_baked = p.add_layer(&identifier, "Tiles", grid)?;
            let groups = original["autoRuleGroups"].as_array().into_iter().flatten();
            // LDtk duplicates the definition and allocates new rule UIDs before discarding the rules.
            let cloned_uids: usize = groups
                .clone()
                .map(|group| 1 + group["rules"].as_array().map_or(0, Vec::len))
                .sum();
            let next = uid_baked + 1 + cloned_uids as i64;
            if next > i64::from(i32::MAX) {
                return Err(invalid("UID exceeds LDtk integer range"));
            }
            p.json["nextUid"] = json!(next);
            let mut new_def = original.clone();
            new_def["uid"] = json!(uid_baked);
            new_def["identifier"] = json!(identifier);
            new_def["type"] = json!("Tiles");
            new_def["__type"] = json!("Tiles");
            new_def["autoRuleGroups"] = json!([]);
            new_def["autoSourceLayerDefUid"] = Value::Null;
            let defs = p.json["defs"]["layers"].as_array_mut().unwrap();
            defs.remove(0);
            defs.insert(index + 1, new_def);

            let keys = p.level_keys();
            progress(0, keys.len());
            for (done, &key) in keys.iter().enumerate() {
                let level = p.level_mut(key).unwrap();
                let mut baked = level["layerInstances"].as_array_mut().unwrap().remove(0);
                baked["seed"] = json!((uuid::Uuid::new_v4().as_u128() % 9_999_999) as u32);
                let rules: BTreeSet<_> = groups
                    .clone()
                    .filter(|g| crate::autotile::group_applied(g, level, &baked, &original))
                    .flat_map(|g| g["rules"].as_array().into_iter().flatten())
                    .filter(|r| r["active"] != false)
                    .filter_map(|r| r["uid"].as_i64())
                    .collect();
                let layers = level["layerInstances"].as_array_mut().unwrap();
                let index = layers
                    .iter()
                    .position(|li| li["layerDefUid"] == uid)
                    .ok_or_else(|| invalid("Original layer instance not found."))?;
                let source = &mut layers[index];
                if let Some(layout) = layout {
                    baked["gridTiles"] =
                        flatten_auto_tiles(source, &baked, &rules, layout, |id, x, y| {
                            *opacity.entry(id).or_insert_with(|| {
                                if let Some(cached) = cached {
                                    cached.as_bytes().get(id as usize) == Some(&b'1')
                                } else {
                                    image.as_ref().is_some_and(|img| {
                                        let size = layout.grid;
                                        x + size > i64::from(img.width())
                                            || y + size > i64::from(img.height())
                                            || (y..y + size).all(|py| {
                                                (x..x + size).all(|px| {
                                                    img.get_pixel(px as u32, py as u32)[3] == 255
                                                })
                                            })
                                    })
                                }
                            })
                        });
                }
                let source_iid = source["iid"].as_str().map(str::to_owned);
                if mode == BakeMode::Empty {
                    if let Some(csv) = source["intGridCsv"].as_array_mut() {
                        csv.fill(json!(0));
                    }
                    if let Some(legacy) = source.get_mut("intGrid") {
                        *legacy = json!([]);
                    }
                    source["autoLayerTiles"] = json!([]);
                }
                if mode == BakeMode::Delete {
                    layers[index] = baked;
                    if let Some(iid) = source_iid {
                        p.grid_presence.remove(&iid);
                    }
                } else {
                    layers.insert(index + 1, baked);
                    if mode == BakeMode::Empty
                        && let Some(iid) = source_iid
                    {
                        p.grid_presence
                            .insert(iid, crate::grid_presence::GridPresence::empty());
                    }
                }
                progress(done + 1, keys.len());
            }
            if mode == BakeMode::Delete {
                // A bake removes the original layer without re-evaluating other layers.
                p.json["defs"]["layers"]
                    .as_array_mut()
                    .unwrap()
                    .remove(index);
            }
            p.refresh_derived_data()?;
            Ok(uid_baked)
        })
    }

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
            "guideGridWid":0,"guideGridHei":0,"guideColor":null,"guideOpacity":0.33,"entityDepthSorting":"Manual","displayOpacity":1,"inactiveOpacity":1,"hideInList":false,
            "hideFieldsWhenInactive":false,"canSelectWhenInactive":true,"renderInWorldView":true,"pxOffsetX":0,"pxOffsetY":0,
            "parallaxFactorX":0,"parallaxFactorY":0,"parallaxScaling":true,"requiredTags":[],"excludedTags":[],
            "layerUidsPreventingAutoTilingHere":[],"uiFilterTags":[],"useAsyncRender":false,
            "intGridValues":if kind=="IntGrid" {json!([{"value":1,"identifier":null,"color":"#000000","tile":null,"groupUid":0}])}else{json!([])},
            "intGridValuesGroups":[],"autoRuleGroups":[],"autoSourceLayerDefUid":null,"tilesetDefUid":null,"tilePivotX":0,"tilePivotY":0,"biomeFieldUid":null
        });
        for (key, instance) in instances {
            if kind == "IntGrid" {
                let iid = instance["iid"].as_str().expect("new layer IID");
                self.grid_presence
                    .insert(iid.into(), crate::grid_presence::GridPresence::empty());
            }
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

    pub fn ensure_embedded_icons(&mut self) -> Result<i64> {
        let tilesets = list(self, "tilesets")?;
        if let Some(uid) = tilesets
            .iter()
            .find(|d| d["embedAtlas"] == "LdtkIcons")
            .and_then(|d| d["uid"].as_i64())
        {
            return Ok(uid);
        }
        let identifier = crate::naming::unique_name(
            "Internal_Icons",
            self.json["identifierStyle"]
                .as_str()
                .unwrap_or("Capitalize"),
            &tilesets
                .iter()
                .filter_map(|d| d["identifier"].as_str().map(str::to_owned))
                .collect(),
        );
        let atlas = crate::exports::embedded_atlas()?;
        let uid = self.add_tileset(
            Path::new(&identifier),
            16,
            atlas.width() as i32,
            atlas.height() as i32,
        )?;
        let definition = self.json["defs"]["tilesets"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        definition["relPath"] = Value::Null;
        definition["embedAtlas"] = json!("LdtkIcons");
        Ok(uid)
    }

    pub fn add_entity_def(&mut self, identifier: &str, width: i32, height: i32) -> Result<i64> {
        check_identifier(self, "entities", identifier)?;
        if width <= 0 || height <= 0 {
            return Err(invalid("entity dimensions must be positive"));
        }
        let uid = next_uid(self)?;
        let [pivot_x, pivot_y] = ["defaultPivotX", "defaultPivotY"].map(|key| {
            self.json
                .get(key)
                .filter(|value| value.is_number())
                .cloned()
                .unwrap_or(json!(0))
        });
        let definition = json!({"identifier":identifier,"uid":uid,"tags":[],"exportToToc":false,"allowOutOfBounds":false,"doc":null,
            "width":width,"height":height,"resizableX":false,"resizableY":false,"minWidth":null,"maxWidth":null,"minHeight":null,"maxHeight":null,
            "keepAspectRatio":false,"tileOpacity":1,"fillOpacity":1,"lineOpacity":1,"hollow":false,"color":"#FFCC00","renderMode":"Rectangle",
            "showName":true,"tilesetId":null,"tileRenderMode":"FitInside","tileRect":null,"uiTileRect":null,"nineSliceBorders":[],"maxCount":0,
            "limitScope":"PerLevel","limitBehavior":"MoveLastOne","pivotX":pivot_x,"pivotY":pivot_y,"fieldDefs":[]});
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
fn geometry(d: &Value) -> Result<Option<AtlasGeometry>> {
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
    if width == 0 && height == 0 && d["relPath"].is_null() && d["embedAtlas"].is_null() {
        return Ok(None);
    }
    // A failed external image load keeps the pointer without decoded dimensions.
    let unresolved =
        width == 0 && height == 0 && d["relPath"].is_string() && d["embedAtlas"].is_null();
    if padding < 0
        || spacing < 0
        || padding > i64::from(i32::MAX)
        || spacing > i64::from(i32::MAX)
        || (!unresolved && (width <= 2 * padding || height <= 2 * padding))
        || width > i64::from(i32::MAX)
        || height > i64::from(i32::MAX)
        || grid > i64::from(i32::MAX)
    {
        return Err(invalid("invalid atlas geometry"));
    }
    let stride = grid + spacing;
    let (cols, rows) = if unresolved {
        (0, 0)
    } else {
        (
            (width - 2 * padding + stride - 1) / stride,
            (height - 2 * padding + stride - 1) / stride,
        )
    };
    Ok(Some(AtlasGeometry {
        grid,
        padding,
        spacing,
        cols,
        rows,
    }))
}
fn atlas_coordinates(id: i64, cols: i64) -> (i64, i64) {
    // LDtk converts division by zero to row zero when remapping an unresolved image.
    let row = if cols == 0 { 0 } else { id / cols };
    (id - row * cols, row)
}

fn flatten_auto_tiles(
    original: &Value,
    baked: &Value,
    rules: &BTreeSet<i64>,
    atlas: AtlasGeometry,
    mut opaque: impl FnMut(i64, i64, i64) -> bool,
) -> Value {
    let grid = baked["__gridSize"].as_i64().unwrap();
    let width = baked["__cWid"].as_i64().unwrap();
    let height = baked["__cHei"].as_i64().unwrap();
    let stride = atlas.grid + atlas.spacing;
    let mut stacks = BTreeMap::<i64, Vec<(i64, i64)>>::new();
    // The auto tile cache is serialized in rule display order, bottom to top.
    for tile in original["autoLayerTiles"].as_array().into_iter().flatten() {
        let (Some(rule), Some(x), Some(y), Some(id)) = (
            tile["d"][0].as_i64(),
            tile["px"][0].as_i64(),
            tile["px"][1].as_i64(),
            tile["t"].as_i64(),
        ) else {
            continue;
        };
        let (cx, cy) = (x / grid, y / grid);
        if !rules.contains(&rule)
            || !(0..width).contains(&cx)
            || !(0..height).contains(&cy)
            || !(0..atlas.cols * atlas.rows).contains(&id)
        {
            continue;
        }
        let flips = tile["f"].as_i64().unwrap_or_default();
        let (col, row) = atlas_coordinates(id, atlas.cols);
        let stack = stacks.entry(cy * width + cx).or_default();
        if opaque(
            id,
            col * stride + atlas.padding,
            row * stride + atlas.padding,
        ) {
            stack.clear();
        } else {
            stack.retain(|&t| t != (id, flips));
        }
        stack.push((id, flips));
    }
    Value::Array(
        stacks
            .into_iter()
            .flat_map(|(cell, stack)| {
                stack.into_iter().map(move |(id, flips)| {
                    let (col, row) = atlas_coordinates(id, atlas.cols);
                    json!({
                        "t": id, "f": flips, "a": 1, "d": [cell],
                        "px": [cell % width * grid, cell / width * grid],
                        "src": [col * stride + atlas.padding, row * stride + atlas.padding],
                    })
                })
            })
            .collect(),
    )
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
    let Some(new) = new.filter(|atlas| atlas.cols > 0) else {
        return Some(id);
    };
    if id < 0 {
        return None;
    }
    if mode == TilesetRemap::AtlasCoordinates {
        let cols = old.map_or(new.cols, |a| a.cols);
        let (x, y) = atlas_coordinates(id, cols);
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
                    let (x, y) = atlas_coordinates(id, a.cols);
                    json!([
                        a.padding + x * (a.grid + a.spacing),
                        a.padding + y * (a.grid + a.spacing)
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
                .transpose()?
                .flatten();
            let old = definition["tilesetDefUid"]
                .as_i64()
                .and_then(|uid| p.tileset(uid))
                .map(geometry)
                .transpose()?
                .flatten();
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
                        .copied()
                        .flatten();
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
                .transpose()?
                .flatten();
            let new = tileset_uid
                .or(default)
                .map(|uid| {
                    p.tileset(uid)
                        .ok_or_else(|| invalid("tileset definition not found"))
                        .and_then(geometry)
                })
                .transpose()?
                .flatten();
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
    pub fn remove_tileset_image(&mut self, uid: i64) -> Result<()> {
        let def = self
            .tileset(uid)
            .ok_or_else(|| invalid("tileset definition not found"))?;
        if !def["embedAtlas"].is_null() {
            return Err(invalid("embedded tileset images cannot be removed"));
        }
        self.update_tileset_definition(
            uid,
            &json!({"relPath":null,"pxWid":0,"pxHei":0,"savedSelections":[],"cachedPixelData":null}),
        )
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
            d["__cWid"] = json!(new.map_or(0, |a| a.cols));
            d["__cHei"] = json!(new.map_or(0, |a| a.rows));
            let mut removed = 0;
            if let Some(selections) = d["savedSelections"].as_array_mut() {
                for selection in selections {
                    if let Some(ids) = selection["ids"].as_array_mut() {
                        removed += remap_ids(ids, old, new, mode, allow_loss)?;
                    }
                }
            }
            if let Some(tags) = d["enumTags"].as_array_mut() {
                for tag in tags {
                    if let Some(ids) = tag["tileIds"].as_array_mut() {
                        removed += remap_ids(ids, old, new, mode, allow_loss)?;
                    }
                }
            }
            if let Some(custom) = d["customData"].as_array_mut() {
                let mut out = vec![];
                for item in custom.iter() {
                    let id = item["tileId"]
                        .as_i64()
                        .and_then(|id| remap_id(id, old, new, mode));
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
            // Editing per-tile metadata leaves atlas geometry and generated tiles intact.
            if mode == TilesetRemap::AtlasCoordinates
                && !allow_loss
                && fields.len() == 1
                && (fields.contains_key("enumTags") || fields.contains_key("customData"))
            {
                return Ok(removed);
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
                        removed += remap_tiles(tiles, old, new, mode, allow_loss)?;
                    }
                }
            }
            for layer in p.json["defs"]["layers"]
                .as_array_mut()
                .expect("validated layer definitions")
            {
                if layer["tilesetDefUid"] == uid {
                    removed += remap_rules(layer, old, new, mode, allow_loss)?;
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
