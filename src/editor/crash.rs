use super::*;

impl EditorApp {
    pub(super) fn capture_crash(&mut self, message: String, fallback: PathBuf) {
        // A second unwind must not overwrite the first recovery or repeat IO.
        if self.crash.is_some() {
            return;
        }
        self.crash = Some((message.clone(), None));
        let needs_recovery = self.dirty || self.stroke || self.gesture.is_some();
        let auto_error = (!self.settle_deferred_auto_paint()).then(|| self.status.clone());
        self.gesture = None;
        self.level_resize = None;
        self.clear_stroke_controls();
        self.pending = None;
        self.panel = None;
        self.command_palette = false;
        self.point_pick = None;
        self.reference_pick = None;
        self.chain_entity = None;
        self.hook_flow = None;
        self.last_chance = None;
        if let Some(cancel) = self.command_cancel.take() {
            let _ = cancel.send(());
        }
        self.command_cancelled = true;
        self.command_open = false;
        self.command_result = None;
        // Ignore future completions: they must not replace the captured document
        // or resume a save-hook continuation after the crash.
        self.loading = None;
        self.loading_progress = None;
        self.saving = None;
        self.clear_save_progress();
        self.exporting = None;
        self.exporting_progress = None;
        self.external_changes = None;
        // Discard potentially incomplete history without reverting current JSON.
        self.history = History::default();
        self.ui_commit = ui_commit::UiCommit::default();
        self.world_context = world_context::WorldContext::default();
        let language = self.settings.language();
        let mut report = message;
        if let Some(error) = auto_error {
            report.push_str("\n\n");
            report.push_str(&language.format(
                "Deferred paint was rolled back: {error}",
                &[("error", &error)],
            ));
        }
        let mut recovery = None;
        if needs_recovery && let Some(project) = &self.project {
            self.dirty = true;
            match project.write_crash_recovery(fallback) {
                Ok(path) => {
                    let level_uid = self
                        .selected
                        .and_then(|key| project.level(key))
                        .and_then(|level| level["uid"].as_i64())
                        .unwrap_or(0);
                    self.settings.remember(&path, level_uid);
                    recovery = Some(path);
                }
                Err(error) => {
                    report.push_str("\n\n");
                    report.push_str(
                        &language
                            .format("Recovery failed: {error}", &[("error", &error.to_string())]),
                    );
                }
            }
        }
        self.status = "The editor encountered an unexpected error".into();
        self.status_error = true;
        self.crash = Some((report, recovery));
    }

