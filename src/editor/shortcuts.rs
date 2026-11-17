use super::*;
use serde_json::json;

impl EditorApp {
    pub(super) fn normalize_entity_palette(&mut self) {
        let Some(project) = &self.project else { return };
        let Some(def) = self
            .selected
            .and_then(|key| project.layer(key, self.layer))
            .and_then(|layer| layer["layerDefUid"].as_i64())
            .and_then(|uid| project.layer_def(uid))
        else {
            return;
        };
        let allowed = project.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entity| library::entity_allowed(entity, def))
            .filter_map(|entity| entity["uid"].as_i64())
            .collect::<Vec<_>>();
        if !self.entity_uid.is_some_and(|uid| allowed.contains(&uid)) {
            self.entity_uid = allowed.first().copied();
        }
    }

    pub(super) fn set_atlas_selection(&mut self, uid: i64, ids: Vec<i32>) -> bool {
        let Some(atlas) = self.project.as_ref().and_then(|p| p.tileset(uid)) else {
            return false;
        };
        let columns = number(atlas, "__cWid", 0.0) as i32;
        let rows = number(atlas, "__cHei", 0.0) as i32;
        let mut selected = Vec::new();
        for id in ids {
            if id >= 0 && id < columns * rows && !selected.contains(&id) {
                selected.push(id);
            }
        }
        let Some(&first) = selected.first() else {
            return false;
        };
        let left = selected.iter().map(|id| id % columns).min().unwrap();
        let right = selected.iter().map(|id| id % columns).max().unwrap();
        let top = selected.iter().map(|id| id / columns).min().unwrap();
        let bottom = selected.iter().map(|id| id / columns).max().unwrap();
        self.tile_id = first;
        self.tile_rect = [left, top, right - left + 1, bottom - top + 1];
        self.atlas_selection = Some((uid, selected));
        self.flips = 0;
        true
    }

    pub(super) fn navigate_palette(&mut self, dx: i32, dy: i32) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|k| project.layer(k, self.layer)) else {
            return;
        };
        let Some(def) = layer["layerDefUid"]
            .as_i64()
            .and_then(|uid| project.layer_def(uid))
        else {
            return;
        };
        if matches!(text(layer, "__type", ""), "Tiles" | "AutoLayer") {
            let Some(atlas) = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| project.tileset(uid))
            else {
                return;
            };
            let uid = atlas["uid"].as_i64().unwrap();
            let columns = number(atlas, "__cWid", 1.0).max(1.0) as i32;
            let rows = number(atlas, "__cHei", 1.0).max(1.0) as i32;
            let mut ids = self.selected_tile_ids_for(uid);
            if ids.is_empty() {
                ids.push(0);
            }
            let min = *ids.iter().min().unwrap();
            let max = *ids.iter().max().unwrap();
            // The source picker uses the coordinates of the first/last IDs here.
            let (left, right, top, bottom) =
                (min % columns, max % columns, min / columns, max / columns);
            let adjacent_x = if dx < 0 {
                left - 1
            } else if dx > 0 {
                right + 1
            } else {
                left
            };
            let adjacent_y = if dy < 0 {
                top - 1
            } else if dy > 0 {
                bottom + 1
            } else {
                top
            };
            let saved_for = |id| {
                atlas["savedSelections"].as_array().and_then(|selections| {
                    selections.iter().find(|s| {
                        s["ids"]
                            .as_array()
                            .is_some_and(|ids| ids.contains(&json!(id)))
                    })
                })
            };
            if let Some(saved) = saved_for(adjacent_x + adjacent_y * columns) {
                let ids = saved["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_i64)
                    .map(|id| id as i32)
                    .collect();
                self.random_tiles = saved["mode"] == "Random";
                self.set_atlas_selection(uid, ids);
                return;
            }
            if saved_for(min).is_some() {
                ids = vec![if dx > 0 {
                    right + top * columns
                } else if dy > 0 {
                    left + bottom * columns
                } else {
                    min
                }];
            }
            let (mut dx, mut dy) = (dx, dy);
            for id in &ids {
                let (x, y) = (id % columns, id / columns);
                if dx != 0 {
                    if x + dx < 0 {
                        dx = columns - right - 1;
                        break;
                    }
                    if x + dx >= columns {
                        dx = -columns + right - left + 1;
                        break;
                    }
                }
                if dy != 0 {
                    if y + dy < 0 {
                        dy = rows - bottom - 1;
                        break;
                    }
                    if y + dy >= rows {
                        dy = -rows + bottom - top + 1;
                        break;
                    }
                }
            }
            self.set_atlas_selection(
                uid,
                ids.into_iter().map(|id| id + dx + dy * columns).collect(),
            );
            return;
        }
        let mut groups: Vec<Vec<i64>> = Vec::new();
        let current = if layer["__type"] == "IntGrid" {
            let values = def["intGridValues"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for uid in std::iter::once(0).chain(
                def["intGridValuesGroups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v["uid"].as_i64()),
            ) {
                let rows = values
                    .iter()
                    .filter(|v| v["groupUid"].as_i64().unwrap_or(0) == uid)
                    .filter_map(|v| v["value"].as_i64())
                    .collect::<Vec<_>>();
                if !rows.is_empty() {
                    groups.push(rows);
                }
            }
            Some(self.int_value as i64)
        } else if layer["__type"] == "Entities" {
            let definitions = project.json["defs"]["entities"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let tags = definitions
                .iter()
                .flat_map(|d| {
                    d["tags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                })
                .collect::<std::collections::BTreeSet<_>>();
            for tag in std::iter::once("").chain(tags) {
                let rows = definitions
                    .iter()
                    .filter(|d| library::entity_allowed(d, def))
                    .filter(|d| {
                        if tag.is_empty() {
                            d["tags"].as_array().is_none_or(Vec::is_empty)
                        } else {
                            d["tags"]
                                .as_array()
                                .is_some_and(|a| a.iter().any(|t| t == tag))
                        }
                    })
                    .filter_map(|d| d["uid"].as_i64())
                    .collect::<Vec<_>>();
                if !rows.is_empty() {
                    groups.push(rows);
                }
            }
            self.entity_uid
        } else {
            return;
        };
        let Some((group, row)) = groups.iter().enumerate().find_map(|(g, rows)| {
            rows.iter()
                .position(|v| Some(*v) == current)
                .map(|r| (g, r))
        }) else {
            return;
        };
        let next = if dy != 0 {
            let flat = groups.iter().flatten().copied().collect::<Vec<_>>();
            let index =
                groups[..group].iter().map(Vec::len).sum::<usize>() as i32 + row as i32 + dy;
            if index < 0 {
                None
            } else {
                flat.get(index as usize).copied()
            }
        } else if dx < 0 && row > 0 {
            groups[group].first().copied()
        } else if dx > 0 && group == groups.len() - 1 {
            groups[group].last().copied()
        } else {
            let index = group as i32 + dx;
            if index < 0 {
                None
            } else {
                groups.get(index as usize).and_then(|g| g.first()).copied()
            }
        };
        if layer["__type"] == "IntGrid" {
            if let Some(v) = next {
                self.int_value = v as i32;
            }
        } else if next.is_some() {
            self.entity_uid = next;
        }
    }
    pub(super) fn switch_layer(&mut self, index: usize) {
        let Some(layer) = self
            .selected
            .and_then(|k| self.project.as_ref()?.layer(k, index))
        else {
            return;
        };
        if self.layer == index {
            return;
        }
        self.layer_notification = Some((
            text(layer, "__identifier", "").to_owned(),
            text(layer, "__type", "Tiles").to_owned(),
            Instant::now(),
        ));
        self.finish_gesture();
        self.finish_stroke();
        self.layer = index;
        self.entity_index = None;
        self.selection = None;
        self.selection_group = None;
        self.stamp = None;
        self.normalize_palette();
    }

    pub(super) fn undo_action(&mut self, redo: bool) {
        self.chain_entity = None;
        self.finish_gesture();
        self.finish_stroke();
        if self.world || self.panel.is_some_and(|p| p != Panel::Entity) {
            return;
        }
        let mut feedback = None;
        if let (Some(project), Some(key)) = (&mut self.project, self.selected) {
            let bounds = self.history.level_change_bounds(project, key, redo);
            let before = project
                .level(key)
                .map(|l| [number(l, "worldX", 0.0), number(l, "worldY", 0.0)]);
            let changed = if redo {
                self.history.redo_level(project, key)
            } else {
                self.history.undo_level(project, key)
            };
            self.dirty |= changed;
            if changed {
                feedback = bounds.map(|bounds| (key, bounds));
            }
            if changed
                && !self.world
                && let (Some(before), Some(after)) = (before, project.level(key))
            {
                self.viewport.center[0] += before[0] - number(after, "worldX", 0.0);
                self.viewport.center[1] += before[1] - number(after, "worldY", 0.0);
            }
        }
        if let Some((key, bounds)) = feedback {
            self.bleep_history(key, bounds, redo);
        }
        self.entity_index = None;
        self.selection = None;
        self.selection_group = None;
        self.stamp = None;
        self.normalize_palette();
    }

    fn fullscreen(&self, ctx: &egui::Context) {
        let active = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!active));
    }

    fn change_depth(&mut self, delta: i64, move_level: bool) {
        self.finish_gesture();
        self.finish_stroke();
        self.world_depth += delta;
        if move_level && let Some(key) = self.selected {
            let depth = self.world_depth;
            self.edit("Level world depth changed", |p| {
                let l = p.level(key).unwrap();
                p.move_level(
                    key,
                    number(l, "worldX", 0.0) as i32,
                    number(l, "worldY", 0.0) as i32,
                    depth as i32,
                )
            });
        }
    }

    pub(super) fn tile_selection_memory(&mut self, load: bool) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|k| project.layer(k, self.layer)) else {
            return;
        };
        let Some(uid) = layer["__tilesetDefUid"].as_i64() else {
            return;
        };
        let Some(def) = project.tileset(uid) else {
            return;
        };
        if load {
            if let Some(saved) = def["savedSelections"].as_array().and_then(|a| {
                a.iter().find(|v| {
                    v["ids"]
                        .as_array()
                        .is_some_and(|ids| ids.contains(&json!(self.tile_id)))
                })
            }) {
                let ids = saved["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_i64)
                    .map(|v| v as i32)
                    .collect::<Vec<_>>();
                let current = self.selected_tile_ids_for(uid);
                if current.len() != ids.len() || current.iter().any(|id| !ids.contains(id)) {
                    self.random_tiles = saved["mode"] == "Random";
                    // Keyboard recall selects the tool value directly; unlike
                    // picker navigation, it preserves the brush transform.
                    let flips = self.flips;
                    self.set_atlas_selection(uid, ids);
                    self.flips = flips;
                }
            }
        } else {
            let ids = self
                .selected_tile_ids_for(uid)
                .into_iter()
                .map(|id| json!(id))
                .collect::<Vec<_>>();
            let mut selections = def["savedSelections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            selections.retain(|s| {
                !s["ids"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|id| ids.contains(id)))
            });
            if ids.len() > 1 {
                selections
                    .push(json!({"ids":ids,"mode":if self.random_tiles{"Random"}else{"Stamp"}}));
            }
            self.edit("Tile selection saved", |p| {
                p.update_tileset_definition(uid, &json!({"savedSelections":selections}))
            });
        }
    }

    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        if update_dialog::is_open(ctx) {
            return;
        }
        if world_form_dialog::is_open(ctx) {
            return;
        }
        if help::changelog_is_open(ctx) {
            return;
        }
        if text_editor::is_open(ctx) && ctx.input(|i| i.key_pressed(Key::Escape)) {
            text_editor::request_validate(ctx);
            return;
        }
        if palette_popout::consume_escape(ctx) {
            return;
        }
        if self.is_busy()
            || self.pending.is_some()
            || self.suggest_backups
            || self.command_open
            || !self.resource_missing.is_empty()
            || self.crash.is_some()
            || self.clicktrap
        {
            return;
        }
        if self.definition_clipboard_shortcuts(ctx) {
            return;
        }
        let focus = ctx.wants_keyboard_input();
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if !focus {
                match &event {
                    egui::Event::Copy => {
                        self.copy_selection(ctx, false);
                        continue;
                    }
                    egui::Event::Cut => {
                        self.copy_selection(ctx, true);
                        continue;
                    }
                    egui::Event::Paste(contents) => {
                        if let Ok(value) = serde_json::from_str::<Value>(contents)
                            && value["gridSize"].is_number()
                        {
                            self.clipboard = Some(value);
                        }
                        self.stamp = self.clipboard.clone();
                        continue;
                    }
                    _ => {}
                }
            }
            let egui::Event::Key {
                key,
                pressed,
                repeat: false,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if key == Key::Space && !focus && !modifiers.any() {
                if pressed {
                    self.space_pressed = Some(Instant::now());
                    self.space_dragged = false;
                } else if self
                    .space_pressed
                    .take()
                    .is_some_and(|t| t.elapsed().as_secs_f32() <= 0.2)
                    && !self.space_dragged
                {
                    self.fit = true;
                }
                continue;
            }
            if !pressed {
                continue;
            }
            if key == Key::Escape {
                self.chain_entity = None;
                if self.world_context.picking {
                    self.cancel_world_context();
                    continue;
                }
                if self.field_pick.is_some() {
                    self.cancel_field_pick();
                    continue;
                }
                if focus {
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                } else {
                    self.cancel_gesture();
                    self.selection = None;
                    self.selection_group = None;
                    self.stamp = None;
                    self.command_palette = false;
                    self.show_panel(None);
                    self.entity_index = None;
                }
                continue;
            }
            if key == Key::F12 {
                self.finish_gesture();
                self.show_panel(Some(Panel::Settings));
                continue;
            }
            if (key == Key::F11 && !cfg!(target_os = "macos"))
                || (modifiers.command && key == Key::Enter)
                || (modifiers.command && modifiers.shift && key == Key::F)
                || (modifiers.command && modifiers.mac_cmd && modifiers.ctrl && key == Key::F)
            {
                self.fullscreen(ctx);
                continue;
            }
            if modifiers.command {
                match key {
                    Key::S => {
                        self.save(modifiers.shift);
                    }
                    Key::O => self.request(ctx, Action::Browse),
                    Key::N => self.request(ctx, Action::New),
                    Key::W => self.request(ctx, Action::Close),
                    Key::Q => self.request(ctx, Action::Quit),
                    Key::F | Key::K => {
                        self.command_palette = true;
                        self.search.clear();
                        self.search_focus = false;
                    }
                    Key::P if modifiers.shift => {
                        self.command_palette = true;
                        self.search.clear();
                        self.search_focus = false;
                    }
                    Key::M if cfg!(target_os = "macos") => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                    }
                    Key::H if cfg!(target_os = "macos") => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false))
                    }
                    Key::H => self.set_details(!self.details),
                    Key::R => {
                        self.finish_gesture();
                        self.run_manual_command();
                    }
                    Key::Z if !focus && !self.panel.is_some_and(|p| p != Panel::Entity) => {
                        self.undo_action(modifiers.shift)
                    }
                    Key::Y if !focus && !self.panel.is_some_and(|p| p != Panel::Entity) => {
                        self.undo_action(true)
                    }
                    Key::A if !focus => self.select_all(),
                    Key::C if !focus => self.copy_selection(ctx, false),
                    Key::X if !focus => self.copy_selection(ctx, true),
                    Key::V if !focus => self.stamp = self.clipboard.clone(),
                    Key::PageUp if !focus => self.change_depth(1, true),
                    Key::PageDown if !focus => self.change_depth(-1, true),
                    _ => {}
                }
                continue;
            }
            if focus || self.command_palette {
                continue;
            }
            if let Some(index) = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
                Key::Num0,
            ]
            .iter()
            .position(|&k| k == key)
            {
                self.switch_layer(index);
                continue;
            }
            if let Some(index) = [
                Key::F1,
                Key::F2,
                Key::F3,
                Key::F4,
                Key::F5,
                Key::F6,
                Key::F7,
                Key::F8,
                Key::F9,
                Key::F10,
            ]
            .iter()
            .position(|&k| k == key)
            {
                let filter = self.settings.value["layerFilter"].as_str().unwrap_or("");
                let visible = self.project.as_ref().and_then(|project| {
                    let level = self.selected.and_then(|key| project.level(key))?;
                    level["layerInstances"]
                        .as_array()?
                        .iter()
                        .enumerate()
                        .filter(|(_, layer)| {
                            layer["layerDefUid"]
                                .as_i64()
                                .and_then(|uid| project.layer_def(uid))
                                .is_some_and(|def| {
                                    def["hideInList"] != true
                                        && (filter.is_empty()
                                            || def["uiFilterTags"].as_array().is_some_and(|tags| {
                                                tags.iter().any(|tag| tag == filter)
                                            }))
                                })
                        })
                        .nth(index)
                        .map(|(i, _)| i)
                });
                if let Some(index) = visible {
                    self.switch_layer(index);
                }
                continue;
            }
            if !focus {
                let panel = match key {
                    Key::P => Some(Panel::Project),
                    Key::C => Some(Panel::Level),
                    Key::L if !modifiers.shift => Some(Panel::Layers),
                    Key::E if !modifiers.shift => Some(Panel::Entities),
                    Key::U => Some(Panel::Enums),
                    Key::T if !modifiers.shift => Some(Panel::Tilesets),
                    Key::H if !modifiers.shift => Some(Panel::Help),
                    Key::Backtick => Some(Panel::World),
                    Key::W
                        if modifiers.shift || self.settings.value["navigationKeys"] != "Wasd" =>
                    {
                        Some(Panel::World)
                    }
                    _ => None,
                };
                if let Some(panel) = panel {
                    self.toggle_toolbar_panel(panel);
                    continue;
                }
            }
            if self
                .panel
                .is_some_and(|p| !matches!(p, Panel::Entity | Panel::World))
            {
                continue;
            }
            let navigation = self.settings.value["navigationKeys"]
                .as_str()
                .unwrap_or("Wasd");
            let (up, down, left, right) = match navigation {
                "Arrows" => (
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::ArrowLeft,
                    Key::ArrowRight,
                ),
                "Zqsd" => (Key::Z, Key::S, Key::Q, Key::D),
                _ => (Key::W, Key::S, Key::A, Key::D),
            };
            if !modifiers.any() && [up, down, left, right].contains(&key) {
                self.navigate_palette(
                    i32::from(key == right) - i32::from(key == left),
                    i32::from(key == down) - i32::from(key == up),
                );
                continue;
            }
            match key {
                Key::G => self.grid = !self.grid,
                Key::Tab => self.zen = !self.zen,
                Key::R if modifiers.shift => self.render_auto = !self.render_auto,
                Key::R => self.random_tiles = !self.random_tiles,
                Key::T if modifiers.shift => self.tile_stacking = !self.tile_stacking,
                Key::E if modifiers.shift => {
                    self.set_empty_selection(!self.settings.bool("emptySpaceSelection"));
                }
                Key::A if modifiers.shift || self.settings.value["navigationKeys"] != "Wasd" => {
                    self.set_single_layer_mode(!self.single_layer)
                }
                Key::H if modifiers.shift && cfg!(target_os = "macos") => {
                    self.set_details(!self.details)
                }
                Key::X => self.flips ^= 1,
                Key::Y => self.flips ^= 2,
                Key::H if modifiers.shift => self.flips ^= 1,
                Key::V if modifiers.shift => self.flips ^= 2,
                Key::S if modifiers.shift || self.settings.value["navigationKeys"] == "Arrows" => {
                    self.tile_selection_memory(false)
                }
                Key::L if modifiers.shift => self.tile_selection_memory(true),
                Key::PageUp => self.change_depth(1, modifiers.shift),
                Key::PageDown => self.change_depth(-1, modifiers.shift),
                Key::Delete | Key::Backspace => self.delete_selection(),
                Key::F => self.fit = true,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn atlas_app() -> (EditorApp, i64) {
        let mut p = Project::new();
        let layer = p.add_layer("Tiles", "Tiles", 16).unwrap();
        let atlas = p
            .add_tileset(std::path::Path::new("unused.png"), 16, 64, 32)
            .unwrap();
        p.assign_tileset(layer, atlas).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        (app, atlas)
    }
    #[test]
    fn atlas_selection_filters_invalid_ids_and_keeps_sparse_bounds() {
        let (mut app, atlas) = atlas_app();
        app.flips = 3;
        assert!(app.set_atlas_selection(atlas, vec![7, -1, 2, 7, 8]));
        assert_eq!(app.atlas_selection, Some((atlas, vec![7, 2])));
        assert_eq!(app.tile_id, 7);
        assert_eq!(app.tile_rect, [2, 0, 2, 2]);
        assert_eq!(app.flips, 0);
        assert!(!app.set_atlas_selection(atlas, vec![-1, 8]));
        assert_eq!(app.atlas_selection, Some((atlas, vec![7, 2])));
    }
    #[test]
    fn entity_palette_respects_required_and_excluded_layer_tags() {
        let mut p = Project::new();
        let layer = p.add_layer("Actors", "Entities", 16).unwrap();
        let untagged = p.add_entity_def("Untagged", 16, 16).unwrap();
        let allowed = p.add_entity_def("Allowed", 16, 16).unwrap();
        let excluded = p.add_entity_def("Excluded", 16, 16).unwrap();
        p.update_entity_definition(allowed, &json!({"tags":["Actor"]}))
            .unwrap();
        p.update_entity_definition(excluded, &json!({"tags":["Actor","Hidden"]}))
            .unwrap();
        let def = p.json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["uid"] == layer)
            .unwrap();
        def["requiredTags"] = json!(["Actor"]);
        def["excludedTags"] = json!(["Hidden"]);
        let mut app = EditorApp::empty();
        app.set_project(p);
        for current in [untagged, excluded, allowed] {
            app.entity_uid = Some(current);
            app.normalize_palette();
            assert_eq!(app.entity_uid, Some(allowed));
        }
        app.project
            .as_mut()
            .unwrap()
            .update_entity_definition(allowed, &json!({"tags":[]}))
            .unwrap();
        app.normalize_palette();
        assert_eq!(app.entity_uid, None);
    }
    #[test]
    fn saved_selection_shortcut_preserves_sparse_ids_and_existing_flips() {
        let (mut app, atlas) = atlas_app();
        app.tile_rect = [0, 0, 3, 1];
        app.atlas_selection = Some((atlas, vec![0, 2]));
        app.tile_selection_memory(false);
        assert_eq!(
            app.project.as_ref().unwrap().tileset(atlas).unwrap()["savedSelections"][0]["ids"],
            json!([0, 2])
        );
        app.atlas_selection = None;
        app.tile_id = 2;
        app.tile_rect = [2, 0, 1, 1];
        app.flips = 3;
        app.tile_selection_memory(true);
        assert_eq!(app.atlas_selection, Some((atlas, vec![0, 2])));
        assert_eq!(app.tile_rect, [0, 0, 3, 1]);
        assert_eq!(app.flips, 3);
        app.random_tiles = true;
        app.tile_selection_memory(true);
        assert!(app.random_tiles);
    }
    #[test]
    fn atlas_navigation_preserves_sparse_mask_and_wraps_as_a_group() {
        let (mut app, atlas) = atlas_app();
        app.tile_rect = [0, 0, 3, 1];
        app.atlas_selection = Some((atlas, vec![0, 2]));
        app.flips = 1;
        app.navigate_palette(0, 1);
        assert_eq!(app.atlas_selection, Some((atlas, vec![4, 6])));
        assert_eq!(app.flips, 0);
        app.navigate_palette(0, 1);
        assert_eq!(app.atlas_selection, Some((atlas, vec![0, 2])));
        app.project
            .as_mut()
            .unwrap()
            .update_tileset_definition(
                atlas,
                &json!({"savedSelections":[{"ids":[3,7],"mode":"Random"}]}),
            )
            .unwrap();
        app.navigate_palette(1, 0);
        assert_eq!(app.atlas_selection, Some((atlas, vec![3, 7])));
        assert!(app.random_tiles);
    }
    #[test]
    fn function_layer_keys_follow_filtered_list_but_number_keys_use_actual_order() {
        let mut p = Project::new();
        p.add_layer("Visible", "IntGrid", 16).unwrap();
        p.add_layer("Filtered", "IntGrid", 16).unwrap();
        p.add_layer("Hidden", "IntGrid", 16).unwrap();
        for d in p.json["defs"]["layers"].as_array_mut().unwrap() {
            d["hideInList"] = json!(d["identifier"] == "Hidden");
            d["uiFilterTags"] = if d["identifier"] == "Visible" {
                json!(["Map"])
            } else {
                json!([])
            };
        }
        let key = p.level_keys()[0];
        let visible = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["__identifier"] == "Visible")
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.settings.value["layerFilter"] = json!("Map");
        let ctx = egui::Context::default();
        for (key, expected) in [(Key::F1, visible), (Key::Num1, 0)] {
            let raw = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| app.shortcuts(ctx));
            assert_eq!(app.layer, expected);
        }
    }
}
