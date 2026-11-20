use crate::{DeferredAutoLayers, Error, LevelKey, Project, Result};
use serde_json::{Value, json};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn integer(value: &Value, name: &str) -> Result<i64> {
    value
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid(&format!("missing or invalid {name}")))
}
fn dimensions(layer: &Value) -> Result<(i32, i32, i32)> {
    let width =
        i32::try_from(integer(layer, "__cWid")?).map_err(|_| invalid("invalid layer width"))?;
    let height =
        i32::try_from(integer(layer, "__cHei")?).map_err(|_| invalid("invalid layer height"))?;
    let grid =
        i32::try_from(integer(layer, "__gridSize")?).map_err(|_| invalid("invalid grid size"))?;
    if width <= 0
        || height <= 0
        || grid <= 0
        || width.checked_mul(height).is_none()
        || width.checked_mul(grid).is_none()
        || height.checked_mul(grid).is_none()
    {
        return Err(invalid(
            "layer dimensions must be positive and fit within i32",
        ));
    }
    Ok((width, height, grid))
}
fn inside(x: i32, y: i32, width: i32, height: i32) -> bool {
    x >= 0 && y >= 0 && x < width && y < height
}

impl Project {
    pub fn remove_entity(&mut self, key: LevelKey, layer: usize, index: usize) -> Result<bool> {
        if self
            .layer(key, layer)
            .is_none_or(|l| l["__type"] != "Entities")
        {
            return Err(invalid("entity removal requires an Entities layer"));
        }
        self.remove_entity_with_references(key, layer, index)
    }

    pub fn paint_int_grid(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        value: i32,
    ) -> Result<bool> {
        Ok(self.paint_int_grid_cells(key, layer_index, &[([x, y], value)])? > 0)
    }
    /// Paint a stroke or rectangle with one auto-rule invalidation and no project clone.
    pub fn paint_int_grid_cells(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        cells: &[([i32; 2], i32)],
    ) -> Result<usize> {
        self.paint_int_grid_cells_with_deferred(key, layer_index, cells, None)
    }

