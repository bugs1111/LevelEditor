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
            || self.exporting.is_some()
            || self.hook_flow.is_some()
    }

    pub(super) fn poll_services(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            match result {
                Ok(loaded) => {
                    if let Some(original) = loaded.backup_original {
                        let iid = text(&loaded.project.json, "iid", "").to_owned();
                        ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::new(("backup-original", iid)), original)
                        });
                    }
                    self.set_project(loaded.project);
                    self.dirty = loaded.migrated;
                    self.resource_scan(ctx);
                    self.queue_project_hooks(ctx, "AfterLoad", hooks::Continuation::None);
                    if !loaded.warnings.is_empty() {
                        self.status = loaded.warnings.join("; ");
                        self.status_error = true;
                    }
                }
                Err(e) => {
                    self.status = format!("Open failed: {e}");
                    self.status_error = true;
                }
            }
        }
        if let Some(result) = self.saving.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.saving = None;
            match result {
                Ok(saved) => {
                    let changed_resources = self.changed_resource_files();
                    self.project = Some(saved.project);
                    self.dirty = false;
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
                    self.status = format!("Save failed: {e}");
                    self.status_error = true;
                }
            }
        }
        if let Some(result) = self.exporting.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.exporting = None;
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
        if self.is_busy() || self.project.is_some() && !self.capture_ready() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        self.pump_resources(ctx);
        if self.last_file_check.elapsed() > std::time::Duration::from_secs(2)
            && !self.is_busy()
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

    pub(super) fn export_async(&mut self, format: &str, dir: PathBuf) {
        if self.is_busy() {
            return;
        }
        let Some(project) = self.project.clone() else {
            return;
        };
        let format = format.to_owned();
        let (tx, rx) = mpsc::channel();
        self.exporting = Some(rx);
        self.status = "Exporting…".into();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let result = if format == "tmx" {
                project.export_tiled_with_report(&dir)
            } else {
                match format.as_str() {
                    "simple" => project.export_simplified(&dir),
                    "png" => project.export_png(&dir),
                    "csv" => project.export_csv(&dir),
                    _ => project.export_gamemaker(&dir),
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
        let mut dialog = rfd::FileDialog::new().add_filter("Ogmo project", &["ogmo"]);
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
                .map(|report| SavedProject { project, report })
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn service_dialogs(&mut self, ctx: &egui::Context) {
        if self.hook_flow.is_some() {
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
                ui.heading("Project files changed outside LDtk");
                for path in &paths {
                    ui.label(path.display().to_string());
                }
                ui.horizontal(|ui| {
                    if ui.button("Reload from disk").clicked()
                        && let Some(path) = self.project.as_ref().and_then(|p| p.path.clone())
                    {
                        self.external_changes = None;
                        self.request(ctx, Action::Open(path));
                    }
                    if ui.button("Keep my version").clicked() {
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
