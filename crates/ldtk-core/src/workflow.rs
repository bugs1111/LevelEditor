//! Transactional document workflows. Unknown JSON members remain authoritative.
use crate::grid_presence::{GridPresence, update_presence};
use crate::{Error, LevelKey, Project, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
pub(crate) fn iid() -> Value {
    json!(uuid::Uuid::new_v4().to_string())
}
pub(crate) fn layer_cell_extent(pixels: i64, offset: i64, grid: i64) -> i64 {
    let span = pixels.saturating_sub(offset).max(0);
    span / grid + i64::from(span % grid != 0)
}
pub(crate) fn valid_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}
pub(crate) fn alloc_uid(p: &mut Project) -> Result<i64> {
    fn max(v: &Value) -> i64 {
        match v {
            Value::Object(m) => m
                .values()
                .map(max)
                .max()
                .unwrap_or(-1)
                .max(m.get("uid").and_then(Value::as_i64).unwrap_or(-1)),
            Value::Array(a) => a.iter().map(max).max().unwrap_or(-1),
            _ => -1,
        }
    }
    let n = p.json["nextUid"]
        .as_i64()
        .unwrap_or(0)
        .max(max(&p.json) + 1);
    if !(0..i64::from(i32::MAX)).contains(&n) {
        return Err(invalid("UID overflow"));
    }
    p.json["nextUid"] = json!(n + 1);
    Ok(n)
}
fn container(p: &Project, world: Option<usize>) -> Result<&Value> {
    match world {
        Some(w) => p.json["worlds"]
            .get(w)
            .ok_or_else(|| invalid("world not found")),
        None => Ok(&p.json),
    }
}
fn world_mut(p: &mut Project, world: Option<usize>) -> Result<&mut Value> {
    match world {
        Some(w) => p.json["worlds"]
            .get_mut(w)
            .ok_or_else(|| invalid("world not found")),
        None => Ok(&mut p.json),
    }
}
fn levels_mut(p: &mut Project, world: Option<usize>) -> Result<&mut Vec<Value>> {
    world_mut(p, world)?["levels"]
        .as_array_mut()
        .ok_or_else(|| invalid("levels must be an array"))
}
pub(crate) fn tidy_gridvania_defaults(world: &mut Value) {
    if world["worldLayout"] != "GridVania" {
        return;
    }
    for (dimension, grid) in [
        ("defaultLevelWidth", "worldGridWidth"),
        ("defaultLevelHeight", "worldGridHeight"),
    ] {
        if let Some(grid) = world[grid].as_f64().filter(|value| *value > 0.) {
            let size = world[dimension].as_f64().unwrap_or(256.);
            world[dimension] = json!(((size / grid).round().max(1.) * grid) as i64);
        }
    }
}
fn unique_level(p: &Project, name: &str) -> Result<()> {
    if !valid_identifier(name) {
        return Err(invalid("invalid level identifier"));
    }
    if p.level_keys()
        .iter()
        .any(|k| p.level(*k).is_some_and(|l| l["identifier"] == name))
    {
        return Err(invalid("level identifier already exists"));
    }
    Ok(())
}
pub(crate) fn visit_fields_mut(level: &mut Value, mut f: impl FnMut(&mut Value)) {
    if let Some(fields) = level["fieldInstances"].as_array_mut() {
        for field in fields {
            f(field);
        }
    }
    visit_entity_fields_mut(level, f);
}
fn visit_entity_fields_mut(level: &mut Value, mut f: impl FnMut(&mut Value)) {
    if let Some(layers) = level["layerInstances"].as_array_mut() {
        for layer in layers {
            if let Some(entities) = layer["entityInstances"].as_array_mut() {
                for entity in entities {
                    if let Some(fields) = entity["fieldInstances"].as_array_mut() {
                        for field in fields {
                            f(field);
                        }
                    }
                }
            }
        }
    }
}
fn ref_iid(v: &Value) -> Option<&str> {
    v.as_str().or_else(|| v["entityIid"].as_str())
}
pub(crate) fn remap_refs(level: &mut Value, remaps: &BTreeMap<String, Value>, clear_outside: bool) {
    visit_fields_mut(level, |field| {
        remap_field_refs(field, remaps, clear_outside)
    });
}
fn remap_field_refs(field: &mut Value, remaps: &BTreeMap<String, Value>, clear_outside: bool) {
    let array = match field["__type"].as_str() {
        Some("Array<EntityRef>") => true,
        Some("EntityRef") => false,
        _ => return,
    };
    if let Some(values) = field["realEditorValues"].as_array_mut() {
        values.retain_mut(|value| {
            let Some(old) = value["params"][0].as_str() else {
                return true;
            };
            if let Some(new) = remaps.get(old) {
                value["params"][0] = new.clone();
                true
            } else {
                !clear_outside
            }
        });
    }
    let remap_value = |value: &mut Value| {
        let Some(old) = ref_iid(value) else {
            return true;
        };
        if let Some(new) = remaps.get(old) {
            if value.is_string() {
                *value = new.clone();
            } else {
                value["entityIid"] = new.clone();
            }
        } else if clear_outside {
            if array {
                return false;
            }
            *value = Value::Null;
        }
        true
    };
    if array {
        if let Some(values) = field["__value"].as_array_mut() {
            values.retain_mut(remap_value);
        }
    } else {
        remap_value(&mut field["__value"]);
    }
}
impl Project {
    pub fn cleanup_tag(input: &str) -> Option<String> {
        let tag = crate::naming::cleanup(input, "Free");
        (!tag.is_empty() && tag != "_").then_some(tag)
    }

