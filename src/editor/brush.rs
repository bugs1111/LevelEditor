use super::*;
use ldtk_core::TilePaint;

pub(super) const ASYNC_RENDER_DELAY: f64 = 0.25;

pub(super) struct EntityRenderSnapshot {
    level: LevelKey,
    layer_uid: i64,
    now: f64,
    entities: Option<Vec<Value>>,
}

pub(super) struct SelectionRenderSnapshot {
    index: usize,
    layer_uid: i64,
    field: &'static str,
    content: Vec<Value>,
}

struct TileRenderSnapshot {
    layer_uid: i64,
    grid: f32,
    now: f64,
    tiles: Option<Vec<Value>>,
}

pub(super) fn random_index(state: &mut u64, count: usize) -> usize {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (state.wrapping_mul(0x2545f4914f6cdd1d) % count as u64) as usize
}

fn tile_preview_color(
    assets: &mut Assets,
    tileset: &Value,
    path: &Path,
    id: i32,
    src: [i32; 2],
    grid: i32,
) -> Color32 {
    let [r, g, b, _] = canvas::cached_tile_average(tileset, id as usize)
        .or_else(|| assets.average_tile_color(path, src, [grid, grid]))
        .unwrap_or([136, 136, 136, 255]);
    Color32::from_rgb(r, g, b)
}

impl EditorApp {
    pub(super) fn async_preview(
        &mut self,
        level: LevelKey,
        now: f64,
    ) -> &mut canvas::AsyncPaintPreview {
        let preview = &mut self.async_paint;
        if !preview
            .as_ref()
            .is_some_and(|p| p.is_active_for(level, now))
        {
            *preview = Some(canvas::AsyncPaintPreview {
                level,
                deadline: now,
                layers: Default::default(),
                hint_grid: None,
                hints: Default::default(),
            });
        }
        let preview = preview.as_mut().unwrap();
        preview.deadline = now + ASYNC_RENDER_DELAY;
        preview
    }

    pub(super) fn capture_entity_render(
        &self,
        level: LevelKey,
        layer: usize,
    ) -> Option<EntityRenderSnapshot> {
        let project = self.project.as_ref()?;
        let instance = project.layer(level, layer)?;
        let layer_uid = instance["layerDefUid"].as_i64()?;
        if project.layer_def(layer_uid)?["useAsyncRender"] != true {
            return None;
        }
        let now = self.repaint.as_ref()?.input(|input| input.time);
        let retained = self
            .async_paint
            .as_ref()
            .filter(|preview| preview.is_active_for(level, now))
            .and_then(|preview| preview.layers.get(&layer_uid))
            .is_some_and(|layer| layer.old_entities.is_some());
        Some(EntityRenderSnapshot {
            level,
            layer_uid,
            now,
            entities: if retained {
                None
            } else {
                instance["entityInstances"].as_array().cloned()
            },
        })
    }

    pub(super) fn defer_entity_render(&mut self, snapshot: Option<EntityRenderSnapshot>) {
        if let Some(snapshot) = &snapshot {
            self.async_preview(snapshot.level, snapshot.now);
        }
        self.retain_entity_render(snapshot);
    }

    pub(super) fn retain_entity_render(&mut self, snapshot: Option<EntityRenderSnapshot>) {
        if let Some(snapshot) = snapshot
            && let Some(entities) = snapshot.entities
            && let Some(preview) = self
                .async_paint
                .as_mut()
                .filter(|p| p.is_active_for(snapshot.level, snapshot.now))
        {
            preview
                .layers
                .entry(snapshot.layer_uid)
                .or_default()
                .old_entities = Some(entities);
        }
    }

