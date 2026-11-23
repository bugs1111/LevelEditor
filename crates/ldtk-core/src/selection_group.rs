//! Mixed-layer selections retain stable object identities and original layer-local data.
use crate::workflow::{iid, invalid, remap_field_refs};
use crate::{DeferredAutoLayers, Error, LevelKey, Project, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct SelectionDragReport {
    pub out_of_bounds: Vec<String>,
    pub copy_limits: Vec<(i64, String)>,
}
pub fn selection_group_is_entity_only(group: &Value) -> bool {
    let Some(layers) = group["layers"].as_array() else {
        return false;
    };
    let mut found = false;
    for entry in layers {
        if entry["points"].as_array().is_some_and(|v| !v.is_empty())
            || entry["cellMask"].as_array().is_some_and(|v| !v.is_empty())
            || (group["emptySpace"] == true
                && matches!(
                    entry["clipboard"]["type"].as_str(),
                    Some("IntGrid" | "Tiles")
                ))
            || entry["clipboard"]["gridTiles"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
            || entry["clipboard"]["intGridCsv"]
                .as_array()
                .is_some_and(|v| v.iter().any(|n| n.as_i64().unwrap_or(0) != 0))
        {
            return false;
        }
        found |= entry["clipboard"]["entityInstances"]
            .as_array()
            .is_some_and(|v| !v.is_empty());
    }
    found
}
fn n(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or(0)
}
fn pair(v: &Value) -> Result<[i64; 2]> {
    Ok([
        v[0].as_i64()
            .ok_or_else(|| invalid("selection coordinate missing"))?,
        v[1].as_i64()
            .ok_or_else(|| invalid("selection coordinate missing"))?,
    ])
}
fn layer_index(p: &Project, key: LevelKey, entry: &Value) -> Result<usize> {
    p.level(key)
        .and_then(|l| l["layerInstances"].as_array())
        .and_then(|ls| {
            ls.iter()
                .position(|l| l["layerDefUid"] == entry["layerDefUid"])
        })
        .ok_or_else(|| invalid("selected layer no longer exists"))
}
fn grid(layer: &Value) -> Result<i64> {
    layer["__gridSize"]
        .as_i64()
        .filter(|g| *g > 0)
        .ok_or_else(|| invalid("invalid selection layer grid"))
}
fn fields_mut<'a>(
    p: &'a mut Project,
    key: LevelKey,
    index: usize,
    owner: &str,
) -> Result<&'a mut Vec<Value>> {
    p.layer_mut(key, index)
        .and_then(|l| l["entityInstances"].as_array_mut())
        .and_then(|es| es.iter_mut().find(|e| e["iid"] == owner))
        .and_then(|e| e["fieldInstances"].as_array_mut())
        .ok_or_else(|| invalid("selected point owner no longer exists"))
}
fn point_value(field: &Value, index: usize) -> Value {
    if field["__type"] == "Array<Point>" {
        field["__value"][index].clone()
    } else {
        field["__value"].clone()
    }
}
fn put_point(field: &mut Value, index: usize, point: Option<[i64; 2]>, insert: bool) -> Result<()> {
    let array = field["__type"] == "Array<Point>";
    if insert && point.is_none() {
        return Ok(());
    }
    if !field["realEditorValues"].is_array() {
        field["realEditorValues"] = json!([]);
    }
    let values = field["realEditorValues"]
        .as_array_mut()
        .expect("created editor values");
    if let Some([x, y]) = point {
        let wrapper = json!({"id":"V_String","params":[format!("{x},{y}")]});
        if array && insert {
            if index > values.len() {
                return Err(invalid("selected point index changed"));
            }
            values.insert(index, wrapper);
        } else {
            if index >= values.len() {
                if array {
                    return Err(invalid("selected point index changed"));
                }
                values.resize(index + 1, Value::Null);
            }
            values[index] = wrapper;
        }
    } else if index < values.len() {
        values.remove(index);
    }
    Ok(())
}
fn points_in(layer: &Value, rect: [i64; 4], all: bool) -> Vec<Value> {
    let mut out = vec![];
    for entity in layer["entityInstances"].as_array().into_iter().flatten() {
        for field in entity["fieldInstances"].as_array().into_iter().flatten() {
            let kind = field["__type"].as_str().unwrap_or("");
            if kind != "Point" && kind != "Array<Point>" {
                continue;
            }
            let values = if kind == "Array<Point>" {
                field["__value"].as_array().cloned().unwrap_or_default()
            } else {
                vec![field["__value"].clone()]
            };
            for (index, value) in values.into_iter().enumerate() {
                let (Some(x), Some(y)) = (value["cx"].as_i64(), value["cy"].as_i64()) else {
                    continue;
                };
                if all
                    || (x >= rect[0]
                        && y >= rect[1]
                        && x < rect[0] + rect[2]
                        && y < rect[1] + rect[3])
                {
                    out.push(json!({"entityIid":entity["iid"],"fieldDefUid":field["defUid"],"arrayIndex":index,"isArray":kind=="Array<Point>","value":value}));
                }
            }
        }
    }
    out
}
fn capture_layer(
    p: &Project,
    key: LevelKey,
    index: usize,
    rect: [i32; 4],
    all: bool,
) -> Result<Option<Value>> {
    let layer = p
        .layer(key, index)
        .ok_or_else(|| invalid("selection layer not found"))?;
    if !matches!(
        layer["__type"].as_str(),
        Some("IntGrid" | "Tiles" | "Entities")
    ) {
        return Ok(None);
    }
    let g = grid(layer)?;
    let offset = [n(layer, "__pxTotalOffsetX"), n(layer, "__pxTotalOffsetY")];
    let cell = if all {
        [0, 0, n(layer, "__cWid"), n(layer, "__cHei")]
    } else {
        let left = (i64::from(rect[0]) - offset[0]) / g;
        let top = (i64::from(rect[1]) - offset[1]) / g;
        let right = (i64::from(rect[0]) + i64::from(rect[2]) - 1 - offset[0]) / g;
        let bottom = (i64::from(rect[1]) + i64::from(rect[3]) - 1 - offset[1]) / g;
        [left, top, right - left + 1, bottom - top + 1]
    };
    let cell32 = cell.map(|v| {
        i32::try_from(v).map_err(|_| invalid("selection rectangle exceeds integer range"))
    });
    let [a, b, c, d] = cell32;
    let mut clipboard = p.copy_region(key, index, [a?, b?, c?, d?])?;
    if layer["__type"] == "Entities" {
        let mut selected = vec![];
        for entity in layer["entityInstances"].as_array().into_iter().flatten() {
            let px = pair(&entity["px"])?;
            let pivot = &entity["__pivot"];
            let cx = (px[0] - i64::from(pivot[0].as_f64() == Some(1.))) / g;
            let cy = (px[1] - i64::from(pivot[1].as_f64() == Some(1.))) / g;
            if all
                || (cx >= cell[0]
                    && cy >= cell[1]
                    && cx < cell[0] + cell[2]
                    && cy < cell[1] + cell[3])
            {
                let mut e = entity.clone();
                e["px"] = json!([px[0] - cell[0] * g, px[1] - cell[1] * g]);
                selected.push(e);
            }
        }
        clipboard["entityInstances"] = Value::Array(selected);
    }
    let mask = clipboard["intGridCsv"]
        .as_array()
        .map(|vs| {
            vs.iter()
                .enumerate()
                .filter_map(|(i, v)| (v.as_i64().unwrap_or(0) != 0).then_some(i))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let points = points_in(layer, cell, all);
    let mut entry = json!({
        "layerDefUid": layer["layerDefUid"], "identifier": layer["__identifier"],
        "layerIndex": index, "origin": [offset[0]+cell[0]*g, offset[1]+cell[1]*g], "rect": cell
    });
    entry["clipboard"] = clipboard;
    entry["cellMask"] = mask.into();
    entry["points"] = Value::Array(points);
    Ok(Some(entry))
}
fn empty_group(p: &Project, key: LevelKey, rect: [i32; 4]) -> Result<Value> {
    crate::selection::rect_valid(rect)?;
    let level = p.level(key).ok_or_else(|| invalid("level not loaded"))?;
    Ok(json!({
        "type":"SelectionGroup", "levelIid":level["iid"], "origin":[rect[0],rect[1]],
        "size":[rect[2],rect[3]], "gridSize":1, "compatibleGrids":true, "emptySpace":false,
        "layers":[], "elementCount":0
    }))
}
fn build(p: &Project, key: LevelKey, layers: &[usize], rect: [i32; 4], all: bool) -> Result<Value> {
    let mut group = empty_group(p, key, rect)?;
    let mut entries = Vec::new();
    let mut unique = BTreeSet::new();
    for &index in layers {
        if unique.insert(index)
            && let Some(entry) = capture_layer(p, key, index, rect, all)?
        {
            entries.push(entry);
        }
    }
    group["layers"] = Value::Array(entries);
    update_group_stats(&mut group);
    Ok(group)
}
fn selected_ids(group: &Value) -> BTreeSet<String> {
    group["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| {
            e["clipboard"]["entityInstances"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|e| e["iid"].as_str().map(str::to_owned))
        .collect()
}
fn transform(
    point: [i64; 2],
    origin: [i64; 2],
    size: [i64; 2],
    target: [i64; 2],
    extent: [i64; 2],
    flips: [bool; 2],
) -> [i64; 2] {
    std::array::from_fn(|i| {
        target[i]
            + if flips[i] {
                size[i] - (point[i] - origin[i]) - extent[i]
            } else {
                point[i] - origin[i]
            }
    })
}
fn delete(p: &mut Project, key: LevelKey, group: &Value, delete_objects: bool) -> Result<usize> {
    if group["type"] != "SelectionGroup" {
        return Err(invalid("not a selection group"));
    }
    let ids = selected_ids(group);
    let mut count = 0;
    for entry in group["layers"]
        .as_array()
        .ok_or_else(|| invalid("selection layers missing"))?
    {
        let index = layer_index(p, key, entry)?;
        let layer = p.layer(key, index).expect("selected layer");
        let g = grid(layer)?;
        let cell = pair(&entry["rect"])?;
        let cw = n(&entry["clipboard"], "width");
        let width = n(layer, "__cWid");
        let height = n(layer, "__cHei");
        p.edit_layer_presence(key, index, |layer, presence| {
            if let Some(csv) = layer["intGridCsv"].as_array_mut() {
                for mask in entry["cellMask"].as_array().into_iter().flatten() {
                    let i = mask
                        .as_i64()
                        .ok_or_else(|| invalid("selection cell index invalid"))?;
                    if cw <= 0 {
                        return Err(invalid("selection width invalid"));
                    }
                    let x = cell[0] + i % cw;
                    let y = cell[1] + i / cw;
                    if x >= 0
                        && y >= 0
                        && x < width
                        && y < height
                        && let Some(v) = csv.get_mut((y * width + x) as usize)
                    {
                        presence.write((y * width + x) as usize, v, json!(0));
                        count += 1;
                    }
                }
            }
            Ok(())
        })?;
        let layer = p.layer_mut(key, index).expect("selected layer");
        let tile_cells: BTreeSet<_> = entry["clipboard"]["gridTiles"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|tile| pair(&tile["px"]).map(|px| (px[0] + cell[0] * g, px[1] + cell[1] * g)))
            .collect::<Result<_>>()?;
        if let Some(tiles) = layer["gridTiles"].as_array_mut() {
            let before = tiles.len();
            tiles.retain(|t| {
                !tile_cells.contains(&(
                    t["px"][0].as_i64().unwrap_or(-1),
                    t["px"][1].as_i64().unwrap_or(-1),
                ))
            });
            count += before - tiles.len();
        }
        if delete_objects {
            if let Some(entities) = layer["entityInstances"].as_array_mut() {
                let before = entities.len();
                entities.retain(|e| !e["iid"].as_str().is_some_and(|id| ids.contains(id)));
                count += before - entities.len();
            }
            let mut points = entry["points"].as_array().cloned().unwrap_or_default();
            points
                .sort_by_key(|point| std::cmp::Reverse(point["arrayIndex"].as_u64().unwrap_or(0)));
            for point in points {
                let owner = point["entityIid"]
                    .as_str()
                    .ok_or_else(|| invalid("point owner missing"))?;
                if ids.contains(owner) {
                    continue;
                }
                let uid = point["fieldDefUid"].clone();
                let f = fields_mut(p, key, index, owner)?
                    .iter_mut()
                    .find(|f| f["defUid"] == uid)
                    .ok_or_else(|| invalid("selected point field missing"))?;
                put_point(
                    f,
                    point["arrayIndex"].as_u64().unwrap_or(0) as usize,
                    None,
                    false,
                )?;
                count += 1;
            }
        }
    }
    Ok(count)
}
impl Project {
    pub fn copy_selection_group(
        &self,
        key: LevelKey,
        layers: &[usize],
        rect_px: [i32; 4],
    ) -> Result<Value> {
        build(self, key, layers, rect_px, false)
    }
    pub fn select_all_group(&self, key: LevelKey, layers: &[usize]) -> Result<Value> {
        let l = self.level(key).ok_or_else(|| invalid("level not loaded"))?;
        let w = i32::try_from(n(l, "pxWid")).map_err(|_| invalid("invalid level width"))?;
        let h = i32::try_from(n(l, "pxHei")).map_err(|_| invalid("invalid level height"))?;
        build(self, key, layers, [0, 0, w, h], true)
    }
    /// Resolve stable selected objects from their current values before copy or movement.
    pub fn refresh_selection_group(&self, key: LevelKey, group: &Value) -> Result<Value> {
        let level = self.level(key).ok_or_else(|| invalid("level not loaded"))?;
        if group["levelIid"] != level["iid"] {
            return Err(invalid("selection belongs to a different level"));
        }
        let mut group = group.clone();
        for entry in group["layers"]
            .as_array_mut()
            .ok_or_else(|| invalid("selection layers missing"))?
        {
            let index = layer_index(self, key, entry)?;
            let layer = self.layer(key, index).expect("selected layer");
            let g = grid(layer)?;
            if entry["clipboard"]["gridSize"] != g {
                return Err(invalid("selected layer grid changed; reselect its objects"));
            }
            let cell = pair(&entry["rect"])?;
            let cw = n(&entry["clipboard"], "width");
            let width = n(layer, "__cWid");
            let height = n(layer, "__cHei");
            let mut mask = vec![];
            let mut csv = entry["clipboard"]["intGridCsv"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for value in entry["cellMask"].as_array().into_iter().flatten() {
                let i = value
                    .as_i64()
                    .ok_or_else(|| invalid("selection cell index invalid"))?;
                if cw <= 0 {
                    return Err(invalid("selection width invalid"));
                }
                let x = cell[0] + i % cw;
                let y = cell[1] + i / cw;
                if x >= 0
                    && y >= 0
                    && x < width
                    && y < height
                    && let Some(v) = layer["intGridCsv"].get((y * width + x) as usize)
                    && v.as_i64().unwrap_or(0) != 0
                    && let Some(dst) = csv.get_mut(i as usize)
                {
                    *dst = v.clone();
                    mask.push(value.clone());
                }
            }
            let tile_cells: BTreeSet<_> = entry["clipboard"]["gridTiles"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| pair(&t["px"]).map(|p| (p[0] + cell[0] * g, p[1] + cell[1] * g)))
                .collect::<Result<_>>()?;
            let mut tiles = vec![];
            for t in layer["gridTiles"].as_array().into_iter().flatten() {
                let px = pair(&t["px"])?;
                if tile_cells.contains(&(px[0], px[1])) {
                    let mut t = t.clone();
                    t["px"] = json!([px[0] - cell[0] * g, px[1] - cell[1] * g]);
                    tiles.push(t);
                }
            }
            let mut entities = vec![];
            for selected in entry["clipboard"]["entityInstances"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(current) = layer["entityInstances"]
                    .as_array()
                    .and_then(|es| es.iter().find(|e| e["iid"] == selected["iid"]))
                {
                    let mut e = current.clone();
                    let px = pair(&e["px"])?;
                    e["px"] = json!([px[0] - cell[0] * g, px[1] - cell[1] * g]);
                    entities.push(e);
                }
            }
            let mut points = vec![];
            for point in entry["points"].as_array().into_iter().flatten() {
                let current = layer["entityInstances"]
                    .as_array()
                    .and_then(|es| es.iter().find(|e| e["iid"] == point["entityIid"]))
                    .and_then(|e| e["fieldInstances"].as_array())
                    .and_then(|fs| fs.iter().find(|f| f["defUid"] == point["fieldDefUid"]));
                if let Some(f) = current {
                    let value = point_value(f, point["arrayIndex"].as_u64().unwrap_or(0) as usize);
                    if !value.is_null() {
                        let mut point = point.clone();
                        point["value"] = value;
                        points.push(point);
                    }
                }
            }
            entry["cellMask"] = json!(mask);
            entry["points"] = json!(points);
            entry["clipboard"]["intGridCsv"] = json!(csv);
            entry["clipboard"]["gridTiles"] = json!(tiles);
            entry["clipboard"]["entityInstances"] = json!(entities);
        }
        update_group_stats(&mut group);
        Ok(group)
    }
    pub fn cut_selection_group(
        &mut self,
        key: LevelKey,
        layers: &[usize],
        rect_px: [i32; 4],
    ) -> Result<Value> {
        self.edit_document(|p| {
            let group = p.copy_selection_group(key, layers, rect_px)?;
            delete(p, key, &group, true)?;
            p.refresh_auto_and_derived()?;
            Ok(group)
        })
    }
    pub fn delete_selection_group(&mut self, key: LevelKey, group: &Value) -> Result<usize> {
        self.edit_document(|p| {
            let group = p.refresh_selection_group(key, group)?;
            let count = delete(p, key, &group, true)?;
            p.refresh_auto_and_derived()?;
            Ok(count)
        })
    }
    pub fn paste_selection_group(
        &mut self,
        key: LevelKey,
        group: &Value,
        origin_px: [i32; 2],
        flip_x: bool,
        flip_y: bool,
        stack: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            apply(
                p,
                key,
                group,
                origin_px.map(i64::from),
                [flip_x, flip_y],
                stack,
                true,
            )
            .map(|(count, _)| count)
        })
    }
    pub fn move_selection_group(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta_px: [i32; 2],
        flip_x: bool,
        flip_y: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            let group = p.refresh_selection_group(key, group)?;
            if group["compatibleGrids"] == false {
                return Err(invalid("selection contains incompatible layer grid sizes"));
            }
            let origin = pair(&group["origin"])?;
            let target = [
                origin[0] + i64::from(delta_px[0]),
                origin[1] + i64::from(delta_px[1]),
            ];
            delete(p, key, &group, false)?;
            apply(p, key, &group, target, [flip_x, flip_y], false, false).map(|(count, _)| count)
        })
    }
}
#[allow(clippy::too_many_arguments)]
fn move_entity_points(
    entity: &mut Value,
    source_offset: [i64; 2],
    dest_offset: [i64; 2],
    g: i64,
    origin: [i64; 2],
    size: [i64; 2],
    target: [i64; 2],
    flips: [bool; 2],
    bounds: [i64; 2],
) -> Result<()> {
    for field in entity["fieldInstances"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        let kind = field["__type"].as_str().unwrap_or("");
        if kind != "Point" && kind != "Array<Point>" {
            continue;
        }
        let values = if kind == "Array<Point>" {
            field["__value"].as_array().cloned().unwrap_or_default()
        } else {
            vec![field["__value"].clone()]
        };
        for (index, value) in values.into_iter().enumerate().rev() {
            let (Some(x), Some(y)) = (value["cx"].as_i64(), value["cy"].as_i64()) else {
                continue;
            };
            let px = transform(
                [source_offset[0] + x * g, source_offset[1] + y * g],
                origin,
                size,
                target,
                [g, g],
                flips,
            );
            let p = [(px[0] - dest_offset[0]) / g, (px[1] - dest_offset[1]) / g];
            put_point(
                field,
                index,
                (p[0] >= 0 && p[1] >= 0 && p[0] < bounds[0] && p[1] < bounds[1]).then_some(p),
                false,
            )?;
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn apply_deferred(
    p: &mut Project,
    key: LevelKey,
    group: &Value,
    target: [i64; 2],
    flips: [bool; 2],
    stack: bool,
    duplicate: bool,
    provided_remaps: Option<&BTreeMap<String, Value>>,
    move_originals: &BTreeSet<String>,
    finish: bool,
) -> Result<(usize, Value, Vec<String>)> {
    if group["type"] != "SelectionGroup" {
        return Err(invalid("not a selection group"));
    }
    if element_count(group) == 0 {
        let mut out = group.clone();
        out["origin"] = json!(target);
        out["levelIid"] = p.level(key).ok_or_else(|| invalid("level not loaded"))?["iid"].clone();
        out["elementCount"] = json!(0);
        return Ok((0, out, vec![]));
    }
    let origin = pair(&group["origin"])?;
    let size = pair(&group["size"])?;
    let mut material = group.clone();
    let old_ids = selected_ids(group);
    let mut remaps = provided_remaps.cloned().unwrap_or_default();
    if duplicate {
        for entry in material["layers"]
            .as_array_mut()
            .ok_or_else(|| invalid("selection layers missing"))?
        {
            for entity in entry["clipboard"]["entityInstances"]
                .as_array_mut()
                .into_iter()
                .flatten()
            {
                let old = entity["iid"]
                    .as_str()
                    .ok_or_else(|| invalid("entity IID missing"))?
                    .to_owned();
                if move_originals.contains(&old) {
                    continue;
                }
                let new = remaps.get(&old).cloned().unwrap_or_else(iid);
                remaps.insert(old, new.clone());
                entity["iid"] = new;
            }
        }
        for entry in material["layers"].as_array_mut().expect("validated layers") {
            for entity in entry["clipboard"]["entityInstances"].as_array_mut().into_iter().flatten() {
                if entity["iid"].as_str().is_some_and(|id| move_originals.contains(id)) {
                    continue;
                }
                for field in entity["fieldInstances"].as_array_mut().into_iter().flatten() {
                    remap_field_refs(field, &remaps, false);
                }
            }
        }
    }
    let defs = p.json["defs"]["entities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut count = 0;
    let mut out_of_bounds = vec![];
    for entry in material["layers"]
        .as_array()
        .ok_or_else(|| invalid("selection layers missing"))?
    {
        let index = layer_index(p, key, entry)?;
        let layer = p.layer(key, index).expect("selected layer");
        let g = grid(layer)?;
        if entry["clipboard"]["gridSize"] != g {
            return Err(invalid("destination layer grid differs from clipboard"));
        }
        let offset = [n(layer, "__pxTotalOffsetX"), n(layer, "__pxTotalOffsetY")];
        let source_origin = pair(&entry["origin"])?;
        let cell = pair(&entry["rect"])?;
        let source_offset = [
            source_origin[0] - cell[0] * g,
            source_origin[1] - cell[1] * g,
        ];
        let width = n(layer, "__cWid");
        let height = n(layer, "__cHei");
        let cw = n(&entry["clipboard"], "width");
        let kind = entry["clipboard"]["type"].as_str().unwrap_or("");
        if kind != layer["__type"].as_str().unwrap_or("") {
            return Err(invalid("destination layer type differs from clipboard"));
        }
        if group["emptySpace"] == true && (kind == "IntGrid" || kind == "Tiles") {
            let left = (target[0] - offset[0]) / g;
            let top = (target[1] - offset[1]) / g;
            let right = (target[0] + size[0] - 1 - offset[0]) / g;
            let bottom = (target[1] + size[1] - 1 - offset[1]) / g;
            if kind == "IntGrid" {
                p.edit_layer_presence(key, index, |layer, presence| {
                    let csv = layer["intGridCsv"]
                        .as_array_mut()
                        .ok_or_else(|| invalid("IntGrid missing"))?;
                    for y in top.max(0)..=bottom.min(height - 1) {
                        for x in left.max(0)..=right.min(width - 1) {
                            if let Some(v) = csv.get_mut((y * width + x) as usize) {
                                presence.write((y * width + x) as usize, v, json!(0));
                            }
                        }
                    }
                    Ok(())
                })?;
            } else if let Some(tiles) =
                p.layer_mut(key, index).expect("selected layer")["gridTiles"].as_array_mut()
            {
                tiles.retain(|t| {
                    let x = t["px"][0].as_i64().unwrap_or(0) / g;
                    let y = t["px"][1].as_i64().unwrap_or(0) / g;
                    x < left || y < top || x > right || y > bottom
                });
            }
        }
        match kind {
            "IntGrid" => {
                let mut cells = vec![];
                for mask in entry["cellMask"].as_array().into_iter().flatten() {
                    let i = mask
                        .as_i64()
                        .ok_or_else(|| invalid("selection cell index invalid"))?;
                    if cw <= 0 {
                        return Err(invalid("selection width invalid"));
                    }
                    let px = transform(
                        [source_origin[0] + i % cw * g, source_origin[1] + i / cw * g],
                        origin,
                        size,
                        target,
                        [g, g],
                        flips,
                    );
                    if (px[0] - offset[0]) % g != 0 || (px[1] - offset[1]) % g != 0 {
                        return Err(invalid(
                            "group destination does not align to all layer grids",
                        ));
                    }
                    let x = (px[0] - offset[0]) / g;
                    let y = (px[1] - offset[1]) / g;
                    let value = entry["clipboard"]["intGridCsv"]
                        .get(i as usize)
                        .cloned()
                        .ok_or_else(|| invalid("selection IntGrid data missing"))?;
                    if x >= 0 && y >= 0 && x < width && y < height {
                        cells.push(((y * width + x) as usize, value));
                    }
                }
                let palette = p
                    .layer_def(entry["layerDefUid"].as_i64().unwrap_or(-1))
                    .and_then(|d| d["intGridValues"].as_array())
                    .ok_or_else(|| invalid("IntGrid palette missing"))?;
                if cells
                    .iter()
                    .any(|(_, v)| *v != 0 && !palette.iter().any(|p| p["value"] == *v))
                {
                    return Err(invalid("clipboard contains undefined IntGrid values"));
                }
                p.edit_layer_presence(key, index, |layer, presence| {
                    let csv = layer["intGridCsv"]
                        .as_array_mut()
                        .ok_or_else(|| invalid("IntGrid missing"))?;
                    for (i, v) in cells {
                        let dst = csv
                            .get_mut(i)
                            .ok_or_else(|| invalid("IntGrid dimensions invalid"))?;
                        presence.write(i, dst, v);
                        count += 1;
                    }
                    Ok(())
                })?;
            }
            "Tiles" => {
                if entry["clipboard"]["tilesetUid"]
                    != p.layer(key, index).expect("selected layer")["__tilesetDefUid"]
                {
                    return Err(invalid("clipboard tileset differs from destination"));
                }
                let mut additions = vec![];
                let mut touched = BTreeSet::new();
                for tile in entry["clipboard"]["gridTiles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let px = pair(&tile["px"])?;
                    let dst = transform(
                        [source_origin[0] + px[0], source_origin[1] + px[1]],
                        origin,
                        size,
                        target,
                        [g, g],
                        flips,
                    );
                    let dst = [dst[0] - offset[0], dst[1] - offset[1]];
                    if dst[0] % g != 0 || dst[1] % g != 0 {
                        return Err(invalid(
                            "group destination does not align to all layer grids",
                        ));
                    }
                    if dst[0] < 0 || dst[1] < 0 || dst[0] >= width * g || dst[1] >= height * g {
                        continue;
                    }
                    let mut tile = tile.clone();
                    tile["px"] = json!(dst);
                    tile["d"] = json!([dst[1] / g * width + dst[0] / g]);
                    tile["f"] = json!(
                        tile["f"].as_i64().unwrap_or(0)
                            ^ i64::from(flips[0])
                            ^ (i64::from(flips[1]) << 1)
                    );
                    touched.insert((dst[0], dst[1]));
                    additions.push(tile);
                }
                let dst = p.layer_mut(key, index).expect("selected layer")["gridTiles"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("gridTiles missing"))?;
                if !stack {
                    dst.retain(|t| {
                        !touched.contains(&(
                            t["px"][0].as_i64().unwrap_or(0),
                            t["px"][1].as_i64().unwrap_or(0),
                        ))
                    });
                }
                for tile in additions {
                    if let Some(i) = dst.iter().position(|t| {
                        t["px"] == tile["px"] && t["t"] == tile["t"] && t["f"] == tile["f"]
                    }) {
                        dst.remove(i);
                    }
                    dst.push(tile);
                    count += 1;
                }
            }
            "Entities" => {
                let mut additions = vec![];
                let mut moved = BTreeMap::new();
                for entity in entry["clipboard"]["entityInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let mut e = entity.clone();
                    let px = pair(&e["px"])?;
                    let w = n(&e, "width");
                    let h = n(&e, "height");
                    let pivot = [
                        e["__pivot"][0].as_f64().unwrap_or(0.),
                        e["__pivot"][1].as_f64().unwrap_or(0.),
                    ];
                    let mut dst = transform(
                        [source_origin[0] + px[0], source_origin[1] + px[1]],
                        origin,
                        size,
                        target,
                        [0, 0],
                        flips,
                    );
                    if flips[0] {
                        dst[0] += ((2. * pivot[0] - 1.) * w as f64) as i64;
                    }
                    if flips[1] {
                        dst[1] += ((2. * pivot[1] - 1.) * h as f64) as i64;
                    }
                    let local = [dst[0] - offset[0], dst[1] - offset[1]];
                    let def = defs
                        .iter()
                        .find(|d| d["uid"] == e["defUid"])
                        .ok_or_else(|| invalid("clipboard entity definition missing"))?;
                    e["px"] = json!(local);
                    let in_bounds = def["allowOutOfBounds"] == true
                        || !crate::workflow::entity_out_of_layer_bounds(
                            p,
                            p.layer(key, index).expect("selected layer"),
                            &e,
                        );
                    if !in_bounds {
                        out_of_bounds.extend(def["identifier"].as_str().map(str::to_owned));
                    }
                    let id = entity["iid"].as_str().ok_or_else(|| invalid("entity IID missing"))?;
                    if duplicate && in_bounds {
                        move_entity_points(
                            &mut e,
                            source_offset,
                            offset,
                            g,
                            origin,
                            size,
                            entry["pointTranslation"].as_array().map_or(target, |_| {
                                [
                                    origin[0] + entry["pointTranslation"][0].as_i64().unwrap_or(0),
                                    origin[1] + entry["pointTranslation"][1].as_i64().unwrap_or(0),
                                ]
                            }),
                            flips,
                            [width, height],
                        )?;
                    }
                    if !duplicate || move_originals.contains(id) {
                        moved.insert(id, in_bounds.then_some(e));
                    } else if in_bounds {
                        additions.push(e);
                    }
                }
                let entities = p.layer_mut(key, index).expect("selected layer")["entityInstances"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("entities missing"))?;
                if !moved.is_empty() {
                    // Instance order is the manual depth order. Apply moves in the original slots.
                    entities.retain_mut(|entity| {
                        match entity["iid"].as_str().and_then(|id| moved.remove(id)) {
                            Some(Some(updated)) => {
                                *entity = updated;
                                count += 1;
                                true
                            }
                            Some(None) => false,
                            None => true,
                        }
                    });
                }
                if duplicate && !additions.is_empty() {
                    crate::selection::validate_entity_clipboard(p, key, index, &additions)?;
                    count += additions.len();
                    p.layer_mut(key, index).expect("selected layer")["entityInstances"]
                        .as_array_mut()
                        .expect("validated entities")
                        .extend(additions);
                }
            }
            _ => return Err(invalid("unsupported selection layer type")),
        }
        let mut points = entry["points"].as_array().cloned().unwrap_or_default();
        points.sort_by_key(|p| std::cmp::Reverse(p["arrayIndex"].as_u64().unwrap_or(0)));
        for point in points {
            let owner = point["entityIid"]
                .as_str()
                .ok_or_else(|| invalid("point owner missing"))?;
            if duplicate && old_ids.contains(owner) {
                continue;
            }
            let value = &point["value"];
            let x = n(value, "cx");
            let y = n(value, "cy");
            let px = transform(
                [source_offset[0] + x * g, source_offset[1] + y * g],
                origin,
                size,
                target,
                [g, g],
                flips,
            );
            let pt = [(px[0] - offset[0]) / g, (px[1] - offset[1]) / g];
            let uid = &point["fieldDefUid"];
            let fields = match fields_mut(p, key, index, owner) {
                Ok(fields) => fields,
                Err(_) if old_ids.contains(owner) => continue,
                Err(e) => return Err(e),
            };
            let field = fields
                .iter_mut()
                .find(|f| f["defUid"] == *uid)
                .ok_or_else(|| invalid("selected point field missing"))?;
            let idx = point["arrayIndex"].as_u64().unwrap_or(0) as usize;
            let insert = duplicate && field["__type"] == "Array<Point>";
            let in_bounds = pt[0] >= 0 && pt[1] >= 0 && pt[0] < width && pt[1] < height;
            if !in_bounds {
                out_of_bounds.extend(field["__identifier"].as_str().map(str::to_owned));
            }
            put_point(field, idx, in_bounds.then_some(pt), insert)?;
            count += 1;
        }
    }
    if finish {
        p.refresh_auto_and_derived()?;
    }
    let result = selection_result(p, key, group, &material, target, flips, duplicate, &remaps)?;
    Ok((count, result, out_of_bounds))
}
fn element_count(group: &Value) -> usize {
    group["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(layer_element_count)
        .sum()
}
fn layer_element_count(entry: &Value) -> usize {
    let cells = entry["cellMask"].as_array().map_or(0, Vec::len);
    let tiles = entry["clipboard"]["gridTiles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| t["px"].to_string())
        .collect::<BTreeSet<_>>()
        .len();
    cells
        + tiles
        + entry["clipboard"]["entityInstances"]
            .as_array()
            .map_or(0, Vec::len)
        + entry["points"].as_array().map_or(0, Vec::len)
}
#[allow(clippy::too_many_arguments)]
fn selection_result(
    p: &Project,
    key: LevelKey,
    original: &Value,
    material: &Value,
    target: [i64; 2],
    flips: [bool; 2],
    duplicate: bool,
    remaps: &BTreeMap<String, Value>,
) -> Result<Value> {
    let origin = pair(&original["origin"])?;
    let size = pair(&original["size"])?;
    let selected = selected_ids(original);
    let mut out = material.clone();
    out["levelIid"] = p.level(key).ok_or_else(|| invalid("level not loaded"))?["iid"].clone();
    out["origin"] = json!(target);
    let source_entries = original["layers"]
        .as_array()
        .ok_or_else(|| invalid("selection layers missing"))?;
    for (entry, source) in out["layers"]
        .as_array_mut()
        .ok_or_else(|| invalid("selection layers missing"))?
        .iter_mut()
        .zip(source_entries)
    {
        let index = layer_index(p, key, entry)?;
        let layer = p.layer(key, index).expect("selected layer");
        let g = grid(layer)?;
        let offset = [n(layer, "__pxTotalOffsetX"), n(layer, "__pxTotalOffsetY")];
        let old_cell = pair(&source["rect"])?;
        let source_origin = pair(&source["origin"])?;
        let source_offset = [
            source_origin[0] - old_cell[0] * g,
            source_origin[1] - old_cell[1] * g,
        ];
        let cw = n(&entry["clipboard"], "width");
        let ch = n(&entry["clipboard"], "height");
        let next_origin = transform(source_origin, origin, size, target, [cw * g, ch * g], flips);
        let next_cell = [
            (next_origin[0] - offset[0]) / g,
            (next_origin[1] - offset[1]) / g,
        ];
        entry["origin"] = json!([offset[0] + next_cell[0] * g, offset[1] + next_cell[1] * g]);
        entry["rect"] = json!([next_cell[0], next_cell[1], cw, ch]);
        entry["clipboard"]["origin"] = json!(next_cell);
        entry["layerIndex"] = json!(index);
        if let Some(mask) = entry["cellMask"].as_array_mut() {
            for value in mask {
                let i = value
                    .as_i64()
                    .ok_or_else(|| invalid("selection cell index invalid"))?;
                let x = if flips[0] { cw - 1 - i % cw } else { i % cw };
                let y = if flips[1] { ch - 1 - i / cw } else { i / cw };
                *value = json!(y * cw + x);
            }
        }
        for tile in entry["clipboard"]["gridTiles"]
            .as_array_mut()
            .into_iter()
            .flatten()
        {
            let px = pair(&tile["px"])?;
            tile["px"] = json!([
                if flips[0] {
                    (cw - 1) * g - px[0]
                } else {
                    px[0]
                },
                if flips[1] {
                    (ch - 1) * g - px[1]
                } else {
                    px[1]
                }
            ]);
        }
        let point_target_origin = source["pointTranslation"].as_array().map_or(target, |_| {
            [
                origin[0] + source["pointTranslation"][0].as_i64().unwrap_or(0),
                origin[1] + source["pointTranslation"][1].as_i64().unwrap_or(0),
            ]
        });
        let point_target = |point: &Value| -> Option<[i64; 2]> {
            let v = &point["value"];
            let x = v["cx"].as_i64()?;
            let y = v["cy"].as_i64()?;
            let px = transform(
                [source_offset[0] + x * g, source_offset[1] + y * g],
                origin,
                size,
                point_target_origin,
                [g, g],
                flips,
            );
            let pt = [(px[0] - offset[0]) / g, (px[1] - offset[1]) / g];
            (pt[0] >= 0 && pt[1] >= 0 && pt[0] < n(layer, "__cWid") && pt[1] < n(layer, "__cHei"))
                .then_some(pt)
        };
        let mut points = vec![];
        let source_points = source["points"].as_array().cloned().unwrap_or_default();
        for point in &source_points {
            let Some(pt) = point_target(point) else {
                continue;
            };
            let owner = point["entityIid"]
                .as_str()
                .ok_or_else(|| invalid("point owner missing"))?;
            let field_uid = &point["fieldDefUid"];
            let old_index = point["arrayIndex"].as_u64().unwrap_or(0) as usize;
            let mut next_index = old_index;
            if duplicate && selected.contains(owner) {
                if let Some(field) = source["clipboard"]["entityInstances"]
                    .as_array()
                    .and_then(|es| es.iter().find(|e| e["iid"] == owner))
                    .and_then(|e| e["fieldInstances"].as_array())
                    .and_then(|fs| fs.iter().find(|f| f["defUid"] == *field_uid))
                    && field["__type"] == "Array<Point>"
                {
                    next_index = 0;
                    for value in field["__value"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(old_index)
                    {
                        let synthetic = json!({"value":value});
                        if point_target(&synthetic).is_some() {
                            next_index += 1;
                        }
                    }
                }
            } else {
                let lower = source_points.iter().filter(|other| {
                    other["entityIid"] == point["entityIid"]
                        && other["fieldDefUid"] == *field_uid
                        && other["arrayIndex"].as_u64().unwrap_or(0) < old_index as u64
                });
                let array = point["isArray"].as_bool().unwrap_or_else(|| {
                    layer["entityInstances"]
                        .as_array()
                        .and_then(|es| es.iter().find(|e| e["iid"] == owner))
                        .and_then(|e| e["fieldInstances"].as_array())
                        .and_then(|fs| fs.iter().find(|f| f["defUid"] == *field_uid))
                        .is_some_and(|f| f["__type"] == "Array<Point>")
                });
                if array {
                    let changes = lower
                        .filter(|other| point_target(other).is_some() == duplicate)
                        .count();
                    next_index = if duplicate {
                        old_index + changes
                    } else {
                        old_index.saturating_sub(changes)
                    };
                }
            }
            let mut point = point.clone();
            if let Some(new) = remaps.get(owner) {
                point["entityIid"] = new.clone();
            }
            point["arrayIndex"] = json!(next_index);
            point["value"] = json!({"cx":pt[0],"cy":pt[1]});
            points.push(point);
        }
        entry["points"] = json!(points);
    }
    let mut out = p.refresh_selection_group(key, &out)?;
    out["elementCount"] = json!(element_count(&out));
    Ok(out)
}
impl Project {
    pub fn move_selection_group_with_result(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta_px: [i32; 2],
        flip_x: bool,
        flip_y: bool,
    ) -> Result<Value> {
        self.edit_document(|p| {
            let group = p.refresh_selection_group(key, group)?;
            if group["compatibleGrids"] == false {
                return Err(invalid("selection contains incompatible layer grid sizes"));
            }
            let origin = pair(&group["origin"])?;
            let target = [
                origin[0] + i64::from(delta_px[0]),
                origin[1] + i64::from(delta_px[1]),
            ];
            delete(p, key, &group, false)?;
            apply(p, key, &group, target, [flip_x, flip_y], false, false).map(|(_, group)| group)
        })
    }
    pub fn paste_selection_group_with_result(
        &mut self,
        key: LevelKey,
        group: &Value,
        origin_px: [i32; 2],
        flip_x: bool,
        flip_y: bool,
        stack: bool,
    ) -> Result<Value> {
        self.edit_document(|p| {
            apply(
                p,
                key,
                group,
                origin_px.map(i64::from),
                [flip_x, flip_y],
                stack,
                true,
            )
            .map(|(_, group)| group)
        })
    }
}
fn update_group_stats(group: &mut Value) {
    let mut grids = vec![];
    let mut total = 0;
    for entry in group["layers"].as_array().into_iter().flatten() {
        let count = layer_element_count(entry);
        total += count;
        if count > 0
            && let Some(grid) = entry["clipboard"]["gridSize"].as_i64().filter(|g| *g > 0)
        {
            grids.push(grid);
        }
    }
    let largest = grids.iter().copied().max().unwrap_or(1);
    group["gridSize"] = json!(largest);
    group["compatibleGrids"] = json!(grids.iter().all(|g| largest % g == 0));
    group["elementCount"] = total.into();
}
fn apply(
    p: &mut Project,
    key: LevelKey,
    group: &Value,
    target: [i64; 2],
    flips: [bool; 2],
    stack: bool,
    duplicate: bool,
) -> Result<(usize, Value)> {
    apply_deferred(p, key, group, target, flips, stack, duplicate, None, &BTreeSet::new(), true)
        .map(|(count, group, _)| (count, group))
}

fn geometry(entry: &Value) -> Result<([f64; 2], f64)> {
    let v = &entry["geometry"];
    let origin = [
        v["origin"][0]
            .as_f64()
            .ok_or_else(|| invalid("layer geometry origin missing"))?,
        v["origin"][1]
            .as_f64()
            .ok_or_else(|| invalid("layer geometry origin missing"))?,
    ];
    let scale = v["scale"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or_else(|| invalid("invalid layer geometry scale"))?;
    if origin.iter().any(|v| !v.is_finite()) {
        return Err(invalid("invalid layer geometry origin"));
    }
    Ok((origin, scale))
}
fn geometry_stats(group: &mut Value) -> Result<()> {
    let mut smart: Option<(f64, i64)> = None;
    let mut grids = vec![];
    let mut total = 0;
    for entry in group["layers"]
        .as_array_mut()
        .ok_or_else(|| invalid("selection layers missing"))?
    {
        let (origin, scale) = geometry(entry)?;
        let raw = n(&entry["clipboard"], "gridSize");
        let cell = pair(&entry["rect"])?;
        entry["worldOrigin"] = json!([
            origin[0] + cell[0] as f64 * raw as f64 * scale,
            origin[1] + cell[1] as f64 * raw as f64 * scale
        ]);
        let count = layer_element_count(entry);
        total += count;
        if count > 0 {
            let scaled = raw as f64 * scale;
            grids.push(raw);
            if smart.is_none_or(|previous| scaled > previous.0) {
                smart = Some((scaled, raw));
            }
        }
    }
    let smart = smart.unwrap_or((1., 1));
    group["geometryAware"] = json!(true);
    group["scaledGridSize"] = json!(smart.0);
    group["gridSize"] = json!(smart.1);
    group["compatibleGrids"] = json!(grids.iter().all(|g| *g >= smart.1 || smart.1 % g == 0));
    group["elementCount"] = total.into();
    Ok(())
}

fn selection_bounds(a: [i64; 4], b: [i64; 4]) -> [i64; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    [
        x,
        y,
        (a[0] + a[2]).max(b[0] + b[2]) - x,
        (a[1] + a[3]).max(b[1] + b[3]) - y,
    ]
}

fn merge_selection_entries(first: &mut Value, second: Value) -> Result<()> {
    let rect = |entry: &Value| {
        let [x, y] = pair(&entry["rect"])?;
        Ok::<_, Error>([
            x,
            y,
            n(&entry["clipboard"], "width"),
            n(&entry["clipboard"], "height"),
        ])
    };
    let [x, y, width, height] = selection_bounds(rect(first)?, rect(&second)?);
    let [old_x, old_y] = pair(&first["rect"])?;
    let [ox, oy] = pair(&first["origin"])?;
    let grid = n(&first["clipboard"], "gridSize");
    let mut clipboard = first["clipboard"].clone();
    clipboard["origin"] = json!([x, y]);
    clipboard["width"] = json!(width);
    clipboard["height"] = json!(height);
    let mut cells = BTreeMap::new();
    let mut tiles = Vec::new();
    let mut entities = Vec::new();
    let mut points = Vec::new();
    let mut tile_cells = BTreeSet::new();
    let mut entity_ids = BTreeSet::new();
    let mut point_ids = BTreeSet::new();
    for entry in [&*first, &second] {
        let [left, top, old_width, _] = rect(entry)?;
        let [dx, dy] = [left - x, top - y];
        let clip = &entry["clipboard"];
        for cell in entry["cellMask"].as_array().into_iter().flatten() {
            let i = cell
                .as_i64()
                .ok_or_else(|| invalid("selection cell index invalid"))?;
            let index = (i / old_width + dy) * width + i % old_width + dx;
            cells.insert(index as usize, clip["intGridCsv"][i as usize].clone());
        }
        let mut entry_tiles = BTreeSet::new();
        for tile in clip["gridTiles"].as_array().into_iter().flatten() {
            let [px, py] = pair(&tile["px"])?;
            let absolute = [px + dx * grid, py + dy * grid];
            if !tile_cells.contains(&absolute) {
                entry_tiles.insert(absolute);
                let mut tile = tile.clone();
                tile["px"] = json!(absolute);
                tiles.push(tile);
            }
        }
        tile_cells.extend(entry_tiles);
        for entity in clip["entityInstances"].as_array().into_iter().flatten() {
            if entity_ids.insert(entity["iid"].as_str().unwrap_or_default()) {
                let [px, py] = pair(&entity["px"])?;
                let mut entity = entity.clone();
                entity["px"] = json!([px + dx * grid, py + dy * grid]);
                entities.push(entity);
            }
        }
        for point in entry["points"].as_array().into_iter().flatten() {
            if point_ids.insert((
                point["entityIid"].as_str().unwrap_or_default(),
                n(point, "fieldDefUid"),
                n(point, "arrayIndex"),
            )) {
                points.push(point.clone());
            }
        }
    }
    if clipboard["type"] == "IntGrid" {
        let area = width
            .checked_mul(height)
            .filter(|area| *area <= 16_777_216)
            .ok_or_else(|| invalid("clipboard allocation too large"))?;
        let mut csv = vec![json!(0); area as usize];
        for (&index, value) in &cells {
            csv[index] = value.clone();
        }
        clipboard["intGridCsv"] = json!(csv);
    }
    clipboard["gridTiles"] = json!(tiles);
    clipboard["entityInstances"] = json!(entities);
    first["clipboard"] = clipboard;
    first["origin"] = json!([ox + (x - old_x) * grid, oy + (y - old_y) * grid]);
    first["rect"] = json!([x, y, width, height]);
    first["cellMask"] = json!(cells.keys().collect::<Vec<_>>());
    first["points"] = json!(points);
    Ok(())
}

impl Project {
    /// Add freshly captured elements to a selection, keeping cells and object identities unique.
    pub fn append_selection_group(
        &self,
        key: LevelKey,
        previous: &Value,
        mut added: Value,
        geometries: &Value,
    ) -> Result<Value> {
        let mut group = self.refresh_selection_group_geometry(key, previous, geometries)?;
        let entries = group["layers"].as_array_mut().expect("group layers");
        let added_entries = added["layers"]
            .as_array_mut()
            .ok_or_else(|| invalid("selection layers missing"))?;
        for entry in std::mem::take(added_entries) {
            if entry["cellMask"].as_array().is_none_or(Vec::is_empty)
                && entry["points"].as_array().is_none_or(Vec::is_empty)
                && ["gridTiles", "entityInstances"].iter().all(|name| {
                    entry["clipboard"][*name]
                        .as_array()
                        .is_none_or(Vec::is_empty)
                })
            {
                continue;
            }
            if let Some(first) = entries
                .iter_mut()
                .find(|existing| existing["layerDefUid"] == entry["layerDefUid"])
            {
                merge_selection_entries(first, entry)?;
            } else {
                entries.push(entry);
            }
        }
        let rect = |group: &Value| {
            let [x, y] = pair(&group["origin"])?;
            let [w, h] = pair(&group["size"])?;
            Ok::<_, Error>([x, y, w, h])
        };
        let [x, y, w, h] = selection_bounds(rect(&group)?, rect(&added)?);
        group["origin"] = json!([x, y]);
        group["size"] = json!([w, h]);
        let mut rects = group["originalRects"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        rects.extend(
            added["originalRects"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        group["originalRects"] = json!(rects);
        group["emptySpace"] = json!(group["emptySpace"] == true || added["emptySpace"] == true);
        geometry_stats(&mut group)?;
        Ok(group)
    }

    /// Capture per-layer cell rectangles computed with the renderer's current camera transform.
    pub fn copy_selection_group_from_regions(
        &self,
        key: LevelKey,
        regions: &Value,
        bounds_px: [i32; 4],
    ) -> Result<Value> {
        let mut group = empty_group(self, key, bounds_px)?;
        let mut entries = vec![];
        let mut seen = BTreeSet::new();
        for region in regions
            .as_array()
            .ok_or_else(|| invalid("selection regions must be an array"))?
        {
            let index = region["layerIndex"]
                .as_u64()
                .ok_or_else(|| invalid("selection layer index missing"))?
                as usize;
            if !seen.insert(index) {
                continue;
            }
            let layer = self
                .layer(key, index)
                .ok_or_else(|| invalid("selection layer missing"))?;
            let g = grid(layer)?;
            let [x, y] = pair(&region["rect"])?;
            let w = region["rect"][2]
                .as_i64()
                .ok_or_else(|| invalid("selection width missing"))?;
            let h = region["rect"][3]
                .as_i64()
                .ok_or_else(|| invalid("selection height missing"))?;
            let rect = [
                x * g + n(layer, "__pxTotalOffsetX"),
                y * g + n(layer, "__pxTotalOffsetY"),
                w * g,
                h * g,
            ];
            let mut r = [0i32; 4];
            for (dst, v) in r.iter_mut().zip(rect) {
                *dst = i32::try_from(v)
                    .map_err(|_| invalid("selection rectangle exceeds integer range"))?;
            }
            crate::selection::rect_valid(r)?;
            if let Some(mut entry) = capture_layer(self, key, index, r, region["all"] == true)? {
                entry["geometry"] = region["geometry"].clone();
                geometry(&entry)?;
                entries.push(entry);
            }
        }
        group["layers"] = Value::Array(entries);
        geometry_stats(&mut group)?;
        let level = self.level(key).ok_or_else(|| invalid("level not loaded"))?;
        group["originalRects"] = if group["elementCount"] == 0 {
            json!([])
        } else {
            json!([{
                "leftPx": bounds_px[0].max(0),
                "rightPx": (i64::from(bounds_px[0]) + i64::from(bounds_px[2]) - 1).min(n(level, "pxWid")),
                "topPx": bounds_px[1].max(0),
                "bottomPx": (i64::from(bounds_px[1]) + i64::from(bounds_px[3]) - 1).min(n(level, "pxHei"))
            }])
        };
        Ok(group)
    }
    pub fn refresh_selection_group_geometry(
        &self,
        key: LevelKey,
        group: &Value,
        geometries: &Value,
    ) -> Result<Value> {
        let mut group = self.refresh_selection_group(key, group)?;
        for entry in group["layers"].as_array_mut().expect("group layers") {
            let geo = geometries
                .as_array()
                .and_then(|gs| gs.iter().find(|g| g["layerDefUid"] == entry["layerDefUid"]))
                .ok_or_else(|| invalid("selected layer geometry missing"))?;
            entry["geometry"] = geo.clone();
        }
        geometry_stats(&mut group)?;
        Ok(group)
    }
    pub fn move_entity_selection_freely_with_geometry(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta_world: [f64; 2],
        geometries: &Value,
    ) -> Result<Value> {
        self.edit_document(|p| {
            let group = p.refresh_selection_group_geometry(key, group, geometries)?;
            if !selection_group_is_entity_only(&group) {
                return Err(invalid("free movement requires an entity-only selection"));
            }
            apply_geometry(
                p,
                key,
                &group,
                delta_world,
                geometries,
                [false, false],
                false,
                false,
                true,
                None,
                true,
            )
            .map(|(group, _)| group)
        })
    }

    /// Geometry records cover all destination layers; `visible: false` excludes
    /// a layer from empty-space arrival cleanup, independently of selectability.
    pub fn move_selection_group_with_geometry(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta_world: [f64; 2],
        geometries: &Value,
        flip_x: bool,
        flip_y: bool,
    ) -> Result<Value> {
        self.edit_document(|p| {
            let group = p.refresh_selection_group_geometry(key, group, geometries)?;
            apply_geometry(
                p,
                key,
                &group,
                delta_world,
                geometries,
                [flip_x, flip_y],
                false,
                false,
                false,
                None,
                true,
            )
            .map(|(group, _)| group)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paste_selection_group_with_geometry(
        &mut self,
        key: LevelKey,
        group: &Value,
        origin_world: [f64; 2],
        geometries: &Value,
        flip_x: bool,
        flip_y: bool,
        stack: bool,
    ) -> Result<Value> {
        self.edit_document(|p| {
            let group = paste_geometry(group, geometries)?;
            let delta = [
                origin_world[0] - group["origin"][0].as_f64().unwrap_or(0.),
                origin_world[1] - group["origin"][1].as_f64().unwrap_or(0.),
            ];
            apply_geometry(
                p,
                key,
                &group,
                delta,
                geometries,
                [flip_x, flip_y],
                stack,
                true,
                false,
                None,
                true,
            )
            .map(|(group, _)| group)
        })
    }

    /// Apply a tool drag together with immediate rule updates. The caller settles
    /// the delayed generated layers before publishing the edit's history entry.
    #[allow(clippy::too_many_arguments)]
    pub fn drag_selection_group_with_geometry(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta: [f64; 2],
        geometries: &Value,
        copy: bool,
        free_entities: bool,
        pending: Option<&mut DeferredAutoLayers>,
    ) -> Result<Value> {
        self.drag_selection_group_with_report(key, group, delta, geometries, copy, free_entities, pending)
            .map(|(group, _)| group)
    }

    /// Apply a drag and return the selection and its user-facing notices.
    #[allow(clippy::too_many_arguments)]
    pub fn drag_selection_group_with_report(
        &mut self,
        key: LevelKey,
        group: &Value,
        delta: [f64; 2],
        geometries: &Value,
        copy: bool,
        free_entities: bool,
        pending: Option<&mut DeferredAutoLayers>,
    ) -> Result<(Value, SelectionDragReport)> {
        let mut candidate = self.clone();
        let group = if copy {
            paste_geometry(group, geometries)?
        } else {
            candidate.refresh_selection_group_geometry(key, group, geometries)?
        };
        let free_entities = free_entities && !copy;
        if free_entities && !selection_group_is_entity_only(&group) {
            return Err(invalid("free movement requires an entity-only selection"));
        }
        let mut report = SelectionDragReport::default();
        let (next, removed) = apply_geometry(
            &mut candidate,
            key,
            &group,
            delta,
            geometries,
            [false; 2],
            copy,
            copy,
            free_entities,
            copy.then_some(&mut report.copy_limits),
            pending.is_none(),
        )?;
        report.out_of_bounds = removed;
        if let Some(pending) = pending {
            let old = self
                .level(key)
                .and_then(|level| level["layerInstances"].as_array())
                .ok_or_else(|| invalid("selection layers missing"))?;
            let new = candidate
                .level(key)
                .and_then(|level| level["layerInstances"].as_array())
                .ok_or_else(|| invalid("selection layers missing"))?;
            let changed = old
                .iter()
                .zip(new)
                .filter_map(|(before, after)| {
                    let uid = after["layerDefUid"].as_i64()?;
                    (before["intGridCsv"] != after["intGridCsv"]
                        || before["gridTiles"] != after["gridTiles"])
                        .then_some(uid)
                })
                .collect();
            candidate.update_auto_layers_after_selection(key, &changed, pending)?;
        }
        *self = candidate;
        Ok((next, report))
    }
}

fn drag_copy_limits(
    p: &Project,
    key: LevelKey,
    group: &Value,
    delta: [f64; 2],
    warnings: &mut Vec<(i64, String)>,
) -> Result<BTreeSet<String>> {
    let mut move_originals = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for entry in group["layers"].as_array().into_iter().flatten() {
        let Some(entities) = entry["clipboard"]["entityInstances"].as_array() else {
            continue;
        };
        if entities.is_empty() {
            continue;
        }
        let index = layer_index(p, key, entry)?;
        let layer = p.layer(key, index).expect("selected layer");
        let origin = pair(&entry["origin"])?;
        let shift = [
            origin[0] - n(layer, "__pxTotalOffsetX") + delta[0].trunc() as i64,
            origin[1] - n(layer, "__pxTotalOffsetY") + delta[1].trunc() as i64,
        ];
        for entity in entities {
            let uid = n(entity, "defUid");
            let def = p.definition("entities", uid)
                .ok_or_else(|| invalid("clipboard entity definition missing"))?;
            let maximum = n(def, "maxCount");
            if maximum <= 0 {
                continue;
            }
            let scope = def["limitScope"].as_str().unwrap_or("PerLevel");
            let count = match counts.entry((uid, (scope == "PerLayer").then_some(index))) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                    crate::selection::entity_count_in_limit_scope(p, key, index, uid, scope)?,
                ),
            };
            let [x, y] = pair(&entity["px"])?;
            let mut destination = entity.clone();
            destination["px"] = json!([x + shift[0], y + shift[1]]);
            let in_bounds = def["allowOutOfBounds"] == true
                || !crate::workflow::entity_out_of_layer_bounds(p, layer, &destination);
            if *count >= maximum as usize {
                let id = entity["iid"].as_str().ok_or_else(|| invalid("entity IID missing"))?;
                move_originals.insert(id.to_owned());
                warnings.push((maximum, def["identifier"].as_str().unwrap_or_default().to_owned()));
                if !in_bounds {
                    *count -= 1;
                }
            } else if in_bounds {
                *count += 1;
            }
        }
    }
    Ok(move_originals)
}

fn paste_geometry(group: &Value, geometries: &Value) -> Result<Value> {
    let mut group = group.clone();
    for entry in group["layers"]
        .as_array_mut()
        .ok_or_else(|| invalid("selection layers missing"))?
    {
        if let Some(geo) = geometries
            .as_array()
            .and_then(|gs| gs.iter().find(|g| g["layerDefUid"] == entry["layerDefUid"]))
        {
            entry["geometry"] = geo.clone();
        }
    }
    geometry_stats(&mut group)?;
    Ok(group)
}
/// Source originalRects are level-space pixels, deliberately converted with
/// raw layer offsets/grid (not the layer's parallax render geometry).
fn clear_arrival_rectangles(
    p: &mut Project,
    key: LevelKey,
    group: &Value,
    delta: [f64; 2],
    geometries: &Value,
) -> Result<Value> {
    let mut rects = match group.get("originalRects") {
        Some(value) => value
            .as_array()
            .cloned()
            .ok_or_else(|| invalid("selection rectangles missing"))?,
        None => {
            let origin = pair(&group["origin"])?;
            let size = pair(&group["size"])?;
            vec![
                json!({"leftPx":origin[0],"rightPx":origin[0] as f64+size[0] as f64-1.,"topPx":origin[1],"bottomPx":origin[1] as f64+size[1] as f64-1.}),
            ]
        }
    };
    for rect in &mut rects {
        for (name, axis) in [("leftPx", 0), ("rightPx", 0), ("topPx", 1), ("bottomPx", 1)] {
            let value = rect[name]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid("invalid selection rectangle"))?;
            rect[name] = json!(value + delta[axis]);
        }
    }
    for geo in geometries
        .as_array()
        .ok_or_else(|| invalid("layer geometries missing"))?
    {
        if geo["visible"] == false {
            continue;
        }
        let index = layer_index(p, key, geo)?;
        let layer = p.layer(key, index).expect("validated layer");
        if layer["visible"] == false
            || !matches!(layer["__type"].as_str(), Some("IntGrid" | "Tiles"))
        {
            continue;
        }
        let grid = grid(layer)? as f64;
        let offset = [
            n(layer, "__pxTotalOffsetX") as f64,
            n(layer, "__pxTotalOffsetY") as f64,
        ];
        let width = n(layer, "__cWid");
        let height = n(layer, "__cHei");
        let is_int_grid = layer["__type"] == "IntGrid";
        for rect in &rects {
            let left = ((rect["leftPx"].as_f64().unwrap() - offset[0]) / grid).trunc() as i64;
            let right =
                ((rect["rightPx"].as_f64().unwrap() + 1. - offset[0]) / grid).trunc() as i64;
            let top = ((rect["topPx"].as_f64().unwrap() - offset[1]) / grid).trunc() as i64;
            let bottom =
                ((rect["bottomPx"].as_f64().unwrap() + 1. - offset[1]) / grid).trunc() as i64;
            if is_int_grid {
                p.edit_layer_presence(key, index, |layer, presence| {
                    let csv = layer["intGridCsv"]
                        .as_array_mut()
                        .ok_or_else(|| invalid("IntGrid missing"))?;
                    for y in top.max(0)..bottom.min(height) {
                        for x in left.max(0)..right.min(width) {
                            if let Some(value) = csv.get_mut((y * width + x) as usize) {
                                presence.write((y * width + x) as usize, value, json!(0));
                            }
                        }
                    }
                    Ok(())
                })?;
            } else if let Some(tiles) =
                p.layer_mut(key, index).expect("validated layer")["gridTiles"].as_array_mut()
            {
                tiles.retain(|tile| {
                    let x = tile["px"][0].as_i64().unwrap_or(0) / grid as i64;
                    let y = tile["px"][1].as_i64().unwrap_or(0) / grid as i64;
                    x < left || x >= right || y < top || y >= bottom
                });
            }
        }
    }
    Ok(json!(rects))
}

#[allow(clippy::too_many_arguments)]
fn apply_geometry(
    p: &mut Project,
    key: LevelKey,
    group: &Value,
    delta: [f64; 2],
    geometries: &Value,
    flips: [bool; 2],
    stack: bool,
    duplicate: bool,
    free_entities: bool,
    drag_warnings: Option<&mut Vec<(i64, String)>>,
    rebuild_all_rules: bool,
) -> Result<(Value, Vec<String>)> {
    if delta
        .iter()
        .any(|v| !v.is_finite() || v.abs() > i32::MAX as f64)
    {
        return Err(invalid("invalid selection movement"));
    }
    if element_count(group) == 0 {
        return Ok((group.clone(), vec![]));
    }
    if !duplicate && !free_entities && group["compatibleGrids"] == false {
        return Err(invalid("selection contains incompatible layer grid sizes"));
    }
    let smart = group["scaledGridSize"]
        .as_f64()
        .filter(|g| *g > 0.)
        .ok_or_else(|| invalid("selection scaled grid missing"))?;
    let move_originals = match drag_warnings {
        Some(warnings) => drag_copy_limits(p, key, group, delta, warnings)?,
        None => BTreeSet::new(),
    };
    let ids = selected_ids(group);
    let remaps = if duplicate {
        ids.iter()
            .filter(|id| !move_originals.contains(*id))
            .map(|id| (id.clone(), json!(iid())))
            .collect()
    } else {
        BTreeMap::new()
    };
    if !duplicate {
        delete(p, key, group, false)?;
    }
    let mut out = group.clone();
    if group["emptySpace"] == true && !free_entities {
        out["originalRects"] = clear_arrival_rectangles(p, key, group, delta, geometries)?;
    }
    let mut entries = vec![];
    let mut out_of_bounds = vec![];
    for original in group["layers"]
        .as_array()
        .ok_or_else(|| invalid("selection layers missing"))?
    {
        let raw = n(&original["clipboard"], "gridSize");
        let (_, scale) = geometry(original)?;
        let scaled = raw as f64 * scale;
        let origin = pair(&original["origin"])?;
        let size = [
            n(&original["clipboard"], "width") * raw,
            n(&original["clipboard"], "height") * raw,
        ];
        let index = layer_index(p, key, original)?;
        let is_entity = p.layer(key, index).expect("selected layer")["__type"] == "Entities";
        let point_delta = delta.map(|d| (d / scaled).trunc() as i64 * raw);
        let raw_delta = if is_entity {
            delta.map(|d| d.trunc() as i64)
        } else {
            delta.map(|d| (d / smart).trunc() as i64 * (smart / scaled).trunc() as i64 * raw)
        };
        let mut material = original.clone();
        material["points"] = json!(if duplicate {
            original["points"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|pt| pt["entityIid"].as_str().is_some_and(|id| ids.contains(id)))
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        });
        material["pointTranslation"] = json!(point_delta);
        let one = json!({"type":"SelectionGroup","levelIid":group["levelIid"],"origin":origin,"size":size,"emptySpace":false,"layers":[material]});
        let (_, result, removed_entities) = apply_deferred(
            p,
            key,
            &one,
            [origin[0] + raw_delta[0], origin[1] + raw_delta[1]],
            flips,
            stack,
            duplicate,
            Some(&remaps),
            &move_originals,
            false,
        )?;
        let mut entry = result["layers"][0].clone();
        let mut point_entry = original.clone();
        point_entry["clipboard"]["entityInstances"] = json!([]);
        point_entry["clipboard"]["gridTiles"] = json!([]);
        point_entry["clipboard"]["intGridCsv"] = json!([]);
        point_entry["cellMask"] = json!([]);
        if duplicate {
            point_entry["points"]
                .as_array_mut()
                .expect("selection points")
                .retain(|pt| !pt["entityIid"].as_str().is_some_and(|id| ids.contains(id)));
        } else if !removed_entities.is_empty() {
            let retained = selected_ids(&result);
            point_entry["points"]
                .as_array_mut()
                .expect("selection points")
                .retain(|pt| {
                    !pt["entityIid"].as_str().is_some_and(|id| ids.contains(id) && !retained.contains(id))
                });
        }
        let points = json!({"type":"SelectionGroup","levelIid":group["levelIid"],"origin":origin,"size":size,"layers":[point_entry]});
        let (_, result, removed_points) = apply_deferred(
            p,
            key,
            &points,
            [origin[0] + point_delta[0], origin[1] + point_delta[1]],
            flips,
            stack,
            duplicate,
            Some(&remaps),
            &move_originals,
            false,
        )?;
        out_of_bounds.extend(removed_entities);
        out_of_bounds.extend(removed_points);
        entry["points"]
            .as_array_mut()
            .expect("selection points")
            .extend(
                result["layers"][0]["points"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        entry
            .as_object_mut()
            .expect("group entry")
            .remove("pointTranslation");
        entries.push(entry);
    }
    if rebuild_all_rules {
        p.refresh_auto_and_derived()?;
    } else {
        p.refresh_derived_data()?;
    }
    out["layers"] = json!(entries);
    out["origin"] = json!([
        group["origin"][0].as_f64().unwrap_or(0.) + delta[0],
        group["origin"][1].as_f64().unwrap_or(0.) + delta[1]
    ]);
    out = p.refresh_selection_group(key, &out)?;
    geometry_stats(&mut out)?;
    Ok((out, out_of_bounds))
}