    pub fn paint_int_grid_cells_with_deferred(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        cells: &[([i32; 2], i32)],
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<usize> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "IntGrid" {
            return Err(invalid("IntGrid painting requires an IntGrid layer"));
        }
        let (width, height, _) = dimensions(layer)?;
        let values = layer["intGridCsv"]
            .as_array()
            .ok_or_else(|| invalid("intGridCsv must be an array"))?;
        if values.len() != width as usize * height as usize {
            return Err(invalid(
                "IntGrid cell count does not match layer dimensions",
            ));
        }
        let uid = layer["layerDefUid"].as_i64();
        let definition = uid.and_then(|uid| self.layer_def(uid));
        let palette: std::collections::BTreeSet<i64> = definition
            .and_then(|d| d["intGridValues"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v["value"].as_i64())
            .collect();
        let mut touched = std::collections::BTreeMap::new();
        for &([x, y], value) in cells {
            if value < 0 {
                return Err(invalid("IntGrid values must be nonnegative"));
            }
            if !inside(x, y, width, height) {
                continue;
            }
            if value > 0 && !palette.contains(&i64::from(value)) {
                return Err(invalid(
                    "IntGrid replacement value is not defined in this layer's palette",
                ));
            }
            let index = (y * width + x) as usize;
            let old = values[index]
                .as_i64()
                .filter(|v| *v >= 0 && i32::try_from(*v).is_ok())
                .ok_or_else(|| invalid("invalid IntGrid cell value"))?;
            touched.insert(index, (old, value));
        }
        touched.retain(|_, (old, new)| *old != i64::from(*new));
        if touched.is_empty() {
            return Ok(0);
        }
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        for (&index, &(_, value)) in &touched {
            self.layer_mut(key, layer_index).expect("validated layer")["intGridCsv"][index] =
                json!(value);
            let x = index as i32 % width;
            let y = index as i32 / width;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let has_auto = self.json["defs"]["layers"].as_array().is_some_and(|defs| {
            defs.iter().any(|d| {
                (d["uid"].as_i64() == uid
                    || d["autoSourceLayerDefUid"].as_i64() == uid
                    || uid.is_some_and(|u| crate::autotile::blocks_auto_layer(d, u)))
                    && active_auto_rules(d)
            })
        });
        if has_auto
            && let Some(uid) = uid
            && let Err(error) = self.update_auto_layers_after_paint(
                key,
                uid,
                [min_x, min_y, max_x - min_x + 1, max_y - min_y + 1],
                deferred,
            )
        {
            for (&index, &(old, _)) in &touched {
                self.layer_mut(key, layer_index).expect("validated layer")["intGridCsv"][index] =
                    json!(old);
            }
            return Err(error);
        }
        let mut erased = touched
            .iter()
            .filter_map(|(&index, &(_, value))| (value == 0).then_some(index))
            .peekable();
        if erased.peek().is_some() {
            let iid = self
                .layer(key, layer_index)
                .and_then(|layer| layer["iid"].as_str())
                .map(str::to_owned);
            crate::grid_presence::update_presence(
                &mut self.grid_presence,
                iid.as_deref(),
                |presence| {
                    for index in erased {
                        presence.set(index, false);
                    }
                },
            );
        }
        Ok(touched.len())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_tile(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        tile_id: i32,
        src: [i32; 2],
        flips: i32,
        erase: bool,
    ) -> Result<bool> {
        self.paint_tile_stacked(key, layer_index, x, y, tile_id, src, flips, erase, false)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn paint_tile_stacked(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        tile_id: i32,
        src: [i32; 2],
        flips: i32,
        erase: bool,
        stack: bool,
    ) -> Result<bool> {
        self.paint_tile_stacked_with_deferred(
            key,
            layer_index,
            x,
            y,
            tile_id,
            src,
            flips,
            erase,
            stack,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_tile_stacked_with_deferred(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        tile_id: i32,
        src: [i32; 2],
        flips: i32,
        erase: bool,
        stack: bool,
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        let uid = layer["layerDefUid"].as_i64();
        let killer = uid.is_some()
            && self.json["defs"]["layers"].as_array().is_some_and(|ds| {
                ds.iter()
                    .any(|d| uid.is_some_and(|u| crate::autotile::blocks_auto_layer(d, u)))
            });
        let old = killer.then(|| layer["gridTiles"].clone());
        let changed =
            self.paint_tile_inner(key, layer_index, x, y, tile_id, src, flips, erase, stack)?;
        if changed
            && killer
            && let Some(uid) = uid
            && let Err(error) =
                self.update_auto_layers_after_paint(key, uid, [x, y, 1, 1], deferred)
        {
            self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"] =
                old.expect("saved killer tiles");
            return Err(error);
        }
        Ok(changed)
    }
    #[allow(clippy::too_many_arguments)]
    fn paint_tile_inner(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        x: i32,
        y: i32,
        tile_id: i32,
        src: [i32; 2],
        flips: i32,
        erase: bool,
        stack: bool,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Tiles" {
            return Err(invalid("manual tiles require a Tiles layer"));
        }
        let (width, height, grid) = dimensions(layer)?;
        if !inside(x, y, width, height) {
            return Ok(false);
        }
        if !erase && (tile_id < 0 || src.iter().any(|v| *v < 0) || !(0..=3).contains(&flips)) {
            return Err(invalid("invalid tile ID, source coordinates or flip flags"));
        }
        if !erase {
            let tileset_uid = layer
                .get("overrideTilesetUid")
                .and_then(Value::as_i64)
                .or_else(|| layer.get("__tilesetDefUid").and_then(Value::as_i64))
                .or_else(|| {
                    layer
                        .get("layerDefUid")
                        .and_then(Value::as_i64)
                        .and_then(|uid| self.layer_def(uid))
                        .and_then(|d| d.get("tilesetDefUid"))
                        .and_then(Value::as_i64)
                })
                .ok_or_else(|| invalid("Tiles layer has no assigned tileset"))?;
            let tileset = self
                .tileset(tileset_uid)
                .ok_or_else(|| invalid("tileset definition not found"))?;
            let tile_grid = integer(tileset, "tileGridSize")?;
            let atlas_width = integer(tileset, "pxWid")?;
            let atlas_height = integer(tileset, "pxHei")?;
            let padding = tileset.get("padding").and_then(Value::as_i64).unwrap_or(0);
            let spacing = tileset.get("spacing").and_then(Value::as_i64).unwrap_or(0);
            if tile_grid <= 0
                || padding < 0
                || spacing < 0
                || atlas_width < tile_grid
                || atlas_height < tile_grid
            {
                return Err(invalid("invalid atlas dimensions, padding or spacing"));
            }
            let stride = tile_grid
                .checked_add(spacing)
                .ok_or_else(|| invalid("atlas stride overflow"))?;
            let cells = |size: i64| {
                size.checked_sub(padding.checked_mul(2)?)
                    .and_then(|n| n.checked_add(stride - 1))
                    .map(|n| n / stride)
            };
            let cols = cells(atlas_width)
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid atlas width"))?;
            let rows = cells(atlas_height)
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("invalid atlas height"))?;
            let count = cols
                .checked_mul(rows)
                .ok_or_else(|| invalid("atlas tile count overflow"))?;
            let id = i64::from(tile_id);
            if id >= count
                || i64::from(src[0]) != padding + (id % cols) * stride
                || i64::from(src[1]) != padding + (id / cols) * stride
            {
                return Err(invalid(
                    "tile ID and source coordinates do not match the assigned atlas",
                ));
            }
        }
        let tiles = layer
            .get("gridTiles")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("gridTiles must be an array"))?;
        if tiles.iter().any(|tile| {
            !tile.is_object()
                || tile
                    .get("px")
                    .and_then(Value::as_array)
                    .is_none_or(|p| p.len() != 2 || p.iter().any(|v| v.as_i64().is_none()))
        }) {
            return Err(invalid("tile pixel coordinates must be integer pairs"));
        }
        let px = json!([x * grid, y * grid]);
        let indices: Vec<usize> = tiles
            .iter()
            .enumerate()
            .filter_map(|(i, tile)| (tile["px"] == px).then_some(i))
            .collect();
        if erase && indices.is_empty() {
            return Ok(false);
        }
        if stack && erase {
            self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
                .as_array_mut()
                .expect("validated tiles")
                .remove(*indices.last().expect("nonempty stack"));
            return Ok(true);
        }
        if !erase && indices.len() == 1 {
            let tile = &tiles[indices[0]];
            if tile["t"] == tile_id && tile["src"] == json!(src) && tile["f"] == flips {
                return Ok(false);
            }
        }
        if stack && !erase {
            let mut replacement = indices
                .iter()
                .find_map(|&i| {
                    let tile = &tiles[i];
                    (tile["t"] == tile_id && tile["f"] == flips).then(|| tile.clone())
                })
                .unwrap_or_else(|| json!({"a":1,"d":[y*width+x]}));
            replacement["px"] = px.clone();
            replacement["src"] = json!(src);
            replacement["t"] = json!(tile_id);
            replacement["f"] = json!(flips);
            let tiles = self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
                .as_array_mut()
                .expect("validated tiles");
            if let Some(i) = tiles
                .iter()
                .position(|t| t["px"] == px && t["t"] == tile_id && t["f"] == flips)
            {
                tiles.remove(i);
            }
            tiles.push(replacement);
            return Ok(true);
        }
        let mut replacement = indices
            .first()
            .map(|&i| tiles[i].clone())
            .unwrap_or_else(|| json!({"a":1,"d":[y*width+x]}));
        if !erase {
            replacement["px"] = px;
            replacement["src"] = json!(src);
            replacement["t"] = json!(tile_id);
            replacement["f"] = json!(flips);
        }
        let tiles = self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
            .as_array_mut()
            .expect("validated tiles");
        if let Some(&first) = indices.first() {
            for &index in indices.iter().rev() {
                tiles.remove(index);
            }
            if !erase {
                tiles.insert(first, replacement);
            }
        } else {
            tiles.push(replacement);
        }
        Ok(true)
    }

    pub fn add_entity(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        x: i32,
        y: i32,
    ) -> Result<bool> {
        self.edit_document(|p| p.add_entity_inner(key, layer_index, def_uid, x, y))
    }
    fn add_entity_inner(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        x: i32,
        y: i32,
    ) -> Result<bool> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Entities" {
            return Err(invalid("entity placement requires an Entities layer"));
        }
        let (_, _, grid) = dimensions(layer)?;
        layer
            .get("entityInstances")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("entityInstances must be an array"))?;
        let definition = self
            .definition("entities", def_uid)
            .ok_or_else(|| invalid("entity definition not found"))?
            .clone();
        let level = self.level(key).expect("validated level");
        let width = integer(level, "pxWid")?;
        let height = integer(level, "pxHei")?;
        if width <= 0 || height <= 0 {
            return Err(invalid("level dimensions must be positive"));
        }
        if x < 0 || y < 0 || i64::from(x) >= width || i64::from(y) >= height {
            return Ok(false);
        }
        if let Some(layer_def) = layer
            .get("layerDefUid")
            .and_then(Value::as_i64)
            .and_then(|uid| self.layer_def(uid))
        {
            let tags = definition.get("tags").and_then(Value::as_array);
            let intersects = |name| {
                layer_def
                    .get(name)
                    .and_then(Value::as_array)
                    .is_some_and(|required| {
                        required
                            .iter()
                            .any(|t| tags.is_some_and(|tags| tags.contains(t)))
                    })
            };
            let requires_tags = layer_def
                .get("requiredTags")
                .and_then(Value::as_array)
                .is_some_and(|tags| !tags.is_empty());
            if (requires_tags && !intersects("requiredTags")) || intersects("excludedTags") {
                return Ok(false);
            }
        }
        let max = definition
            .get("maxCount")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let scope = definition
            .get("limitScope")
            .and_then(Value::as_str)
            .unwrap_or("PerLevel");
        let mut matches = Vec::new();
        if max > 0 {
            if !["PerLayer", "PerLevel", "PerWorld"].contains(&scope) {
                return Err(invalid("unknown entity count limit scope"));
            }
            for candidate_key in self.level_keys() {
                if candidate_key.world != key.world || (scope != "PerWorld" && candidate_key != key)
                {
                    continue;
                }
                let layers = self
                    .level(candidate_key)
                    .and_then(|l| l.get("layerInstances"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("level layers not loaded"))?;
                for (li, candidate) in layers.iter().enumerate() {
                    if scope == "PerLayer" && li != layer_index {
                        continue;
                    }
                    if let Some(entities) =
                        candidate.get("entityInstances").and_then(Value::as_array)
                    {
                        for (ei, entity) in entities.iter().enumerate() {
                            if entity["defUid"] == def_uid {
                                matches.push((candidate_key, li, ei));
                            }
                        }
                    }
                }
            }
        }
        let mut entity = build_entity(self, &definition, def_uid)?;
        let limit_reached = max > 0 && matches.len() as i64 >= max;
        let behavior = definition
            .get("limitBehavior")
            .and_then(Value::as_str)
            .unwrap_or("PreventAdding");
        if limit_reached {
            match behavior {
                "PreventAdding" => return Ok(false),
                "MoveLastOne" => {
                    let &(old_key, li, ei) = matches.last().expect("positive max reached");
                    entity = self.layer(old_key, li).expect("matched layer")["entityInstances"][ei]
                        .clone();
                }
                "DiscardOldOnes" => {}
                _ => return Err(invalid("unknown entity limit behavior")),
            }
        }
        let pivot_x = definition
            .get("pivotX")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let pivot_y = definition
            .get("pivotY")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        entity["px"] = json!([x, y]);
        entity["__grid"] = json!([
            (x - i32::from(pivot_x == 1.0)) / grid,
            (y - i32::from(pivot_y == 1.0)) / grid
        ]);
        let world = key
            .world
            .and_then(|world| self.json.get("worlds")?.get(world))
            .unwrap_or(&self.json);
        let layout = world
            .get("worldLayout")
            .and_then(Value::as_str)
            .unwrap_or("Free");
        if layout == "Free" || layout == "GridVania" {
            entity["__worldX"] = json!(integer(level, "worldX").unwrap_or(0) + i64::from(x));
            entity["__worldY"] = json!(integer(level, "worldY").unwrap_or(0) + i64::from(y));
        }
        // Validate everything above before removing or inserting any existing instance.
        if limit_reached {
            let removals = if behavior == "MoveLastOne" {
                &matches[matches.len() - 1..]
            } else {
                &matches[..matches.len() - max as usize + 1]
            };
            for &(old_key, li, ei) in removals.iter().rev() {
                self.layer_mut(old_key, li).expect("matched layer")["entityInstances"]
                    .as_array_mut()
                    .expect("matched entity array")
                    .remove(ei);
            }
        }
        self.layer_mut(key, layer_index).expect("validated layer")["entityInstances"]
            .as_array_mut()
            .expect("validated entity array")
            .push(entity);
        self.refresh_derived_data()?;
        Ok(true)
    }
}

fn build_entity(project: &Project, definition: &Value, uid: i64) -> Result<Value> {
    let identifier = definition
        .get("identifier")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("entity identifier missing"))?;
    let width = integer(definition, "width")?;
    let height = integer(definition, "height")?;
    if width <= 0 || height <= 0 {
        return Err(invalid("entity dimensions must be positive"));
    }
    let fields = definition
        .get("fieldDefs")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("entity fieldDefs must be an array"))?;
    let fields: Vec<Value> = fields
        .iter()
        .map(|d| project.make_field_instance(d))
        .collect::<Result<_>>()?;
    Ok(json!({
        "__identifier":identifier,"__grid":[0,0],"__pivot":[definition.get("pivotX").cloned().unwrap_or(json!(0)),definition.get("pivotY").cloned().unwrap_or(json!(0))],
        "__tags":definition.get("tags").cloned().unwrap_or(json!([])),"__tile":definition.get("tileRect").cloned().unwrap_or(Value::Null),
        "__smartColor":definition.get("color").cloned().unwrap_or(json!("#FFFFFF")),
        "iid":uuid::Uuid::new_v4().to_string(),"width":width,"height":height,"defUid":uid,"px":[0,0],"fieldInstances":fields
    }))
}

fn active_auto_rules(definition: &Value) -> bool {
    definition
        .get("autoRuleGroups")
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                group.get("active").and_then(Value::as_bool) != Some(false)
                    && group
                        .get("rules")
                        .and_then(Value::as_array)
                        .is_some_and(|rules| {
                            rules.iter().any(|rule| {
                                rule.get("active").and_then(Value::as_bool) != Some(false)
                            })
                        })
            })
        })
}
