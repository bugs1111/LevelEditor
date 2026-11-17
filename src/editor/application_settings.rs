//! Application settings, following the upstream EditAppSettings dialog.
use super::*;

const SCALES: [f64; 18] = [
    0.5, 0.6, 0.7, 0.8, 0.9, 1., 1.1, 1.2, 1.3, 1.4, 1.5, 2., 2.5, 3., 3.5, 4., 4.5, 5.,
];
const WORLD_SWITCH: [(&str, &str); 3] = [
    ("Never", "Never"),
    ("ZoomOutOnly", "Switch when zooming out"),
    ("ZoomInAndOut", "Switch when zooming in or out (default)"),
];

#[derive(Clone, Default)]
struct Session {
    changed: bool,
    restart: bool,
    save_failed: bool,
}
fn session_id() -> egui::Id {
    egui::Id::new("app-settings-session")
}

fn row(ui: &mut egui::Ui, label: &str, help: &str, width: f32, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.;
        source_controls::property_label_nowrap(ui, label, help, width);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 30.),
            egui::Layout::left_to_right(egui::Align::Center),
            body,
        );
    });
}
fn checkbox(ui: &mut egui::Ui, values: &Value, patch: &mut Value, key: &str) {
    let mut value = values[key].as_bool().unwrap_or(false);
    if source_controls::checkbox(ui, &mut value, "").changed() {
        patch[key] = json!(value);
    }
}
fn choice(
    ui: &mut egui::Ui,
    values: &Value,
    patch: &mut Value,
    key: &str,
    options: &[(&str, &str)],
) {
    let current = text(values, key, options[0].0);
    let mut selected = current.to_owned();
    egui::ComboBox::from_id_salt(key)
        .width(ui.available_width())
        .truncate()
        .selected_text(
            options
                .iter()
                .find(|(value, _)| *value == current)
                .map_or(current, |(_, label)| *label),
        )
        .show_ui(ui, |ui| {
            for (value, label) in options {
                ui.selectable_value(&mut selected, (*value).into(), *label);
            }
        });
    if selected != current {
        patch[key] = json!(selected);
    }
}
fn number_choice(
    ui: &mut egui::Ui,
    values: &Value,
    patch: &mut Value,
    key: &str,
    options: &[f64],
    label: impl Fn(f64) -> String,
) {
    let current = values[key].as_f64().unwrap_or(1.);
    let mut selected = current;
    egui::ComboBox::from_id_salt(key)
        .width(ui.available_width())
        .truncate()
        .selected_text(label(current))
        .show_ui(ui, |ui| {
            for value in options {
                ui.selectable_value(&mut selected, *value, label(*value));
            }
        });
    if selected != current {
        patch[key] = json!(selected);
    }
}
fn scale_label(value: f64) -> String {
    format!(
        "{}%{}",
        (value * 100.).round() as i32,
        if value == 1. { " (default)" } else { "" }
    )
}
fn heading(ui: &mut egui::Ui, name: &str) {
    ui.add_space(16.);
    let response = ui.label(
        RichText::new(name.to_uppercase())
            .color(theme::ACCENT)
            .font(FontId::new(
                17.333,
                egui::FontFamily::Name("panel-title".into()),
            )),
    );
    ui.painter().hline(
        ui.max_rect().x_range(),
        response.rect.bottom() + 2.,
        Stroke::new(1_f32, theme::ACCENT),
    );
    ui.add_space(8.);
}

