//! Rectangular, JSON-preserving region clipboard and stamps. Rectangles use layer cells.
use crate::workflow::{iid, invalid, remap_refs};
use crate::{LevelKey, Project, Result};
use serde_json::{Value, json};
use std::cell::{Cell, LazyCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, hash_map::Entry};

fn stacked_tile_key(tile: &Value) -> [&Value; 3] {
    [&tile["px"], &tile["t"], &tile["f"]]
}

pub(crate) fn append_stacked_tiles(dst: &mut Vec<Value>, additions: Vec<Value>) {
    if additions.len() <= 1 {
        if let [tile] = additions.as_slice()
            && let Some(index) = dst.iter().position(|old| stacked_tile_key(old) == stacked_tile_key(tile))
        {
            dst.remove(index);
        }
        dst.extend(additions);
        return;
    }

    // Each incoming tile removes the oldest identical tile. Consume the
    // removals against the existing tiles first, then the incoming tiles.
    let mut removals = HashMap::<_, Cell<(usize, bool)>>::new();
    for tile in &additions {
        removals.entry(stacked_tile_key(tile)).or_default().get_mut().0 += 1;
    }
    dst.retain(|tile| {
        let Some(count) = removals.get(&stacked_tile_key(tile)) else {
            return true;
        };
        let (remaining, _) = count.get();
        count.set((remaining.saturating_sub(1), true));
        remaining == 0
    });
    let keep = additions
        .iter()
        .map(|tile| {
            let count = removals
                .get(&stacked_tile_key(tile))
                .expect("indexed incoming tile");
            let (remaining, had_existing) = count.get();
            // A new stack has no tile for its first insertion to remove.
            let keep = remaining <= usize::from(!had_existing);
            if !keep {
                count.set((remaining - 1, had_existing));
            }
            keep
        })
        .collect::<Vec<_>>();
    dst.extend(additions.into_iter().zip(keep).filter_map(|(tile, keep)| keep.then_some(tile)));
}

