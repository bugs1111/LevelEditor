use super::*;

pub(super) fn single_layer_shortcut_needs_shift(navigation: &str) -> bool {
    !matches!(navigation, "Arrows" | "Zqsd")
}

pub(super) fn save_tile_shortcut_needs_shift(navigation: &str) -> bool {
    navigation != "Arrows"
}
use serde_json::json;

fn fullscreen_key(platform: &str, key: Key, modifiers: egui::Modifiers) -> bool {
    match (
        key,
        modifiers.alt,
        modifiers.shift,
        modifiers.ctrl,
        modifiers.mac_cmd,
    ) {
        (Key::F, false, true, true, false) => platform != "macos",
        (Key::F, false, true, false, true)
        | (Key::F, false, false, true, true)
        | (Key::Enter, false, false, false, true) => platform == "macos",
        (Key::F11, false, false, false, false) | (Key::Enter, true, false, false, false) => {
            platform == "windows"
        }
        _ => false,
    }
}

fn project_search_key(key: Key, modifiers: egui::Modifiers) -> bool {
    !(modifiers.alt || cfg!(target_os = "macos") && modifiers.ctrl)
        && (key != Key::F || !modifiers.shift)
}

fn mac_window_key(platform: &str, key: Key, modifiers: egui::Modifiers) -> bool {
    platform == "macos"
        && matches!(key, Key::H | Key::M)
        && modifiers.mac_cmd
        && !modifiers.ctrl
        && !modifiers.alt
        && !modifiers.shift
}

#[derive(Clone, Copy, Default)]
struct HeldZoomKeys {
    equals: bool,
    plus: bool,
    minus: bool,
}

