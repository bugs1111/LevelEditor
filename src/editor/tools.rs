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
                    match project.drag_selection_group_with_geometry(
                        key,
                        &group,
                        delta,
                        &geometries,
                        copy,
                        free_entities,
                        now.map(|_| &mut rules),
                    ) {
                        Ok(next) => {
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
                            self.status = "Selection moved".into();
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
            } => {
                if moved && let Some(project) = &self.project {
                    let grid = project
                        .layer(key, layer)
                        .map(|l| number(l, "__gridSize", 16.0))
                        .unwrap_or(16.0);
                    let a = canvas::layer_to_world(project, key, layer, &self.viewport, [0.0; 2]);
                    let b = canvas::layer_to_world(project, key, layer, &self.viewport, [1.0, 0.0]);
                    let position = entity_tools::drag_position(
                        origin,
                        start,
                        end,
                        grid,
                        b[0] - a[0],
                        self.grid,
                    );
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
                }
                self.finish_entity_gesture(key, layer, index);
            }
            Gesture::EntityResize {
                key, layer, index, ..
            } => self.finish_entity_gesture(key, layer, index),
        }
        if pointer_release {
            self.release_stroke();
        } else {
            self.finish_stroke();
        }
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
        self.stroke = false;
        self.last_cell = None;
        self.async_paint = None;
        let pending = self.deferred_auto_paint.take();
        if pending.is_some() {
            self.dirty = self.stroke_was_dirty;
        }
        self.painted_cells.clear();
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
        let Some(drag) = self.camera_drag else {
            return;
        };
        // PanView reads both samples through the current camera, including the
        // pointer position on release.
        let world_point = |pos: egui::Pos2| {
            self.viewport
                .to_world(pos.into(), rect.min.into(), rect.size().into())
                .map(f32::round)
        };
        let from = world_point(drag.previous);
        let to = world_point(pointer);
        let physical_zoom = self.viewport.zoom * ctx.pixels_per_point();
        if drag.button == egui::PointerButton::Secondary {
            let delta = -(to[1] - from[1]) * 0.006 * physical_zoom;
            self.zoom_at_canvas(rect, drag.anchor, delta, ctx.pixels_per_point());
        } else {
            self.viewport.center[0] -= to[0] - from[0];
            self.viewport.center[1] -= to[1] - from[1];
            if !self.world
                && !self.camera_motion.is_active()
                && let (Some(project), Some(key)) = (&self.project, self.selected)
            {
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
            return;
        }
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

    fn begin_selection_move(
        &mut self,
        key: LevelKey,
        region: [i32; 4],
        cell: [i32; 2],
        world: [f32; 2],
        copy: bool,
    ) {
        self.history.begin(self.project.as_ref().unwrap(), key);
        self.last_chance = None;
        self.stroke = true;
        self.gesture = Some(Gesture::SelectionMove {
            region,
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
        world: [f32; 2],
    ) -> Option<(LevelKey, usize, [f32; 2], [i32; 2])> {
        let key = self.selected?;
        let index = self.entity_index?;
        if !self.details || !self.layer_visible(key, self.layer) {
            return None;
        }
        let project = self.project.as_ref()?;
        let local = canvas::world_to_layer(project, key, self.layer, &self.viewport, world);
        let handle = self.resize_handle_at(key, self.layer, index, local, self.viewport.zoom)?;
        Some((key, index, local, handle))
    }

    fn begin_entity_resize(
        &mut self,
        key: LevelKey,
        index: usize,
        local: [f32; 2],
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
            start: local,
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
        if self.world || !self.picking_point() || !response.contains_pointer() {
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
        let world = self
            .viewport
            .to_world(pos.into(), rect.min.into(), rect.size().into());
        if let Some((key, index, local, handle)) = self.selected_entity_resize_handle(world) {
            self.begin_entity_resize(key, index, local, handle);
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
        self.cancel_field_pick();
        if button != egui::PointerButton::Primary {
            return true;
        }
        let region = self
            .selection
            .filter(|_| self.selection_contains_world(world));
        let copy = modifiers.command && modifiers.alt;
        if !modifiers.shift && region.is_none() {
            self.select_element_at(world, copy, true);
        } else if let (Some(project), Some(key)) = (&self.project, self.selected)
            && let Some(layer) = project.layer(key, self.layer)
        {
            let grid = number(layer, "__gridSize", 16.).max(1.);
            let local = canvas::world_to_layer(project, key, self.layer, &self.viewport, world);
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
                    started: Instant::now(),
                    world_end: world,
                });
            } else if let Some(region) = region {
                self.begin_selection_move(key, region, cell, world, copy);
            }
        }
        true
    }

    fn update_world_group_move(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        local_world: [f32; 2],
        modifiers: egui::Modifiers,
    ) -> bool {
        let Some(Gesture::WorldGroupMove {
            anchor,
            press,
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
        *end = release.map_or(local_world, |(pos, _)| {
            self.viewport
                .to_world(pos.into(), rect.min.into(), rect.size().into())
        });
        *disable_snap = release.map_or(modifiers.alt, |(_, modifiers)| modifiers.alt);
        *backing_zoom = self.viewport.zoom * ctx.pixels_per_point();
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
        let to_world = |pos: egui::Pos2| {
            self.viewport
                .to_world(pos.into(), rect.min.into(), rect.size().into())
        };
        let (position, crossed) = world_tools::world_drag_motion(ctx, *press);
        let position = position.map(to_world);
        let release = world_tools::world_pointer_release(ctx).map(|(pos, _)| to_world(pos));
        if let Some(position) = release.or(position) {
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
                        .level_keys()
                        .into_iter()
                        .filter(|other| other.world == key.world)
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
            self.viewport.zoom * ctx.pixels_per_point(),
        );
        if let Some(level) = self.project.as_mut().and_then(|p| p.level_mut(key)) {
            level["worldX"] = json!(x);
            level["worldY"] = json!(y);
            level["worldDepth"] = json!(depth);
        }
        true
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
            || ui_fields::enum_picker::is_open(ctx)
            || self.command_open
            || !self.resource_missing.is_empty()
            || self.crash.is_some()
            || self.clicktrap
            || text_editor::is_open(ctx)
            || world_form_dialog::is_open(ctx)
            || help::changelog_is_open(ctx)
        {
            self.camera_drag = None;
            return;
        }
        if self.fit
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
        let Some(pointer) = pointer else {
            return;
        };
        let local_world =
            self.viewport
                .to_world(pointer.into(), rect.min.into(), rect.size().into());
        if self.gesture.is_none()
            && (self.world || !self.picking_point())
            && (!rect.contains(pointer) || !response.hovered())
        {
            self.last_cell = None;
            return;
        }
        if space {
            return;
        }
        if self.project.as_ref().is_some_and(Project::is_backup) {
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
            if self.update_world_group_move(ctx, rect, local_world, modifiers) {
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
                let start =
                    self.viewport
                        .to_world(press.into(), rect.min.into(), rect.size().into());
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
                        self.update_world_group_move(ctx, rect, local_world, modifiers);
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
        if self.gesture.is_none()
            && pressed
            && kind == "Entities"
            && !modifiers.shift
            && !modifiers.alt
        {
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
                    let (key, layer, index) = (*key, *layer, *index);
                    let geometry = self.project.as_ref().and_then(|p| {
                        let e = &p.layer(key, layer)?["entityInstances"][index];
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
                        let [w, h] = result;
                        let position = json!(resize_tools::resized_entity_origin(
                            d, *origin, *size, result, *handle
                        ));
                        (e["width"] != w || e["height"] != h || e["px"] != position)
                            .then_some((w, h, position))
                    });
                    if let Some((w, h, position)) = geometry {
                        let snapshot = self.capture_entity_render(key, layer);
                        let entity = &mut self
                            .project
                            .as_mut()
                            .unwrap()
                            .layer_mut(key, layer)
                            .unwrap()["entityInstances"][index];
                        entity["width"] = json!(w);
                        entity["height"] = json!(h);
                        entity["px"] = position;
                        self.defer_entity_render(snapshot);
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
            self.begin_entity_resize(key, index, local, handle);
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
                modifiers.command && modifiers.alt,
            );
            return;
        }
        if pressed && primary && modifiers.alt && !modifiers.shift {
            self.select_element_at(local_world, modifiers.command, true);
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
            self.stroke_was_dirty = self.dirty;
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
            let position =
                entity_tools::drag_position(*origin, *start, *end, grid, b[0] - a[0], self.grid);
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
