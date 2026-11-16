use super::*;

pub(super) fn random_index(state: &mut u64, count: usize) -> usize {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (state.wrapping_mul(0x2545f4914f6cdd1d) % count as u64) as usize
}

impl EditorApp {
    pub(super) fn paint_cells(&mut self, cells: &[[i32; 2]], erase: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(layer) = self.project.as_ref().and_then(|p| p.layer(key, self.layer)) else {
            return;
        };
        if layer["__type"] == "IntGrid" {
            let changes = cells
                .iter()
                .map(|&cell| (cell, if erase { 0 } else { self.int_value }))
                .collect::<Vec<_>>();
            let width = number(layer, "__cWid", 0.) as i32;
            let height = number(layer, "__cHei", 0.) as i32;
            for &(cell, value) in &changes {
                if cell[0] >= 0
                    && cell[1] >= 0
                    && cell[0] < width
                    && cell[1] < height
                    && layer["intGridCsv"][(cell[0] + cell[1] * width) as usize].as_i64()
                        != Some(i64::from(value))
                {
                    self.history.mark_grid_change(
                        self.project.as_ref().unwrap(),
                        key,
                        self.layer,
                        cell[0],
                        cell[1],
                    );
                }
            }
            let result = self
                .project
                .as_mut()
                .unwrap()
                .paint_int_grid_cells(key, self.layer, &changes);
            match result {
                Ok(count) => self.dirty |= count > 0,
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
        let stacking = if erase {
            stack
        } else {
            let opacity = if let Some(relative) = def["relPath"].as_str() {
                self.assets.tile_is_opaque(
                    &canvas::asset_path(project, relative),
                    src,
                    [grid, grid],
                )
            } else {
                self.assets.embedded_tile_is_opaque(src, [grid, grid])
            };
            let Some(opaque) = opacity else {
                self.status = "Waiting for tileset texture…".into();
                return;
            };
            stack && !opaque
        };
        match self.project.as_mut().unwrap().paint_tile_stacked(
            key,
            self.layer,
            cell[0],
            cell[1],
            tile[0] + tile[1] * columns,
            src,
            self.flips,
            erase,
            stacking,
        ) {
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
        if layer["__type"] == "IntGrid" {
            let cells = (rect[1]..rect[1] + rect[3])
                .flat_map(|y| (rect[0]..rect[0] + rect[2]).map(move |x| [x, y]))
                .collect::<Vec<_>>();
            self.paint_cells(&cells, erase);
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
                for y in rect[1]..rect[1] + rect[3] {
                    for x in rect[0]..rect[0] + rect[2] {
                        self.paint_tile_at(
                            [x, y],
                            [self.tile_rect[0], self.tile_rect[1]],
                            true,
                            self.tile_stacking,
                        );
                    }
                }
            } else if self.random_tiles {
                let cells = (rect[1]..rect[1] + rect[3])
                    .flat_map(|y| (rect[0]..rect[0] + rect[2]).map(move |x| [x, y]))
                    .collect::<Vec<_>>();
                self.paint_cells(&cells, false);
            } else {
                for (dest, src) in tools::rectangle_tiles(rect, self.tile_rect, scale) {
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
