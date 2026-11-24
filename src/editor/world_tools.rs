use super::*;
use serde_json::json;

type WorldGroupOrigins = Vec<(String, [i32; 2])>;

pub(super) struct WorldCopySnapshot {
    pub(super) selected: Option<LevelKey>,
    pub(super) selection: Vec<String>,
    pub(super) dirty: bool,
    pub(super) last_chance: Option<(String, Instant)>,
}

pub(super) fn world_selection_modifier(modifiers: egui::Modifiers) -> bool {
    modifiers.command || (!cfg!(target_os = "macos") && modifiers.ctrl)
}

pub(super) fn world_drag_threshold(ctx: &egui::Context) -> f32 {
    // WorldTool measures the press-to-pointer distance in page coordinates.
    8.0 * ctx.pixels_per_point()
}

pub(super) fn world_pointer_pixel(
    viewport: &Viewport,
    rect: Rect,
    pos: egui::Pos2,
    density: f32,
) -> [f32; 2] {
    viewport.pointer_pixel(
        pos.into(),
        rect.min.into(),
        rect.size().into(),
        [0.; 2],
        density,
    )
}

pub(super) fn world_pointer_release(ctx: &egui::Context) -> Option<(egui::Pos2, egui::Modifiers)> {
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                pressed: false,
                modifiers,
                ..
            } => Some((*pos, *modifiers)),
            _ => None,
        })
    })
}

pub(super) fn world_drag_started(ctx: &egui::Context, press: egui::Pos2) -> bool {
    world_drag_motion(ctx, press).1
}

pub(super) fn world_drag_motion(
    ctx: &egui::Context,
    press: egui::Pos2,
) -> (Option<egui::Pos2>, bool) {
    let threshold = world_drag_threshold(ctx);
    ctx.input(|i| {
        let mut last = None;
        let mut crossed = false;
        for event in &i.events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } if *pos == press => {
                    last = None;
                    crossed = false;
                }
                egui::Event::PointerButton { pressed: false, .. } => break,
                egui::Event::PointerMoved(pos) => {
                    last = Some(*pos);
                    crossed |= press.distance(*pos) >= threshold;
                }
                _ => {}
            }
        }
        (last, crossed)
    })
}

fn world_drag_delta(start: [f32; 2], end: [f32; 2]) -> [i32; 2] {
    std::array::from_fn(|axis| end[axis].round() as i32 - start[axis].round() as i32)
}

