use super::*;

// Camera.hx:260: canvas and level coordinates are both expressed in logical
// pixels here, so the browser's device-pixel ratio cancels out.
fn level_min_zoom(level: &Value, size: Vec2) -> f32 {
    (size.x / (number(level, "pxWid", 256.).max(1.) * 2.3))
        .min(size.y / (number(level, "pxHei", 256.).max(1.) * 2.3))
}

fn contains(bounds: [f32; 4], point: [f32; 2], padding: f32) -> bool {
    point[0] >= bounds[0] - padding
        && point[0] < bounds[2] + padding
        && point[1] >= bounds[1] - padding
        && point[1] < bounds[3] + padding
}

fn level_distance(bounds: [f32; 4], point: [f32; 2]) -> f32 {
    if contains(bounds, point, 0.) {
        return 0.;
    }
    if point[1] >= bounds[1] && point[1] < bounds[3] {
        return (point[0] - bounds[0])
            .abs()
            .min((point[0] - bounds[2]).abs());
    }
    if point[0] >= bounds[0] && point[0] < bounds[2] {
        return (point[1] - bounds[1])
            .abs()
            .min((point[1] - bounds[3]).abs());
    }
    [bounds[0], bounds[2] - 1.]
        .into_iter()
        .flat_map(|x| {
            [bounds[1], bounds[3] - 1.]
                .into_iter()
                .map(move |y| (point[0] - x).hypot(point[1] - y))
        })
        .fold(f32::INFINITY, f32::min)
}

