//! Application settings, following the upstream EditAppSettings dialog.
use super::*;
use crate::app::chrome::ActionTooltip as _;
use source_controls::{
    FormSelectAppearance, form_heading as heading, form_row as row, form_select,
};

const SCALES: [f64; 18] = [
    0.5, 0.6, 0.7, 0.8, 0.9, 1., 1.1, 1.2, 1.3, 1.4, 1.5, 2., 2.5, 3., 3.5, 4., 4.5, 5.,
];
const WORLD_SWITCH: [(&str, &str); 3] = [
    ("Never", "Never"),
    ("ZoomOutOnly", "Switch when zooming out"),
    ("ZoomInAndOut", "Switch when zooming in or out (default)"),
];
const LANGUAGES: [(&str, &str); 3] = [
    ("system", "Follow system"),
    ("en", "English"),
    ("zh-CN", "简体中文"),
];
const SINGLE_LAYER_INTENSITIES: &[f64] = &[0., 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.];
const NEARBY_TILE_DISTANCES: &[f64] = &[0., 1., 1.5, 2.];
const SINGLE_LAYER_DEFAULT_PRESET: f64 = 0.9;
const NEARBY_TILE_DEFAULT_PRESET: f64 = 1.;

#[derive(Clone, Default)]
struct Session {
    changed: bool,
    restart: bool,
    save_failed: bool,
}
fn session_id() -> egui::Id {
    egui::Id::new("app-settings-session")
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
    let selected = options.iter().position(|(value, _)| *value == current);
    let rows: Vec<_> = options
        .iter()
        .map(|(_, label)| (locale::tr(ui, label), None))
        .collect();
    if let Some(index) = form_select(
        ui,
        key,
        30.,
        &rows,
        selected,
        rows[selected.unwrap_or(0)].0,
        FormSelectAppearance::Normal,
    ) && options[index].0 != current
    {
        patch[key] = json!(options[index].0);
    }
}
fn normalize_preset(values: &mut Value, key: &str, options: &[f64], default: f64) -> f64 {
    let current = values[key].as_f64().unwrap_or(default);
    let Some(preset) = options.iter().copied().find(|value| *value >= current) else {
        return default;
    };
    if preset != current {
        values[key] = json!(preset);
    }
    preset
}

