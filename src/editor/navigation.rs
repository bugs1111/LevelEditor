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

pub(super) fn level_world_origin(project: &Project, key: LevelKey) -> [f32; 2] {
    let world = key
        .world
        .and_then(|index| project.json["worlds"].get(index))
        .unwrap_or(&project.json);
    if matches!(
        world["worldLayout"].as_str(),
        Some("LinearHorizontal" | "LinearVertical")
    ) {
        canvas::placements(project, key)
            .into_iter()
            .find(|(candidate, _)| *candidate == key)
            .map_or([0.; 2], |(_, [x, y, ..])| [x, y])
    } else {
        let level = project.level(key).unwrap_or(&Value::Null);
        [number(level, "worldX", 0.), number(level, "worldY", 0.)]
    }
}

impl EditorApp {
    pub(super) fn canvas_pointer_pixels(
        &self,
        project: &Project,
        key: LevelKey,
        rect: Rect,
        pointer: egui::Pos2,
        pixels_per_point: f32,
    ) -> ([f32; 2], [f32; 2]) {
        let offset = if self.world {
            [0.; 2]
        } else {
            level_world_origin(project, key)
        };
        let world_pixel = self.viewport.pointer_pixel(
            pointer.into(),
            rect.min.into(),
            rect.size().into(),
            offset,
            pixels_per_point,
        );
        ([world_pixel[0] - offset[0], world_pixel[1] - offset[1]], world_pixel)
    }

