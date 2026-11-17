use super::*;
use serde_json::json;

pub(super) enum Gesture {
    Rectangle {
        start: [i32; 2],
        end: [i32; 2],
        erase: bool,
        select: bool,
        started: Instant,
        world_end: [f32; 2],
    },
    SelectionMove {
        region: [i32; 4],
        start: [i32; 2],
        end: [i32; 2],
        copy: bool,
        world_start: [f32; 2],
        world_end: [f32; 2],
        moved: bool,
    },
    EntityMove {
        key: LevelKey,
        layer: usize,
        index: usize,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        moved: bool,
    },
    EntityResize {
        key: LevelKey,
        layer: usize,
        index: usize,
        start: [f32; 2],
        size: [i32; 2],
        origin: [i32; 2],
        handle: [i32; 2],
    },
    LevelMove {
        key: LevelKey,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        depth: i32,
        moved: bool,
        clicked_same: bool,
    },
}

pub(super) fn region(a: [i32; 2], b: [i32; 2]) -> [i32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        (a[0] - b[0]).abs() + 1,
        (a[1] - b[1]).abs() + 1,
    ]
}

pub(super) fn stamp_cells(
    origin: [i32; 2],
    tile: [i32; 4],
    scale: i32,
    flips: i32,
) -> Vec<([i32; 2], [i32; 2])> {
    let mut result = Vec::new();
    for y in 0..tile[3] {
        for x in 0..tile[2] {
            result.push((
                [
                    origin[0] + if flips & 1 != 0 { tile[2] - 1 - x } else { x } * scale,
                    origin[1] + if flips & 2 != 0 { tile[3] - 1 - y } else { y } * scale,
                ],
                [tile[0] + x, tile[1] + y],
            ));
        }
    }
    result
}

pub(super) fn rectangle_tiles(
    rect: [i32; 4],
    tile: [i32; 4],
    scale: i32,
) -> Vec<([i32; 2], [i32; 2])> {
    let mut result = Vec::new();
    for y in (0..rect[3]).step_by(scale as usize) {
        for x in (0..rect[2]).step_by(scale as usize) {
            result.push((
                [rect[0] + x, rect[1] + y],
                [tile[0] + x / scale % tile[2], tile[1] + y / scale % tile[3]],
            ));
        }
    }
    result
}

