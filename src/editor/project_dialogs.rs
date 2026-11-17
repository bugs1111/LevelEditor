use super::*;

pub(super) fn suggest_backups(project: &Project) -> bool {
    let Some(path) = &project.path else {
        return false;
    };
    let examples = resource_path("examples/original");
    let bundled = path
        .canonicalize()
        .unwrap_or_else(|_| path.clone())
        .starts_with(examples.canonicalize().unwrap_or(examples));
    !bundled
        && project.json["backupOnSave"] != true
        && !project.is_backup()
        && project.level_keys().len() >= 8
        && !project.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.iter().any(|f| f == "IgnoreBackupSuggest"))
}

fn dialog_frame(padding: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::MEDIUM)
        .corner_radius(3)
        .inner_margin(padding)
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 32,
            spread: 0,
            color: Color32::from_black_alpha(153),
        })
}

fn button(
    ui: &mut egui::Ui,
    label: &str,
    size: Vec2,
    gray: bool,
    font_size: f32,
) -> egui::Response {
    super::panels::source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(RichText::new(label).size(font_size).color(Color32::WHITE))
            .fill(if gray {
                theme::LIGHT
            } else {
                Color32::from_rgb(227, 77, 0)
            })
            .corner_radius(2),
    )
}

impl EditorApp {
    pub(super) fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("unsaved-changes"))
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let message = "Do you want to save before leaving?";
                let width = ui.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap(
                            message.into(),
                            FontId::proportional(14.667),
                            Color32::WHITE,
                        )
                        .size()
                        .x
                });
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(RichText::new(message).size(14.667).color(Color32::WHITE));
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    for (index, label) in ["YES", "NO", "CANCEL"].into_iter().enumerate() {
                        if button(
                            ui,
                            label,
                            egui::vec2((width - 4.) / 3., 35.),
                            index == 2,
                            14.667,
                        )
                        .clicked()
                        {
                            choice = Some(index);
                        }
                    }
                });
            });
        match choice {
            Some(0) if self.save(false) => {
                if let Some(action) = self.pending.take() {
                    self.execute(ctx, action);
                }
            }
            Some(1) => {
                if let Some(action) = self.pending.take() {
                    self.execute(ctx, action);
                }
            }
            Some(2) => self.pending = None,
            _ if modal.should_close() => self.pending = None,
            _ => {}
        }
    }

    pub(super) fn show_backup_suggestion(&mut self, ctx: &egui::Context) {
        let mut enable = None;
        egui::Modal::new(egui::Id::new("suggest-backups"))
            .frame(dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(400.);
                ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                let (header, _) = ui.allocate_exact_size(egui::vec2(400., 36.), Sense::hover());
                ui.painter().rect_filled(header, 0., theme::LIGHT);
                ui.painter().text(header.left_center() + egui::vec2(20., 0.), Align2::LEFT_CENTER,
                    "ENABLE BACKUPS", FontId::new(20., egui::FontFamily::Name("panel-title".into())), Color32::WHITE);
                egui::Frame::new().inner_margin(20).show(ui, |ui| {
                    ui.set_width(360.);
                    ui.label(RichText::new("As your project is growing bigger, it is STRONGLY advised to enable BACKUPS, to secure your work.")
                        .size(14.667).color(Color32::WHITE));
                    ui.add_space(16.);
                    if button(ui, "Enable backups when saving", egui::vec2(360., 67.333), false, 17.333).clicked() {
                        enable = Some(true);
                    }
                    ui.add_space(2.);
                    if button(ui, "No, and I understand the risk.", egui::vec2(360., 35.), true, 14.667).clicked() {
                        enable = Some(false);
                    }
                });
            });
        if let Some(enable) = enable {
            self.edit("Backup settings changed", |project| {
                if enable {
                    project.json["backupOnSave"] = serde_json::json!(true);
                } else {
                    project.json["flags"]
                        .as_array_mut()
                        .unwrap()
                        .push(serde_json::json!("IgnoreBackupSuggest"));
                }
                Ok(())
            });
            self.suggest_backups = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        app.repaint = Some(ctx.clone());
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            focused: true,
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
        })
    }

    fn label(output: &egui::FullOutput, value: &str) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == value => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible label: {value}"))
    }

    fn click(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    #[test]
    fn leaving_dirty_project_uses_source_question_and_three_horizontal_choices() {
        for action in [Action::Close, Action::Quit] {
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.dirty = true;
            app.pending = Some(action);
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            let question = label(&output, "Do you want to save before leaving?");
            let choices = ["YES", "NO", "CANCEL"].map(|name| label(&output, name));
            assert!(
                choices[0].center().x < choices[1].center().x
                    && choices[1].center().x < choices[2].center().x
            );
            assert!(
                choices
                    .iter()
                    .all(|rect| (rect.center().y - choices[0].center().y).abs() < 1.
                        && rect.top() > question.bottom())
            );
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text()=="Unsaved changes")));
            click(&mut app, &ctx, choices[2].center());
            assert!(app.project.is_some() && app.dirty && app.pending.is_none());
            assert!(!app.allow_close);
        }
    }

    fn large_project() -> Project {
        let mut project = Project::new();
        for index in 1..8 {
            project
                .add_level(None, &format!("Level_{index}"), 256, 256)
                .unwrap();
        }
        project.path = Some(std::env::temp_dir().join("level-editor-backup-prompt.ldtk"));
        project.json["backupOnSave"] = json!(false);
        project
    }

    #[test]
    fn source_backup_choices_enable_backups_or_remember_the_decline() {
        for (choice, enabled) in [
            ("Enable backups when saving", true),
            ("No, and I understand the risk.", false),
        ] {
            let mut app = EditorApp::empty();
            app.set_project(large_project());
            let original = app.project.as_ref().unwrap().json.clone();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            label(&output, "ENABLE BACKUPS");
            let enable = label(&output, "Enable backups when saving");
            let decline = label(&output, "No, and I understand the risk.");
            assert!(enable.bottom() < decline.top());
            assert_eq!(
                app.project.as_ref().unwrap().json,
                original,
                "opening must not change the project"
            );
            click(&mut app, &ctx, label(&output, choice).center());
            let project = app.project.as_ref().unwrap();
            assert_eq!(project.json["backupOnSave"], json!(enabled));
            assert_eq!(
                project.json["flags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f == "IgnoreBackupSuggest"),
                !enabled
            );
            assert!(app.dirty);
            let project = project.clone();
            app.set_project(project);
            let output = frame(&mut app, &ctx, vec![]);
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text()=="ENABLE BACKUPS")));
        }
    }

    #[test]
    fn backup_suggestion_respects_the_source_threshold_and_suppressions() {
        let mut project = large_project();
        assert!(suggest_backups(&project));
        let mut small = project.clone();
        small.json["levels"].as_array_mut().unwrap().pop();
        assert!(!suggest_backups(&small));
        project.json["backupOnSave"] = json!(true);
        assert!(!suggest_backups(&project));
        project.json["backupOnSave"] = json!(false);
        project.json["flags"] = json!(["IgnoreBackupSuggest"]);
        assert!(!suggest_backups(&project));
        project.json["flags"] = json!([]);
        project.path = Some(resource_path("examples/original/Entities.ldtk"));
        assert!(!suggest_backups(&project));
        project.path = None;
        assert!(!suggest_backups(&project));
    }

    #[test]
    fn function_keys_show_the_changed_layer_name_and_type_without_editing() {
        let mut project = Project::new();
        for (name, kind) in [
            ("Actors", "Entities"),
            ("Walls", "IntGrid"),
            ("Decoration", "Tiles"),
            ("Generated", "AutoLayer"),
        ] {
            project.add_layer(name, kind, 16).unwrap();
        }
        for index in 4..10 {
            project
                .add_layer(&format!("Layer_{index}"), "IntGrid", 16)
                .unwrap();
        }
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        for key in [
            Key::F2,
            Key::F3,
            Key::F4,
            Key::F5,
            Key::F6,
            Key::F7,
            Key::F8,
            Key::F9,
            Key::F10,
            Key::F1,
        ] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            let output = frame(&mut app, &ctx, vec![]);
            let name = text(
                app.project
                    .as_ref()
                    .unwrap()
                    .layer(app.selected.unwrap(), app.layer)
                    .unwrap(),
                "__identifier",
                "",
            );
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t)
                if t.galley.text()==name && (t.galley.job.sections[0].format.font_id.size-40.).abs()<0.1 && t.pos.y<80.)), "missing top layer notification for {name}");
            assert_eq!(app.project.as_ref().unwrap().json, original);
            assert!(!app.dirty);
        }
        app.layer_notification.as_mut().unwrap().2 =
            Instant::now() - std::time::Duration::from_secs(2);
        frame(&mut app, &ctx, vec![]);
        assert!(app.layer_notification.is_none());
        app.switch_layer(app.layer);
        assert!(
            app.layer_notification.is_none(),
            "selecting the current layer must not repeat the notification"
        );
    }
}