fn dims(layer: &Value) -> Result<(i64, i64, i64)> {
    let w = layer["__cWid"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("invalid layer width"))?;
    let h = layer["__cHei"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("invalid layer height"))?;
    let g = layer["__gridSize"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("invalid layer grid"))?;
    Ok((w, h, g))
}
pub(super) fn rect_valid(rect: [i32; 4]) -> Result<()> {
    if rect[2] <= 0
        || rect[3] <= 0
        || rect[0].checked_add(rect[2]).is_none()
        || rect[1].checked_add(rect[3]).is_none()
    {
        Err(invalid("selection rectangle must have positive dimensions"))
    } else {
        Ok(())
    }
}
fn within(px: &Value, rect: [i32; 4], g: i64) -> bool {
    let x = px[0].as_i64().unwrap_or(i64::MIN);
    let y = px[1].as_i64().unwrap_or(i64::MIN);
    x >= i64::from(rect[0]) * g
        && y >= i64::from(rect[1]) * g
        && x < i64::from(rect[0] + rect[2]) * g
        && y < i64::from(rect[1] + rect[3]) * g
}
fn erase_region(p: &mut Project, key: LevelKey, layer: usize, rect: [i32; 4]) -> Result<()> {
    let instance = p
        .layer(key, layer)
        .ok_or_else(|| invalid("layer not found"))?;
    let (w, h, g) = dims(instance)?;
    match instance["__type"].as_str() {
        Some("IntGrid") => {
            p.edit_layer_presence(key, layer, |layer, presence| {
                let csv = layer["intGridCsv"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("IntGrid missing"))?;
                if csv.len()
                    != usize::try_from(w * h).map_err(|_| invalid("invalid grid dimensions"))?
                {
                    return Err(invalid("invalid IntGrid length"));
                }
                for y in i64::from(rect[1]).max(0)..i64::from(rect[1] + rect[3]).min(h) {
                    for x in i64::from(rect[0]).max(0)..i64::from(rect[0] + rect[2]).min(w) {
                        let index = (y * w + x) as usize;
                        presence.write(index, &mut csv[index], json!(0));
                    }
                }
                Ok(())
            })?;
        }
        Some("Tiles") => {
            let ts = p.layer_mut(key, layer).expect("validated layer")["gridTiles"]
                .as_array_mut()
                .ok_or_else(|| invalid("tiles missing"))?;
            ts.retain(|t| !within(&t["px"], rect, g));
        }
        Some("Entities") => {
            let es = p.layer_mut(key, layer).expect("validated layer")["entityInstances"]
                .as_array_mut()
                .ok_or_else(|| invalid("entities missing"))?;
            es.retain(|e| !within(&e["px"], rect, g));
        }
        _ => {
            return Err(invalid(
                "selection requires IntGrid, Tiles or Entities layer",
            ));
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn paste(
    p: &mut Project,
    key: LevelKey,
    index: usize,
    clip: &Value,
    x: i32,
    y: i32,
    flip_x: bool,
    flip_y: bool,
    stack: bool,
    duplicate: bool,
) -> Result<usize> {
    let layer = p
        .layer(key, index)
        .ok_or_else(|| invalid("layer not found"))?;
    let (w, h, g) = dims(layer)?;
    let cw = clip["width"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("clipboard width invalid"))?;
    let ch = clip["height"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("clipboard height invalid"))?;
    if layer["__type"] != clip["type"] || clip["gridSize"] != g {
        return Err(invalid("clipboard layer type or grid differs"));
    }
    let mut count = 0;
    match clip["type"].as_str() {
        Some("IntGrid") => {
            let src = clip["intGridCsv"]
                .as_array()
                .ok_or_else(|| invalid("clipboard IntGrid missing"))?;
            if cw.checked_mul(ch).is_none_or(|n| n as usize != src.len()) {
                return Err(invalid("clipboard IntGrid size mismatch"));
            }
            let palette = p
                .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
                .and_then(|d| d["intGridValues"].as_array())
                .ok_or_else(|| invalid("IntGrid palette missing"))?;
            if src.iter().any(|v| {
                v.as_i64()
                    .is_none_or(|n| n < 0 || n > 0 && !palette.iter().any(|d| d["value"] == *v))
            }) {
                return Err(invalid("clipboard IntGrid value is not defined"));
            }
            p.edit_layer_presence(key, index, |layer, presence| {
                let dst = layer["intGridCsv"]
                    .as_array_mut()
                    .ok_or_else(|| invalid("destination IntGrid missing"))?;
                if dst.len() != w as usize * h as usize {
                    return Err(invalid("destination IntGrid size mismatch"));
                }
                for cy in 0..ch {
                    for cx in 0..cw {
                        let dx = i64::from(x) + if flip_x { cw - 1 - cx } else { cx };
                        let dy = i64::from(y) + if flip_y { ch - 1 - cy } else { cy };
                        if dx >= 0 && dy >= 0 && dx < w && dy < h {
                            let index = (dy * w + dx) as usize;
                            presence.write(
                                index,
                                &mut dst[index],
                                src[(cy * cw + cx) as usize].clone(),
                            );
                            count += 1;
                        }
                    }
                }
                Ok(())
            })?;
        }
        Some("Tiles") => {
            if !clip["tilesetUid"].is_null() && clip["tilesetUid"] != layer["__tilesetDefUid"] {
                return Err(invalid("clipboard tileset differs"));
            }
            let mut additions = vec![];
            let mut touched = BTreeSet::new();
            for tile in clip["gridTiles"]
                .as_array()
                .ok_or_else(|| invalid("clipboard tiles missing"))?
            {
                let mut t = tile.clone();
                let px = t["px"][0]
                    .as_i64()
                    .ok_or_else(|| invalid("invalid tile x"))?;
                let py = t["px"][1]
                    .as_i64()
                    .ok_or_else(|| invalid("invalid tile y"))?;
                let dx = i64::from(x) * g + if flip_x { (cw - 1) * g - px } else { px };
                let dy = i64::from(y) * g + if flip_y { (ch - 1) * g - py } else { py };
                if dx < 0 || dy < 0 || dx >= w * g || dy >= h * g {
                    continue;
                }
                if t["t"].as_i64().is_none_or(|n| n < 0)
                    || t["src"].as_array().is_none_or(|a| {
                        a.len() != 2 || a.iter().any(|n| n.as_i64().is_none_or(|n| n < 0))
                    })
                {
                    return Err(invalid("invalid clipboard tile"));
                }
                let f = t["f"].as_i64().unwrap_or(0);
                if !(0..=3).contains(&f) {
                    return Err(invalid("invalid tile flip flags"));
                }
                t["px"] = json!([dx, dy]);
                t["f"] = json!(f ^ i64::from(flip_x) ^ (i64::from(flip_y) << 1));
                t["d"] = json!([dy / g * w + dx / g]);
                touched.insert((dx, dy));
                additions.push(t);
            }
            count = additions.len();
            let dst = p.layer_mut(key, index).expect("validated layer")["gridTiles"]
                .as_array_mut()
                .ok_or_else(|| invalid("destination tiles missing"))?;
            if !stack {
                dst.retain(|t| {
                    !touched.contains(&(
                        t["px"][0].as_i64().unwrap_or(-1),
                        t["px"][1].as_i64().unwrap_or(-1),
                    ))
                });
            }
            if stack {
                append_stacked_tiles(dst, additions);
            } else {
                dst.extend(additions);
            }
        }
        Some("Entities") => {
            let mut additions = clip["entityInstances"]
                .as_array()
                .cloned()
                .ok_or_else(|| invalid("clipboard entities missing"))?;
            if duplicate {
                validate_entity_clipboard(p, key, index, &additions)?;
            }
            let mut remaps = BTreeMap::new();
            for e in &mut additions {
                if p.definition("entities", e["defUid"].as_i64().unwrap_or(-1))
                    .is_none()
                {
                    return Err(invalid("clipboard entity definition missing"));
                }
                let old = e["iid"]
                    .as_str()
                    .ok_or_else(|| invalid("clipboard entity IID missing"))?
                    .to_owned();
                if duplicate {
                    let new = iid();
                    remaps.insert(old, new.clone());
                    e["iid"] = new;
                }
                let px = e["px"][0]
                    .as_i64()
                    .ok_or_else(|| invalid("invalid entity x"))?;
                let py = e["px"][1]
                    .as_i64()
                    .ok_or_else(|| invalid("invalid entity y"))?;
                let ew = e["width"].as_f64().unwrap_or(0.);
                let eh = e["height"].as_f64().unwrap_or(0.);
                let pivot_x = e["__pivot"][0].as_f64().unwrap_or(0.);
                let pivot_y = e["__pivot"][1].as_f64().unwrap_or(0.);
                let dx = i64::from(x) * g
                    + if flip_x {
                        cw * g - px + ((2. * pivot_x - 1.) * ew) as i64
                    } else {
                        px
                    };
                let dy = i64::from(y) * g
                    + if flip_y {
                        ch * g - py + ((2. * pivot_y - 1.) * eh) as i64
                    } else {
                        py
                    };
                e["px"] = json!([dx, dy]);
            }
            if duplicate {
                let mut temporary = json!({"layerInstances":[{"entityInstances":additions}]});
                crate::field_edit::offset_point_fields(
                    &mut temporary["layerInstances"][0],
                    i64::from(x) - clip["origin"][0].as_i64().unwrap_or(0),
                    i64::from(y) - clip["origin"][1].as_i64().unwrap_or(0),
                    w,
                    h,
                )?;
                remap_refs(&mut temporary, &remaps, false);
                additions = temporary["layerInstances"][0]["entityInstances"]
                    .as_array()
                    .expect("temporary entities")
                    .clone();
            }
            count = additions.len();
            p.layer_mut(key, index).expect("validated layer")["entityInstances"]
                .as_array_mut()
                .ok_or_else(|| invalid("destination entities missing"))?
                .extend(additions);
        }
        _ => return Err(invalid("unsupported clipboard type")),
    }
    p.rebuild_auto_layers(key)?;
    p.refresh_derived_data()?;
    Ok(count)
}
impl Project {
    pub fn copy_region(&self, key: LevelKey, index: usize, rect: [i32; 4]) -> Result<Value> {
        self.copy_region_contents(key, index, rect, true)
    }

    pub(crate) fn copy_region_contents(
        &self,
        key: LevelKey,
        index: usize,
        rect: [i32; 4],
        copy_entities: bool,
    ) -> Result<Value> {
        rect_valid(rect)?;
        let layer = self
            .layer(key, index)
            .ok_or_else(|| invalid("layer not found"))?;
        let (w, h, g) = dims(layer)?;
        if !["Tiles", "Entities", "IntGrid"].contains(&layer["__type"].as_str().unwrap_or("")) {
            return Err(invalid("selection requires a manual layer"));
        }
        let count = i64::from(rect[2]) * i64::from(rect[3]);
        if count > 16_777_216 {
            return Err(invalid("clipboard allocation too large"));
        }
        let mut clip = json!({"type":layer["__type"],"gridSize":g,"width":rect[2],"height":rect[3],"origin":[rect[0],rect[1]],"tilesetUid":layer["__tilesetDefUid"],"gridTiles":[],"intGridCsv":[],"entityInstances":[]});
        if layer["__type"] == "IntGrid" {
            let csv = layer["intGridCsv"]
                .as_array()
                .ok_or_else(|| invalid("IntGrid missing"))?;
            if csv.len() != w as usize * h as usize {
                return Err(invalid("IntGrid size mismatch"));
            }
            let mut cells = Vec::with_capacity(count as usize);
            for cy in 0..rect[3] {
                for cx in 0..rect[2] {
                    let x = i64::from(rect[0]) + i64::from(cx);
                    let y = i64::from(rect[1]) + i64::from(cy);
                    cells.push(if x >= 0 && y >= 0 && x < w && y < h {
                        csv[(y * w + x) as usize].clone()
                    } else {
                        json!(0)
                    });
                }
            }
            clip["intGridCsv"] = Value::Array(cells);
        } else {
            let name = if layer["__type"] == "Tiles" {
                "gridTiles"
            } else {
                "entityInstances"
            };
            let elems = layer[name]
                .as_array()
                .ok_or_else(|| invalid("layer elements missing"))?;
            if name == "entityInstances" && !copy_entities {
                return Ok(clip);
            }
            let mut selected = vec![];
            for e in elems {
                if within(&e["px"], rect, g) {
                    let mut e = e.clone();
                    e["px"] = json!([
                        e["px"][0].as_i64().unwrap_or(0) - i64::from(rect[0]) * g,
                        e["px"][1].as_i64().unwrap_or(0) - i64::from(rect[1]) * g
                    ]);
                    selected.push(e);
                }
            }
            clip[name] = Value::Array(selected);
        }
        Ok(clip)
    }
    pub fn cut_region(&mut self, key: LevelKey, index: usize, rect: [i32; 4]) -> Result<Value> {
        self.edit_document(|p| {
            let clip = p.copy_region(key, index, rect)?;
            erase_region(p, key, index, rect)?;
            p.rebuild_auto_layers(key)?;
            p.refresh_derived_data()?;
            Ok(clip)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paste_region(
        &mut self,
        key: LevelKey,
        index: usize,
        clip: &Value,
        x: i32,
        y: i32,
        flip_x: bool,
        flip_y: bool,
        stack: bool,
    ) -> Result<usize> {
        self.edit_document(|p| paste(p, key, index, clip, x, y, flip_x, flip_y, stack, true))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn move_region(
        &mut self,
        key: LevelKey,
        index: usize,
        rect: [i32; 4],
        dx: i32,
        dy: i32,
        flip_x: bool,
        flip_y: bool,
    ) -> Result<usize> {
        self.edit_document(|p| {
            let clip = p.copy_region(key, index, rect)?;
            let x = rect[0]
                .checked_add(dx)
                .ok_or_else(|| invalid("selection x overflow"))?;
            let y = rect[1]
                .checked_add(dy)
                .ok_or_else(|| invalid("selection y overflow"))?;
            erase_region(p, key, index, rect)?;
            paste(p, key, index, &clip, x, y, flip_x, flip_y, false, false)
        })
    }
}
pub(crate) fn validate_entity_clipboard(
    p: &Project,
    key: LevelKey,
    layer_index: usize,
    entities: &[Value],
) -> Result<()> {
    let layer = p
        .layer(key, layer_index)
        .ok_or_else(|| invalid("layer not found"))?;
    let ld = p.layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1));
    let mut checked = HashSet::new();
    let mut existing_counts = HashMap::new();
    let incoming_counts = LazyCell::new(|| {
        let mut counts = HashMap::<_, usize>::new();
        for uid in entities.iter().filter_map(|entity| entity["defUid"].as_i64()) {
            *counts.entry(uid).or_default() += 1;
        }
        counts
    });
    for e in entities {
        let uid = e["defUid"]
            .as_i64()
            .ok_or_else(|| invalid("clipboard entity definition UID missing"))?;
        if !checked.insert(uid) {
            continue;
        }
        let d = p
            .definition("entities", uid)
            .ok_or_else(|| invalid("clipboard entity definition missing"))?;
        if let Some(ld) = ld {
            let matches = |name| {
                ld[name].as_array().is_some_and(|tags| {
                    tags.iter()
                        .any(|tag| d["tags"].as_array().is_some_and(|ts| ts.contains(tag)))
                })
            };
            if (ld["requiredTags"]
                .as_array()
                .is_some_and(|ts| !ts.is_empty())
                && !matches("requiredTags"))
                || matches("excludedTags")
            {
                return Err(invalid(
                    "clipboard entity does not match destination layer tags",
                ));
            }
        }
        let max = d["maxCount"].as_i64().unwrap_or(0);
        if max <= 0 {
            continue;
        }
        let scope = match d["limitScope"].as_str() {
            Some("PerWorld") => "PerWorld",
            Some("PerLayer") => "PerLayer",
            _ => "PerLevel",
        };
        let counts = match existing_counts.entry(scope) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let mut counts = HashMap::<_, usize>::new();
                visit_entities_in_limit_scope(p, key, layer_index, scope, |entity| {
                    if let Some(uid) = entity["defUid"].as_i64() {
                        *counts.entry(uid).or_default() += 1;
                    }
                })?;
                entry.insert(counts)
            }
        };
        let count = incoming_counts[&uid] + counts.get(&uid).copied().unwrap_or(0);
        if count > max as usize {
            return Err(invalid("clipboard duplication exceeds entity count limit"));
        }
    }
    Ok(())
}

pub(crate) fn entity_count_in_limit_scope(
    p: &Project,
    key: LevelKey,
    layer_index: usize,
    uid: i64,
    scope: &str,
) -> Result<usize> {
    let mut count = 0;
    visit_entities_in_limit_scope(p, key, layer_index, scope, |entity| {
        count += usize::from(entity["defUid"] == uid);
    })?;
    Ok(count)
}

fn visit_entities_in_limit_scope(
    p: &Project,
    key: LevelKey,
    layer_index: usize,
    scope: &str,
    mut visit: impl FnMut(&Value),
) -> Result<()> {
    for candidate in p.iter_level_keys() {
        if candidate.world != key.world || (scope != "PerWorld" && candidate != key) {
            continue;
        }
        let level = p.level(candidate).ok_or_else(|| invalid("level not loaded"))?;
        for (index, layer) in level["layerInstances"].as_array().into_iter().flatten().enumerate() {
            if scope == "PerLayer" && index != layer_index {
                continue;
            }
            layer["entityInstances"].as_array().into_iter().flatten().for_each(&mut visit);
        }
    }
    Ok(())
}
