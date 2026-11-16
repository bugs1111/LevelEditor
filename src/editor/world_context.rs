//! World canvas context actions and the source level-location picker.
use super::*;
use serde_json::json;

#[derive(Default)]
pub(super) struct WorldContext {
    menu: Option<WorldMenu>,
    picking: bool,
    pointer: Option<[f32; 2]>,
    no_room: bool,
}
#[derive(Clone, Copy)]
struct WorldMenu {
    screen: egui::Pos2,
    position: [f32; 2],
    level: Option<LevelKey>,
}
#[derive(Clone, Copy, Debug)]
struct Spot {
    rect: [f32; 4],
    size: [i32; 2],
    index: Option<usize>,
    overlaps: bool,
}

fn container(p: &Project, world: Option<usize>) -> &Value {
    world
        .and_then(|i| p.json["worlds"].get(i))
        .unwrap_or(&p.json)
}
fn round(value: f32) -> f32 {
    (value + 0.5).floor()
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[0] + a[2] > b[0] && a[1] < b[3] && a[1] + a[3] > b[1]
}
fn normalize_linear(p: &mut Project, world: Option<usize>) {
    if text(container(p, world), "worldLayout", "Free").starts_with("Linear") {
        for key in p.level_keys().into_iter().filter(|k| k.world == world) {
            if let Some(level) = p.level_mut(key) {
                level["worldX"] = json!(-1);
                level["worldY"] = json!(-1);
            }
        }
    }
}
fn level_name(p: &Project, base: &str) -> String {
    panels::unique_name(
        &p.cleanup_identifier(base),
        p.level_keys()
            .into_iter()
            .filter_map(|key| p.level(key)?["identifier"].as_str()),
    )
}
impl EditorApp {
    fn context_placements(&self) -> Vec<(LevelKey, [f32; 4])> {
        self.project
            .as_ref()
            .map(|p| {
                canvas::placements(
                    p,
                    LevelKey {
                        world: self.active_world,
                        level: 0,
                    },
                )
            })
            .unwrap_or_default()
    }
    fn context_pointer(&self, rect: Rect, pointer: egui::Pos2) -> [f32; 2] {
        let mut point = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        if !self.world
            && let Some((_, b)) = self
                .context_placements()
                .into_iter()
                .find(|(k, _)| Some(*k) == self.selected)
        {
            point[0] += b[0];
            point[1] += b[1];
        }
        point.map(round)
    }
    pub(super) fn world_context_menu(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        if self.project.as_ref().is_none_or(Project::is_backup)
            || self.is_busy()
            || self.pending.is_some()
            || self.crash.is_some()
            || self.clicktrap
            || text_editor::is_open(ctx)
            || world_form_dialog::is_open(ctx)
            || help::changelog_is_open(ctx)
            || self.command_palette
            || self.command_open
            || !self.resource_missing.is_empty()
        {
            self.cancel_world_context();
            return false;
        }
        if self.world_context.picking {
            if !self.world {
                self.cancel_world_context();
                return false;
            }
            if self.fit {
                if let (Some(p), Some(key)) = (&self.project, self.selected) {
                    self.viewport
                        .fit_world(canvas::bounds(p, key, true), rect.size().into());
                }
                self.fit = false;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel_world_context();
                return true;
            }
            if response.hovered()
                && let Some(pointer) = ctx.pointer_hover_pos()
            {
                let (pan, delta, scroll, primary, space) = ctx.input(|i| {
                    (
                        i.pointer.button_down(egui::PointerButton::Middle),
                        i.pointer.delta(),
                        i.raw_scroll_delta.y,
                        i.pointer.primary_pressed(),
                        i.key_down(egui::Key::Space),
                    )
                });
                if pan || (space && ctx.input(|i| i.pointer.primary_down())) {
                    self.viewport.center[0] -= delta.x / self.viewport.zoom;
                    self.viewport.center[1] -= delta.y / self.viewport.zoom;
                }
                if scroll != 0. {
                    self.wheel_zoom(ctx, rect, pointer, scroll);
                }
                if !self.world_context.picking {
                    return true;
                }
                let position = self.context_pointer(rect, pointer);
                self.world_context.pointer = Some(position);
                if primary && !space {
                    self.create_level_at(position, true);
                }
            }
            return true;
        }
        if response.hovered()
            && ctx.input(|i| {
                i.pointer.secondary_pressed()
                    && (i.modifiers.is_none()
                        || cfg!(target_os = "macos")
                            && i.modifiers.ctrl
                            && !i.modifiers.alt
                            && !i.modifiers.shift
                            && !i.modifiers.command)
            })
            && let Some(pointer) = ctx.pointer_hover_pos()
        {
            let position = self.context_pointer(rect, pointer);
            let hit = self.level_at_world(position);
            if self.world || hit.is_none() {
                if let Some(key) = hit {
                    self.navigate_level(key, false);
                }
                self.world_context.menu = Some(WorldMenu {
                    screen: pointer,
                    position,
                    level: hit,
                });
            }
        }
        if self.world_context.no_room {
            let mut pick = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("world-no-room")).show(ctx, |ui| {
                ui.label("No room for a level here! Do you want to pick another location?");
                ui.horizontal(|ui| {
                    pick = ui.button("Pick another location").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if pick {
                self.start_level_spot_picker();
            } else if cancel {
                self.cancel_world_context();
            }
            return true;
        }
        let Some(menu) = self.world_context.menu else {
            return false;
        };
        enum Action {
            New,
            Duplicate(LevelKey),
            Delete(LevelKey),
            World(usize),
            Move(LevelKey, usize),
        }
        let mut action = None;
        let mut open = true;
        // Source opens on button-down; the corresponding right-button release must not close it.
        let close = if ctx.input(|i| {
            i.pointer.button_down(egui::PointerButton::Secondary)
                || i.pointer.button_released(egui::PointerButton::Secondary)
        }) {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        };
        egui::Popup::menu(response)
            .id(egui::Id::new("world-context-menu"))
            .at_position(menu.screen)
            .open_bool(&mut open)
            .close_behavior(close)
            .show(|ui| {
                if ui.button("New level").clicked() {
                    action = Some(Action::New);
                }
                if let Some(key) = menu.level {
                    if ui.button("Duplicate").clicked() {
                        action = Some(Action::Duplicate(key));
                    }
                    if ui.button("Delete").clicked() {
                        action = Some(Action::Delete(key));
                    }
                }
                if let Some(worlds) = self
                    .project
                    .as_ref()
                    .and_then(|p| p.json["worlds"].as_array())
                    .filter(|w| w.len() > 1)
                {
                    ui.separator();
                    ui.label(if menu.level.is_some() {
                        "Move this level to:"
                    } else {
                        "Go to world:"
                    });
                    for (index, world) in worlds.iter().enumerate() {
                        let label = format!(
                            "{} ({} levels)",
                            text(world, "identifier", "World"),
                            world["levels"].as_array().map_or(0, Vec::len)
                        );
                        if ui
                            .add_enabled(Some(index) != self.active_world, egui::Button::new(label))
                            .clicked()
                        {
                            action = Some(
                                menu.level
                                    .map_or(Action::World(index), |key| Action::Move(key, index)),
                            );
                        }
                    }
                }
            });
        if !open || action.is_some() {
            self.world_context.menu = None;
        }
        match action {
            Some(Action::New) => {
                if !self.create_level_at(menu.position, false) {
                    self.world_context.no_room = true;
                }
            }
            Some(Action::Duplicate(key)) => self.duplicate_context_level(key),
            Some(Action::Delete(key)) => self.delete_context_level(key),
            Some(Action::World(world)) => self.navigate_world(Some(world)),
            Some(Action::Move(key, world)) => self.move_context_level(key, world),
            None => {}
        }
        true
    }
    pub(super) fn world_context_overlay(&mut self, ui: &egui::Ui, rect: Rect) {
        if !self.world_context.picking {
            return;
        }
        if let Some(spot) = self
            .world_context
            .pointer
            .and_then(|p| self.level_insert_spot(p))
        {
            let [x, y, w, h] = spot.rect;
            let a = self
                .viewport
                .to_screen([x, y], rect.min.into(), rect.size().into());
            let b = self
                .viewport
                .to_screen([x + w, y + h], rect.min.into(), rect.size().into());
            let preview = Rect::from_min_max(a.into(), b.into());
            let color = if spot.overlaps {
                Color32::from_rgb(255, 90, 90)
            } else {
                Color32::from_rgb(100, 180, 255)
            };
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(preview, 0., color.gamma_multiply(0.15));
            painter.rect_stroke(
                preview,
                0.,
                Stroke::new(2_f32, color),
                egui::StrokeKind::Inside,
            );
            painter.text(
                preview.center(),
                egui::Align2::CENTER_CENTER,
                "+",
                egui::FontId::proportional(36.),
                color,
            );
        }
        let mut cancel = false;
        egui::Area::new(egui::Id::new("level-spot-picker-instructions"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.center_top() + egui::vec2(-150., 12.))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Pick a location for the new level");
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            });
        if cancel {
            self.cancel_world_context();
        }
    }
    pub(super) fn cancel_world_context(&mut self) {
        self.world_context = WorldContext::default();
    }
    pub(super) fn start_level_spot_picker(&mut self) {
        if self.project.as_ref().is_none_or(Project::is_backup) {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        if !self.world {
            self.world = true;
            self.fit = true;
        }
        self.world_context = WorldContext {
            picking: true,
            ..Default::default()
        };
    }
    fn level_insert_spot(&self, position: [f32; 2]) -> Option<Spot> {
        let p = self.project.as_ref()?;
        let world = container(p, self.active_world);
        let size = [
            number(world, "defaultLevelWidth", 256.) as i32,
            number(world, "defaultLevelHeight", 256.) as i32,
        ];
        let [w, h] = size.map(|v| v as f32);
        let position = position.map(round);
        let mut spot = Spot {
            rect: [position[0] - w * 0.5, position[1] - h * 0.5, w, h],
            size,
            index: None,
            overlaps: false,
        };
        let placements = self.context_placements();
        let layout = text(world, "worldLayout", "Free");
        if layout.starts_with("Linear") {
            let axis = usize::from(layout == "LinearVertical");
            let mut points: Vec<_> = placements
                .iter()
                .enumerate()
                .map(|(index, (_, b))| (index, b[axis]))
                .collect();
            points.push((
                placements.len(),
                placements.last().map_or(0., |(_, b)| b[axis + 2]),
            ));
            let (index, coord) = points.into_iter().min_by(|a, b| {
                (position[axis] - a.1)
                    .abs()
                    .total_cmp(&(position[axis] - b.1).abs())
            })?;
            spot.index = Some(index);
            if axis == 0 {
                spot.rect = [coord - w * 0.5, -h * 0.1, w, h + (h * 0.2).trunc()];
            } else {
                spot.rect = [-w * 0.1, coord - h * 0.5, w + (w * 0.2).trunc(), h];
            }
        } else {
            spot.overlaps = placements.iter().any(|(_, b)| {
                position[0] >= b[0]
                    && position[0] < b[2]
                    && position[1] >= b[1]
                    && position[1] < b[3]
            });
            if !spot.overlaps {
                for (_, b) in &placements {
                    if overlaps(spot.rect, *b) {
                        let center = [round((b[0] + b[2]) * 0.5), round((b[1] + b[3]) * 0.5)];
                        let bw = b[2] - b[0];
                        let bh = b[3] - b[1];
                        let slope = (spot.rect[1] + h * 0.5 - center[1])
                            / (spot.rect[0] + w * 0.5 - center[0]);
                        if slope * bw * 0.5 >= -bh * 0.5 && slope * bw * 0.5 <= bh * 0.5 {
                            spot.rect[0] = if spot.rect[0] < center[0] {
                                b[0] - w
                            } else {
                                b[2]
                            };
                        } else {
                            spot.rect[1] = if spot.rect[1] < center[1] {
                                b[1] - h
                            } else {
                                b[3]
                            };
                        }
                    }
                }
                spot.overlaps = placements.iter().any(|(_, b)| overlaps(spot.rect, *b));
            }
            if layout == "GridVania" || self.grid {
                let grid = if layout == "GridVania" {
                    [
                        number(world, "worldGridWidth", 256.),
                        number(world, "worldGridHeight", 256.),
                    ]
                } else {
                    [number(&p.json, "defaultGridSize", 16.); 2]
                };
                for (axis, grid) in grid.into_iter().enumerate() {
                    spot.rect[axis] = round(spot.rect[axis] / grid.max(1.)) * grid.max(1.);
                }
            }
        }
        Some(spot)
    }
    fn create_level_at(&mut self, position: [f32; 2], reject_overlap: bool) -> bool {
        let Some(spot) = self.level_insert_spot(position) else {
            return false;
        };
        if reject_overlap && spot.overlaps {
            return false;
        }
        let world = self.active_world;
        let depth = self.world_depth as i32;
        let key = self.edit("Create level", |p| {
            let name = level_name(p, "Level1");
            let mut key = p.add_level(world, &name, spot.size[0], spot.size[1])?;
            if let Some(index) = spot.index {
                key = p.move_level_to_world(key, world, index)?;
                normalize_linear(p, world);
            } else {
                p.move_level(
                    key,
                    round(spot.rect[0]) as i32,
                    round(spot.rect[1]) as i32,
                    depth,
                )?;
            }
            p.level_mut(key).unwrap()["useAutoIdentifier"] = json!(true);
            p.refresh_derived_data()?;
            Ok(key)
        });
        if let Some(key) = key {
            self.navigate_level(key, false);
            self.center_context_level(key);
            true
        } else {
            false
        }
    }
    fn center_context_level(&mut self, key: LevelKey) {
        if let Some((_, b)) = self
            .context_placements()
            .into_iter()
            .find(|(k, _)| *k == key)
        {
            self.viewport.center = if self.world {
                [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5]
            } else {
                [(b[2] - b[0]) * 0.5, (b[3] - b[1]) * 0.5]
            };
        }
    }
    pub(super) fn duplicate_context_level(&mut self, key: LevelKey) {
        if let Some(copy) = self.edit("Duplicate level", |p| {
            let source = p.level(key).unwrap();
            let auto = source["useAutoIdentifier"].clone();
            let name = level_name(p, text(source, "identifier", "Level1"));
            let copy = p.duplicate_level(key, &name)?;
            if !text(container(p, key.world), "worldLayout", "Free").starts_with("Linear") {
                let level = p.level(copy).unwrap();
                let delta = number(&p.json, "defaultGridSize", 16.) as i32 * 4;
                p.move_level(
                    copy,
                    number(level, "worldX", 0.) as i32 + delta,
                    number(level, "worldY", 0.) as i32 + delta,
                    number(level, "worldDepth", 0.) as i32,
                )?;
            } else {
                normalize_linear(p, key.world);
            }
            p.level_mut(copy).unwrap()["useAutoIdentifier"] = auto;
            p.refresh_derived_data()?;
            Ok(copy)
        }) {
            self.navigate_level(copy, false);
        }
    }
    pub(super) fn delete_context_level(&mut self, key: LevelKey) {
        let placements = self.context_placements();
        if placements.len() <= 1 {
            self.status = "You can't delete the last level.".into();
            return;
        }
        let Some((_, b)) = placements.iter().find(|(k, _)| *k == key) else {
            return;
        };
        let closest = placements
            .iter()
            .filter(|(k, _)| *k != key)
            .min_by(|(_, a), (_, c)| {
                let distance = |a: &[f32; 4]| {
                    ((a[0] - b[2]).max(0.) + (b[0] - a[2]).max(0.))
                        .max((a[1] - b[3]).max(0.) + (b[1] - a[3]).max(0.))
                };
                distance(a).total_cmp(&distance(c))
            })
            .map(|(k, _)| *k)
            .unwrap();
        let next = LevelKey {
            level: closest.level - usize::from(closest.level > key.level),
            ..closest
        };
        if self
            .edit_recoverable("Delete level", |p| p.remove_level(key))
            .is_some()
        {
            self.navigate_level(next, false);
            self.center_context_level(next);
        }
    }
    fn move_context_level(&mut self, key: LevelKey, world: usize) {
        if let Some(target) = self.edit("Move level to world", |p| {
            let count = p
                .level_keys()
                .iter()
                .filter(|k| k.world == Some(world))
                .count();
            let origin = canvas::placements(p, key)
                .into_iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, bounds)| [bounds[0] as i32, bounds[1] as i32]);
            let target = p.move_level_to_world(key, Some(world), count)?;
            if !text(container(p, Some(world)), "worldLayout", "Free").starts_with("Linear")
                && let Some(origin) = origin
            {
                // Linear JSON uses -1; World.moveToWorld keeps the source runtime position.
                let level = p.level_mut(target).unwrap();
                level["worldX"] = json!(origin[0]);
                level["worldY"] = json!(origin[1]);
            }
            normalize_linear(p, key.world);
            normalize_linear(p, Some(world));
            p.refresh_derived_data()?;
            Ok(target)
        }) {
            self.world = true;
            self.navigate_level(target, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const K: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    fn editor() -> EditorApp {
        let mut a = EditorApp::empty();
        a.set_project(Project::new());
        a.world = true;
        a.fit = false;
        a
    }
    #[test]
    fn free_and_gridvania_spots_match_source_deinterlace_and_grid_rounding() {
        let mut a = editor();
        let spot = a.level_insert_spot([300., 128.]).unwrap();
        assert_eq!(spot.rect, [256., 0., 256., 256.]);
        assert!(!spot.overlaps);
        assert!(a.level_insert_spot([128., 128.]).unwrap().overlaps);
        let p = a.project.as_mut().unwrap();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(128);
        p.json["worldGridHeight"] = json!(64);
        p.json["defaultLevelWidth"] = json!(128);
        p.json["defaultLevelHeight"] = json!(64);
        assert_eq!(
            a.level_insert_spot([330., 300.]).unwrap().rect,
            [256., 256., 128., 64.]
        );
    }
    #[test]
    fn linear_picker_inserts_at_nearest_boundary_and_reorganizes_both_axes() {
        for (layout, point, axis) in [
            ("LinearHorizontal", [290., 0.], 0),
            ("LinearVertical", [0., 290.], 1),
        ] {
            let mut a = editor();
            let p = a.project.as_mut().unwrap();
            p.add_level(None, "Second", 128, 128).unwrap();
            p.json["worldLayout"] = json!(layout);
            p.json["defaultLevelWidth"] = json!(96);
            p.json["defaultLevelHeight"] = json!(96);
            assert_eq!(a.level_insert_spot(point).unwrap().index, Some(1));
            assert!(a.create_level_at(point, true));
            let p = a.project.as_ref().unwrap();
            let key = a.selected.unwrap();
            assert_eq!(key.level, 1);
            assert_eq!(p.level(key).unwrap()["pxWid"], 96);
            assert_eq!(p.level(key).unwrap()["useAutoIdentifier"], true);
            let coord = if axis == 0 { "worldX" } else { "worldY" };
            assert_eq!(p.level(key).unwrap()[coord], -1);
            let placements = canvas::placements(p, key);
            assert_eq!(placements[1].1[axis], 304.);
            assert_eq!(placements[2].1[axis], 448.);
            assert!(a.world);
            assert!(a.history.can_undo());
        }
    }
    #[test]
    fn source_direct_new_allows_overlap_but_picker_rejects_it() {
        let mut a = editor();
        assert!(!a.create_level_at([128., 128.], true));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.create_level_at([128., 128.], false));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    #[test]
    fn duplicate_offset_and_delete_closest_last_chance_match_source() {
        let mut a = editor();
        a.duplicate_context_level(K);
        let p = a.project.as_ref().unwrap();
        assert_eq!(p.level_keys().len(), 2);
        assert_eq!(p.level(a.selected.unwrap()).unwrap()["worldX"], 64);
        a.delete_context_level(a.selected.unwrap());
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
        a.delete_context_level(K);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        a.restore_last_chance();
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    fn frame(
        a: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        a.repaint = Some(ctx.clone());
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(800., 600.), Sense::click_and_drag());
                        a.canvas_input(ctx, rect, &response);
                        a.world_context_overlay(ui, rect);
                    });
            },
        )
    }
    fn button(
        pos: egui::Pos2,
        button: egui::PointerButton,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        }
    }
    fn label_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, label))
            .unwrap_or_else(|| panic!("missing menu label {label}"))
    }
    fn right_click(a: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) -> egui::FullOutput {
        frame(
            a,
            ctx,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Secondary,
                true,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Secondary,
                false,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        )
    }
    fn click(a: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        frame(
            a,
            ctx,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Primary,
                true,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Primary,
                false,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
    }
    #[test]
    fn actual_world_right_click_menu_duplicates_and_deletes_with_last_chance() {
        let mut a = editor();
        a.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        let pos = egui::pos2(400., 300.);
        let output = right_click(&mut a, &ctx, pos);
        let duplicate = label_center(&output, "Duplicate");
        click(&mut a, &ctx, duplicate);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
        let output = right_click(&mut a, &ctx, pos);
        let delete = label_center(&output, "Delete");
        click(&mut a, &ctx, delete);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
        let output = right_click(&mut a, &ctx, pos);
        let delete = label_center(&output, "Delete");
        click(&mut a, &ctx, delete);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
    }
    #[test]
    fn actual_picker_rejects_occupied_click_then_creates_at_preview_in_all_layouts() {
        for layout in ["Free", "GridVania", "LinearHorizontal", "LinearVertical"] {
            let mut a = editor();
            a.project.as_mut().unwrap().json["worldLayout"] = json!(layout);
            a.viewport.center = [128., 128.];
            let ctx = egui::Context::default();
            a.start_level_spot_picker();
            if matches!(layout, "Free" | "GridVania") {
                click(&mut a, &ctx, egui::pos2(400., 300.));
                assert!(a.world_context.picking);
                assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
            }
            let pos = egui::pos2(720., 560.);
            let world = a.context_pointer(
                Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.)),
                pos,
            );
            let spot = a.level_insert_spot(world).unwrap();
            assert!(!spot.overlaps);
            click(&mut a, &ctx, pos);
            assert!(!a.world_context.picking, "{layout}");
            assert_eq!(
                a.project.as_ref().unwrap().level_keys().len(),
                2,
                "{layout}"
            );
            assert!(a.dirty);
            let level = a
                .project
                .as_ref()
                .unwrap()
                .level(a.selected.unwrap())
                .unwrap();
            assert_eq!(level["useAutoIdentifier"], true);
        }
    }
    #[test]
    fn context_menu_requires_unmodified_world_or_empty_space_and_backup_stays_readonly() {
        let mut a = editor();
        a.world = false;
        a.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        let inside = egui::pos2(400., 300.);
        right_click(&mut a, &ctx, inside);
        assert!(a.world_context.menu.is_none());
        let outside = egui::pos2(760., 560.);
        let output = right_click(&mut a, &ctx, outside);
        label_center(&output, "New level");
        a.cancel_world_context();
        let modifiers = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        frame(
            &mut a,
            &ctx,
            vec![button(
                outside,
                egui::PointerButton::Secondary,
                true,
                modifiers,
            )],
            modifiers,
        );
        assert!(a.world_context.menu.is_none());
        a.project.as_mut().unwrap().path = Some(std::path::PathBuf::from(
            "/private/tmp/backup_2026-11-16_12-00-00/world.ldtk",
        ));
        assert!(a.project.as_ref().unwrap().is_backup());
        a.world = true;
        right_click(&mut a, &ctx, inside);
        assert!(a.world_context.menu.is_none());
        a.start_level_spot_picker();
        assert!(!a.world_context.picking);
    }
    #[test]
    fn world_menu_moves_level_with_iid_intact_and_goes_to_empty_world() {
        let mut a = editor();
        let p = a.project.as_mut().unwrap();
        p.add_world("Other").unwrap();
        a.active_world = Some(0);
        a.selected = Some(LevelKey {
            world: Some(0),
            level: 0,
        });
        a.viewport.center = [128., 128.];
        let iid = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap()["iid"]
            .clone();
        let ctx = egui::Context::default();
        let output = right_click(&mut a, &ctx, egui::pos2(400., 300.));
        let target = label_center(&output, "Other (0 levels)");
        click(&mut a, &ctx, target);
        assert_eq!(a.active_world, Some(1));
        assert_eq!(
            a.project
                .as_ref()
                .unwrap()
                .level(a.selected.unwrap())
                .unwrap()["iid"],
            iid
        );
        a.fit = false;
        a.viewport.center = [128., 128.];
        let output = right_click(&mut a, &ctx, egui::pos2(760., 560.));
        let target = label_center(&output, "World_0 (0 levels)");
        click(&mut a, &ctx, target);
        assert_eq!(a.active_world, Some(0));
        assert!(a.selected.is_none());
        assert!(a.world);
        a.start_level_spot_picker();
        assert!(a.create_level_at([0., 0.], true));
        assert_eq!(a.selected.unwrap().world, Some(0));
    }
    #[test]
    fn actual_empty_space_new_level_and_picker_cancel_preserve_source_mode() {
        let mut a = editor();
        a.world = false;
        a.viewport.center = [128., 128.];
        a.project
            .as_mut()
            .unwrap()
            .move_level(K, 512, 256, 0)
            .unwrap();
        let ctx = egui::Context::default();
        let point = egui::pos2(760., 560.);
        let position = a.context_pointer(
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.)),
            point,
        );
        let expected = a.level_insert_spot(position).unwrap().rect;
        let output = right_click(&mut a, &ctx, point);
        click(&mut a, &ctx, label_center(&output, "New level"));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
        assert!(!a.world);
        let level = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap();
        assert_eq!(level["worldX"], expected[0] as i32);
        assert_eq!(level["worldY"], expected[1] as i32);
        a.start_level_spot_picker();
        assert!(a.world);
        let output = frame(&mut a, &ctx, vec![], egui::Modifiers::NONE);
        // Area does one sizing pass before its interactive position stabilizes.
        let _ = output;
        let output = frame(&mut a, &ctx, vec![], egui::Modifiers::NONE);
        click(&mut a, &ctx, label_center(&output, "Cancel"));
        assert!(!a.world_context.picking);
        assert!(a.world);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    #[test]
    fn moving_from_linear_to_free_preserves_effective_world_position() {
        let mut a = editor();
        let p = a.project.as_mut().unwrap();
        p.add_level(None, "Second", 128, 128).unwrap();
        p.set_world_layout(None, "LinearHorizontal", 256, 256)
            .unwrap();
        p.add_world("Other").unwrap();
        a.active_world = Some(0);
        a.selected = Some(LevelKey {
            world: Some(0),
            level: 1,
        });
        a.move_context_level(a.selected.unwrap(), 1);
        let level = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap();
        assert_eq!(level["worldX"], 304);
        assert_eq!(level["worldY"], 0);
    }
}
