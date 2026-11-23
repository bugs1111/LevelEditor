use super::*;

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

pub(super) fn random_index(state: &mut u64, count: usize) -> usize {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (state.wrapping_mul(0x2545f4914f6cdd1d) % count as u64) as usize
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
        for &cell in cells {
            if self.random_tiles {
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
            } else {
                let flips = if erase { 0 } else { self.flips };
                let stack = self.tile_stacking && (!erase || ids.len() == 1);
                for (dest, src) in tools::stamp_cells(cell, tile, scale, flips) {
                    if !ids.contains(&(src[0] + src[1] * columns)) {
                        continue;
                    }
                    self.paint_tile_at(dest, src, erase, stack);
                }
            }
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
        let pending = layer["layerDefUid"].as_i64().and_then(|layer_uid| {
            let layer_def = project.layer_def(layer_uid)?;
            if layer_def["useAsyncRender"] != true {
                return None;
            }
            let now = self.repaint.as_ref()?.input(|i| i.time);
            let old_tiles = self
                .async_paint
                .as_ref()
                .filter(|p| p.is_active_for(key, now))
                .and_then(|p| p.layers.get(&layer_uid))
                .is_none_or(|p| p.old_tiles.is_none())
                .then(|| canvas::values(&layer["gridTiles"]).to_vec());
            let color = if erase {
                Color32::RED
            } else {
                let path = asset
                    .as_deref()
                    .unwrap_or_else(|| Path::new("@ldtk/embedded-atlas"));
                let [r, g, b, _] = canvas::cached_tile_average(def, tile_id as usize)
                    .or_else(|| self.assets.average_tile_color(path, src, [grid, grid]))
                    .unwrap_or([136, 136, 136, 255]);
                Color32::from_rgb(r, g, b)
            };
            Some((
                layer_uid,
                number(layer, "__gridSize", 16.),
                now,
                old_tiles,
                color,
            ))
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
                    if let Some((layer_uid, grid, now, old_tiles, color)) = pending {
                        let preview = self.async_preview(key, now);
                        preview.hint_grid.get_or_insert(grid);
                        if let Some(old_tiles) = old_tiles {
                            preview.layers.entry(layer_uid).or_default().old_tiles =
                                Some(old_tiles);
                        }
                        preview.hints.insert(cell, color);
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
                for cell in cells() {
                    self.paint_tile_at(
                        cell,
                        [self.tile_rect[0], self.tile_rect[1]],
                        true,
                        self.tile_stacking,
                    );
                }
            } else if self.random_tiles {
                self.paint_cells(&cells().collect::<Vec<_>>(), false);
            } else {
                // Preserve the stamp phase of the raw rectangle at the clipped boundary.
                for (dest, src) in tools::rectangle_tiles(
                    [rect[0], rect[1]],
                    [left, top, right, bottom],
                    self.tile_rect,
                    scale,
                ) {
                    if !ids.contains(&(src[0] + src[1] * columns)) {
                        continue;
                    }
                    self.paint_tile_at(dest, src, false, self.tile_stacking);
                }
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
            for cell in cells {
                self.paint_tile_at(cell, [0, 0], true, false);
            }
        } else if kind == "IntGrid" || self.random_tiles {
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
            for cell in cells {
                let dx = cell[0] - left;
                let dy = cell[1] - top;
                if dx.rem_euclid(scale) != 0 || dy.rem_euclid(scale) != 0 {
                    continue;
                }
                let src = [
                    self.tile_rect[0] + dx.div_euclid(scale).rem_euclid(self.tile_rect[2]),
                    self.tile_rect[1] + dy.div_euclid(scale).rem_euclid(self.tile_rect[3]),
                ];
                if ids.contains(&(src[0] + src[1] * columns)) {
                    self.paint_tile_at(cell, src, erase, self.tile_stacking);
                }
            }
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