impl EditorApp {
    pub(super) fn wheel_zoom(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        pointer: egui::Pos2,
        scroll: f32,
    ) {
        if self.defer_ui_action(
            ctx,
            ui_commit::DeferredUiAction::Wheel {
                rect,
                pointer,
                scroll,
            },
        ) {
            return;
        }
        let before = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        let minimum = if self.world {
            0.03
        } else {
            self.project
                .as_ref()
                .and_then(|p| self.selected.and_then(|key| p.level(key)))
                .map(|level| level_min_zoom(level, rect.size()))
                .unwrap_or(0.03)
        };
        // App.onAppMouseWheel normalizes DOM deltaY; Editor.deltaZoom applies
        // 0.15 * zoom. egui's vertical scroll has the opposite DOM sign.
        let normalization = if ctx.input(|i| i.modifiers.ctrl) {
            0.03
        } else {
            0.0015
        };
        self.viewport.zoom = (self.viewport.zoom
            * (1. + scroll * normalization * self.settings.float("mouseWheelSpeed", 1.0)))
        .max(minimum)
        .min(32.);
        let after = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        for axis in 0..2 {
            self.viewport.center[axis] += before[axis] - after[axis];
        }
        let preference = self.settings.value["autoWorldModeSwitch"]
            .as_str()
            .unwrap_or("ZoomInAndOut");
        let Some(project) = &self.project else { return };
        let Some(key) = self.selected else { return };
        let placements = canvas::placements(project, key);
        let target = if !self.world
            && scroll < 0.
            && preference != "Never"
            && self.viewport.zoom <= minimum
        {
            placements
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, bounds)| (key, *bounds, true))
        } else if self.world && scroll > 0. && preference == "ZoomInAndOut" {
            placements
                .iter()
                .filter(|(candidate, bounds)| {
                    project.level(*candidate).is_some_and(|level| {
                        number(level, "worldDepth", 0.) as i64 == self.world_depth
                    }) && contains(*bounds, before, 500.)
                })
                .min_by(|a, b| level_distance(a.1, before).total_cmp(&level_distance(b.1, before)))
                .filter(|(candidate, _)| {
                    project.level(*candidate).is_some_and(|level| {
                        self.viewport.zoom > level_min_zoom(level, rect.size())
                    })
                })
                .map(|(candidate, bounds)| (*candidate, *bounds, false))
        } else {
            None
        };
        if let Some((key, bounds, world)) = target {
            self.finish_gesture();
            self.finish_stroke();
            self.navigate_level(key, false);
            self.world = world;
            let direction = if world { 1. } else { -1. };
            self.viewport.center[0] += direction * bounds[0];
            self.viewport.center[1] += direction * bounds[1];
            self.selection = None;
            self.selection_group = None;
            self.entity_index = None;
            self.stamp = None;
            self.panel = None;
            self.fit = false;
            self.normalize_palette();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn editor() -> EditorApp {
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.move_level(key, 1000, 2000, 0).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.fit = false;
        app.viewport.center = [128., 128.];
        app
    }
    fn canvas() -> Rect {
        Rect::from_min_size(egui::pos2(300., 40.), egui::vec2(800., 600.))
    }
    fn close(a: [f32; 2], b: [f32; 2]) {
        for i in 0..2 {
            assert!((a[i] - b[i]).abs() < 0.001, "{a:?} != {b:?}");
        }
    }
    #[test]
    fn zoom_out_enters_world_at_source_minimum_and_keeps_cursor_anchor() {
        let mut app = editor();
        let rect = canvas();
        let pointer = rect.center() + egui::vec2(30., -20.);
        let before = app
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        app.wheel_zoom(&egui::Context::default(), rect, pointer, -100.);
        assert!(app.world);
        assert!((app.viewport.zoom - 600. / (256. * 2.3)).abs() < 0.0001);
        close(
            app.viewport
                .to_world(pointer.into(), rect.min.into(), rect.size().into()),
            [before[0] + 1000., before[1] + 2000.],
        );
        assert!(!app.fit);
    }
    #[test]
    fn never_clamps_at_level_minimum_without_switching_and_zoomoutonly_stays_in_world() {
        let mut app = editor();
        let rect = canvas();
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.wheel_zoom(&egui::Context::default(), rect, rect.center(), -10000.);
        assert!(!app.world);
        assert!((app.viewport.zoom - 600. / (256. * 2.3)).abs() < 0.0001);
        app.settings.value["autoWorldModeSwitch"] = json!("ZoomOutOnly");
        app.wheel_zoom(&egui::Context::default(), rect, rect.center(), -10.);
        assert!(app.world);
        app.wheel_zoom(&egui::Context::default(), rect, rect.center(), 1000.);
        assert!(app.world);
    }
    #[test]
    fn zoom_in_selects_nearest_same_depth_level_and_keeps_global_anchor() {
        let mut app = editor();
        let rect = canvas();
        let first = app.selected.unwrap();
        let other = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Other", 256, 256)
            .unwrap();
        let above = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Above", 256, 256)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(other, 1400, 2000, 0)
            .unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(above, 1400, 2000, 1)
            .unwrap();
        app.world = true;
        app.viewport.center = [1528., 2128.];
        app.viewport.zoom = 1.;
        app.wheel_zoom(&egui::Context::default(), rect, rect.center(), 100.);
        assert!(!app.world);
        assert_eq!(app.selected, Some(other));
        assert_ne!(app.selected, Some(first));
        close(app.viewport.center, [128., 128.]);
        app.world = true;
        app.viewport.center = [3000., 2000.];
        app.viewport.zoom = 1.;
        app.wheel_zoom(&egui::Context::default(), rect, rect.center(), 100.);
        assert!(app.world);
    }
    #[test]
    fn native_mouse_wheel_event_reaches_world_transition() {
        let mut app = editor();
        let ctx = egui::Context::default();
        for scroll in [None, Some(-1000.)] {
            let mut events = vec![egui::Event::PointerMoved(egui::pos2(400., 300.))];
            if let Some(delta) = scroll {
                events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., delta),
                    modifiers: egui::Modifiers::NONE,
                });
            }
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
                    });
                },
            );
        }
        assert!(app.world);
        assert!(!app.fit);
    }
    #[test]
    fn wheel_speed_and_control_scaling_follow_source_additive_zoom() {
        let mut app = editor();
        app.world = true;
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.settings.value["mouseWheelSpeed"] = json!(2.);
        app.viewport.zoom = 2.;
        let rect = canvas();
        let ctx = egui::Context::default();
        app.wheel_zoom(&ctx, rect, rect.center(), 100.);
        assert!((app.viewport.zoom - 2.6).abs() < 0.0001);
        let _ = ctx.run(
            egui::RawInput {
                modifiers: egui::Modifiers::CTRL,
                ..Default::default()
            },
            |ctx| {
                app.viewport.zoom = 2.;
                app.wheel_zoom(ctx, rect, rect.center(), 1.);
            },
        );
        assert!((app.viewport.zoom - 2.12).abs() < 0.0001);
    }
}