fn track_zoom_keys(ctx: &egui::Context, search_open: bool) {
    let id = egui::Id::new("held-camera-zoom-keys");
    let mut keys = ctx
        .data(|data| data.get_temp::<HeldZoomKeys>(id))
        .unwrap_or_default();
    ctx.input(|input| {
        if search_open || !input.focused {
            keys = HeldZoomKeys::default();
            return;
        }
        for event in &input.events {
            match event {
                egui::Event::WindowFocused(_) => keys = HeldZoomKeys::default(),
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed,
                    ..
                } => {
                    // Shift can be released before the Equals key. Its logical
                    // key then changes from Plus to Equals on key release.
                    match physical_key.unwrap_or(*key) {
                        Key::Equals => keys.equals = *pressed,
                        Key::Plus => keys.plus = *pressed,
                        Key::Minus => keys.minus = *pressed,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    });
    ctx.data_mut(|data| data.insert_temp(id, keys));
}

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

    pub(super) fn navigate_palette(&mut self, ctx: &egui::Context, dx: i32, dy: i32) {
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
        if layer["__type"] == "Tiles" {
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
                if self.set_atlas_selection(uid, ids) {
                    palette_popout::request_focus(ctx, uid, false);
                }
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
            let direction = (dx, dy);
            let (mut dx, mut dy) = direction;
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
            if self.set_atlas_selection(
                uid,
                ids.into_iter().map(|id| id + dx + dy * columns).collect(),
            ) {
                palette_popout::request_focus(ctx, uid, direction != (dx, dy));
            }
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
        self.quick_notification = Some(notifications::layer_notification(layer));
        self.finish_gesture();
        self.finish_stroke();
        // Preserve a directly selected entity when its inspector closes.
        let group = self.selection_group.take().or_else(|| {
            let (project, key, entity) =
                (self.project.as_ref()?, self.selected?, self.entity_index?);
            project.layer(key, self.layer)?["entityInstances"].get(entity)?;
            self.element_selection_group(
                project,
                key,
                self.layer,
                [0, 0],
                selection_click::Element::Entity(entity),
            )
            .ok()
        });
        self.point_pick = None;
        self.layer = index;
        self.entity_index = None;
        if self.panel == Some(Panel::Entity) {
            self.panel = None;
        }
        self.selection = None;
        if let Some(group) = group {
            self.set_selection_group(group);
        }
        self.stamp = None;
        self.normalize_palette();
    }

    pub(super) fn undo_action(&mut self, redo: bool) {
        if !self.settle_deferred_auto_paint() {
            return;
        }
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
                self.point_pick = None;
                self.async_paint = None;
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

    fn fullscreen(&mut self, ctx: &egui::Context) {
        let active = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        if !active {
            self.report_canvas_notice(notifications::CanvasNotice::EnteredFullscreen);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!active));
    }

    pub(super) fn world_depth_bounds(&self) -> Option<(i64, i64)> {
        let project = self.project.as_ref()?;
        Some(
            project
                .level_keys_in_world(self.active_world)
                .filter_map(|key| project.level(key)?["worldDepth"].as_i64())
                .fold((0, 0), |(min, max), depth| (min.min(depth), max.max(depth))),
        )
    }

    pub(super) fn select_world_depth(&mut self, depth: i64) {
        if self.world_depth != depth {
            self.finish_gesture();
            self.finish_stroke();
            self.world_depth = depth;
            self.level_form_depth_override = None;
            self.world_hover = None;
        }
    }

    fn change_depth(&mut self, delta: i64, move_level: bool) {
        if self.project.is_none() {
            return;
        }
        if !self.world {
            self.show_panel(Some(Panel::World));
            return;
        }
        if move_level {
            self.move_selected_level_world_depth(delta);
            return;
        }
        if let Some((min, max)) = self.world_depth_bounds()
            && ((delta > 0 && self.world_depth < max) || (delta < 0 && self.world_depth > min))
        {
            self.select_world_depth(self.world_depth + delta);
        }
    }

    pub(super) fn tile_selection_memory(&mut self, ctx: &egui::Context, load: bool) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|k| project.layer(k, self.layer)) else {
            return;
        };
        if layer["__type"] != "Tiles" {
            return;
        }
        let Some(uid) = layer["__tilesetDefUid"].as_i64() else {
            return;
        };
        let Some(def) = project.tileset(uid) else {
            return;
        };
        let current = self.selected_tile_ids_for(uid);
        if load {
            if let Some(saved) = current
                .first()
                .and_then(|&id| library::saved_selection_for(def, id))
            {
                let ids = library::saved_selection_ids(saved);
                if current.len() != ids.len() || current.iter().any(|id| !ids.contains(id)) {
                    let random = saved["mode"] == "Random";
                    // Keyboard recall selects the tool value directly; unlike
                    // picker navigation, it preserves the brush transform.
                    let flips = self.flips;
                    if self.set_atlas_selection(uid, ids) {
                        self.random_tiles = random;
                        self.flips = flips;
                        palette_popout::request_focus(ctx, uid, false);
                        let count = self.selected_tile_ids_for(uid).len();
                        if count > 1 {
                            self.notify_quick(locale::format_ctx(
                                ctx,
                                "Loaded selection of {count} tiles",
                                &[("count", &count.to_string())],
                            ));
                        }
                    }
                }
            }
        } else {
            let ids = current.into_iter().map(|id| json!(id)).collect::<Vec<_>>();
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
            if self
                .edit("Tile selection saved", |p| {
                    p.update_tileset_definition(uid, &json!({"savedSelections":selections}))
                })
                .is_some()
            {
                self.report_saved_tile_selection();
            }
        }
    }

    fn flip_tile_brush(&mut self, ctx: &egui::Context, axis: i32) {
        self.flips ^= axis;
        let label = match (axis, self.flips & axis != 0) {
            (1, true) => "X-flip: ON",
            (1, false) => "X-flip: off",
            (_, true) => "Y-flip: ON",
            (_, false) => "Y-flip: off",
        };
        self.notify_quick(locale::tr_ctx(ctx, label).into());
    }

    fn change_entity_depth(&mut self, up: bool, extreme: bool) {
        if self.world || self.panel.is_some_and(|panel| panel != Panel::Entity) {
            return;
        }
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let Some(layers) = project
            .level(key)
            .and_then(|level| level["layerInstances"].as_array())
        else {
            return;
        };
        let (layer_index, index) = if let Some(group) = &self.selection_group {
            if group["elementCount"] != 1 {
                return;
            }
            let Some((entry, iid)) = group["layers"].as_array().and_then(|entries| {
                entries.iter().find_map(|entry| {
                    Some((
                        entry,
                        entry["clipboard"]["entityInstances"][0]["iid"].as_str()?,
                    ))
                })
            }) else {
                return;
            };
            let Some((layer_index, layer)) = layers
                .iter()
                .enumerate()
                .find(|(_, layer)| layer["layerDefUid"] == entry["layerDefUid"])
            else {
                return;
            };
            let Some(index) = layer["entityInstances"]
                .as_array()
                .and_then(|entities| entities.iter().position(|entity| entity["iid"] == iid))
            else {
                return;
            };
            (layer_index, index)
        } else if let Some(index) = self.entity_index {
            (self.layer, index)
        } else {
            return;
        };
        let Some(layer) = layers.get(layer_index) else {
            return;
        };
        if project
            .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
            .is_none_or(|def| def["entityDepthSorting"].as_str().unwrap_or("Manual") != "Manual")
        {
            return;
        }
        let total = layer["entityInstances"].as_array().map_or(0, Vec::len);
        if index >= total {
            return;
        }
        let target = if extreme {
            if up { total - 1 } else { 0 }
        } else if up {
            (index + 1).min(total - 1)
        } else {
            index.saturating_sub(1)
        };
        if target == index {
            return;
        }
        if self
            .edit_level_marked("Entity depth changed", key, |p, history| {
                let entity = &p.layer(key, layer_index).unwrap()["entityInstances"][index];
                history.mark_entity_change(p, key, layer_index, entity);
                let entities = p.layer_mut(key, layer_index).unwrap()["entityInstances"]
                    .as_array_mut()
                    .unwrap();
                let entity = entities.remove(index);
                entities.insert(target, entity);
                Ok(())
            })
            .is_some()
            && self.layer == layer_index
            && self.entity_index == Some(index)
        {
            self.entity_index = Some(target);
        }
    }

    pub(super) fn keyboard_zoom(&mut self, ctx: &egui::Context, rect: Rect) {
        let keys = ctx
            .data(|data| data.get_temp::<HeldZoomKeys>(egui::Id::new("held-camera-zoom-keys")))
            .unwrap_or_default();
        let zoom_in = keys.equals || keys.plus;
        if self.command_palette || self.selected.is_none() || !zoom_in && !keys.minus {
            return;
        }
        let focus = ctx.memory(|memory| memory.focused());
        if focus.is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some()) {
            return;
        }
        let Some(tmod) = self.camera_motion.frame_tmod(
            ctx.cumulative_frame_nr(),
            ctx.input(|input| input.unstable_dt),
        ) else {
            return;
        };
        let delta = 0.45 * tmod * 0.15;
        let deltas = [zoom_in.then_some(delta), keys.minus.then_some(-delta)]
            .into_iter()
            .flatten()
            .collect();
        let level_origin = if self.world
            && let (Some(project), Some(level)) = (&self.project, self.selected)
        {
            navigation::level_world_origin(project, level)
        } else {
            [0.; 2]
        };
        let pointer = self.viewport.level_point_page(
            rect,
            level_origin,
            self.viewport.center,
            ctx.pixels_per_point(),
        );
        self.zoom_from_input(ctx, rect, pointer, deltas);
        ctx.request_repaint();
    }

    fn canvas_display_shortcut(&mut self, key: Key, modifiers: egui::Modifiers) -> bool {
        if modifiers.alt || (cfg!(target_os = "macos") && modifiers.ctrl) {
            return false;
        }
        match (key, modifiers.command, modifiers.shift) {
            (Key::H, true, false) if !cfg!(target_os = "macos") => self.set_details(!self.details),
            (Key::H, false, true) if cfg!(target_os = "macos") => self.set_details(!self.details),
            (Key::G, false, false) => self.set_grid(!self.grid),
            (Key::R, false, true) => self.set_auto_layer_rendering(!self.render_auto, true),
            (Key::T, false, true) => self.set_tile_stacking(!self.tile_stacking),
            (Key::E, false, true) => {
                self.set_empty_selection(!self.settings.bool("emptySpaceSelection"));
            }
            (Key::A, false, shift)
                if shift
                    || !single_layer_shortcut_needs_shift(text(
                        &self.settings.value,
                        "navigationKeys",
                        "Wasd",
                    )) =>
            {
                self.set_single_layer_mode(!self.single_layer);
            }
            _ => return false,
        }
        true
    }

    fn open_project_search(&mut self) {
        if !self.command_palette {
            self.search_index = 0;
            self.search_show_all = false;
        }
        self.command_palette = true;
        self.search_focus = false;
    }

    fn dismiss_project_search(&mut self, ctx: &egui::Context, modifiers: egui::Modifiers) {
        self.command_palette = false;
        ctx.input_mut(|input| input.consume_key(modifiers, Key::Escape));
        ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }

    fn application_settings_shortcut(&mut self, key: Key, modifiers: egui::Modifiers) -> bool {
        if key != Key::F12 || modifiers.any() {
            return false;
        }
        if self.panel != Some(Panel::Settings) {
            self.show_panel(Some(Panel::Settings));
        }
        true
    }

    fn toolbar_panel_shortcut(&mut self, key: Key, modifiers: egui::Modifiers) -> bool {
        if modifiers.alt || modifiers.command || modifiers.ctrl {
            return false;
        }
        let panel = match (key, modifiers.shift) {
            (Key::P, false) => Panel::Project,
            (Key::C, false) => Panel::Level,
            (Key::L, false) => Panel::Layers,
            (Key::E, false) => Panel::Entities,
            (Key::U, false) => Panel::Enums,
            (Key::T, false) => Panel::Tilesets,
            (Key::H, false) => Panel::Help,
            (Key::Backtick, false) | (Key::W, true) => Panel::World,
            (Key::W, false)
                if matches!(
                    self.settings.value["navigationKeys"].as_str(),
                    Some("Arrows" | "Zqsd")
                ) =>
            {
                Panel::World
            }
            _ => return false,
        };
        self.toggle_toolbar_panel(panel);
        true
    }

    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let window_key = ctx.input_mut(|input| {
            let pressed = input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if fullscreen_key(std::env::consts::OS, *key, *modifiers)
                    || mac_window_key(std::env::consts::OS, *key, *modifiers) =>
                {
                    Some((*key, *modifiers))
                }
                _ => None,
            });
            pressed.and_then(|(key, modifiers)| {
                (input.focused && input.consume_key(modifiers, key)).then_some(key)
            })
        });
        match window_key {
            #[cfg(target_os = "macos")]
            Some(Key::H) => {
                if let Some(main_thread) = objc2::MainThreadMarker::new() {
                    objc2_app_kit::NSApplication::sharedApplication(main_thread).hide(None);
                }
            }
            #[cfg(target_os = "macos")]
            Some(Key::M) => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            Some(_) => self.fullscreen(ctx),
            None => {}
        }
        let world_context_open = self.world_context.menu_open();
        let selector_open = ui_fields::enum_picker::is_open(ctx)
            || self.panel == Some(Panel::World) && Self::world_selector_open(ctx)
            || self.panel == Some(Panel::Layers) && egui::Popup::is_any_open(ctx)
            || self.layer_context_menu.is_some()
            || egui::Popup::is_id_open(ctx, Self::layer_filter_popup_id())
            || palette_popout::tileset_selector_open(ctx);
        track_zoom_keys(
            ctx,
            self.command_palette || selector_open || world_context_open,
        );
        if selector_open {
            return;
        }
        if update_dialog::is_open(ctx) {
            return;
        }
        if world_form_dialog::is_open(ctx)
            || Self::definition_removal_open(ctx)
            || Self::backup_restore_open(ctx)
            || Self::home_backup_delete_open(ctx)
        {
            return;
        }
        if help::changelog_is_open(ctx)
            || self.panel == Some(Panel::Project)
                && (Self::tiled_export_disclaimer_open(ctx)
                    || ctx.memory(|mem| mem.top_modal_layer().is_some()))
        {
            return;
        }
        if self.enum_sync_is_open()
            || panels::enum_value_removal_open(ctx)
            || self.layer_bake.is_some()
            || self.int_grid_removal.is_some()
            || self.rule_draft.is_some()
            || self.panel == Some(Panel::Rules) && ctx.memory(|m| m.top_modal_layer().is_some())
        {
            return;
        }
        if self.panel == Some(Panel::Layers) && ctx.memory(|mem| mem.top_modal_layer().is_some()) {
            return;
        }
        if text_editor::is_open(ctx) && ctx.input(|i| i.key_pressed(Key::Escape)) {
            text_editor::request_validate(ctx);
            return;
        }
        if self.panel == Some(Panel::Tilesets)
            && !text_editor::is_open(ctx)
            && ctx.memory(|m| m.top_modal_layer().is_some())
        {
            return;
        }
        if palette_popout::consume_escape(ctx) {
            return;
        }
        if self.is_busy()
            || self.pending.is_some()
            || self.suggest_backups
            || self.command_open
            || self.crash.is_some()
            || self.clicktrap
        {
            return;
        }
        let missing_resources = !self.resource_missing.is_empty();
        if world_context_open || missing_resources {
            let allow_search = world_context_open && !missing_resources && self.project.is_some();
            let allow_canvas_shortcuts = world_context_open
                && !missing_resources
                && !self.command_palette
                && !ctx.wants_keyboard_input();
            for event in ctx.input(|input| input.events.clone()) {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } = event
                else {
                    continue;
                };
                if key == Key::Escape && world_context_open && self.command_palette {
                    self.dismiss_project_search(ctx, modifiers);
                    break;
                }
                if !missing_resources && self.application_settings_shortcut(key, modifiers) {
                    break;
                }
                if modifiers.command
                    && !modifiers.alt
                    && !(cfg!(target_os = "macos") && modifiers.ctrl)
                {
                    match key {
                        Key::S if self.external_changes.is_none() => {
                            self.save(modifiers.shift);
                            break;
                        }
                        Key::W | Key::Q
                            if world_context_open && !missing_resources && !modifiers.shift =>
                        {
                            let action = if key == Key::W {
                                Action::Close
                            } else {
                                Action::Quit
                            };
                            self.request(ctx, action);
                            break;
                        }
                        Key::F | Key::K | Key::P
                            if allow_search && project_search_key(key, modifiers) =>
                        {
                            self.open_project_search();
                            break;
                        }
                        _ => {}
                    }
                }
                if allow_canvas_shortcuts {
                    if self.toolbar_panel_shortcut(key, modifiers) {
                        break;
                    }
                    self.canvas_display_shortcut(key, modifiers);
                }
            }
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
                {
                    self.space_fit_pending = true;
                }
                continue;
            }
            if !pressed {
                continue;
            }
            if key == Key::Escape {
                if matches!(self.panel, Some(Panel::Help | Panel::Project))
                    && egui::Popup::is_any_open(ctx)
                {
                    egui::Popup::close_all(ctx);
                    ctx.input_mut(|input| input.consume_key(modifiers, Key::Escape));
                    continue;
                }
                if self.command_palette {
                    self.dismiss_project_search(ctx, modifiers);
                    continue;
                }
                self.chain_entity = None;
                if self.world_context.picking {
                    self.cancel_world_context();
                    continue;
                }
                if self.picking_field() {
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
                    self.dismiss_panel();
                    self.entity_index = None;
                }
                continue;
            }
            if self.application_settings_shortcut(key, modifiers) {
                continue;
            }
            if key == Key::Enter
                && !modifiers.any()
                && self.project.is_none()
                && self.panel.is_none()
                && !egui::Popup::is_any_open(ctx)
                && ctx.memory(|mem| mem.top_modal_layer().is_none())
                && let Some(path) = self.settings.most_recent_project().map(Path::to_owned)
                && ctx.input_mut(|input| input.focused && input.consume_key(modifiers, key))
            {
                self.request(ctx, Action::Open(path));
                break;
            }
            if modifiers.command {
                if self.canvas_display_shortcut(key, modifiers) {
                    continue;
                }
                match key {
                    Key::S => {
                        self.save(modifiers.shift);
                    }
                    Key::O => self.request(ctx, Action::Browse),
                    Key::N => self.request(ctx, Action::New),
                    Key::W if self.project.is_some() => self.request(ctx, Action::Close),
                    Key::W | Key::Q => self.request(ctx, Action::Quit),
                    Key::F | Key::K | Key::P
                        if self.project.is_some() && project_search_key(key, modifiers) =>
                    {
                        self.open_project_search();
                    }
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
                    Key::ArrowUp | Key::ArrowDown if !focus && !self.command_palette => {
                        self.change_entity_depth(key == Key::ArrowUp, modifiers.shift)
                    }
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
                if let Some(&index) = self.visible_layer_indices(false).get(index) {
                    self.switch_layer(index);
                }
                continue;
            }
            if self.toolbar_panel_shortcut(key, modifiers) {
                continue;
            }
            if self
                .panel
                .is_some_and(|p| !matches!(p, Panel::Entity | Panel::World))
            {
                continue;
            }
            if self.canvas_display_shortcut(key, modifiers) {
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
                    ctx,
                    i32::from(key == right) - i32::from(key == left),
                    i32::from(key == down) - i32::from(key == up),
                );
                continue;
            }
            let tile_tool = self
                .selected
                .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
                .is_some_and(|layer| layer["__type"] == "Tiles");
            match key {
                Key::Tab if !modifiers.any() => {
                    ctx.input_mut(|input| input.consume_key(modifiers, Key::Tab));
                    ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                    self.toggle_zen_mode(ctx);
                }
                Key::R if tile_tool && !modifiers.any() => {
                    self.random_tiles = !self.random_tiles;
                    let label = if self.random_tiles {
                        "Random mode"
                    } else {
                        "Stamp mode"
                    };
                    self.notify_quick(locale::tr_ctx(ctx, label).into());
                }
                Key::X | Key::H if tile_tool && (key == Key::X || modifiers.shift) => {
                    self.flip_tile_brush(ctx, 1);
                }
                Key::Y | Key::V if tile_tool && (key == Key::Y || modifiers.shift) => {
                    self.flip_tile_brush(ctx, 2);
                }
                Key::S if modifiers.shift || !save_tile_shortcut_needs_shift(navigation) => {
                    self.tile_selection_memory(ctx, false)
                }
                Key::L if modifiers.shift => self.tile_selection_memory(ctx, true),
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

    fn keyboard_zoom_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        time: f64,
        focused: bool,
        events: Vec<egui::Event>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            time: Some(time),
            focused,
            modifiers: events
                .iter()
                .find_map(|event| match event {
                    egui::Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default(),
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        let _ = ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
    }

    fn keyboard_zoom_editor() -> (EditorApp, egui::Context) {
        let mut project = Project::new();
        let selected = project.level_keys()[0];
        project.move_level(selected, 1024, 2048, 0).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.apply_initial_preferences(&ctx);
        app.set_project(project);
        keyboard_zoom_frame(&mut app, &ctx, 1., true, vec![]);
        (app, ctx)
    }

    #[test]
    fn enter_on_home_opens_the_most_recent_project_when_no_modal_is_open() {
        let directory = tempfile::tempdir().unwrap();
        let latest = directory.path().join("Newest.ldtk");
        let older = directory.path().join("Older.ldtk");
        Project::new().save(&latest).unwrap();
        Project::new().save(&older).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.settings.value["recentProjects"] = serde_json::json!([latest, older]);
        let changelog = egui::Id::new("release-notes-open");
        ctx.data_mut(|data| data.insert_temp(changelog, true));
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.,
            true,
            vec![key_event(
                Key::Enter,
                Key::Enter,
                true,
                egui::Modifiers::NONE,
            )],
        );
        assert!(app.loading.is_none() && app.project.is_none());

        ctx.data_mut(|data| data.insert_temp(changelog, false));
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            2.,
            true,
            vec![key_event(
                Key::Enter,
                Key::Enter,
                false,
                egui::Modifiers::NONE,
            )],
        );
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            3.,
            true,
            vec![key_event(
                Key::Enter,
                Key::Enter,
                true,
                egui::Modifiers::NONE,
            )],
        );
        assert!(app.loading.is_some());
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while app.loading.is_some() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.poll_services(&ctx);
        }
        assert_eq!(
            app.project.as_ref().and_then(|p| p.path.as_ref()),
            Some(&latest)
        );
    }

    fn key_event(
        key: Key,
        physical: Key,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(physical),
            pressed,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn fullscreen_only_accepts_the_original_linux_chord() {
        let (mut app, ctx) = keyboard_zoom_editor();
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let ctrl_shift = egui::Modifiers {
            shift: true,
            ..ctrl
        };
        for (key, modifiers, expected) in [
            (Key::F11, egui::Modifiers::NONE, false),
            (Key::Enter, ctrl, false),
            (
                Key::F,
                egui::Modifiers {
                    alt: true,
                    ..ctrl_shift
                },
                false,
            ),
            (Key::F, ctrl_shift, true),
        ] {
            let output = ctx.run(
                egui::RawInput {
                    focused: true,
                    modifiers,
                    events: vec![key_event(key, key, true, modifiers)],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            let toggled = output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Fullscreen(true)));
            assert_eq!(toggled, expected, "{key:?} {modifiers:?}");
            assert!(!app.command_palette, "{key:?} {modifiers:?} opened search");
            let _ = ctx.run(
                egui::RawInput {
                    focused: true,
                    events: vec![key_event(key, key, false, modifiers)],
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
        }
    }

    #[test]
    fn mac_window_shortcuts_require_unmodified_command() {
        let cmd = egui::Modifiers {
            command: true,
            mac_cmd: true,
            ..Default::default()
        };
        for key in [Key::H, Key::M] {
            assert!(mac_window_key("macos", key, cmd));
            assert!(!mac_window_key("windows", key, cmd));
            assert!(!mac_window_key("linux", key, cmd));
            for extra in [
                egui::Modifiers { ctrl: true, ..cmd },
                egui::Modifiers { alt: true, ..cmd },
                egui::Modifiers { shift: true, ..cmd },
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
                egui::Modifiers::NONE,
            ] {
                assert!(!mac_window_key("macos", key, extra), "{key:?} {extra:?}");
            }
        }
        assert!(!mac_window_key("macos", Key::F, cmd));
    }

    #[test]
    fn fullscreen_shortcuts_follow_the_original_platform_bindings() {
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..none
        };
        let cmd = egui::Modifiers {
            mac_cmd: true,
            command: true,
            ..none
        };
        let alt = egui::Modifiers { alt: true, ..none };
        for (platform, key, modifiers, accepted) in [
            ("windows", Key::F11, none, true),
            ("windows", Key::F11, ctrl, false),
            ("windows", Key::Enter, alt, true),
            ("windows", Key::Enter, ctrl, false),
            (
                "windows",
                Key::F,
                egui::Modifiers {
                    shift: true,
                    ..ctrl
                },
                true,
            ),
            ("macos", Key::F11, none, false),
            ("macos", Key::Enter, cmd, true),
            ("macos", Key::Enter, alt, false),
            ("macos", Key::Enter, ctrl, false),
            ("macos", Key::F, egui::Modifiers { ctrl: true, ..cmd }, true),
            (
                "macos",
                Key::F,
                egui::Modifiers { shift: true, ..cmd },
                true,
            ),
            (
                "macos",
                Key::F,
                egui::Modifiers {
                    ctrl: true,
                    shift: true,
                    ..cmd
                },
                false,
            ),
            (
                "macos",
                Key::F,
                egui::Modifiers {
                    shift: true,
                    ..ctrl
                },
                false,
            ),
        ] {
            assert_eq!(
                fullscreen_key(platform, key, modifiers),
                accepted,
                "{platform} {key:?} {modifiers:?}"
            );
        }
    }

    #[test]
    fn batched_blur_and_refocus_restore_shortcuts_without_a_click() {
        let (mut app, ctx) = keyboard_zoom_editor();
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("help-video-thumbnails-requested"), true);
        });
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            2.,
            true,
            vec![
                egui::Event::WindowFocused(false),
                egui::Event::WindowFocused(true),
            ],
        );
        assert!(
            app.clicktrap,
            "the refocus frame still guards canvas clicks"
        );
        keyboard_zoom_frame(&mut app, &ctx, 2.05, true, vec![]);
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            2.1,
            true,
            vec![key_event(Key::H, Key::H, true, egui::Modifiers::NONE)],
        );
        assert_eq!(app.panel, Some(Panel::Help));

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            3.,
            false,
            vec![
                egui::Event::WindowFocused(true),
                egui::Event::WindowFocused(false),
            ],
        );
        keyboard_zoom_frame(&mut app, &ctx, 3.05, false, vec![]);
        assert!(
            app.clicktrap,
            "the final blur guards the canvas until refocus"
        );
    }

    #[test]
    fn canvas_option_shortcuts_save_preferences_and_report_source_feedback() {
        let (mut app, ctx) = keyboard_zoom_editor();
        app.quick_notification_test_age = Some(std::time::Duration::ZERO);
        let mut project = Project::new();
        let level = project.level_keys()[0];
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.paint_int_grid(level, 0, 0, 0, 1).unwrap();
        app.set_project(project);
        let settings_dir = tempfile::tempdir().unwrap();
        let settings = settings_dir.path().join("settings.json");
        app.settings.path = Some(settings.clone());
        app.capture_selection([0, 0, 1, 1]);
        assert!(app.selection_group.is_some());

        let mut time = 2.;
        let mut press = |app: &mut EditorApp, key, modifiers| {
            keyboard_zoom_frame(
                app,
                &ctx,
                time,
                true,
                vec![key_event(key, key, true, modifiers)],
            );
            keyboard_zoom_frame(
                app,
                &ctx,
                time + 0.05,
                true,
                vec![key_event(key, key, false, modifiers)],
            );
            time += 0.1;
        };
        let message = |app: &EditorApp| {
            app.quick_notification
                .as_ref()
                .map(|notice| notice.0.clone())
        };
        let saved = || serde_json::from_slice::<Value>(&std::fs::read(&settings).unwrap()).unwrap();

        press(&mut app, Key::G, egui::Modifiers::NONE);
        assert!(!app.grid);
        assert_eq!(message(&app).as_deref(), Some("Grid: off"));
        assert_eq!(saved()["grid"], false);
        assert!(app.selection_group.is_some());

        press(&mut app, Key::T, egui::Modifiers::SHIFT);
        assert!(!app.tile_stacking);
        assert_eq!(message(&app).as_deref(), Some("Tile stacking: off"));
        assert_eq!(saved()["tileStacking"], false);
        assert!(app.selection_group.is_none());

        press(&mut app, Key::E, egui::Modifiers::SHIFT);
        assert_eq!(message(&app).as_deref(), Some("Select empty spaces: off"));
        assert_eq!(saved()["emptySpaceSelection"], false);

        let (key, modifiers) = if cfg!(target_os = "macos") {
            (Key::H, egui::Modifiers::SHIFT)
        } else {
            (
                Key::H,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            )
        };
        press(&mut app, key, modifiers);
        assert_eq!(message(&app).as_deref(), Some("Showing tiles only"));
        assert_eq!(saved()["showDetails"], false);

        let settings_before = std::fs::read(&settings).unwrap();
        press(&mut app, Key::R, egui::Modifiers::SHIFT);
        assert!(!app.render_auto);
        assert_eq!(message(&app).as_deref(), Some("Auto-layers rendering: off"));
        assert_eq!(std::fs::read(&settings).unwrap(), settings_before);
        assert!(!app.dirty);
    }

    #[test]
    fn held_keyboard_zoom_uses_frame_time_and_releases_shifted_equals_by_physical_key() {
        let (mut app, ctx) = keyboard_zoom_editor();
        app.settings.value["mouseWheelSpeed"] = json!(2.);
        app.viewport.zoom = 2.;
        app.viewport.center = [80., 112.];
        let start = app.viewport.center;
        let delta = 0.45 * 0.15 * 1.5;
        let mut zoom = 2.;
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.025,
            true,
            vec![
                egui::Event::PointerMoved(egui::pos2(40., 400.)),
                key_event(Key::Plus, Key::Equals, true, egui::Modifiers::SHIFT),
            ],
        );
        zoom *= 1. + delta;
        assert!(
            (app.viewport.zoom - zoom).abs() < 0.0001,
            "{} != {zoom}",
            app.viewport.zoom
        );
        for time in [1.05, 1.8] {
            keyboard_zoom_frame(&mut app, &ctx, time, true, vec![]);
            zoom *= 1. + delta;
            assert!((app.viewport.zoom - zoom).abs() < 0.0001);
        }
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.825,
            true,
            vec![key_event(
                Key::Equals,
                Key::Equals,
                false,
                egui::Modifiers::NONE,
            )],
        );
        assert!((app.viewport.zoom - zoom).abs() < 0.0001);
        for (center, start) in app.viewport.center.into_iter().zip(start) {
            assert!((center - start).abs() * zoom <= 2.);
        }

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.85,
            true,
            vec![
                key_event(Key::Equals, Key::Equals, true, egui::Modifiers::COMMAND),
                key_event(Key::Plus, Key::Plus, true, egui::Modifiers::COMMAND),
                key_event(Key::Minus, Key::Minus, true, egui::Modifiers::COMMAND),
            ],
        );
        zoom *= (1. + delta) * (1. - delta);
        assert!((app.viewport.zoom - zoom).abs() < 0.0001);
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.875,
            true,
            vec![
                key_event(Key::Equals, Key::Equals, false, egui::Modifiers::COMMAND),
                key_event(Key::Minus, Key::Minus, false, egui::Modifiers::COMMAND),
            ],
        );
        zoom *= 1. + delta;
        assert!((app.viewport.zoom - zoom).abs() < 0.0001);
        assert_eq!(ctx.zoom_factor(), 1.);
    }

    #[test]
    fn keyboard_zoom_uses_the_sources_page_coordinate_at_a_world_level_edge() {
        let (mut app, ctx) = keyboard_zoom_editor();
        let anchor = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.move_level(anchor, 1024, 2048, 1).unwrap();
        let left = project.add_level(None, "Left", 256, 256).unwrap();
        project.move_level(left, 896, 2048, 0).unwrap();
        let right = project.add_level(None, "Right", 256, 256).unwrap();
        project.move_level(right, 1152, 2048, 0).unwrap();
        app.world = true;
        app.panel = Some(Panel::World);
        app.world_depth = 0;
        app.viewport.center = [1152., 2176.];
        app.viewport.zoom = 1.498;

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.025,
            true,
            vec![key_event(
                Key::Equals,
                Key::Equals,
                true,
                egui::Modifiers::NONE,
            )],
        );
        assert!(!app.world);
        assert_eq!(app.selected, Some(left));
    }

    #[test]
    fn held_keyboard_zoom_respects_focus_and_uses_the_same_world_transition_as_wheel_zoom() {
        let (mut app, ctx) = keyboard_zoom_editor();
        let selected = app.selected;
        app.viewport.zoom = 1.6;
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.025,
            true,
            vec![key_event(
                Key::Minus,
                Key::Minus,
                true,
                egui::Modifiers::NONE,
            )],
        );
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
        assert_eq!(app.selected, selected);
        assert_eq!(app.viewport.center, [1152., 2176.]);
        let zoom = app.viewport.zoom;

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.05,
            false,
            vec![egui::Event::WindowFocused(false)],
        );
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.075,
            true,
            vec![egui::Event::WindowFocused(true)],
        );
        keyboard_zoom_frame(&mut app, &ctx, 1.1, true, vec![]);
        assert_eq!(app.viewport.zoom, zoom);

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.125,
            true,
            vec![key_event(Key::P, Key::P, true, egui::Modifiers::COMMAND)],
        );
        assert!(app.command_palette);
        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.15,
            true,
            vec![
                key_event(Key::Plus, Key::Plus, true, egui::Modifiers::NONE),
                egui::Event::Text("+".into()),
            ],
        );
        assert_eq!(app.search, "+");
        assert_eq!(app.viewport.zoom, zoom);
        assert!(app.world);

        keyboard_zoom_frame(
            &mut app,
            &ctx,
            1.175,
            true,
            vec![key_event(
                Key::Escape,
                Key::Escape,
                true,
                egui::Modifiers::NONE,
            )],
        );
        keyboard_zoom_frame(&mut app, &ctx, 1.2, true, vec![]);
        assert!(!app.command_palette);
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
        assert_eq!(app.selected, selected);
        assert_eq!(app.viewport.zoom, zoom);
        assert_eq!(app.viewport.center, [1152., 2176.]);
    }

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
        let ctx = egui::Context::default();
        app.tile_rect = [0, 0, 3, 1];
        app.atlas_selection = Some((atlas, vec![0, 2]));
        app.random_tiles = true;
        app.tile_selection_memory(&ctx, false);
        assert_eq!(
            app.project.as_ref().unwrap().tileset(atlas).unwrap()["savedSelections"][0]["ids"],
            json!([0, 2])
        );
        assert_eq!(
            app.canvas_notices.len(),
            1,
            "memory save acknowledges the selection"
        );
        app.atlas_selection = None;
        app.tile_id = 1;
        app.tile_rect = [2, 0, 1, 1];
        app.flips = 3;
        app.random_tiles = false;
        app.tile_selection_memory(&ctx, true);
        assert_eq!(app.atlas_selection, Some((atlas, vec![0, 2])));
        assert_eq!(app.tile_rect, [0, 0, 3, 1]);
        assert_eq!(app.flips, 3);
        assert!(app.random_tiles);
        assert_eq!(palette_popout::take_focus(&ctx, atlas), Some(false));
        assert!(
            matches!(&app.quick_notification, Some((text, None, _)) if text == "Loaded selection of 2 tiles")
        );
        app.quick_notification = None;
        app.random_tiles = false;
        app.tile_selection_memory(&ctx, true);
        assert!(!app.random_tiles);
        assert_eq!(palette_popout::take_focus(&ctx, atlas), None);
        assert!(app.quick_notification.is_none());
    }
    #[test]
    fn tile_keyboard_mode_and_flip_changes_show_source_notifications() {
        let (mut app, _) = atlas_app();
        let ctx = egui::Context::default();
        for (key, expected) in [
            (Key::R, "Random mode"),
            (Key::R, "Stamp mode"),
            (Key::X, "X-flip: ON"),
            (Key::Y, "Y-flip: ON"),
            (Key::X, "X-flip: off"),
        ] {
            let events = [true, false]
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .to_vec();
            let _ = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| app.shortcuts(ctx),
            );
            assert!(
                matches!(&app.quick_notification, Some((name, None, _)) if name == expected),
                "{expected}"
            );
            app.quick_notification = None;
        }
        assert_eq!(app.flips, 2);
        assert!(!app.random_tiles);
    }
    #[test]
    fn atlas_navigation_preserves_sparse_mask_and_wraps_as_a_group() {
        let (mut app, atlas) = atlas_app();
        let ctx = egui::Context::default();
        app.tile_rect = [0, 0, 3, 1];
        app.atlas_selection = Some((atlas, vec![0, 2]));
        app.flips = 1;
        app.navigate_palette(&ctx, 0, 1);
        assert_eq!(app.atlas_selection, Some((atlas, vec![4, 6])));
        assert_eq!(app.flips, 0);
        app.navigate_palette(&ctx, 0, 1);
        assert_eq!(app.atlas_selection, Some((atlas, vec![0, 2])));
        app.project
            .as_mut()
            .unwrap()
            .update_tileset_definition(
                atlas,
                &json!({"savedSelections":[{"ids":[3,7],"mode":"Random"}]}),
            )
            .unwrap();
        app.navigate_palette(&ctx, 1, 0);
        assert_eq!(app.atlas_selection, Some((atlas, vec![3, 7])));
        assert!(app.random_tiles);
    }
    #[test]
    fn preview_154_command_p_opens_search_and_preserves_query() {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.search = "previous".into();
        let ctx = egui::Context::default();
        let modifiers = egui::Modifiers::COMMAND;
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::P,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.shortcuts(ctx));
        assert!(app.command_palette);
        assert_eq!(app.search, "previous");
    }

    #[test]
    fn preview_154_manual_entity_depth_shortcuts_preserve_selection_and_undo() {
        let mut p = Project::new();
        p.add_layer("Ground", "IntGrid", 16).unwrap();
        let layer_uid = p.add_layer("Actors", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let level = p.level_keys()[0];
        for x in [16, 32, 48] {
            p.add_entity(level, 0, def, x, 16).unwrap();
        }
        let ids: Vec<_> = p.layer(level, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["iid"].clone())
            .collect();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_index = Some(1);
        app.selection_group = Some(
            json!({"elementCount": 1, "layers": [{"layerDefUid": layer_uid, "clipboard": {"entityInstances": [{"iid": ids[1]}]}}]}),
        );
        let ctx = egui::Context::default();
        let send = |app: &mut EditorApp, key, shift| {
            let modifiers = egui::Modifiers {
                ctrl: true,
                command: true,
                shift,
                ..Default::default()
            };
            let raw = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| app.shortcuts(ctx));
            let release = egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            };
            let _ = ctx.run(release, |ctx| app.shortcuts(ctx));
        };
        app.project.as_mut().unwrap().json["defs"]["layers"][0]["entityDepthSorting"] =
            json!("YAscending");
        send(&mut app, Key::ArrowUp, false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(level, 0).unwrap()["entityInstances"][1]["iid"],
            ids[1]
        );
        app.project.as_mut().unwrap().json["defs"]["layers"][0]["entityDepthSorting"] =
            json!("Manual");
        app.selection_group.as_mut().unwrap()["elementCount"] = json!(2);
        send(&mut app, Key::ArrowUp, false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(level, 0).unwrap()["entityInstances"][1]["iid"],
            ids[1]
        );
        app.selection_group = None;
        app.panel = Some(Panel::Entity);
        send(&mut app, Key::ArrowUp, false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(level, 0).unwrap()["entityInstances"][2]["iid"],
            ids[1]
        );
        assert_eq!(app.entity_index, Some(2));
        app.switch_layer(1);
        assert_eq!(app.layer, 1);
        assert_eq!(app.panel, None);
        assert_eq!(app.entity_index, None);
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 1);
        send(&mut app, Key::ArrowDown, true);
        assert_eq!(
            app.project.as_ref().unwrap().layer(level, 0).unwrap()["entityInstances"][0]["iid"],
            ids[1]
        );
        assert_eq!(app.entity_index, None);
        assert!(
            app.history
                .level_change_bounds(app.project.as_ref().unwrap(), level, false)
                .is_some()
        );
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(level, 0).unwrap()["entityInstances"][2]["iid"],
            ids[1]
        );
        assert!(app.selection_group.is_none());
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
        let project = app.project.as_ref().unwrap();
        app.settings
            .set_ui_state_int("LayerUIFilter", project.json["iid"].as_str(), 0);
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
