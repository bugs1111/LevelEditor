#[cfg(test)]
mod acceptance_review_repro {
    use super::*;
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 800.),
            )),
            events,
            modifiers,
            ..Default::default()
        };
        app.ui_commit_raw_input(ctx, &mut input);
        ctx.run(input, |ctx| {
            app.shortcuts(ctx);
            app.dialogs(ctx);
            app.palette_dialog(ctx);
            app.flush_ui_action(ctx);
        })
    }
    fn key(key: Key, command: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                command,
                mac_cmd: command,
                ..Default::default()
            },
        }
    }
    #[test]
    fn searching_another_entity_from_text_modal_commits_before_owner_changes() {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let source = p.add_entity_def("Source", 16, 16).unwrap();
        let target = p.add_entity_def("Target", 16, 16).unwrap();
        let uid = p
            .add_field_definition(Some(source), "Text", "Multilines", false)
            .unwrap();
        p.update_field_definition(uid, &serde_json::json!({"canBeNull":false}))
            .unwrap();
        let a = p.level_keys()[0];
        let b = p.add_level(None, "Other", 256, 256).unwrap();
        p.add_entity(a, 0, source, 16, 16).unwrap();
        p.add_entity(b, 0, source, 16, 16).unwrap();
        p.add_entity(b, 0, target, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        app.panel = Some(Panel::Entity);
        app.entity_index = Some(0);
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "(empty string)" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("multiline button");
        let click = |pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), click(true)],
        );
        frame(&mut app, &ctx, vec![click(false)]);
        frame(&mut app, &ctx, vec![]);
        assert!(text_editor::is_open(&ctx));
        frame(&mut app, &ctx, vec![egui::Event::Text("draft".into())]);
        frame(&mut app, &ctx, vec![key(Key::K, true)]);
        app.search = "entity Target".into();
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![key(Key::ArrowDown, false)]);
        frame(&mut app, &ctx, vec![key(Key::Enter, false)]);
        for _ in 0..5 {
            frame(&mut app, &ctx, vec![]);
        }
        assert!(
            !text_editor::is_open(&ctx),
            "Old modal owner must not become unreachable while navigation waits for validation"
        );
        assert_eq!(
            app.project.as_ref().unwrap().layer(a, 0).unwrap()["entityInstances"][0]["fieldInstances"]
                [0]["__value"],
            "draft"
        );
        assert_eq!(app.selected, Some(b));
        assert_eq!(app.entity_index, None);
        assert_eq!(app.panel, None);
        assert!(
            app.fit,
            "Search fits the destination level and flashes its entity"
        );
    }
}

#[cfg(test)]
mod acceptance_form_repro {
    use super::*;
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let modifiers = events
            .iter()
            .find_map(|e| match e {
                egui::Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600., 400.),
            )),
            events,
            modifiers,
            ..Default::default()
        };
        app.ui_commit_raw_input(ctx, &mut input);
        ctx.run(input, |ctx| {
            app.shortcuts(ctx);
            if !app.is_busy() {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let patch = ui_fields::form(
                        ui,
                        &app.project.as_ref().unwrap().json,
                        &[ui_fields::Property(
                            "doc",
                            "Doc",
                            ui_fields::Widget::Multiline,
                        )],
                    );
                    if ui_fields::changed(&patch) {
                        app.edit("Doc edited", |p| {
                            ui_fields::apply_patch(&mut p.json, &patch);
                            Ok(())
                        });
                    }
                });
            }
            app.flush_ui_action(ctx);
        })
    }
    #[test]
    fn textarea_final_keystroke_batched_with_save_must_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Doc.ldtk");
        let mut p = Project::new();
        p.json["doc"] = "".into();
        p.save(&path).unwrap();
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        app.set_project(p);
        let out = frame(&mut app, &ctx, vec![]);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r)
                    if r.rect.width() > 200. && r.rect.height() > 30. && r.rect.height() < 100. =>
                {
                    Some(r.rect.center())
                }
                _ => None,
            })
            .expect("textarea rectangle");
        let click = |pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), click(true)],
        );
        frame(&mut app, &ctx, vec![click(false)]);
        frame(&mut app, &ctx, vec![egui::Event::Text("first".into())]);
        assert!(!app.dirty);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Text(" last".into()),
                egui::Event::Key {
                    key: Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        command: true,
                        mac_cmd: true,
                        ..Default::default()
                    },
                },
            ],
        );
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while app.saving.is_some() && Instant::now() < until {
            app.poll_services(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(Project::load(&path).unwrap().json["doc"], "first last");
    }
}
use super::*;
