use super::*;
use serde_json::json;

impl EditorApp {
    pub(super) fn navigate_neighbor_level(&mut self, ctx: &egui::Context, key: LevelKey) {
        if self.defer_ui_action(ctx, ui_commit::DeferredUiAction::NeighborLevel(key)) {
            return;
        }
        let (Some(project), Some(previous)) = (&self.project, self.selected) else {
            return;
        };
        let old_origin = navigation::level_world_origin(project, previous);
        let new_origin = navigation::level_world_origin(project, key);
        self.navigate_level(key, false);
        if self.selected == Some(key) {
            self.camera_motion.cancel();
            for axis in 0..2 {
                self.viewport.center[axis] += old_origin[axis] - new_origin[axis];
            }
        }
    }

    fn clear_level_selection(&mut self) {
        self.finish_rule_edit();
        self.level_form_depth_override = None;
        self.entity_index = None;
        self.selection = None;
        self.selection_group = None;
        self.stamp = None;
        self.hidden.clear();
        self.point_pick = None;
        if matches!(self.panel, Some(Panel::Entity | Panel::Rules)) {
            self.panel = None;
        }
    }
    pub(super) fn navigate_level(&mut self, key: LevelKey, fit: bool) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::Navigate { key, fit }) {
            return;
        }
        if self.project.as_ref().and_then(|p| p.level(key)).is_none() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.async_paint = None;
        self.cancel_world_context();
        self.world_selection.clear();
        self.world_hover = None;
        let layer_uid = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
            .map(|l| l["layerDefUid"].clone());
        if self.selected != Some(key) && self.panel == Some(Panel::Level) {
            self.panel = None;
        }
        self.clear_level_selection();
        self.selected = Some(key);
        self.active_world = key.world;
        self.update_reference_world_preview(key);
        let level = self.project.as_ref().unwrap().level(key).unwrap();
        self.layer = layer_uid
            .and_then(|uid| {
                level["layerInstances"]
                    .as_array()?
                    .iter()
                    .position(|l| l["layerDefUid"] == uid)
            })
            .unwrap_or(0);
        self.world_depth = level["worldDepth"].as_i64().unwrap_or(0);
        self.fit = fit;
        if fit {
            self.camera_motion.cancel();
        }
        self.normalize_palette();
        self.remember_selected_level();
    }
    pub(super) fn navigate_world(&mut self, world: Option<usize>) {
        self.switch_world(world, true);
    }
    pub(in crate::app) fn switch_world(&mut self, world: Option<usize>, open_world_view: bool) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(
            &ctx,
            ui_commit::DeferredUiAction::World {
                world,
                open_world_view,
            },
        ) {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.async_paint = None;
        self.cancel_world_context();
        self.world_selection.clear();
        self.world_hover = None;
        if let Some(key) = self
            .project
            .as_ref()
            .and_then(|p| p.level_keys().into_iter().find(|key| key.world == world))
        {
            self.navigate_level(key, true);
        } else {
            self.clear_level_selection();
            self.selected = None;
            self.active_world = world;
            self.world_depth = 0;
            self.layer = 0;
            self.fit = true;
        }
        if self.world {
            self.notify_world_view();
        }
        self.show_panel((open_world_view || self.selected.is_none()).then_some(Panel::World));
        self.reset_reference_world_preview();
        self.camera_motion.cancel();
        self.fit = true;
    }

    pub(super) fn animate_camera_fit(&mut self) {
        if let (Some(project), Some(key)) = (&self.project, self.selected) {
            self.fit = false;
            self.camera_motion
                .fit(canvas::bounds(project, key, self.world), self.world);
        }
    }

    pub(super) fn set_world_view(&mut self, world: bool) {
        if self.world == world {
            return;
        }
        self.world = world;
        self.level_form_depth_override = None;
        self.world_hover = None;
        self.reset_reference_world_preview();
        self.fit = false;
        self.world_selection.clear();
        self.selection = None;
        self.selection_group = None;
        self.entity_index = None;
        self.stamp = None;
        if world {
            self.notify_world_view();
        }
        if let (Some(project), Some(key)) = (&self.project, self.selected) {
            let placements = canvas::placements(project, key);
            if let Some((_, bounds)) = placements.iter().find(|(placed, _)| *placed == key) {
                let direction = if world { 1.0 } else { -1.0 };
                let offset = [direction * bounds[0], direction * bounds[1]];
                self.viewport.center[0] += offset[0];
                self.viewport.center[1] += offset[1];
                self.camera_motion.translate_center(offset);
            }
            if world {
                self.camera_motion
                    .enter_world(canvas::bounds(project, key, true), placements.len() <= 1);
            } else {
                self.world_depth = project
                    .level(key)
                    .and_then(|level| level["worldDepth"].as_i64())
                    .unwrap_or(0);
                self.animate_camera_fit();
            }
        }
    }

    pub(in crate::app) fn select_layer_definition(&mut self, uid: Option<i64>) {
        self.definition_uid = uid;
        let index = uid.and_then(|uid| {
            let level = self.project.as_ref()?.level(self.selected?)?;
            level["layerInstances"]
                .as_array()?
                .iter()
                .position(|layer| layer["layerDefUid"] == uid)
        });
        if let Some(index) = index {
            self.switch_layer(index);
        }
    }

    pub(super) fn show_panel(&mut self, panel: Option<Panel>) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::Panel(panel)) {
            return;
        }
        // Opening a source modal stops the Point tool, while reference picking can span panels.
        if self.picking_point_in_level_panel()
            || self.picking_point() && panel.is_some_and(|panel| panel != Panel::Entity)
        {
            self.cancel_point_pick();
        }
        if self.panel == Some(Panel::Settings) && panel != Some(Panel::Settings) {
            self.finish_settings_dialog(&ctx);
        }
        if self.panel == Some(Panel::Rules) && panel != Some(Panel::Rules) {
            self.finish_rule_edit();
            self.rule_hover = None;
            self.last_rule_list_row = None;
        }
        self.finish_gesture();
        self.finish_stroke();
        self.async_paint = None;
        self.cancel_world_context();
        if matches!(
            panel,
            Some(
                Panel::Project
                    | Panel::Level
                    | Panel::Layers
                    | Panel::Entities
                    | Panel::Enums
                    | Panel::Tilesets
                    | Panel::Help
                    | Panel::World
                    | Panel::Fields
                    | Panel::Rules
            )
        ) {
            // Source panels clear the level selection when they open.
            self.selection = None;
            self.selection_group = None;
            self.entity_index = None;
        }
        if panel == Some(Panel::World) {
            self.set_world_view(true);
        } else if self.panel == Some(Panel::World) || panel.is_none() && self.world {
            self.set_world_view(false);
        }
        if self.panel != panel {
            // The standalone form opens directly on the current level.
            // The embedded World form checks the active depth on opening.
            self.level_form_depth_override = self.selected.filter(|_| panel == Some(Panel::Level));
            if panel == Some(Panel::Layers) {
                self.definition_uid = self
                    .selected
                    .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
                    .and_then(|layer| layer["layerDefUid"].as_i64());
            }
        }
        self.panel = panel;
    }

    pub(super) fn dismiss_panel(&mut self) {
        let return_to_level = self.panel == Some(Panel::Fields) && self.field_owner.is_none();
        self.show_panel(return_to_level.then_some(Panel::Level));
    }

    pub(super) fn toggle_toolbar_panel(&mut self, panel: Panel) {
        let open = self.panel != Some(panel);
        self.show_panel(open.then_some(panel));
    }
    pub(super) fn follow_entity(&mut self, iid: &str) -> bool {
        let target = self.project.as_ref().and_then(|p| {
            p.level_keys().into_iter().find_map(|key| {
                p.level(key)?["layerInstances"]
                    .as_array()?
                    .iter()
                    .enumerate()
                    .find_map(|(layer, l)| {
                        l["entityInstances"]
                            .as_array()?
                            .iter()
                            .position(|e| e["iid"] == iid)
                            .map(|index| (key, layer, index))
                    })
            })
        });
        let Some((key, layer, index)) = target else {
            return false;
        };
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::FollowEntity(iid.into())) {
            return true;
        }
        self.navigate_level(key, false);
        self.world = false;
        self.layer = layer;
        self.entity_index = Some(index);
        self.panel = Some(Panel::Entity);
        if let Some(project) = &self.project
            && let Some(entity) = project
                .layer(key, layer)
                .and_then(|l| l["entityInstances"].get(index))
        {
            self.entity_uid = entity["defUid"].as_i64();
            self.bleep_entity(key, layer, entity);
            // followEntityRef also calls SelectionTool.select, which flashes again.
            self.bleep_entity(key, layer, entity);
            let geometries = self.selection_geometries(key);
            let regions =
                json!([{"layerIndex":layer,"rect":[0,0,1,1],"geometry":geometries[layer]}]);
            if let Ok(mut group) =
                project.copy_selection_group_from_regions(key, &regions, [0, 0, 1, 1])
            {
                group["layers"][0]["clipboard"]["entityInstances"] = json!([entity]);
                group["layers"][0]["points"] = json!([]);
                group["originalRects"] = json!([]);
                group["emptySpace"] = json!(false);
                if let Ok(group) =
                    project.refresh_selection_group_geometry(key, &group, &geometries)
                {
                    self.selection_group = Some(group);
                }
            }
            self.camera_motion.cancel_pan();
            self.viewport.center = canvas::layer_to_world(
                project,
                key,
                layer,
                &self.viewport,
                [
                    entity["px"][0].as_f64().unwrap_or(0.0) as f32,
                    entity["px"][1].as_f64().unwrap_or(0.0) as f32,
                ],
            );
        }
        if let Some(group) = self.selection_group.take() {
            self.set_selection_group(group);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            focused: true,
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
        let _ = ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
    }

    fn press(app: &mut EditorApp, ctx: &egui::Context, key: Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                }],
            );
        }
    }

    fn camera_editor(density: f32) -> (EditorApp, egui::Context) {
        let mut project = Project::new();
        let level = project.level_keys()[0];
        project.move_level(level, 1000, 2000, 0).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.options_mut(|options| options.zoom_factor = density);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(project);
        frame(&mut app, &ctx, vec![]);
        (app, ctx)
    }

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    fn near_zoom_anchor(view: &Viewport, expected: [f32; 2]) {
        for (actual, expected) in view.center.into_iter().zip(expected) {
            assert!((actual - expected).abs() * view.zoom <= 1.01);
        }
    }

    #[test]
    fn manual_world_entry_animates_only_zoom_at_backing_density_and_exit_fits_the_level() {
        let (mut app, ctx) = camera_editor(2.);
        app.viewport.center = [80., 112.];
        app.viewport.zoom = 2.;
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, &ctx, vec![key(Key::PageUp, true)]);
        assert!(app.world);
        near_zoom_anchor(&app.viewport, [1080., 2112.]);
        let target = (1140. / (256. * 1.1 + 300. * 2.) * 0.8_f32).max(0.30 / 2.) * 0.5;
        near(app.viewport.zoom, 2. + (target - 2.) * 0.22 * 2. * 2.);
        for _ in 0..114 {
            let center = app.viewport.center;
            frame(&mut app, &ctx, vec![]);
            near_zoom_anchor(&app.viewport, center);
        }
        assert!((app.viewport.zoom - target).abs() <= 0.04 * app.viewport.zoom);
        assert_ne!(app.viewport.zoom, target);

        let zoom = app.viewport.zoom;
        let local = [app.viewport.center[0] - 1000., app.viewport.center[1] - 2000.];
        frame(&mut app, &ctx, vec![key(Key::Backtick, true)]);
        assert!(!app.world);
        near_zoom_anchor(&app.viewport, local.map(|axis| axis + (128. - axis) * 0.15));
        let level_target = 900. / (256. + 80. * 2.);
        near(
            app.viewport.zoom,
            zoom + (level_target - zoom) * (0.22 * zoom * 2.).max(0.1),
        );
    }

    #[test]
    fn entering_world_during_space_fit_keeps_center_target_and_middle_pan_cancels_it_only() {
        let (mut app, ctx) = camera_editor(1.);
        app.viewport.center = [80., 64.];
        app.viewport.zoom = 0.4;
        press(&mut app, &ctx, Key::Space, egui::Modifiers::NONE);
        let after_space = app.viewport.center;
        near(after_space[0], 80. + (128. - 80.) * 0.15);
        let key = egui::Event::Key {
            key: Key::Backtick,
            physical_key: Some(Key::Backtick),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, &ctx, vec![key]);
        assert!(app.world);
        near(
            app.viewport.center[0],
            1000. + after_space[0] + (128. - after_space[0]) * 0.15,
        );

        let start = egui::pos2(1000., 450.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(start)]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let end = start + egui::vec2(30., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        let center_after_pan = app.viewport.center;
        let zoom_after_pan = app.viewport.zoom;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Middle,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        near_zoom_anchor(&app.viewport, center_after_pan);
        assert_ne!(app.viewport.zoom, zoom_after_pan);
    }

    #[test]
    fn wheel_uses_animated_minimum_then_cancels_both_space_fit_targets() {
        let (mut app, ctx) = camera_editor(1.);
        app.settings.value["autoWorldModeSwitch"] = json!("Never");
        app.viewport.center = [80., 64.];
        app.viewport.zoom = 0.4;
        press(&mut app, &ctx, Key::Space, egui::Modifiers::NONE);
        let pointer = egui::pos2(990., 505.);
        let anchor = |app: &EditorApp| {
            app.viewport
                .to_world(pointer.into(), [300., 0.], [1140., 900.])
        };
        let before = anchor(&app);
        let expected_zoom = app.viewport.zoom * 1.006;
        assert!(expected_zoom < 900. / (256. * 2.3));
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., 4.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        near(app.viewport.zoom, expected_zoom);
        for (actual, expected) in anchor(&app).into_iter().zip(before) {
            assert!((actual - expected).abs() * expected_zoom <= 1.);
        }
        let after = app.viewport.center;
        for _ in 0..5 {
            frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(app.viewport.center, after);
        near(app.viewport.zoom, expected_zoom);
    }

    #[test]
    fn following_an_entity_replaces_the_previous_fit_center_and_keeps_zooming() {
        let (mut app, ctx) = camera_editor(1.);
        let level = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let def = project.add_entity_def("Target", 16, 16).unwrap();
        assert!(project.add_entity(level, 0, def, 224, 192).unwrap());
        let iid = project.layer(level, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        app.viewport.center = [64., 64.];
        app.viewport.zoom = 0.5;
        press(&mut app, &ctx, Key::Space, egui::Modifiers::NONE);
        assert!(app.follow_entity(&iid));
        let center = app.viewport.center;
        let zoom = app.viewport.zoom;
        frame(&mut app, &ctx, vec![]);
        near_zoom_anchor(&app.viewport, center);
        assert!(app.viewport.zoom > zoom);
    }

    #[test]
    fn stationary_middle_and_space_secondary_releases_cancel_their_motion_targets() {
        for button in [egui::PointerButton::Middle, egui::PointerButton::Secondary] {
            let (mut app, ctx) = camera_editor(1.);
            let pos = egui::pos2(1000., 450.);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
            app.viewport.center = [64., 64.];
            app.viewport.zoom = 0.5;
            press(&mut app, &ctx, Key::Space, egui::Modifiers::NONE);
            let secondary = button == egui::PointerButton::Secondary;
            if secondary {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key: Key::Space,
                        physical_key: Some(Key::Space),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            let center = app.viewport.center;
            let zoom = app.viewport.zoom;
            frame(&mut app, &ctx, vec![]);
            if secondary {
                assert_eq!(app.viewport.center, center, "{button:?}");
            } else {
                near_zoom_anchor(&app.viewport, center);
            }
            assert_eq!(app.viewport.zoom == zoom, secondary, "{button:?}");
        }
    }

    #[test]
    fn world_depth_shortcut_from_level_only_opens_world_view() {
        let mut p = Project::new();
        let first = p.level_keys()[0];
        let second = p.add_level(None, "Further", 256, 256).unwrap();
        p.move_level(second, 512, 0, 1).unwrap();
        let original = p.json.clone();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        frame(&mut app, &ctx, vec![]);
        for (key, modifiers) in [
            (Key::PageUp, egui::Modifiers::NONE),
            (Key::PageUp, egui::Modifiers::SHIFT),
            (Key::PageDown, egui::Modifiers::COMMAND),
        ] {
            press(&mut app, &ctx, key, modifiers);
            assert_eq!(app.panel, Some(Panel::World), "{key:?} {modifiers:?}");
            assert!(app.world);
            assert_eq!(app.world_depth, 0);
            assert_eq!(app.selected, Some(first));
            assert_eq!(app.project.as_ref().unwrap().json, original);
            assert!(!app.dirty);
            press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
            assert!(!app.world);
        }
    }

    #[test]
    fn world_depth_shortcuts_bound_browsing_and_move_from_the_selected_levels_depth() {
        let mut p = Project::new();
        let other_world = p.add_world("Elsewhere").unwrap();
        let first = p.level_keys()[0];
        let second = p.add_level(first.world, "Further", 256, 256).unwrap();
        let remote = p.add_level(Some(other_world), "Remote", 256, 256).unwrap();
        p.move_level(second, 512, 0, 2).unwrap();
        p.move_level(remote, 0, 0, 9).unwrap();
        let original = p.json.clone();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        frame(&mut app, &ctx, vec![]);
        press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
        app.camera_motion.cancel();
        app.world_toggle_selection(first);
        let selected_levels = app.world_selection.clone();
        let center = app.viewport.center;
        for expected in [1, 2, 2] {
            press(&mut app, &ctx, Key::PageUp, egui::Modifiers::NONE);
            assert_eq!(app.world_depth, expected);
        }
        assert_eq!(app.selected, Some(first));
        assert_eq!(app.world_selection, selected_levels);
        assert_eq!(app.viewport.center, center);
        assert_eq!(app.project.as_ref().unwrap().json, original);

        press(&mut app, &ctx, Key::PageUp, egui::Modifiers::SHIFT);
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.level(first).unwrap()["worldDepth"], 0);
        assert_eq!(p.level(second).unwrap()["worldDepth"], 1);
        assert_eq!(p.level(remote).unwrap()["worldDepth"], 9);
        assert!(app.dirty);
        assert_eq!(app.world_depth, 0);
        assert_eq!(app.world_selection, selected_levels);

        press(&mut app, &ctx, Key::PageDown, egui::Modifiers::NONE);
        assert_eq!(app.world_depth, 0);
        press(&mut app, &ctx, Key::PageUp, egui::Modifiers::NONE);
        assert_eq!(app.world_depth, 1);
        press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
        assert!(!app.world);
        assert_eq!(app.world_depth, 0);
        assert!(app.world_selection.is_empty());
    }

    #[test]
    fn returning_to_the_level_view_restores_the_selected_levels_depth() {
        let mut p = Project::new();
        let anchor = p.level_keys()[0];
        let neighbor = p.add_level(anchor.world, "Neighbor", 256, 256).unwrap();
        p.move_level(anchor, 0, 0, 1).unwrap();
        p.move_level(neighbor, 512, 0, 0).unwrap();
        let original = p.json.clone();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        frame(&mut app, &ctx, vec![]);

        press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
        press(&mut app, &ctx, Key::PageDown, egui::Modifiers::NONE);
        assert_eq!(app.world_depth, 0);
        press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
        assert!(!app.world);
        assert_eq!(app.world_depth, 1);
        assert_eq!(app.selected, Some(anchor));

        press(&mut app, &ctx, Key::C, egui::Modifiers::NONE);
        assert_eq!(app.panel, Some(Panel::Level));
        press(&mut app, &ctx, Key::C, egui::Modifiers::NONE);
        press(&mut app, &ctx, Key::Backtick, egui::Modifiers::NONE);
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
        assert_eq!(app.world_depth, 1);
        assert_eq!(app.selected, Some(anchor));
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn selecting_a_different_level_closes_its_standalone_properties() {
        let mut project = Project::new();
        let first = project.level_keys()[0];
        let second = project.add_level(first.world, "Second", 256, 256).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.show_panel(Some(Panel::Level));
        app.navigate_level(first, false);
        assert_eq!(app.panel, Some(Panel::Level));
        app.navigate_level(second, false);
        assert_eq!(app.selected, Some(second));
        assert_eq!(app.panel, None);
        app.show_panel(Some(Panel::World));
        app.navigate_level(first, false);
        assert_eq!(app.panel, Some(Panel::World));
        assert!(!app.dirty);
    }

    #[test]
    fn command_palette_world_result_opens_the_world_panel_and_fits_its_levels() {
        let mut p = Project::new();
        let world = p.add_world("RemoteWorld").unwrap();
        let first = p.add_level(Some(world), "FirstTarget", 256, 256).unwrap();
        let second = p.add_level(Some(world), "SecondTarget", 256, 256).unwrap();
        p.move_level(first, 1024, 2048, 2).unwrap();
        p.move_level(second, 1536, 2048, 2).unwrap();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        frame(&mut app, &ctx, vec![]);
        press(&mut app, &ctx, Key::P, egui::Modifiers::COMMAND);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("RemoteWorld".into())],
        );
        assert_eq!(app.search, "RemoteWorld");
        press(&mut app, &ctx, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(app.active_world, Some(world));
        assert_eq!(app.selected, Some(first));
        assert_eq!(app.world_depth, 2);
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
        assert_eq!(app.viewport.center, [1408., 2176.]);
    }

    #[test]
    fn opening_a_definition_panel_clears_the_canvas_selection_but_keeps_the_clipboard() {
        let mut project = Project::new();
        project.add_layer("Entities", "Entities", 16).unwrap();
        let uid = project.add_entity_def("Actor", 16, 16).unwrap();
        let level = project.level_keys()[0];
        project.add_entity(level, 0, uid, 64, 80).unwrap();
        project.add_entity(level, 0, uid, 104, 104).unwrap();
        let original = project.json.clone();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(project);
        frame(&mut app, &ctx, vec![]);

        press(&mut app, &ctx, Key::A, egui::Modifiers::COMMAND);
        press(&mut app, &ctx, Key::C, egui::Modifiers::COMMAND);
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);
        let clipboard = app.clipboard.clone().unwrap();
        press(&mut app, &ctx, Key::L, egui::Modifiers::NONE);
        assert_eq!(app.panel, Some(Panel::Layers));
        assert!(app.selection.is_none() && app.selection_group.is_none());
        press(&mut app, &ctx, Key::L, egui::Modifiers::NONE);
        assert!(app.panel.is_none());
        press(&mut app, &ctx, Key::Delete, egui::Modifiers::NONE);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert_eq!(app.clipboard, Some(clipboard));
        assert!(!app.dirty);
    }

    #[test]
    fn navigation_never_carries_an_entity_index_or_selection_into_another_level() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let a = p.level_keys()[0];
        let b = p.add_level(None, "Second", 256, 256).unwrap();
        p.add_entity(a, 0, uid, 16, 16).unwrap();
        p.add_entity(b, 0, uid, 32, 32).unwrap();
        p.move_level(b, 0, 0, 3).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_index = Some(0);
        app.selection = Some([0, 0, 1, 1]);
        app.navigate_level(b, true);
        app.delete_selection();
        assert!(
            app.entity_index.is_none() && app.selection.is_none() && app.selection_group.is_none()
        );
        assert_eq!(app.world_depth, 3);
        assert_eq!(
            app.project.as_ref().unwrap().layer(b, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}