    pub(super) fn capture_selection_render(
        &self,
        key: LevelKey,
        group: &Value,
    ) -> Vec<SelectionRenderSnapshot> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        project
            .level(key)
            .and_then(|level| level["layerInstances"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, layer)| {
                let layer_uid = layer["layerDefUid"].as_i64()?;
                if project.layer_def(layer_uid)?["useAsyncRender"] != true {
                    return None;
                }
                let field = match layer["__type"].as_str()? {
                    "IntGrid" => "intGridCsv",
                    "Tiles" => "gridTiles",
                    "Entities" => "entityInstances",
                    _ => return None,
                };
                if (group["emptySpace"] != true || field == "entityInstances")
                    && !canvas::values(&group["layers"])
                        .iter()
                        .any(|entry| entry["layerDefUid"] == layer_uid)
                {
                    return None;
                }
                Some(SelectionRenderSnapshot {
                    index,
                    layer_uid,
                    field,
                    content: canvas::values(&layer[field]).to_vec(),
                })
            })
            .collect()
    }

    pub(super) fn defer_selection_render(
        &mut self,
        key: LevelKey,
        now: f64,
        snapshots: Vec<SelectionRenderSnapshot>,
    ) {
        if let Some(preview) = &mut self.async_paint
            && preview.is_active_for(key, now)
        {
            preview.deadline = now + ASYNC_RENDER_DELAY;
        }
        for old in snapshots {
            let layer = self.project.as_ref().and_then(|p| p.layer(key, old.index));
            if !layer.is_some_and(|l| canvas::values(&l[old.field]) != old.content) {
                continue;
            }
            let layer = self
                .async_preview(key, now)
                .layers
                .entry(old.layer_uid)
                .or_default();
            let retained = match old.field {
                "intGridCsv" => &mut layer.old_intgrid_cells,
                "gridTiles" => &mut layer.old_tiles,
                _ => &mut layer.old_entities,
            };
            retained.get_or_insert(old.content);
        }
    }

    fn paint_with_auto_rules<T>(
        &mut self,
        level: LevelKey,
        paint: impl FnOnce(
            &mut Project,
            Option<&mut ldtk_core::DeferredAutoLayers>,
        ) -> ldtk_core::Result<T>,
    ) -> ldtk_core::Result<T> {
        let project = self.project.as_mut().unwrap();
        let Some(now) = self
            .stroke
            .then_some(self.repaint.as_ref())
            .flatten()
            .map(|ctx| ctx.input(|i| i.time))
        else {
            return paint(project, None);
        };
        let pending = self
            .deferred_auto_paint
            .get_or_insert_with(|| DeferredAutoPaint {
                level,
                deadline: now,
                rules: Default::default(),
                prior_selection: None,
            });
        let result = paint(project, Some(&mut pending.rules));
        if pending.rules.is_empty() {
            self.deferred_auto_paint = None;
        } else if result.is_ok() {
            pending.deadline = now + ASYNC_RENDER_DELAY;
        }
        result
    }

    pub(super) fn paint_cells(&mut self, cells: &[[i32; 2]], erase: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        if layer["__type"] == "IntGrid" {
            let async_layer = layer["layerDefUid"].as_i64().and_then(|uid| {
                let def = project.layer_def(uid)?;
                (def["useAsyncRender"] == true).then_some((uid, def))
            });
            let pending = async_layer.and_then(|(uid, def)| {
                let now = self.repaint.as_ref()?.input(|i| i.time);
                let color = if erase {
                    Color32::RED
                } else {
                    let value = canvas::values(&def["intGridValues"])
                        .iter()
                        .find(|v| v["value"] == self.int_value)?;
                    theme::color(text(value, "color", "#ffcc00"))
                };
                Some((uid, number(layer, "__gridSize", 16.), now, color))
            });
            let changes = cells
                .iter()
                .map(|&cell| (cell, if erase { 0 } else { self.int_value }))
                .collect::<Vec<_>>();
            let width = number(layer, "__cWid", 0.) as i32;
            let height = number(layer, "__cHei", 0.) as i32;
            let mut previous = Vec::new();
            for &(cell, value) in &changes {
                if cell[0] >= 0 && cell[1] >= 0 && cell[0] < width && cell[1] < height {
                    let old = layer["intGridCsv"][(cell[0] + cell[1] * width) as usize].as_i64();
                    if old == Some(i64::from(value)) {
                        continue;
                    }
                    if pending.is_some()
                        && let Some(old) = old
                    {
                        previous.push((cell, old));
                    }
                    self.history.mark_grid_change(
                        self.project.as_ref().unwrap(),
                        key,
                        self.layer,
                        cell[0],
                        cell[1],
                    );
                }
            }
            let layer_index = self.layer;
            let result = self.paint_with_auto_rules(key, |project, pending| {
                project.paint_int_grid_cells_with_deferred(key, layer_index, &changes, pending)
            });
            match result {
                Ok(count) => {
                    self.dirty |= count > 0;
                    if count > 0
                        && let Some((layer_uid, grid, now, color)) = pending
                    {
                        let preview = self.async_preview(key, now);
                        preview.hint_grid.get_or_insert(grid);
                        let layer = preview.layers.entry(layer_uid).or_default();
                        for (cell, old) in previous {
                            if layer.old_intgrid_cells.is_none() {
                                layer.old_intgrid.entry(cell).or_insert(old);
                            }
                            preview.hints.insert(cell, color);
                        }
                    }
                }
                Err(e) => {
                    self.status = e.to_string();
                    self.status_error = true;
                }
            }
            return;
        }
        if layer["__type"] != "Tiles" {
            return;
        }
        let Some(tileset) = layer["__tilesetDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.tileset(uid))
        else {
            return;
        };
        let scale = (number(tileset, "tileGridSize", 16.0) / number(layer, "__gridSize", 16.0))
            .round()
            .max(1.0) as i32;
        let tile = self.tile_rect;
        let uid = tileset["uid"].as_i64().unwrap_or(-1);
        let columns = number(tileset, "__cWid", 1.0).max(1.0) as i32;
        let ids = self.selected_tile_ids_for(uid);
        if ids.is_empty() {
            return;
        }
        let width = number(layer, "__cWid", 0.) as i32;
        let height = number(layer, "__cHei", 0.) as i32;
        let inside = |[x, y]: [i32; 2]| (0..width).contains(&x) && (0..height).contains(&y);
        if self.random_tiles {
            let mut seen = HashSet::new();
            if cells.len() > 1
                && (!self.tile_stacking || cells.iter().all(|&cell| seen.insert(cell)))
            {
                let mut tiles = self.sample_random_tiles(cells.iter().copied(), columns, &ids);
                // The random brush consumes a choice even when a cell is outside the layer.
                tiles.retain(|(cell, _)| inside(*cell));
                self.apply_tile_stroke(tiles, erase, self.tile_stacking);
                return;
            }
            // On a repeated cell, stacking can skip the next choice only after a
            // successful edit. Keep that decision in the same order as the input.
            for &cell in cells {
                if self.painted_cells.contains(&cell) {
                    continue;
                }
                let id = ids[random_index(&mut self.random_state, ids.len())];
                self.paint_tile_at(
                    cell,
                    [id % columns, id / columns],
                    erase,
                    self.tile_stacking,
                );
            }
            return;
        }

        let flips = if erase { 0 } else { self.flips };
        let tiles = cells
            .iter()
            .flat_map(|&cell| tools::stamp_cells(cell, tile, scale, flips))
            .filter(|&(cell, src)| inside(cell) && ids.contains(&(src[0] + src[1] * columns)))
            .collect();
        let stack = self.tile_stacking && (!erase || ids.len() == 1);
        self.apply_tile_stroke(tiles, erase, stack);
    }

    fn apply_tile_stroke(&mut self, mut tiles: Vec<([i32; 2], [i32; 2])>, erase: bool, stack: bool) {
        if !erase {
            self.paint_tile_region(tiles);
            return;
        }

        if stack {
            tiles.retain(|(cell, _)| !self.painted_cells.contains(cell));
        }
        if tiles.len() > 1 {
            let destinations: Vec<_> = tiles.iter().map(|&(dest, _)| dest).collect();
            if self.erase_tile_region(&destinations, stack).is_ok() {
                return;
            }
        }
        for (dest, src) in tiles {
            self.paint_tile_at(dest, src, true, stack);
        }
    }

    fn paint_tile_at(&mut self, cell: [i32; 2], tile: [i32; 2], erase: bool, stack: bool) {
        if stack && self.painted_cells.contains(&cell) {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let width = number(layer, "__cWid", 0.0) as i32;
        let height = number(layer, "__cHei", 0.0) as i32;
        if cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height {
            return;
        }
        let Some(def) = layer["__tilesetDefUid"]
            .as_i64()
            .and_then(|uid| project.tileset(uid))
        else {
            return;
        };
        let grid = number(def, "tileGridSize", 16.0) as i32;
        let spacing = number(def, "spacing", 0.0) as i32;
        let padding = number(def, "padding", 0.0) as i32;
        let columns = number(def, "__cWid", 1.0).max(1.0) as i32;
        let src = [
            padding + tile[0] * (grid + spacing),
            padding + tile[1] * (grid + spacing),
        ];
        let asset = def["relPath"]
            .as_str()
            .map(|path| canvas::asset_path(project, path));
        let stacking = if erase {
            stack
        } else {
            let opacity = if let Some(path) = &asset {
                self.assets.tile_is_opaque(path, src, [grid, grid])
            } else {
                self.assets.embedded_tile_is_opaque(src, [grid, grid])
            };
            let Some(opaque) = opacity else {
                self.status = "Waiting for tileset texture…".into();
                return;
            };
            stack && !opaque
        };
        let tile_id = tile[0] + tile[1] * columns;
        let pending = self.capture_tile_render(key, layer).map(|snapshot| {
            let color = if erase {
                Color32::RED
            } else {
                let path = asset
                    .as_deref()
                    .unwrap_or_else(|| Path::new("@ldtk/embedded-atlas"));
                tile_preview_color(&mut self.assets, def, path, tile_id, src, grid)
            };
            (snapshot, color)
        });
        let layer_index = self.layer;
        let flips = self.flips;
        match self.paint_with_auto_rules(key, |project, pending| {
            project.paint_tile_stacked_with_deferred(
                key,
                layer_index,
                cell[0],
                cell[1],
                tile_id,
                src,
                flips,
                erase,
                stacking,
                pending,
            )
        }) {
            Ok(changed) => {
                self.dirty |= changed;
                if changed {
                    self.history.mark_grid_change(
                        self.project.as_ref().unwrap(),
                        key,
                        self.layer,
                        cell[0],
                        cell[1],
                    );
                    if let Some((snapshot, color)) = pending {
                        self.defer_tile_render(key, snapshot, [(cell, color)]);
                    }
                }
                if stack {
                    self.painted_cells.insert(cell);
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }

    fn capture_tile_render(&self, key: LevelKey, layer: &Value) -> Option<TileRenderSnapshot> {
        let layer_uid = layer["layerDefUid"].as_i64()?;
        if self.project.as_ref()?.layer_def(layer_uid)?["useAsyncRender"] != true {
            return None;
        }
        let now = self.repaint.as_ref()?.input(|input| input.time);
        let tiles = self
            .async_paint
            .as_ref()
            .filter(|preview| preview.is_active_for(key, now))
            .and_then(|preview| preview.layers.get(&layer_uid))
            .is_none_or(|layer| layer.old_tiles.is_none())
            .then(|| canvas::values(&layer["gridTiles"]).to_vec());
        Some(TileRenderSnapshot {
            layer_uid,
            grid: number(layer, "__gridSize", 16.),
            now,
            tiles,
        })
    }

    fn defer_tile_render(
        &mut self,
        key: LevelKey,
        snapshot: TileRenderSnapshot,
        hints: impl IntoIterator<Item = ([i32; 2], Color32)>,
    ) {
        let preview = self.async_preview(key, snapshot.now);
        preview.hint_grid.get_or_insert(snapshot.grid);
        if let Some(tiles) = snapshot.tiles {
            preview.layers.entry(snapshot.layer_uid).or_default().old_tiles = Some(tiles);
        }
        preview.hints.extend(hints);
    }

    fn erase_tile_region(&mut self, cells: &[[i32; 2]], stack: bool) -> ldtk_core::Result<()> {
        if cells.is_empty() {
            return Ok(());
        }
        let Some(key) = self.selected else {
            return Ok(());
        };
        let Some(project) = &self.project else {
            return Ok(());
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return Ok(());
        };
        if layer["__tilesetDefUid"].as_i64().and_then(|uid| project.tileset(uid)).is_none() {
            return Ok(());
        }
        let snapshot = self.capture_tile_render(key, layer);
        let index = self.layer;
        let changed = self.paint_with_auto_rules(key, |project, pending| {
            project.erase_tile_cells_stacked_with_deferred(key, index, cells, stack, pending)
        })?;
        if stack {
            self.painted_cells.extend(cells.iter().copied());
        }
        if changed.is_empty() {
            return Ok(());
        }
        self.dirty = true;
        let project = self.project.as_ref().unwrap();
        for &[x, y] in &changed {
            self.history.mark_grid_change(project, key, index, x, y);
        }
        if let Some(snapshot) = snapshot {
            self.defer_tile_render(
                key, snapshot, changed.into_iter().map(|cell| (cell, Color32::RED)),
            );
        }
        Ok(())
    }

    fn paint_tile_region(&mut self, tiles: impl IntoIterator<Item = ([i32; 2], [i32; 2])>) {
        let tiles: Vec<_> = tiles.into_iter().collect();
        if tiles.len() > 1 && self.try_paint_tile_region(&tiles) {
            return;
        }
        for (dest, src) in tiles {
            self.paint_tile_at(dest, src, false, self.tile_stacking);
        }
    }

    fn paint_random_tile_region(
        &mut self,
        cells: impl IntoIterator<Item = [i32; 2]>,
        columns: i32,
        ids: &[i32],
    ) {
        if ids.is_empty() {
            return;
        }
        let tiles = self.sample_random_tiles(cells, columns, ids);
        self.paint_tile_region(tiles);
    }

    fn sample_random_tiles(
        &mut self,
        cells: impl IntoIterator<Item = [i32; 2]>,
        columns: i32,
        ids: &[i32],
    ) -> Vec<([i32; 2], [i32; 2])> {
        cells
            .into_iter()
            .filter(|cell| !self.painted_cells.contains(cell))
            .map(|cell| {
                let id = ids[random_index(&mut self.random_state, ids.len())];
                (cell, [id % columns, id / columns])
            })
            .collect()
    }

    fn try_paint_tile_region(&mut self, tiles: &[([i32; 2], [i32; 2])]) -> bool {
        let Some((project, key)) = self.project.as_ref().zip(self.selected) else {
            return true;
        };
        let index = self.layer;
        let Some(layer) = project.layer(key, index) else {
            return true;
        };
        let Some(def) = layer["__tilesetDefUid"].as_i64().and_then(|uid| project.tileset(uid)) else {
            return true;
        };
        let grid = number(def, "tileGridSize", 16.) as i32;
        let stride = grid + number(def, "spacing", 0.) as i32;
        let padding = number(def, "padding", 0.) as i32;
        let columns = number(def, "__cWid", 1.).max(1.) as i32;
        let asset = def["relPath"].as_str().map(|path| canvas::asset_path(project, path));
        let path = asset.as_deref().unwrap_or_else(|| Path::new("@ldtk/embedded-atlas"));
        let mut seen = HashSet::new();
        let mut edits: Vec<_> = tiles
            .iter()
            .copied()
            .filter(|(cell, _)| {
                !self.tile_stacking || (!self.painted_cells.contains(cell) && seen.insert(*cell))
            })
            .map(|(cell, [x, y])| TilePaint {
                cell,
                id: x + y * columns,
                src: [padding + x * stride, padding + y * stride],
                stack: false,
            })
            .collect();
        let Some(first) = edits.first() else {
            return true;
        };
        if edits.len() == 1 {
            return false;
        }
        let first_opacity = self.assets.tile_is_opaque(path, first.src, [grid, grid]);
        if first_opacity.is_none() {
            self.status = "Waiting for tileset texture…".into();
            return true;
        }
        if self.tile_stacking {
            let mut opacity = std::collections::HashMap::from([(first.src, first_opacity)]);
            for edit in &mut edits {
                edit.stack = *opacity.entry(edit.src).or_insert_with(|| {
                    self.assets.tile_is_opaque(path, edit.src, [grid, grid])
                }) == Some(false);
            }
        }
        let render = self.capture_tile_render(key, layer).map(|snapshot| {
            let mut colors = std::collections::HashMap::new();
            for &TilePaint { id, src, .. } in &edits {
                colors.entry((id, src)).or_insert_with(|| {
                    tile_preview_color(&mut self.assets, def, path, id, src, grid)
                });
            }
            (snapshot, colors)
        });
        let flips = self.flips;
        let Ok(changed) = self.paint_with_auto_rules(key, |project, pending| {
            project.paint_tile_cells_with_deferred(key, index, &edits, flips, pending)
        }) else {
            // The batch is atomic. Let single-cell painting report any invalid edits as usual.
            return false;
        };
        if self.tile_stacking {
            self.painted_cells.extend(edits.iter().map(|edit| edit.cell));
        }
        if !changed.is_empty() {
            self.dirty = true;
            let project = self.project.as_ref().unwrap();
            for &edit in &changed {
                let [x, y] = edits[edit].cell;
                self.history.mark_grid_change(project, key, index, x, y);
            }
            if let Some((snapshot, colors)) = render {
                self.defer_tile_render(key, snapshot, changed.into_iter().map(|edit| {
                    let TilePaint { cell, id, src, .. } = edits[edit];
                    (cell, colors[&(id, src)])
                }));
            }
        }
        true
    }

    pub(super) fn paint_rectangle(&mut self, rect: [i32; 4], erase: bool) {
        let Some(layer) = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
        else {
            return;
        };
        let left = rect[0].max(0);
        let top = rect[1].max(0);
        let right = rect[0].saturating_add(rect[2]).min(number(layer, "__cWid", 0.) as i32);
        let bottom = rect[1].saturating_add(rect[3]).min(number(layer, "__cHei", 0.) as i32);
        if left >= right || top >= bottom {
            return;
        }
        let cells = || (top..bottom).flat_map(|y| (left..right).map(move |x| [x, y]));
        if layer["__type"] == "IntGrid" {
            self.paint_cells(&cells().collect::<Vec<_>>(), erase);
        } else if layer["__type"] == "Tiles" {
            let Some(def) = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| self.project.as_ref()?.tileset(uid))
            else {
                return;
            };
            let scale = (number(def, "tileGridSize", 16.0) / number(layer, "__gridSize", 16.0))
                .round()
                .max(1.0) as i32;
            let columns = number(def, "__cWid", 1.0).max(1.0) as i32;
            let ids = self.selected_tile_ids_for(def["uid"].as_i64().unwrap_or(-1));
            // Rectangle erasing always follows the destination cells; the selection only controls drawing.
            if erase {
                let cells: Vec<_> = cells()
                    .filter(|cell| !self.tile_stacking || !self.painted_cells.contains(cell))
                    .collect();
                if self.erase_tile_region(&cells, self.tile_stacking).is_err() {
                    // The batch is atomic. Replay single-cell edits to preserve partial success.
                    for cell in cells {
                        self.paint_tile_at(cell, [0, 0], true, self.tile_stacking);
                    }
                }
            } else if self.random_tiles {
                self.paint_random_tile_region(cells(), columns, &ids);
            } else {
                // Preserve the stamp phase of the raw rectangle at the clipped boundary.
                let tiles = tools::rectangle_tiles(
                    [rect[0], rect[1]],
                    [left, top, right, bottom],
                    self.tile_rect,
                    scale,
                );
                self.paint_tile_region(tiles.into_iter().filter(|(_, [x, y])| {
                    ids.contains(&(*x + *y * columns))
                }));
            }
        }
    }

    pub(super) fn fill(&mut self, start: [i32; 2], erase: bool) {
        let Some(layer) = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
        else {
            return;
        };
        let width = number(layer, "__cWid", 0.0) as i32;
        let height = number(layer, "__cHei", 0.0) as i32;
        if start[0] < 0 || start[1] < 0 || start[0] >= width || start[1] >= height {
            return;
        }
        let kind = text(layer, "__type", "");
        let mut values = vec![0i64; (width * height) as usize];
        if kind == "IntGrid" {
            for (dst, src) in values
                .iter_mut()
                .zip(layer["intGridCsv"].as_array().into_iter().flatten())
            {
                *dst = src.as_i64().unwrap_or(0);
            }
        } else if kind == "Tiles" {
            values.fill(-1);
            let grid = number(layer, "__gridSize", 16.0) as i32;
            for tile in layer["gridTiles"].as_array().into_iter().flatten() {
                let x = tile["px"][0].as_i64().unwrap_or(0) as i32 / grid;
                let y = tile["px"][1].as_i64().unwrap_or(0) as i32 / grid;
                if x >= 0 && y >= 0 && x < width && y < height {
                    values[(x + y * width) as usize] = tile["t"].as_i64().unwrap_or(-1);
                }
            }
        } else {
            return;
        }
        let target = values[(start[0] + start[1] * width) as usize];
        if kind == "Tiles" && target >= 0 {
            let grid = number(layer, "__gridSize", 16.0) as i32;
            values.fill(0);
            for tile in layer["gridTiles"].as_array().into_iter().flatten() {
                if tile["t"].as_i64() != Some(target) {
                    continue;
                }
                let x = tile["px"][0].as_i64().unwrap_or(0) as i32 / grid;
                let y = tile["px"][1].as_i64().unwrap_or(0) as i32 / grid;
                if x >= 0 && y >= 0 && x < width && y < height {
                    values[(x + y * width) as usize] = 1;
                }
            }
        }
        let target = if kind == "Tiles" && target >= 0 {
            1
        } else {
            target
        };
        if kind == "IntGrid" && target == if erase { 0 } else { self.int_value as i64 } {
            return;
        }
        let mut seen = vec![false; values.len()];
        let mut pending = vec![start];
        let mut cells = Vec::new();
        while let Some(cell) = pending.pop() {
            if cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height {
                continue;
            }
            let index = (cell[0] + cell[1] * width) as usize;
            if seen[index] || values[index] != target {
                continue;
            }
            seen[index] = true;
            cells.push(cell);
            pending.extend([
                [cell[0] - 1, cell[1]],
                [cell[0] + 1, cell[1]],
                [cell[0], cell[1] - 1],
                [cell[0], cell[1] + 1],
            ]);
        }
        if kind == "Tiles" && erase {
            if let Err(error) = self.erase_tile_region(&cells, false) {
                self.status = error.to_string();
                self.status_error = true;
            }
        } else if kind == "IntGrid" {
            self.paint_cells(&cells, erase);
        } else {
            // Source bucket fill repeats the rectangular stamp relative to the clicked cell.
            let Some(layer) = self
                .selected
                .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
            else {
                return;
            };
            let Some(def) = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| self.project.as_ref()?.tileset(uid))
            else {
                return;
            };
            let columns = number(def, "__cWid", 1.0).max(1.0) as i32;
            let ids = self.selected_tile_ids_for(def["uid"].as_i64().unwrap_or(-1));
            if self.random_tiles {
                self.paint_random_tile_region(cells, columns, &ids);
                return;
            }
            let scale = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| self.project.as_ref()?.tileset(uid))
                .map(|d| {
                    (number(d, "tileGridSize", 16.0) / number(layer, "__gridSize", 16.0))
                        .round()
                        .max(1.0) as i32
                })
                .unwrap_or(1);
            let left = cells.iter().map(|p| p[0]).min().unwrap_or(start[0]);
            let top = cells.iter().map(|p| p[1]).min().unwrap_or(start[1]);
            let tile = self.tile_rect;
            self.paint_tile_region(cells.into_iter().filter_map(|cell| {
                let dx = cell[0] - left;
                let dy = cell[1] - top;
                if dx.rem_euclid(scale) != 0 || dy.rem_euclid(scale) != 0 {
                    return None;
                }
                let src = [
                    tile[0] + dx.div_euclid(scale).rem_euclid(tile[2]),
                    tile[1] + dy.div_euclid(scale).rem_euclid(tile[3]),
                ];
                ids.contains(&(src[0] + src[1] * columns)).then_some((cell, src))
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sparse_editor() -> (EditorApp, LevelKey) {
        let mut p = Project::new();
        let layer = p.add_layer("Tiles", "Tiles", 16).unwrap();
        let uid = p
            .add_tileset(std::path::Path::new("test.png"), 16, 48, 16)
            .unwrap();
        p.assign_tileset(layer, uid).unwrap();
        p.json["defs"]["tilesets"][0]["relPath"] = Value::Null;
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.tile_rect = [0, 0, 3, 1];
        app.atlas_selection = Some((uid, vec![0, 2]));
        app.assets.embedded(&egui::Context::default());
        let key = app.selected.unwrap();
        (app, key)
    }

    #[test]
    fn tile_region_paint_preserves_preview_and_stroke_tracking() {
        let (mut batch, key) = sparse_editor();
        let project = batch.project.as_mut().unwrap();
        project.json["defs"]["layers"][0]["useAsyncRender"] = Value::Bool(true);
        let uid = project.layer(key, 0).unwrap()["layerDefUid"].as_i64().unwrap();
        let mut sequential = EditorApp::empty();
        sequential.set_project(project.clone());
        for app in [&mut batch, &mut sequential] {
            let ctx = egui::Context::default();
            app.assets.embedded(&ctx);
            app.repaint = Some(ctx);
            app.flips = 1;
            app.stroke = true;
            app.history.begin_layer(app.project.as_ref().unwrap(), key, 0);
        }

        let original = batch.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"].clone();
        batch.paint_rectangle([0, 0, 3, 1], false);
        sequential.paint_tile_at([0, 0], [0, 0], false, true);
        sequential.paint_tile_at([2, 0], [2, 0], false, true);

        let preview = batch.async_paint.as_ref().unwrap();
        assert_eq!(preview.layers[&uid].old_tiles.as_ref(), original.as_array());
        assert_eq!(preview.hints, sequential.async_paint.as_ref().unwrap().hints);
        assert_eq!(batch.painted_cells, sequential.painted_cells);
        assert_eq!(
            batch.project.as_ref().unwrap().json,
            sequential.project.as_ref().unwrap().json
        );
        batch.finish_stroke();
        let project = batch.project.as_mut().unwrap();
        let painted = project.layer(key, 0).unwrap()["gridTiles"].clone();
        assert!(batch.history.undo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], original);
        assert!(batch.history.redo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], painted);
    }

    #[test]
    fn rectangle_erase_keeps_lower_tiles_and_does_not_repeat_within_the_stroke() {
        use serde_json::json;

        let (mut app, key) = sparse_editor();
        let bottom = json!([{"px":[0,0],"t":0},{"px":[16,0],"t":1}]);
        app.project.as_mut().unwrap().layer_mut(key, 0).unwrap()["gridTiles"] = json!([
            bottom[0], {"px":[16,0],"t":2}, {"px":[0,0],"t":2}, bottom[1], {"px":[16,0],"t":0}
        ]);
        app.paint_rectangle([0, 0, 2, 1], true);
        let expected = json!([bottom[0], {"px":[16,0],"t":2}, bottom[1]]);
        assert_eq!(app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"], expected);
        app.paint_rectangle([0, 0, 2, 1], true);
        assert_eq!(app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"], expected);
        app.tile_stacking = false;
        app.paint_rectangle([0, 0, 2, 1], true);
        assert_eq!(app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"], json!([]));
    }

    #[test]
    fn tile_bucket_erase_preserves_async_preview_and_undo() {
        use serde_json::json;

        let (mut app, key) = sparse_editor();
        let project = app.project.as_mut().unwrap();
        let original = json!([
            {"px":[0,0],"t":2}, {"px":[0,0],"t":0}, {"px":[16,0],"t":0}, {"px":[48,0],"t":0}
        ]);
        project.json["defs"]["layers"][0]["useAsyncRender"] = json!(true);
        project.layer_mut(key, 0).unwrap()["gridTiles"] = original.clone();
        let uid = project.layer(key, 0).unwrap()["layerDefUid"].as_i64().unwrap();
        app.history.begin_layer(project, key, 0);
        app.repaint = Some(egui::Context::default());
        app.stroke = true;

        app.fill([0, 0], true);

        let preview = app.async_paint.as_ref().unwrap();
        assert_eq!(preview.layers[&uid].old_tiles.as_ref(), original.as_array());
        assert_eq!(preview.hints, [([0, 0], Color32::RED), ([1, 0], Color32::RED)].into());
        app.finish_stroke();
        let project = app.project.as_mut().unwrap();
        let remaining = json!([{"px":[48,0],"t":0}]);
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], remaining);
        assert!(app.history.undo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], original);
        assert!(app.history.redo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], remaining);
    }

    #[test]
    fn overlapping_drag_stamps_keep_the_order_of_sequential_painting() {
        use serde_json::json;

        let (mut app, key) = sparse_editor();
        app.flips = 1;
        app.tile_stacking = false;
        app.paint_cells(&[[0, 0], [2, 0]], false);
        let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
            .as_array().unwrap();
        let positions_and_ids: Vec<_> = tiles.iter().map(|tile| (&tile["px"], &tile["t"])).collect();
        assert_eq!(positions_and_ids, [
            (&json!([32, 0]), &json!(2)),
            (&json!([0, 0]), &json!(2)),
            (&json!([64, 0]), &json!(0)),
        ]);
        assert!(tiles.iter().all(|tile| tile["f"] == 1));
        assert!(!app.status_error);
    }

    #[test]
    fn random_drag_matches_sequential_paint_and_erase() {
        use serde_json::json;

        let distinct = [[-1, 0], [0, 0], [1, 0], [2, 0]];
        let repeated = [[-1, 0], [0, 0], [0, 0], [1, 0], [2, 0]];
        for cells in [distinct.as_slice(), repeated.as_slice()] {
            for erase in [false, true] {
                let (mut batch, key) = sparse_editor();
                let (mut sequential, _) = sparse_editor();
                for app in [&mut batch, &mut sequential] {
                    let project = app.project.as_mut().unwrap();
                    project.layer_mut(key, 0).unwrap()["gridTiles"] = json!([
                        {"px":[0,0],"t":0}, {"px":[0,0],"t":2},
                        {"px":[16,0],"t":2}, {"px":[32,0],"t":2}
                    ]);
                    app.random_tiles = true;
                    app.tile_stacking = true;
                    app.flips = 1;
                    app.random_state = 42;
                    app.painted_cells.insert([1, 0]);
                }

                batch.paint_cells(cells, erase);
                for &cell in cells {
                    if !sequential.painted_cells.contains(&cell) {
                        let tile = [0, 2][random_index(&mut sequential.random_state, 2)];
                        sequential.paint_tile_at(cell, [tile, 0], erase, true);
                    }
                }

                assert_eq!(
                    batch.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"],
                    sequential.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                );
                assert_eq!(batch.random_state, sequential.random_state);
                assert_eq!(batch.painted_cells, sequential.painted_cells);
                assert_eq!(batch.dirty, sequential.dirty);
                assert!(!batch.status_error && !sequential.status_error);
            }
        }
    }

    #[test]
    fn drag_erase_leaves_lower_tiles_and_only_erases_once_per_stroke() {
        use serde_json::json;

        let (mut app, key) = sparse_editor();
        let project = app.project.as_mut().unwrap();
        let atlas = project.json["defs"]["tilesets"][0]["uid"].as_i64().unwrap();
        let lower = json!([{"px":[0,0],"t":0},{"px":[16,0],"t":1}]);
        let original = json!([
            lower[0], {"px":[0,0],"t":2}, lower[1], {"px":[16,0],"t":2}
        ]);
        project.layer_mut(key, 0).unwrap()["gridTiles"] = original.clone();
        app.atlas_selection = Some((atlas, vec![0]));
        app.tile_rect = [0, 0, 1, 1];
        app.tile_stacking = true;
        app.stroke = true;
        app.history.begin_layer(project, key, 0);
        let cells = [[-1, 0], [0, 0], [1, 0], [1, 0]];
        app.paint_cells(&cells, true);
        app.paint_cells(&cells, true);
        assert_eq!(app.painted_cells, HashSet::from([[0, 0], [1, 0]]));
        app.finish_stroke();
        let project = app.project.as_mut().unwrap();
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], lower);
        assert!(app.history.undo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], original);
        assert!(app.history.redo(project));
        assert_eq!(project.layer(key, 0).unwrap()["gridTiles"], lower);
    }

    #[test]
    #[ignore = "manual overlapping tile stamp performance comparison"]
    fn benchmark_large_overlapping_tile_stamps() {
        use serde_json::json;
        use std::time::{Duration, Instant};

        let (mut template, key) = sparse_editor();
        let project = template.project.as_mut().unwrap();
        let (width, height) = (128, 96);
        project.resize_level(key, width * 16, height * 16, 0, 0).unwrap();
        let row = 48;
        let cells: Vec<_> = (4..68).map(|x| [x, row]).collect();
        let original: Vec<_> = (0..height)
            .flat_map(|y| (0..width).map(move |x| {
                json!({"px":[x * 16, y * 16],"t":0,"src":[0,0],"f":0,"a":0.75})
            }))
            .collect();
        project.layer_mut(key, 0).unwrap()["gridTiles"] = Value::Array(original.clone());
        let atlas = project.json["defs"]["tilesets"][0]["uid"].as_i64().unwrap();

        for stacking in [false, true] {
            let mut times = [Duration::ZERO; 5];
            for time in &mut times {
                let mut app = EditorApp::empty();
                app.set_project(project.clone());
                app.assets.store_file_image(
                    &egui::Context::default(),
                    Path::new("@ldtk/embedded-atlas"),
                    image::RgbaImage::from_pixel(48, 16, image::Rgba([255; 4])),
                ).unwrap();
                app.tile_rect = [1, 0, 2, 1];
                app.atlas_selection = Some((atlas, vec![1, 2]));
                app.tile_stacking = stacking;
                app.flips = 1;

                let start = Instant::now();
                app.paint_cells(&cells, false);
                *time = start.elapsed();

                assert!(app.dirty && !app.status_error);
                let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                    .as_array().unwrap();
                let start = (row * width + 4) as usize;
                let end = start + cells.len() + 1;
                assert_eq!(tiles.len(), original.len());
                assert_eq!(&tiles[..start], &original[..start]);
                assert_eq!(&tiles[end..], &original[end..]);
                for (index, tile) in tiles[start..end].iter().enumerate() {
                    let x = index as i32 + 4;
                    let id = if x == 68 || (stacking && x > 4) { 1 } else { 2 };
                    assert_eq!(*tile, json!({
                        "px":[x * 16,row * 16],"t":id,"src":[id * 16,0],"f":1,"a":0.75
                    }));
                }
            }
            times.sort_unstable();
            let mode = if stacking { "stacking" } else { "overwrite" };
            eprintln!("Overlapping stamps {mode}: median {:?}; samples {times:?}", times[2]);
        }
    }

    #[test]
    #[ignore = "manual interpolated tile stroke performance comparison"]
    fn benchmark_large_interpolated_tile_stroke() {
        benchmark_interpolated_tile_stroke(false);
    }

    #[test]
    #[ignore = "manual interpolated random tile stroke performance comparison"]
    fn benchmark_large_interpolated_random_tile_stroke() {
        benchmark_interpolated_tile_stroke(true);
    }

    fn benchmark_interpolated_tile_stroke(random: bool) {
        use serde_json::json;
        use std::time::{Duration, Instant};

        let (mut template, key) = sparse_editor();
        let project = template.project.as_mut().unwrap();
        let (width, height) = (128, 96);
        project.resize_level(key, width * 16, height * 16, 0, 0).unwrap();
        let row = 48;
        let cells: Vec<_> = (4..100).map(|x| [x, row]).collect();
        let values: Vec<_> = (0..height)
            .flat_map(|y| (0..width).map(move |x| {
                json!({"px":[x * 16,y * 16],"t":0,"f":0,"src":[0,0],"a":0.75})
            }))
            .collect();
        project.layer_mut(key, 0).unwrap()["gridTiles"] = Value::Array(values.clone());
        let atlas = project.json["defs"]["tilesets"][0]["uid"].as_i64().unwrap();
        for erase in [false, true] {
            let mut samples = [Duration::ZERO; 5];
            for time in &mut samples {
                let mut app = EditorApp::empty();
                app.set_project(project.clone());
                app.assets.embedded(&egui::Context::default());
                app.tile_stacking = false;
                app.tile_rect = [2, 0, 1, 1];
                let ids = if random { vec![0, 2] } else { vec![2] };
                app.atlas_selection = Some((atlas, ids.clone()));
                app.flips = 1;
                app.random_tiles = random;
                app.random_state = 42;
                let mut expected_state = app.random_state;
                let expected_ids: Vec<_> = cells.iter().map(|_| {
                    if random { ids[random_index(&mut expected_state, ids.len())] } else { 2 }
                }).collect();
                let start = Instant::now();
                app.paint_cells(&cells, erase);
                *time = start.elapsed();
                assert_eq!(app.random_state, expected_state);
                let painted = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                    .as_array().unwrap();
                assert!(app.dirty && !app.status_error);
                let from = (row * width + 4) as usize;
                let to = from + cells.len();
                assert_eq!(&painted[..from], &values[..from]);
                assert_eq!(&painted[painted.len() - (values.len() - to)..], &values[to..]);
                if erase {
                    assert_eq!(painted.len(), values.len() - cells.len());
                } else {
                    assert_eq!(painted.len(), values.len());
                    for ((tile, &[x, y]), id) in painted[from..to].iter().zip(&cells).zip(expected_ids) {
                        assert_eq!(*tile, json!({"px":[x * 16,y * 16],"t":id,"f":1,"src":[id * 16,0],"a":0.75}));
                    }
                }
            }
            samples.sort_unstable();
            let operation = if erase { "erase" } else { "paint" };
            let brush = if random { "random tile" } else { "tile" };
            eprintln!("Interpolated {brush} stroke {operation}: median {:?}; samples {samples:?}", samples[2]);
        }
    }

    #[test]
    #[ignore = "manual bucket erase performance comparison"]
    fn benchmark_large_tile_bucket_erase() {
        use serde_json::json;
        use std::time::Instant;

        let (mut template, key) = sparse_editor();
        template.project.as_mut().unwrap().resize_level(key, 64 * 16, 64 * 16, 0, 0).unwrap();
        let mut tiles: Vec<_> = (0..64)
            .flat_map(|y| (0..62).map(move |x| json!({"px":[x * 16, y * 16],"t":0})))
            .collect();
        let survivors = [
            json!({"px":[63 * 16,0],"t":0}),
            json!({"px":[63 * 16,0],"t":2}),
            json!({"px":[63 * 16,16],"t":2}),
        ];
        tiles.insert(5, survivors[0].clone());
        tiles.insert(9, survivors[1].clone());
        tiles.push(json!({"px":[0,0],"t":2}));
        tiles.push(survivors[2].clone());
        // The top tile determines the target, but all tiles in reached cells are erased.
        tiles.push(json!({"px":[0,0],"t":0}));
        let project = template.project.as_ref().unwrap();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.project.as_mut().unwrap().layer_mut(key, 0).unwrap()["gridTiles"] = json!(tiles);
            let start = Instant::now();
            app.fill([0, 0], true);
            samples.push(start.elapsed());
            assert!(app.dirty);
            assert!(!app.status_error);
            assert_eq!(
                app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"],
                json!(survivors)
            );
        }
        samples.sort_unstable();
        eprintln!("Large tile bucket erase: median {:?}; samples {samples:?}", samples[2]);
    }

    #[test]
    #[ignore = "manual bucket paint performance comparison"]
    fn benchmark_large_tile_bucket_paint() {
        use serde_json::json;
        use std::time::Instant;

        let (mut template, key) = sparse_editor();
        let project = template.project.as_mut().unwrap();
        project.resize_level(key, 64 * 16, 64 * 16, 0, 0).unwrap();
        let atlas = project.json["defs"]["tilesets"][0]["uid"].as_i64().unwrap();
        let wall = (0..64)
            .map(|y| json!({"px":[62 * 16, y * 16],"t":1}))
            .collect::<Vec<_>>();
        project.layer_mut(key, 0).unwrap()["gridTiles"] = json!(wall);
        let mut samples = Vec::new();
        for _ in 0..5 {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.assets.embedded(&egui::Context::default());
            app.tile_rect = [2, 0, 1, 1];
            app.atlas_selection = Some((atlas, vec![2]));
            app.flips = 1;
            let start = Instant::now();
            app.fill([0, 0], false);
            samples.push(start.elapsed());
            assert!(app.dirty && !app.status_error);
            let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                .as_array().unwrap();
            assert_eq!(&tiles[..wall.len()], wall);
            assert_eq!(tiles.len(), 63 * 64);
            let mut positions = HashSet::new();
            for tile in &tiles[wall.len()..] {
                let x = tile["px"][0].as_i64().unwrap();
                let y = tile["px"][1].as_i64().unwrap();
                assert!((0..62 * 16).contains(&x) && (0..64 * 16).contains(&y));
                assert_eq!((x % 16, y % 16), (0, 0));
                assert!(positions.insert([x, y]));
                assert_eq!(tile["t"], 2);
                assert_eq!(tile["src"], json!([32, 0]));
                assert_eq!(tile["f"], 1);
            }
        }
        samples.sort_unstable();
        eprintln!("Large tile bucket paint: median {:?}; samples {samples:?}", samples[2]);
    }

    #[test]
    #[ignore = "manual tile edit performance comparison"]
    fn benchmark_large_tile_rectangle_erase_and_random_fill() {
        use serde_json::json;
        use std::time::Instant;

        let (mut template, key) = sparse_editor();
        let project = template.project.as_mut().unwrap();
        project.resize_level(key, 64 * 16, 64 * 16, 0, 0).unwrap();
        let atlas = project.json["defs"]["tilesets"][0]["uid"].as_i64().unwrap();
        let wall: Vec<_> = (0..64).map(|y| json!({"px":[62 * 16, y * 16],"t":1})).collect();
        let mut dense = wall.clone();
        dense.extend((0..64).flat_map(|y| (0..62).map(move |x| json!({"px":[x * 16, y * 16],"t":0}))));
        dense.push(json!({"px":[0, 0],"t":2}));
        let mut erase_times = Vec::new();
        let mut fill_times = Vec::new();
        for _ in 0..5 {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.project.as_mut().unwrap().layer_mut(key, 0).unwrap()["gridTiles"] = json!(dense);
            let start = Instant::now();
            app.paint_rectangle([0, 0, 62, 64], true);
            erase_times.push(start.elapsed());
            let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"].as_array().unwrap();
            assert_eq!(&tiles[..wall.len()], wall);
            assert_eq!(&tiles[wall.len()..], &[json!({"px":[0,0],"t":0})]);
            assert!(app.dirty && !app.status_error);

            app.set_project(project.clone());
            app.project.as_mut().unwrap().layer_mut(key, 0).unwrap()["gridTiles"] = json!(wall);
            app.assets.embedded(&egui::Context::default());
            app.tile_rect = [0, 0, 3, 1];
            app.atlas_selection = Some((atlas, vec![0, 2]));
            app.random_tiles = true;
            app.random_state = 42;
            let mut random = app.random_state;
            let expected: Vec<_> = (0..62 * 64).map(|_| [0, 2][random_index(&mut random, 2)]).collect();
            let start = Instant::now();
            app.fill([0, 0], false);
            fill_times.push(start.elapsed());
            assert_eq!(app.random_state, random);
            let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"].as_array().unwrap();
            assert_eq!(&tiles[..wall.len()], wall);
            assert!(tiles[wall.len()..].iter().map(|t| t["t"].as_i64().unwrap()).eq(expected));
            assert!(app.dirty && !app.status_error);
        }
        erase_times.sort_unstable();
        fill_times.sort_unstable();
        eprintln!("Large tile rectangle erase: median {:?}; samples {erase_times:?}", erase_times[2]);
        eprintln!("Large random tile fill: median {:?}; samples {fill_times:?}", fill_times[2]);
    }

    #[test]
    fn sparse_stamp_rectangle_fill_random_and_erase_preserve_holes() {
        let (mut app, key) = sparse_editor();
        app.flips = 1;
        app.paint_cells(&[[0, 0]], false);
        let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
            .as_array()
            .unwrap();
        assert_eq!(tiles.len(), 2);
        assert!(
            tiles
                .iter()
                .any(|t| t["px"] == serde_json::json!([0, 0]) && t["t"] == 2)
        );
        assert!(
            tiles
                .iter()
                .any(|t| t["px"] == serde_json::json!([32, 0]) && t["t"] == 0)
        );
        app.painted_cells.clear();
        app.paint_rectangle([0, 1, 6, 1], false);
        let tiles = app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
            .as_array()
            .unwrap();
        assert!(!tiles.iter().any(|t| t["px"] == serde_json::json!([16, 16])));
        assert!(
            tiles
                .iter()
                .any(|t| t["px"] == serde_json::json!([0, 16]) && t["t"] == 0 && t["f"] == 1)
        );
        app.painted_cells.clear();
        app.paint_cells(&[[0, 0]], true);
        assert!(
            !app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["px"][1] == 0)
        );
        app.painted_cells.clear();
        app.random_tiles = true;
        for x in 0..16 {
            app.paint_cells(&[[x, 2]], false);
        }
        assert!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["t"] != 1)
        );
        app.painted_cells.clear();
        app.random_tiles = false;
        app.fill([0, 3], false);
        assert!(
            !app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["px"] == serde_json::json!([16, 48]))
        );
    }
}
