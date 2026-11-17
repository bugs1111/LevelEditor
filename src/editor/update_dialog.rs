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
                        ui.heading("Installing update…");
                        ui.label("LDtk will restart when installation is ready.");
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
                format!(
                    "Downloading update: {}%",
                    if *total == 0 { 0 } else { done * 100 / total }
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
                .anchor(Align2::RIGHT_TOP, egui::vec2(0., 15.))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let title = version.as_ref().map_or(label.clone(), |version| {
                            format!("{label}\nVersion {version}")
                        });
                        if super::panels::source_controls::button(ui, title).clicked() {
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
                            && super::panels::source_controls::button(ui, "×").clicked()
                        {
                            dialog.ignored = version;
                        }
                    });
                });
        }
        if dialog.open {
            let mut close = false;
            let modal=egui::Modal::new(id().with("dialog")).show(ctx,|ui| {
                ui.set_width(460.);ui.set_max_width(460.);
                ui.heading("LevelEditor updates");
                ui.label(format!("Installed version {}",env!("CARGO_PKG_VERSION")));
                match &snapshot {
                    State::Idle=>{ui.label("Check for a published LevelEditor release.");},
                    State::Checking=>{ui.horizontal(|ui|{ui.spinner();ui.label("Looking for update…");});if super::panels::source_controls::button(ui,"Cancel check").clicked(){dialog.updater.cancel();}},
                    State::Current(version)=>{ui.label(format!("App is up-to-date ({version})."));},
                    State::Available(release)=>{
                        ui.label(format!("Update {} is available.",release.version));
                        if !release.notes.is_empty() {egui::ScrollArea::vertical().max_height(180.).show(ui,|ui|{ui.label(&release.notes);});}
                        ui.hyperlink_to("Release notes",&release.page);
                        if super::panels::source_controls::button(ui,"Download update").clicked() {download(&mut dialog,ctx);}
                    },
                    State::Downloading{version,done,total}=>{
                        ui.label(format!("Downloading update {version}…"));
                        ui.add(egui::ProgressBar::new(if *total==0 {0.}else{*done as f32/ *total as f32}).show_percentage());
                        ui.label(format!("{:.1} / {:.1} MiB",*done as f64/1_048_576.,*total as f64/1_048_576.));
                        if super::panels::source_controls::button(ui,"Cancel download").clicked() {dialog.updater.cancel();}
                    },
                    State::Ready(prepared)=>{
                        ui.label(format!("Update {} downloaded and SHA-256 verified.",prepared.version));
                        if super::panels::source_controls::button(ui,"Install update and restart").clicked() {install=true;}
                    },
                    State::Cancelled=>{ui.label("Update cancelled. Nothing was installed.");},
                    State::Failed(error)=>{ui.colored_label(Color32::LIGHT_RED,error);},
                    State::Installing|State::Restarting=>{},
                }
                ui.separator();
                let mut automatic=self.settings.bool("autoInstallUpdates");
                if super::panels::source_controls::checkbox(ui,&mut automatic,"Automatically download updates").changed() {
                    self.settings.value["autoInstallUpdates"]=serde_json::json!(automatic);let _=self.settings.save();
                }
                ui.label("This repository is private. A token needs Contents: read access. It is used only for this session and is never saved in settings.");
                ui.add(egui::TextEdit::singleline(&mut dialog.token).id(id().with("session-token")).password(true).hint_text("Session token, or GH_TOKEN / GITHUB_TOKEN"));
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!dialog.updater.busy(),|ui|{if super::panels::source_controls::button(ui,"Check for updates").clicked(){check(&mut dialog,ctx);}});
                    ui.hyperlink_to("Open Releases",updater::RELEASES_URL);
                    close=super::panels::source_controls::button(ui,"Close").clicked();
                });
            });
            if close || modal.should_close() {
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
                    app.dialogs(ctx);
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
            app.dialogs(ctx);
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