fn bounds_distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    ((a[0] - b[2]).max(0.) + (b[0] - a[2]).max(0.))
        .max((a[1] - b[3]).max(0.) + (b[1] - a[3]).max(0.))
}
fn bounds_at(position: [i32; 2], size: [i32; 2]) -> [f32; 4] {
    [
        position[0] as f32,
        position[1] as f32,
        (position[0] + size[0]) as f32,
        (position[1] + size[1]) as f32,
    ]
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn require_level(project: &Project, iid: &str) -> ldtk_core::Result<LevelKey> {
    project
        .level_by_iid(iid)
        .map(|(key, _)| key)
        .ok_or_else(|| ldtk_core::Error::Invalid("selected level no longer exists".into()))
}

pub(super) fn duplicate_world_level(
    project: &mut Project,
    key: LevelKey,
) -> ldtk_core::Result<LevelKey> {
    let source = project
        .level(key)
        .ok_or_else(|| ldtk_core::Error::Invalid("selected level no longer exists".into()))?;
    let name = panels::unique_name(
        &project.cleanup_identifier(text(source, "identifier", "Level")),
        project
            .level_keys()
            .into_iter()
            .filter_map(|key| project.level(key)?["identifier"].as_str()),
    );
    let auto = source["useAutoIdentifier"].clone();
    let copy = project.duplicate_level(key, &name)?;
    let linear = linear_world_axis(project, copy).is_some();
    let level = project.level_mut(copy).unwrap();
    level["useAutoIdentifier"] = auto;
    if linear {
        level["worldX"] = json!(-1);
        level["worldY"] = json!(-1);
    }
    Ok(copy)
}

fn copy_world_group(
    project: &mut Project,
    origins: &[(String, [i32; 2])],
    anchor: LevelKey,
) -> ldtk_core::Result<(LevelKey, WorldGroupOrigins)> {
    let anchor_iid = project
        .level(anchor)
        .and_then(|level| level["iid"].as_str())
        .ok_or_else(|| ldtk_core::Error::Invalid("selected level no longer exists".into()))?
        .to_owned();
    let mut copies = Vec::with_capacity(origins.len());
    let mut copy_anchor = None;
    for (iid, origin) in origins {
        let key = require_level(project, iid)?;
        let copy = duplicate_world_level(project, key)?;
        let copy_iid = project.level(copy).unwrap()["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        if *iid == anchor_iid {
            copy_anchor = Some(copy_iid.clone());
        }
        copies.push((copy_iid, *origin));
    }
    let copy_anchor = require_level(project, copy_anchor.as_deref().unwrap_or_default())?;
    Ok((copy_anchor, copies))
}

fn place_world_group(
    project: &mut Project,
    origins: &[(String, [i32; 2])],
    delta: [i32; 2],
) -> ldtk_core::Result<()> {
    for (iid, origin) in origins {
        let key = require_level(project, iid)?;
        let level = project.level_mut(key).unwrap();
        level["worldX"] = json!(origin[0] + delta[0]);
        level["worldY"] = json!(origin[1] + delta[1]);
    }
    project.refresh_derived_data()
}

pub(super) fn linear_world_axis(project: &Project, key: LevelKey) -> Option<usize> {
    let world = key
        .world
        .and_then(|index| project.json["worlds"].get(index))
        .unwrap_or(&project.json);
    match world["worldLayout"].as_str() {
        Some("LinearHorizontal") => Some(0),
        Some("LinearVertical") => Some(1),
        _ => None,
    }
}

pub(super) fn linear_level_insert_point(
    project: &Project,
    moved: LevelKey,
    axis: usize,
    original: i32,
    end: f32,
) -> Option<(usize, f32)> {
    let placements = canvas::placements(project, moved);
    let (last, last_bounds) = placements.last()?;
    let trailing = (*last != moved).then_some((last.level + 1, last_bounds[axis + 2]));
    let end = end.round();
    placements
        .iter()
        .map(|(key, bounds)| {
            (
                key.level,
                if *key == moved {
                    original as f32
                } else {
                    bounds[axis]
                },
            )
        })
        .chain(trailing)
        .min_by(|a, b| (a.1 - end).abs().total_cmp(&(b.1 - end).abs()))
}

impl EditorApp {
    pub(super) fn linear_drag_preview(&self) -> Option<(LevelKey, [i32; 2])> {
        let Some(tools::Gesture::LevelMove {
            key,
            start,
            end,
            origin,
            moved: true,
            ..
        }) = &self.gesture
        else {
            return None;
        };
        linear_world_axis(self.project.as_ref()?, *key)?;
        Some((
            *key,
            self.world_drag_position(*key, *start, *end, *origin, false, 1.),
        ))
    }

    pub(super) fn start_world_group_copy(&mut self) {
        let (
            Some(project),
            Some(tools::Gesture::WorldGroupMove {
                anchor,
                origins,
                copy,
                ..
            }),
        ) = (&mut self.project, &mut self.gesture)
        else {
            return;
        };
        if linear_world_axis(project, *anchor).is_some() {
            return;
        }
        self.history.begin_project(project);
        match copy_world_group(project, origins, *anchor) {
            Ok((level, copies)) => {
                *copy = Some(WorldCopySnapshot {
                    selected: self.selected,
                    selection: std::mem::take(&mut self.world_selection),
                    dirty: self.dirty,
                    last_chance: self.last_chance.take(),
                });
                self.selected = Some(level);
                self.world_selection = copies.iter().map(|(iid, _)| iid.clone()).collect();
                *anchor = level;
                *origins = copies;
                self.dirty = true;
            }
            Err(error) => {
                self.history.cancel(project);
                self.gesture = None;
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }

    pub(super) fn restore_world_copy(&mut self, snapshot: WorldCopySnapshot) {
        self.selected = snapshot.selected;
        self.world_selection = snapshot.selection;
        self.dirty = snapshot.dirty;
        self.last_chance = snapshot.last_chance;
    }

    pub(super) fn finish_world_group_copy(
        &mut self,
        origins: &[(String, [i32; 2])],
        delta: [i32; 2],
        snapshot: WorldCopySnapshot,
    ) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        match place_world_group(project, origins, delta) {
            Ok(()) => {
                self.history.commit(project);
                self.status = "Duplicate levels".into();
                self.status_error = false;
                self.remember_selected_level();
            }
            Err(error) => {
                self.history.cancel(project);
                self.restore_world_copy(snapshot);
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }

    pub(super) fn world_toggle_selection(&mut self, key: LevelKey) {
        let Some(iid) = self
            .project
            .as_ref()
            .and_then(|p| p.level(key))
            .and_then(|l| l["iid"].as_str())
            .map(str::to_owned)
        else {
            return;
        };
        if self.world_selection.contains(&iid) {
            self.world_selection.retain(|item| item != &iid);
        } else {
            self.world_selection.push(iid);
        }
    }

    pub(super) fn world_group_origins(&self) -> Vec<(String, [i32; 2])> {
        let (Some(project), Some(selected)) = (&self.project, self.selected) else {
            return Vec::new();
        };
        let levels = project
            .level_keys_in_world(selected.world)
            .filter_map(|key| {
                let level = project.level(key)?;
                Some((level["iid"].as_str()?, level))
            })
            .collect::<std::collections::HashMap<_, _>>();
        self.world_selection
            .iter()
            .filter_map(|iid| {
                let level = levels.get(iid.as_str())?;
                Some((
                    iid.clone(),
                    [
                        number(level, "worldX", 0.) as i32,
                        number(level, "worldY", 0.) as i32,
                    ],
                ))
            })
            .collect()
    }

    pub(super) fn world_group_delta(
        &self,
        anchor: LevelKey,
        start: [f32; 2],
        end: [f32; 2],
    ) -> [i32; 2] {
        let Some(project) = &self.project else {
            return [0, 0];
        };
        let Some(level) = project.level(anchor) else {
            return [0, 0];
        };
        let container = anchor
            .world
            .and_then(|w| project.json["worlds"].get(w))
            .unwrap_or(&project.json);
        let origin = [
            number(level, "worldX", 0.) as i32,
            number(level, "worldY", 0.) as i32,
        ];
        let delta = world_drag_delta(start, end);
        let mut target = [origin[0] + delta[0], origin[1] + delta[1]];
        match text(container, "worldLayout", "Free") {
            "GridVania" => {
                let grids = [
                    number(container, "worldGridWidth", 256.).max(1.) as i32,
                    number(container, "worldGridHeight", 256.).max(1.) as i32,
                ];
                for axis in 0..2 {
                    target[axis] = target[axis].div_euclid(grids[axis]) * grids[axis];
                }
            }
            "LinearHorizontal" | "LinearVertical" => return [0, 0],
            _ if self.grid => {
                let grid = project.json["defs"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|l| l["type"] != "Entities")
                    .map(|l| number(l, "gridSize", 16.).max(1.) as i32)
                    .min()
                    .unwrap_or_else(|| {
                        number(&project.json, "defaultGridSize", 16.).max(1.) as i32
                    });
                for value in &mut target {
                    *value = *value / grid * grid;
                }
            }
            _ => {}
        }
        [target[0] - origin[0], target[1] - origin[1]]
    }

    pub(super) fn world_group_release_delta(
        &self,
        anchor: LevelKey,
        start: [f32; 2],
        end: [f32; 2],
        origins: &[(String, [i32; 2])],
        disable_snap: bool,
        backing_zoom: f32,
    ) -> [i32; 2] {
        let grid_delta = self.world_group_delta(anchor, start, end);
        let Some(project) = &self.project else {
            return grid_delta;
        };
        let container = anchor
            .world
            .and_then(|w| project.json["worlds"].get(w))
            .unwrap_or(&project.json);
        if disable_snap || text(container, "worldLayout", "Free") != "Free" {
            return grid_delta;
        }
        let grid = project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["type"] != "Entities")
            .map(|l| number(l, "gridSize", 16.).max(1.) as i32)
            .min()
            .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.).max(1.) as i32);
        let threshold = grid as f32 / (backing_zoom * 0.4);
        let raw_delta = world_drag_delta(start, end);
        let mut correction = [grid_delta[0] - raw_delta[0], grid_delta[1] - raw_delta[1]];
        let selected = origins
            .iter()
            .map(|(iid, pos)| (iid.as_str(), *pos))
            .collect::<std::collections::BTreeMap<_, _>>();
        let levels = canvas::placements(project, anchor)
            .into_iter()
            .filter_map(|(key, bounds)| {
                let level = project.level(key)?;
                let iid = level["iid"].as_str()?;
                let size = [
                    number(level, "pxWid", 0.) as i32,
                    number(level, "pxHei", 0.) as i32,
                ];
                let position = selected
                    .get(iid)
                    .map(|p| [p[0] + raw_delta[0], p[1] + raw_delta[1]])
                    .unwrap_or([bounds[0] as i32, bounds[1] as i32]);
                Some((iid.to_owned(), position, size))
            })
            .collect::<Vec<_>>();
        let selected_levels = origins
            .iter()
            .filter_map(|(iid, _)| levels.iter().find(|(id, _, _)| id == iid))
            .collect::<Vec<_>>();
        for (_, other_pos, other_size) in levels
            .iter()
            .filter(|(iid, _, _)| !selected.contains_key(iid.as_str()))
        {
            let other_bounds = bounds_at(*other_pos, *other_size);
            for (iid, pos, size) in selected_levels.iter().copied() {
                if bounds_distance(bounds_at(*pos, *size), other_bounds) > threshold {
                    continue;
                }
                for axis in 0..2 {
                    'snap: for offset in [0, size[axis]] {
                        for edge in [other_pos[axis], other_pos[axis] + other_size[axis]] {
                            let change = edge - (pos[axis] + offset);
                            if (change as f32).abs() > threshold {
                                continue;
                            }
                            let mut candidate = *pos;
                            candidate[axis] += change;
                            if !levels.iter().any(|(id, p, size)| {
                                id != iid
                                    && overlaps(bounds_at(candidate, *size), bounds_at(*p, *size))
                            }) {
                                correction[axis] = change;
                                break 'snap;
                            }
                        }
                    }
                }
            }
        }
        [raw_delta[0] + correction[0], raw_delta[1] + correction[1]]
    }

    pub(super) fn apply_world_group_move(
        &mut self,
        origins: &[(String, [i32; 2])],
        start: [f32; 2],
        end: [f32; 2],
        anchor: LevelKey,
        disable_snap: bool,
        backing_zoom: f32,
    ) -> Option<()> {
        let delta =
            self.world_group_release_delta(anchor, start, end, origins, disable_snap, backing_zoom);
        self.edit("Move levels", |p| place_world_group(p, origins, delta))
    }

    #[cfg(test)]
    fn duplicate_world_group(
        &mut self,
        origins: &[(String, [i32; 2])],
        start: [f32; 2],
        end: [f32; 2],
        anchor: LevelKey,
    ) -> Option<()> {
        let delta = self.world_group_delta(anchor, start, end);
        let (new_anchor, copies) = self.edit("Duplicate levels", |p| {
            let (anchor, copies) = copy_world_group(p, origins, anchor)?;
            place_world_group(p, &copies, delta)?;
            Ok((anchor, copies))
        })?;
        self.world_selection = copies.into_iter().map(|(iid, _)| iid).collect();
        self.selected = Some(new_anchor);
        Some(())
    }

    pub(super) fn update_world_hover(&mut self, ctx: &egui::Context, rect: Rect, hovered: bool) {
        let pointer = ctx
            .pointer_hover_pos()
            .filter(|pointer| rect.contains(*pointer));
        let resize_cursor = ctx.output(|output| {
            matches!(
                output.cursor_icon,
                egui::CursorIcon::ResizeHorizontal
                    | egui::CursorIcon::ResizeVertical
                    | egui::CursorIcon::ResizeNwSe
                    | egui::CursorIcon::ResizeNeSw
            )
        });
        if !hovered
            || pointer.is_none()
            || !self.world_selection.is_empty()
            || self.gesture.is_some()
            || self.camera_drag.is_some()
            || self.level_resize.is_some()
            || self.picking_field()
            || self.world_context.picking
            || self.canvas_input_blocked(ctx)
            || resize_cursor
        {
            self.world_hover = None;
            return;
        }
        if ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerMoved(_)))
        }) {
            self.world_hover = pointer.and_then(|pos| self.level_under_pointer(ctx, rect, pos));
        }
        if self.world_hover.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    pub(super) fn draw_world_selection(&self, ui: &mut egui::Ui, rect: Rect) {
        if !self.world {
            return;
        }
        let (Some(project), Some(selected)) = (&self.project, self.selected) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(rect);
        let screen = |p| {
            egui::Pos2::from(
                self.viewport
                    .to_screen(p, rect.min.into(), rect.size().into()),
            )
        };
        if let Some(tools::Gesture::LevelMove {
            key,
            end,
            origin,
            moved: true,
            ..
        }) = &self.gesture
            && let Some(axis) = linear_world_axis(project, *key)
            && let Some((_, coordinate)) =
                linear_level_insert_point(project, *key, axis, origin[axis], end[axis])
        {
            let extent = project
                .level_keys_in_world(key.world)
                .filter(|other| other != key)
                .filter_map(|other| project.level(other))
                .map(|level| number(level, if axis == 0 { "pxHei" } else { "pxWid" }, 0.))
                .fold(0., f32::max);
            let mut from = [-100.; 2];
            from[axis] = coordinate;
            let mut to = from;
            to[1 - axis] = extent + 100.;
            painter.line_segment(
                [screen(from), screen(to)],
                Stroke::new(
                    10. * self.viewport.zoom,
                    Color32::from_rgba_unmultiplied(0x72, 0xfe, 0xff, 128),
                ),
            );
        }
        let delta = match &self.gesture {
            Some(tools::Gesture::WorldGroupMove {
                anchor,
                start,
                end,
                moved: true,
                ..
            }) => self.world_group_delta(*anchor, *start, *end),
            _ => [0, 0],
        };
        for (key, bounds) in canvas::placements(project, selected) {
            if self.world_selection_contains(key) {
                let area = Rect::from_two_pos(
                    screen([bounds[0] + delta[0] as f32, bounds[1] + delta[1] as f32]),
                    screen([bounds[2] + delta[0] as f32, bounds[3] + delta[1] as f32]),
                );
                painter.rect_filled(
                    area,
                    0.,
                    egui::Color32::from_rgba_unmultiplied(0, 255, 0, 38),
                );
                painter.rect_stroke(
                    area,
                    0.,
                    egui::Stroke::new(3_f32, egui::Color32::from_rgba_unmultiplied(0, 255, 0, 204)),
                    egui::StrokeKind::Middle,
                );
            }
        }
        if let Some(tools::Gesture::WorldRectangle {
            corners: Some([start, end]),
            ..
        }) = &self.gesture
        {
            let area = Rect::from_two_pos(screen(*start), screen(*end));
            painter.rect_filled(
                area,
                0.,
                egui::Color32::from_rgba_unmultiplied(0, 255, 0, 51),
            );
            painter.rect_stroke(
                area,
                0.,
                egui::Stroke::new(2_f32, egui::Color32::from_rgba_unmultiplied(0, 255, 0, 204)),
                egui::StrokeKind::Middle,
            );
        }
    }

    pub(super) fn world_selection_contains(&self, key: LevelKey) -> bool {
        self.project
            .as_ref()
            .and_then(|p| p.level(key))
            .and_then(|l| l["iid"].as_str())
            .is_some_and(|iid| self.world_selection.iter().any(|selected| selected == iid))
    }

    pub(super) fn world_marquee_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        let mut active = matches!(self.gesture, Some(tools::Gesture::WorldRectangle { .. }));
        if !active
            && (self.gesture.is_some() || !ctx.input(|input| input.pointer.primary_pressed()))
        {
            return false;
        }
        let density = ctx.pixels_per_point();
        let point = |viewport: &Viewport, pos| world_pointer_pixel(viewport, rect, pos, density);
        let (events, modifiers) = ctx.input(|input| (input.events.clone(), input.modifiers));
        let mut handled = active;
        for event in events {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } if !active
                    && response.interact_rect.contains(pos)
                    && ctx
                        .layer_id_at(pos)
                        .is_none_or(|layer| layer == response.layer_id) =>
                {
                    let start = point(&self.viewport, pos);
                    if !modifiers.shift && self.level_at_world(start).is_some() {
                        return false;
                    }
                    if !modifiers.shift && !world_selection_modifier(modifiers) {
                        self.world_selection.clear();
                    }
                    self.gesture = Some(tools::Gesture::WorldRectangle {
                        press: pos,
                        corners: None,
                    });
                    active = true;
                    handled = true;
                }
                egui::Event::PointerMoved(pos) if active => {
                    if let Some(tools::Gesture::WorldRectangle { press, corners }) =
                        &mut self.gesture
                    {
                        // The original Coords resolves the saved page position
                        // through the current camera on each pointer move.
                        let start = point(&self.viewport, *press);
                        let end = point(&self.viewport, pos);
                        *corners = Some([start, end]);
                        self.world_select_rectangle(
                            start,
                            end,
                            world_selection_modifier(modifiers),
                        );
                    }
                }
                egui::Event::PointerButton { pressed: false, .. } if active => {
                    self.gesture = None;
                    active = false;
                }
                _ => {}
            }
        }
        handled
    }

    fn world_select_rectangle(&mut self, start: [f32; 2], end: [f32; 2], additive: bool) {
        let Some(project) = &self.project else { return };
        let Some(selected) = self.selected else {
            return;
        };
        let rect = [
            start[0].min(end[0]),
            start[1].min(end[1]),
            start[0].max(end[0]),
            start[1].max(end[1]),
        ];
        let mut selection = if additive {
            std::mem::take(&mut self.world_selection)
        } else {
            Vec::new()
        };
        let mut seen: HashSet<_> = selection.iter().map(String::as_str).collect();
        let mut additions = Vec::new();
        for (_, level, bounds) in canvas::iter_placements(project, selected) {
            if level["worldDepth"].as_i64().unwrap_or(0) != self.world_depth {
                continue;
            }
            if bounds[0] < rect[2]
                && bounds[2] > rect[0]
                && bounds[1] < rect[3]
                && bounds[3] > rect[1]
                && let Some(iid) = level["iid"].as_str()
                && seen.insert(iid)
            {
                additions.push(iid.to_owned());
            }
        }
        selection.extend(additions);
        self.world_selection = selection;
    }

    pub(super) fn level_under_pointer(
        &self,
        ctx: &egui::Context,
        rect: Rect,
        pointer: egui::Pos2,
    ) -> Option<LevelKey> {
        let (project, key) = (self.project.as_ref()?, self.selected?);
        let (local, world) =
            self.canvas_pointer_pixels(project, key, rect, pointer, ctx.pixels_per_point());
        if !self.world && entity_tools::placement_in_level(project, key, local) {
            return None;
        }
        self.level_at_world(world)
    }

    pub(super) fn level_at_world(&self, position: [f32; 2]) -> Option<LevelKey> {
        let project = self.project.as_ref()?;
        let selected = self.selected?;
        let position = position.map(|value| (value + 0.5).floor());
        let contains = |bounds: [f32; 4]| {
            position[0] >= bounds[0]
                && position[0] < bounds[2]
                && position[1] >= bounds[1]
                && position[1] < bounds[3]
        };
        let active = |level: &Value| number(level, "worldDepth", 0.) as i64 == self.world_depth;
        if matches!(
            canvas::world_layout(project, selected),
            "LinearHorizontal" | "LinearVertical"
        ) {
            return canvas::iter_placements(project, selected)
                .filter(|(_, level, bounds)| contains(*bounds) && active(level))
                .map(|(key, _, _)| key)
                .last();
        }
        project
            .level_keys_in_world(selected.world)
            .rev()
            .find(|&key| {
                project.level(key).is_some_and(|level| {
                    contains(canvas::free_level_bounds(level)) && active(level)
                })
            })
    }

    pub(super) fn world_drag_position(
        &self,
        key: LevelKey,
        start: [f32; 2],
        end: [f32; 2],
        origin: [i32; 2],
        disable_edge_snap: bool,
        backing_zoom: f32,
    ) -> [i32; 2] {
        let Some(project) = &self.project else {
            return origin;
        };
        let Some(level) = project.level(key) else {
            return origin;
        };
        let container = key
            .world
            .and_then(|world| project.json["worlds"].get(world))
            .unwrap_or(&project.json);
        let delta = world_drag_delta(start, end);
        let mut position = std::array::from_fn(|axis| origin[axis] + delta[axis]);
        if container["worldLayout"] == "GridVania" {
            let grid = [
                number(container, "worldGridWidth", 256.).max(1.) as i32,
                number(container, "worldGridHeight", 256.).max(1.) as i32,
            ];
            return std::array::from_fn(|axis| position[axis].div_euclid(grid[axis]) * grid[axis]);
        }
        let size = [number(level, "pxWid", 256.), number(level, "pxHei", 256.)];
        if let Some(axis) = linear_world_axis(project, key) {
            position[1 - axis] = (-0.8 * size[1 - axis]) as i32;
            return position;
        }
        let grid = project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|layer| layer["type"] != "Entities")
            .map(|layer| number(layer, "gridSize", 16.).max(1.) as i32)
            .min()
            .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.).max(1.) as i32);
        if self.grid {
            position = position.map(|value| value / grid * grid);
        }
        if disable_edge_snap {
            return position;
        }
        let size = size.map(|value| value as i32);
        let neighbours = canvas::placements(project, key)
            .into_iter()
            .filter(|(candidate, _)| *candidate != key)
            .map(|(_, bounds)| bounds)
            .collect::<Vec<_>>();
        let distance = grid as f32 / (backing_zoom * 0.4);
        for neighbour in &neighbours {
            if bounds_distance(bounds_at(position, size), *neighbour) > distance {
                continue;
            }
            // Source snaps X, Y, then X again: Y can make an X candidate valid.
            for axis in [0, 1, 0] {
                for offset in [0, size[axis]] {
                    for edge in [neighbour[axis], neighbour[axis + 2]] {
                        if (position[axis] as f32 + offset as f32 - edge).abs() > distance {
                            continue;
                        }
                        let mut candidate = position;
                        candidate[axis] = edge as i32 - offset;
                        if !neighbours
                            .iter()
                            .any(|other| overlaps(bounds_at(candidate, size), *other))
                        {
                            position = candidate;
                        }
                    }
                }
            }
        }
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> EditorApp {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.world = true;
        app.viewport.zoom = 1.;
        app
    }

    #[test]
    #[ignore = "manual large world selection performance comparison"]
    fn benchmark_large_world_marquee_and_pointer_hit() {
        let mut app = editor();
        let project = app.project.as_mut().unwrap();
        let size = 8_192;
        project.json["worldLayout"] = json!("Free");
        project.json["levels"] = (0..size)
            .map(|i| {
                json!({
                    "iid":format!("level-{i}"),"worldX":i*20,"worldY":0,"pxWid":16,"pxHei":16,
                    "worldDepth":usize::from(i % 5 == 2)
                })
            })
            .collect::<Vec<_>>()
            .into();
        let expected: Vec<_> = std::iter::once(size - 1)
            .chain((0..size - 1).filter(|i| i % 5 != 2))
            .map(|i| format!("level-{i}"))
            .collect();
        let mut marquee = [std::time::Duration::ZERO; 5];
        let mut hit = marquee;
        for (marquee_time, hit_time) in marquee.iter_mut().zip(&mut hit) {
            app.world_selection = vec![format!("level-{}", size - 1)];
            let start = Instant::now();
            app.world_select_rectangle([-1., -1.], [size as f32 * 20., 20.], true);
            *marquee_time = start.elapsed();
            assert_eq!(app.world_selection, expected);
            let start = Instant::now();
            for i in 0..64 {
                if i % 8 == 0 {
                    assert_eq!(app.level_at_world([-20., 4.]), None);
                    continue;
                }
                let level = if i % 2 == 0 {
                    size - 1 - i % 8 * 5
                } else {
                    1 + i * 125
                };
                let position = [level as f32 * 20. + 3., 4.];
                assert_eq!(
                    app.level_at_world(position),
                    Some(LevelKey { world: None, level })
                );
            }
            *hit_time = start.elapsed();
        }
        marquee.sort_unstable();
        hit.sort_unstable();
        eprintln!(
            "Large world marquee: median {:?}; samples {marquee:?}",
            marquee[2]
        );
        eprintln!(
            "Large world pointer hit: median {:?}; samples {hit:?}",
            hit[2]
        );
    }

    fn marquee_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        rect: Rect,
        modifiers: egui::Modifiers,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                modifiers,
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (canvas, response) =
                            ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                        app.canvas_controls(ctx, canvas, &response);
                        app.draw_world_selection(ui, rect);
                    });
            },
        )
    }

    fn marquee_button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        }
    }

    #[test]
    fn single_level_copy_stays_live_and_only_resnaps_on_pointer_moves() {
        for (deselect_last, move_without_alt, density, expected_x) in [
            (false, false, 1., 80),
            (true, false, 1., 80),
            (true, true, 1., 112),
            (false, true, 2., 81),
        ] {
            let mut app = editor();
            app.fit = false;
            app.grid = false;
            app.viewport.center = [256., 256.];
            let first = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            let neighbor = project.add_level(None, "Neighbor", 128, 128).unwrap();
            project.move_level(neighbor, 368, 320, 0).unwrap();
            let before = project.json.clone();
            if deselect_last {
                app.world_toggle_selection(first);
            }
            let ctx = egui::Context::default();
            ctx.options_mut(|options| options.zoom_factor = density);
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 850.));
            let start = egui::Pos2::from(app.viewport.to_screen(
                [64., 64.],
                [0., 0.],
                rect.size().into(),
            ));
            let mut end = start + egui::vec2(80., 320.);
            let none = egui::Modifiers::NONE;
            let press = egui::Modifiers {
                command: deselect_last,
                ..Default::default()
            };
            let copy = egui::Modifiers {
                alt: true,
                command: true,
                ..Default::default()
            };
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(start)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                press,
                vec![marquee_button(start, true, press)],
            );
            assert!(app.world_selection.is_empty());
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![egui::Event::PointerMoved(start + egui::vec2(2., 0.))],
            );
            assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);

            marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![egui::Event::PointerMoved(end)],
            );
            let selected = app.selected.unwrap();
            let project = app.project.as_ref().unwrap();
            assert_eq!(project.level_keys().len(), 3);
            assert!(app.dirty);
            assert!(app.world_selection.is_empty());
            let held = project.level(selected).unwrap();
            let held_iid = held["iid"].clone();
            assert_ne!(held_iid, before["levels"][0]["iid"]);
            assert_eq!(
                [held["worldX"].as_i64(), held["worldY"].as_i64()],
                [Some(80), Some(320)]
            );

            if move_without_alt {
                end.x += 1.;
                marquee_frame(
                    &mut app,
                    &ctx,
                    rect,
                    none,
                    vec![egui::Event::PointerMoved(end)],
                );
            } else {
                marquee_frame(&mut app, &ctx, rect, none, vec![]);
            }
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![marquee_button(end, false, none)],
            );
            let project = app.project.as_mut().unwrap();
            let saved = project.level(app.selected.unwrap()).unwrap();
            assert_eq!(saved["iid"], held_iid);
            assert_eq!(
                [saved["worldX"].as_i64(), saved["worldY"].as_i64()],
                [Some(expected_x), Some(320)]
            );
            assert_eq!(project.level(first).unwrap()["worldX"], 0);
            assert!(app.history.undo(project));
            assert_eq!(project.json, before);
            assert!(!app.history.undo(project));
        }
    }

    #[test]
    fn gridvania_copy_uses_level_origin_plus_pointer_delta_before_grid_floor() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        let p = app.project.as_mut().unwrap();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(256);
        p.json["worldGridHeight"] = json!(256);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 650.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [248., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(16., -80.);
        let none = egui::Modifiers::NONE;
        let copy = egui::Modifiers {
            alt: true,
            command: true,
            ..Default::default()
        };
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(start, true, none)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(end, false, none)],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level_keys().len(), 2);
        let new = project.level(app.selected.unwrap()).unwrap();
        assert_eq!(
            [new["worldX"].as_i64(), new["worldY"].as_i64()],
            [Some(0), Some(-256)]
        );
    }

    #[test]
    fn linear_copy_draws_the_live_level_and_insert_cue_before_reordering() {
        for (layout, axis) in [("LinearHorizontal", 0), ("LinearVertical", 1)] {
            let mut app = editor();
            app.fit = false;
            app.grid = false;
            let mut center = [0., 0.];
            center[axis] = 400.;
            app.viewport.center = center;
            let first = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            let second = project.add_level(None, "Second", 128, 128).unwrap();
            project.json["worldLayout"] = json!(layout);
            if axis == 1 {
                for key in [first, second] {
                    let level = project.level_mut(key).unwrap();
                    level["worldX"] = json!(-1);
                    level["worldY"] = json!(-1);
                }
            }
            project.refresh_derived_data().unwrap();
            let before = project.json.clone();
            if axis == 1 {
                app.world_toggle_selection(first);
            }
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400., 1400.));
            let start = egui::Pos2::from(app.viewport.to_screen(
                [64., 64.],
                [0., 0.],
                rect.size().into(),
            ));
            let mut move_by = [0., 0.];
            move_by[axis] = 786.;
            let end = start + egui::Vec2::from(move_by);
            let none = egui::Modifiers::NONE;
            let press = egui::Modifiers {
                command: axis == 1,
                ..Default::default()
            };
            let copy = egui::Modifiers {
                alt: true,
                command: true,
                ..Default::default()
            };
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(start)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                press,
                vec![marquee_button(start, true, press)],
            );
            let held_overlay = marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![egui::Event::PointerMoved(end)],
            );
            let project = app.project.as_ref().unwrap();
            assert_eq!(project.level_keys().len(), 3);
            let selected = app.selected.unwrap();
            let level = project.level(selected).unwrap();
            let iid = level["iid"].clone();
            assert_ne!(selected, first);
            assert_eq!(
                project.level(first).unwrap()["worldX"],
                before["levels"][0]["worldX"]
            );
            assert_eq!(
                project.level(first).unwrap()["worldY"],
                before["levels"][0]["worldY"]
            );
            assert_eq!(project.json["levels"][1]["iid"], iid);
            let mut pos = [-204., -204.];
            pos[axis] = 786.;
            assert_eq!(
                [level["worldX"].as_i64(), level["worldY"].as_i64()],
                [Some(-1), Some(-1)]
            );
            let mut line_start = [-100., -100.];
            line_start[axis] = 736.;
            let line_start = egui::Pos2::from(app.viewport.to_screen(
                line_start,
                [0., 0.],
                rect.size().into(),
            ));
            assert!(held_overlay.shapes.iter().any(|shape| matches!(shape.shape,
                egui::Shape::LineSegment { points, stroke } if points[0] == line_start && stroke.width == 10.)));

            let min = egui::Pos2::from(app.viewport.to_screen(pos, [0., 0.], rect.size().into()));
            let render = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            app.draw_project_canvas(ctx, ui.painter(), rect, selected);
                        });
                },
            );
            assert!(render.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Rect(r) if r.stroke.color == crate::theme::ACCENT && r.rect.min == min - egui::Vec2::splat(2.))));

            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![marquee_button(end, false, none)],
            );
            let project = app.project.as_mut().unwrap();
            assert_eq!(project.json["levels"][2]["iid"], iid);
            assert_eq!(project.json["levels"][2]["worldX"], -1);
            assert_eq!(project.json["levels"][2]["worldY"], -1);
            assert_eq!(project.level(app.selected.unwrap()).unwrap()["iid"], iid);
            assert!(app.history.undo(project));
            assert_eq!(project.json, before);
        }
    }

    #[test]
    fn stationary_shift_click_does_not_draw_a_marquee_or_replace_the_selection() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let pos = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            rect.min.into(),
            rect.size().into(),
        ));
        let shift = egui::Modifiers::SHIFT;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(pos)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(pos, true, shift)],
        );
        let held = marquee_frame(&mut app, &ctx, rect, shift, vec![]);
        assert!(app.world_selection_contains(second));
        assert!(!app.world_selection_contains(first));
        assert!(!held.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(r) if r.fill == Color32::from_rgba_unmultiplied(0, 255, 0, 51))));
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(pos, false, shift)],
        );
        assert!(app.world_selection_contains(second));
        assert!(!app.world_selection_contains(first));
        assert!(app.gesture.is_none());
    }

    #[test]
    fn marquee_rounds_each_mouse_move_and_mouse_release_does_not_expand_it() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        app.viewport.zoom = 2.;
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        let third = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Third", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(third, 480, 0, 0)
            .unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let screen = |x| {
            egui::Pos2::from(
                app.viewport
                    .to_screen([x, 64.], rect.min.into(), rect.size().into()),
            )
        };
        let mut start = screen(280.);
        start.y -= 40.;
        let edge = screen(320.);
        let inside = screen(320.5);
        let release = screen(490.);
        let shift = egui::Modifiers::SHIFT;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(start, true, shift)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(edge)],
        );
        assert!(!app.world_selection_contains(second));
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(inside)],
        );
        assert!(app.world_selection_contains(second));
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(release, false, shift)],
        );
        assert!(app.world_selection_contains(second));
        assert!(!app.world_selection_contains(third));
    }

    #[test]
    fn marquee_rounds_negative_half_pixels_away_from_zero() {
        let mut app = editor();
        app.fit = false;
        app.viewport.zoom = 2.;
        let project = app.project.as_mut().unwrap();
        let level = project.add_level(None, "Negative", 16, 16).unwrap();
        project.move_level(level, -64, 32, 0).unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let screen =
            |world| egui::Pos2::from(app.viewport.to_screen(world, [0., 0.], rect.size().into()));
        let start = screen([-100., 16.]);
        let edge = screen([-63.5, 48.]);
        let shift = egui::Modifiers::SHIFT;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(start, true, shift)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(edge)],
        );
        assert!(!app.world_selection_contains(level));
    }

    #[test]
    fn marquee_reprojects_its_press_position_after_camera_zoom_on_the_next_move() {
        let mut app = editor();
        app.fit = false;
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let second = project.add_level(None, "Second", 128, 128).unwrap();
        project.move_level(second, 320, 0, 0).unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let screen =
            |world| egui::Pos2::from(app.viewport.to_screen(world, [0., 0.], rect.size().into()));
        let start = screen([250., 16.]);
        let end = screen([330., 100.]);
        let shift = egui::Modifiers::SHIFT;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![marquee_button(start, true, shift)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(end)],
        );
        assert!(app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., 666.67),
                modifiers: shift,
            }],
        );
        assert!(app.viewport.zoom > 1.9);
        assert!(app.world_selection_contains(first));
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            shift,
            vec![egui::Event::PointerMoved(end + egui::vec2(2., 0.))],
        );
        assert!(!app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
    }

    #[test]
    fn additive_marquee_keeps_intermediate_hits_when_press_move_and_release_share_a_frame() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.world_toggle_selection(first);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let screen = |world| {
            egui::Pos2::from(
                app.viewport
                    .to_screen(world, rect.min.into(), rect.size().into()),
            )
        };
        let start = screen([280., 16.]);
        let inside = screen([400., 100.]);
        let outside = screen([300., 100.]);
        let modifiers = egui::Modifiers {
            shift: true,
            command: true,
            ctrl: !cfg!(target_os = "macos"),
            mac_cmd: cfg!(target_os = "macos"),
            ..Default::default()
        };
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            modifiers,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            modifiers,
            vec![
                marquee_button(start, true, modifiers),
                egui::Event::PointerMoved(inside),
                egui::Event::PointerMoved(outside),
                marquee_button(outside, false, modifiers),
            ],
        );
        assert!(app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        assert!(app.gesture.is_none());
    }

    #[test]
    fn preview_154_world_hover_is_hidden_during_multiselection() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let pos = egui::Pos2::from(app.viewport.to_screen(
            [20., 20.],
            rect.min.into(),
            rect.size().into(),
        ));
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            app.update_world_hover(ctx, rect, true);
                            app.draw_project_canvas(ctx, ui.painter(), rect, key);
                        });
                },
            )
        };
        let has_hover = |out: &egui::FullOutput| {
            out.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgba_premultiplied(38,31,0,38)))
        };
        app.world = false;
        let _ = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        app.set_world_view(true);
        assert!(
            !has_hover(&frame(&mut app, vec![])),
            "entry keeps the stationary pointer inactive"
        );
        let normal = frame(
            &mut app,
            vec![egui::Event::PointerMoved(pos + egui::vec2(1., 0.))],
        );
        assert!(has_hover(&normal));
        assert_eq!(
            normal.platform_output.cursor_icon,
            egui::CursorIcon::PointingHand
        );
        assert!(
            has_hover(&frame(&mut app, vec![])),
            "hover remains with the pointer still"
        );
        let outside = frame(
            &mut app,
            vec![egui::Event::PointerMoved(pos - egui::vec2(40., 40.))],
        );
        assert!(!has_hover(&outside));
        app.world_toggle_selection(key);
        let selected = frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        assert!(!has_hover(&selected));
    }

    #[test]
    fn preview_154_neighbor_hover_and_press_work_in_level_view() {
        let mut app = editor();
        app.world = false;
        app.fit = false;
        app.viewport.center = [220., 64.];
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_layer("Entities", "Entities", 16).unwrap();
        app.entity_uid = Some(project.add_entity_def("Actor", 16, 16).unwrap());
        project.move_level(first, 160, -48, 0).unwrap();
        let neighbor = project.add_level(None, "Neighbor", 128, 128).unwrap();
        project.move_level(neighbor, 430, -48, 0).unwrap();
        let other_depth = project.add_level(None, "OtherDepth", 128, 128).unwrap();
        project.move_level(other_depth, 590, -48, 1).unwrap();
        let project_json = project.json.clone();

        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let pos = |x| {
            egui::Pos2::from(
                app.viewport
                    .to_screen([x, 32.], rect.min.into(), rect.size().into()),
            )
        };
        let (current_pos, neighbor_pos, other_pos) = (pos(40.), pos(290.), pos(450.));
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) =
                                ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                            app.update_world_hover(ctx, rect, response.hovered());
                            app.draw_project_canvas(ctx, ui.painter(), rect, app.selected.unwrap());
                            app.draw_brush_preview(ui, rect, response.hovered());
                        });
                },
            )
        };
        let has_hover = |out: &egui::FullOutput| {
            out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgba_premultiplied(38, 31, 0, 38)))
        };
        let _ = frame(&mut app, vec![]);
        let current = frame(&mut app, vec![egui::Event::PointerMoved(current_pos)]);
        assert!(!has_hover(&current));
        assert_eq!(current.platform_output.cursor_icon, egui::CursorIcon::Cell);
        let other = frame(&mut app, vec![egui::Event::PointerMoved(other_pos)]);
        assert!(!has_hover(&other));
        let over = frame(&mut app, vec![egui::Event::PointerMoved(neighbor_pos)]);
        assert_eq!(app.world_hover, Some(neighbor));
        assert!(has_hover(&over));
        assert_eq!(
            over.platform_output.cursor_icon,
            egui::CursorIcon::PointingHand
        );
        assert!(has_hover(&frame(&mut app, vec![])));

        let press = frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: neighbor_pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.selected, Some(neighbor));
        assert_eq!(app.viewport.center, [-50., 64.]);
        assert!(!app.world);
        assert!(!app.dirty);
        assert!(!has_hover(&press));
        assert_ne!(press.platform_output.cursor_icon, egui::CursorIcon::Cell);
        let release = frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: neighbor_pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!has_hover(&release));
        assert_eq!(release.platform_output.cursor_icon, egui::CursorIcon::Cell);
        assert_eq!(app.project.as_ref().unwrap().json, project_json);
    }

    #[test]
    fn preview_154_world_rectangle_selection_respects_depth_and_additive_selection() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        let other_depth = p.add_level(None, "Other", 128, 128).unwrap();
        p.move_level(other_depth, 350, 0, 1).unwrap();
        app.world_select_rectangle([0., 0.], [500., 150.], false);
        assert_eq!(app.world_selection.len(), 2);
        assert!(app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        assert!(!app.world_selection_contains(other_depth));
        let base = app.world_selection.clone();
        app.world_select_rectangle([700., 500.], [800., 600.], true);
        assert_eq!(app.world_selection, base);
    }

    #[test]
    fn preview_154_shift_drag_in_world_selects_multiple_levels_with_pointer_events() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        app.viewport.zoom = 1.;
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [-10., -10.],
            rect.min.into(),
            rect.size().into(),
        ));
        let end = egui::Pos2::from(app.viewport.to_screen(
            [500., 150.],
            rect.min.into(),
            rect.size().into(),
        ));
        let modifiers = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        assert!(app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
    }

    #[test]
    fn preview_154_rectangle_selection_uses_current_control_modifier() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let screen = |world| {
            egui::Pos2::from(
                app.viewport
                    .to_screen(world, rect.min.into(), rect.size().into()),
            )
        };
        let start = screen([0., 0.]);
        let inside = screen([260., 260.]);
        let outside = screen([-20., -20.]);
        let shift = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        let additive = egui::Modifiers {
            shift: true,
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, modifiers, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, shift, vec![]);
        frame(&mut app, shift, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            shift,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: shift,
            }],
        );
        assert!(app.world_selection_contains(second));
        frame(&mut app, additive, vec![egui::Event::PointerMoved(inside)]);
        assert!(app.world_selection_contains(first) && app.world_selection_contains(second));
        frame(&mut app, additive, vec![egui::Event::PointerMoved(outside)]);
        assert!(app.world_selection_contains(first) && app.world_selection_contains(second));
    }

    #[test]
    fn preview_154_command_click_toggles_world_selection() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        for (world, key, expected) in [
            ([100., 100.], first, true),
            ([350., 50.], second, true),
            ([100., 100.], first, false),
        ] {
            let pos = egui::Pos2::from(app.viewport.to_screen(
                world,
                rect.min.into(),
                rect.size().into(),
            ));
            frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }],
                );
            }
            assert_eq!(app.world_selection_contains(key), expected);
        }
        assert!(app.world_selection_contains(second));
    }

    #[test]
    fn clicking_the_current_level_in_a_world_group_enters_only_on_release_without_command() {
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let none = egui::Modifiers::NONE;
        for (down, up, expected_world, batched) in [
            (none, none, false, false),
            (command, none, false, false),
            (none, command, true, false),
            (command, command, true, false),
            (command, none, false, true),
            (none, command, true, true),
        ] {
            let mut app = editor();
            app.fit = false;
            let first = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            let second = project.add_level(None, "Second", 128, 128).unwrap();
            project.move_level(second, 320, 0, 0).unwrap();
            let before = project.json.clone();
            app.world_toggle_selection(first);
            app.world_toggle_selection(second);
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
            let point = egui::Pos2::from(app.viewport.to_screen(
                [64., 64.],
                [0., 0.],
                rect.size().into(),
            ));
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                down,
                vec![egui::Event::PointerMoved(point)],
            );
            let press = marquee_button(point, true, down);
            let release = marquee_button(point, false, up);
            if batched {
                marquee_frame(&mut app, &ctx, rect, up, vec![press, release]);
            } else {
                marquee_frame(&mut app, &ctx, rect, down, vec![press]);
                assert!(app.world);
                assert!(app.world_selection_contains(second));
                marquee_frame(&mut app, &ctx, rect, up, vec![release]);
            }
            assert_eq!(app.world, expected_world, "press {down:?}, release {up:?}");
            assert_eq!(app.selected, Some(first));
            assert_eq!(app.project.as_ref().unwrap().json, before);
            assert!(!app.dirty);
            if !expected_world {
                assert!(app.world_selection.is_empty());
            }
        }
    }

    #[test]
    fn world_click_uses_the_level_current_on_press_and_requires_real_mouse_release() {
        let mut app = editor();
        app.fit = false;
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let second = project.add_level(None, "Second", 128, 128).unwrap();
        project.move_level(second, 320, 0, 0).unwrap();
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let point = egui::Pos2::from(app.viewport.to_screen(
            [360., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let command = egui::Modifiers {
            command: true,
            ..Default::default()
        };
        let none = egui::Modifiers::NONE;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![egui::Event::PointerMoved(point)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![marquee_button(point, true, command)],
        );
        assert_eq!(app.selected, Some(second));
        assert!(app.world_selection.is_empty());
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(point, false, none)],
        );
        assert!(app.world);
        assert_ne!(app.selected, Some(first));

        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(point, true, none)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::WindowFocused(false)],
        );
        assert!(app.world);
        assert_eq!(app.selected, Some(second));
        assert!(!app.dirty);
    }

    #[test]
    fn high_density_world_click_with_a_small_move_remains_a_click() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        let first = app.selected.unwrap();
        let before = app.project.as_ref().unwrap().json.clone();
        let ctx = egui::Context::default();
        ctx.options_mut(|options| options.zoom_factor = 2.);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(10., 0.);
        let none = egui::Modifiers::NONE;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(start, true, none)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(end, false, none)],
        );
        assert!(!app.world);
        assert_eq!(app.selected, Some(first));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.dirty);
    }

    #[test]
    fn preview_154_group_move_preserves_offsets_and_is_one_undo() {
        let mut app = editor();
        app.grid = false;
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let origins = app.world_group_origins();
        app.apply_world_group_move(&origins, [0., 0.], [40., 80.], first, true, 1.)
            .unwrap();
        let p = app.project.as_ref().unwrap();
        assert_eq!(
            (
                p.level(first).unwrap()["worldX"].as_i64(),
                p.level(first).unwrap()["worldY"].as_i64()
            ),
            (Some(40), Some(80))
        );
        assert_eq!(
            (
                p.level(second).unwrap()["worldX"].as_i64(),
                p.level(second).unwrap()["worldY"].as_i64()
            ),
            (Some(360), Some(80))
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.level(first).unwrap()["worldX"], 0);
        assert_eq!(p.level(second).unwrap()["worldX"], 320);
    }

    #[test]
    fn preview_154_duplicate_group_preserves_originals_and_undoes_as_one_action() {
        let mut app = editor();
        app.grid = false;
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let origins = app.world_group_origins();
        app.duplicate_world_group(&origins, [0., 0.], [40., 80.], first)
            .unwrap();
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.level_keys().len(), 4);
        assert_eq!(
            p.level_keys()
                .iter()
                .filter_map(|k| p.level(*k))
                .filter(|l| l["worldX"] == 40 || l["worldX"] == 360)
                .count(),
            2
        );
        assert_eq!(app.world_selection.len(), 2);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);
    }

    #[test]
    fn preview_154_world_group_keeps_selection_order_for_drag_and_duplicate() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let second = project.add_level(None, "Second", 128, 128).unwrap();
        project.move_level(second, 320, 0, 0).unwrap();
        let first_iid = project.level(first).unwrap()["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let second_iid = project.level(second).unwrap()["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        project.level_mut(first).unwrap()["testMarker"] = json!("first");
        project.level_mut(second).unwrap()["testMarker"] = json!("second");
        app.world_toggle_selection(second);
        app.world_toggle_selection(first);
        let origins = app.world_group_origins();
        assert_eq!(
            origins
                .iter()
                .map(|(iid, _)| iid.as_str())
                .collect::<Vec<_>>(),
            [second_iid.as_str(), first_iid.as_str()]
        );
        app.duplicate_world_group(&origins, [0., 0.], [40., 80.], first)
            .unwrap();
        let project = app.project.as_ref().unwrap();
        let markers = app
            .world_selection
            .iter()
            .map(|iid| {
                project
                    .level_keys()
                    .into_iter()
                    .find_map(|key| {
                        let level = project.level(key)?;
                        (level["iid"] == *iid)
                            .then(|| level["testMarker"].as_str().unwrap().to_owned())
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(markers, ["second", "first"]);
    }

    #[test]
    fn preview_154_world_group_drag_rounds_each_world_coordinate_before_delta() {
        let mut app = editor();
        app.grid = false;
        let first = app.selected.unwrap();
        app.world_toggle_selection(first);
        let origins = app.world_group_origins();
        let start = [0.49, -0.49];
        let end = [0.51, -0.51];
        assert_eq!(app.world_group_delta(first, start, end), [1, -1]);
        assert_eq!(
            app.world_group_release_delta(first, start, end, &origins, true, 1.),
            [1, -1]
        );
    }

    #[test]
    fn preview_154_group_release_snaps_to_nearby_unselected_level_unless_alt() {
        let mut app = editor();
        app.grid = false;
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        let neighbour = p.add_level(None, "Neighbour", 128, 128).unwrap();
        p.move_level(neighbour, 500, 0, 0).unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let origins = app.world_group_origins();
        assert_eq!(
            app.world_group_release_delta(first, [0., 0.], [40., 0.], &origins, false, 1.),
            [52, 0]
        );
        assert_eq!(
            app.world_group_release_delta(first, [0., 0.], [40., 0.], &origins, true, 1.),
            [40, 0]
        );
    }

    #[test]
    fn grouped_drop_snaps_in_backing_pixels_and_uses_alt_from_mouseup() {
        let none = egui::Modifiers::NONE;
        let alt = egui::Modifiers::ALT;
        for (density, up, frame_modifiers, move_after_drop, expected) in [
            (1., none, none, false, 72),
            (2., none, none, false, 40),
            (1., alt, none, false, 40),
            (1., none, alt, false, 72),
            (1., none, none, true, 72),
        ] {
            let mut app = editor();
            app.fit = false;
            app.grid = false;
            app.viewport.center = [256., 128.];
            let anchor = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            let other = project.add_level(None, "Second", 128, 128).unwrap();
            project.move_level(other, 320, 0, 0).unwrap();
            let neighbor = project.add_level(None, "Neighbor", 128, 128).unwrap();
            project.move_level(neighbor, 520, 0, 0).unwrap();
            let original = project.json.clone();
            app.world_toggle_selection(anchor);
            app.world_toggle_selection(other);
            let ctx = egui::Context::default();
            ctx.options_mut(|options| options.zoom_factor = density);
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
            let start = egui::Pos2::from(app.viewport.to_screen(
                [100., 100.],
                [0., 0.],
                rect.size().into(),
            ));
            let end = start + egui::vec2(40., 0.);
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(start)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![marquee_button(start, true, none)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(end)],
            );
            assert_eq!(app.project.as_ref().unwrap().json, original);
            assert!(!app.dirty);
            let mut events = vec![marquee_button(end, false, up)];
            if move_after_drop {
                events.push(egui::Event::PointerMoved(end + egui::vec2(80., 0.)));
            }
            marquee_frame(&mut app, &ctx, rect, frame_modifiers, events);
            let project = app.project.as_mut().unwrap();
            assert_eq!(
                project.level(anchor).unwrap()["worldX"],
                expected,
                "DPR {density}, mouseup {up:?}, frame {frame_modifiers:?}"
            );
            assert_eq!(project.level(other).unwrap()["worldX"], 320 + expected);
            assert_eq!(project.level(neighbor).unwrap()["worldX"], 520);
            assert!(app.dirty);
            assert!(app.history.undo(project));
            assert_eq!(project.json, original);
        }
    }

    #[test]
    fn fractional_world_drag_uses_render_pixels_for_preview_and_drop() {
        for grouped in [false, true] {
            let mut app = editor();
            app.fit = false;
            app.grid = false;
            app.viewport.zoom = 1.5;
            app.viewport.center = [320.26666, 128.];
            let anchor = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            project.move_level(anchor, 320, 0, 0).unwrap();
            let other = project.add_level(None, "Second", 128, 128).unwrap();
            project.move_level(other, 650, 0, 0).unwrap();
            if grouped {
                app.world_toggle_selection(anchor);
                app.world_toggle_selection(other);
            }
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
            let press = egui::pos2(500., 350.);
            let held = egui::pos2(526., 350.);
            let release = egui::pos2(529., 350.);
            let alt = egui::Modifiers::ALT;
            let level_x = |app: &EditorApp, level| {
                app.project.as_ref().unwrap().level(level).unwrap()["worldX"]
                    .as_i64()
                    .unwrap()
            };
            for event in [
                egui::Event::PointerMoved(press),
                marquee_button(press, true, alt),
                egui::Event::PointerMoved(held),
            ] {
                marquee_frame(&mut app, &ctx, rect, alt, vec![event]);
            }
            assert_eq!(level_x(&app, anchor), if grouped { 320 } else { 337 });
            if grouped {
                let Some(tools::Gesture::WorldGroupMove { start, end, .. }) = app.gesture else {
                    panic!("expected a group preview");
                };
                assert_eq!(app.world_group_delta(anchor, start, end), [17, 0]);
            }
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                alt,
                vec![marquee_button(release, false, alt)],
            );
            assert_eq!(level_x(&app, anchor), if grouped { 339 } else { 337 });
            assert_eq!(level_x(&app, other), if grouped { 669 } else { 650 });
        }
    }

    #[test]
    fn grouped_drag_rounds_the_negative_half_world_press_before_placing_levels() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        app.viewport.zoom = 2.;
        let anchor = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.move_level(anchor, -64, 0, 0).unwrap();
        let other = project.add_level(None, "Second", 128, 128).unwrap();
        project.move_level(other, 320, 0, 0).unwrap();
        app.world_toggle_selection(anchor);
        app.world_toggle_selection(other);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [-31.5, 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(80., 0.);
        let alt = egui::Modifiers::ALT;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            alt,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            alt,
            vec![marquee_button(start, true, alt)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            alt,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            alt,
            vec![marquee_button(end, false, alt)],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level(anchor).unwrap()["worldX"], -23);
        assert_eq!(project.level(other).unwrap()["worldX"], 361);
    }

    #[test]
    fn preview_154_pointer_drag_moves_selected_levels_on_release() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [100., 100.],
            rect.min.into(),
            rect.size().into(),
        ));
        let end = start + egui::vec2(40., 80.);
        let modifiers = egui::Modifiers::NONE;
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        assert_eq!(
            app.project.as_ref().unwrap().level(first).unwrap()["worldX"],
            0
        );
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(first).unwrap()["worldX"],
            40
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(second).unwrap()["worldX"],
            360
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().level(second).unwrap()["worldX"],
            320
        );
    }

    #[test]
    fn preview_154_command_drag_keeps_and_moves_selected_levels() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [100., 100.],
            rect.min.into(),
            rect.size().into(),
        ));
        let end = start + egui::vec2(40., 80.);
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        assert!(!app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        assert!(app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            app.world_selection,
            vec![
                project.level(second).unwrap()["iid"].as_str().unwrap(),
                project.level(first).unwrap()["iid"].as_str().unwrap()
            ]
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(first).unwrap()["worldX"],
            0
        );
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(first).unwrap()["worldX"],
            40
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(second).unwrap()["worldX"],
            360
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().level(second).unwrap()["worldX"],
            320
        );
    }

    #[test]
    fn group_copy_is_live_at_the_threshold_and_snaps_the_same_copies_on_drop() {
        for (density, keep_alt, blur, expected_x) in [
            (1., false, false, 72),
            (2., false, false, 40),
            (1., true, false, 40),
            (1., true, true, 72),
        ] {
            let mut app = editor();
            app.fit = false;
            app.grid = false;
            app.viewport.center = [256., 256.];
            let anchor = app.selected.unwrap();
            let p = app.project.as_mut().unwrap();
            let other = p.add_level(None, "Second", 128, 128).unwrap();
            p.move_level(other, 320, 0, 0).unwrap();
            let neighbor = p.add_level(None, "Neighbor", 128, 128).unwrap();
            p.move_level(neighbor, 520, 320, 0).unwrap();
            let original = p.json.clone();
            let originals = [anchor, other]
                .map(|key| p.level(key).unwrap()["iid"].as_str().unwrap().to_owned());
            app.world_toggle_selection(anchor);
            app.world_toggle_selection(other);
            let ctx = egui::Context::default();
            ctx.options_mut(|options| options.zoom_factor = density);
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 850.));
            let start = egui::Pos2::from(app.viewport.to_screen(
                [100., 100.],
                [0., 0.],
                rect.size().into(),
            ));
            let end = start + egui::vec2(40., 320.);
            let none = egui::Modifiers::NONE;
            let copy = egui::Modifiers {
                alt: true,
                command: true,
                ..Default::default()
            };
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(start)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![marquee_button(start, true, none)],
            );
            assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 3);
            let live = marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![egui::Event::PointerMoved(end)],
            );
            let p = app.project.as_ref().unwrap();
            assert_eq!(p.level_keys().len(), 5);
            assert!(app.dirty);
            let copies = app.world_selection.clone();
            assert_eq!(copies.len(), 2);
            assert!(copies.iter().all(|iid| !originals.contains(iid)));
            assert_eq!(p.level(app.selected.unwrap()).unwrap()["iid"], copies[0]);
            let point = |p: &Project, iid: &str| {
                let level = p
                    .level_keys()
                    .into_iter()
                    .find_map(|key| p.level(key).filter(|l| l["iid"] == iid))
                    .unwrap();
                [
                    level["worldX"].as_i64().unwrap(),
                    level["worldY"].as_i64().unwrap(),
                ]
            };
            assert_eq!(point(p, &copies[0]), [0, 0]);
            assert_eq!(point(p, &copies[1]), [320, 0]);
            assert!(live.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(r) if r.fill == egui::Color32::from_rgba_unmultiplied(0, 255, 0, 38) && r.rect.left() == start.x - 100. + 40.)));

            let release = if keep_alt { egui::Modifiers::ALT } else { none };
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                release,
                vec![if blur {
                    egui::Event::WindowFocused(false)
                } else {
                    marquee_button(end, false, release)
                }],
            );
            let project = app.project.as_mut().unwrap();
            assert_eq!(project.level_keys().len(), 5);
            assert_eq!(app.world_selection, copies);
            assert_eq!(point(project, &copies[0]), [expected_x, 320]);
            assert_eq!(point(project, &copies[1]), [320 + expected_x, 320]);
            assert_eq!(point(project, &originals[0]), [0, 0]);
            assert_eq!(point(project, &originals[1]), [320, 0]);
            assert!(app.history.undo(project));
            assert_eq!(project.json, original);
            assert!(!app.history.undo(project));
        }
    }

    #[test]
    fn group_copy_uses_a_threshold_move_in_the_same_input_batch_as_press() {
        let mut app = editor();
        app.fit = false;
        let anchor = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let other = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(other, 320, 0, 0).unwrap();
        app.world_toggle_selection(anchor);
        app.world_toggle_selection(other);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(40., 80.);
        let none = egui::Modifiers::NONE;
        let copy = egui::Modifiers {
            alt: true,
            command: true,
            ..Default::default()
        };
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![
                marquee_button(start, true, none),
                egui::Event::PointerMoved(end),
            ],
        );
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 4);
    }

    #[test]
    fn group_drag_only_samples_the_copy_modifiers_at_the_first_threshold() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        let anchor = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let other = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(other, 320, 0, 0).unwrap();
        app.world_toggle_selection(anchor);
        app.world_toggle_selection(other);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(40., 80.);
        let copy = egui::Modifiers {
            alt: true,
            command: true,
            ..Default::default()
        };
        let none = egui::Modifiers::NONE;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![marquee_button(start, true, copy)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(start + egui::vec2(2., 0.))],
        );
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(end + egui::vec2(4., 0.))],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(end, false, none)],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level_keys().len(), 2);
        assert_eq!(project.level(anchor).unwrap()["worldX"], 40);
        assert_eq!(project.level(other).unwrap()["worldX"], 360);
    }

    #[test]
    fn escape_rolls_back_a_live_group_copy_before_leaving_world_view() {
        let mut app = editor();
        app.fit = false;
        let anchor = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let other = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(other, 320, 0, 0).unwrap();
        let original = p.json.clone();
        app.world_toggle_selection(anchor);
        app.world_toggle_selection(other);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let none = egui::Modifiers::NONE;
        let copy = egui::Modifiers {
            alt: true,
            command: true,
            ..Default::default()
        };
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            none,
            vec![marquee_button(start, true, none)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(start + egui::vec2(40., 80.))],
        );
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 4);
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: Some(Key::Escape),
                    pressed: true,
                    repeat: false,
                    modifiers: none,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.gesture.is_none());
        assert_eq!(app.selected, Some(anchor));
        assert!(!app.world);
        assert!(app.world_selection.is_empty());
        assert!(!app.dirty);
        let project = app.project.as_mut().unwrap();
        assert_eq!(project.json, original);
        assert!(!app.history.undo(project));
    }

    #[test]
    fn preview_154_alt_command_drag_duplicates_selected_levels() {
        let mut app = editor();
        app.fit = false;
        app.grid = false;
        app.viewport.center = [256., 128.];
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Second", 128, 128).unwrap();
        p.move_level(second, 320, 0, 0).unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [100., 100.],
            rect.min.into(),
            rect.size().into(),
        ));
        let end = start + egui::vec2(40., 80.);
        let modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
        );
        frame(&mut app, vec![egui::Event::PointerMoved(end)]);
        assert_eq!(
            app.project.as_ref().unwrap().level(first).unwrap()["worldX"],
            0
        );
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
        );
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 4);
        assert_eq!(app.world_selection.len(), 2);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);
    }

    #[test]
    fn preview_154_alt_command_click_deselects_without_copying() {
        let mut app = editor();
        app.fit = false;
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 128, 128)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 320, 0, 0)
            .unwrap();
        app.world_toggle_selection(first);
        app.world_toggle_selection(second);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let pos = egui::Pos2::from(app.viewport.to_screen(
            [100., 100.],
            rect.min.into(),
            rect.size().into(),
        ));
        let modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let frame = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), egui::Sense::click_and_drag());
                            app.canvas_controls(ctx, canvas, &response);
                        });
                },
            );
        };
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                }],
            );
        }
        assert!(!app.world_selection_contains(first));
        assert!(app.world_selection_contains(second));
        assert_eq!(app.project.as_ref().unwrap().level_keys().len(), 2);
    }

    #[test]
    fn preview_154_linear_control_drag_of_last_selected_level_reorders_it() {
        let mut app = editor();
        app.fit = false;
        app.viewport.center = [256., 0.];
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_level(None, "Second", 128, 128).unwrap();
        project.json["worldLayout"] = json!("LinearHorizontal");
        project.refresh_derived_data().unwrap();
        let iid = project.level(first).unwrap()["iid"].clone();
        app.world_toggle_selection(first);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(450., 0.);
        let command = egui::Modifiers::COMMAND;
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![marquee_button(start, true, command)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![marquee_button(end, false, command)],
        );
        assert_eq!(app.project.as_ref().unwrap().json["levels"][1]["iid"], iid);
        assert!(app.world_selection.is_empty());
    }

    #[test]
    fn linear_drag_cannot_copy_when_the_world_has_only_one_level() {
        let mut app = editor();
        app.fit = false;
        let first = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.json["worldLayout"] = json!("LinearHorizontal");
        project.refresh_derived_data().unwrap();
        let before = project.json.clone();
        app.world_toggle_selection(first);
        let ctx = egui::Context::default();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
        let start = egui::Pos2::from(app.viewport.to_screen(
            [64., 64.],
            [0., 0.],
            rect.size().into(),
        ));
        let end = start + egui::vec2(80., 0.);
        let command = egui::Modifiers::COMMAND;
        let copy = egui::Modifiers {
            alt: true,
            ..command
        };
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![egui::Event::PointerMoved(start)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![marquee_button(start, true, command)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            copy,
            vec![egui::Event::PointerMoved(end)],
        );
        marquee_frame(
            &mut app,
            &ctx,
            rect,
            command,
            vec![marquee_button(end, false, command)],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.json, before);
        assert_eq!(app.selected, Some(first));
        assert!(app.world_selection.is_empty());
    }

    #[test]
    fn preview_154_linear_world_cannot_duplicate_selected_levels_by_dragging() {
        for layout in ["LinearHorizontal", "LinearVertical"] {
            let mut app = editor();
            let first = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            project.add_level(None, "Second", 128, 128).unwrap();
            project.json["worldLayout"] = json!(layout);
            let second = project.level_keys()[1];
            app.world_toggle_selection(first);
            app.world_toggle_selection(second);
            let before = app.project.as_ref().unwrap().json.clone();
            app.fit = false;
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 700.));
            let start = egui::Pos2::from(app.viewport.to_screen(
                [64., 64.],
                [0., 0.],
                rect.size().into(),
            ));
            let end = start + egui::vec2(64., 32.);
            let none = egui::Modifiers::NONE;
            let copy = egui::Modifiers {
                alt: true,
                command: true,
                ..Default::default()
            };
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![egui::Event::PointerMoved(start)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                none,
                vec![marquee_button(start, true, none)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![egui::Event::PointerMoved(end)],
            );
            marquee_frame(
                &mut app,
                &ctx,
                rect,
                copy,
                vec![marquee_button(end, false, copy)],
            );
            assert_eq!(app.project.as_ref().unwrap().json, before, "{layout}");
            assert_eq!(app.world_selection.len(), 2);
        }
    }

    #[test]
    fn preview_154_world_selection_is_cleared_when_switching_world_or_leaving_world_mode() {
        let mut app = editor();
        let first = app.selected.unwrap();
        app.world_toggle_selection(first);
        app.navigate_world(None);
        assert!(app.world_selection.is_empty());
        app.world_toggle_selection(first);
        app.panel = Some(Panel::World);
        app.show_panel(None);
        assert!(app.world_selection.is_empty());
    }

    #[test]
    fn linear_world_pointer_hit_uses_computed_positions_gaps_and_depth() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let world = app.project.as_mut().unwrap();
        let second = world.add_level(None, "Second", 128, 128).unwrap();
        world.move_level(second, 9_000, 9_000, 1).unwrap();
        world.json["worldLayout"] = json!("LinearHorizontal");
        assert_eq!(app.level_at_world([255.49, 10.]), Some(first));
        assert_eq!(app.level_at_world([255.5, 10.]), None);
        assert_eq!(app.level_at_world([304., 10.]), None);
        app.world_depth = 1;
        assert_eq!(app.level_at_world([303.5, 10.]), Some(second));
        assert_eq!(app.level_at_world([432., 10.]), None);
        app.project.as_mut().unwrap().json["worldLayout"] = json!("LinearVertical");
        assert_eq!(app.level_at_world([10., 303.5]), Some(second));
        assert_eq!(app.level_at_world([10., 432.]), None);
    }

    #[test]
    fn world_hit_only_selects_the_active_depth_and_prefers_last_same_depth_level() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let second = p.add_level(None, "Overlap", 256, 256).unwrap();
        p.move_level(second, 0, 0, 1).unwrap();
        assert_eq!(app.level_at_world([30., 30.]), Some(first));
        app.world_depth = 1;
        assert_eq!(app.level_at_world([30., 30.]), Some(second));
        app.world_depth = 2;
        assert_eq!(app.level_at_world([30., 30.]), None);
        app.project
            .as_mut()
            .unwrap()
            .move_level(second, 0, 0, 0)
            .unwrap();
        app.world_depth = 0;
        assert_eq!(app.level_at_world([30., 30.]), Some(second));
    }
    #[test]
    fn free_drag_uses_smallest_nonentity_grid_and_truncates_negative_coordinates() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        p.add_layer("Objects", "Entities", 4).unwrap();
        p.add_layer("Ground", "IntGrid", 16).unwrap();
        p.add_layer("Fine", "Tiles", 8).unwrap();
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [19., -19.], [0, 0], true, 1.),
            [16, -16]
        );
        app.grid = false;
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [19., -19.], [0, 0], true, 1.),
            [19, -19]
        );
    }
    #[test]
    fn free_drag_snaps_near_edges_unless_modifiers_disable_and_does_not_snap_into_overlap() {
        let mut app = editor();
        app.grid = false;
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let other = p.add_level(None, "Neighbour", 256, 256).unwrap();
        p.move_level(other, 300, 0, 0).unwrap();
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], false, 1.),
            [44, 0]
        );
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], true, 1.),
            [40, 0]
        );
        let p = app.project.as_mut().unwrap();
        let blocker = p.add_level(None, "Blocker", 16, 256).unwrap();
        p.move_level(blocker, 290, 0, 3).unwrap(); // Other depths also block snapping in the source.
        assert_eq!(
            app.world_drag_position(key, [0., 0.], [40., 0.], [0, 0], false, 1.),
            [34, 0]
        );
    }
    #[test]
    fn gridvania_drag_floors_negative_world_positions() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(256);
        p.json["worldGridHeight"] = json!(128);
        assert_eq!(
            app.world_drag_position(key, [10., 10.], [-1., 140.], [0, 0], false, 1.),
            [-256, 128]
        );
    }
}