impl EditorApp {
    pub(in crate::app) fn show_settings_dialog(&mut self, ctx: &egui::Context) {
        let width = 980_f32.min((ctx.content_rect().width() - 42.).max(260.));
        let frame = egui::Frame::new()
            .fill(theme::MEDIUM)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .shadow(egui::epaint::Shadow {
                offset: [0, 8],
                blur: 32,
                spread: 0,
                color: Color32::from_black_alpha(153),
            });
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("app-settings-dialog"))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(8., 10.);
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(12.667));
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Button, FontId::proportional(12.667));
                let (header, _) = ui.allocate_exact_size(egui::vec2(width, 53.), Sense::hover());
                ui.painter().rect_filled(header, 0., theme::LIGHT);
                egui::Image::new(egui::include_image!("../../assets/icons/settings.svg")).paint_at(
                    ui,
                    Rect::from_min_size(header.min + egui::vec2(20., 6.5), Vec2::splat(40.)),
                );
                ui.painter().text(
                    header.min + egui::vec2(72., 26.5),
                    Align2::LEFT_CENTER,
                    "APPLICATION SETTINGS",
                    FontId::new(20., egui::FontFamily::Name("panel-title".into())),
                    Color32::WHITE,
                );
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 20,
                        right: 20,
                        top: 6,
                        bottom: 20,
                    })
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .id_salt("app-settings-body")
                            .max_height((ctx.content_rect().height() - 173.).max(80.))
                            .max_width(width - 40.)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(940.);
                                self.settings_properties(ui);
                            });
                        ui.add_space(6.);
                        close = source_controls::add_sized_button(
                            ui,
                            [width - 40., 42.],
                            egui::Button::new(RichText::new("CLOSE").color(Color32::WHITE))
                                .fill(Color32::from_rgb(235, 78, 0)),
                        )
                        .clicked();
                    });
            });
        if close || modal.should_close() {
            self.show_panel(None);
        }
    }

    pub(super) fn settings_properties(&mut self, ui: &mut egui::Ui) {
        let values = self.settings.value.clone();
        let mut patch = json!({});
        // Source dl.form columns size to the longest non-wrapping label plus its help badge.
        let label_width = |label: &str| {
            ui.painter()
                .layout_no_wrap(label.into(), FontId::proportional(12.667), theme::TEXT)
                .size()
                .x
                + 32.
        };
        let left_width = label_width("\"Nearby levels\" tiles rendering distance")
            .max(label_width("Fade LDtk window when not focused"));
        let right_width = label_width("World view with mouse wheel");
        if let Some(version) = update_dialog::settings_update_version(ui.ctx()) {
            if source_controls::add_sized_button(
                ui,
                [940., 64.],
                egui::Button::image_and_text(
                    egui::Image::new(egui::include_image!("../../assets/icons/appUpdate.svg"))
                        .fit_to_exact_size(Vec2::splat(32.)),
                    RichText::new(format!("A new update is available!\nInstall {version}"))
                        .color(Color32::WHITE),
                )
                .fill(Color32::from_rgb(44, 128, 179)),
            )
            .clicked()
            {
                self.show_panel(None);
                self.open_updater(ui.ctx());
            }
            ui.add_space(22.);
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.allocate_ui_with_layout(egui::vec2(380.,0.), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(380.);
                heading(ui,"General");
                row(ui,"Re-open last project","openLastProject",left_width, |ui| checkbox(ui,&values,&mut patch,"openLastProject"));
                heading(ui,"Visuals");
                for (key,label,help) in [("appUiScale","General UI scale","appScale"),("editorUiScale","Level UI scale","fontScale")] {
                    row(ui,label,help,left_width, |ui| number_choice(ui,&values,&mut patch,key,&SCALES,scale_label));
                }
                row(ui,"Entity fields aspect","fieldsRender",left_width, |ui| choice(ui,&values,&mut patch,"fieldsRender",&[("FR_Outline","Outlined texts (default)"),("FR_Table","Opaque tables")]));
                row(ui,"\"Nearby levels\" tiles rendering distance","nearbyTilesRenderingDist",left_width, |ui| number_choice(ui,&values,&mut patch,"nearbyTilesRenderingDist",&[0.,1.,1.5,2.], |value| if value==0. {"Disabled".into()} else {format!("{} pixels",(value*64.).ceil() as i32)}));
                row(ui,"Single-layer mode intensity","singleLayerModeIntensity",left_width, |ui| number_choice(ui,&values,&mut patch,"singleLayerModeIntensity",&[0.,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.], |value| format!("{}%",(value*100.).round() as i32)));
                for (key,label) in [("startFullScreen","Start in fullscreen mode"),("colorBlind","Prefer color-blind colors"),("blurMask","Fade LDtk window when not focused")] {
                    row(ui,label,key,left_width, |ui| checkbox(ui,&values,&mut patch,key));
                }
            });
            ui.add_space(32.);
            ui.allocate_ui_with_layout(egui::vec2(528.,0.), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(528.);
                heading(ui,"Controls");
                row(ui,"Mouse wheel speed","mouseWheelSpeed",right_width, |ui| {
                    let percent = (self.settings.float("mouseWheelSpeed",1.) * 100.).round() as i32;
                    if let Some(value) = ui_fields::buffered_text(ui,"mouseWheelSpeed",&percent.to_string())
                        && let Ok(value) = value.trim().trim_end_matches('%').parse::<f64>()
                        && value.is_finite() { patch["mouseWheelSpeed"] = json!(value.clamp(25.,300.)/100.); }
                    ui.label("%");
                });
                row(ui,"World view with mouse wheel","autoSwitchOnZoom",right_width, |ui| choice(ui,&values,&mut patch,"autoWorldModeSwitch",&WORLD_SWITCH));
                row(ui,"Navigation keys layout","navKeys",right_width, |ui| choice(ui,&values,&mut patch,"navigationKeys",&[("Wasd","WASD"),("Zqsd","ZQSD"),("Arrows","ARROWS")]));
                heading(ui,"Advanced options");
                row(ui,"Try to use best GPU","Try to use best GPU",right_width, |ui| { checkbox(ui,&values,&mut patch,"useBestGPU"); ui.label("(Recommended)"); });
                row(ui,"Auto update","nativeAutoDownloadUpdates",right_width, |ui| {
                    let response = ui.scope(|ui| checkbox(ui,&values,&mut patch,"autoInstallUpdates")).response;
                    response.on_hover_text("Automatically download updates on supported native platforms. Installation still requires confirmation.");
                    ui.label("(Recommended)");
                    if ui.small_button("Check for updates").clicked() { self.open_updater(ui.ctx()); }
                });
                row(ui,"Log file","Log file",right_width, |ui| {
                    if let Some(path) = app_log::path(self.settings.path.as_deref()) {
                        let path_width = (ui.available_width()-76.).max(40.);
                        let mut display = path.display().to_string();
                        ui.add_sized([path_width,30.], egui::TextEdit::singleline(&mut display).interactive(false));
                        text_editor::log_editor(ui,&path);
                        ui.scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(4.,2.);
                            if source_controls::add_sized_button(ui,[30.,30.],egui::Button::image(
                                egui::Image::new(egui::include_image!("../../assets/icons/locate.svg")).fit_to_exact_size(Vec2::splat(16.))))
                                .on_hover_text("Locate log file").clicked()
                                && let Err(error)=home_files::locate(&path,true) { self.status=error.to_string(); self.status_error=true; }
                        });
                    } else { ui.label("No log file in this session"); }
                });
            });
        });
        if changed(&patch) {
            apply_patch(&mut self.settings.value, &patch);
            if patch["openLastProject"] == false {
                self.settings.value["lastProject"] = Value::Null;
            } else if patch["openLastProject"] == true {
                self.remember_selected_level();
            }
            if patch.get("appUiScale").is_some() {
                ui.ctx()
                    .set_zoom_factor(self.settings.float("appUiScale", 1.));
            }
            if let Some(fullscreen) = patch["startFullScreen"].as_bool() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            }
            let mut session = ui
                .ctx()
                .data_mut(|data| data.get_temp::<Session>(session_id()))
                .unwrap_or_default();
            session.restart |= patch.get("useBestGPU").is_some();
            match self.settings.save() {
                Ok(()) => {
                    session.changed = true;
                    session.save_failed = false;
                }
                Err(error) => {
                    session.save_failed = true;
                    self.status = error.to_string();
                    self.status_error = true;
                }
            }
            ui.ctx()
                .data_mut(|data| data.insert_temp(session_id(), session));
        }
    }

    pub(in crate::app) fn finish_settings_dialog(&mut self, ctx: &egui::Context) {
        if let Some(session) = ctx.data_mut(|data| data.remove_temp::<Session>(session_id()))
            && session.changed
            && !session.save_failed
        {
            self.status = if session.restart {
                "Saved. You need to RESTART the app to apply your changes."
            } else {
                "Settings saved."
            }
            .into();
            self.status_error = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        frame_size(app, ctx, events, egui::vec2(1440., 900.))
    }
    fn frame_size(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: Vec2,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
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
        eframe::App::raw_input_hook(app, ctx, &mut input);
        let output = ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest())
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
        output
    }
    fn label(output: &egui::FullOutput, name: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .rev()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == name => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible setting: {name}"))
    }
    fn checkbox_pos(output: &egui::FullOutput, name: &str) -> egui::Pos2 {
        badge_pos(output, name) + egui::vec2(32., 0.)
    }
    fn badge_pos(output: &egui::FullOutput, name: &str) -> egui::Pos2 {
        let label = label(output, name);
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "?" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .filter(|pos| pos.x > label.x && (pos.y - label.y).abs() < 2.)
            .min_by(|a, b| a.x.total_cmp(&b.x))
            .unwrap()
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
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    fn setup() -> (EditorApp, egui::Context) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app.show_panel(Some(Panel::Settings));
        for _ in 0..12 {
            frame(&mut app, &ctx, vec![]);
        }
        (app, ctx)
    }

    #[test]
    fn source_app_settings_has_original_two_columns_and_groups() {
        let (mut app, ctx) = setup();
        let original = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "\"Nearby levels\" tiles rendering distance");
        label(&output, "World view with mouse wheel");
        let general = label(&output, "GENERAL");
        let visuals = label(&output, "VISUALS");
        let controls = label(&output, "CONTROLS");
        let advanced = label(&output, "ADVANCED OPTIONS");
        assert!(controls.x > general.x + 380.);
        assert!((controls.y - general.y).abs() < 1.);
        assert!(
            visuals.y > general.y && advanced.y > controls.y,
            "general={general:?}, visuals={visuals:?}, controls={controls:?}, advanced={advanced:?}"
        );
        let rect = egui::AreaState::load(&ctx, egui::Id::new("app-settings-dialog"))
            .unwrap()
            .rect();
        assert!((rect.width() - 982.).abs() < 1., "{rect:?}");
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn source_app_settings_headings_have_original_caps_color_font_and_rules() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "APPLICATION SETTINGS");
        for name in ["GENERAL", "VISUALS", "CONTROLS", "ADVANCED OPTIONS"] {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => Some(text),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing source heading {name}"));
            assert_eq!(
                text.galley.job.sections[0].format.color,
                Color32::from_rgb(255, 204, 0)
            );
            assert_eq!(
                text.galley.job.sections[0].format.font_id,
                FontId::new(17.333, egui::FontFamily::Name("panel-title".into()))
            );
            let bottom = text.pos.y + text.galley.size().y;
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::LineSegment {points,stroke} if stroke.color==Color32::from_rgb(255,204,0) && points[0].y>=bottom && points[0].y<=bottom+4. && (points[1].x-points[0].x).abs()>=380.)));
        }
    }

    #[test]
    fn source_app_settings_restores_friendly_choices_and_percentage_selectors() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        for name in [
            "Outlined texts (default)",
            "Switch when zooming in or out (default)",
            "WASD",
            "64 pixels",
        ] {
            label(&output, name);
        }
        let row = label(&output, "General UI scale");
        let scale = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text() == "100% (default)"
                        && (text.pos.y + text.galley.size().y * 0.5 - row.y).abs() < 5. =>
                {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        click(&mut app, &ctx, scale);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "80%"));
        assert_eq!(app.settings.float("appUiScale", 1.), 0.8);
    }

    #[test]
    fn source_app_settings_wheel_percent_is_clamped_and_saved_as_a_factor() {
        let (mut app, ctx) = setup();
        let dir = tempfile::tempdir().unwrap();
        app.settings.path = Some(dir.path().join("settings.json"));
        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, "100");
        click(&mut app, &ctx, pos);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        frame(&mut app, &ctx, vec![egui::Event::Text("500".into())]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(app.settings.float("mouseWheelSpeed", 1.), 3.);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(app.settings.path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(saved["mouseWheelSpeed"], 3.);
    }

    #[test]
    fn source_app_settings_disabling_reopen_clears_the_saved_project() {
        let (mut app, ctx) = setup();
        app.settings.value["openLastProject"] = json!(true);
        app.settings.value["lastProject"] = json!({"filePath":"old.ldtk","levelUid":1});
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            checkbox_pos(&output, "Re-open last project"),
        );
        assert!(!app.settings.bool("openLastProject"));
        assert!(app.settings.value["lastProject"].is_null());
    }

    #[test]
    fn source_app_settings_fullscreen_and_gpu_have_their_real_effects() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            checkbox_pos(&output, "Start in fullscreen mode"),
        );
        assert!(app.settings.bool("startFullScreen"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, checkbox_pos(&output, "Try to use best GPU"));
        assert!(!app.settings.bool("useBestGPU"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "CLOSE"));
        frame(&mut app, &ctx, vec![]);
        assert!(app.panel.is_none());
        assert!(app.status.contains("RESTART"));
        assert!(!app.dirty);
    }

    #[test]
    fn source_app_settings_close_commits_wheel_draft_before_hiding_the_form() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "100"));
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        frame(&mut app, &ctx, vec![egui::Event::Text("125".into())]);
        assert_eq!(app.settings.float("mouseWheelSpeed", 1.), 1.);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "CLOSE"));
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        assert!(app.panel.is_none());
        assert_eq!(app.settings.float("mouseWheelSpeed", 1.), 1.25);
    }

    #[test]
    fn source_app_settings_help_describes_gpu_and_native_update_behavior() {
        for (name, expected) in [
            ("Try to use best GPU", "extra battery life"),
            ("Auto update", "Installation still requires confirmation"),
        ] {
            let (mut app, ctx) = setup();
            ctx.style_mut(|style| {
                style.interaction.tooltip_delay = 0.;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let output = frame(&mut app, &ctx, vec![]);
            let pos = badge_pos(&output, name);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
            for _ in 0..4 {
                frame(&mut app, &ctx, vec![]);
            }
            let output = frame(&mut app, &ctx, vec![]);
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text().contains(expected))),"wrong help for {name}");
        }
    }

    #[test]
    fn source_app_settings_close_preserves_a_failed_save_error() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            checkbox_pos(&output, "Re-open last project"),
        );
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("file-in-place-of-directory");
        std::fs::write(&blocked, "blocked").unwrap();
        app.settings.path = Some(blocked.join("settings.json"));
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            checkbox_pos(&output, "Prefer color-blind colors"),
        );
        assert!(app.status_error);
        let error = app.status.clone();
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "CLOSE"));
        assert!(app.status_error);
        assert_eq!(app.status, error);
    }

    #[test]
    fn source_app_settings_project_transitions_finish_the_settings_session() {
        for close in [true, false] {
            let (mut app, ctx) = setup();
            let output = frame(&mut app, &ctx, vec![]);
            click(&mut app, &ctx, checkbox_pos(&output, "Try to use best GPU"));
            assert!(!app.settings.bool("useBestGPU"));
            if close {
                app.request(&ctx, Action::Close);
            } else {
                app.set_project(Project::new());
            }
            assert!(app.panel.is_none());
            assert!(app.status.contains("RESTART"));
            app.status = "A later message".into();
            app.show_panel(Some(Panel::Settings));
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            click(&mut app, &ctx, label(&output, "CLOSE"));
            assert_eq!(app.status, "A later message");
        }
    }

    #[test]
    fn source_app_settings_remains_inside_a_small_home_window() {
        for size in [egui::vec2(800., 500.), egui::vec2(460., 340.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            app.show_panel(Some(Panel::Settings));
            for _ in 0..16 {
                frame_size(&mut app, &ctx, vec![], size);
            }
            let rect = egui::AreaState::load(&ctx, egui::Id::new("app-settings-dialog"))
                .unwrap()
                .rect();
            assert!(
                rect.left() >= 0.
                    && rect.top() >= 0.
                    && rect.right() <= size.x
                    && rect.bottom() <= size.y,
                "{size:?}: {rect:?}"
            );
        }
    }
}
