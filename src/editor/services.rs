use super::*;

pub(super) struct LoadedProject {
    pub project: Project,
    pub migrated: bool,
    pub warnings: Vec<String>,
    pub backup_original: Option<PathBuf>,
}

pub(super) struct SavedProject {
    pub project: Project,
    pub report: ldtk_core::SaveReport,
}

pub(super) enum SaveFailure {
    Write { path: PathBuf, message: String },
    MissingTarget(PathBuf),
}

pub(super) fn prepare_project(
    mut project: Project,
    warnings: Vec<String>,
) -> Result<LoadedProject, String> {
    let version = text(&project.json, "jsonVersion", "0.0");
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let modern = (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) >= (1, 5);
    let cached = modern
        && project.level_keys().iter().all(|&key| {
            project
                .level(key)
                .and_then(|l| l["layerInstances"].as_array())
                .is_some_and(|layers| {
                    layers
                        .iter()
                        .filter(|l| matches!(l["__type"].as_str(), Some("IntGrid" | "AutoLayer")))
                        .all(|l| l["autoLayerTiles"].is_array())
                })
        });
    let migrated = project.migrate_to_current().map_err(|e| e.to_string())?;
    if !cached {
        for key in project.level_keys() {
            project
                .rebuild_auto_layers(key)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(LoadedProject {
        project,
        migrated,
        warnings,
        backup_original: None,
    })
}

impl EditorApp {
    pub(super) fn create_project_async(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.is_busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.status = "Creating project…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut project = Project::new();
                project.save(&path).map_err(|e| e.to_string())?;
                Ok(LoadedProject {
                    project,
                    migrated: false,
                    warnings: Vec::new(),
                    backup_original: None,
                })
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn is_busy(&self) -> bool {
        self.loading.is_some()
            || self.saving.is_some()
            || self.holding_save_completion()
            || self.save_failure.is_some()
            || self.application_write.is_some()
            || self.exporting.is_some()
            || self.hook_flow.is_some()
            || matches!(self.layer_bake, Some(bake_dialog::LayerBake::Running(_)))
            || self.rebuilding_auto_layers.is_some()
    }

    pub(super) fn poll_services(&mut self, ctx: &egui::Context) {
        self.poll_layer_bake();
        self.poll_auto_layer_rebuild();
        // Keep completed load progress visible for the frame that installs the project.
        if self.loading.is_none() {
            self.loading_progress = None;
        }
        if let Some(result) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            match result {
                Ok(loaded) => {
                    if self.loading_progress.is_some() {
                        ctx.request_repaint();
                    }
                    if let Some(original) = loaded.backup_original {
                        let iid = text(&loaded.project.json, "iid", "").to_owned();
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new(("backup-original", iid)), original)
                        });
                    }
                    self.set_project(loaded.project);
                    self.dirty = loaded.migrated;
                    if loaded.migrated
                        && let Some(project) = &self.project
                    {
                        for key in project.level_keys() {
                            if let Some(level) = project.level(key) {
                                self.history.mark_level_modified(level);
                            }
                        }
                    }
                    self.resource_scan(ctx);
                    self.queue_project_hooks(ctx, "AfterLoad", hooks::Continuation::None);
                    if !loaded.warnings.is_empty() {
                        self.status = loaded.warnings.join("; ");
                        self.status_error = true;
                    }
                }
                Err(e) => {
                    self.loading_progress = None;
                    self.status = format!("Open failed: {e}");
                    self.status_error = true;
                }
            }
        }
        if let Some(result) = self.saving.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.saving = None;
            match result {
                Ok(saved) => {
                    for diagnostic in &saved.report.export_diagnostics {
                        app_log::record(ctx, self.settings.path.as_deref(), diagnostic, true);
                    }
                    let changed_resources = self.changed_resource_files();
                    self.discard_unused_images(&saved.project);
                    self.project = Some(saved.project);
                    self.dirty = false;
                    self.history.clear_modified_levels();
                    self.file_snapshot = self
                        .project
                        .as_ref()
                        .and_then(|p| p.filesystem_snapshot().ok());
                    self.status = if saved.report.export_errors.is_empty() {
                        format!("Saved {} files", saved.report.outputs.len())
                    } else {
                        format!("Project saved; {}", saved.report.export_errors.join("; "))
                    };
                    self.status_error = !saved.report.export_errors.is_empty();
                    if let Some(progress) = &self.saving_progress {
                        progress
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .finish(Instant::now());
                    } else {
                        self.save_attempt = None;
                    }
                    if !changed_resources.is_empty() {
                        self.changed_resources(ctx, changed_resources);
                    }
                    if let Some(project) = &self.project
                        && let Some(path) = &project.path
                    {
                        let uid = self
                            .selected
                            .and_then(|k| project.level(k))
                            .and_then(|l| l["uid"].as_i64())
                            .unwrap_or(0);
                        self.settings.remember(path, uid);
                    }
                    self.queue_project_hooks(ctx, "AfterSave", hooks::Continuation::AfterSave);
                }
                Err(e) => {
                    self.saving_progress = None;
                    self.status = format!("Save failed: {e}");
                    self.status_error = true;
                    if let Some(path) = self.save_attempt.take() {
                        let folder = path.with_extension("");
                        let message = match &e {
                            ldtk_core::Error::Io { path: failed, .. }
                                if folder != path
                                    && failed.starts_with(&folder)
                                    && folder.is_file() =>
                            {
                                format!(
                                    "I need to create a folder named \"{}\", but there is a file with the exact same name there.",
                                    folder
                                        .file_name()
                                        .unwrap_or(folder.as_os_str())
                                        .to_string_lossy()
                                )
                            }
                            _ => e.to_string(),
                        };
                        self.save_failure = Some(SaveFailure::Write { path, message });
                    }
                }
            }
        }
        self.animate_save_progress(ctx, Instant::now());
        if let Some(result) = self.exporting.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.exporting = None;
            self.exporting_progress = None;
            match result {
                Ok(report) => {
                    self.status = format!(
                        "Exported {} files{}",
                        report.outputs.len(),
                        if report.warnings.is_empty() {
                            String::new()
                        } else {
                            format!("; {}", report.warnings.join("; "))
                        }
                    );
                    self.status_error = !report.warnings.is_empty();
                }
                Err(e) => {
                    self.status = format!("Export failed: {e}");
                    self.status_error = true;
                }
            }
        }
        if self.save_failure.is_none()
            && self.application_write.is_none()
            && (self.is_busy() || self.project.is_some() && !self.capture_ready())
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.pump_resources(ctx);
        if self.last_file_check.elapsed() > std::time::Duration::from_secs(2)
            && !self.is_busy()
            && self.pending.is_none()
            && !self.stroke
            && self.gesture.is_none()
            && self.level_resize.is_none()
            && !world_form_dialog::is_open(ctx)
        {
            self.last_file_check = Instant::now();
            if let Some(snapshot) = &self.file_snapshot
                && let Ok(paths) = snapshot.changed_files()
                && !paths.is_empty()
            {
                self.changed_resources(ctx, paths);
            }
        }
    }

    pub(super) fn start_auto_layer_rebuild(&mut self, ctx: &egui::Context) {
        if self.is_busy() || !self.settle_deferred_auto_paint() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        let Some(mut project) = self.project.clone() else {
            return;
        };
        let keys = project.level_keys();
        let progress = Arc::new(Mutex::new(Some((0, keys.len()))));
        let written = progress.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.rebuilding_auto_layers = Some(rx);
        self.auto_layer_rebuild_progress = Some(progress);
        self.pending_auto_layer_rebuild = false;
        self.status = "Updating auto-layers…".into();
        self.status_error = false;
        std::thread::spawn(move || {
            let result = (|| {
                for (i, &key) in keys.iter().enumerate() {
                    project.rebuild_auto_layers(key)?;
                    *written.lock().unwrap_or_else(|e| e.into_inner()) = Some((i + 1, keys.len()));
                    ctx.request_repaint();
                }
                Ok(project)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_auto_layer_rebuild(&mut self) {
        let Some(job) = &self.rebuilding_auto_layers else {
            return;
        };
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(ldtk_core::Error::Invalid(
                "Auto-layer rebuild was interrupted.".into(),
            )),
        };
        self.rebuilding_auto_layers = None;
        self.auto_layer_rebuild_progress = None;
        match result {
            Ok(project) => {
                self.pending_auto_layer_rebuild = self
                    .edit("Auto-layers rebuilt", |p| {
                        *p = project;
                        Ok(())
                    })
                    .is_none();
            }
            Err(error) => {
                self.pending_auto_layer_rebuild = true;
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }

    pub(super) fn export_async(&mut self, format: &str, dir: PathBuf) {
        if self.is_busy() || !self.settle_deferred_auto_paint() {
            return;
        }
        self.finish_stroke();
        let Some(project) = self.project.clone() else {
            return;
        };
        let format = format.to_owned();
        let (tx, rx) = mpsc::channel();
        self.exporting = Some(rx);
        let progress = Arc::new(Mutex::new(None));
        self.exporting_progress = Some(progress.clone());
        self.status = "Exporting…".into();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let mut report_progress = |done, total| {
                *progress.lock().unwrap_or_else(|e| e.into_inner()) =
                    (total > 0).then_some((done, total));
                if let Some(ctx) = &repaint {
                    ctx.request_repaint();
                }
            };
            let result = if format == "tmx" {
                project.export_tiled_with_report_progress(&dir, &mut report_progress)
            } else {
                match format.as_str() {
                    "simple" => project.export_simplified_with_progress(&dir, &mut report_progress),
                    "png" => project.export_png_with_progress(&dir, &mut report_progress),
                    "csv" => project.export_csv_with_progress(&dir, &mut report_progress),
                    _ => project.export_gamemaker_with_progress(&dir, &mut report_progress),
                }
                .map(|outputs| ldtk_core::exports::ExportReport {
                    outputs,
                    warnings: Vec::new(),
                })
            }
            .map_err(|e| e.to_string());
            let _ = tx.send(result);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
    }

    pub(super) fn import_ogmo_async(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.status = "Importing Ogmo project…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = Project::import_ogmo_with_report(path)
                .and_then(|mut report| {
                    let path = report
                        .project
                        .path
                        .clone()
                        .expect("Ogmo importer supplies output path");
                    let output = available_import_path(&path);
                    report.project.save(&output)?;
                    Ok(LoadedProject {
                        project: report.project,
                        migrated: false,
                        warnings: report.warnings,
                        backup_original: None,
                    })
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn browse_ogmo(&mut self, ctx: &egui::Context) {
        let mut dialog = FileDialog::new().add_filter("Ogmo project", &["ogmo"]);
        if let Some(dir) = self.settings.ui_dir(None, "ImportOgmo", None) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            if let Some(dir) = path.parent() {
                self.settings.store_ui_dir(None, "ImportOgmo", dir);
            }
            self.import_ogmo_async(ctx, path);
        }
    }

    pub(super) fn backup_async(
        &mut self,
        ctx: &egui::Context,
        backup: PathBuf,
        original: PathBuf,
        restore: bool,
    ) {
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.status = if restore {
            "Restoring backup…"
        } else {
            "Opening backup…"
        }
        .into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let mut project = if restore {
                    Project::restore_backup(&backup, &original)
                } else {
                    Project::load_backup(&backup, &original)
                }
                .map_err(|e| e.to_string())?;
                let migrated = project.migrate_to_current().map_err(|e| e.to_string())?;
                for key in project.level_keys() {
                    project
                        .rebuild_auto_layers(key)
                        .map_err(|e| e.to_string())?;
                }
                Ok(LoadedProject {
                    project,
                    migrated,
                    warnings: Vec::new(),
                    backup_original: Some(original),
                })
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn rename_async(&mut self, ctx: &egui::Context, name: String) {
        let Some(mut project) = self.project.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.saving = Some(rx);
        self.status = "Renaming project…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = project
                .rename_project(&name)
                .map(|report| SavedProject { project, report });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn change_project_extension(&mut self, use_ldtk: bool) {
        if self.is_busy() {
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let old = project.path.clone();
        match project.rename_project_extension(use_ldtk, self.file_snapshot.as_mut()) {
            Ok(true) => {
                let path = project.path.as_ref().expect("renamed project path");
                let uid = self
                    .selected
                    .and_then(|key| project.level(key))
                    .and_then(|level| level["uid"].as_i64())
                    .unwrap_or(0);
                if let Some(old) = old {
                    let name = old.to_string_lossy();
                    self.settings.value["recentProjects"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|p| p.as_str() != Some(name.as_ref()));
                }
                self.settings.remember(path, uid);
                let extension = if use_ldtk { ".ldtk" } else { ".json" };
                self.status = format!("Changed file extension to {extension}");
                self.status_error = false;
            }
            Ok(false) => {}
            Err(error) => {
                self.status = format!("Couldn't rename project file: {error}");
                self.status_error = true;
            }
        }
    }

    pub(super) fn service_dialogs(&mut self, ctx: &egui::Context) {
        if self.hook_flow.is_some() || self.layer_bake.is_some() {
            return;
        }
        if self.application_write.is_some() {
            self.show_application_directory_warning(ctx);
            return;
        }
        if self.save_failure.is_some() {
            self.show_save_failure(ctx);
            return;
        }
        if self.is_busy() {
            egui::Modal::new(egui::Id::new("background-operation")).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&self.status);
                });
            });
            return;
        }
        if let Some(paths) = self.external_changes.clone() {
            egui::Modal::new(egui::Id::new("external-file-change")).show(ctx, |ui| {
                ui.heading(locale::tr(ui, "Project files changed outside LDtk"));
                for path in &paths {
                    ui.label(path.display().to_string());
                }
                ui.horizontal(|ui| {
                    if ui.button(locale::tr(ui, "Reload from disk")).clicked()
                        && let Some(path) = self.project.as_ref().and_then(|p| p.path.clone())
                    {
                        self.external_changes = None;
                        self.request(ctx, Action::Open(path));
                    }
                    if ui.button(locale::tr(ui, "Keep my version")).clicked() {
                        self.external_changes = None;
                        self.file_snapshot = self
                            .project
                            .as_ref()
                            .and_then(|p| p.filesystem_snapshot().ok());
                        self.dirty = true;
                    }
                });
            });
        }
    }

    pub(super) fn remember_selected_level(&mut self) {
        if !self.settings.bool("openLastProject") {
            return;
        }
        if let (Some(project), Some(key)) = (&self.project, self.selected)
            && let (Some(path), Some(uid)) = (
                &project.path,
                project.level(key).and_then(|l| l["uid"].as_i64()),
            )
            && (self.settings.value["lastProject"]["filePath"].as_str() != path.to_str()
                || self.settings.value["lastProject"]["levelUid"].as_i64() != Some(uid))
        {
            self.settings.remember(path, uid);
        }
    }
    pub(super) fn persist_preferences(&mut self) {
        self.remember_selected_level();
        for (key, value) in [
            ("grid", self.grid),
            ("showDetails", self.details),
            ("zenMode", self.zen),
            ("singleLayerMode", self.single_layer),
            ("tileStacking", self.tile_stacking),
            ("tileEnumOverlays", self.tile_tags),
        ] {
            self.settings.value[key] = value.into();
        }
        if let Err(e) = self.settings.save() {
            self.status = e.to_string();
        }
    }
}

fn available_import_path(path: &std::path::Path) -> PathBuf {
    if !path.exists() {
        return path.to_owned();
    }
    for suffix in 2.. {
        let mut name = path.file_stem().unwrap_or_default().to_os_string();
        name.push(format!("_{suffix}.ldtk"));
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod creation_tests {
    use super::*;
    fn wait(app: &mut EditorApp, ctx: &egui::Context) {
        let until = Instant::now() + std::time::Duration::from_secs(5);
        while app.loading.is_some() && Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.poll_services(ctx);
        }
        assert!(app.loading.is_none());
    }

    #[test]
    fn external_file_poll_waits_for_unsaved_reload_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let mut project = Project::new();
        project.save(&path).unwrap();
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.dirty = true;

        let mut external = Project::load(&path).unwrap();
        external.json["bgColor"] = "#123456".into();
        external.save(&path).unwrap();
        app.request(&ctx, Action::Open(path.clone()));
        assert!(matches!(app.pending, Some(Action::Open(_))));

        app.last_file_check = Instant::now() - std::time::Duration::from_secs(3);
        app.poll_services(&ctx);
        assert!(app.external_changes.is_none());

        app.pending = None;
        app.poll_services(&ctx);
        assert_eq!(app.external_changes, Some(vec![path]));
    }

    #[test]
    fn preview_154_completed_load_progress_survives_project_installation_frame() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let (tx, rx) = mpsc::channel();
        app.loading = Some(rx);
        app.loading_progress = Some(Arc::new(Mutex::new(Some((2, 2)))));
        tx.send(prepare_project(Project::new(), Vec::new()))
            .unwrap();

        app.poll_services(&ctx);
        assert!(app.project.is_some());
        assert!(app.loading.is_none());
        assert_eq!(
            *app.loading_progress.as_ref().unwrap().lock().unwrap(),
            Some((2, 2))
        );

        app.poll_services(&ctx);
        assert!(app.loading_progress.is_none());
    }

    #[test]
    fn preview_154_background_export_completion_clears_progress() {
        let ctx = egui::Context::default();
        for result in [
            Ok(ldtk_core::exports::ExportReport {
                outputs: vec![PathBuf::from("level.png")],
                warnings: vec![],
            }),
            Err("write failed".to_owned()),
        ] {
            let failed = result.is_err();
            let mut app = EditorApp::empty();
            let (tx, rx) = mpsc::channel();
            app.exporting = Some(rx);
            app.exporting_progress = Some(Arc::new(Mutex::new(Some((1, 2)))));
            tx.send(result).unwrap();
            app.poll_services(&ctx);
            assert!(app.exporting.is_none());
            assert!(app.exporting_progress.is_none());
            assert_eq!(app.status_error, failed);
        }
    }

    #[test]
    fn preview_154_saved_png_logs_unavailable_background_without_save_failure() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let settings_path = directory.path().join("settings.json");
        app.settings.path = Some(settings_path.clone());
        let mut project = Project::new();
        let level = project.level_keys()[0];
        project.resize_level(level, 32, 32, 0, 0).unwrap();
        project.add_layer("Solid", "IntGrid", 16).unwrap();
        project.json["imageExportMode"] = "OneImagePerLevel".into();
        let level = project.level_mut(level).unwrap();
        level["bgRelPath"] = "missing.png".into();
        level["bgPos"] = "Unscaled".into();
        app.set_project(project);
        app.save_async_at(directory.path().join("Game.ldtk"));

        let until = Instant::now() + std::time::Duration::from_secs(5);
        while app.saving.is_some() && Instant::now() < until {
            app.poll_services(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.saving.is_none());
        assert!(!app.status_error, "{}", app.status);
        assert!(app.status.starts_with("Saved "), "{}", app.status);
        assert!(directory.path().join("Game/png/Level_0.png").is_file());

        app_log::flush();
        let log = std::fs::read_to_string(app_log::path(Some(&settings_path)).unwrap())
            .unwrap_or_default();
        assert!(
            log.lines().any(|line| line.contains("ERROR")
                && line.contains("Level_0")
                && line.contains("missing.png")),
            "Missing PNG background diagnostic in application log: {log:?}"
        );
    }

    #[test]
    fn new_project_is_published_before_editor_becomes_editable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("New.ldtk");
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.create_project_async(&ctx, path.clone());
        assert!(app.is_busy());
        wait(&mut app, &ctx);
        assert!(!app.dirty);
        let saved = Project::load(&path).unwrap();
        assert_eq!(saved.json, app.project.as_ref().unwrap().json);
        assert_eq!(app.project.as_ref().unwrap().path.as_ref(), Some(&path));
        assert!(app.panel.is_none());
        assert_eq!(
            app.settings.value["recentProjects"][0],
            path.to_str().unwrap()
        );
    }
    #[test]
    fn failed_creation_preserves_the_current_project() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("not-a-directory");
        std::fs::write(&blocked, b"existing").unwrap();
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let before = app.project.as_ref().unwrap().json.clone();
        app.create_project_async(&ctx, blocked.join("New.ldtk"));
        wait(&mut app, &ctx);
        assert!(app.status_error);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert_eq!(std::fs::read(blocked).unwrap(), b"existing");
    }
    #[test]
    fn importing_again_creates_another_file_and_preserves_existing_edits() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Game.ogmo");
        std::fs::create_dir(dir.path().join("levels")).unwrap();
        std::fs::write(&source,serde_json::json!({"ogmoVersion":"3.4","levelPaths":["levels"],"layers":[],"tilesets":[],"entities":[]}).to_string()).unwrap();
        std::fs::write(
            dir.path().join("levels/1.json"),
            serde_json::json!({"width":32,"height":32,"layers":[]}).to_string(),
        )
        .unwrap();
        let original = dir.path().join("Game.ldtk");
        let mut p = Project::new();
        p.json["bgColor"] = "#ABCDEF".into();
        p.save(&original).unwrap();
        let before = std::fs::read(&original).unwrap();
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.import_ogmo_async(&ctx, source);
        wait(&mut app, &ctx);
        assert_eq!(std::fs::read(original).unwrap(), before);
        let imported = dir.path().join("Game_2.ldtk");
        assert_eq!(app.project.as_ref().unwrap().path.as_ref(), Some(&imported));
        assert!(Project::load(imported).is_ok());
        assert!(!app.dirty);
    }
    #[test]
    fn opening_backup_replaces_a_previous_same_iid_original_path_binding() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("A.ldtk");
        let b = dir.path().join("B.ldtk");
        let mut p = Project::new();
        p.save(&a).unwrap();
        let backup_a = p.backup().unwrap().unwrap().join("A.ldtk");
        p.save(&b).unwrap();
        let backup_b = p.backup().unwrap().unwrap().join("B.ldtk");
        let key = egui::Id::new(("backup-original", text(&p.json, "iid", "").to_owned()));
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        for (backup, original) in [(backup_a, a), (backup_b, b)] {
            app.backup_async(&ctx, backup, original.clone(), false);
            wait(&mut app, &ctx);
            assert_eq!(ctx.data_mut(|d| d.get_temp::<PathBuf>(key)), Some(original));
            assert!(app.project.as_ref().unwrap().is_backup());
        }
    }
}
