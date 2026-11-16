use super::*;

pub(super) struct LevelResize {
    key: LevelKey,
    start: [f32; 2],
    end: [f32; 2],
    screen_start: egui::Pos2,
    handle: [i32; 2],
    size: [i32; 2],
    world_origin: [i32; 2],
    depth: i32,
    grid: i32,
    world_grid: Option<[i32; 2]>,
    moved: bool,
}
const HANDLES: [[i32; 2]; 8] = [
    [0, -1],
    [0, 1],
    [-1, 0],
    [1, 0],
    [-1, -1],
    [1, -1],
    [-1, 1],
    [1, 1],
];
fn handle_position(size: [i32; 2], handle: [i32; 2]) -> [f32; 2] {
    std::array::from_fn(|axis| match handle[axis] {
        -1 => -4.,
        1 => size[axis] as f32 + 4.,
        _ => (size[axis] / 2) as f32,
    })
}
fn cursor(handle: [i32; 2]) -> egui::CursorIcon {
    match handle {
        [0, _] => egui::CursorIcon::ResizeVertical,
        [_, 0] => egui::CursorIcon::ResizeHorizontal,
        [-1, -1] | [1, 1] => egui::CursorIcon::ResizeNwSe,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}
impl LevelResize {
    fn bounds(&self) -> [i32; 4] {
        let mut bounds = [0, 0, self.size[0], self.size[1]];
        for axis in 0..2 {
            let delta =
                ((self.end[axis] - self.start[axis]) / self.grid as f32).floor() as i32 * self.grid;
            if self.handle[axis] < 0 {
                bounds[axis] = delta;
            } else if self.handle[axis] > 0 {
                bounds[axis + 2] += delta;
            }
            if let Some(grid) = self.world_grid {
                for index in [axis, axis + 2] {
                    bounds[index] = ((bounds[index] as f32 / grid[axis] as f32 + 0.5).floor()
                        as i32)
                        * grid[axis];
                }
            }
        }
        bounds
    }
    fn valid(&self, bounds: [i32; 4]) -> bool {
        bounds[2] - bounds[0] >= self.grid && bounds[3] - bounds[1] >= self.grid
    }
}
impl EditorApp {
    pub(super) fn level_resize_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        if self.project.as_ref().is_some_and(Project::is_backup) {
            return false;
        }
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let Some(pointer) = pointer else {
            return self.level_resize.is_some();
        };
        if let Some(drag) = &mut self.level_resize {
            let world = self
                .viewport
                .to_world(pointer.into(), rect.min.into(), rect.size().into());
            drag.end = std::array::from_fn(|axis| {
                (world[axis] + 0.5).floor()
                    - if self.world {
                        drag.world_origin[axis] as f32
                    } else {
                        0.
                    }
            });
            drag.moved |= pointer.distance(drag.screen_start) >= 4.;
            ctx.set_cursor_icon(if drag.moved {
                egui::CursorIcon::Grabbing
            } else {
                cursor(drag.handle)
            });
            return true;
        }
        if self.gesture.is_some()
            || self.stroke
            || self.field_pick.is_some()
            || !response.hovered()
            || ctx.input(|i| i.modifiers.any() || i.key_down(Key::Space))
        {
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        let Some(key) = self.selected else {
            return false;
        };
        let Some(level) = project.level(key) else {
            return false;
        };
        let depth = number(level, "worldDepth", 0.) as i32;
        if i64::from(depth) != self.world_depth {
            return false;
        }
        let size = [
            number(level, "pxWid", 256.) as i32,
            number(level, "pxHei", 256.) as i32,
        ];
        let world_origin = canvas::placements(project, key)
            .into_iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, bounds)| [bounds[0] as i32, bounds[1] as i32])
            .unwrap_or([0, 0]);
        let world = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        let local = std::array::from_fn(|axis| {
            (world[axis] + 0.5).floor()
                - if self.world {
                    world_origin[axis] as f32
                } else {
                    0.
                }
        });
        if !self.world
            && local[0] >= 0.
            && local[1] >= 0.
            && local[0] < size[0] as f32
            && local[1] < size[1] as f32
        {
            return false;
        }
        // Original loop deliberately uses the last matching handle.
        let handle = HANDLES.into_iter().rfind(|handle| {
            let point = handle_position(size, *handle);
            (local[0] - point[0]).hypot(local[1] - point[1]) <= 15.
        });
        let Some(handle) = handle else { return false };
        ctx.set_cursor_icon(cursor(handle));
        if ctx.input(|i| i.pointer.primary_pressed()) {
            let grid = project
                .layer(key, self.layer)
                .map(|layer| number(layer, "__gridSize", 16.))
                .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.))
                .max(1.) as i32;
            let container = key
                .world
                .and_then(|world| project.json["worlds"].get(world))
                .unwrap_or(&project.json);
            let world_grid = (container["worldLayout"] == "GridVania").then(|| {
                [
                    number(container, "worldGridWidth", 256.).max(1.) as i32,
                    number(container, "worldGridHeight", 256.).max(1.) as i32,
                ]
            });
            self.level_resize = Some(LevelResize {
                key,
                start: local,
                end: local,
                screen_start: pointer,
                handle,
                size,
                world_origin,
                depth,
                grid,
                world_grid,
                moved: false,
            });
        }
        true
    }

    pub(super) fn finish_level_resize(&mut self) {
        let Some(drag) = self.level_resize.take() else {
            return;
        };
        let bounds = drag.bounds();
        if !drag.moved || !drag.valid(bounds) || bounds == [0, 0, drag.size[0], drag.size[1]] {
            return;
        }
        let [left, top, right, bottom] = bounds;
        if self
            .edit_level("Level resized", drag.key, |project| {
                project.move_level(
                    drag.key,
                    drag.world_origin[0] + left,
                    drag.world_origin[1] + top,
                    drag.depth,
                )?;
                project.resize_level(drag.key, right - left, bottom - top, -left, -top)
            })
            .is_some()
        {
            self.selection = None;
            self.selection_group = None;
            self.entity_index = None;
            if !self.world {
                self.viewport.center[0] -= left as f32;
                self.viewport.center[1] -= top as f32;
            }
        }
    }
    pub(super) fn cancel_level_resize(&mut self) {
        self.level_resize = None;
    }

    pub(super) fn draw_level_resize_preview(&self, ui: &egui::Ui, rect: Rect) {
        let Some(project) = &self.project else { return };
        let Some(key) = self.selected else { return };
        let Some(level) = project.level(key) else {
            return;
        };
        let origin = if self.world {
            canvas::placements(project, key)
                .into_iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, bounds)| [bounds[0], bounds[1]])
                .unwrap_or([0., 0.])
        } else {
            [0., 0.]
        };
        let point = |local: [f32; 2]| -> egui::Pos2 {
            self.viewport
                .to_screen(
                    [origin[0] + local[0], origin[1] + local[1]],
                    rect.min.into(),
                    rect.size().into(),
                )
                .into()
        };
        let painter = ui.painter().with_clip_rect(rect);
        if (self.world || !self.details)
            && number(level, "worldDepth", 0.) as i64 == self.world_depth
            && !ui.input(|i| i.modifiers.any() || i.key_down(Key::Space))
        {
            let size = [
                number(level, "pxWid", 256.) as i32,
                number(level, "pxHei", 256.) as i32,
            ];
            for handle in HANDLES {
                painter.circle_filled(
                    point(handle_position(size, handle)),
                    5. * self.viewport.zoom,
                    Color32::WHITE,
                );
            }
        }
        if let Some(drag) = &self.level_resize
            && drag.moved
        {
            let bounds = drag.bounds();
            painter.rect_stroke(
                Rect::from_two_pos(
                    point([bounds[0] as f32, bounds[1] as f32]),
                    point([bounds[2] as f32, bounds[3] as f32]),
                ),
                0.,
                Stroke::new(
                    4. * self.viewport.zoom,
                    if drag.valid(bounds) {
                        Color32::from_rgb(255, 204, 0)
                    } else {
                        Color32::RED
                    },
                ),
                StrokeKind::Middle,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> EditorApp {
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        app.set_project(project);
        app.fit = false;
        app.viewport.center = [128., 128.];
        app.viewport.zoom = 1.;
        app
    }
    fn resize(app: &EditorApp, handle: [i32; 2], end: [f32; 2]) -> LevelResize {
        LevelResize {
            key: app.selected.unwrap(),
            start: [0., 0.],
            end,
            screen_start: egui::Pos2::ZERO,
            handle,
            size: [256, 256],
            world_origin: [100, 200],
            depth: 0,
            grid: 16,
            world_grid: None,
            moved: true,
        }
    }
    #[test]
    fn ruler_edges_floor_grid_delta_and_gridvania_rounds_bounds() {
        let app = editor();
        let mut drag = resize(&app, [-1, -1], [-1., 17.]);
        assert_eq!(drag.bounds(), [-16, 16, 256, 256]);
        drag.world_grid = Some([256, 128]);
        drag.end = [-140., 70.];
        assert_eq!(drag.bounds(), [-256, 128, 256, 256]);
    }
    #[test]
    fn left_top_resize_moves_world_origin_translates_contents_and_undo_restores_both() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        p.move_level(key, 100, 200, 0).unwrap();
        p.paint_int_grid(key, 0, 0, 0, 1).unwrap();
        app.level_resize = Some(resize(&app, [-1, -1], [-16., -32.]));
        app.finish_level_resize();
        let p = app.project.as_ref().unwrap();
        let level = p.level(key).unwrap();
        assert_eq!(
            (level["worldX"].as_i64(), level["worldY"].as_i64()),
            (Some(84), Some(168))
        );
        assert_eq!(level["pxWid"], 272);
        assert_eq!(level["pxHei"], 288);
        assert_eq!(p.layer(key, 0).unwrap()["intGridCsv"][35], 1);
        assert_eq!(app.viewport.center, [144., 160.]);
        assert!(app.history.undo_level(app.project.as_mut().unwrap(), key));
        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["pxWid"], 256);
        assert_eq!(level["worldX"], 100);
        assert_eq!(level["layerInstances"][0]["intGridCsv"][0], 1);
    }
    #[test]
    fn invalid_bounds_click_without_drag_and_cancel_never_mutate_document() {
        let mut app = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        app.level_resize = Some(resize(&app, [-1, 0], [256., 0.]));
        app.finish_level_resize();
        assert_eq!(app.project.as_ref().unwrap().json, before);
        let mut drag = resize(&app, [1, 1], [32., 32.]);
        drag.moved = false;
        app.level_resize = Some(drag);
        app.finish_level_resize();
        assert_eq!(app.project.as_ref().unwrap().json, before);
        app.level_resize = Some(resize(&app, [1, 1], [32., 32.]));
        app.cancel_level_resize();
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.history.can_undo());
    }
    #[test]
    fn native_handle_drag_previews_without_mutation_then_commits_release_position() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let ctx = egui::Context::default();
        let start = egui::pos2(532., 432.);
        let end = start + egui::vec2(32., 16.);
        for (index, events) in [
            vec![egui::Event::PointerMoved(start)],
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerMoved(start + egui::vec2(16., 0.))],
            vec![
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                        app.draw_level_resize_preview(ui, rect);
                    });
                },
            );
            if index == 2 {
                assert!(app.level_resize.as_ref().is_some_and(|drag| drag.moved));
                assert_eq!(
                    app.project.as_ref().unwrap().level(key).unwrap()["pxWid"],
                    256
                );
            }
        }
        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["pxWid"], 288);
        assert_eq!(level["pxHei"], 272);
        assert!(app.level_resize.is_none());
        assert!(
            app.history
                .can_undo_level(app.project.as_ref().unwrap(), key)
        );
    }

    #[test]
    fn world_resize_preserves_camera_and_requires_at_least_one_grid_cell() {
        let mut app = editor();
        app.world = true;
        let key = app.selected.unwrap();
        app.level_resize = Some(resize(&app, [1, 1], [-240., -240.]));
        app.finish_level_resize();
        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["pxWid"], 16);
        assert_eq!(level["pxHei"], 16);
        assert_eq!(app.viewport.center, [128., 128.]);
    }
}
