use crate::{DeferredAutoLayers, Error, LevelKey, Project, Result};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Distinguishes a rejected placement from a click where the brush cannot place an entity.
pub enum EntityPlacement {
    Placed,
    Ignored,
    LimitReached { maximum: i64, identifier: String },
}

pub struct TilePaint {
    pub cell: [i32; 2],
    pub id: i32,
    pub src: [i32; 2],
    pub stack: bool,
}

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

fn tile_pixel(tile: &Value) -> Result<[i64; 2]> {
    tile["px"]
        .as_array()
        .and_then(|coords| match coords.as_slice() {
            [x, y] => Some([x.as_i64()?, y.as_i64()?]),
            _ => None,
        })
        .ok_or_else(|| invalid("tile pixel coordinates must be integer pairs"))
}

fn validate_tile_source(
    [padding, stride, cols, count]: [i64; 4],
    id: i32,
    src: [i32; 2],
) -> Result<()> {
    let id = i64::from(id);
    if id >= count
        || i64::from(src[0]) != padding + (id % cols) * stride
        || i64::from(src[1]) != padding + (id / cols) * stride
    {
        return Err(invalid(
            "tile ID and source coordinates do not match the assigned atlas",
        ));
    }
    Ok(())
}

fn write_tile(tile: &mut Value, px: [i32; 2], id: i32, src: [i32; 2], flips: i32) {
    tile["px"] = json!(px);
    tile["src"] = json!(src);
    tile["t"] = json!(id);
    tile["f"] = json!(flips);
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
        let palette = definition
            .and_then(|d| d["intGridValues"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v["value"].as_i64());
        let batch_palette =
            std::cell::LazyCell::new(|| palette.clone().collect::<std::collections::HashSet<_>>());
        let is_defined = |value| {
            if cells.len() == 1 {
                palette.clone().any(|id| id == value)
            } else {
                batch_palette.contains(&value)
            }
        };
        let mut touched = Vec::new();
        for &([x, y], value) in cells {
            if value < 0 {
                return Err(invalid("IntGrid values must be nonnegative"));
            }
            if !inside(x, y, width, height) {
                continue;
            }
            if value > 0 && !is_defined(i64::from(value)) {
                return Err(invalid(
                    "IntGrid replacement value is not defined in this layer's palette",
                ));
            }
            let index = (y * width + x) as usize;
            let old = values[index]
                .as_i64()
                .filter(|v| *v >= 0 && i32::try_from(*v).is_ok())
                .ok_or_else(|| invalid("invalid IntGrid cell value"))?;
            touched.push((index, old, value));
        }
        // A stable sort keeps repeated coordinates in brush order. The final write wins.
        touched.sort_by_key(|&(index, _, _)| index);
        touched.dedup_by(|later, earlier| {
            if later.0 != earlier.0 {
                return false;
            }
            earlier.2 = later.2;
            true
        });
        touched.retain(|&(_, old, new)| old != i64::from(new));
        if touched.is_empty() {
            return Ok(0);
        }
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        let values = self.layer_mut(key, layer_index).expect("validated layer")["intGridCsv"]
            .as_array_mut()
            .expect("validated cells");
        for &(index, _, value) in &touched {
            values[index] = value.into();
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
            let values = self.layer_mut(key, layer_index).expect("validated layer")["intGridCsv"]
                .as_array_mut()
                .expect("validated cells");
            for &(index, old, _) in &touched {
                values[index] = old.into();
            }
            return Err(error);
        }
        let mut erased = touched
            .iter()
            .filter_map(|&(index, _, value)| (value == 0).then_some(index))
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
        let killer = uid.is_some_and(|uid| self.tile_layer_blocks_auto(uid));
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

    fn tile_layer_blocks_auto(&self, uid: i64) -> bool {
        self.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|def| crate::autotile::blocks_auto_layer(def, uid))
    }

    /// Remove every tile in the requested cells and return changed cells in input order.
    pub fn erase_tile_cells_with_deferred(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        cells: &[[i32; 2]],
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<Vec<[i32; 2]>> {
        self.erase_tile_cells_stacked_with_deferred(key, layer_index, cells, false, deferred)
    }

    /// Erase either the top tile or the entire stack in each cell.
    pub fn erase_tile_cells_stacked_with_deferred(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        cells: &[[i32; 2]],
        stack: bool,
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<Vec<[i32; 2]>> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Tiles" {
            return Err(invalid("manual tiles require a Tiles layer"));
        }
        let (width, height, grid) = dimensions(layer)?;
        let pixel = |cell: [i32; 2]| cell.map(|axis| i64::from(axis) * i64::from(grid));
        let mut targets: HashMap<_, _> = cells
            .iter()
            .copied()
            .filter(|&[x, y]| inside(x, y, width, height))
            .map(|cell| (pixel(cell), None))
            .collect();
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let tiles = layer["gridTiles"]
            .as_array()
            .ok_or_else(|| invalid("gridTiles must be an array"))?;
        let mut removals = Vec::new();
        for (index, tile) in tiles.iter().enumerate() {
            if let Some(top) = targets.get_mut(&tile_pixel(tile)?) {
                *top = Some(index);
                if !stack {
                    removals.push(index);
                }
            }
        }
        if stack {
            removals.extend(targets.values().copied().flatten());
            removals.sort_unstable();
        }
        if removals.is_empty() {
            return Ok(Vec::new());
        }
        let changed: Vec<_> = cells
            .iter()
            .copied()
            .filter(|&cell| targets.remove(&pixel(cell)).flatten().is_some())
            .collect();
        let blocker = layer["layerDefUid"]
            .as_i64()
            .filter(|&uid| self.tile_layer_blocks_auto(uid))
            .map(|uid| (uid, tiles.clone()));
        remove_tile_indices(
            self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
                .as_array_mut()
                .expect("validated tiles"),
            &removals,
        );
        self.update_auto_after_tile_batch(
            key,
            layer_index,
            blocker,
            changed.iter().copied(),
            deferred,
        )?;
        Ok(changed)
    }

    /// Paint in input order, allowing repeated writes to cells in overwrite mode.
    /// Returns the input indices that changed.
    pub fn paint_tile_cells_with_deferred(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        edits: &[TilePaint],
        flips: i32,
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<Vec<usize>> {
        let layer = self
            .layer(key, layer_index)
            .ok_or_else(|| invalid("layer not found"))?;
        if layer["__type"] != "Tiles" {
            return Err(invalid("manual tiles require a Tiles layer"));
        }
        let (width, height, grid) = dimensions(layer)?;
        let mut atlas = None;
        let mut targets = HashMap::with_capacity(edits.len());
        let mut groups = vec![None; edits.len()];
        for (
            index,
            &TilePaint {
                cell: [x, y],
                id,
                src,
                stack,
            },
        ) in edits.iter().enumerate()
        {
            if !inside(x, y, width, height) {
                continue;
            }
            if id < 0 || src.iter().any(|v| *v < 0) || !(0..=3).contains(&flips) {
                return Err(invalid("invalid tile ID, source coordinates or flip flags"));
            }
            let layout = match atlas {
                Some(layout) => layout,
                None => *atlas.insert(self.tile_atlas_layout(layer)?),
            };
            validate_tile_source(layout, id, src)?;
            let pixel = [i64::from(x * grid), i64::from(y * grid)];
            let group = *targets.entry(pixel).or_insert(index);
            if group != index && (stack || edits[group].stack) {
                return Err(invalid("stacked tile painting requires distinct cells"));
            }
            groups[index] = Some(group);
        }
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let tiles = layer["gridTiles"]
            .as_array()
            .ok_or_else(|| invalid("gridTiles must be an array"))?;
        let mut first = vec![None; edits.len()];
        let mut stacked = vec![false; edits.len()];
        let mut raise = vec![None; edits.len()];
        let mut removals = Vec::new();
        for (index, tile) in tiles.iter().enumerate() {
            if let Some(&edit) = targets.get(&tile_pixel(tile)?) {
                if first[edit].is_none() {
                    first[edit] = Some(index);
                } else {
                    stacked[edit] = true;
                    if !edits[edit].stack {
                        removals.push(index);
                    }
                }
                if edits[edit].stack
                    && raise[edit].is_none()
                    && tile["t"] == edits[edit].id
                    && tile["f"] == flips
                {
                    raise[edit] = Some(index);
                }
            }
        }
        let mut changed = Vec::new();
        let mut write_order = Vec::new();
        let mut latest: Vec<Option<usize>> = vec![None; edits.len()];
        for (index, (&TilePaint { id, src, .. }, group)) in edits.iter().zip(groups).enumerate() {
            let Some(group) = group else { continue };
            let unchanged = if let Some(previous) = latest[group] {
                edits[previous].id == id && edits[previous].src == src
            } else {
                !stacked[group]
                    && first[group].is_some_and(|first| {
                        let tile = &tiles[first];
                        tile["t"] == id && tile["src"] == json!(src) && tile["f"] == flips
                    })
            };
            if !unchanged {
                changed.push(index);
                if latest[group].replace(index).is_none() {
                    write_order.push(group);
                }
            }
        }
        if changed.is_empty() {
            return Ok(changed);
        }
        let blocker = layer["layerDefUid"]
            .as_i64()
            .filter(|&uid| self.tile_layer_blocks_auto(uid))
            .map(|uid| (uid, tiles.clone()));
        let tiles = self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
            .as_array_mut()
            .expect("validated tiles");
        // A new cell is appended on its first effective write. Later overwrites
        // only change its contents, so the last write can reuse that position.
        for group in write_order {
            let TilePaint {
                cell: [x, y],
                id,
                src,
                stack,
            } = edits[latest[group].expect("changed tile group")];
            let tile = if !stack && let Some(first) = first[group] {
                &mut tiles[first]
            } else {
                let tile = if let Some(raise) = raise[group] {
                    removals.push(raise);
                    std::mem::take(&mut tiles[raise])
                } else {
                    json!({"a":1,"d":[y*width+x]})
                };
                tiles.push(tile);
                tiles.last_mut().expect("just pushed")
            };
            write_tile(tile, [x * grid, y * grid], id, src, flips);
        }
        removals.sort_unstable();
        remove_tile_indices(tiles, &removals);
        self.update_auto_after_tile_batch(
            key,
            layer_index,
            blocker,
            changed.iter().map(|&index| edits[index].cell),
            deferred,
        )?;
        Ok(changed)
    }

    fn update_auto_after_tile_batch(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        blocker: Option<(i64, Vec<Value>)>,
        mut cells: impl Iterator<Item = [i32; 2]>,
        deferred: Option<&mut DeferredAutoLayers>,
    ) -> Result<()> {
        if let Some((uid, old)) = blocker {
            let [x, y] = cells.next().expect("changed tiles");
            let [left, top, right, bottom] =
                cells.fold([x, y, x, y], |[left, top, right, bottom], [x, y]| {
                    [left.min(x), top.min(y), right.max(x), bottom.max(y)]
                });
            if let Err(error) = self.update_auto_layers_after_paint(
                key,
                uid,
                [left, top, right - left + 1, bottom - top + 1],
                deferred,
            ) {
                self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"] =
                    Value::Array(old);
                return Err(error);
            }
        }
        Ok(())
    }

    fn tile_atlas_layout(&self, layer: &Value) -> Result<[i64; 4]> {
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
        Ok([padding, stride, cols, count])
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
            validate_tile_source(self.tile_atlas_layout(layer)?, tile_id, src)?;
        }
        let tiles = layer
            .get("gridTiles")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("gridTiles must be an array"))?;
        let px = [x * grid, y * grid];
        let target = px.map(i64::from);
        let mut indices = Vec::new();
        for (index, tile) in tiles.iter().enumerate() {
            if tile_pixel(tile)? == target {
                indices.push(index);
            }
        }
        if erase && indices.is_empty() {
            return Ok(false);
        }
        if !erase && let [index] = indices.as_slice() {
            let tile = &tiles[*index];
            if tile["t"] == tile_id && tile["src"] == json!(src) && tile["f"] == flips {
                return Ok(false);
            }
        }
        let matching = if stack && !erase {
            indices
                .iter()
                .copied()
                .find(|&i| tiles[i]["t"] == tile_id && tiles[i]["f"] == flips)
        } else {
            None
        };
        let tiles = self.layer_mut(key, layer_index).expect("validated layer")["gridTiles"]
            .as_array_mut()
            .expect("validated tiles");
        if erase {
            if stack {
                tiles.remove(*indices.last().expect("nonempty stack"));
            } else {
                remove_tile_indices(tiles, &indices);
            }
            return Ok(true);
        }

        let tile = if !stack && let Some((&first, rest)) = indices.split_first() {
            remove_tile_indices(tiles, rest);
            &mut tiles[first]
        } else {
            let tile = matching
                .map(|index| tiles.remove(index))
                .unwrap_or_else(|| json!({"a":1,"d":[y*width+x]}));
            tiles.push(tile);
            tiles.last_mut().expect("just pushed")
        };
        write_tile(tile, px, tile_id, src, flips);
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
        self.place_entity(key, layer_index, def_uid, [x, y], [x, y])
    }

    /// Place an entity from a level-space click. Its snapped anchor can lie outside the level.
    pub fn place_entity(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        cursor: [i32; 2],
        position: [i32; 2],
    ) -> Result<bool> {
        self.place_entity_with_outcome(key, layer_index, def_uid, cursor, position)
            .map(|outcome| matches!(outcome, EntityPlacement::Placed))
    }

    pub fn place_entity_with_outcome(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        cursor: [i32; 2],
        position: [i32; 2],
    ) -> Result<EntityPlacement> {
        self.edit_document(|p| p.add_entity_inner(key, layer_index, def_uid, cursor, position))
    }
    fn add_entity_inner(
        &mut self,
        key: LevelKey,
        layer_index: usize,
        def_uid: i64,
        cursor: [i32; 2],
        position: [i32; 2],
    ) -> Result<EntityPlacement> {
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
        let [click_x, click_y] = cursor;
        if click_x < 0 || click_y < 0 || i64::from(click_x) >= width || i64::from(click_y) >= height
        {
            return Ok(EntityPlacement::Ignored);
        }
        let [x, y] = position;
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
                return Ok(EntityPlacement::Ignored);
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
                "PreventAdding" => {
                    return Ok(EntityPlacement::LimitReached {
                        maximum: max,
                        identifier: definition["identifier"].as_str().unwrap_or_default().into(),
                    });
                }
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
            (i64::from(x) - i64::from(pivot_x == 1.0)) / i64::from(grid),
            (i64::from(y) - i64::from(pivot_y == 1.0)) / i64::from(grid)
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
        Ok(EntityPlacement::Placed)
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

/// Indices are in draw order. Compact once when a cell contains multiple tiles.
fn remove_tile_indices(tiles: &mut Vec<Value>, indices: &[usize]) {
    match indices {
        [] => {}
        [index] => {
            tiles.remove(*index);
        }
        _ => {
            let mut remove = indices.iter().copied().peekable();
            let mut index = 0;
            tiles.retain(|_| {
                let keep = remove.next_if_eq(&index).is_none();
                index += 1;
                keep
            });
        }
    }
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
