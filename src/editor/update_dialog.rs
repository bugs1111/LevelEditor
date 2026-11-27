use super::panels::source_controls;
use super::*;
use crate::updater::{self, State, Updater};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Dialog {
    updater: Updater,
    open: bool,
    token: String,
    ignored: Option<String>,
}

fn id() -> egui::Id {
    egui::Id::new("native-rust-updater")
}
fn state(ctx: &egui::Context) -> Arc<Mutex<Dialog>> {
    ctx.data_mut(|data| {
        if let Some(state) = data.get_temp::<Arc<Mutex<Dialog>>>(id()) {
            state
        } else {
            let state = Arc::new(Mutex::new(Dialog::default()));
            data.insert_temp(id(), state.clone());
            state
        }
    })
}
pub(super) fn settings_update_version(ctx: &egui::Context) -> Option<String> {
    let shared = state(ctx);
    let dialog = shared.lock().unwrap_or_else(|error| error.into_inner());
    match &dialog.updater.state {
        State::Available(release) => Some(release.version.clone()),
        State::Ready(prepared) => Some(prepared.version.clone()),
        _ => None,
    }
}
pub(super) fn is_open(ctx: &egui::Context) -> bool {
    state(ctx)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .open
}
fn token(dialog: &Dialog) -> Option<String> {
    if dialog.token.trim().is_empty() {
        updater::environment_token()
    } else {
        Some(dialog.token.trim().to_owned())
    }
}
fn check(dialog: &mut Dialog, ctx: &egui::Context) {
    let token = token(dialog);
    let ctx = ctx.clone();
    dialog.updater.check(token, move || ctx.request_repaint());
}
fn download(dialog: &mut Dialog, ctx: &egui::Context) {
    let token = token(dialog);
    let ctx = ctx.clone();
    dialog
        .updater
        .download(token, move || ctx.request_repaint());
}

fn update_button(ui: &mut egui::Ui, button: egui::Button<'_>, primary: bool) -> egui::Response {
    ui.scope(|ui| {
        let (fill, hover) = if primary {
            (
                Color32::from_rgb(227, 77, 0),
                Color32::from_rgb(255, 93, 74),
            )
        } else {
            (theme::LIGHT, Color32::from_rgb(115, 127, 157))
        };
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.weak_bg_fill = fill;
        widgets.hovered.weak_bg_fill = hover;
        widgets.active.weak_bg_fill = hover;
        source_controls::add_sized_button(ui, [0., 32.], button.corner_radius(2))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    })
    .inner
}

fn action_button(ui: &mut egui::Ui, label: &str, primary: bool) -> egui::Response {
    let label = RichText::new(locale::tr(ui, label)).color(Color32::WHITE);
    update_button(ui, egui::Button::new(label), primary)
}