fn number_choice(
    ui: &mut egui::Ui,
    patch: &mut Value,
    key: &str,
    options: &[f64],
    current: f64,
    default: f64,
    label: impl Fn(f64) -> String,
) {
    let default_suffix = locale::tr(ui, " (default)");
    let choice_label = |value| {
        let mut text = label(value);
        if value == default {
            text.push_str(default_suffix);
        }
        text
    };
    let labels: Vec<_> = options.iter().map(|&value| choice_label(value)).collect();
    let rows: Vec<_> = labels.iter().map(|label| (label.as_str(), None)).collect();
    if let Some(index) = form_select(
        ui,
        key,
        30.,
        &rows,
        options.iter().position(|&value| value == current),
        &choice_label(current),
        FormSelectAppearance::Normal,
    ) && options[index] != current
    {
        patch[key] = json!(options[index]);
    }
}
fn percent_label(value: f64) -> String {
    format!("{}%", (value * 100.).round() as i32)
}
impl EditorApp {
    pub(in crate::app) fn show_settings_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("app-settings-dialog");
        let height_id = id.with("content-height");
        let content_height = ctx.data_mut(|data| data.get_temp::<f32>(height_id));
        let screen_height = ctx.content_rect().height();
        // The original switches to top-aligned scrolling when the form itself
        // reaches the window height. Its footer is part of the scroll content.
        let force_scroll = screen_height <= content_height.unwrap_or(590.);
        let width = 980.;
        let mut area = egui::Modal::default_area(id)
            .default_width(width + 2.)
            .constrain(false);
        if force_scroll {
            area = area.anchor(egui::Align2::CENTER_TOP, Vec2::ZERO);
        }
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
        let modal = egui::Modal::new(id)
            .area(area)
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_max_width(if force_scroll { width + 10. } else { width });
                ui.set_max_height(if force_scroll {
                    (screen_height - 2.).max(0.)
                } else {
                    f32::INFINITY
                });
                ui.spacing_mut().scroll = source_controls::scroll_style();
                let content_visuals = source_controls::scroll_visuals(ui);
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("app-settings-scroll")
                    .auto_shrink([true, true])
                    .scroll_source(egui::scroll_area::ScrollSource {
                        scroll_bar: true,
                        drag: false,
                        mouse_wheel: true,
                    });
                if !force_scroll || content_height.is_none() {
                    scroll = scroll.vertical_scroll_offset(0.);
                }
                scroll
                    .show(ui, |ui| {
                        *ui.visuals_mut() = content_visuals;
                        ui.set_width(width);
                        let top = ui.next_widget_position().y;
                        ui.spacing_mut().item_spacing = egui::vec2(8., 10.);
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Body, FontId::proportional(12.667));
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Button, FontId::proportional(12.667));
                        let (header, _) =
                            ui.allocate_exact_size(egui::vec2(width, 53.), Sense::hover());
                        ui.painter().rect_filled(header, 0., theme::LIGHT);
                        egui::Image::new(egui::include_image!("../../assets/icons/settings.svg"))
                            .paint_at(
                                ui,
                                Rect::from_min_size(
                                    header.min + egui::vec2(20., 6.5),
                                    Vec2::splat(40.),
                                ),
                            );
                        ui.painter().text(
                            header.min + egui::vec2(72., 26.5),
                            Align2::LEFT_CENTER,
                            locale::tr(ui, "APPLICATION SETTINGS"),
                            theme::TextRole::DialogTitle.font(),
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
                                ui.set_width(width - 40.);
                                self.settings_properties(ui);
                                let content_height = ui.next_widget_position().y - top;
                                ui.add_space(6.);
                                close = source_controls::add_sized_button(
                                    ui,
                                    [width - 40., 42.],
                                    egui::Button::new(
                                        RichText::new(locale::tr(ui, "CLOSE"))
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(235, 78, 0)),
                                )
                                .clicked();
                                content_height
                            })
                            .inner
                    })
                    .inner
            });
        if close || modal.should_close() {
            self.show_panel(None);
        } else if self.panel == Some(Panel::Settings) {
            ctx.data_mut(|data| data.insert_temp(height_id, modal.inner));
            if force_scroll != (screen_height <= modal.inner) {
                ctx.request_repaint();
            }
        }
    }

    pub(super) fn settings_properties(&mut self, ui: &mut egui::Ui) {
        // Opening the source form rounds historical values up in memory.
        // The ordinary settings save path persists them with the next edit.
        let nearby_distance = normalize_preset(
            &mut self.settings.value,
            "nearbyTilesRenderingDist",
            NEARBY_TILE_DISTANCES,
            NEARBY_TILE_DEFAULT_PRESET,
        );
        let single_layer_intensity = normalize_preset(
            &mut self.settings.value,
            "singleLayerModeIntensity",
            SINGLE_LAYER_INTENSITIES,
            SINGLE_LAYER_DEFAULT_PRESET,
        );
        let values = self.settings.value.clone();
        let ctx = ui.ctx().clone();
        let mut patch = json!({});
        // Source dl.form columns size to the longest non-wrapping label plus its help badge.
        let label_width = |label: &str| {
            ui.painter()
                .layout_no_wrap(
                    locale::tr(ui, label).into(),
                    FontId::proportional(12.667),
                    theme::TEXT,
                )
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
                    RichText::new(locale::format(
                        ui,
                        "A new update is available!\nInstall {version}",
                        &[("version", &version)],
                    ))
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
                row(ui,"Language","language",left_width, |ui| choice(ui,&values,&mut patch,"language",&LANGUAGES));
                row(ui,"Re-open last project","openLastProject",left_width, |ui| checkbox(ui,&values,&mut patch,"openLastProject"));
                heading(ui,"Visuals");
                for (key,label,help) in [("appUiScale","General UI scale","appScale"),("editorUiScale","Level UI scale","fontScale")] {
                    row(ui,label,help,left_width, |ui| number_choice(ui,&mut patch,key,&SCALES,values[key].as_f64().unwrap_or(1.),1.,percent_label));
                }
                row(ui,"Entity fields aspect","fieldsRender",left_width, |ui| choice(ui,&values,&mut patch,"fieldsRender",&[("FR_Outline","Outlined texts (default)"),("FR_Table","Opaque tables")]));
                row(ui,"\"Nearby levels\" tiles rendering distance","nearbyTilesRenderingDist",left_width, |ui| {
                    number_choice(ui,&mut patch,"nearbyTilesRenderingDist",NEARBY_TILE_DISTANCES,nearby_distance,NEARBY_TILE_DEFAULT_PRESET, |value| {
                        if value==0. {
                            locale::tr_ctx(&ctx, "Disabled").into()
                        } else {
                            locale::format_ctx(&ctx, "{value} pixels", &[("value", &((value*64.).ceil() as i32).to_string())])
                        }
                    });
                });
                row(ui,"Single-layer mode intensity","singleLayerModeIntensity",left_width, |ui| {
                    number_choice(ui,&mut patch,"singleLayerModeIntensity",SINGLE_LAYER_INTENSITIES,single_layer_intensity,SINGLE_LAYER_DEFAULT_PRESET,percent_label);
                });
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
                    if let Some(value) = source_controls::number_input_sized(
                        ui,
                        "mouseWheelSpeed",
                        &percent.to_string(),
                        source_controls::NumberStyle::PercentSlider { min: 25., max: 300. },
                        62.,
                    )
                        && let Ok(value) = value.trim().trim_end_matches('%').parse::<f64>()
                        && value.is_finite() { patch["mouseWheelSpeed"] = json!(value.round().clamp(25.,300.)/100.); }
                    ui.label("%");
                });
                row(ui,"World view with mouse wheel","autoSwitchOnZoom",right_width, |ui| choice(ui,&values,&mut patch,"autoWorldModeSwitch",&WORLD_SWITCH));
                row(ui,"Navigation keys layout","navKeys",right_width, |ui| choice(ui,&values,&mut patch,"navigationKeys",&[("Wasd","WASD"),("Zqsd","ZQSD"),("Arrows","ARROWS")]));
                heading(ui,"Advanced options");
                row(ui,"Try to use best GPU","Try to use best GPU",right_width, |ui| { checkbox(ui,&values,&mut patch,"useBestGPU"); ui.label(locale::tr(ui, "(Recommended)")); });
                row(ui,"Auto update","nativeAutoDownloadUpdates",right_width, |ui| {
                    let response = ui.scope(|ui| checkbox(ui,&values,&mut patch,"autoInstallUpdates")).response;
                    response.action_tooltip(locale::tr(ui, "Automatically download updates on supported native platforms. Installation still requires confirmation."));
                    ui.label(locale::tr(ui, "(Recommended)"));
                    if source_controls::small_button(ui, "Check for updates").clicked() { self.open_updater(ui.ctx()); }
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
                                .action_tooltip(locale::tr(ui, "Locate log file")).clicked()
                                && let Err(error)=home_files::locate(&path,true) { self.status=error.to_string(); self.status_error=true; }
                        });
                    } else { ui.label(locale::tr(ui, "No log file in this session")); }
                });
                row(ui,"AI assistant (MCP)","mcp",right_width, |ui| {
                    let project = self.project.as_ref().and_then(|p| p.path.as_deref()).and_then(Path::to_str);
                    let executable = std::env::current_exe().ok();
                    let command = executable.as_deref().and_then(Path::to_str);
                    let response = ui.add_enabled(
                        project.is_some() && command.is_some(),
                        egui::Button::new(locale::tr(ui, "Copy MCP configuration")),
                    );
                    if response.disabled_action_tooltip(locale::tr(ui, "Open a saved project to copy its MCP configuration.")).clicked()
                        && let (Some(project), Some(command)) = (project, command)
                    {
                        let config = json!({"mcpServers":{(env!("CARGO_PKG_NAME")):{"command":command,"args":["--mcp",project]}}});
                        ui.ctx().copy_text(serde_json::to_string_pretty(&config).expect("MCP configuration is JSON"));
                        self.status = "MCP configuration copied.".into();
                        self.status_error = false;
                    }
                });
            });
        });
        if changed(&patch) {
            apply_patch(&mut self.settings.value, &patch);
            if patch.get("language").is_some() {
                locale::set(ui.ctx(), self.settings.language());
                ui.ctx().request_repaint();
            }
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
        ctx.data_mut(|data| {
            data.remove::<f32>(egui::Id::new("app-settings-dialog").with("content-height"));
        });
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
        setup_with_fonts(|_| {})
    }

    fn setup_with_fonts(configure: impl FnOnce(&egui::Context)) -> (EditorApp, egui::Context) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        configure(&ctx);
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
    fn copied_mcp_configuration_uses_the_level_editor_server_name() {
        let (mut app, ctx) = setup();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Sample Project.ldtk");
        let project = app.project.as_mut().unwrap();
        std::fs::write(&path, project.json.to_string()).unwrap();
        project.path = Some(path.clone());

        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, "Copy MCP configuration");
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos), button(true)],
        );
        let output = frame(&mut app, &ctx, vec![button(false)]);
        let [egui::OutputCommand::CopyText(text)] = output.platform_output.commands.as_slice()
        else {
            panic!("Expected the copied MCP configuration");
        };
        let config: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            config,
            json!({"mcpServers":{"level-editor":{
                "command":std::env::current_exe().unwrap().to_str().unwrap(),
                "args":["--mcp",path.to_str().unwrap()]
            }}})
        );
    }

    #[test]
    fn f12_replaces_the_context_menu_with_application_settings() {
        for world in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            app.set_project(Project::new());
            if world {
                app.show_panel(Some(Panel::World));
            }
            let project = app.project.as_ref().unwrap().json.clone();
            let pos = egui::pos2(1320., 800.);
            frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            assert!(app.world_context.menu_open());
            let key = |key| {
                [true, false]
                    .into_iter()
                    .map(|pressed| egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .collect()
            };
            frame(&mut app, &ctx, key(Key::F12));
            assert_eq!(app.panel, Some(Panel::Settings));
            assert!(!app.world_context.menu_open());
            assert!(!app.world);
            let output = frame(&mut app, &ctx, key(Key::F12));
            label(&output, "APPLICATION SETTINGS");
            frame(&mut app, &ctx, key(Key::Escape));
            assert!(app.panel.is_none());
            assert!(!app.world_context.menu_open());
            assert_eq!(app.project.as_ref().unwrap().json, project);
        }
    }

    #[test]
    fn language_selection_updates_ui_and_persists_without_changing_the_project() {
        let (mut app, ctx) = setup();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        app.settings.path = Some(path.clone());
        let project = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "English"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "简体中文"));
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "应用设置");
        label(&output, "界面语言");
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["language"], "zh-CN");
        app.settings = settings::Settings::load_at(Some(path.clone()));
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "应用设置");

        click(&mut app, &ctx, label(&output, "简体中文"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "English"));
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "APPLICATION SETTINGS");
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["language"], "en");

        click(&mut app, &ctx, label(&output, "English"));
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "Follow system"));
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, locale::tr_ctx(&ctx, "Follow system"));
        assert_eq!(
            settings::Settings::load_at(Some(path)).value["language"],
            "system"
        );
        assert_eq!(
            app.settings.language(),
            locale::Language::from_code(Some("system"))
        );
        assert_eq!(app.project.as_ref().unwrap().json, project);
        assert!(!app.dirty);
    }

    #[test]
    fn chinese_ui_translates_the_external_project_change_dialog() {
        let (mut app, ctx) = setup();
        app.show_panel(None);
        app.settings.value["language"] = json!("zh-CN");
        app.external_changes = Some(vec![PathBuf::from("地图工程.ldtk")]);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "工程文件已被外部程序修改");
        label(&output, "地图工程.ldtk");
        label(&output, "从磁盘重新加载");
        label(&output, "保留当前版本");
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
                theme::TextRole::SectionTitle.font()
            );
            let bottom = text.pos.y + text.galley.size().y;
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::LineSegment {points,stroke} if stroke.color==Color32::from_rgb(255,204,0) && points[0].y>=bottom && points[0].y<=bottom+4. && (points[1].x-points[0].x).abs()>=380.)));
        }
    }

    #[test]
    fn source_app_settings_restores_friendly_choices_and_percentage_selectors() {
        let (mut app, ctx) = setup_with_fonts(theme::use_reference_layout_fonts);
        let output = frame(&mut app, &ctx, vec![]);
        for name in [
            "Outlined texts (default)",
            "Switch when zooming in or out (default)",
            "WASD",
            "64 pixels (default)",
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
    fn app_settings_uses_the_original_presets_and_waits_for_an_edit_to_save() {
        let (mut app, ctx) = setup();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        app.settings.path = Some(path.clone());
        app.settings.value["singleLayerModeIntensity"] = json!(0.73);
        app.settings.value["nearbyTilesRenderingDist"] = json!(1.12);
        app.settings.save().unwrap();
        let saved = std::fs::read(&path).unwrap();

        let output = frame(&mut app, &ctx, vec![]);
        assert_eq!(app.settings.value["singleLayerModeIntensity"], 0.8);
        assert_eq!(app.settings.value["nearbyTilesRenderingDist"], 1.5);
        label(&output, "80%");
        label(&output, "96 pixels");
        click(&mut app, &ctx, label(&output, "CLOSE"));
        assert!(app.panel.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), saved);

        app.show_panel(Some(Panel::Settings));
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            checkbox_pos(&output, "Re-open last project"),
        );
        let saved = settings::Settings::load_at(Some(path));
        assert!(saved.bool("openLastProject"));
        assert_eq!(saved.value["singleLayerModeIntensity"], 0.8);
        assert_eq!(saved.value["nearbyTilesRenderingDist"], 1.5);
        assert!(!app.dirty);
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
    fn source_app_settings_wheel_percent_drag_commits_on_release() {
        let (mut app, ctx) = setup();
        let output = frame(&mut app, &ctx, vec![]);
        let start = label(&output, "100");
        let end = start + egui::vec2(50., 0.);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                button(start, true),
                egui::Event::PointerMoved(start + egui::vec2(10., 0.)),
            ],
        );
        let preview = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)]);
        label(&preview, "120");
        assert_eq!(app.settings.float("mouseWheelSpeed", 1.), 1.);
        frame(&mut app, &ctx, vec![button(end, false)]);
        assert_eq!(app.settings.float("mouseWheelSpeed", 1.), 1.2);
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
    fn source_app_settings_centers_fixed_columns_and_scrolls_the_entire_short_dialog() {
        let (mut app, ctx) = setup_with_fonts(theme::use_reference_layout_fonts);
        let rect = || {
            egui::AreaState::load(&ctx, egui::Id::new("app-settings-dialog"))
                .unwrap()
                .rect()
        };
        let narrow = egui::vec2(800., 600.);
        for _ in 0..8 {
            frame_size(&mut app, &ctx, vec![], narrow);
        }
        let centered = rect();
        assert!((centered.width() - 982.).abs() < 1., "{centered:?}");
        assert!(centered.left() < 0. && centered.right() > narrow.x);
        assert!((centered.center().x - narrow.x / 2.).abs() < 1.);
        assert!((centered.center().y - narrow.y / 2.).abs() < 1.);

        let short = egui::vec2(1050., 500.);
        for _ in 0..8 {
            frame_size(&mut app, &ctx, vec![], short);
        }
        assert!(rect().top().abs() < 1., "{:?}", rect());
        let before = frame_size(&mut app, &ctx, vec![], short);
        let controls = label(&before, "CONTROLS");
        frame_size(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(egui::pos2(430., 230.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -400.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            short,
        );
        for _ in 0..12 {
            frame_size(&mut app, &ctx, vec![], short);
        }
        let scrolled = frame_size(&mut app, &ctx, vec![], short);
        assert!(label(&scrolled, "ADVANCED OPTIONS").y < controls.y + 120.);
        let close = label(&scrolled, "CLOSE");
        assert!(close.y > 0. && close.y < short.y);
        for pressed in [true, false] {
            frame_size(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: close,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                short,
            );
        }
        assert!(app.panel.is_none());
    }
}