    pub(super) fn wheel_zoom(&mut self, ctx: &egui::Context, rect: Rect, pointer: egui::Pos2) {
        // winit reports wheel ticks separately from smooth pixel scrolling.
        // Chromium delivers a discrete tick as 120 pixels on Linux and Windows
        // and converts Cocoa wheel data at 40 pixels per unit.
        let pixels_per_tick = if cfg!(target_os = "macos") { 40. } else { 120. };
        let speed = self.settings.float("mouseWheelSpeed", 1.0);
        let deltas: Vec<_> = ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| {
                    let egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                    } = event
                    else {
                        return None;
                    };
                    let pixels = match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * pixels_per_tick,
                        egui::MouseWheelUnit::Page => delta.y * rect.height(),
                    };
                    let normalization = if modifiers.ctrl { 0.20 } else { 0.01 };
                    (pixels != 0.).then_some(pixels * normalization * speed * 0.15)
                })
                .collect()
        });
        if !deltas.is_empty() {
            self.zoom_from_input(ctx, rect, pointer, deltas);
        }
    }

    pub(super) fn zoom_from_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        pointer: egui::Pos2,
        deltas: Vec<f32>,
    ) {
        if self.defer_ui_action(
            ctx,
            ui_commit::DeferredUiAction::Zoom {
                rect,
                pointer,
                deltas: deltas.clone(),
            },
        ) {
            return;
        }
        for delta in deltas {
            self.zoom_at_canvas(rect, pointer, delta, ctx.pixels_per_point());
        }
    }

    pub(super) fn zoom_at_canvas(
        &mut self,
        rect: Rect,
        pointer: egui::Pos2,
        delta: f32,
        pixels_per_point: f32,
    ) {
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let absolute_minimum = 0.03 / pixels_per_point;
        let level_minimum = if self.world {
            absolute_minimum
        } else {
            project
                .level(key)
                .map(|level| level_min_zoom(level, rect.size()))
                .unwrap_or(absolute_minimum)
        };
        let minimum = if self.camera_motion.is_active() {
            absolute_minimum
        } else {
            level_minimum
        };
        let zoom = (self.viewport.zoom * (1. + delta))
            .max(minimum)
            .min(32. / pixels_per_point);
        let center_before_zoom = self.viewport.center;
        let (focus, _) = self.canvas_pointer_pixels(project, key, rect, pointer, pixels_per_point);
        let level_origin = if self.world {
            level_world_origin(project, key)
        } else {
            [0.; 2]
        };
        self.viewport
            .zoom_at_level_point(zoom, rect, level_origin, focus, pixels_per_point);
        self.camera_motion.cancel();
        let preference = self.settings.value["autoWorldModeSwitch"]
            .as_str()
            .unwrap_or("ZoomInAndOut");
        let placements = canvas::placements(project, key);
        let target = if !self.world
            && delta < 0.
            && preference != "Never"
            && self.viewport.zoom <= level_minimum
        {
            placements
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, bounds)| (key, *bounds, true))
        } else if self.world && delta > 0. && preference == "ZoomInAndOut" {
            // The source publishes the new zoom to the world renderer before
            // applying the cursor's center adjustment. Coords uses that
            // pixel-snapped render origin when it scores the levels.
            let point = Viewport {
                center: center_before_zoom,
                zoom,
            }
            .pointer_pixel(
                pointer.into(),
                rect.min.into(),
                rect.size().into(),
                [0.; 2],
                pixels_per_point,
            );
            let score = |bounds| {
                let contained_bonus = if contains(bounds, point, 0.) {
                    100.
                } else {
                    0.
                };
                contained_bonus - level_distance(bounds, point)
            };
            placements
                .iter()
                .filter(|(candidate, bounds)| {
                    project.level(*candidate).is_some_and(|level| {
                        number(level, "worldDepth", 0.) as i64 == self.world_depth
                    }) && contains(*bounds, point, 500.)
                })
                .min_by(|a, b| score(b.1).total_cmp(&score(a.1)))
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
            self.panel = world.then_some(Panel::World);
            if world {
                self.notify_world_view();
            }
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

    fn stays_near_pointer(actual: [f32; 2], expected: [f32; 2], zoom: f32) {
        for axis in 0..2 {
            let shift = (actual[axis] - expected[axis]).abs() * zoom;
            assert!(
                shift <= 2.,
                "zoom anchor shifted from {expected:?} to {actual:?}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn startup_fits_the_level_after_the_requested_window_state_arrives() {
        let mut app = editor();
        app.fit = true;
        app.startup_window = Some(StartupWindow::NotYetMapped);
        app.settings.value["startFullScreen"] = json!(true);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut draw = |size, focused, maximized, fullscreen| {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.focused = Some(focused);
            viewport.maximized = Some(maximized);
            viewport.fullscreen = Some(fullscreen);
            let output = ctx.run(input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest());
            });
            (app.fit, output.viewport_output[&egui::ViewportId::ROOT].commands.clone())
        };
        let normal = egui::vec2(800., 600.);
        let full = egui::vec2(1440., 900.);
        assert!(draw(normal, false, false, false).0);
        let (fit_pending, commands) = draw(normal, true, false, false);
        assert!(fit_pending);
        assert!(commands.iter().any(|cmd| matches!(cmd, egui::ViewportCommand::Maximized(true))));
        let (fit_pending, commands) = draw(egui::vec2(1440., 876.), true, true, false);
        assert!(fit_pending);
        assert!(commands.iter().any(|cmd| matches!(cmd, egui::ViewportCommand::Fullscreen(true))));
        assert!(!draw(full, true, true, true).0);
        assert!(!draw(normal, true, false, false).1.iter().any(|cmd| {
            matches!(cmd, egui::ViewportCommand::Maximized(_) | egui::ViewportCommand::Fullscreen(_))
        }));
        assert!((app.viewport.zoom - 900. / (256. + 80.)).abs() < 0.001);
    }

    #[test]
    fn initial_fit_at_custom_ui_scale_uses_the_first_native_window_size() {
        let mut app = editor();
        app.fit = true;
        app.settings.value["appUiScale"] = json!(1.5);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.apply_initial_preferences(&ctx);
        let logical_size = egui::vec2(1440., 900.) / ctx.zoom_factor();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, logical_size)),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest());
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
        assert!(!app.fit);
        let expected = 600. / (256. + 80. * 1.5);
        assert!(
            (app.viewport.zoom - expected).abs() < 0.001,
            "first-fit zoom {} != {expected}",
            app.viewport.zoom
        );
    }

    #[test]
    fn initial_fit_converts_source_device_scaled_margins_to_the_canvas_units() {
        for density in [1., 2.] {
            for world in [false, true] {
                let mut app = editor();
                app.fit = true;
                app.world = world;
                let ctx = egui::Context::default();
                let mut input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500., 600.),
                    )),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(density);
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) = ui
                                .allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                        });
                });
                let expected = if world {
                    500. / (256. * 1.1 + 300. * density)
                } else {
                    500. / (256. + 80. * density)
                };
                assert!(
                    (app.viewport.zoom - expected).abs() < 0.001,
                    "DPR {density}, world {world}: {} != {expected}",
                    app.viewport.zoom
                );
                close(
                    app.viewport.center,
                    if world { [1128., 2128.] } else { [128., 128.] },
                );
            }
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
        app.zoom_at_canvas(rect, pointer, -0.15, 1.);
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
        assert!((app.viewport.zoom - 600. / (256. * 2.3)).abs() < 0.0001);
        stays_near_pointer(
            app.viewport
                .to_world(pointer.into(), rect.min.into(), rect.size().into()),
            [before[0] + 1000., before[1] + 2000.],
            app.viewport.zoom,
        );
        assert!(!app.fit);
    }

    #[test]
    fn wheel_zoom_uses_the_original_snapped_focus() {
        let mut app = editor();
        let key = app.selected.unwrap();
        app.project
            .as_mut()
            .unwrap()
            .move_level(key, 0, 0, 0)
            .unwrap();
        let rect = Rect::from_min_size(egui::pos2(300., 0.), egui::vec2(500., 600.));
        app.viewport
            .fit([0., 0., 256., 256.], rect.size().into(), 1.);
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., 120.),
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| app.wheel_zoom(ctx, rect, egui::pos2(452., 315.)),
        );
        let origin = app.viewport.level_render_canvas_origin(rect, 1.);
        let level_top_left = app.viewport.to_screen([0.; 2], origin, rect.size().into());
        close(level_top_left, [344., 73.]);
    }

    #[test]
    fn never_clamps_at_level_minimum_without_switching_and_zoomoutonly_stays_in_world() {
        let mut app = editor();
        let rect = canvas();
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.zoom_at_canvas(rect, rect.center(), -15., 1.);
        assert!(!app.world);
        assert!((app.viewport.zoom - 600. / (256. * 2.3)).abs() < 0.0001);
        app.settings.value["autoWorldModeSwitch"] = json!("ZoomOutOnly");
        app.zoom_at_canvas(rect, rect.center(), -0.015, 1.);
        assert!(app.world);
        app.zoom_at_canvas(rect, rect.center(), 1.5, 1.);
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
        app.zoom_at_canvas(rect, rect.center(), 0.15, 1.);
        assert!(!app.world);
        assert_eq!(app.selected, Some(other));
        assert_ne!(app.selected, Some(first));
        close(app.viewport.center, [128., 128.]);
        app.world = true;
        app.viewport.center = [3000., 2000.];
        app.viewport.zoom = 1.;
        app.zoom_at_canvas(rect, rect.center(), 0.15, 1.);
        assert!(app.world);
    }
    #[test]
    fn zoom_in_prefers_the_level_owning_the_rounded_shared_edge() {
        for x in [1256., 1255.75] {
            let mut app = editor();
            let right = app
                .project
                .as_mut()
                .unwrap()
                .add_level(None, "Right", 256, 256)
                .unwrap();
            app.project
                .as_mut()
                .unwrap()
                .move_level(right, 1256, 2000, 0)
                .unwrap();
            app.world = true;
            app.viewport.center = [x, 2128.];
            app.viewport.zoom = 1.;
            let rect = canvas();
            app.zoom_at_canvas(rect, rect.center(), 0.15, 1.);
            assert!(!app.world);
            assert_eq!(app.selected, Some(right), "world point x={x}");
            close(app.viewport.center, [x - 1256., 128.]);
        }
    }

    #[test]
    fn zoom_in_rounds_the_world_pointer_before_the_source_padding_test() {
        let mut app = editor();
        app.world = true;
        app.viewport.center = [499.75, 2128.];
        app.viewport.zoom = 1.;
        let rect = canvas();
        app.zoom_at_canvas(rect, rect.center(), 0.15, 1.);
        assert!(!app.world);
        close(app.viewport.center, [-500.25, 128.]);
    }

    #[test]
    fn zoom_in_scores_with_the_new_zoom_before_publishing_the_cursor_pan() {
        let mut app = editor();
        let left = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let right = project.add_level(None, "Right", 256, 256).unwrap();
        project.move_level(right, 1256, 2000, 0).unwrap();
        app.world = true;
        app.viewport.center = [1128., 2128.];
        app.viewport.zoom = 1.06;
        let rect = canvas();
        let pointer = rect.center() + egui::vec2(180., 0.);
        let anchor = app
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        assert!(anchor[0] > 1256.);

        app.zoom_at_canvas(rect, pointer, 3.6, 1.);
        assert!(!app.world);
        assert_eq!(app.selected, Some(left));
        // Reprojecting level pixel [298, 128] moves its backing position by
        // [650, 1] before the camera compensates and switches to level coordinates.
        let zoom = app.viewport.zoom;
        close(app.viewport.center, [128. + 650. / zoom, 128. + 1. / zoom]);
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
    fn native_wheel_preserves_detents_event_modifiers_and_order() {
        let mut app = editor();
        app.world = true;
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.viewport.zoom = 2.;
        let ctx = egui::Context::default();
        let pointer = egui::pos2(400., 300.);
        for events in [
            vec![egui::Event::PointerMoved(pointer)],
            vec![
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0., 1.),
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., 2.),
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
        ] {
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
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) = ui
                                .allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                        });
                },
            );
        }
        let browser_pixels_per_tick = if cfg!(target_os = "macos") { 40. } else { 120. };
        let expected = 2. * (1. + browser_pixels_per_tick * 0.0015) * 1.06;
        assert!(
            (app.viewport.zoom - expected).abs() < 0.0001,
            "batched zoom {} != {expected}",
            app.viewport.zoom
        );
    }

    #[test]
    fn wheel_bounds_use_source_backing_pixels_at_high_density() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(2.);
        let _ = ctx.run(input, |ctx| {
            let rect = canvas();
            for world in [true, false] {
                let mut app = editor();
                app.world = world;
                app.settings.value["autoWorldModeSwitch"] = json!("Never");
                app.viewport.zoom = 8.;
                let pointer = rect.center() + egui::vec2(50., -50.);
                app.zoom_at_canvas(rect, pointer, 1.5, ctx.pixels_per_point());
                assert_eq!(app.viewport.zoom, 16., "world mode {world}");
                // The anchor pixel [134, 122] stays at its previous backing position.
                close(app.viewport.center, [131., 125.]);
                app.zoom_at_canvas(rect, pointer, -1.5, ctx.pixels_per_point());
                let expected = if world { 0.015 } else { 600. / (256. * 2.3) };
                assert!(
                    (app.viewport.zoom - expected).abs() < 0.0001,
                    "world mode {world}: {} != {expected}",
                    app.viewport.zoom
                );
            }
        });
    }

    #[test]
    fn wheel_speed_and_control_scaling_follow_source_additive_zoom() {
        let mut app = editor();
        app.world = true;
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.settings.value["mouseWheelSpeed"] = json!(2.);
        let rect = canvas();
        let ctx = egui::Context::default();
        for (scroll, modifiers, expected) in [
            (100., egui::Modifiers::NONE, 2.6),
            (1., egui::Modifiers::CTRL, 2.12),
        ] {
            app.viewport.zoom = 2.;
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., scroll),
                        modifiers,
                    }],
                    ..Default::default()
                },
                |ctx| app.wheel_zoom(ctx, rect, rect.center()),
            );
            assert!((app.viewport.zoom - expected).abs() < 0.0001);
        }
    }
}