impl EditorApp {
    pub(super) fn edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, None, false, |p, _| operation(p))
    }
    pub(super) fn edit_recoverable<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, None, true, |p, _| operation(p))
    }
    pub(super) fn edit_level<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), false, |p, _| operation(p))
    }
    pub(super) fn edit_level_recoverable<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), true, |p, _| operation(p))
    }
    pub(super) fn edit_level_marked<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project, &mut History) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), false, operation)
    }
    fn edit_operation<T>(
        &mut self,
        label: &str,
        key: Option<LevelKey>,
        recoverable: bool,
        operation: impl FnOnce(&mut Project, &mut History) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.finish_gesture();
        self.finish_stroke();
        self.last_chance = None;
        let project = self.project.as_mut()?;
        if project.is_backup() {
            self.status = "Restore the backup before editing it".into();
            return None;
        }
        let definitions = key.is_none().then(|| project.json["defs"].clone());
        let entity_owner = key
            .filter(|_| self.panel == Some(Panel::Entity))
            .and_then(|key| {
                let entity = project
                    .layer(key, self.layer)?
                    .get("entityInstances")?
                    .get(self.entity_index?)?;
                Some((key, self.layer, entity["iid"].as_str()?.to_owned()))
            });
        if let Some(key) = key {
            self.history.begin(project, key);
        } else {
            self.history.begin_project(project);
        }
        match operation(project, &mut self.history) {
            Ok(value) => {
                if let Some((key, layer, iid)) = entity_owner
                    && let Some(entity) = project
                        .layer(key, layer)
                        .and_then(|l| l["entityInstances"].as_array())
                        .and_then(|es| es.iter().find(|e| e["iid"] == iid))
                {
                    self.history.mark_entity_change(project, key, layer, entity);
                }
                let changed = self.history.commit(project);
                self.dirty |= changed;
                if changed {
                    if definitions.as_ref().is_some_and(|before| {
                        before["entities"] != project.json["defs"]["entities"]
                    }) {
                        self.chain_entity = None;
                    }
                    if definitions
                        .as_ref()
                        .is_some_and(|before| timeline_invalidated(before, &project.json["defs"]))
                    {
                        self.history.clear_level_timelines();
                    }
                    if recoverable {
                        self.last_chance = Some((label.into(), Instant::now()));
                    }
                }
                self.status = label.into();
                self.status_error = false;
                Some(value)
            }
            Err(error) => {
                self.history.cancel(project);
                self.status = error.to_string();
                self.status_error = true;
                None
            }
        }
    }
    pub(super) fn restore_last_chance(&mut self) {
        if self.last_chance.take().is_none() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        if let Some(project) = &mut self.project {
            self.dirty |= self.history.undo(project);
            self.history.clear_level_timelines();
        }
        self.panel = None;
        self.selection = None;
        self.selection_group = None;
        self.stamp = None;
        self.entity_index = None;
        self.normalize_palette();
    }

    pub(super) fn finish_gesture(&mut self) {
        self.finish_level_resize();
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let Some(key) = self.selected else {
            return;
        };
        match gesture {
            Gesture::Rectangle {
                start,
                end,
                erase,
                select,
                started,
                world_end,
            } => {
                let r = region(start, end);
                if select {
                    if r[2] == 1 && r[3] == 1 {
                        self.select_element_at(world_end, false, false);
                    } else {
                        self.capture_selection(r);
                    }
                    self.finish_stroke();
                    return;
                }
                if start == end && started.elapsed().as_secs_f32() <= 0.22 {
                    self.fill(start, erase);
                } else {
                    self.paint_rectangle(r, erase);
                }
            }
            Gesture::SelectionMove {
                region,
                start,
                end,
                copy,
                world_start,
                world_end,
                moved,
            } => {
                if !moved {
                    self.finish_stroke();
                    return;
                }
                if let Some(group) = self.selection_group.clone() {
                    let delta = self.selection_world_delta(world_start, world_end);
                    if delta == [0.0; 2] {
                        self.finish_stroke();
                        return;
                    }
                    let geometries = self.selection_geometries(key);
                    let origin = std::array::from_fn(|axis| {
                        group["origin"][axis].as_f64().unwrap_or(0.0) + delta[axis]
                    });
                    if let Some(group) =
                        self.edit_level_marked("Selection moved", key, |p, history| {
                            bleep::mark_group(history, p, key, &group, false);
                            bleep::mark_moved_group(history, p, key, &group, delta);
                            let next = if copy {
                                p.paste_selection_group_with_geometry(
                                    key,
                                    &group,
                                    origin,
                                    &geometries,
                                    false,
                                    false,
                                    true,
                                )
                            } else {
                                p.move_selection_group_with_geometry(
                                    key,
                                    &group,
                                    delta,
                                    &geometries,
                                    false,
                                    false,
                                )
                            }?;
                            Ok(next)
                        })
                    {
                        self.set_selection_group(group);
                    }
                    return;
                }
                let layer = self.layer;
                if self
                    .edit_level("Selection moved", key, |p| {
                        p.move_region(
                            key,
                            layer,
                            region,
                            end[0] - start[0],
                            end[1] - start[1],
                            false,
                            false,
                        )?;
                        p.rebuild_auto_layers(key)
                    })
                    .is_some()
                {
                    self.selection = Some([
                        region[0] + end[0] - start[0],
                        region[1] + end[1] - start[1],
                        region[2],
                        region[3],
                    ]);
                }
                return;
            }
            Gesture::LevelMove {
                key,
                end,
                moved,
                clicked_same,
                ..
            } => {
                if !moved {
                    if clicked_same {
                        self.world = false;
                        self.fit = true;
                    }
                    self.finish_stroke();
                    return;
                }
                if let Some(project) = &mut self.project {
                    let container = key
                        .world
                        .and_then(|i| project.json["worlds"].get(i))
                        .unwrap_or(&project.json);
                    let layout = text(container, "worldLayout", "Free");
                    if layout.starts_with("Linear") {
                        let axis = usize::from(layout == "LinearVertical");
                        let placements = canvas::placements(project, key);
                        let mut points = placements
                            .iter()
                            .map(|(k, b)| (k.level, b[axis]))
                            .collect::<Vec<_>>();
                        if let Some((last, b)) = placements.last()
                            && last.level != key.level
                        {
                            points.push((last.level + 1, b[axis + 2]));
                        }
                        if let Some(&(insertion, _)) = points.iter().min_by(|a, b| {
                            (a.1 - end[axis]).abs().total_cmp(&(b.1 - end[axis]).abs())
                        }) {
                            let index = insertion - usize::from(insertion > key.level);
                            match project.move_level_to_world(key, key.world, index) {
                                Ok(next) => self.selected = Some(next),
                                Err(e) => {
                                    self.history.cancel(project);
                                    self.status = e.to_string();
                                }
                            }
                        }
                    } else if let Err(error) = project.refresh_derived_data() {
                        self.history.cancel(project);
                        self.status = error.to_string();
                        self.status_error = true;
                    }
                }
            }
            Gesture::EntityMove {
                key,
                layer,
                index,
                start,
                end,
                origin,
                moved,
            } => {
                if moved && let Some(project) = &self.project {
                    let grid = project
                        .layer(key, layer)
                        .map(|l| number(l, "__gridSize", 16.0))
                        .unwrap_or(16.0);
                    let a = canvas::layer_to_world(project, key, layer, &self.viewport, [0.0; 2]);
                    let b = canvas::layer_to_world(project, key, layer, &self.viewport, [1.0, 0.0]);
                    let position =
                        entity_tools::drag_position(origin, start, end, grid, b[0] - a[0]);
                    if let Some(entity) = self
                        .project
                        .as_mut()
                        .and_then(|p| p.layer_mut(key, layer))
                        .and_then(|l| l["entityInstances"].get_mut(index))
                    {
                        entity["px"] = json!(position);
                    }
                }
                self.finish_entity_gesture(key, layer, index);
            }
            Gesture::EntityResize {
                key, layer, index, ..
            } => self.finish_entity_gesture(key, layer, index),
        }
        self.finish_stroke();
    }

    fn finish_entity_gesture(&mut self, key: LevelKey, layer: usize, index: usize) {
        if let Some(project) = &mut self.project {
            if let Some(entity) = project
                .layer(key, layer)
                .and_then(|l| l["entityInstances"].get(index))
            {
                self.history.mark_entity_change(project, key, layer, entity);
            }
            match project.finish_entity_transform(key, layer, index) {
                Ok(true) => self.entity_index = None,
                Ok(false) => {}
                Err(error) => {
                    self.history.cancel(project);
                    self.status = error.to_string();
                    self.status_error = true;
                }
            }
        }
    }

    pub(super) fn cancel_gesture(&mut self) {
        self.cancel_level_resize();
        self.gesture = None;
        self.stroke = false;
        self.last_cell = None;
        self.painted_cells.clear();
        if let Some(project) = &mut self.project {
            self.history.cancel(project);
        }
    }

    pub(super) fn pointer_lifecycle(&mut self, ctx: &egui::Context) -> bool {
        if self.clicktrap
            && ctx.input(|i| {
                i.events
                    .iter()
                    .any(|event| matches!(event, egui::Event::WindowFocused(true)))
            })
        {
            self.focus_frames = Some(2);
        }
        let lost = ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::PointerGone | egui::Event::WindowFocused(false)
                )
            })
        });
        if lost {
            if self.project.is_some()
                && ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::WindowFocused(false)))
                })
            {
                self.clicktrap = true;
                self.focus_frames = None;
            }
            self.finish_gesture();
            self.finish_stroke();
            ctx.input_mut(|i| i.pointer = Default::default());
        }
        lost
    }

    pub(super) fn canvas_controls(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) {
        if self.pointer_lifecycle(ctx) {
            return;
        }
        let released = ctx.input(|i| i.pointer.any_released());
        self.canvas_controls_frame(ctx, rect, response);
        if released {
            self.finish_gesture();
            self.finish_stroke();
        }
    }

    fn canvas_controls_frame(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) {
        if self.project.is_none()
            || self.pending.is_some()
            || self.suggest_backups
            || self.is_busy()
            || self.command_palette
            || self.command_open
            || !self.resource_missing.is_empty()
            || self.crash.is_some()
            || self.clicktrap
            || text_editor::is_open(ctx)
            || world_form_dialog::is_open(ctx)
            || help::changelog_is_open(ctx)
        {
            return;
        }
        if self.fit
            && let (Some(project), Some(key)) = (&self.project, self.selected)
        {
            let bounds = canvas::bounds(project, key, self.world);
            if self.world {
                self.viewport.fit_world(bounds, rect.size().into());
            } else {
                self.viewport.fit(bounds, rect.size().into());
            }
            self.fit = false;
        }
        if self
            .panel
            .is_some_and(|panel| !matches!(panel, Panel::Entity | Panel::World))
        {
            return;
        }
        if self.level_resize_input(ctx, rect, response) {
            return;
        }
        let (
            pointer,
            delta,
            scroll,
            space,
            modifiers,
            primary,
            secondary,
            middle,
            pressed,
            released,
        ) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.delta(),
                i.raw_scroll_delta.y,
                i.key_down(Key::Space),
                i.modifiers,
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.middle_down(),
                i.pointer.any_pressed(),
                i.pointer.any_released(),
            )
        });
        let Some(pointer) = pointer else {
            return;
        };
        let local_world =
            self.viewport
                .to_world(pointer.into(), rect.min.into(), rect.size().into());
        if response.hovered() && scroll != 0.0 {
            self.wheel_zoom(ctx, rect, pointer, scroll);
        }
        if response.hovered() && (middle || (space && primary)) {
            if delta.length_sq() > 0.0 {
                self.space_dragged = true;
            }
            self.viewport.center[0] -= delta.x / self.viewport.zoom;
            self.viewport.center[1] -= delta.y / self.viewport.zoom;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            return;
        }
        if response.hovered() && space && secondary {
            if delta.length_sq() > 0.0 {
                self.space_dragged = true;
            }
            self.viewport.zoom_at(
                (delta.y * 0.01).exp(),
                pointer.into(),
                rect.min.into(),
                rect.size().into(),
            );
            return;
        }
        if self.gesture.is_none() && (!rect.contains(pointer) || !response.hovered()) {
            self.last_cell = None;
            return;
        }
        if space {
            return;
        }
        if self.project.as_ref().is_some_and(Project::is_backup) {
            return;
        }
        if self.field_pick_input(ctx, rect, response) {
            return;
        }
        if self.world {
            if let Some(Gesture::LevelMove {
                key,
                start,
                end,
                origin,
                depth,
                moved,
                ..
            }) = &mut self.gesture
            {
                *end = local_world;
                *moved |= (local_world[0] - start[0]).hypot(local_world[1] - start[1])
                    * self.viewport.zoom
                    >= 8.0;
                if !*moved {
                    return;
                }
                let key = *key;
                let depth = *depth;
                let start = *start;
                let origin = *origin;
                let [x, y] = self.world_drag_position(
                    key,
                    start,
                    local_world,
                    origin,
                    modifiers.shift || modifiers.command,
                );
                if let Some(level) = self.project.as_mut().and_then(|p| p.level_mut(key)) {
                    level["worldX"] = json!(x);
                    level["worldY"] = json!(y);
                    level["worldDepth"] = json!(depth);
                }
                return;
            }
            if pressed
                && primary
                && !modifiers.shift
                && let (Some(project), Some(key)) = (&self.project, self.selected)
            {
                let hit = self.level_at_world(local_world).and_then(|candidate| {
                    canvas::placements(project, key)
                        .into_iter()
                        .find(|(k, _)| *k == candidate)
                });
                if let Some((candidate, b)) = hit {
                    let clicked_same = self.selected == Some(candidate);
                    self.navigate_level(candidate, false);
                    if response.double_clicked() {
                        self.world = false;
                        self.layer = 0;
                        self.fit = true;
                    } else {
                        let project = self.project.as_ref().unwrap();
                        let depth = project
                            .level(candidate)
                            .map(|l| number(l, "worldDepth", 0.0) as i32)
                            .unwrap_or(0);
                        self.history.begin_project(project);
                        self.last_chance = None;
                        self.stroke = true;
                        self.gesture = Some(Gesture::LevelMove {
                            key: candidate,
                            start: local_world,
                            end: local_world,
                            origin: [b[0] as i32, b[1] as i32],
                            depth,
                            moved: false,
                            clicked_same,
                        });
                    }
                }
            }
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let project = self.project.as_ref().unwrap();
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        if layer["visible"] == false
            || self
                .hidden
                .contains(&layer["layerDefUid"].as_i64().unwrap_or(-1))
        {
            return;
        }
        let grid = number(layer, "__gridSize", 16.0).max(1.0);
        let local = canvas::world_to_layer(project, key, self.layer, &self.viewport, local_world);
        let cell = [
            (local[0] / grid).floor() as i32,
            (local[1] / grid).floor() as i32,
        ];
        let width = number(layer, "__cWid", 0.0) as i32;
        let height = number(layer, "__cHei", 0.0) as i32;
        let kind = text(layer, "__type", "").to_owned();
        if !self.details
            && kind == "IntGrid"
            && !project
                .layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1))
                .is_some_and(|d| {
                    d["autoRuleGroups"]
                        .as_array()
                        .is_some_and(|r| !r.is_empty())
                })
        {
            return;
        }

        if kind == "Entities" && !self.details {
            self.finish_gesture();
            self.finish_stroke();
            return;
        }
        if pressed && kind == "Entities" && !modifiers.shift && !modifiers.alt {
            let erase = secondary || (cfg!(target_os = "macos") && modifiers.ctrl);
            if !erase && self.point_interaction(key, local, local_world, cell, false) {
                return;
            }
        }
        let project = self.project.as_ref().unwrap();
        let layer = project.layer(key, self.layer).unwrap();
        if let Some(gesture) = &mut self.gesture {
            match gesture {
                Gesture::Rectangle { end, world_end, .. } => {
                    *world_end = local_world;
                    *end = [
                        cell[0].clamp(0, (width - 1).max(0)),
                        cell[1].clamp(0, (height - 1).max(0)),
                    ];
                }
                Gesture::SelectionMove {
                    end,
                    world_start,
                    world_end,
                    moved,
                    ..
                } => {
                    *end = cell;
                    *world_end = local_world;
                    *moved |= (local_world[0] - world_start[0])
                        .hypot(local_world[1] - world_start[1])
                        * self.viewport.zoom
                        >= 10.0;
                }
                Gesture::EntityMove {
                    key,
                    layer,
                    start,
                    end,
                    moved,
                    ..
                } => {
                    *end = local;
                    let a = canvas::layer_to_world(project, *key, *layer, &self.viewport, [0.0; 2]);
                    let b =
                        canvas::layer_to_world(project, *key, *layer, &self.viewport, [1.0, 0.0]);
                    *moved |= (local[0] - start[0]).hypot(local[1] - start[1])
                        * self.viewport.zoom
                        * (b[0] - a[0])
                        >= 10.0;
                }
                Gesture::EntityResize {
                    key,
                    layer,
                    index,
                    start,
                    size,
                    origin,
                    handle,
                } => {
                    let geometry = self.project.as_ref().and_then(|p| {
                        let e = &p.layer(*key, *layer)?["entityInstances"][*index];
                        let d = p.json["defs"]["entities"]
                            .as_array()?
                            .iter()
                            .find(|d| d["uid"] == e["defUid"])?;
                        let requested = resize_tools::requested_entity_size(
                            d,
                            *size,
                            [local[0] - start[0], local[1] - start[1]],
                            *handle,
                        );
                        let result = ldtk_core::constrained_entity_size(
                            d,
                            requested,
                            *size,
                            self.grid.then_some(grid as i32),
                        );
                        Some((
                            result,
                            resize_tools::resized_entity_origin(d, *origin, *size, result, *handle),
                        ))
                    });
                    if let Some(([w, h], pos)) = geometry
                        && let Some(entity) = self
                            .project
                            .as_mut()
                            .and_then(|p| p.layer_mut(*key, *layer))
                            .and_then(|l| l["entityInstances"].as_array_mut())
                            .and_then(|a| a.get_mut(*index))
                    {
                        entity["width"] = json!(w);
                        entity["height"] = json!(h);
                        entity["px"] = json!(pos);
                    }
                }
                _ => {}
            }
            return;
        }
        if pressed
            && primary
            && kind == "Entities"
            && !modifiers.alt
            && !modifiers.shift
            && let Some(index) = self.entity_index
            && let Some(handle) =
                self.resize_handle_at(key, self.layer, index, local, self.viewport.zoom)
        {
            let entity = &layer["entityInstances"][index];
            let origin = [
                entity["px"][0].as_i64().unwrap_or(0) as i32,
                entity["px"][1].as_i64().unwrap_or(0) as i32,
            ];
            self.history.begin(project, key);
            self.last_chance = None;
            self.stroke = true;
            self.gesture = Some(Gesture::EntityResize {
                key,
                layer: self.layer,
                index,
                start: local,
                size: [
                    number(entity, "width", 16.0) as i32,
                    number(entity, "height", 16.0) as i32,
                ],
                origin,
                handle,
            });
            return;
        }
        if pressed
            && primary
            && !modifiers.shift
            && self.selection_contains_world(local_world)
            && let Some(region) = self.selection
        {
            self.history.begin(project, key);
            self.last_chance = None;
            self.stroke = true;
            self.gesture = Some(Gesture::SelectionMove {
                region,
                start: cell,
                end: cell,
                copy: modifiers.command && modifiers.alt,
                world_start: local_world,
                world_end: local_world,
                moved: false,
            });
            return;
        }
        if pressed && primary && modifiers.alt && !modifiers.shift {
            self.select_element_at(local_world, modifiers.command, true);
            return;
        }
        if cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height {
            self.last_cell = None;
            return;
        }
        if !(primary || secondary || (released && self.stroke)) || modifiers.alt && !modifiers.shift
        {
            return;
        }
        let erase = if released && self.stroke {
            self.stroke_erase
        } else {
            secondary || (cfg!(target_os = "macos") && modifiers.ctrl)
        };
        if !self.stroke {
            if !pressed || !response.hovered() {
                return;
            }
            if kind == "Entities" {
                self.history.begin(project, key);
            } else {
                self.history.begin_layer(project, key, self.layer);
            }
            self.last_chance = None;
            self.selection = None;
            self.selection_group = None;
            if kind != "Entities" {
                self.entity_index = None;
                if self.panel == Some(Panel::Entity) {
                    self.panel = None;
                }
            }
            self.stroke = true;
            self.stroke_erase = erase;
        }
        if pressed && modifiers.shift {
            self.gesture = Some(Gesture::Rectangle {
                start: cell,
                end: cell,
                erase,
                select: modifiers.alt,
                started: Instant::now(),
                world_end: local_world,
            });
            return;
        }
        if pressed
            && kind == "Entities"
            && !erase
            && let Some((index, entity)) = hit_entity(project, layer, local)
        {
            let brush_uid = self.entity_uid;
            self.entity_index = Some(index);
            self.entity_uid = entity["defUid"].as_i64();
            self.panel = Some(Panel::Entity);
            self.selection = None;
            self.selection_group = None;
            let origin = [
                entity["px"][0].as_i64().unwrap_or(0) as i32,
                entity["px"][1].as_i64().unwrap_or(0) as i32,
            ];
            if modifiers.command {
                self.gesture = Some(Gesture::EntityResize {
                    key,
                    layer: self.layer,
                    index,
                    start: local,
                    size: [
                        number(entity, "width", 16.0) as i32,
                        number(entity, "height", 16.0) as i32,
                    ],
                    origin,
                    handle: [1, 1],
                });
            } else {
                self.gesture = Some(Gesture::EntityMove {
                    key,
                    layer: self.layer,
                    index,
                    start: local,
                    end: local,
                    origin,
                    moved: false,
                });
            }
            self.try_chain_to(key, self.layer, index, false, brush_uid);
            return;
        }
        if self.stamp.is_some() {
            if pressed && !erase {
                self.paste_at(cell);
            }
            return;
        }
        if kind == "Entities" {
            if erase && (pressed || released || ctx.input(|i| i.pointer.delta().length_sq() > 0.)) {
                self.erase_entities_at_input(ctx, rect, local, pressed);
            } else if pressed {
                self.entity_index = None;
                self.selection = None;
                self.selection_group = None;
                self.edit_entity(local, false);
                self.start_created_entity_transform(key, local);
            }
        } else if self.last_cell != Some(cell) {
            self.paint_cells(&stroke_cells(self.last_cell.unwrap_or(cell), cell), erase);
        }
        self.last_cell = Some(cell);
    }

    pub(super) fn draw_tool_overlay(&mut self, ui: &egui::Ui, rect: Rect) {
        if self.world {
            return;
        }
        if let Some(Gesture::EntityMove {
            key,
            layer,
            index,
            start,
            end,
            origin,
            moved: true,
        }) = &self.gesture
            && let Some(project) = &self.project
            && let Some(instance) = project.layer(*key, *layer)
        {
            let grid = number(instance, "__gridSize", 16.0);
            let a = canvas::layer_to_world(project, *key, *layer, &self.viewport, [0.0; 2]);
            let b = canvas::layer_to_world(project, *key, *layer, &self.viewport, [1.0, 0.0]);
            let position = entity_tools::drag_position(*origin, *start, *end, grid, b[0] - a[0]);
            let offset = [
                number(instance, "__pxTotalOffsetX", 0.0),
                number(instance, "__pxTotalOffsetY", 0.0),
            ];
            let group = json!({"origin":offset,"size":[grid,grid],"layers":[{"layerDefUid":instance["layerDefUid"],"origin":offset,"rect":[0,0,1,1],"points":[],"clipboard":{"gridSize":grid,"width":1,"height":1,"entityInstances":[instance["entityInstances"][*index]],"gridTiles":[],"intGridCsv":[]}}]});
            canvas::draw_selection_ghost(
                ui.ctx(),
                ui.painter(),
                rect,
                project,
                &self.viewport,
                *key,
                &group,
                [
                    (position[0] - origin[0]) as f32,
                    (position[1] - origin[1]) as f32,
                ],
                [false; 2],
                &mut self.assets,
            );
            self.draw_entity_move_arrow(ui, rect);
        }
        if let Some(Gesture::SelectionMove {
            world_start,
            world_end,
            moved: true,
            copy,
            ..
        }) = &self.gesture
            && let (Some(project), Some(key), Some(group)) =
                (&self.project, self.selected, &self.selection_group)
        {
            let delta = self.selection_world_delta(*world_start, *world_end);
            canvas::draw_group_ghost(
                ui.ctx(),
                ui.painter(),
                rect,
                project,
                &self.viewport,
                key,
                group,
                delta.map(|v| v as f32),
                [false; 2],
                if *copy {
                    canvas::GhostMode::Copy
                } else {
                    canvas::GhostMode::Move
                },
                &mut self.assets,
            );
            if group["layers"].as_array().into_iter().flatten().any(|e| {
                ["entityInstances", "gridTiles"]
                    .iter()
                    .any(|n| e["clipboard"][*n].as_array().is_some_and(|a| !a.is_empty()))
                    || e["cellMask"].as_array().is_some_and(|a| !a.is_empty())
            }) {
                let origin = self.selection_arrow_origin(*world_start);
                selection_render::draw_move_arrow(
                    ui,
                    rect,
                    &self.viewport,
                    origin,
                    [origin[0] + delta[0] as f32, origin[1] + delta[1] as f32],
                    *copy,
                );
            }
        }
        let Some(layer) = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
        else {
            return;
        };
        let grid = number(layer, "__gridSize", 16.0);
        let project = self.project.as_ref().unwrap();
        let key = self.selected.unwrap();
        let draw = |r: [i32; 4], color: Color32| {
            let min = self.viewport.to_screen(
                canvas::layer_to_world(
                    project,
                    key,
                    self.layer,
                    &self.viewport,
                    [r[0] as f32 * grid, r[1] as f32 * grid],
                ),
                rect.min.into(),
                rect.size().into(),
            );
            let max = self.viewport.to_screen(
                canvas::layer_to_world(
                    project,
                    key,
                    self.layer,
                    &self.viewport,
                    [(r[0] + r[2]) as f32 * grid, (r[1] + r[3]) as f32 * grid],
                ),
                rect.min.into(),
                rect.size().into(),
            );
            let screen = Rect::from_two_pos(min.into(), max.into());
            ui.painter().with_clip_rect(rect).rect_stroke(
                screen,
                0.0,
                Stroke::new(2.0_f32, color),
                StrokeKind::Inside,
            );
        };
        self.draw_selection_highlight(ui, rect);
        if let Some(Gesture::Rectangle { start, end, .. }) = self.gesture {
            draw(region(start, end), Color32::WHITE);
        }
    }
}

fn timeline_invalidated(before: &Value, after: &Value) -> bool {
    for kind in ["entities", "levelFields", "enums"] {
        if before[kind] != after[kind] {
            return true;
        }
    }
    let old = before["layers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let new = after["layers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if old.len() != new.len() {
        return true;
    }
    for layer in old {
        let Some(next) = new.iter().find(|l| l["uid"] == layer["uid"]) else {
            return true;
        };
        if ["type", "gridSize"]
            .iter()
            .any(|key| layer[*key] != next[*key])
        {
            return true;
        }
        if layer["intGridValues"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| {
                !next["intGridValues"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|n| n["value"] == v["value"])
            })
        {
            return true;
        }
    }
    for kind in ["tilesets", "externalEnums"] {
        for item in before[kind].as_array().into_iter().flatten() {
            let Some(next) = after[kind]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["uid"] == item["uid"])
            else {
                return true;
            };
            if kind == "externalEnums"
                && item["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|value| {
                        !next["values"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|v| v["id"] == value["id"])
                    })
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_rectangle_keeps_inclusive_cell_extent() {
        assert_eq!(region([4, 7], [2, 3]), [2, 3, 3, 5]);
    }
}
