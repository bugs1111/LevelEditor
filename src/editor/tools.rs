use super::world_tools::WorldCopySnapshot;
use super::*;
use serde_json::json;

#[derive(Clone, Copy)]
pub(super) struct CameraDrag {
    button: egui::PointerButton,
    anchor: egui::Pos2,
    previous: egui::Pos2,
}

pub(super) fn space_secondary_pressed(input: &egui::InputState) -> bool {
    let mut space = input.key_down(Key::Space);
    for event in input.events.iter().rev() {
        match event {
            egui::Event::Key {
                key: Key::Space,
                pressed,
                repeat: false,
                ..
            } => space = !pressed,
            egui::Event::PointerButton {
                button: egui::PointerButton::Secondary,
                pressed: true,
                ..
            } if space => return true,
            _ => {}
        }
    }
    false
}

pub(super) enum Gesture {
    WorldGroupMove {
        anchor: LevelKey,
        press: egui::Pos2,
        clicked_same: bool,
        start: [f32; 2],
        end: [f32; 2],
        origins: Vec<(String, [i32; 2])>,
        moved: bool,
        copy: Option<WorldCopySnapshot>,
        disable_snap: bool,
        backing_zoom: f32,
        deselected_on_press: bool,
    },
    WorldRectangle {
        press: egui::Pos2,
        corners: Option<[[f32; 2]; 2]>,
    },
    Rectangle {
        start: [i32; 2],
        end: [i32; 2],
        erase: bool,
        select: bool,
        entered_level: bool,
        started: Instant,
        world_end: [f32; 2],
    },
    SelectionMove {
        region: [i32; 4],
        press: egui::Pos2,
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
        press: egui::Pos2,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        moved: bool,
    },
    EntityResize {
        key: LevelKey,
        layer: usize,
        index: usize,
        size: [i32; 2],
        origin: [i32; 2],
        handle: [i32; 2],
    },
    LevelMove {
        key: LevelKey,
        press: egui::Pos2,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        depth: i32,
        moved: bool,
        clicked_same: bool,
        copy: Option<WorldCopySnapshot>,
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
    origin: [i32; 2],
    bounds: [i32; 4],
    tile: [i32; 4],
    scale: i32,
) -> Vec<([i32; 2], [i32; 2])> {
    let [left, top, right, bottom] = bounds;
    let start_x = left + (origin[0] - left).rem_euclid(scale);
    let start_y = top + (origin[1] - top).rem_euclid(scale);
    let mut result = Vec::new();
    for y in (start_y..bottom).step_by(scale as usize) {
        for x in (start_x..right).step_by(scale as usize) {
            result.push((
                [x, y],
                [
                    tile[0] + (x - origin[0]) / scale % tile[2],
                    tile[1] + (y - origin[1]) / scale % tile[3],
                ],
            ));
        }
    }
    result
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditHistory {
    Recorded,
    Recoverable,
    Unrecorded,
}

impl EditorApp {
    pub(super) fn edit<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, None, EditHistory::Recorded, |p, _| operation(p))
    }
    pub(super) fn edit_recoverable<T>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, None, EditHistory::Recoverable, |p, _| operation(p))
    }
    pub(super) fn edit_level<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), EditHistory::Recorded, |p, _| operation(p))
    }
    pub(super) fn edit_level_recoverable<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), EditHistory::Recoverable, |p, _| operation(p))
    }
    pub(super) fn edit_level_marked<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project, &mut History) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), EditHistory::Recorded, operation)
    }
    pub(super) fn edit_level_without_undo<T>(
        &mut self,
        label: &str,
        key: LevelKey,
        operation: impl FnOnce(&mut Project) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        self.edit_operation(label, Some(key), EditHistory::Unrecorded, |p, _| operation(p))
    }
    fn edit_operation<T>(
        &mut self,
        label: &str,
        key: Option<LevelKey>,
        history: EditHistory,
        operation: impl FnOnce(&mut Project, &mut History) -> ldtk_core::Result<T>,
    ) -> Option<T> {
        if !self.settle_deferred_auto_paint() {
            return None;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.async_paint = None;
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
        if history != EditHistory::Unrecorded {
            if let Some(key) = key {
                self.history.begin(project, key);
            } else {
                self.history.begin_project(project);
            }
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
                let changed = history == EditHistory::Unrecorded || self.history.commit(project);
                self.dirty |= changed;
                if history == EditHistory::Unrecorded
                    && let Some(level) = key.and_then(|key| project.level(key))
                {
                    self.history.mark_level_modified(level);
                }
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
                    if history == EditHistory::Recoverable {
                        self.last_chance = Some((label.into(), Instant::now()));
                    }
                }
                self.status = label.into();
                self.status_error = false;
                Some(value)
            }
            Err(error) => {
                if history != EditHistory::Unrecorded {
                    self.history.cancel(project);
                }
                self.status = error.to_string();
                self.status_error = true;
                None
            }
        }
    }
    pub(super) fn restore_last_chance(&mut self) {
        let Some((label, _)) = self.last_chance.take() else {
            return;
        };
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
        self.status = format!("Canceled action \"{label}\"");
        self.status_error = false;
    }

    pub(super) fn finish_gesture(&mut self) {
        self.finish_gesture_with_release(false);
    }

    fn finish_gesture_with_release(&mut self, pointer_release: bool) {
        self.finish_level_resize();
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        let Some(key) = self.selected else {
            return;
        };
        match gesture {
            Gesture::WorldRectangle { .. } => {}
            Gesture::WorldGroupMove {
                anchor,
                start,
                end,
                origins,
                moved,
                copy,
                disable_snap,
                backing_zoom,
                ..
            } => {
                if let Some(snapshot) = copy {
                    let delta = self.world_group_release_delta(
                        anchor,
                        start,
                        end,
                        &origins,
                        disable_snap,
                        backing_zoom,
                    );
                    self.finish_world_group_copy(&origins, delta, snapshot);
                    return;
                }
                let linear = self
                    .project
                    .as_ref()
                    .and_then(|p| world_tools::linear_world_axis(p, anchor))
                    .is_some();
                if moved && !linear {
                    self.apply_world_group_move(
                        &origins,
                        start,
                        end,
                        anchor,
                        disable_snap,
                        backing_zoom,
                    );
                }
            }
            Gesture::Rectangle {
                start,
                end,
                erase,
                select,
                entered_level,
                started,
                world_end,
            } => {
                let r = region(start, end);
                if select {
                    if r[2] == 1 && r[3] == 1 {
                        self.append_element_at(world_end);
                    } else {
                        self.capture_selection_rectangle(r, true);
                    }
                    self.finish_stroke();
                    return;
                }
                if !entered_level {
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
                ..
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
                    let free_entities =
                        !self.grid && ldtk_core::selection_group_is_entity_only(&group);
                    let now = self
                        .repaint
                        .as_ref()
                        .filter(|_| pointer_release)
                        .map(|ctx| ctx.input(|i| i.time));
                    let snapshots = now
                        .map(|_| self.capture_selection_render(key, &group))
                        .unwrap_or_default();
                    let Some(project) = self.project.as_mut() else {
                        return;
                    };
                    if project.is_backup() {
                        self.finish_stroke();
                        self.status = "Restore the backup before editing it".into();
                        return;
                    }
                    self.history.begin(project, key);
                    self.stroke_was_dirty = self.dirty;
                    bleep::mark_group(&mut self.history, project, key, &group, false);
                    bleep::mark_moved_group(&mut self.history, project, key, &group, delta);
                    let mut rules = ldtk_core::DeferredAutoLayers::default();
                    match project.drag_selection_group_with_report(
                        key,
                        &group,
                        delta,
                        &geometries,
                        copy,
                        free_entities,
                        now.map(|_| &mut rules),
                    ) {
                        Ok((next, report)) => {
                            if self.panel == Some(Panel::Entity)
                                && group["elementCount"] == 1
                                && let Some(layer) = project.layer(key, self.layer)
                                && group["layers"].as_array().into_iter().flatten().any(|entry| {
                                    entry["layerDefUid"] == layer["layerDefUid"]
                                        && entry["clipboard"]["entityInstances"]
                                            .get(0)
                                            .is_some()
                                })
                            {
                                let selected = next["layers"].as_array().into_iter().flatten()
                                    .find(|entry| entry["layerDefUid"] == layer["layerDefUid"])
                                    .and_then(|entry| entry["clipboard"]["entityInstances"].get(0));
                                self.entity_index = selected.and_then(|entity| {
                                    layer["entityInstances"].as_array()?.iter()
                                        .position(|current| current["iid"] == entity["iid"])
                                });
                                if self.entity_index.is_none() {
                                    self.panel = None;
                                }
                            }
                            if let Some(now) = now
                                && !rules.is_empty()
                            {
                                self.deferred_auto_paint = Some(DeferredAutoPaint {
                                    level: key,
                                    deadline: now + brush::ASYNC_RENDER_DELAY,
                                    rules,
                                    prior_selection: Some(group),
                                });
                                self.dirty = true;
                            } else {
                                self.dirty |= self.history.commit(project);
                            }
                            if let Some(now) = now {
                                self.defer_selection_render(key, now, snapshots);
                            }
                            self.set_selection_group(next);
                            if report.out_of_bounds.is_empty() && report.copy_limits.is_empty() {
                                self.status = "Selection moved".into();
                            } else {
                                self.status.clear();
                                for (maximum, identifier) in report.copy_limits {
                                    self.report_entity_limit(maximum, identifier);
                                }
                                self.report_out_of_bounds(&report.out_of_bounds);
                            }
                            self.status_error = false;
                        }
                        Err(error) => {
                            self.history.cancel(project);
                            self.status = error.to_string();
                            self.status_error = true;
                        }
                    }
                    if pointer_release {
                        self.release_stroke();
                    } else {
                        self.finish_stroke();
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
                origin,
                moved,
                copy,
                ..
            } => {
                if !moved {
                    self.finish_stroke();
                    return;
                }
                if let Some(project) = &mut self.project {
                    let next = if let Some(axis) = world_tools::linear_world_axis(project, key)
                        && let Some((insertion, _)) = world_tools::linear_level_insert_point(
                            project,
                            key,
                            axis,
                            origin[axis],
                            end[axis],
                        ) {
                        let index = insertion - usize::from(insertion > key.level);
                        project.move_level_to_world(key, key.world, index)
                    } else {
                        project.refresh_derived_data().map(|()| key)
                    };
                    match next {
                        Ok(next) => {
                            self.selected = Some(next);
                            if copy.is_some() {
                                self.status = "Duplicate level".into();
                                self.status_error = false;
                                self.remember_selected_level();
                            }
                        }
                        Err(error) => {
                            self.history.cancel(project);
                            if let Some(snapshot) = copy {
                                self.restore_world_copy(snapshot);
                            }
                            self.status = error.to_string();
                            self.status_error = true;
                        }
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
                ..
            } => {
                if moved && let Some(project) = &self.project {
                    let delta = entity_tools::drag_delta(
                        project,
                        key,
                        layer,
                        &self.viewport,
                        [start, end],
                        self.grid,
                    );
                    let position: [i32; 2] =
                        std::array::from_fn(|axis| origin[axis] + delta[axis].trunc() as i32);
                    let position = json!(position);
                    if let Some(entity) = project
                        .layer(key, layer)
                        .and_then(|l| l["entityInstances"].get(index))
                        && entity["px"] != position
                    {
                        let snapshot = self.capture_entity_render(key, layer);
                        let entity = &mut self
                            .project
                            .as_mut()
                            .unwrap()
                            .layer_mut(key, layer)
                            .unwrap()["entityInstances"][index];
                        entity["px"] = position;
                        self.defer_entity_render(snapshot);
                    }
                    self.finish_entity_gesture(key, layer, index);
                }
            }
            Gesture::EntityResize {
                key,
                layer,
                index,
                origin,
                size,
                ..
            } => {
                let resized = self
                    .project
                    .as_ref()
                    .and_then(|project| project.layer(key, layer))
                    .and_then(|layer| layer["entityInstances"].get(index))
                    .is_some_and(|entity| {
                        entity["px"] != json!(origin)
                            || entity["width"] != size[0]
                            || entity["height"] != size[1]
                    });
                if resized {
                    let project = self.project.as_mut().unwrap();
                    let entity = &project.layer(key, layer).unwrap()["entityInstances"][index];
                    self.history.mark_entity_change(project, key, layer, entity);
                    if let Err(error) = project.refresh_derived_data() {
                        self.history.cancel(project);
                        self.async_paint = None;
                        self.status = error.to_string();
                        self.status_error = true;
                    }
                }
            }
        }
        if pointer_release {
            self.release_stroke();
        } else {
            self.finish_stroke();
        }
    }

    fn finish_entity_gesture(&mut self, key: LevelKey, layer: usize, index: usize) {
        if let Some(project) = &mut self.project {
            let name = project
                .layer(key, layer)
                .and_then(|l| l["entityInstances"].get(index))
                .map(|entity| {
                    self.history.mark_entity_change(project, key, layer, entity);
                    text(entity, "__identifier", "").to_owned()
                });
            match project.finish_entity_transform(key, layer, index) {
                Ok(true) => {
                    self.entity_index = None;
                    if self.panel == Some(Panel::Entity) {
                        self.panel = None;
                    }
                    if let Some(name) = name {
                        self.report_out_of_bounds(&[name]);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    self.history.cancel(project);
                    self.async_paint = None;
                    self.status = error.to_string();
                    self.status_error = true;
                }
            }
        }
    }

    pub(super) fn cancel_gesture(&mut self) {
        self.cancel_level_resize();
        let gesture = self.gesture.take();
        self.clear_stroke_controls();
        self.async_paint = None;
        let pending = self.deferred_auto_paint.take();
        if pending.is_some() {
            self.dirty = self.stroke_was_dirty;
        }
        if let Some(project) = &mut self.project {
            self.history.cancel(project);
        }
        if let Some(
            Gesture::WorldGroupMove {
                copy: Some(snapshot),
                ..
            }
            | Gesture::LevelMove {
                copy: Some(snapshot),
                ..
            },
        ) = gesture
        {
            self.restore_world_copy(snapshot);
        }
        if let Some(group) = pending.and_then(|p| p.prior_selection) {
            self.set_selection_group(group);
        }
    }

    pub(super) fn pointer_lifecycle(&mut self, ctx: &egui::Context) -> bool {
        let (lost_focus, pointer_gone, focus) = ctx.input(|input| {
            let mut lost_focus = false;
            let mut pointer_gone = false;
            let mut focus = None;
            for event in &input.events {
                match event {
                    egui::Event::WindowFocused(focused) => {
                        lost_focus |= !focused;
                        focus = Some(*focused);
                    }
                    egui::Event::PointerGone => pointer_gone = true,
                    _ => {}
                }
            }
            (lost_focus, pointer_gone, focus)
        });
        let lost = lost_focus || pointer_gone;
        if lost {
            if lost_focus && self.project.is_some() {
                self.clicktrap = true;
                self.focus_frames = None;
                self.zen_sidebar = chrome::ZenSidebar::default();
                self.space_pressed = None;
                self.space_fit_pending = false;
                if let Some(Gesture::WorldGroupMove {
                    copy: Some(_),
                    disable_snap,
                    ..
                }) = &mut self.gesture
                {
                    *disable_snap = false;
                }
            }
            self.finish_gesture();
            self.finish_stroke();
            self.camera_drag = None;
            ctx.input_mut(|i| i.pointer = Default::default());
        }
        // A rapid blur and refocus can arrive in the same input batch.
        if self.clicktrap && focus == Some(true) {
            self.focus_frames = Some(2);
        }
        lost
    }

    fn camera_drag_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        let Some((mut space, events)) = ctx.input(|i| {
            (self.camera_drag.is_some() || i.pointer.any_pressed()).then(|| {
                let space = i
                    .events
                    .iter()
                    .find_map(|event| match event {
                        egui::Event::Key {
                            key: Key::Space,
                            pressed,
                            repeat: false,
                            ..
                        } => Some(!pressed),
                        _ => None,
                    })
                    .unwrap_or_else(|| i.key_down(Key::Space));
                (space, i.events.clone())
            })
        }) else {
            return false;
        };
        let mut handled = self.camera_drag.is_some();
        for event in events {
            match event {
                egui::Event::Key {
                    key: Key::Space,
                    pressed,
                    ..
                } => space = pressed,
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    ..
                } if (button == egui::PointerButton::Middle
                    || space
                        && matches!(
                            button,
                            egui::PointerButton::Primary | egui::PointerButton::Secondary
                        ))
                    && self.camera_drag.is_none()
                    && !self.stroke
                    && self.gesture.is_none()
                    && response.interact_rect.contains(pos)
                    && ctx
                        .layer_id_at(pos)
                        .is_none_or(|layer| layer == response.layer_id) =>
                {
                    self.camera_drag = Some(CameraDrag {
                        button,
                        anchor: pos,
                        previous: pos,
                    });
                    self.space_dragged = true;
                    handled = true;
                }
                egui::Event::PointerMoved(pos) => self.update_camera_drag(ctx, rect, pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } if self.camera_drag.is_some_and(|drag| drag.button == button) => {
                    self.update_camera_drag(ctx, rect, pos);
                    self.camera_drag = None;
                }
                _ => {}
            }
        }
        if self
            .camera_drag
            .is_some_and(|drag| !ctx.input(|i| i.pointer.button_down(drag.button)))
        {
            self.camera_drag = None;
        }
        if self.camera_drag.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        handled
    }

    fn update_camera_drag(&mut self, ctx: &egui::Context, rect: Rect, pointer: egui::Pos2) {
        let (Some(drag), Some(project), Some(key)) =
            (self.camera_drag, self.project.as_ref(), self.selected)
        else {
            return;
        };
        // Both samples use the current snapped world origin, including on release.
        let density = ctx.pixels_per_point();
        let offset = if self.world {
            [0.; 2]
        } else {
            navigation::level_world_origin(project, key)
        };
        let world_point = |pos: egui::Pos2| {
            self.viewport.pointer_pixel(
                pos.into(),
                rect.min.into(),
                rect.size().into(),
                offset,
                density,
            )
        };
        let from = world_point(drag.previous);
        let to = world_point(pointer);
        let physical_zoom = self.viewport.zoom * density;
        if drag.button == egui::PointerButton::Secondary {
            let delta = -(to[1] - from[1]) * 0.006 * physical_zoom;
            let world_anchor = world_point(drag.anchor);
            let focus = [world_anchor[0] - offset[0], world_anchor[1] - offset[1]];
            let level_origin = if self.world {
                navigation::level_world_origin(project, key)
            } else {
                [0.; 2]
            };
            let anchor = self
                .viewport
                .level_point_page(rect, level_origin, focus, density);
            self.zoom_at_canvas(rect, anchor, delta, density);
        } else {
            self.viewport.center[0] -= to[0] - from[0];
            self.viewport.center[1] -= to[1] - from[1];
            if !self.world && !self.camera_motion.is_active() {
                let bounds = canvas::bounds(project, key, false);
                for (axis, pixels) in [450., 400.].into_iter().enumerate() {
                    let margin = pixels / physical_zoom;
                    self.viewport.center[axis] = self.viewport.center[axis]
                        .clamp(-margin, bounds[axis + 2].max(0.) + margin);
                }
            }
            self.camera_motion.cancel_pan();
        }
        self.space_dragged = true;
        self.camera_drag = Some(CameraDrag {
            previous: pointer,
            ..drag
        });
    }

    pub(super) fn canvas_controls(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) {
        if self.pointer_lifecycle(ctx) {
            self.neighbor_level_press = false;
            return;
        }
        self.neighbor_level_press &= ctx.input(|input| input.pointer.primary_down());
        let released = ctx.input(|i| i.pointer.any_released());
        if !self.stroke
            && response.hovered()
            && ctx.input(|i| i.pointer.any_pressed())
            && !self.settle_deferred_auto_paint()
        {
            return;
        }
        self.canvas_controls_frame(ctx, rect, response);
        if released {
            let enter_level = self.world_click_enters_level(ctx);
            self.finish_gesture_with_release(true);
            if enter_level {
                self.set_world_view(false);
            }
            self.release_stroke();
        }
    }

    fn world_click_enters_level(&self, ctx: &egui::Context) -> bool {
        let press = match &self.gesture {
            Some(
                Gesture::WorldGroupMove {
                    press,
                    clicked_same: true,
                    moved: false,
                    ..
                }
                | Gesture::LevelMove {
                    press,
                    clicked_same: true,
                    moved: false,
                    ..
                },
            ) if self.world => *press,
            _ => return false,
        };
        let threshold = world_tools::world_drag_threshold(ctx);
        !world_tools::world_drag_started(ctx, press)
            && world_tools::world_pointer_release(ctx).is_some_and(|(pos, modifiers)| {
                press.distance(pos) <= threshold
                    && !world_tools::world_selection_modifier(modifiers)
            })
    }

    pub(super) fn begin_selection_move(
        &mut self,
        key: LevelKey,
        region: [i32; 4],
        cell: [i32; 2],
        world: [f32; 2],
        press: egui::Pos2,
        copy: bool,
    ) {
        self.history.begin(self.project.as_ref().unwrap(), key);
        self.last_chance = None;
        self.stroke = true;
        self.gesture = Some(Gesture::SelectionMove {
            region,
            press,
            start: cell,
            end: cell,
            copy,
            world_start: world,
            world_end: world,
            moved: false,
        });
    }

    pub(super) fn selected_entity_resize_handle(
        &self,
        level_pixel: [f32; 2],
    ) -> Option<(LevelKey, usize, [i32; 2])> {
        let key = self.selected?;
        let index = self.entity_index?;
        if !self.details || !self.layer_visible(key, self.layer) {
            return None;
        }
        let handle = self.resize_handle_at(key, self.layer, index, level_pixel)?;
        Some((key, index, handle))
    }

    fn begin_entity_resize(
        &mut self,
        key: LevelKey,
        index: usize,
        handle: [i32; 2],
    ) {
        let project = self.project.as_ref().unwrap();
        let entity = &project.layer(key, self.layer).unwrap()["entityInstances"][index];
        self.history.begin(project, key);
        self.last_chance = None;
        self.stroke = true;
        self.gesture = Some(Gesture::EntityResize {
            key,
            layer: self.layer,
            index,
            origin: std::array::from_fn(|axis| entity["px"][axis].as_i64().unwrap_or(0) as i32),
            size: [
                number(entity, "width", 16.) as i32,
                number(entity, "height", 16.) as i32,
            ],
            handle,
        });
    }

    fn point_picker_tool_press(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        if self.world || self.picking_entity_ref() || !self.picking_point() || !response.contains_pointer() {
            return false;
        }
        let press = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } if rect.contains(*pos)
                    && matches!(
                        button,
                        egui::PointerButton::Primary | egui::PointerButton::Secondary
                    ) =>
                {
                    Some((*pos, *button, *modifiers))
                }
                _ => None,
            })
        });
        let Some((pos, button, modifiers)) = press else {
            return false;
        };
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return false;
        };
        let (world, _) = self.canvas_pointer_pixels(project, key, rect, pos, ctx.pixels_per_point());
        if let Some((key, index, handle)) = self.selected_entity_resize_handle(world) {
            self.begin_entity_resize(key, index, handle);
            return true;
        }
        if button == egui::PointerButton::Primary
            && self.level_resize_input(ctx, rect, response, Some(pos))
        {
            return true;
        }
        if !(modifiers.alt
            || button == egui::PointerButton::Primary
                && self.point_selection_has_priority(world, false))
        {
            return false;
        }
        self.cancel_point_pick();
        if button != egui::PointerButton::Primary {
            return true;
        }
        let region = self
            .selection
            .filter(|_| self.selection_contains_world(world));
        let copy = modifiers.command && modifiers.alt;
        if !modifiers.shift && region.is_none() {
            self.select_element_at(world, copy, Some(pos));
        } else if let (Some(project), Some(key)) = (&self.project, self.selected)
            && let Some(layer) = project.layer(key, self.layer)
        {
            let grid = number(layer, "__gridSize", 16.).max(1.);
            let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, world);
            let cell = local.map(|v| (v / grid).floor() as i32);
            if modifiers.shift {
                self.history.begin(project, key);
                self.last_chance = None;
                self.stroke = true;
                self.gesture = Some(Gesture::Rectangle {
                    start: cell,
                    end: cell,
                    erase: false,
                    select: true,
                    entered_level: true,
                    started: Instant::now(),
                    world_end: world,
                });
            } else if let Some(region) = region {
                self.begin_selection_move(key, region, cell, world, pos, copy);
            }
        }
        true
    }

    fn update_world_group_move(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        pointer: egui::Pos2,
        modifiers: egui::Modifiers,
    ) -> bool {
        let Some(Gesture::WorldGroupMove {
            anchor,
            press,
            start,
            end,
            origins,
            moved,
            disable_snap,
            backing_zoom,
            deselected_on_press,
            ..
        }) = &mut self.gesture
        else {
            return false;
        };
        let release = world_tools::world_pointer_release(ctx);
        let density = ctx.pixels_per_point();
        let to_world = |pos| world_tools::world_pointer_pixel(&self.viewport, rect, pos, density);
        // Coords retains the original page position through camera changes.
        *start = to_world(*press);
        *end = to_world(release.map_or(pointer, |(pos, _)| pos));
        *disable_snap = release.map_or(modifiers.alt, |(_, modifiers)| modifiers.alt);
        *backing_zoom = self.viewport.zoom * density;
        let started = !*moved && world_tools::world_drag_started(ctx, *press);
        if started
            && *deselected_on_press
            && origins.len() > 1
            && let Some(iid) = self
                .project
                .as_ref()
                .and_then(|project| project.level(*anchor))
                .and_then(|level| level["iid"].as_str())
            && let Some(index) = origins.iter().position(|(id, _)| id == iid)
        {
            let origin = origins.remove(index);
            self.world_selection.push(origin.0.clone());
            origins.push(origin);
        }
        *moved |= started;
        if started && modifiers.alt && world_tools::world_selection_modifier(modifiers) {
            self.start_world_group_copy();
        }
        true
    }

    fn update_world_level_move(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        modifiers: egui::Modifiers,
    ) -> bool {
        let Some(Gesture::LevelMove {
            key,
            press,
            start,
            end,
            origin,
            depth,
            moved,
            copy,
            ..
        }) = &mut self.gesture
        else {
            return false;
        };
        let density = ctx.pixels_per_point();
        let to_world = |pos| world_tools::world_pointer_pixel(&self.viewport, rect, pos, density);
        let (position, crossed) = world_tools::world_drag_motion(ctx, *press);
        let position = position.map(to_world);
        let release = world_tools::world_pointer_release(ctx).map(|(pos, _)| to_world(pos));
        if let Some(position) = release.or(position) {
            *start = to_world(*press);
            *end = position;
        }
        let (Some(position), Some(project)) = (position, self.project.as_mut()) else {
            return true;
        };
        let linear = world_tools::linear_world_axis(project, *key).is_some();
        if !*moved {
            if !crossed
                || linear
                    && project
                        .level_keys_in_world(key.world)
                        .nth(1)
                        .is_none()
            {
                return true;
            }
            self.history.begin_project(project);
            if modifiers.alt && world_tools::world_selection_modifier(modifiers) {
                match world_tools::duplicate_world_level(project, *key) {
                    Ok(next) => {
                        *copy = Some(WorldCopySnapshot {
                            selected: self.selected,
                            selection: std::mem::take(&mut self.world_selection),
                            dirty: self.dirty,
                            last_chance: self.last_chance.take(),
                        });
                        self.selected = Some(next);
                        self.dirty = true;
                        *key = next;
                    }
                    Err(error) => {
                        self.history.cancel(project);
                        self.gesture = None;
                        self.status = error.to_string();
                        self.status_error = true;
                        return true;
                    }
                }
            } else {
                self.last_chance = None;
            }
            self.stroke = true;
            *moved = true;
        }
        if linear {
            return true;
        }
        let (key, start, origin, depth) = (*key, *start, *origin, *depth);
        let [x, y] = self.world_drag_position(
            key,
            start,
            position,
            origin,
            modifiers.alt,
            self.viewport.zoom * density,
        );
        if let Some(level) = self.project.as_mut().and_then(|p| p.level_mut(key)) {
            level["worldX"] = json!(x);
            level["worldY"] = json!(y);
            level["worldDepth"] = json!(depth);
        }
        true
    }

    pub(super) fn canvas_input_blocked(&self, ctx: &egui::Context) -> bool {
        self.project.is_none()
        || self.pending.is_some()
        || self.suggest_backups
        || self.is_busy()
        || self.command_palette
        || ui_fields::enum_picker::is_open(ctx)
        || self.command_open
        || !self.resource_missing.is_empty()
        || self.crash.is_some()
        || self.clicktrap
        || text_editor::is_open(ctx)
        || world_form_dialog::is_open(ctx)
        || panels::enum_value_removal_open(ctx)
        || help::changelog_is_open(ctx)
    }

    fn canvas_controls_frame(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) {
        if self.canvas_input_blocked(ctx) {
            self.camera_drag = None;
            return;
        }
        #[cfg(target_os = "linux")]
        let fitting_ready = self.startup_window.is_none();
        #[cfg(not(target_os = "linux"))]
        let fitting_ready = true;
        if self.fit && fitting_ready
            && let (Some(project), Some(key)) = (&self.project, self.selected)
        {
            self.camera_motion.cancel();
            let bounds = canvas::bounds(project, key, self.world);
            if self.world {
                self.viewport
                    .fit_world(bounds, rect.size().into(), ctx.pixels_per_point());
            } else {
                self.viewport
                    .fit(bounds, rect.size().into(), ctx.pixels_per_point());
            }
            self.fit = false;
        }
        if self.picking_point_in_level_panel() {
            if !self.project.as_ref().is_some_and(Project::is_backup) {
                self.field_pick_input(ctx, rect, response);
            }
            return;
        }
        if self
            .panel
            .is_some_and(|panel| !matches!(panel, Panel::Entity | Panel::World))
        {
            self.camera_drag = None;
            return;
        }
        if !self.picking_entity_ref()
            && (self.world || !self.picking_point() || self.level_resize.is_some())
            && self.level_resize_input(ctx, rect, response, None)
        {
            return;
        }
        if response.hovered()
            && let Some(pointer) = ctx.pointer_hover_pos()
        {
            self.wheel_zoom(ctx, rect, pointer);
        }
        if self.camera_drag_input(ctx, rect, response) {
            return;
        }
        let (pointer, space, modifiers, primary, secondary, pressed, released) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.key_down(Key::Space),
                i.modifiers,
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.any_pressed(),
                i.pointer.any_released(),
            )
        });
        let pointer = if matches!(
            self.gesture,
            Some(Gesture::EntityMove { .. } | Gesture::SelectionMove { .. })
        ) {
            ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
            })
            .or(pointer)
        } else {
            pointer
        };
        let Some(pointer) = pointer else {
            return;
        };
        let captured_eraser = self.stroke && self.stroke_erase && self.world_context.menu_open();
        if self.gesture.is_none()
            && (self.world || !self.picking_point())
            && (!rect.contains(pointer) || !response.hovered() && !captured_eraser)
        {
            self.last_cell = None;
            return;
        }
        if space {
            return;
        }
        if self.project.as_ref().is_some_and(Project::is_backup) {
            if pressed
                && response.interact_rect.contains(pointer)
                && ctx
                    .layer_id_at(pointer)
                    .is_none_or(|layer| layer == response.layer_id)
            {
                self.report_canvas_notice(notifications::CanvasNotice::CannotEditBackup);
            }
            return;
        }
        if self.gesture.is_none()
            && self.point_picker_tool_press(ctx, rect, response)
            && self.gesture.is_none()
        {
            return;
        }
        if !(matches!(self.gesture, Some(Gesture::EntityResize { .. }))
            || self.world && self.gesture.is_some())
            && self.field_pick_input(ctx, rect, response)
        {
            return;
        }
        if self.world {
            if ctx.input(|i| i.pointer.primary_pressed()) {
                self.camera_motion.cancel();
            }
            if self.world_marquee_input(ctx, rect, response) {
                return;
            }
            if self.update_world_group_move(ctx, rect, pointer, modifiers) {
                return;
            }
            if self.update_world_level_move(ctx, rect, modifiers) {
                return;
            }
            if let Some((press, press_modifiers)) = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    } => Some((*pos, *modifiers)),
                    _ => None,
                })
            }) && response.interact_rect.contains(press)
                && ctx
                    .layer_id_at(press)
                    .is_none_or(|layer| layer == response.layer_id)
                && !press_modifiers.shift
                && let (Some(project), Some(key)) = (&self.project, self.selected)
            {
                let start = world_tools::world_pointer_pixel(
                    &self.viewport,
                    rect,
                    press,
                    ctx.pixels_per_point(),
                );
                let hit = self.level_at_world(start).and_then(|candidate| {
                    canvas::placements(project, key)
                        .into_iter()
                        .find(|(k, _)| *k == candidate)
                });
                if let Some((candidate, b)) = hit {
                    let clicked_same = self.selected == Some(candidate);
                    let additive = world_tools::world_selection_modifier(press_modifiers);
                    let was_selected = self.world_selection_contains(candidate);
                    let deselected_on_press = additive && was_selected;
                    let before_toggle = deselected_on_press.then(|| self.world_group_origins());
                    if additive {
                        self.world_toggle_selection(candidate);
                    } else if !was_selected {
                        self.world_selection.clear();
                    }
                    if !self.world_selection.is_empty() {
                        let origins = before_toggle.unwrap_or_else(|| self.world_group_origins());
                        self.gesture = Some(Gesture::WorldGroupMove {
                            anchor: candidate,
                            press,
                            clicked_same,
                            start,
                            end: start,
                            origins,
                            moved: false,
                            copy: None,
                            disable_snap: press_modifiers.alt,
                            backing_zoom: self.viewport.zoom * ctx.pixels_per_point(),
                            deselected_on_press,
                        });
                        self.update_world_group_move(ctx, rect, pointer, modifiers);
                        return;
                    }
                    self.navigate_level(candidate, false);
                    let project = self.project.as_ref().unwrap();
                    let depth = project
                        .level(candidate)
                        .map(|l| number(l, "worldDepth", 0.0) as i32)
                        .unwrap_or(0);
                    self.gesture = Some(Gesture::LevelMove {
                        key: candidate,
                        press,
                        start,
                        end: start,
                        origin: [b[0] as i32, b[1] as i32],
                        depth,
                        moved: false,
                        clicked_same,
                        copy: None,
                    });
                    self.update_world_level_move(ctx, rect, modifiers);
                }
            }
            return;
        }
        if self.gesture.is_none()
            && !self.stroke
            && let Some(press) = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    } if !(cfg!(target_os = "macos") && modifiers.ctrl) => Some(*pos),
                    _ => None,
                })
            })
            && response.interact_rect.contains(press)
            && ctx
                .layer_id_at(press)
                .is_none_or(|layer| layer == response.layer_id)
            && let Some(neighbor) = self.level_under_pointer(ctx, rect, press)
        {
            self.neighbor_level_press = primary;
            self.navigate_neighbor_level(ctx, neighbor);
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let project = self.project.as_ref().unwrap();
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let (local_world, _) =
            self.canvas_pointer_pixels(project, key, rect, pointer, ctx.pixels_per_point());
        if !self.canvas_layer_visible(key, self.layer) {
            if pressed && primary && modifiers.alt && !modifiers.shift {
                self.select_element_at(local_world, modifiers.command, Some(pointer));
            } else {
                self.finish_gesture();
                self.finish_stroke();
            }
            return;
        }
        let grid = number(layer, "__gridSize", 16.0).max(1.0);
        let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, local_world);
        let cell = [
            (local[0] / grid).floor() as i32,
            (local[1] / grid).floor() as i32,
        ];
        let width = number(layer, "__cWid", 0.0) as i32;
        let height = number(layer, "__cHei", 0.0) as i32;
        let kind = text(layer, "__type", "").to_owned();
        let resize_positions: Vec<_> = if matches!(self.gesture, Some(Gesture::EntityResize { .. })) {
            let density = ctx.pixels_per_point();
            ctx.input(|input| {
                input
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::PointerMoved(pos) => Some(
                            self.canvas_pointer_pixels(project, key, rect, *pos, density)
                                .0,
                        ),
                        _ => None,
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };
        let rectangle_entered = matches!(
            self.gesture,
            Some(Gesture::Rectangle { entered_level: false, .. })
        ) && (entity_tools::placement_in_level(project, key, local_world)
            || ctx.input(|input| {
                input.events.iter().any(|event| {
                    let egui::Event::PointerMoved(pos) = event else {
                        return false;
                    };
                    let (pixel, _) =
                        self.canvas_pointer_pixels(project, key, rect, *pos, ctx.pixels_per_point());
                    entity_tools::placement_in_level(project, key, pixel)
                })
            }));
        if let Some(gesture) = &mut self.gesture {
            match gesture {
                Gesture::Rectangle {
                    end,
                    world_end,
                    entered_level,
                    ..
                } => {
                    *world_end = local_world;
                    *end = cell;
                    *entered_level |= rectangle_entered;
                }
                Gesture::SelectionMove {
                    end,
                    press,
                    world_end,
                    moved,
                    ..
                } => {
                    *end = cell;
                    *world_end = local_world;
                    *moved = *moved || selection_tools::selection_drag_started(ctx, *press);
                }
                Gesture::EntityMove {
                    press,
                    end,
                    moved,
                    ..
                } => {
                    *end = local_world;
                    *moved = *moved || selection_tools::selection_drag_started(ctx, *press);
                }
                Gesture::EntityResize {
                    key,
                    layer,
                    index,
                    handle,
                    ..
                } => {
                    let (key, layer, index) = (*key, *layer, *index);
                    let handle = *handle;
                    for cursor in resize_positions {
                        let geometry = self.project.as_ref().and_then(|p| {
                            let entity = &p.layer(key, layer)?["entityInstances"][index];
                            let def = p.json["defs"]["entities"]
                                .as_array()?
                                .iter()
                                .find(|def| def["uid"] == entity["defUid"])?;
                            let size = [
                                number(entity, "width", 16.) as i32,
                                number(entity, "height", 16.) as i32,
                            ];
                            let origin = std::array::from_fn(|axis| {
                                entity["px"][axis].as_i64().unwrap_or(0) as i32
                            });
                            let requested =
                                resize_tools::requested_entity_size(def, entity, cursor, handle);
                            let result = ldtk_core::constrained_entity_size(
                                def,
                                requested,
                                size,
                                self.grid.then_some(grid as i32),
                            );
                            let position =
                                resize_tools::resized_entity_origin(def, origin, size, result, handle);
                            (size != result || origin != position).then_some((result, position))
                        });
                        if let Some(([width, height], position)) = geometry {
                            let snapshot = self.capture_entity_render(key, layer);
                            let entity = &mut self
                                .project
                                .as_mut()
                                .unwrap()
                                .layer_mut(key, layer)
                                .unwrap()["entityInstances"][index];
                            entity["width"] = json!(width);
                            entity["height"] = json!(height);
                            entity["px"] = json!(position);
                            self.defer_entity_render(snapshot);
                        }
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
            && let Some(index) = self.entity_index
            && let Some(handle) =
                self.resize_handle_at(key, self.layer, index, local_world)
        {
            self.begin_entity_resize(key, index, handle);
            return;
        }
        if pressed
            && primary
            && !modifiers.shift
            && self.selection_contains_world(local_world)
            && let Some(region) = self.selection
        {
            self.begin_selection_move(
                key,
                region,
                cell,
                local_world,
                pointer,
                modifiers.command && modifiers.alt,
            );
            return;
        }
        if pressed && primary && modifiers.alt && !modifiers.shift {
            self.select_element_at(local_world, modifiers.command, Some(pointer));
            return;
        }
        if kind == "AutoLayer"
            && !(modifiers.alt && modifiers.shift
                || primary
                    && self
                        .stamp
                        .as_ref()
                        .is_some_and(|stamp| stamp["type"] == "SelectionGroup"))
        {
            return;
        }
        let erase = if released && self.stroke {
            self.stroke_erase
        } else {
            secondary || (cfg!(target_os = "macos") && modifiers.ctrl)
        };
        let entity_hit = if pressed && primary && !erase && kind == "Entities" {
            self.generic_element_at(local_world, true)
        } else {
            None
        };
        if !modifiers.shift
            && matches!(entity_hit, Some((_, _, selection_click::Element::Point { .. })))
        {
            self.select_element_at(local_world, false, Some(pointer));
            return;
        }
        let entity_marquee = modifiers.shift
            && pressed
            && primary
            && !erase
            && kind == "Entities"
            && (entity_hit.is_some() || self.selection_contains_world(local_world));
        let entity_brush = kind == "Entities" && !modifiers.alt && self.stamp.is_none();
        let in_level = entity_tools::placement_in_level(project, key, local_world);
        let entity_placement = entity_brush && pressed && !erase && in_level;
        let grid_tool = matches!(kind.as_str(), "IntGrid" | "Tiles");
        let grid_stroke = grid_tool && self.stamp.is_none();
        let paint_rectangle = grid_tool && modifiers.shift;
        if !(grid_stroke || paint_rectangle || entity_placement || entity_brush && erase)
            && (cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height)
        {
            self.last_cell = None;
            return;
        }
        if !(primary || secondary || (released && self.stroke)) || modifiers.alt && !modifiers.shift
        {
            return;
        }
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
            if !modifiers.alt && !entity_marquee {
                self.selection = None;
                self.selection_group = None;
                if kind != "Entities" {
                    self.entity_index = None;
                    if self.panel == Some(Panel::Entity) {
                        self.panel = None;
                    }
                }
            }
            self.stroke_was_dirty = self.dirty;
            self.stroke = true;
            self.stroke_erase = erase;
            self.stroke_waiting_for_level = grid_stroke && !paint_rectangle && !in_level;
        }
        if pressed && modifiers.shift && (!entity_brush || entity_marquee) {
            let select = modifiers.alt || entity_marquee;
            self.gesture = Some(Gesture::Rectangle {
                start: cell,
                end: cell,
                erase,
                select,
                entered_level: select || in_level,
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
            self.gesture = Some(Gesture::EntityMove {
                key,
                layer: self.layer,
                index,
                press: pointer,
                start: local_world,
                end: local_world,
                origin,
                moved: false,
            });
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
            } else if entity_placement {
                self.entity_index = None;
                self.selection = None;
                self.selection_group = None;
                self.place_entity(local, local_world);
                self.start_created_entity_transform(
                    key,
                    pointer,
                    local_world,
                    modifiers.shift.then_some(cell),
                );
            }
        } else {
            // The source retains the last outside sample and begins painting
            // when the pointer first moves into the level.
            let was_waiting = self.stroke_waiting_for_level;
            self.stroke_waiting_for_level &= !in_level;
            if !self.stroke_waiting_for_level && (was_waiting || self.last_cell != Some(cell)) {
                let cells = stroke_cells(self.last_cell.unwrap_or(cell), cell);
                self.paint_cells(&cells, self.stroke_erase);
            }
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
            moved: true,
            ..
        }) = &self.gesture
            && let Some(project) = &self.project
            && let Some(instance) = project.layer(*key, *layer)
        {
            let grid = number(instance, "__gridSize", 16.0);
            let delta = entity_tools::drag_delta(
                project,
                *key,
                *layer,
                &self.viewport,
                [*start, *end],
                self.grid,
            );
            let offset = [
                number(instance, "__pxTotalOffsetX", 0.0),
                number(instance, "__pxTotalOffsetY", 0.0),
            ];
            let group = json!({"origin":offset,"size":[grid,grid],"layers":[{"layerDefUid":instance["layerDefUid"],"origin":offset,"rect":[0,0,1,1],"points":[],"clipboard":{"gridSize":grid,"width":1,"height":1,"entityInstances":[instance["entityInstances"][*index]],"gridTiles":[],"intGridCsv":[]}}]});
            canvas::draw_group_ghost(
                ui.ctx(),
                ui.painter(),
                rect,
                project,
                &self.viewport,
                *key,
                &group,
                delta,
                [false; 2],
                canvas::GhostMode::EntityDrag,
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
        self.draw_selection_highlight(ui, rect);
        let (
            Some(Gesture::Rectangle { start, end, select, entered_level: true, .. }),
            Some(project),
            Some(key),
        ) = (&self.gesture, &self.project, self.selected)
        else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let color = if *select {
            Color32::WHITE
        } else {
            match layer["__type"].as_str() {
                Some("Tiles") => Color32::BLACK,
                Some("IntGrid") => layer["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid))
                    .and_then(|def| {
                        canvas::values(&def["intGridValues"])
                            .iter()
                            .find(|value| value["value"] == self.int_value)
                    })
                    .and_then(|value| value["color"].as_str())
                    .map_or(Color32::BLACK, theme::color),
                _ => return,
            }
        };
        let grid = number(layer, "__gridSize", 16.0);
        let origin = self.viewport.level_render_canvas_origin(rect, ui.ctx().pixels_per_point());
        let [x, y, width, height] = region(*start, *end);
        let to_screen = |cell: [i32; 2]| {
            self.viewport.to_screen(
                canvas::layer_to_pointer_world(
                    project,
                    key,
                    self.layer,
                    &self.viewport,
                    cell.map(|coordinate| coordinate as f32 * grid),
                ),
                origin,
                rect.size().into(),
            )
        };
        let screen = Rect::from_two_pos(
            to_screen([x, y]).into(),
            to_screen([x + width, y + height]).into(),
        );
        let scale = screen.width() / (width as f32 * grid);
        preview::paint_pick_grid(&ui.painter().with_clip_rect(rect), screen, scale, color, true);
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

    #[test]
    fn int_grid_rectangle_preview_uses_the_palette_color_for_fill_and_edges() {
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.json["defs"]["layers"][0]["intGridValues"] =
            json!([{"value":1,"identifier":"Solid","color":"#759944","groupUid":0}]);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.int_value = 1;
        app.viewport.center = [128.; 2];
        app.viewport.zoom = 2.;
        app.gesture = Some(Gesture::Rectangle {
            start: [2, 2],
            end: [4, 4],
            erase: false,
            select: false,
            entered_level: true,
            started: Instant::now(),
            world_end: [64.; 2],
        });
        let ctx = egui::Context::default();
        let canvas = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512., 512.));
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.draw_tool_overlay(ui, canvas));
        });
        let color = theme::color("#759944");
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(shape)
                if shape.fill == color.gamma_multiply(0.35)
                && shape.rect == Rect::from_min_max(egui::pos2(64., 64.), egui::pos2(160., 160.)))
        }));
        for expected in [Color32::BLACK, color] {
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.vertices.len() == 8
                    && mesh.vertices.iter().all(|vertex| vertex.color == expected))
            }));
        }
    }

    #[test]
    fn selection_rectangles_accumulate_without_selecting_gaps_or_duplicates() {
        let mut project = Project::new();
        let key = LevelKey {
            world: None,
            level: 0,
        };
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        let width = project.layer(key, 0).unwrap()["__cWid"].as_u64().unwrap() as usize;
        for x in [1, 4, 7] {
            project.layer_mut(key, 0).unwrap()["intGridCsv"][width + x] = 1.into();
        }
        let mut app = EditorApp::empty();
        app.set_project(project);
        for (start, end) in [([1, 1], [2, 1]), ([6, 1], [7, 1]), ([7, 1], [8, 1])] {
            app.gesture = Some(Gesture::Rectangle {
                start,
                end,
                erase: false,
                select: true,
                entered_level: true,
                started: Instant::now(),
                world_end: [end[0] as f32 * 16. + 8., 24.],
            });
            app.finish_gesture();
        }
        let group = app.selection_group.as_ref().unwrap();
        assert_eq!(group["elementCount"], 2);
        let project = app.project.as_mut().unwrap();
        project.delete_selection_group(key, group).unwrap();
        let csv = &project.layer(key, 0).unwrap()["intGridCsv"];
        assert_eq!(csv[width + 1], 0);
        assert_eq!(csv[width + 4], 1);
        assert_eq!(csv[width + 7], 0);
    }

    #[test]
    fn selection_rectangle_single_cell_uses_other_layers_even_in_single_layer_mode() {
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let uid = project.add_entity_def("Actor", 16, 16).unwrap();
        project.add_entity(key, 0, uid, 88, 88).unwrap();
        let width = project.layer(key, 1).unwrap()["__cWid"].as_u64().unwrap() as usize;
        project.layer_mut(key, 1).unwrap()["intGridCsv"][width + 1] = 1.into();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.layer = 1;
        app.single_layer = true;

        for (start, end, count) in [
            ([1, 1], [2, 1], 1),
            ([4, 5], [5, 5], 1),
            ([5, 5], [5, 5], 2),
            ([8, 8], [8, 8], 0),
        ] {
            app.gesture = Some(Gesture::Rectangle {
                start,
                end,
                erase: false,
                select: true,
                entered_level: true,
                started: Instant::now(),
                world_end: end.map(|cell| cell as f32 * 16. + 8.),
            });
            app.finish_gesture();
            assert_eq!(app.layer, 1);
            let actual = app
                .selection_group
                .as_ref()
                .map_or(0, |group| group["elementCount"].as_u64().unwrap());
            assert_eq!(actual, count);
        }
    }
}