    pub(super) fn crash_dialog(&mut self, ctx: &egui::Context) {
        let Some((message, recovery)) = &self.crash else {
            return;
        };
        let mut close = false;
        let mut keep = false;
        let language = self.settings.language();
        let tr = |text| language.translate(text);
        egui::Modal::new(egui::Id::new("crash-recovery")).show(ctx, |ui| {
            ui.set_width(600.0);
            ui.heading(tr("The editor encountered an unexpected error"));
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                ui.monospace(message);
            });
            ui.separator();
            if let Some(path) = recovery {
                ui.label(tr("Your current work was saved in this recovery project:"));
                ui.monospace(path.display().to_string());
                if ui.button(tr("Copy recovery path")).clicked() {
                    ctx.copy_text(path.to_string_lossy().into_owned());
                }
            } else if self.dirty {
                ui.label(tr("A recovery file could not be written. Your project is still in memory."));
            } else {
                ui.label(tr("There were no unsaved edits to recover."));
            }
            ui.label(tr("Restarting is recommended. Keeping this window may encounter the same error; undo history has been reset."));
            ui.horizontal(|ui| {
                close = ui.button(tr("Close app")).clicked();
                keep = ui.button(tr("Keep window")).clicked();
            });
        });
        if close {
            // Bypass ordinary dirty-save hooks after an explicit crash-page exit.
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if keep {
            self.crash = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn recovery_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100., 780.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest()),
        )
    }

    fn visible_text(output: &egui::FullOutput, value: &str) -> Option<Rect> {
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == value => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
    }

    #[test]
    fn chinese_crash_recovery_copies_the_exact_recovery_path() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = EditorApp::empty();
        app.settings.value["language"] = json!("zh-CN");
        app.project = Some(Project::new());
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        locale::set(&ctx, app.settings.language());
        app.capture_crash(
            "raw panic details".into(),
            directory.path().join("Recovery folder"),
        );
        let recovery = app
            .crash
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        recovery_frame(&mut app, &ctx, vec![]);
        let dialog = recovery_frame(&mut app, &ctx, vec![]);

        for label in [
            "编辑器遇到意外错误",
            "当前编辑已保存到以下恢复工程",
            "raw panic details",
            recovery.as_str(),
            "退出应用",
            "保留窗口",
        ] {
            assert!(
                visible_text(&dialog, label).is_some(),
                "missing label {label}"
            );
        }
        let pos = visible_text(&dialog, "复制恢复路径").unwrap().center();
        recovery_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        let clicked = recovery_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(clicked.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == &recovery)
        ));
    }

    #[test]
    fn chinese_crash_recovery_distinguishes_failed_writes_from_clean_projects() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("not-a-directory");
        std::fs::write(&blocked, "keep").unwrap();
        for (dirty, expected) in [
            (true, "无法写入恢复文件。工程仍保留在内存中。"),
            (false, "没有需要恢复的未保存修改。"),
        ] {
            let mut app = EditorApp::empty();
            app.settings.value["language"] = json!("zh-CN");
            app.project = Some(Project::new());
            app.dirty = dirty;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            locale::set(&ctx, app.settings.language());
            app.capture_crash("raw panic details".into(), blocked.clone());
            recovery_frame(&mut app, &ctx, vec![]);
            let dialog = recovery_frame(&mut app, &ctx, vec![]);
            assert!(
                visible_text(&dialog, expected).is_some(),
                "missing label {expected}"
            );
            assert!(visible_text(&dialog, "复制恢复路径").is_none());
            let (details, path) = app.crash.as_ref().unwrap();
            assert!(path.is_none() && details.starts_with("raw panic details"));
            assert_eq!(details.contains("恢复文件写入失败"), dirty);
        }
    }

    #[test]
    fn crash_saves_dirty_memory_registers_recovery_and_stops_interaction_once() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("game.ldtk");
        let mut project = Project::new();
        project.save(&source).unwrap();
        let disk = std::fs::read(&source).unwrap();
        let mut app = EditorApp::empty();
        app.settings.path = Some(dir.path().join("settings.json"));
        app.project = Some(project);
        app.history.begin_project(app.project.as_ref().unwrap());
        app.project.as_mut().unwrap().json["levels"][0]["identifier"] = json!("UnsavedAtCrash");
        app.dirty = true;
        app.stroke = true;
        app.last_cell = Some([3, 4]);
        app.pending = Some(Action::Quit);
        app.gesture = Some(tools::Gesture::Rectangle {
            start: [0, 0],
            end: [2, 2],
            erase: false,
            select: false,
            entered_level: true,
            started: Instant::now(),
            world_end: [32.0, 32.0],
        });
        let (cancel, cancelled) = mpsc::channel();
        app.command_cancel = Some(cancel);
        app.command_open = true;
        app.capture_crash("drawing failed".into(), dir.path().join("fallback"));
        let (message, path) = app.crash.as_ref().unwrap();
        assert!(message.contains("drawing failed"));
        let path = path.clone().unwrap();
        assert_eq!(
            Project::load(&path).unwrap().json["levels"][0]["identifier"],
            "UnsavedAtCrash"
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "UnsavedAtCrash"
        );
        assert_eq!(std::fs::read(&source).unwrap(), disk);
        assert!(app.dirty);
        assert!(app.gesture.is_none() && !app.stroke && app.last_cell.is_none());
        assert!(app.pending.is_none() && !app.command_open && app.command_cancel.is_none());
        assert!(cancelled.try_recv().is_ok());
        let settings: Value =
            serde_json::from_slice(&std::fs::read(app.settings.path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(
            settings["recentProjects"][0],
            path.to_string_lossy().as_ref()
        );
        app.capture_crash("second panic".into(), dir.path().join("fallback"));
        assert_eq!(
            app.project.as_ref().unwrap().list_backups().unwrap().len(),
            1
        );
    }

    #[test]
    fn in_progress_gesture_recovers_memory_before_dirty_history_commit() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = EditorApp::empty();
        let key = LevelKey {
            world: None,
            level: 0,
        };
        let mut project = Project::new();
        project.add_layer("Entities", "Entities", 16).unwrap();
        let uid = project.add_entity_def("Actor", 16, 16).unwrap();
        project.add_entity(key, 0, uid, 0, 0).unwrap();
        app.history.begin(&project, key);
        project.layer_mut(key, 0).unwrap()["entityInstances"][0]["px"] = json!([48, 0]);
        app.project = Some(project);
        app.gesture = Some(tools::Gesture::EntityMove {
            key,
            layer: 0,
            index: 0,
            press: egui::Pos2::ZERO,
            start: [0.0, 0.0],
            end: [0.0, 0.0],
            origin: [0, 0],
            moved: true,
        });
        assert!(!app.dirty);
        app.capture_crash("moving failed".into(), dir.path().to_path_buf());
        let path = app.crash.as_ref().unwrap().1.as_ref().unwrap();
        assert_eq!(
            Project::load(path).unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"],
            json!([48, 0])
        );
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"],
            json!([48, 0])
        );
        assert!(app.dirty && app.gesture.is_none());
    }

    #[test]
    fn clean_crash_and_normal_dirty_drop_do_not_create_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("game.ldtk");
        let mut project = Project::new();
        project.save(&source).unwrap();
        {
            let mut app = EditorApp::empty();
            app.project = Some(project.clone());
            app.capture_crash("clean crash".into(), dir.path().join("fallback"));
            assert!(app.crash.as_ref().unwrap().1.is_none());
            assert!(project.list_backups().unwrap().is_empty());
        }
        {
            let mut app = EditorApp::empty();
            app.settings.path = Some(dir.path().join("settings.json"));
            app.project = Some(project.clone());
            app.project.as_mut().unwrap().json["levels"][0]["identifier"] =
                json!("DiscardNormally");
            app.dirty = true;
        }
        assert!(project.list_backups().unwrap().is_empty());
        assert!(!dir.path().join("crash-recovery").exists());
        assert!(!dir.path().join("fallback").exists());
        assert_ne!(
            Project::load(source).unwrap().json["levels"][0]["identifier"],
            "DiscardNormally"
        );
    }

    #[test]
    fn panic_unwind_drop_preserves_dirty_memory_in_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("game.ldtk");
        let mut project = Project::new();
        project.save(&source).unwrap();
        let saved = project.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut app = EditorApp::empty();
            app.settings.path = Some(dir.path().join("settings.json"));
            app.project = Some(project);
            app.project.as_mut().unwrap().json["levels"][0]["identifier"] = json!("Unwinding");
            app.dirty = true;
            panic!("simulated native failure outside update");
        }));
        assert!(result.is_err());
        let backups = saved.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].crash);
        assert_eq!(
            Project::load(&backups[0].path).unwrap().json["levels"][0]["identifier"],
            "Unwinding"
        );
        assert_ne!(
            Project::load(source).unwrap().json["levels"][0]["identifier"],
            "Unwinding"
        );
    }

    #[test]
    fn recovery_write_error_is_shown_without_losing_dirty_project() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, "keep").unwrap();
        let mut app = EditorApp::empty();
        app.project = Some(Project::new());
        app.dirty = true;
        app.capture_crash("original fault".into(), blocked.clone());
        let (message, path) = app.crash.as_ref().unwrap();
        assert!(message.contains("original fault") && message.contains("Recovery failed"));
        assert!(path.is_none() && app.project.is_some() && app.dirty);
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "keep");
    }
}