impl EditorApp {
    pub(super) fn shutdown_updater(&self, ctx: &egui::Context) {
        state(ctx)
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .updater
            .shutdown();
    }
    pub(super) fn initialize_updater(&mut self, ctx: &egui::Context) {
        let state = state(ctx);
        let mut dialog = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(error) = updater::prior_install_failure() {
            dialog.updater.state = State::Failed(error);
            dialog.open = true;
        } else {
            check(&mut dialog, ctx);
        }
    }
    pub(super) fn open_updater(&mut self, ctx: &egui::Context) {
        let state = state(ctx);
        let mut dialog = state.lock().unwrap_or_else(|e| e.into_inner());
        dialog.open = true;
        dialog.ignored = None;
        if matches!(
            dialog.updater.state,
            State::Idle | State::Current(_) | State::Cancelled | State::Failed(_)
        ) {
            check(&mut dialog, ctx);
        }
    }
    pub(super) fn update_installing(&self, ctx: &egui::Context) -> bool {
        let state = state(ctx);
        let dialog = state.lock().unwrap_or_else(|e| e.into_inner());
        matches!(dialog.updater.state, State::Installing | State::Restarting)
    }
    pub(super) fn install_pending_update(&mut self, ctx: &egui::Context) {
        let state = state(ctx);
        let mut dialog = state.lock().unwrap_or_else(|e| e.into_inner());
        let reopen = self.project.as_ref().and_then(|p| p.path.clone());
        let wake = ctx.clone();
        dialog
            .updater
            .install(reopen, move || wake.request_repaint());
    }
    pub(super) fn update_dialog(&mut self, ctx: &egui::Context) {
        let state = state(ctx);
        let mut dialog = state.lock().unwrap_or_else(|e| e.into_inner());
        dialog.updater.poll();
        if self.settings.bool("autoInstallUpdates")
            && matches!(dialog.updater.state, State::Available(_))
        {
            download(&mut dialog, ctx);
        }
        let snapshot = dialog.updater.state.clone();
        if matches!(snapshot, State::Installing | State::Restarting) {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(theme::DARK))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.4).max(20.));
                        ui.spinner();
                        ui.heading(locale::tr(ui, "Installing update…"));
                        ui.label(locale::tr(
                            ui,
                            "Level Editor will restart when installation is ready.",
                        ));
                    });
                });
            if matches!(snapshot, State::Restarting) {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        let mut install = false;
        let banner = match &snapshot {
            State::Checking => Some(("Checking for update…".to_owned(), None)),
            State::Available(release) if dialog.ignored.as_deref() != Some(&release.version) => {
                Some(("Download update".into(), Some(release.version.clone())))
            }
            State::Downloading {
                version,
                done,
                total,
            } => Some((
                locale::format_ctx(
                    ctx,
                    "Downloading update: {percent}%",
                    &[(
                        "percent",
                        &(if *total == 0 { 0 } else { done * 100 / total }).to_string(),
                    )],
                ),
                Some(version.clone()),
            )),
            State::Ready(prepared) if dialog.ignored.as_deref() != Some(&prepared.version) => {
                Some((
                    "Install update and restart".into(),
                    Some(prepared.version.clone()),
                ))
            }
            State::Failed(_) => Some(("Could not update · Details".into(), None)),
            _ => None,
        };
        if let Some((label, version)) = banner {
            egui::Area::new(id().with("banner"))
                .anchor(Align2::RIGHT_TOP, egui::vec2(-10., 12.))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let label = locale::tr(ui, &label);
                        let title = version.as_ref().map_or_else(
                            || label.to_owned(),
                            |version| {
                                locale::format(
                                    ui,
                                    "{label}\nVersion {version}",
                                    &[("label", label), ("version", version)],
                                )
                            },
                        );
                        let (icon, color) = if matches!(snapshot, State::Failed(_)) {
                            (
                                egui::include_image!("../../assets/icons/warn.svg"),
                                theme::ACCENT,
                            )
                        } else {
                            (
                                egui::include_image!("../../assets/icons/appUpdate.svg"),
                                Color32::WHITE,
                            )
                        };
                        let icon = egui::Image::new(icon)
                            .fit_to_exact_size(Vec2::splat(18.))
                            .tint(color);
                        let title = RichText::new(title)
                            .font(FontId::new(14., egui::FontFamily::Name("body-bold".into())))
                            .color(color);
                        if update_button(ui, egui::Button::image_and_text(icon, title), false)
                            .clicked()
                        {
                            match snapshot {
                                State::Available(_) => {
                                    download(&mut dialog, ctx);
                                    dialog.open = true;
                                }
                                State::Ready(_) => install = true,
                                _ => dialog.open = true,
                            }
                        }
                        if version.is_some()
                            && !dialog.updater.busy()
                            && action_button(ui, "×", false).clicked()
                        {
                            dialog.ignored = version;
                        }
                    });
                });
        }
        if dialog.open {
            let screen = ctx.content_rect();
            let width = (screen.width() - 24.).clamp(120., 520.);
            let modal = egui::Modal::new(id().with("dialog"))
                .frame(project_dialogs::dialog_frame(0).stroke(Stroke::new(1_f32, theme::LIGHT)))
                .backdrop_color(Color32::from_black_alpha(140))
                .show(ctx, |ui| {
                    ui.set_width(width);
                    ui.set_max_height((screen.height() - 24.).max(0.));
                    project_dialogs::dialog_title(ui, "Level Editor updates", width, theme::LIGHT);
                    ui.spacing_mut().scroll = source_controls::scroll_style();
                    let visuals = source_controls::scroll_visuals(ui);
                    egui::ScrollArea::vertical()
                        .min_scrolled_height(0.)
                        .show(ui, |ui| {
                            *ui.visuals_mut() = visuals;
                            egui::Frame::new()
                                .inner_margin(16)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    self.update_dialog_body(
                                        ui,
                                        &mut dialog,
                                        &snapshot,
                                        &mut install,
                                    )
                                })
                                .inner
                        })
                        .inner
                });
            if modal.inner || modal.should_close() {
                dialog.open = false;
            }
        }
        if dialog.updater.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if install {
            dialog.open = false;
        }
        drop(dialog);
        if install {
            self.request(ctx, Action::InstallUpdate);
        }
    }

    fn update_dialog_body(
        &mut self,
        ui: &mut egui::Ui,
        dialog: &mut Dialog,
        snapshot: &State,
        install: &mut bool,
    ) -> bool {
        let ctx = ui.ctx().clone();
        let version = locale::format(
            ui,
            "Installed version {version}",
            &[("version", env!("CARGO_PKG_VERSION"))],
        );
        ui.label(RichText::new(version).small());
        ui.add_space(4.);
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match snapshot {
                    State::Idle => {
                        ui.label(locale::tr(
                            ui,
                            "Check for a published Level Editor release.",
                        ));
                    }
                    State::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(locale::tr(ui, "Looking for update…"));
                        });
                    }
                    State::Current(version) => {
                        ui.label(locale::format(
                            ui,
                            "App is up-to-date ({version}).",
                            &[("version", version)],
                        ));
                    }
                    State::Available(release) => {
                        ui.label(locale::format(
                            ui,
                            "Update {version} is available.",
                            &[("version", &release.version)],
                        ));
                        if !release.notes.is_empty() {
                            egui::ScrollArea::vertical()
                                .max_height(120.)
                                .show(ui, |ui| {
                                    ui.label(&release.notes);
                                });
                        }
                        source_controls::hyperlink(ui, "Release notes", &release.page);
                    }
                    State::Downloading {
                        version,
                        done,
                        total,
                    } => {
                        ui.label(locale::format(
                            ui,
                            "Downloading update {version}…",
                            &[("version", version)],
                        ));
                        let progress = if *total == 0 {
                            0.
                        } else {
                            *done as f32 / *total as f32
                        };
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .fill(theme::ACCENT)
                                .show_percentage(),
                        );
                        ui.label(format!(
                            "{:.1} / {:.1} MiB",
                            *done as f64 / 1_048_576.,
                            *total as f64 / 1_048_576.
                        ));
                    }
                    State::Ready(prepared) => {
                        ui.label(locale::format(
                            ui,
                            "Update {version} downloaded and SHA-256 verified.",
                            &[("version", &prepared.version)],
                        ));
                    }
                    State::Cancelled => {
                        ui.label(locale::tr(ui, "Update cancelled. Nothing was installed."));
                    }
                    State::Failed(error) => {
                        ui.horizontal_top(|ui| {
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/icons/warn.svg"
                                ))
                                .fit_to_exact_size(Vec2::splat(18.))
                                .tint(theme::ACCENT),
                            );
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(locale::tr(ui, "Update failed"))
                                        .strong()
                                        .color(theme::ACCENT),
                                );
                                ui.add(egui::Label::new(error).wrap().selectable(true));
                            });
                        });
                    }
                    State::Installing | State::Restarting => {}
                }
            });

        ui.add_space(6.);
        let mut automatic = self.settings.bool("autoInstallUpdates");
        if source_controls::checkbox(ui, &mut automatic, "Automatically download updates").changed()
        {
            self.settings.value["autoInstallUpdates"] = serde_json::json!(automatic);
            let _ = self.settings.save();
        }
        ui.label(RichText::new(locale::tr(ui, "If authentication is required, use a token with Contents read access. The token is used only for this session.")).small());
        ui.add(
            egui::TextEdit::singleline(&mut dialog.token)
                .id(id().with("session-token"))
                .desired_width(f32::INFINITY)
                .password(true)
                .hint_text(locale::tr(ui, "Session token, or GH_TOKEN / GITHUB_TOKEN")),
        );
        source_controls::hyperlink(ui, "Open Releases", updater::RELEASES_URL);
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            match snapshot {
                State::Available(_) if action_button(ui, "Download update", true).clicked() => {
                    download(dialog, &ctx);
                }
                State::Ready(_)
                    if action_button(ui, "Install update and restart", true).clicked() =>
                {
                    *install = true;
                }
                State::Checking if action_button(ui, "Cancel check", false).clicked() => {
                    dialog.updater.cancel();
                }
                State::Downloading { .. }
                    if action_button(ui, "Cancel download", false).clicked() =>
                {
                    dialog.updater.cancel();
                }
                _ => {}
            }
            let failed = matches!(snapshot, State::Failed(_));
            ui.add_enabled_ui(!dialog.updater.busy(), |ui| {
                let label = if failed { "Retry" } else { "Check for updates" };
                let primary = !matches!(snapshot, State::Available(_) | State::Ready(_));
                if action_button(ui, label, primary).clicked() {
                    check(dialog, &ctx);
                }
            });
            action_button(ui, "Close", false).clicked()
        })
        .inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escape_closes_updater_without_closing_parent_settings() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.show_panel(Some(Panel::Settings));
        let shared = state(&ctx);
        {
            let mut dialog = shared.lock().unwrap();
            dialog.open = true;
            dialog.updater.state = State::Current("0.1.1".into());
        }
        for escape in [false, false, false, true] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    focused: true,
                    events: if escape {
                        vec![egui::Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        }]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    app.dialogs(ctx, false);
                    app.update_dialog(ctx);
                },
            );
        }
        assert!(!shared.lock().unwrap().open);
        assert_eq!(app.panel, Some(Panel::Settings));
    }
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200., 800.),
            )),
            events,
            ..Default::default()
        };
        app.ui_commit_raw_input(ctx, &mut input);
        ctx.run(input, |ctx| {
            app.dialogs(ctx, false);
            app.update_dialog(ctx);
            app.flush_ui_action(ctx);
        })
    }
    fn center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {label}"))
    }
    fn click(app: &mut EditorApp, ctx: &egui::Context, position: egui::Pos2) -> egui::FullOutput {
        frame(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    }
    #[test]
    fn releases_link_uses_a_shared_underline_and_opens_the_release_page() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        {
            let shared = state(&ctx);
            let mut dialog = shared.lock().unwrap();
            dialog.open = true;
            dialog.updater.state = State::Current("0.1.1".into());
        }
        let _ = frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let link = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Open Releases" => Some(text),
                _ => None,
            })
            .expect("release link");
        assert_eq!(link.underline, Stroke::new(1_f32, theme::ACCENT));
        let clicked = click(&mut app, &ctx, link.pos + link.galley.size() * 0.5);
        assert!(clicked.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::OpenUrl(url) if url.url == updater::RELEASES_URL)
        }));
    }

    #[test]
    fn failed_banner_opens_error_details_and_can_retry_or_close() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let shared = state(&ctx);
        let error = "Fixture: release lookup failed";
        shared.lock().unwrap().updater = Updater::failed_fixture(error);

        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let banner = center(&output, "Could not update · Details");
        let hovered = frame(&mut app, &ctx, vec![egui::Event::PointerMoved(banner)]);
        assert_eq!(
            hovered.platform_output.cursor_icon,
            egui::CursorIcon::PointingHand
        );
        click(&mut app, &ctx, banner);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(is_open(&ctx));
        center(&output, error);
        click(&mut app, &ctx, center(&output, "Close"));
        assert!(!is_open(&ctx));

        frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, banner);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, center(&output, "Retry"));
        let output = frame(&mut app, &ctx, vec![]);
        center(
            &output,
            &format!("App is up-to-date ({}).", env!("CARGO_PKG_VERSION")),
        );
        click(&mut app, &ctx, center(&output, "Close"));
        assert!(!is_open(&ctx));
    }
    #[test]
    fn install_closes_its_dialog_before_showing_clickable_unsaved_cancel_and_discard() {
        for choice in ["CANCEL", "NO", "YES"] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            let dir = tempfile::tempdir().unwrap();
            let mut project = Project::new();
            project.save(dir.path().join("project.ldtk")).unwrap();
            app.repaint = Some(ctx.clone());
            app.set_project(project);
            app.dirty = true;
            let shared = state(&ctx);
            {
                let mut d = shared.lock().unwrap();
                d.open = true;
                d.updater = Updater::ready_fixture();
            }
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            click(
                &mut app,
                &ctx,
                center(&output, "Install update and restart"),
            );
            assert!(matches!(app.pending, Some(Action::InstallUpdate)));
            assert!(
                !shared.lock().unwrap().open,
                "the update modal must close before the unsaved modal owns input"
            );
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            click(&mut app, &ctx, center(&output, choice));
            if choice == "YES" {
                for _ in 0..200 {
                    app.poll_hooks(&ctx);
                    app.poll_services(&ctx);
                    if shared.lock().unwrap().updater.test_install_requests == 1 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            assert!(
                app.pending.is_none(),
                "{choice} must receive the actual click"
            );
            assert_eq!(
                shared.lock().unwrap().updater.test_install_requests,
                usize::from(choice != "CANCEL")
            );
        }
    }
    #[test]
    fn clicking_install_from_a_focused_field_commits_its_draft_before_the_unsaved_prompt() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let actor = p.add_entity_def("Actor", 16, 16).unwrap();
        let uid = p
            .add_field_definition(Some(actor), "Text", "String", false)
            .unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, actor, 32, 32).unwrap();
        let iid = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_editor_value(key, Some(&iid), uid, serde_json::json!("old"))
            .unwrap();
        app.set_project(p);
        app.entity_index = Some(0);
        app.panel = Some(Panel::Entity);
        app.dirty = false;
        let shared = state(&ctx);
        shared.lock().unwrap().updater = Updater::ready_fixture();
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, center(&output, "old"));
        let command = egui::Modifiers {
            command: true,
            mac_cmd: cfg!(target_os = "macos"),
            ctrl: !cfg!(target_os = "macos"),
            ..Default::default()
        };
        let output = frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
                egui::Event::Text("draft".into()),
            ],
        );
        click(
            &mut app,
            &ctx,
            center(&output, "Install update and restart\nVersion 1.1.0"),
        );
        for _ in 0..5 {
            frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["fieldInstances"]
                [0]["__value"],
            "draft"
        );
        assert!(matches!(app.pending, Some(Action::InstallUpdate)));
        assert_eq!(shared.lock().unwrap().updater.test_install_requests, 0);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, center(&output, "CANCEL"));
        assert!(app.pending.is_none());
    }
    #[test]
    fn session_token_is_password_masked_and_never_written_to_settings() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let shared = state(&ctx);
        {
            let mut d = shared.lock().unwrap();
            d.open = true;
            d.updater.state = State::Current("0.1.0".into());
        }
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
        ctx.memory_mut(|m| m.request_focus(id().with("session-token")));
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("fixture-session-token".into())],
        );
        assert_eq!(shared.lock().unwrap().token, "fixture-session-token");
        assert!(
            !app.settings
                .value
                .to_string()
                .contains("fixture-session-token")
        );
        assert!(!output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.text().contains("fixture-session-token"))));
    }
}
