use super::*;

impl EditorApp {
    pub(super) fn erase_entities_at_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        local: [f32; 2],
        pressed: bool,
    ) {
        let mut down = !pressed;
        let mut positions = Vec::new();
        ctx.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    } if *button == egui::PointerButton::Secondary
                        || cfg!(target_os = "macos")
                            && *button == egui::PointerButton::Primary
                            && modifiers.ctrl =>
                    {
                        if *pressed {
                            down = true;
                            // The source entity tool runs one useAt sample
                            // before its own press-time removal.
                            positions.push(*pos);
                        }
                        if down {
                            positions.push(*pos);
                        }
                        if !*pressed {
                            down = false;
                        }
                    }
                    egui::Event::PointerMoved(pos) if down => positions.push(*pos),
                    _ => {}
                }
            }
        });
        if positions.is_empty() {
            self.erase_entity_or_point_at(local);
        } else {
            for position in positions.into_iter().filter(|p| rect.contains(*p)) {
                let (Some(project), Some(key)) = (&self.project, self.selected) else {
                    return;
                };
                let (world, _) = self.canvas_pointer_pixels(
                    project,
                    key,
                    rect,
                    position,
                    ctx.pixels_per_point(),
                );
                let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, world);
                self.erase_entity_or_point_at(local);
            }
        }
    }
    fn erase_entity_or_point_at(&mut self, local: [f32; 2]) {
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let grid = number(layer, "__gridSize", 16.);
        let Some((index, uid, array_index)) = point_tools::hit_point(project, layer, local) else {
            self.erase_entity(local);
            return;
        };
        let entity = layer["entityInstances"][index].clone();
        let iid = text(&entity, "iid", "").to_owned();
        let field = entity["fieldInstances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["defUid"] == uid)
            .unwrap();
        let array = field["__type"] == "Array<Point>";
        let point = if array {
            &field["__value"][array_index]
        } else {
            &field["__value"]
        };
        let position = [
            (number(point, "cx", 0.) + 0.5) * grid,
            (number(point, "cy", 0.) + 0.5) * grid,
        ];
        let definition = project.json["defs"]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["uid"] == entity["defUid"])
            .unwrap();
        let color = canvas::entity_smart_color(project, &entity, definition, true);
        let snapshot = self.capture_entity_render(key, self.layer);
        self.history
            .mark_entity_change(project, key, self.layer, &entity);
        let p = self.project.as_mut().unwrap();
        let result = if array {
            p.edit_field_array(
                key,
                Some(&iid),
                uid,
                ldtk_core::FieldArrayEdit::Remove { index: array_index },
            )
        } else {
            p.set_field_editor_value(key, Some(&iid), uid, Value::Null)
        };
        match result {
            Ok(()) => {
                self.defer_entity_render(snapshot);
                self.dirty = true;
                self.chain_entity = None;
                self.selection = None;
                self.selection_group = None;
                self.entity_index = Some(index);
                self.entity_uid = entity["defUid"].as_i64();
                self.panel = Some(Panel::Entity);
                self.bleep_entity(key, self.layer, &entity);
                self.bleep_point(key, position, color);
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut EditorApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256., 256.),
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
    fn button(x: f32, y: f32, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: egui::pos2(x, y),
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        }
    }
    fn setup(points: bool) -> (EditorApp, LevelKey) {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        p.update_entity_definition(uid, &serde_json::json!({"pivotX":0.5,"pivotY":1}))
            .unwrap();
        let key = p.level_keys()[0];
        if points {
            let f = p
                .add_field_definition(Some(uid), "Path", "Point", true)
                .unwrap();
            p.add_entity(key, 0, uid, 200, 16).unwrap();
            let iid = text(&p.layer(key, 0).unwrap()["entityInstances"][0], "iid", "").to_owned();
            p.set_field_editor_value(
                key,
                Some(&iid),
                f,
                serde_json::json!([{"cx":1,"cy":4},{"cx":3,"cy":4},{"cx":5,"cy":4}]),
            )
            .unwrap();
        } else {
            for x in [16, 48, 80] {
                p.add_entity(key, 0, uid, x, 16).unwrap();
            }
        }
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(uid);
        app.fit = false;
        app.viewport = Viewport {
            center: [128., 128.],
            zoom: 1.,
        };
        (app, key)
    }
    #[test]
    fn right_drag_samples_move_and_release_and_clears_stale_inspector_index() {
        let (mut app, key) = setup(false);
        let ctx = egui::Context::default();
        app.entity_index = Some(2);
        app.panel = Some(Panel::Entity);
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(20., 8.)),
                button(20., 8., true),
            ],
        );
        assert!(app.entity_index.is_none());
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(55., 8.))],
        );
        frame(&mut app, &ctx, vec![button(84., 8., false)]);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(
            !app.history
                .can_undo_level(app.project.as_ref().unwrap(), key)
        );
    }
    #[test]
    fn a_pending_stamp_never_pastes_on_a_right_click() {
        let (mut app, key) = setup(false);
        let ctx = egui::Context::default();
        app.select_all();
        app.copy_selection(&ctx, false);
        app.stamp = app.clipboard.clone();
        app.selection = None;
        app.selection_group = None;
        let before =
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"].clone();
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(120., 120.)),
                button(120., 120., true),
            ],
        );
        frame(&mut app, &ctx, vec![button(120., 120., false)]);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"],
            before
        );
    }
    #[test]
    fn point_eraser_uses_the_rounded_cell_for_press_and_release() {
        let (mut app, key) = setup(true);
        let ctx = egui::Context::default();
        app.viewport.zoom = 2.;
        frame(&mut app, &ctx, vec![]);
        let edge = egui::pos2(31., 16.);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(edge),
                button(edge.x, edge.y, true),
            ],
        );
        frame(&mut app, &ctx, vec![button(edge.x, edge.y, false)]);
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 1);
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!([{"cx":1,"cy":4},{"cx":3,"cy":4}]),
        );
    }

    #[test]
    fn point_eraser_respects_entity_display_order_when_a_body_overlaps_a_point() {
        let (mut app, key) = setup(true);
        let project = app.project.as_mut().unwrap();
        let layer_uid = project.layer(key, 0).unwrap()["layerDefUid"]
            .as_i64()
            .unwrap();
        project
            .update_layer_properties(
                layer_uid,
                &serde_json::json!({
                    "entityDepthSorting":"YDescending"
                }),
            )
            .unwrap();
        project
            .add_entity(key, 0, app.entity_uid.unwrap(), 88, 80)
            .unwrap();
        let layer = project.layer(key, 0).unwrap();
        let hit = point_tools::hit_point(project, layer, [88., 72.]).unwrap();
        assert_eq!((hit.0, hit.2), (0, 2));
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(88., 72.)),
                button(88., 72., true),
            ],
        );
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 1);
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!([{"cx":1,"cy":4},{"cx":3,"cy":4}]),
        );
        frame(&mut app, &ctx, vec![button(120., 120., false)]);
        app.undo_action(false);
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 2);
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!([{"cx":1,"cy":4},{"cx":3,"cy":4},{"cx":5,"cy":4}]),
        );
    }

    #[test]
    fn a_point_array_eraser_stroke_is_one_undo_and_never_deletes_its_owner() {
        let (mut app, key) = setup(true);
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(24., 72.)),
                button(24., 72., true),
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(56., 72.))],
        );
        frame(&mut app, &ctx, vec![button(88., 72., false)]);
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 1);
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!([])
        );
        app.undo_action(false);
        assert_eq!(app.project.as_ref().unwrap().layer(key,0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"].as_array().unwrap().len(),3);
        assert!(
            !app.history
                .can_undo_level(app.project.as_ref().unwrap(), key)
        );
    }
}
