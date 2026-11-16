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
        if !["IntGrid", "Tiles", "Entities"].contains(&kind) {
            return Err(Error::Unsupported(
                "new layers currently support IntGrid, Tiles and Entities".into(),
            ));
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
        if grid <= 0 || width < grid || height < grid || width % grid != 0 || height % grid != 0 {
            return Err(invalid(
                "atlas dimensions must be positive multiples of its grid size",
            ));
        }
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
        let definition = json!({"__cWid":width/grid,"__cHei":height/grid,"identifier":identifier,"uid":uid,"relPath":rel,"embedAtlas":null,
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
        let definition = self
            .layer_def(layer_uid)
            .ok_or_else(|| invalid("layer definition not found"))?;
        if definition["type"] != "Tiles" {
            return Err(invalid(
                "tileset assignment currently requires a Tiles layer",
            ));
        }
        let tileset = self
            .tileset(tileset_uid)
            .ok_or_else(|| invalid("tileset definition not found"))?;
        let grid = definition["gridSize"]
            .as_i64()
            .filter(|g| *g > 0)
            .ok_or_else(|| invalid("invalid layer grid size"))?;
        let atlas_grid = tileset["tileGridSize"]
            .as_i64()
            .filter(|g| *g > 0)
            .ok_or_else(|| invalid("invalid tileset grid size"))?;
        let width = tileset["pxWid"]
            .as_i64()
            .ok_or_else(|| invalid("invalid tileset width"))?;
        let height = tileset["pxHei"]
            .as_i64()
            .ok_or_else(|| invalid("invalid tileset height"))?;
        let padding = tileset["padding"].as_i64().unwrap_or(0);
        let spacing = tileset["spacing"].as_i64().unwrap_or(0);
        if atlas_grid != grid || padding < 0 || spacing < 0 || width < grid || height < grid {
            return Err(invalid(
                "layer and tileset grid sizes or atlas dimensions are incompatible",
            ));
        }
        let cols = (width - 2 * padding + spacing) / (grid + spacing);
        let rows = (height - 2 * padding + spacing) / (grid + spacing);
        let count = cols
            .checked_mul(rows)
            .filter(|n| *n > 0)
            .ok_or_else(|| invalid("invalid tileset dimensions"))?;
        let rel = tileset.get("relPath").cloned().unwrap_or(Value::Null);
        let mut layers = vec![];
        for key in self.level_keys() {
            let instances = self
                .level(key)
                .and_then(|l| l.get("layerInstances"))
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("level layers are not loaded"))?;
            for (index, instance) in instances.iter().enumerate() {
                if instance["layerDefUid"] != layer_uid {
                    continue;
                }
                if !instance["overrideTilesetUid"].is_null() {
                    return Err(Error::Unsupported("assigning a layer with per-level tileset overrides requires an explicit override choice".into()));
                }
                let tiles = instance["gridTiles"]
                    .as_array()
                    .ok_or_else(|| invalid("gridTiles must be an array"))?;
                for tile in tiles {
                    let tile_id = tile["t"]
                        .as_i64()
                        .filter(|n| *n >= 0 && *n < count)
                        .ok_or_else(|| invalid("existing tile ID is outside the new atlas"))?;
                    let expected = json!([
                        padding + (tile_id % cols) * (grid + spacing),
                        padding + (tile_id / cols) * (grid + spacing)
                    ]);
                    if tile["src"] != expected {
                        return Err(Error::Unsupported("existing tile coordinates differ in the new atlas; remapping is required".into()));
                    }
                }
                layers.push((key, index));
            }
        }
        let defs = self.json["defs"]["layers"]
            .as_array_mut()
            .expect("found definition array");
        defs.iter_mut()
            .find(|d| d["uid"] == layer_uid)
            .expect("found layer definition")["tilesetDefUid"] = json!(tileset_uid);
        for (key, index) in layers {
            let layer = self.layer_mut(key, index).expect("validated layer");
            layer["__tilesetDefUid"] = json!(tileset_uid);
            layer["__tilesetRelPath"] = rel.clone();
        }
        Ok(())
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