    pub fn edit_document<T>(&mut self, edit: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let mut candidate = self.clone();
        let result = edit(&mut candidate)?;
        candidate
            .external_levels
            .reuse_equal_documents(&self.external_levels);
        *self = candidate;
        Ok(result)
    }
    /// Enable original MultiWorlds serialization. Legacy keys move to world index zero.
    pub fn enable_multiworlds(&mut self) -> Result<bool> {
        self.edit_document(|p|{
        if p.json["flags"].as_array().is_some_and(|f|f.contains(&json!("MultiWorlds"))){return Ok(false);}
        let mut world=json!({"iid":p.json["dummyWorldIid"],"identifier":"World_0","levels":p.json["levels"]});
        if world["iid"].is_null(){world["iid"]=iid();p.json["dummyWorldIid"]=world["iid"].clone();}
        for (field,default)in[("worldLayout",json!("Free")),("worldGridWidth",json!(256)),("worldGridHeight",json!(256)),("defaultLevelWidth",json!(256)),("defaultLevelHeight",json!(256))]{world[field]=p.json.get(field).filter(|v|!v.is_null()).cloned().unwrap_or(default);p.json[field]=Value::Null;}
        p.json["levels"]=json!([]);
        let worlds=p.json["worlds"].as_array_mut().ok_or_else(||invalid("worlds must be an array"))?;
        let mut name="World_0".to_owned();let mut suffix=1;while worlds.iter().any(|w|w["identifier"]==name){name=format!("World_{suffix}");suffix+=1;}world["identifier"]=json!(name);worlds.insert(0,world);
        let flags=p.json["flags"].as_array_mut().ok_or_else(||invalid("flags must be an array"))?;flags.push(json!("MultiWorlds"));p.refresh_derived_data()?;Ok(true)
    })
    }
    /// Move the only world back to the root, keeping its IID for references.
    pub fn disable_multiworlds(&mut self) -> Result<bool> {
        self.edit_document(|p| {
            let flags = p.json["flags"]
                .as_array_mut()
                .ok_or_else(|| invalid("flags must be an array"))?;
            if !flags.iter().any(|flag| flag == "MultiWorlds") {
                return Ok(false);
            }
            flags.retain(|flag| flag != "MultiWorlds");
            if p.json["levels"]
                .as_array()
                .is_some_and(|levels| !levels.is_empty())
            {
                return Err(invalid("project already contains levels at the root"));
            }
            let worlds = p.json["worlds"]
                .as_array_mut()
                .ok_or_else(|| invalid("worlds must be an array"))?;
            if worlds.len() != 1 {
                return Err(invalid(
                    "exactly one world is required to disable MultiWorlds",
                ));
            }
            let mut world = worlds.pop().expect("one remaining world");
            if world["iid"].is_string() {
                p.json["dummyWorldIid"] = std::mem::take(&mut world["iid"]);
            }
            for field in [
                "levels",
                "worldLayout",
                "worldGridWidth",
                "worldGridHeight",
                "defaultLevelWidth",
                "defaultLevelHeight",
            ] {
                p.json[field] = std::mem::take(&mut world[field]);
            }
            Ok(true)
        })
    }
    pub fn add_world(&mut self, name: &str) -> Result<usize> {
        self.edit_document(|p|{
        if !valid_identifier(name){return Err(invalid("invalid world identifier"));}
        p.enable_multiworlds()?;
        let worlds=p.json["worlds"].as_array_mut().ok_or_else(||invalid("worlds must be an array"))?;
        if worlds.iter().any(|w|w["identifier"]==name){return Err(invalid("world identifier already exists"));}
        let index=worlds.len();worlds.push(json!({"iid":iid(),"identifier":name,"levels":[],"defaultLevelWidth":256,"defaultLevelHeight":256,"worldGridWidth":256,"worldGridHeight":256,"worldLayout":"Free"}));Ok(index)
    })
    }
    pub fn remove_world(&mut self, index: usize) -> Result<()> {
        self.edit_document(|p| {
            let count = container(p, Some(index))?["levels"]
                .as_array()
                .ok_or_else(|| invalid("levels missing"))?
                .len();
            for level in (0..count).rev() {
                p.remove_level(LevelKey {
                    world: Some(index),
                    level,
                })?;
            }
            p.json["worlds"]
                .as_array_mut()
                .ok_or_else(|| invalid("worlds missing"))?
                .remove(index);
            p.refresh_derived_data()
        })
    }
    pub fn add_level(
        &mut self,
        world: Option<usize>,
        name: &str,
        width: i32,
        height: i32,
    ) -> Result<LevelKey> {
        self.edit_document(|p|{
 unique_level(p,name)?;if width<=0||height<=0{return Err(invalid("level dimensions must be positive"));}container(p,world)?;let uid=alloc_uid(p)?;
 let mut level:Value=serde_json::from_str::<Value>(include_str!("empty.ldtk")).expect("template")["levels"][0].clone();level["identifier"]=json!(name);level["useAutoIdentifier"]=json!(false);level["iid"]=iid();level["uid"]=json!(uid);level["pxWid"]=json!(width);level["pxHei"]=json!(height);level["__bgColor"]=p.json["defaultLevelBgColor"].clone();
 let defs=p.json["defs"]["layers"].as_array().cloned().unwrap_or_default();let mut layers=vec![];for d in defs{let g=d["gridSize"].as_i64().filter(|v|*v>0).ok_or_else(||invalid("invalid layer grid"))?;let cw=(i64::from(width)+g-1)/g;let ch=(i64::from(height)+g-1)/g;let count=cw.checked_mul(ch).filter(|n|*n<=16_777_216).ok_or_else(||invalid("level IntGrid allocation too large"))?;let ts=p.tileset(d["tilesetDefUid"].as_i64().unwrap_or(-1));layers.push(json!({"__identifier":d["identifier"],"__type":d["type"],"__gridSize":g,"__cWid":cw,"__cHei":ch,"__opacity":d["displayOpacity"],"__pxTotalOffsetX":d["pxOffsetX"].as_i64().unwrap_or(0),"__pxTotalOffsetY":d["pxOffsetY"].as_i64().unwrap_or(0),"__tilesetDefUid":d["tilesetDefUid"],"__tilesetRelPath":ts.map(|v|v["relPath"].clone()),"iid":iid(),"levelId":uid,"layerDefUid":d["uid"],"pxOffsetX":0,"pxOffsetY":0,"visible":true,"optionalRules":[],"intGridCsv":if d["type"]=="IntGrid"{vec![json!(0);count as usize]}else{vec![]},"autoLayerTiles":[],"seed":0,"overrideTilesetUid":null,"gridTiles":[],"entityInstances":[]}));}
 for layer in &layers { if layer["__type"] == "IntGrid" { let iid = layer["iid"].as_str().expect("new layer IID"); p.grid_presence.insert(iid.into(), GridPresence::empty()); } }
 level["layerInstances"]=json!(layers);let fields=p.json["defs"]["levelFields"].as_array().cloned().unwrap_or_default();level["fieldInstances"]=json!(fields.iter().map(|d|p.make_field_instance(d)).collect::<Result<Vec<_>>>()?);
 let levels=levels_mut(p,world)?;let key=LevelKey{world,level:levels.len()};levels.push(level);
 tidy_gridvania_defaults(world_mut(p, world)?);
 p.refresh_auto_and_derived()?;Ok(key)
 })
    }
    pub fn duplicate_level(&mut self, key: LevelKey, name: &str) -> Result<LevelKey> {
        self.edit_document(|p| {
            unique_level(p, name)?;
            let mut copy = p
                .level(key)
                .ok_or_else(|| invalid("level not loaded"))?
                .clone();
            let uid = alloc_uid(p)?;
            copy["iid"] = iid();
            copy["uid"] = json!(uid);
            copy["identifier"] = json!(name);
            copy["useAutoIdentifier"] = json!(false);
            copy["externalRelPath"] = Value::Null;
            let mut remaps = BTreeMap::new();
            if let Some(layers) = copy["layerInstances"].as_array_mut() {
                for layer in layers {
                    layer["iid"] = iid();
                    layer["levelId"] = json!(uid);
                    if let Some(entities) = layer["entityInstances"].as_array_mut() {
                        for entity in entities {
                            let old = entity["iid"]
                                .as_str()
                                .ok_or_else(|| invalid("entity IID missing"))?
                                .to_owned();
                            let new = iid();
                            remaps.insert(old, new.clone());
                            entity["iid"] = new;
                        }
                    }
                }
            }
            // A level duplicate retargets entity fields. Its level fields keep their links.
            visit_entity_fields_mut(&mut copy, |field| remap_field_refs(field, &remaps, true));
            let new = LevelKey {
                world: key.world,
                level: key.level + 1,
            };
            levels_mut(p, key.world)?.insert(new.level, copy);
            tidy_gridvania_defaults(world_mut(p, key.world)?);
            p.refresh_derived_data()?;
            Ok(new)
        })
    }
    pub fn remove_level(&mut self, key: LevelKey) -> Result<()> {
        self.edit_document(|p| {
            let level = p.level(key).ok_or_else(|| invalid("level not loaded"))?;
            let layer_iids: Vec<_> = level["layerInstances"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|layer| layer["iid"].as_str().map(str::to_owned))
                .collect();
            for iid in layer_iids {
                p.grid_presence.remove(&iid);
            }
            let level = levels_mut(p, key.world)?.remove(key.level);
            if let Some(path) = level["externalRelPath"].as_str() {
                p.external_levels.remove(path);
            }
            p.refresh_derived_data()
        })
    }
    pub fn move_level(&mut self, key: LevelKey, x: i32, y: i32, depth: i32) -> Result<()> {
        self.edit_document(|p| {
            let level = p
                .level_mut(key)
                .ok_or_else(|| invalid("level not loaded"))?;
            level["worldX"] = json!(x);
            level["worldY"] = json!(y);
            level["worldDepth"] = json!(depth);
            p.refresh_derived_data()
        })
    }
    /// Resize and translate existing layer contents by pixel offsets. Cells outside the result are cropped.
    pub fn resize_level(
        &mut self,
        key: LevelKey,
        width: i32,
        height: i32,
        offset_x: i32,
        offset_y: i32,
    ) -> Result<()> {
        self.edit_document(|p| {
            if width <= 0 || height <= 0 {
                return Err(invalid("level dimensions must be positive"));
            }
            let entity_defs = p.json["defs"]["entities"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut grid_presence = std::mem::take(&mut p.grid_presence);
            let level = p
                .level_mut(key)
                .ok_or_else(|| invalid("level not loaded"))?;
            level["pxWid"] = json!(width);
            level["pxHei"] = json!(height);
            if let Some(layers) = level["layerInstances"].as_array_mut() {
                for layer in layers {
                    let g = layer["__gridSize"]
                        .as_i64()
                        .filter(|g| *g > 0)
                        .ok_or_else(|| invalid("invalid grid size"))?;
                    let total_x = layer["pxOffsetX"].as_i64().unwrap_or(0) + i64::from(offset_x);
                    let total_y = layer["pxOffsetY"].as_i64().unwrap_or(0) + i64::from(offset_y);
                    let cell_dx = total_x / g;
                    let cell_dy = total_y / g;
                    let remainder_x = total_x % g;
                    let remainder_y = total_y % g;
                    let ow = layer["__cWid"].as_i64().unwrap_or(0);
                    let nw = layer_cell_extent(i64::from(width), remainder_x, g);
                    let nh = layer_cell_extent(i64::from(height), remainder_y, g);
                    let count = nw
                        .checked_mul(nh)
                        .filter(|n| *n <= 16_777_216)
                        .ok_or_else(|| invalid("resized grid too large"))?;
                    if layer["__type"] == "IntGrid" {
                        let old = layer["intGridCsv"]
                            .as_array()
                            .ok_or_else(|| invalid("IntGrid missing"))?;
                        let layer_iid = layer["iid"].as_str();
                        let old_presence = layer_iid.and_then(|iid| grid_presence.get(iid));
                        let remapped =
                            ow != nw || old.len() != count as usize || cell_dx != 0 || cell_dy != 0;
                        let mut new_presence = GridPresence::empty();
                        let mut cells = vec![json!(0); count as usize];
                        if ow > 0 {
                            for (i, v) in old.iter().enumerate() {
                                let x = i as i64 % ow + cell_dx;
                                let y = i as i64 / ow + cell_dy;
                                if x >= 0 && y >= 0 && x < nw && y < nh {
                                    let index = (y * nw + x) as usize;
                                    cells[index] = v.clone();
                                    if remapped
                                        && *v == 0
                                        && old_presence.is_none_or(|state| state.contains(i))
                                    {
                                        new_presence.set(index, true);
                                    }
                                }
                            }
                        }
                        if remapped && let Some(iid) = layer_iid {
                            grid_presence.insert(iid.into(), new_presence);
                        }
                        layer["intGridCsv"] = json!(cells);
                    }
                    layer["__cWid"] = json!(nw);
                    layer["__cHei"] = json!(nh);
                    for name in ["gridTiles", "autoLayerTiles"] {
                        if let Some(tiles) = layer[name].as_array_mut() {
                            tiles.retain_mut(|tile| {
                                let x = tile["px"][0].as_i64().unwrap_or(0) + cell_dx * g;
                                let y = tile["px"][1].as_i64().unwrap_or(0) + cell_dy * g;
                                tile["px"] = json!([x, y]);
                                if name == "gridTiles" {
                                    tile["d"] = json!([y / g * nw + x / g]);
                                }
                                x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(height)
                            });
                        }
                    }
                    if let Some(entities) = layer["entityInstances"].as_array_mut() {
                        entities.retain_mut(|entity| {
                            let x = entity["px"][0].as_i64().unwrap_or(0) + cell_dx * g;
                            let y = entity["px"][1].as_i64().unwrap_or(0) + cell_dy * g;
                            entity["px"] = json!([x, y]);
                            let allowed = entity_defs
                                .iter()
                                .find(|d| d["uid"] == entity["defUid"])
                                .is_some_and(|d| d["allowOutOfBounds"] == true);
                            allowed
                                || (x >= 0
                                    && y >= 0
                                    && x < i64::from(width)
                                    && y < i64::from(height))
                        });
                    }
                    crate::field_edit::offset_point_fields(layer, cell_dx, cell_dy, nw, nh)?;
                    layer["pxOffsetX"] = json!(remainder_x);
                    layer["pxOffsetY"] = json!(remainder_y);
                }
            }
            p.grid_presence = grid_presence;
            p.refresh_auto_and_derived()
        })
    }
    pub fn move_entity(
        &mut self,
        key: LevelKey,
        layer: usize,
        index: usize,
        x: i32,
        y: i32,
    ) -> Result<()> {
        self.edit_document(|p| {
            let entity = p
                .layer_mut(key, layer)
                .and_then(|l| l["entityInstances"].get_mut(index))
                .ok_or_else(|| invalid("entity not found"))?;
            entity["px"] = json!([x, y]);
            p.refresh_derived_data()
        })
    }
    pub fn resize_entity(
        &mut self,
        key: LevelKey,
        layer: usize,
        index: usize,
        width: i32,
        height: i32,
    ) -> Result<()> {
        self.edit_document(|p| {
            let entity = p
                .layer(key, layer)
                .and_then(|l| l["entityInstances"].get(index))
                .ok_or_else(|| invalid("entity not found"))?;
            let d = p
                .definition("entities", entity["defUid"].as_i64().unwrap_or(-1))
                .ok_or_else(|| invalid("entity definition missing"))?;
            let initial = [
                entity["width"].as_i64().unwrap_or(1) as i32,
                entity["height"].as_i64().unwrap_or(1) as i32,
            ];
            for (axis, value, current) in [
                ("resizableX", width, initial[0]),
                ("resizableY", height, initial[1]),
            ] {
                if d[axis] != true && value != current {
                    return Err(invalid("entity dimension is not resizable"));
                }
            }
            let [width, height] = constrained_entity_size(d, [width, height], initial, None);
            let e =
                &mut p.layer_mut(key, layer).expect("validated layer")["entityInstances"][index];
            e["width"] = json!(width);
            e["height"] = json!(height);
            p.refresh_derived_data()
        })
    }
    pub fn remove_entity_with_references(
        &mut self,
        key: LevelKey,
        layer: usize,
        index: usize,
    ) -> Result<bool> {
        self.edit_document(|p| {
            let entities = p
                .layer_mut(key, layer)
                .and_then(|l| l["entityInstances"].as_array_mut())
                .ok_or_else(|| invalid("entity layer not found"))?;
            if index >= entities.len() {
                return Ok(false);
            }
            entities.remove(index);
            p.refresh_derived_data()?;
            Ok(true)
        })
    }
    pub fn rename_definition(&mut self, kind: &str, uid: i64, name: &str) -> Result<()> {
        if !valid_identifier(name) {
            return Err(invalid("invalid definition identifier"));
        }
        self.edit_document(|p| {
            let defs = p.json["defs"][kind]
                .as_array_mut()
                .ok_or_else(|| invalid("definition kind not found"))?;
            if defs
                .iter()
                .any(|d| d["uid"] != uid && d["identifier"] == name)
            {
                return Err(invalid("definition identifier already exists"));
            }
            let d = defs
                .iter_mut()
                .find(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("definition not found"))?;
            let old = d["identifier"].as_str().unwrap_or("").to_owned();
            d["identifier"] = json!(name);
            if kind == "enums" || kind == "externalEnums" {
                p.rename_enum_type_cache(kind, &old, name)?;
            }
            p.refresh_derived_data()
        })
    }
    pub fn reorder_layer(&mut self, uid: i64, index: usize) -> Result<()> {
        self.edit_document(|p| {
            let defs = p.json["defs"]["layers"]
                .as_array_mut()
                .ok_or_else(|| invalid("layer definitions missing"))?;
            let old = defs
                .iter()
                .position(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("layer definition not found"))?;
            if index >= defs.len() {
                return Err(invalid("layer index outside definition list"));
            }
            let d = defs.remove(old);
            defs.insert(index, d);
            let order: Vec<_> = defs.iter().map(|d| d["uid"].clone()).collect();
            for key in p.level_keys() {
                let layers = p
                    .level_mut(key)
                    .and_then(|l| l["layerInstances"].as_array_mut())
                    .ok_or_else(|| invalid("level layers not loaded"))?;
                layers.sort_by_key(|l| {
                    order
                        .iter()
                        .position(|uid| *uid == l["layerDefUid"])
                        .unwrap_or(usize::MAX)
                });
            }
            p.refresh_derived_data()
        })
    }
    pub fn update_layer_properties(&mut self, uid: i64, patch: &Value) -> Result<()> {
        self.edit_document(|p| {
            let fields = patch
                .as_object()
                .ok_or_else(|| invalid("property patch must be an object"))?;
            if fields
                .keys()
                .any(|k| ["uid", "type", "gridSize"].contains(&k.as_str()))
            {
                return Err(invalid("UID, type and grid size need explicit conversion"));
            }
            let target = p
                .layer_def(uid)
                .ok_or_else(|| invalid("layer definition not found"))?;
            let new_source_grid = if target["type"] == "AutoLayer"
                && let Some(source) = fields.get("autoSourceLayerDefUid").filter(|v| !v.is_null())
            {
                let source = source
                    .as_i64()
                    .and_then(|id| p.layer_def(id))
                    .filter(|layer| layer["type"] == "IntGrid")
                    .ok_or_else(|| invalid("auto-layer source must be an IntGrid layer"))?;
                let grid = source["gridSize"]
                    .as_i64()
                    .and_then(|n| i32::try_from(n).ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid("invalid source grid size"))?;
                (target["gridSize"] != grid).then_some(grid)
            } else {
                None
            };
            if let Some(blockers) = fields.get("layerUidsPreventingAutoTilingHere") {
                let blockers = blockers
                    .as_array()
                    .ok_or_else(|| invalid("blocker layers must be an array"))?;
                let mut seen = std::collections::BTreeSet::new();
                for blocker in blockers {
                    let blocker = blocker
                        .as_i64()
                        .ok_or_else(|| invalid("invalid blocker layer UID"))?;
                    let target = p
                        .layer_def(blocker)
                        .ok_or_else(|| invalid("blocker layer not found"))?;
                    if blocker == uid
                        || !seen.insert(blocker)
                        || !matches!(target["type"].as_str(), Some("Tiles" | "IntGrid"))
                    {
                        return Err(invalid("invalid blocker layer"));
                    }
                }
            }
            if let Some(name) = patch["identifier"].as_str() {
                p.rename_definition("layers", uid, name)?;
            }
            let defs = p.json["defs"]["layers"]
                .as_array_mut()
                .ok_or_else(|| invalid("layer definitions missing"))?;
            let d = defs
                .iter_mut()
                .find(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("layer definition not found"))?;
            for (k, v) in fields {
                d[k] = v.clone();
            }
            let palette_edit = d["type"] == "IntGrid"
                && (fields.contains_key("intGridValues")
                    || fields.contains_key("intGridValuesGroups"));
            let palette_only = palette_edit && fields.keys()
                .all(|k| matches!(k.as_str(), "intGridValues" | "intGridValuesGroups"));
            let mut palette_invalidated = Vec::new();
            if palette_edit {
                let mut group_ids = std::collections::BTreeSet::from([0]);
                for group in d["intGridValuesGroups"]
                    .as_array()
                    .ok_or_else(|| invalid("IntGrid groups must be an array"))?
                {
                    let id = group["uid"]
                        .as_i64()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| invalid("IntGrid group UID must be positive"))?;
                    if !group_ids.insert(id) {
                        return Err(invalid("duplicate IntGrid group UID"));
                    }
                }
                let mut palette = std::collections::BTreeSet::from([0]);
                for value in d["intGridValues"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("IntGrid values must be an array"))?
                {
                    let id = value["value"]
                        .as_i64()
                        .filter(|id| *id > 0 && *id <= i64::from(i32::MAX))
                        .ok_or_else(|| invalid("IntGrid value must be positive"))?;
                    if !palette.insert(id) {
                        return Err(invalid("duplicate IntGrid value"));
                    }
                    if !group_ids.contains(&value["groupUid"].as_i64().unwrap_or(0)) {
                        value["groupUid"] = json!(0);
                    }
                }
                let mut grid_presence = std::mem::take(&mut p.grid_presence);
                for key in p.level_keys() {
                    let mut removed = false;
                    let layers = p
                        .level_mut(key)
                        .and_then(|l| l["layerInstances"].as_array_mut())
                        .ok_or_else(|| invalid("level layers not loaded"))?;
                    for layer in layers {
                        let iid = layer["iid"].as_str().map(str::to_owned);
                        if layer["layerDefUid"] == uid
                            && let Some(csv) = layer["intGridCsv"].as_array_mut()
                        {
                            update_presence(&mut grid_presence, iid.as_deref(), |presence| {
                                for (index, value) in csv.iter_mut().enumerate() {
                                    if value.as_i64().is_none_or(|id| !palette.contains(&id)) {
                                        presence.write(index, value, json!(0));
                                        removed = true;
                                    }
                                }
                            });
                        }
                    }
                    if removed && palette_only {
                        palette_invalidated.push(key);
                    }
                }
                p.grid_presence = grid_presence;
            }
            if let Some(grid) = new_source_grid {
                p.change_layer_grid_size(uid, grid)
            } else if palette_only {
                // Removing a painted value invalidates the layer that lost its cells.
                for key in palette_invalidated {
                    p.rebuild_auto_layer(key, uid)?;
                }
                p.refresh_derived_data()
            } else {
                p.refresh_auto_and_derived()
            }
        })
    }
    pub fn remove_definition(&mut self, kind: &str, uid: i64) -> Result<()> {
        self.edit_document(|p| {
            let defs = p.json["defs"][kind]
                .as_array_mut()
                .ok_or_else(|| invalid("definition kind missing"))?;
            let index = defs
                .iter()
                .position(|d| d["uid"] == uid)
                .ok_or_else(|| invalid("definition not found"))?;
            let removed = defs.remove(index);
            if kind == "enums" || kind == "externalEnums" {
                p.remove_enum_field_definitions(
                    removed["identifier"].as_str().unwrap_or(""),
                    kind,
                )?;
                if let Some(tilesets) = p.json["defs"]["tilesets"].as_array_mut() {
                    for tileset in tilesets {
                        if tileset["tagsSourceEnumUid"] == uid {
                            tileset["tagsSourceEnumUid"] = Value::Null;
                            tileset["enumTags"] = json!([]);
                        }
                    }
                }
            }
            let mut grid_presence = std::mem::take(&mut p.grid_presence);
            for key in p.level_keys() {
                let level = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                if let Some(layers) = level["layerInstances"].as_array_mut() {
                    if kind == "layers" {
                        layers.retain(|layer| {
                            if layer["layerDefUid"] != uid {
                                return true;
                            }
                            if let Some(iid) = layer["iid"].as_str() {
                                grid_presence.remove(iid);
                            }
                            false
                        });
                    }
                    for layer in layers {
                        if kind == "entities"
                            && let Some(entities) = layer["entityInstances"].as_array_mut()
                        {
                            entities.retain(|e| e["defUid"] != uid);
                        }
                        if kind == "tilesets" && layer["__tilesetDefUid"] == uid {
                            layer["__tilesetDefUid"] = Value::Null;
                            layer["__tilesetRelPath"] = Value::Null;
                            layer["overrideTilesetUid"] = Value::Null;
                            layer["gridTiles"] = json!([]);
                            layer["autoLayerTiles"] = json!([]);
                        }
                    }
                }
            }
            p.grid_presence = grid_presence;
            if kind == "layers"
                && let Some(defs) = p.json["defs"]["layers"].as_array_mut()
            {
                for d in defs {
                    if let Some(blockers) = d["layerUidsPreventingAutoTilingHere"].as_array_mut() {
                        blockers.retain(|blocker| blocker.as_i64() != Some(uid));
                    }
                    for k in ["autoSourceLayerDefUid", "autoTilesKilledByOtherLayerUid"] {
                        if d[k] == uid {
                            d[k] = Value::Null;
                            if k == "autoSourceLayerDefUid" {
                                d["autoRuleGroups"] = json!([]);
                            }
                        }
                    }
                }
            }
            if kind == "tilesets" {
                clear_tileset_references(&mut p.json, uid);
                for l in p.external_levels.values_mut() {
                    clear_tileset_references(l, uid);
                }
            }
            p.refresh_auto_and_derived()
        })
    }
    pub(crate) fn refresh_table_of_contents(&mut self) -> Result<()> {
        let keys = self.level_keys();
        let old_toc = self.json["toc"].as_array();
        let old_export = self.json["flags"].as_array().is_some_and(|flags| {
            flags
                .iter()
                .any(|flag| flag == "ExportOldTableOfContentData")
        });
        let mut toc = Vec::new();
        for def in self.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|def| def["exportToToc"] == true)
        {
            let mut entry = old_toc
                .into_iter()
                .flatten()
                .find(|entry| entry["identifier"] == def["identifier"])
                .cloned()
                .unwrap_or_else(|| json!({}));
            entry["identifier"] = def["identifier"].clone();
            let mut instances = Vec::new();
            let mut data = Vec::new();
            for &key in &keys {
                let level = self
                    .level(key)
                    .ok_or_else(|| invalid("all external levels must be loaded"))?;
                let world_iid = if key.world.is_some() {
                    &container(self, key.world)?["iid"]
                } else {
                    &self.json["dummyWorldIid"]
                };
                for layer in level["layerInstances"].as_array().into_iter().flatten() {
                    for entity in layer["entityInstances"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|entity| entity["defUid"] == def["uid"])
                    {
                        let iids = json!({
                            "entityIid": entity["iid"],
                            "layerIid": layer["iid"],
                            "levelIid": level["iid"],
                            "worldIid": world_iid,
                        });
                        if old_export {
                            instances.push(iids.clone());
                        }
                        let mut fields = serde_json::Map::new();
                        for field in entity["fieldInstances"].as_array().into_iter().flatten() {
                            if def["fieldDefs"].as_array().is_some_and(|defs| {
                                defs.iter().any(|field_def| {
                                    field_def["uid"] == field["defUid"]
                                        && field_def["exportToToc"] == true
                                })
                            }) && let Some(name) = field["__identifier"].as_str()
                            {
                                fields.insert(name.to_owned(), field["__value"].clone());
                            }
                        }
                        let (world_x, world_y) = effective_position(self, key)?;
                        data.push(json!({
                            "iids": iids,
                            "worldX": world_x + entity["px"][0].as_i64().unwrap_or(0),
                            "worldY": world_y + entity["px"][1].as_i64().unwrap_or(0),
                            "widPx": entity["width"],
                            "heiPx": entity["height"],
                            "fields": fields,
                        }));
                    }
                }
            }
            entry["instances"] = json!(instances);
            entry["instancesData"] = json!(data);
            toc.push(entry);
        }
        if !toc.is_empty() || old_toc.is_some() || self.json["jsonVersion"] == "1.5.4" {
            self.json["toc"] = json!(toc);
        }
        Ok(())
    }

    /// Refresh exported values after structural edits, including references in other external documents.
    pub fn refresh_derived_data(&mut self) -> Result<()> {
        self.refresh_level_names()?;
        crate::field_edit::refresh_field_values(self)?;
        let keys = self.level_keys();
        for key in &keys {
            if self.level(*key).is_none() {
                return Err(invalid("all external levels must be loaded"));
            }
        }
        let layer_defs = self.json["defs"]["layers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let entity_defs = self.json["defs"]["entities"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let tilesets = self.json["defs"]["tilesets"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let enum_defs = [
            self.json["defs"]["enums"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            self.json["defs"]["externalEnums"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        ]
        .concat();
        let identifier_style = self.json["identifierStyle"]
            .as_str()
            .unwrap_or("Capitalize")
            .to_owned();
        let default_bg = self.json["defaultLevelBgColor"]
            .as_str()
            .or_else(|| self.json["bgColor"].as_str())
            .map(str::to_owned);
        let mut refs = BTreeMap::new();
        for key in &keys {
            let world = container(self, key.world)?;
            let world_iid = if key.world.is_some() {
                world["iid"].clone()
            } else {
                self.json["dummyWorldIid"].clone()
            };
            let free = world["worldLayout"]
                .as_str()
                .is_none_or(|l| l == "Free" || l == "GridVania");
            let level = self.level_mut(*key).expect("validated level");
            if let Some(bg) = level["bgColor"].as_str().or(default_bg.as_deref()) {
                level["__bgColor"] = json!(bg);
            }
            let level_iid = level["iid"].clone();
            let uid = level["uid"].clone();
            let wx = level["worldX"].as_i64().unwrap_or(0);
            let wy = level["worldY"].as_i64().unwrap_or(0);
            if let Some(layers) = level["layerInstances"].as_array_mut() {
                for layer in layers {
                    let d = layer_defs.iter().find(|d| d["uid"] == layer["layerDefUid"]);
                    if let Some(d) = d {
                        layer["__identifier"] = d["identifier"].clone();
                        layer["__opacity"] = d.get("displayOpacity").cloned().unwrap_or(json!(1));
                        layer["__pxTotalOffsetX"] = json!(
                            d["pxOffsetX"].as_i64().unwrap_or(0)
                                + layer["pxOffsetX"].as_i64().unwrap_or(0)
                        );
                        layer["__pxTotalOffsetY"] = json!(
                            d["pxOffsetY"].as_i64().unwrap_or(0)
                                + layer["pxOffsetY"].as_i64().unwrap_or(0)
                        );
                        let ts_uid = layer["overrideTilesetUid"]
                            .as_i64()
                            .or_else(|| d["tilesetDefUid"].as_i64());
                        layer["__tilesetDefUid"] = json!(ts_uid);
                        layer["__tilesetRelPath"] = tilesets
                            .iter()
                            .find(|ts| ts["uid"].as_i64() == ts_uid)
                            .map(|ts| ts["relPath"].clone())
                            .unwrap_or(Value::Null);
                    }
                    layer["levelId"] = uid.clone();
                    let liid = layer["iid"].clone();
                    let g = layer["__gridSize"].as_i64().unwrap_or(1).max(1);
                    if let Some(entities) = layer["entityInstances"].as_array_mut() {
                        for e in entities {
                            let x = e["px"][0].as_i64().unwrap_or(0);
                            let y = e["px"][1].as_i64().unwrap_or(0);
                            if let Some(d) = entity_defs.iter().find(|d| d["uid"] == e["defUid"]) {
                                e["__identifier"] = d["identifier"].clone();
                                e["__smartColor"] = smart_color(
                                    d["fieldDefs"].as_array(),
                                    e["fieldInstances"].as_array(),
                                    &enum_defs,
                                    &identifier_style,
                                )
                                .unwrap_or_else(|| d["color"].clone());
                                e["__tile"] = e["fieldInstances"]
                                    .as_array()
                                    .and_then(|fs| {
                                        fs.iter().find_map(|f| {
                                            (!f["__tile"].is_null()).then(|| f["__tile"].clone())
                                        })
                                    })
                                    .unwrap_or_else(|| d["tileRect"].clone());
                                e["__tags"] = d.get("tags").cloned().unwrap_or(json!([]));
                                e["__pivot"] = json!([
                                    canonical_number(d["pivotX"].as_f64().unwrap_or(0.)),
                                    canonical_number(d["pivotY"].as_f64().unwrap_or(0.))
                                ]);
                                e["__grid"] = json!([
                                    (x - i64::from(d["pivotX"].as_f64() == Some(1.))) / g,
                                    (y - i64::from(d["pivotY"].as_f64() == Some(1.))) / g
                                ]);
                            }
                            if free {
                                e["__worldX"] = json!(wx + x);
                                e["__worldY"] = json!(wy + y);
                            } else if let Some(e) = e.as_object_mut() {
                                e.remove("__worldX");
                                e.remove("__worldY");
                            }
                            if let Some(i) = e["iid"].as_str() && refs.insert(i.to_owned(),json!({"entityIid":i,"layerIid":liid,"levelIid":level_iid,"worldIid":world_iid})).is_some(){return Err(invalid("duplicate entity IID"));}
                        }
                    }
                }
            }
        }
        for key in &keys {
            let level = self.level_mut(*key).expect("validated level");
            visit_fields_mut(level, |f| refresh_refs(f, &refs));
        }
        self.refresh_table_of_contents()?;
        for key in &keys {
            let neighbours = level_neighbours(self, *key)?;
            let l = self.level_mut(*key).expect("validated level");
            l["__neighbours"] = json!(neighbours);
        }
        for key in keys {
            let rel = match container(self, key.world)?
                .get("levels")
                .and_then(|l| l.get(key.level))
                .and_then(|l| l["externalRelPath"].as_str())
            {
                Some(s) => s.to_owned(),
                None => continue,
            };
            let external = self
                .external_levels
                .get(&rel)
                .expect("validated external")
                .clone();
            let stub = &mut levels_mut(self, key.world)?[key.level];
            if let (Some(stub), Some(doc)) = (stub.as_object_mut(), external.as_object()) {
                for (k, v) in doc {
                    if [
                        "identifier",
                        "iid",
                        "uid",
                        "worldX",
                        "worldY",
                        "worldDepth",
                        "pxWid",
                        "pxHei",
                        "__bgColor",
                        "bgColor",
                        "useAutoIdentifier",
                        "bgRelPath",
                        "bgPos",
                        "bgPivotX",
                        "bgPivotY",
                        "__smartColor",
                        "__bgPos",
                        "fieldInstances",
                        "__neighbours",
                    ]
                    .contains(&k.as_str())
                    {
                        stub.insert(k.clone(), v.clone());
                    }
                }
                stub.insert("layerInstances".into(), Value::Null);
            }
        }
        Ok(())
    }
}
fn clear_tileset_references(v: &mut Value, uid: i64) {
    match v {
        Value::Object(m) => {
            for key in [
                "tilesetDefUid",
                "tilesetUid",
                "tilesetId",
                "iconTilesetUid",
                "overrideTilesetUid",
            ] {
                if m.get(key).is_some_and(|v| *v == uid) {
                    m.insert(key.into(), Value::Null);
                }
            }
            for value in m.values_mut() {
                clear_tileset_references(value, uid);
            }
        }
        Value::Array(a) => {
            for v in a {
                clear_tileset_references(v, uid);
            }
        }
        _ => {}
    }
}
fn refresh_refs(field: &mut Value, refs: &BTreeMap<String, Value>) {
    if !field["__type"]
        .as_str()
        .is_some_and(|s| s.contains("EntityRef"))
    {
        return;
    }
    let array = field["__type"]
        .as_str()
        .is_some_and(|s| s.starts_with("Array<"));
    if let Some(values) = field["realEditorValues"].as_array_mut() {
        if array {
            values.retain(|v| {
                v.is_null() || v["params"][0].as_str().is_none_or(|i| refs.contains_key(i))
            });
        } else {
            for v in values {
                if v["params"][0]
                    .as_str()
                    .is_some_and(|i| !refs.contains_key(i))
                {
                    *v = Value::Null;
                }
            }
        }
    }
    if array {
        if let Some(values) = field["__value"].as_array_mut() {
            values.retain_mut(|v| {
                if v.is_null() {
                    return true;
                }
                if let Some(new) = ref_iid(v).and_then(|iid| refs.get(iid)) {
                    *v = new.clone();
                    true
                } else {
                    false
                }
            });
        }
    } else {
        field["__value"] = ref_iid(&field["__value"])
            .and_then(|i| refs.get(i))
            .cloned()
            .unwrap_or(Value::Null);
    }
}
fn level_neighbours(p: &Project, key: LevelKey) -> Result<Vec<Value>> {
    let w = container(p, key.world)?;
    if w["worldLayout"]
        .as_str()
        .is_some_and(|l| l.starts_with("Linear"))
    {
        return Ok(vec![]);
    }
    let l = p.level(key).ok_or_else(|| invalid("level missing"))?;
    let x = l["worldX"].as_i64().unwrap_or(0);
    let y = l["worldY"].as_i64().unwrap_or(0);
    let r = x + l["pxWid"].as_i64().unwrap_or(0);
    let b = y + l["pxHei"].as_i64().unwrap_or(0);
    let z = l["worldDepth"].as_i64().unwrap_or(0);
    let mut overlaps = vec![];
    let mut touching = vec![];
    for other in p.level_keys() {
        if other == key || other.world != key.world {
            continue;
        }
        let o = p.level(other).ok_or_else(|| invalid("level missing"))?;
        let ox = o["worldX"].as_i64().unwrap_or(0);
        let oy = o["worldY"].as_i64().unwrap_or(0);
        let or = ox + o["pxWid"].as_i64().unwrap_or(0);
        let ob = oy + o["pxHei"].as_i64().unwrap_or(0);
        let oz = o["worldDepth"].as_i64().unwrap_or(0);
        if ox < r && or > x && oy < b && ob > y {
            if (oz - z).abs() <= 1 {
                overlaps.push(
                    json!({"levelIid":o["iid"],"dir":if oz==z{"o"}else if oz>z{">"}else{"<"}}),
                );
            }
        } else if oz == z && ox <= r && or >= x && oy <= b && ob >= y {
            let dir = if ox == r && oy == b {
                "se"
            } else if or == x && oy == b {
                "sw"
            } else if ox == r && ob == y {
                "ne"
            } else if or == x && ob == y {
                "nw"
            } else if ox >= r {
                "e"
            } else if or <= x {
                "w"
            } else if ob <= y {
                "n"
            } else {
                "s"
            };
            touching.push(json!({"levelIid":o["iid"],"dir":dir}));
        }
    }
    overlaps.extend(touching);
    Ok(overlaps)
}
impl Project {
    /// Rename a definition tag using LDtk's Free identifier rules.
    /// Entity-wide renames also update entity field and layer tag filters.
    /// Layer renames are scoped to layer UI tags.
    pub fn rename_definition_tag(
        &mut self,
        kind: &str,
        uid: i64,
        old: &str,
        new: &str,
        everywhere: bool,
    ) -> Result<()> {
        fn rename(tags: &mut Value, old: &str, new: &str) {
            if let Some(tags) = tags.as_array_mut()
                && tags.iter().any(|v| v == old)
            {
                tags.retain(|v| v != old);
                if !tags.iter().any(|v| v == new) {
                    tags.push(json!(new));
                }
            }
        }
        let new = Self::cleanup_tag(new).ok_or_else(|| invalid("invalid tag"))?;
        let key = match kind {
            "layers" => "uiFilterTags",
            "entities" | "tilesets" | "enums" => "tags",
            _ => return Err(invalid("unsupported tag definition kind")),
        };
        self.edit_document(|p| {
            let defs = p.json["defs"][kind]
                .as_array_mut()
                .ok_or_else(|| invalid("definitions missing"))?;
            if !defs.iter().any(|d| {
                d["uid"] == uid
                    && d[key]
                        .as_array()
                        .is_some_and(|t| t.iter().any(|v| v == old))
            }) {
                return Err(invalid("definition tag not found"));
            }
            if old == new {
                return Ok(());
            }
            for def in defs {
                if (everywhere || def["uid"] == uid)
                    && let Some(tags) = def.get_mut(key)
                {
                    rename(tags, old, &new);
                }
                if everywhere && kind == "entities" {
                    for field in def
                        .get_mut("fieldDefs")
                        .and_then(Value::as_array_mut)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(tags) = field.get_mut("allowedRefTags") {
                            rename(tags, old, &new);
                        }
                    }
                }
            }
            if everywhere && kind == "entities" {
                for layer in p.json["defs"]["layers"]
                    .as_array_mut()
                    .into_iter()
                    .flatten()
                {
                    for key in ["requiredTags", "excludedTags"] {
                        if let Some(tags) = layer.get_mut(key) {
                            rename(tags, old, &new);
                        }
                    }
                }
            }
            if kind == "layers" {
                Ok(())
            } else {
                p.refresh_derived_data()
            }
        })
    }

    pub fn update_entity_definition(&mut self, uid: i64, patch: &Value) -> Result<()> {
        self.edit_document(|p| {
            let old = p
                .definition("entities", uid)
                .ok_or_else(|| invalid("entity definition not found"))?
                .clone();
            let fields = patch
                .as_object()
                .ok_or_else(|| invalid("entity patch must be an object"))?;
            if fields
                .keys()
                .any(|k| ["uid", "fieldDefs"].contains(&k.as_str()))
            {
                return Err(invalid("UID and fields require explicit operations"));
            }
            if let Some(name) = patch["identifier"].as_str() {
                p.rename_definition("entities", uid, name)?;
            }
            let d = p.json["defs"]["entities"]
                .as_array_mut()
                .expect("found entities")
                .iter_mut()
                .find(|d| d["uid"] == uid)
                .expect("found entity");
            for (k, v) in fields {
                d[k] = v.clone();
            }
            for key in ["width", "height"] {
                if d[key]
                    .as_i64()
                    .is_none_or(|n| n <= 0 || n > i64::from(i32::MAX))
                {
                    return Err(invalid("entity dimensions must be positive integers"));
                }
            }
            for key in ["pivotX", "pivotY"] {
                if d[key].as_f64().is_none_or(|n| !(0. ..=1.).contains(&n)) {
                    return Err(invalid("entity pivot must be between zero and one"));
                }
            }
            for (min, max) in [("minWidth", "maxWidth"), ("minHeight", "maxHeight")] {
                if d[min]
                    .as_i64()
                    .zip(d[max].as_i64())
                    .is_some_and(|(a, b)| a > b)
                {
                    return Err(invalid("entity minimum exceeds maximum"));
                }
            }
            let new = d.clone();
            for key in p.level_keys() {
                let level = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                if let Some(ls) = level["layerInstances"].as_array_mut() {
                    for l in ls {
                        if let Some(es) = l["entityInstances"].as_array_mut() {
                            for e in es {
                                if e["defUid"] != uid {
                                    continue;
                                }
                                for (axis, resizable, min, max) in [
                                    ("width", "resizableX", "minWidth", "maxWidth"),
                                    ("height", "resizableY", "minHeight", "maxHeight"),
                                ] {
                                    let mut value =
                                        if new[resizable] != true || e[axis] == old[axis] {
                                            new[axis].as_i64().expect("validated size")
                                        } else {
                                            e[axis].as_i64().unwrap_or(1)
                                        };
                                    if let Some(m) = new[min].as_i64() {
                                        value = value.max(m);
                                    }
                                    if let Some(m) = new[max].as_i64() {
                                        value = value.min(m);
                                    }
                                    e[axis] = json!(value);
                                }
                                e["__smartColor"] = new["color"].clone();
                                e["__tile"] = new["tileRect"].clone();
                            }
                        }
                    }
                }
            }
            p.refresh_derived_data()
        })
    }
    pub fn update_tileset_definition(&mut self, uid: i64, patch: &Value) -> Result<()> {
        self.update_tileset_definition_with_policy(
            uid,
            patch,
            crate::definitions::TilesetRemap::AtlasCoordinates,
            false,
        )
        .map(|_| ())
    }
    pub fn update_definition(&mut self, kind: &str, uid: i64, patch: &Value) -> Result<()> {
        match kind {
            "entities" => self.update_entity_definition(uid, patch),
            "tilesets" => self.update_tileset_definition(uid, patch),
            "layers" => self.update_layer_properties(uid, patch),
            "enums" => self.update_enum_definition(uid, patch),
            _ => Err(invalid("unknown editable definition kind")),
        }
    }
}
impl Project {
    pub fn set_world_layout(
        &mut self,
        world: Option<usize>,
        layout: &str,
        grid_w: i32,
        grid_h: i32,
    ) -> Result<()> {
        self.edit_document(|p| {
            if !["Free", "GridVania", "LinearHorizontal", "LinearVertical"].contains(&layout)
                || grid_w <= 0
                || grid_h <= 0
            {
                return Err(invalid("invalid world layout or grid dimensions"));
            }
            let old = container(p, world)?["worldLayout"]
                .as_str()
                .unwrap_or("Free")
                .to_owned();
            let old_w = container(p, world)?["worldGridWidth"]
                .as_i64()
                .unwrap_or(256)
                .max(1);
            let old_h = container(p, world)?["worldGridHeight"]
                .as_i64()
                .unwrap_or(256)
                .max(1);
            let mut linear_pos = 0;
            for key in p.level_keys().into_iter().filter(|k| k.world == world) {
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                if layout.starts_with("Linear") {
                    l["worldX"] = json!(-1);
                    l["worldY"] = json!(-1);
                } else if layout == "GridVania" {
                    let (x, y) = if old == "LinearHorizontal" {
                        let x = linear_pos * i64::from(grid_w);
                        linear_pos += (l["pxWid"].as_i64().unwrap_or(1) + i64::from(grid_w) - 1)
                            / i64::from(grid_w);
                        (x, 0)
                    } else if old == "LinearVertical" {
                        let y = linear_pos * i64::from(grid_h);
                        linear_pos += (l["pxHei"].as_i64().unwrap_or(1) + i64::from(grid_h) - 1)
                            / i64::from(grid_h);
                        (0, y)
                    } else {
                        let denom_x = if old == "GridVania" {
                            old_w
                        } else {
                            i64::from(grid_w)
                        };
                        let denom_y = if old == "GridVania" {
                            old_h
                        } else {
                            i64::from(grid_h)
                        };
                        (
                            l["worldX"].as_i64().unwrap_or(0) / denom_x * i64::from(grid_w),
                            l["worldY"].as_i64().unwrap_or(0) / denom_y * i64::from(grid_h),
                        )
                    };
                    l["worldX"] = json!(x);
                    l["worldY"] = json!(y);
                } else if old.starts_with("Linear") {
                    if old == "LinearHorizontal" {
                        l["worldX"] = json!(linear_pos);
                        l["worldY"] = json!(0);
                        linear_pos += l["pxWid"].as_i64().unwrap_or(0) + 48;
                    } else {
                        l["worldY"] = json!(linear_pos);
                        l["worldX"] = json!(0);
                        linear_pos += l["pxHei"].as_i64().unwrap_or(0) + 48;
                    }
                }
            }
            let w = world_mut(p, world)?;
            w["worldLayout"] = json!(layout);
            w["worldGridWidth"] = json!(grid_w);
            w["worldGridHeight"] = json!(grid_h);
            if old != layout {
                tidy_gridvania_defaults(w);
            }
            p.refresh_derived_data()
        })
    }
    pub fn change_layer_grid_size(&mut self, uid: i64, new_grid: i32) -> Result<()> {
        self.edit_document(|p| {
            if new_grid <= 0 {
                return Err(invalid("grid size must be positive"));
            }
            p.layer_def(uid)
                .ok_or_else(|| invalid("layer definition not found"))?;
            let affected: Vec<_> = p.json["defs"]["layers"]
                .as_array()
                .expect("found layers")
                .iter()
                .filter(|d| d["uid"] == uid || d["autoSourceLayerDefUid"] == uid)
                .filter_map(|d| d["uid"].as_i64())
                .collect();
            let entities = p.json["defs"]["entities"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut grid_presence = std::mem::take(&mut p.grid_presence);
            for key in p.level_keys() {
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| invalid("level not loaded"))?;
                let pw = l["pxWid"].as_i64().unwrap_or(0);
                let ph = l["pxHei"].as_i64().unwrap_or(0);
                if let Some(ls) = l["layerInstances"].as_array_mut() {
                    for layer in ls {
                        if !affected.iter().any(|uid| layer["layerDefUid"] == *uid) {
                            continue;
                        }
                        let old = layer["__gridSize"]
                            .as_i64()
                            .filter(|n| *n > 0)
                            .ok_or_else(|| invalid("invalid old grid size"))?;
                        let ow = layer["__cWid"].as_i64().unwrap_or(0);
                        let g = i64::from(new_grid);
                        let nw = layer_cell_extent(pw, layer["pxOffsetX"].as_i64().unwrap_or(0), g);
                        let nh = layer_cell_extent(ph, layer["pxOffsetY"].as_i64().unwrap_or(0), g);
                        let cells = nw
                            .checked_mul(nh)
                            .filter(|n| *n <= 16_777_216)
                            .ok_or_else(|| invalid("grid allocation too large"))?;
                        match layer["__type"].as_str() {
                            Some("IntGrid") => {
                                let csv = layer["intGridCsv"]
                                    .as_array()
                                    .ok_or_else(|| invalid("IntGrid missing"))?;
                                let mut out = vec![json!(0); cells as usize];
                                if ow > 0 {
                                    for (i, v) in csv.iter().enumerate() {
                                        let x = i as i64 % ow;
                                        let y = i as i64 / ow;
                                        if x < nw && y < nh {
                                            out[(y * nw + x) as usize] = v.clone();
                                        }
                                    }
                                }
                                layer["intGridCsv"] = json!(out);
                                if old != g
                                    && let Some(iid) = layer["iid"].as_str()
                                {
                                    grid_presence.insert(iid.into(), GridPresence::empty());
                                }
                            }
                            Some("Tiles") => {
                                if let Some(tiles) = layer["gridTiles"].as_array_mut() {
                                    tiles.retain_mut(|t| {
                                        let x = t["px"][0].as_i64().unwrap_or(0) / old;
                                        let y = t["px"][1].as_i64().unwrap_or(0) / old;
                                        t["px"] = json!([x * g, y * g]);
                                        t["d"] = json!([y * nw + x]);
                                        x >= 0 && y >= 0 && x < nw && y < nh
                                    });
                                }
                            }
                            Some("Entities") => {
                                if let Some(es) = layer["entityInstances"].as_array_mut() {
                                    for e in es {
                                        for i in 0..2 {
                                            e["px"][i] = json!(
                                                (e["px"][i].as_f64().unwrap_or(0.) * g as f64
                                                    / old as f64)
                                                    .floor()
                                                    as i64
                                            );
                                        }
                                        if let Some(d) =
                                            entities.iter().find(|d| d["uid"] == e["defUid"])
                                        {
                                            for axis in ["width", "height"] {
                                                if e[axis] != d[axis] {
                                                    e[axis] = json!(
                                                        (e[axis].as_f64().unwrap_or(1.) * g as f64
                                                            / old as f64)
                                                            .floor()
                                                            .max(1.)
                                                            as i64
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        layer["__gridSize"] = json!(g);
                        layer["__cWid"] = json!(nw);
                        layer["__cHei"] = json!(nh);
                        layer["autoLayerTiles"] = json!([]);
                    }
                }
            }
            p.grid_presence = grid_presence;
            for d in p.json["defs"]["layers"]
                .as_array_mut()
                .expect("found layers")
            {
                if affected.iter().any(|uid| d["uid"] == *uid) {
                    d["gridSize"] = json!(new_grid);
                }
            }
            for key in p.level_keys() {
                p.rebuild_auto_layers(key)?;
            }
            p.refresh_derived_data()
        })
    }
    /// Explicitly discard incompatible content. Auto-layer tiles can be flattened into Tiles.
    pub fn convert_layer_type(&mut self, uid: i64, kind: &str, allow_loss: bool) -> Result<()> {
        self.edit_document(|p|{if !["IntGrid","Entities","Tiles","AutoLayer"].contains(&kind){return Err(invalid("unknown layer type"));}let d=p.layer_def(uid).ok_or_else(||invalid("layer definition not found"))?.clone();if d["type"]==kind{return Ok(());}
 let mut grid_presence = std::mem::take(&mut p.grid_presence);
 for key in p.level_keys(){let l=p.level_mut(key).ok_or_else(||invalid("level not loaded"))?;if let Some(ls)=l["layerInstances"].as_array_mut(){for layer in ls{if layer["layerDefUid"]!=uid{continue;}let loses_grid=kind!="IntGrid"&&layer["intGridCsv"].as_array().is_some_and(|vs|vs.iter().any(|v|*v!=0));let loses_entities=kind!="Entities"&&layer["entityInstances"].as_array().is_some_and(|vs|!vs.is_empty());let loses_tiles=kind!="Tiles"&&layer["gridTiles"].as_array().is_some_and(|vs|!vs.is_empty());if !allow_loss&&(loses_grid||loses_entities||loses_tiles){return Err(invalid("layer conversion would discard content; explicit allow_loss is required"));}
 if kind=="Tiles"&&d["type"]!="Tiles"{let w=layer["__cWid"].as_i64().unwrap_or(1).max(1);let g=layer["__gridSize"].as_i64().unwrap_or(1).max(1);let mut tiles=layer["autoLayerTiles"].as_array().cloned().unwrap_or_default();for t in &mut tiles{let x=t["px"][0].as_i64().unwrap_or(0)/g;let y=t["px"][1].as_i64().unwrap_or(0)/g;t["px"]=json!([x*g,y*g]);t["d"]=json!([y*w+x]);}layer["gridTiles"]=json!(tiles);}else if kind!="Tiles"{layer["gridTiles"]=json!([]);}
if kind!="Entities"{layer["entityInstances"]=json!([]);}
if let Some(iid) = layer["iid"].as_str() {
    if kind == "IntGrid" {
        grid_presence.insert(iid.into(), GridPresence::empty());
    } else {
        grid_presence.remove(iid);
    }
}
if kind=="IntGrid"{let count=layer["__cWid"].as_u64().unwrap_or(0).checked_mul(layer["__cHei"].as_u64().unwrap_or(0)).filter(|n|*n<=16_777_216).ok_or_else(||invalid("IntGrid allocation too large"))?;layer["intGridCsv"]=json!(vec![0;count as usize]);}else{layer["intGridCsv"]=json!([]);}layer["__type"]=json!(kind);layer["autoLayerTiles"]=json!([]);}}}
 p.grid_presence = grid_presence;
 let def=p.json["defs"]["layers"].as_array_mut().expect("found layers").iter_mut().find(|d|d["uid"]==uid).expect("found definition");def["type"]=json!(kind);def["__type"]=json!(kind);if kind=="Tiles"||kind=="Entities"{def["autoRuleGroups"]=json!([]);def["autoSourceLayerDefUid"]=Value::Null;}
if kind=="IntGrid"&&def["intGridValues"].as_array().is_none_or(|vs|vs.is_empty()){def["intGridValues"]=json!([{"value":1,"identifier":null,"color":"#000000","tile":null,"groupUid":0}]);}
if kind!="IntGrid"{for other in p.json["defs"]["layers"].as_array_mut().expect("found layers"){if other["autoSourceLayerDefUid"]==uid{other["autoSourceLayerDefUid"]=Value::Null;other["autoRuleGroups"]=json!([]);}}}p.refresh_derived_data()})
    }
}
impl Project {
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_entity(
        &mut self,
        from: LevelKey,
        from_layer: usize,
        index: usize,
        to: LevelKey,
        to_layer: usize,
        x: i32,
        y: i32,
    ) -> Result<()> {
        self.edit_document(|p| {
            let mut entity = p
                .layer(from, from_layer)
                .and_then(|l| l["entityInstances"].get(index))
                .cloned()
                .ok_or_else(|| invalid("entity not found"))?;
            let destination = p
                .layer(to, to_layer)
                .ok_or_else(|| invalid("destination layer not found"))?;
            if destination["__type"] != "Entities" {
                return Err(invalid("destination requires an Entities layer"));
            }
            destination["entityInstances"]
                .as_array()
                .ok_or_else(|| invalid("destination entity array missing"))?;
            let def = p
                .definition("entities", entity["defUid"].as_i64().unwrap_or(-1))
                .ok_or_else(|| invalid("entity definition missing"))?;
            if let Some(layer) = p.layer_def(destination["layerDefUid"].as_i64().unwrap_or(-1)) {
                let tags = def["tags"].as_array();
                let matches = |name| {
                    layer[name].as_array().is_some_and(|wanted| {
                        wanted
                            .iter()
                            .any(|t| tags.is_some_and(|tags| tags.contains(t)))
                    })
                };
                if (layer["requiredTags"]
                    .as_array()
                    .is_some_and(|tags| !tags.is_empty())
                    && !matches("requiredTags"))
                    || matches("excludedTags")
                {
                    return Err(invalid("destination layer excludes this entity's tags"));
                }
            }
            entity["px"] = json!([x, y]);
            p.layer_mut(from, from_layer).expect("validated source")["entityInstances"]
                .as_array_mut()
                .expect("validated source array")
                .remove(index);
            p.layer_mut(to, to_layer).expect("validated destination")["entityInstances"]
                .as_array_mut()
                .expect("validated destination array")
                .push(entity);
            p.refresh_derived_data()
        })
    }
}
impl Project {
    pub(crate) fn refresh_auto_and_derived(&mut self) -> Result<()> {
        self.refresh_derived_data()?;
        for key in self.level_keys() {
            self.rebuild_auto_layers(key)?;
        }
        self.refresh_derived_data()
    }
}
fn smart_color(
    defs: Option<&Vec<Value>>,
    fields: Option<&Vec<Value>>,
    enums: &[Value],
    identifier_style: &str,
) -> Option<Value> {
    for d in defs? {
        if d["useForSmartColor"] != true {
            continue;
        }
        let f = fields?.iter().find(|f| f["defUid"] == d["uid"])?;
        let values = if f["__type"]
            .as_str()
            .is_some_and(|s| s.starts_with("Array<"))
        {
            f["__value"].as_array().cloned().unwrap_or_default()
        } else {
            vec![f["__value"].clone()]
        };
        for v in values {
            if v.is_null() {
                continue;
            }
            if f["__type"].as_str().is_some_and(|s| s.contains("Color")) {
                return Some(v);
            }
            if let Some(name) = f["__type"]
                .as_str()
                .and_then(|s| s.trim_end_matches('>').split_once('.').map(|(_, s)| s))
                && let Some(color) = enums
                    .iter()
                    .find(|e| e["identifier"] == name)
                    .and_then(|e| {
                        let value = if e["externalRelPath"].is_string() {
                            v.clone()
                        } else {
                            v.as_str()
                                .map(|s| json!(crate::naming::cleanup(s, identifier_style)))
                                .unwrap_or(v.clone())
                        };
                        e["values"].as_array()?.iter().find(|ev| ev["id"] == value)
                    })
                    .map(|ev| ev["color"].clone())
                    .filter(|v| !v.is_null())
            {
                if let Some(c) = color.as_i64() {
                    return Some(json!(format!("#{:06X}", c & 0xFFFFFF)));
                }
                return Some(color);
            }
        }
    }
    None
}
fn canonical_number(n: f64) -> Value {
    let n = (n * 1000. + 0.5).floor() / 1000.;
    if n.fract() == 0. {
        json!(n as i64)
    } else {
        json!(n)
    }
}
pub(crate) fn effective_position(p: &Project, key: LevelKey) -> Result<(i64, i64)> {
    let world = container(p, key.world)?;
    let layout = world["worldLayout"].as_str().unwrap_or("Free");
    if layout.starts_with("Linear") {
        let mut pos = 0;
        for other in p
            .level_keys()
            .into_iter()
            .filter(|other| other.world == key.world)
        {
            if other == key {
                break;
            }
            let l = p.level(other).ok_or_else(|| invalid("level not loaded"))?;
            pos += l[if layout == "LinearHorizontal" {
                "pxWid"
            } else {
                "pxHei"
            }]
            .as_i64()
            .unwrap_or(0)
                + 48;
        }
        Ok(if layout == "LinearHorizontal" {
            (pos, 0)
        } else {
            (0, pos)
        })
    } else {
        let l = p.level(key).ok_or_else(|| invalid("level not loaded"))?;
        Ok((
            l["worldX"].as_i64().unwrap_or(0),
            l["worldY"].as_i64().unwrap_or(0),
        ))
    }
}
impl Project {
    pub fn move_level_to_world(
        &mut self,
        key: LevelKey,
        world: Option<usize>,
        index: usize,
    ) -> Result<LevelKey> {
        self.edit_document(|p| {
            p.level(key)
                .ok_or_else(|| invalid("source level not loaded"))?;
            let target_len = container(p, world)?["levels"]
                .as_array()
                .ok_or_else(|| invalid("destination levels missing"))?
                .len()
                - usize::from(key.world == world);
            if index > target_len {
                return Err(invalid("destination level index outside world"));
            }
            let level = levels_mut(p, key.world)?.remove(key.level);
            levels_mut(p, world)?.insert(index, level);
            p.refresh_derived_data()?;
            Ok(LevelKey {
                world,
                level: index,
            })
        })
    }
    pub fn duplicate_world(&mut self, index: usize, name: &str) -> Result<usize> {
        self.edit_document(|p| {
            let original = container(p, Some(index))?.clone();
            let source_keys: Vec<_> = p
                .level_keys()
                .into_iter()
                .filter(|k| k.world == Some(index))
                .collect();
            let source_levels = source_keys
                .iter()
                .map(|k| {
                    p.level(*k)
                        .cloned()
                        .ok_or_else(|| invalid("source level not loaded"))
                })
                .collect::<Result<Vec<_>>>()?;
            let new_index = p.add_world(name)?;
            let mut world = original;
            world["iid"] = iid();
            world["identifier"] = json!(name);
            let mut levels = vec![];
            let mut remaps = BTreeMap::new();
            let mut names = std::collections::BTreeSet::new();
            for mut level in source_levels {
                let uid = alloc_uid(p)?;
                level["uid"] = json!(uid);
                level["iid"] = iid();
                level["externalRelPath"] = Value::Null;
                level["useAutoIdentifier"] = json!(false);
                let original = level["identifier"].as_str().unwrap_or("Level");
                let mut name = format!("{original}_copy");
                let mut suffix = 2;
                while names.contains(&name) || unique_level(p, &name).is_err() {
                    name = format!("{original}_copy{suffix}");
                    suffix += 1;
                }
                names.insert(name.clone());
                level["identifier"] = json!(name);
                if let Some(layers) = level["layerInstances"].as_array_mut() {
                    for layer in layers {
                        layer["iid"] = iid();
                        layer["levelId"] = json!(uid);
                        if let Some(es) = layer["entityInstances"].as_array_mut() {
                            for e in es {
                                let old = e["iid"]
                                    .as_str()
                                    .ok_or_else(|| invalid("entity IID missing"))?
                                    .to_owned();
                                let new = iid();
                                remaps.insert(old, new.clone());
                                e["iid"] = new;
                            }
                        }
                    }
                }
                levels.push(level);
            }
            for level in &mut levels {
                remap_refs(level, &remaps, true);
            }
            world["levels"] = json!(levels);
            p.json["worlds"][new_index] = world;
            p.refresh_derived_data()?;
            Ok(new_index)
        })
    }
}
/// Preview a right/bottom entity handle without allocating or changing a document.
/// A height-only request uses the original vertical-handle aspect-ratio rule.
pub fn constrained_entity_size(
    def: &Value,
    requested: [i32; 2],
    initial: [i32; 2],
    snap: Option<i32>,
) -> [i32; 2] {
    let resizable = [def["resizableX"] == true, def["resizableY"] == true];
    let mut size = initial.map(i64::from);
    for axis in 0..2 {
        if resizable[axis] {
            size[axis] = i64::from(requested[axis]);
            if requested[axis] != initial[axis]
                && let Some(grid) = snap.filter(|g| *g > 0)
            {
                size[axis] =
                    ((size[axis] as f64 / f64::from(grid) + 0.5).floor() as i64) * i64::from(grid);
            }
        }
    }
    if resizable[0] && resizable[1] && def["keepAspectRatio"] == true {
        let ratio = def["height"]
            .as_f64()
            .unwrap_or(f64::from(initial[1]))
            .max(1.)
            / def["width"]
                .as_f64()
                .unwrap_or(f64::from(initial[0]))
                .max(1.);
        if requested[0] == initial[0] && requested[1] != initial[1] {
            size[0] = (size[1] as f64 / ratio + 0.5).floor() as i64;
        } else {
            size[1] = (size[0] as f64 * ratio + 0.5).floor() as i64;
        }
    }
    for (axis, (min, max)) in [("minWidth", "maxWidth"), ("minHeight", "maxHeight")]
        .into_iter()
        .enumerate()
    {
        if let Some(n) = def[min].as_i64() {
            size[axis] = size[axis].max(n);
        }
        if let Some(n) = def[max].as_i64() {
            size[axis] = size[axis].min(n);
        }
        size[axis] = size[axis].clamp(1, i64::from(i32::MAX));
    }
    size.map(|v| v as i32)
}
/// Original EntityTool anchors a snapped placement at cell origin plus its pivot.
pub fn snapped_entity_position(def: &Value, local: [f32; 2], grid: i32, snap: bool) -> [i32; 2] {
    let local = local.map(|v| (f64::from(v) + 0.5).floor());
    if !snap || grid <= 0 {
        return local.map(|v| v as i32);
    }
    let grid = f64::from(grid);
    ["pivotX", "pivotY"].map(|key| {
        let axis = usize::from(key == "pivotY");
        let cell = (local[axis] / grid).floor();
        ((cell + def[key].as_f64().unwrap_or(0.)) * grid + 0.5).floor() as i32
    })
}

pub(crate) fn entity_out_of_layer_bounds(p: &Project, layer: &Value, entity: &Value) -> bool {
    let definition = p.layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1));
    let factor = definition
        .and_then(|d| d["parallaxFactorX"].as_f64())
        .unwrap_or(0.);
    let scale = if definition.is_some_and(|d| d["parallaxScaling"] == true) && factor != 0. {
        (1. - factor).max(0.01)
    } else {
        1.
    };
    let grid = layer["__gridSize"].as_f64().unwrap_or(1.) * scale;
    (0..2).any(|axis| {
        let offset = layer[if axis == 0 {
            "__pxTotalOffsetX"
        } else {
            "__pxTotalOffsetY"
        }]
        .as_f64()
        .unwrap_or(0.);
        let limit = layer[if axis == 0 { "__cWid" } else { "__cHei" }]
            .as_f64()
            .unwrap_or(0.)
            * grid;
        let px = entity["px"][axis].as_f64().unwrap_or(0.);
        px < offset || px > offset + limit
    })
}
impl Project {
    /// Finish a drag and tidy just its entity. Returns true when the entity was removed.
    pub fn finish_entity_transform(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        index: usize,
    ) -> Result<bool> {
        self.edit_document(|p| {
            let layer = p
                .layer(key, layer_index)
                .ok_or_else(|| invalid("entity layer missing"))?;
            let entity = layer["entityInstances"]
                .get(index)
                .ok_or_else(|| invalid("entity not found"))?;
            let def = p
                .definition("entities", entity["defUid"].as_i64().unwrap_or(-1))
                .ok_or_else(|| invalid("entity definition missing"))?;
            let remove =
                def["allowOutOfBounds"] != true && entity_out_of_layer_bounds(p, layer, entity);
            if remove {
                p.layer_mut(key, layer_index).expect("validated layer")["entityInstances"]
                    .as_array_mut()
                    .expect("entity array")
                    .remove(index);
            } else {
                let mut touched = json!({"entityInstances":[entity.clone()]});
                crate::field_edit::offset_point_fields(
                    &mut touched,
                    0,
                    0,
                    layer["__cWid"].as_i64().unwrap_or(0),
                    layer["__cHei"].as_i64().unwrap_or(0),
                )?;
                p.layer_mut(key, layer_index).expect("validated layer")["entityInstances"][index] =
                    touched["entityInstances"][0].take();
            }
            p.refresh_derived_data()?;
            Ok(remove)
        })
    }
}

#[path = "entity_transfer.rs"]
mod entity_transfer;
