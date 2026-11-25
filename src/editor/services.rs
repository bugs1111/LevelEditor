use super::*;

pub(super) struct LoadedProject {
    pub project: Project,
    snapshot: Option<ldtk_core::FilesystemSnapshot>,
    pub migrated: bool,
    pub warnings: Vec<String>,
    pub backup_original: Option<PathBuf>,
    keep_canvas_preview: bool,
}

#[derive(Debug)]
pub(super) enum LoadFailure {
    MissingProject(PathBuf),
    ChangedDuringOpen,
    Other(String),
}

pub(super) struct SavedProject {
    pub project: Project,
    pub report: ldtk_core::SaveReport,
}

pub(super) enum SaveFailure {
    Write {
        path: PathBuf,
        reason: SaveWriteFailure,
    },
    MissingTarget(PathBuf),
}

pub(super) enum SaveWriteFailure {
    DirectoryConflict,
    Io(String),
    Interrupted,
}

pub(super) const SAVE_INTERRUPTED: &str =
    "Saving was interrupted before completion could be confirmed.";

fn poll_worker<T>(worker: &mut Option<mpsc::Receiver<T>>) -> Option<Result<T, mpsc::RecvError>> {
    let result = match worker.as_ref()?.try_recv() {
        Ok(result) => Ok(result),
        Err(mpsc::TryRecvError::Empty) => return None,
        Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
    };
    *worker = None;
    Some(result)
}

fn save_folder_conflicts(path: &Path) -> bool {
    let folder = path.with_extension("");
    folder != path && std::fs::metadata(folder).is_ok_and(|metadata| !metadata.is_dir())
}

fn unreadable_auto_tile_caches(project: &Project) -> Vec<(LevelKey, usize, i64)> {
    let mut repairs = Vec::new();
    for key in project.level_keys() {
        let Some(level) = project.level(key) else {
            continue;
        };
        for (index, layer) in level["layerInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(uid) = layer["layerDefUid"]
                .as_i64()
                .or_else(|| layer["layerDefId"].as_i64())
            else {
                continue;
            };
            let Some(definition) = project.layer_def(uid) else {
                continue;
            };
            let has_auto_layer = definition["type"] == "AutoLayer"
                || definition["type"] == "IntGrid"
                    && (definition["tilesetDefUid"].is_i64()
                        || definition["autoTilesetDefUid"].is_i64());
            if has_auto_layer
                && !layer["autoLayerTiles"].as_array().is_some_and(|tiles| {
                    tiles.iter().all(|tile| {
                        ["d", "px", "src"]
                            .into_iter()
                            .all(|field| tile[field][0].is_i64() && tile[field][1].is_i64())
                    })
                })
            {
                repairs.push((key, index, uid));
            }
        }
    }
    repairs
}

pub(super) fn load_project(
    path: PathBuf,
    progress: impl FnMut(usize, usize),
) -> Result<LoadedProject, LoadFailure> {
    let (project, documents) = Project::load_with_document_snapshot_progress(&path, progress)
        .map_err(|error| {
            let missing_document = matches!(
                &error,
                ldtk_core::Error::Io { source, .. }
                    if source.kind() == std::io::ErrorKind::NotFound
            );
            if missing_document
                && std::fs::metadata(&path)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            {
                LoadFailure::MissingProject(path)
            } else {
                LoadFailure::Other(error.to_string())
            }
        })?;
    prepare_project(project, documents)
}

fn prepare_project(
    mut project: Project,
    documents: ldtk_core::FilesystemSnapshot,
) -> Result<LoadedProject, LoadFailure> {
    let failure = |error: ldtk_core::Error| LoadFailure::Other(error.to_string());
    let version = text(&project.json, "jsonVersion", "0.0");
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let modern = (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) >= (1, 5);
    let repairs = unreadable_auto_tile_caches(&project);
    for &(key, index, _) in &repairs {
        // Migration expects an array or null. The cache is derived from the source grid.
        project.layer_mut(key, index).expect("enumerated layer")["autoLayerTiles"] = Value::Null;
    }
    let migrated = project.migrate_to_current().map_err(failure)?;
    if modern {
        for &(key, _, uid) in &repairs {
            project.rebuild_auto_layer(key, uid).map_err(failure)?;
        }
        project.reconcile_cached_auto_tiles();
    } else {
        for key in project.level_keys() {
            project.rebuild_auto_layers(key).map_err(failure)?;
        }
    }
    let mut snapshot = documents.clone();
    project.watch_available_resources(&mut snapshot);
    if !documents.changed_files().map_err(failure)?.is_empty() {
        return Err(LoadFailure::ChangedDuringOpen);
    }
    Ok(LoadedProject {
        snapshot: Some(snapshot),
        project,
        migrated: migrated || !repairs.is_empty(),
        warnings: Vec::new(),
        backup_original: None,
        keep_canvas_preview: false,
    })
}

impl EditorApp {
    pub(super) fn reject_conflicting_save_folder(&mut self, path: &Path) -> bool {
        if !save_folder_conflicts(path) {
            return false;
        }
        self.fail_save(path.to_owned(), SaveWriteFailure::DirectoryConflict);
        true
    }

    pub(super) fn fail_save(&mut self, path: PathBuf, reason: SaveWriteFailure) {
        let status = match &reason {
            SaveWriteFailure::DirectoryConflict => {
                format!(
                    "Save failed: {} occupies the project folder path",
                    path.with_extension("").display()
                )
            }
            SaveWriteFailure::Io(error) => format!("Save failed: {error}"),
            SaveWriteFailure::Interrupted => SAVE_INTERRUPTED.into(),
        };
        self.report_save_error(status);
        self.save_failure = Some(SaveFailure::Write { path, reason });
    }

    fn report_save_error(&mut self, status: String) {
        self.clear_save_progress();
        self.status = status;
        self.status_error = true;
        self.report_saving_failed();
    }

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
                    snapshot: project.filesystem_snapshot().ok(),
                    project,
                    migrated: false,
                    warnings: Vec::new(),
                    backup_original: None,
                    keep_canvas_preview: false,
                })
            })();
            let _ = tx.send(result.map_err(LoadFailure::Other));
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
        if let Some(result) = poll_worker(&mut self.loading) {
            match result {
                Ok(Ok(loaded)) => {
                    let read_only = loaded.project.is_backup();
                    if self.loading_progress.is_some() {
                        ctx.request_repaint();
                    }
                    if let Some(original) = loaded.backup_original
                        && let Some(backup) = loaded.project.path.as_deref()
                        && read_only
                    {
                        ctx.data_mut(|d| {
                            d.insert_temp(project_dialogs::backup_original_key(backup), original)
                        });
                    }
                    if !loaded.keep_canvas_preview {
                        self.details = true;
                        self.render_auto = true;
                        self.settings.value["showDetails"] = true.into();
                    }
                    self.install_project(loaded.project, loaded.snapshot);
                    self.dirty = loaded.migrated && !read_only;
                    if self.dirty
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
                Ok(Err(error)) => {
                    self.loading_progress = None;
                    self.status = match error {
                        LoadFailure::MissingProject(path) => {
                            self.remove_recent("recentProjects", &path);
                            "Project file not found".into()
                        }
                        LoadFailure::ChangedDuringOpen => {
                            "Project files changed while opening. Open the project again.".into()
                        }
                        LoadFailure::Other(error) => format!("Open failed: {error}"),
                    };
                    self.status_error = true;
                }
                Err(_) => {
                    self.loading_progress = None;
                    self.status = "The project operation was interrupted.".into();
                    self.status_error = true;
                }
            }
        }
        if let Some(result) = poll_worker(&mut self.saving) {
            match result {
                Ok(Ok(mut saved)) => {
                    for diagnostic in &saved.report.export_diagnostics {
                        app_log::record(ctx, self.settings.path.as_deref(), diagnostic, true);
                    }
                    let mut snapshot = saved.report.document_snapshot.take();
                    if let Some(current) = &mut snapshot {
                        if let Some(previous) = self.file_snapshot.as_ref() {
                            current.carry_resource_watches(previous);
                        }
                        saved
                            .project
                            .refresh_resource_watches(current, &Default::default());
                    }
                    self.discard_unused_images(&saved.project);
                    self.project = Some(saved.project);
                    self.dirty = false;
                    self.history.clear_modified_levels();
                    self.file_snapshot = snapshot;
                    let saved_name = self
                        .project
                        .as_ref()
                        .and_then(|p| p.path.as_deref())
                        .and_then(Path::file_stem)
                        .filter(|_| saved.report.export_errors.is_empty())
                        .map(|name| name.to_string_lossy().into_owned());
                    self.status = if saved.report.export_errors.is_empty() {
                        notifications::SAVED_PROJECT_TITLE.into()
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
                    let changed_resources = self.changed_resource_files();
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
                    self.queue_project_hooks(
                        ctx,
                        "AfterSave",
                        hooks::Continuation::AfterSave(saved_name),
                    );
                }
                Ok(Err(ldtk_core::Error::FilesChanged(paths)))
                    if self.save_attempt.as_ref().is_none_or(|attempt| {
                        self.project
                            .as_ref()
                            .and_then(|project| project.path.as_ref())
                            == Some(attempt)
                    }) =>
                {
                    self.clear_save_progress();
                    self.external_changes = Some(paths);
                    self.status = "Project files changed outside LDtk".into();
                    self.status_error = true;
                }
                Ok(Err(e)) => {
                    if let Some(path) = self.save_attempt.take() {
                        let folder = path.with_extension("");
                        let reason = match &e {
                            ldtk_core::Error::FilesChanged(_) => SaveWriteFailure::Io(
                                locale::tr_ctx(ctx, "The destination file changed during saving.")
                                    .into(),
                            ),
                            ldtk_core::Error::Io { path: failed, .. }
                                if failed.starts_with(&folder) && save_folder_conflicts(&path) =>
                            {
                                SaveWriteFailure::DirectoryConflict
                            }
                            _ => SaveWriteFailure::Io(e.to_string()),
                        };
                        self.fail_save(path, reason);
                    } else {
                        self.report_save_error(format!("Save failed: {e}"));
                    }
                }
                Err(_) => {
                    if let Some(path) = self.save_attempt.take() {
                        self.fail_save(path, SaveWriteFailure::Interrupted);
                    } else {
                        self.report_save_error("The file operation was interrupted.".into());
                    }
                }
            }
        }
        self.animate_save_progress(ctx, Instant::now());
        if let Some(result) = poll_worker(&mut self.exporting) {
            self.exporting_progress = None;
            match result {
                Ok(Ok(report)) => {
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
                Ok(Err(e)) => {
                    self.status = format!("Export failed: {e}");
                    self.status_error = true;
                }
                Err(_) => {
                    self.status = "Export was interrupted.".into();
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
            let mut paths = self.changed_document_files();
            paths.extend(self.changed_resource_files());
            if !paths.is_empty() {
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
            let result = if matches!(format.as_str(), "tmx" | "tmj") {
                use ldtk_core::exports::TiledFormat;
                let format = if format == "tmj" {
                    TiledFormat::Tmj
                } else {
                    TiledFormat::Tmx
                };
                project.export_tiled_format_with_report_progress(&dir, format, &mut report_progress)
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
                        snapshot: report.project.filesystem_snapshot().ok(),
                        project: report.project,
                        migrated: false,
                        warnings: report.warnings,
                        backup_original: None,
                        keep_canvas_preview: false,
                    })
                })
                .map_err(|error| LoadFailure::Other(error.to_string()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn browse_ogmo(&mut self, ctx: &egui::Context) {
        let mut dialog = FileDialog::new().add_filter("Supported file types", &["ogmo"]);
        if let Some(dir) = self
            .settings
            .ui_dir(None, "ImportOgmo", None)
            .or_else(|| self.settings.default_project_dialog_dir())
        {
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
        // Relinking or restoring the open backup reuses the original editor's preview state.
        let keep_canvas_preview = self
            .project
            .as_ref()
            .and_then(|project| project.path.as_ref())
            == Some(&backup);
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
                let (mut project, warnings) = if restore {
                    Project::restore_backup_with_exports(&backup, &original)
                        .map(|(project, report)| (project, report.warnings))
                } else {
                    Project::load_backup(&backup, &original).map(|project| (project, Vec::new()))
                }
                .map_err(|e| e.to_string())?;
                let migrated = project.migrate_to_current().map_err(|e| e.to_string())?;
                for key in project.level_keys() {
                    project
                        .rebuild_auto_layers(key)
                        .map_err(|e| e.to_string())?;
                }
                Ok(LoadedProject {
                    snapshot: project.filesystem_snapshot().ok(),
                    project,
                    migrated,
                    warnings,
                    backup_original: Some(original),
                    keep_canvas_preview,
                })
            })();
            let _ = tx.send(result.map_err(LoadFailure::Other));
            ctx.request_repaint();
        });
    }

    pub(super) fn rename_async(&mut self, ctx: &egui::Context, name: String) {
        let Some(mut project) = self.project.clone() else {
            return;
        };
        let snapshot = self.file_snapshot.clone();
        let (tx, rx) = mpsc::channel();
        self.saving = Some(rx);
        self.status = "Renaming project…".into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = project
                .rename_project_with_snapshot(&name, snapshot.as_ref())
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
                        match self
                            .file_snapshot
                            .get_or_insert_with(Default::default)
                            .acknowledge_document_changes(&paths)
                        {
                            Ok(()) => {
                                let remaining = self.changed_document_files();
                                self.external_changes =
                                    (!remaining.is_empty()).then_some(remaining);
                                self.dirty = true;
                            }
                            Err(error) => {
                                self.status = error.to_string();
                                self.status_error = true;
                            }
                        }
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
    fn opening_does_not_use_newer_disk_bytes_as_the_baseline_for_old_documents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let external = dir
            .path()
            .join(project.external_levels.keys().next().unwrap());
        for target in [&path, &external] {
            let original = std::fs::read(target).unwrap();
            let mut changed = original.clone();
            changed.push(b'\n');
            let result = load_project(path.clone(), |done, total| {
                if total > 0 && done == total {
                    std::fs::write(target, &changed).unwrap();
                }
            });
            assert!(matches!(result, Err(LoadFailure::ChangedDuringOpen)));
            std::fs::write(target, original).unwrap();
        }
    }

    #[test]
    fn unreadable_artwork_keeps_the_document_and_other_resource_watches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let unreadable = dir.path().join("A.png");
        let missing = dir.path().join("Missing.png");
        let readable = dir.path().join("Z.png");
        std::fs::create_dir(&unreadable).unwrap();
        std::fs::write(&readable, b"first artwork").unwrap();
        let mut project = Project::new();
        project.add_tileset(&unreadable, 16, 16, 16).unwrap();
        project.add_tileset(&missing, 16, 16, 16).unwrap();
        project.add_tileset(&readable, 16, 16, 16).unwrap();
        project.save(&path).unwrap();

        let loaded = load_project(path.clone(), |_, _| {}).unwrap();
        let snapshot = loaded.snapshot.expect("the document needs a baseline");
        let mut changed = std::fs::read(&path).unwrap();
        changed.push(b'\n');
        std::fs::write(&path, changed).unwrap();
        std::fs::write(&missing, b"restored artwork").unwrap();
        std::fs::write(&readable, b"other artwork").unwrap();
        assert_eq!(snapshot.changed_files().unwrap(), [path, missing, readable]);
    }

    #[test]
    fn disconnected_open_export_and_rename_workers_release_the_editor() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.dirty = true;
        let document = app.project.as_ref().unwrap().json.clone();

        let (open_sender, open_job) = mpsc::channel();
        app.loading = Some(open_job);
        app.loading_progress = Some(Arc::new(Mutex::new(Some((0, 1)))));
        app.poll_services(&ctx);
        assert!(app.is_busy());
        drop(open_sender);
        app.poll_services(&ctx);
        assert!(!app.is_busy());
        assert!(app.loading_progress.is_none() && app.status_error);
        assert_eq!(app.status, "The project operation was interrupted.");

        let (export_sender, export_job) = mpsc::channel();
        app.exporting = Some(export_job);
        app.exporting_progress = Some(Arc::new(Mutex::new(Some((0, 1)))));
        app.poll_services(&ctx);
        assert!(app.is_busy());
        drop(export_sender);
        app.poll_services(&ctx);
        assert!(!app.is_busy());
        assert!(app.exporting_progress.is_none() && app.status_error);
        assert_eq!(app.status, "Export was interrupted.");

        let (rename_sender, rename_job) = mpsc::channel();
        app.saving = Some(rename_job);
        drop(rename_sender);
        app.poll_services(&ctx);
        assert!(!app.is_busy());
        assert!(app.save_failure.is_none() && app.status_error);
        assert_eq!(app.status, "The file operation was interrupted.");

        assert!(app.dirty);
        assert_eq!(app.project.as_ref().unwrap().json, document);
    }

    #[test]
    fn renaming_preserves_source_documents_edited_outside_the_editor() {
        let ctx = egui::Context::default();
        for edit_external_level in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("Map.ldtk");
            let mut project = Project::new();
            project.set_external_levels(true).unwrap();
            project.save(&path).unwrap();
            let changed_path = if edit_external_level {
                dir.path()
                    .join(project.external_levels.keys().next().unwrap())
            } else {
                path.clone()
            };
            let mut app = EditorApp::empty();
            let loaded = load_project(path.clone(), |_, _| {}).unwrap();
            app.install_project(loaded.project, loaded.snapshot);
            let mut changed_bytes = std::fs::read(&changed_path).unwrap();
            changed_bytes.push(b'\n');
            std::fs::write(&changed_path, &changed_bytes).unwrap();

            app.rename_async(&ctx, "Renamed".into());
            let result = app
                .saving
                .take()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(15))
                .unwrap();
            let (sender, receiver) = mpsc::channel();
            assert!(sender.send(result).is_ok());
            app.saving = Some(receiver);
            app.poll_services(&ctx);

            assert!(
                std::fs::read(&changed_path).is_ok_and(|bytes| bytes == changed_bytes),
                "renaming must preserve the edited source document"
            );
            assert_eq!(
                app.external_changes.as_deref(),
                Some(std::slice::from_ref(&changed_path))
            );
            assert_eq!(app.project.as_ref().unwrap().path.as_ref(), Some(&path));
            assert!(!dir.path().join("Renamed.ldtk").exists());
            assert!(!dir.path().join("Renamed").exists());
        }
    }

    #[test]
    fn opening_repairs_only_missing_or_malformed_auto_tile_caches() {
        use serde_json::json;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cached.ldtk");
        let mut project = Project::new();
        let atlas = project
            .add_tileset(&dir.path().join("Atlas.png"), 16, 32, 16)
            .unwrap();
        let affected = project.add_layer("Affected", "IntGrid", 16).unwrap();
        let retained = project.add_layer("Retained", "IntGrid", 16).unwrap();
        let level = project.level_keys()[0];
        let second = project.add_level(None, "Untouched", 32, 32).unwrap();
        let index_for = |project: &Project, key, uid| {
            project.level(key).unwrap()["layerInstances"]
                .as_array()
                .unwrap()
                .iter()
                .position(|layer| layer["layerDefUid"] == uid)
                .unwrap()
        };
        let [affected_index, retained_index] =
            [affected, retained].map(|uid| index_for(&project, level, uid));
        let mut rule_ids = Vec::new();
        for uid in [affected, retained] {
            project.assign_tileset(uid, atlas).unwrap();
            let group = project.add_auto_rule_group(uid, "Walls").unwrap();
            let rule = project.add_auto_rule(uid, group, 1).unwrap();
            project
                .update_auto_rule(
                    uid,
                    group,
                    rule,
                    &json!({"pattern":[1],"tileRectsIds":[[0]]}),
                )
                .unwrap();
            rule_ids.push(rule);
        }
        project
            .paint_int_grid(level, affected_index, 0, 0, 1)
            .unwrap();
        let expected = project.layer(level, affected_index).unwrap()["autoLayerTiles"].clone();
        assert_eq!(expected.as_array().unwrap().len(), 1);
        let cached_tile = |rule| {
            json!([{
                "d":[rule,0],"t":1,"f":0,"a":1,"px":[0,0],"src":[16,0],"future":"keep"
            }])
        };
        let valid = cached_tile(rule_ids[1]);
        project.layer_mut(level, retained_index).unwrap()["autoLayerTiles"] = valid.clone();
        let second_index = index_for(&project, second, affected);
        project.layer_mut(second, second_index).unwrap()["autoLayerTiles"] =
            cached_tile(rule_ids[0]);
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();

        let ext_file = |key: LevelKey| {
            dir.path().join(
                project.json["levels"][key.level]["externalRelPath"]
                    .as_str()
                    .unwrap(),
            )
        };
        let original = std::fs::read(ext_file(level)).unwrap();
        let mut untouched = std::fs::read(ext_file(second)).unwrap();
        untouched.extend_from_slice(b" \n");
        std::fs::write(ext_file(second), &untouched).unwrap();
        for malformed in [
            Value::Null,
            json!({}),
            json!([{"d":null,"px":[0,0],"src":[0,0]}]),
        ] {
            let mut corrupt: Value = serde_json::from_slice(&original).unwrap();
            corrupt["layerInstances"][affected_index]["autoLayerTiles"] = malformed;
            std::fs::write(ext_file(level), serde_json::to_vec(&corrupt).unwrap()).unwrap();
            let mut loaded = load_project(path.clone(), |_, _| {}).unwrap();
            assert!(loaded.migrated);
            assert_eq!(
                loaded.project.layer(level, affected_index).unwrap()["autoLayerTiles"],
                expected
            );
            assert_eq!(
                loaded.project.layer(level, retained_index).unwrap()["autoLayerTiles"],
                valid
            );
            assert_eq!(
                loaded.project.layer(second, second_index).unwrap()["autoLayerTiles"],
                cached_tile(rule_ids[0])
            );
            loaded.project.save(&path).unwrap();
            assert_eq!(std::fs::read(ext_file(second)).unwrap(), untouched);
        }
    }

    #[test]
    fn opening_missing_recent_project_removes_only_that_history_entry() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("Moved.ldtk");
        let malformed = directory.path().join("Malformed.ldtk");
        let external = directory.path().join("External.ldtk");
        std::fs::write(&malformed, b"broken JSON").unwrap();
        let mut project = Project::new();
        project.json["externalLevels"] = true.into();
        project.save(&external).unwrap();
        let level = project.json["levels"][0]["externalRelPath"]
            .as_str()
            .unwrap();
        std::fs::remove_file(directory.path().join(level)).unwrap();

        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let settings = directory.path().join("settings.json");
        app.settings.path = Some(settings.clone());
        let history = serde_json::json!([missing, malformed, external]);
        app.settings.value["recentProjects"] = history.clone();
        app.settings.save().unwrap();

        for path in [&malformed, &external] {
            app.request(&ctx, Action::Open(path.clone()));
            wait(&mut app, &ctx);
            assert!(app.status_error);
            assert_eq!(app.settings.value["recentProjects"], history);
        }

        app.request(&ctx, Action::Open(missing.clone()));
        wait(&mut app, &ctx);
        assert!(app.project.is_none());
        assert!(app.status_error);
        assert_eq!(app.status, "Project file not found");
        let expected = serde_json::json!([malformed, external]);
        assert_eq!(app.settings.value["recentProjects"], expected);
        let stored: Value = serde_json::from_slice(&std::fs::read(settings).unwrap()).unwrap();
        assert_eq!(stored["recentProjects"], expected);
    }

    #[test]
    fn opening_a_project_resets_canvas_preview_but_navigating_a_level_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("World.ldtk");
        let mut project = Project::new();
        let second = project.add_level(None, "Second", 256, 256).unwrap();
        project.save(&original).unwrap();
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.set_details(false);
        app.set_auto_layer_rendering(false, false);
        app.navigate_level(second, true);
        assert_eq!(app.selected, Some(second));
        assert!(!app.details && !app.render_auto);

        app.open_async(&ctx, original);
        wait(&mut app, &ctx);
        assert!(app.details && app.render_auto);
        assert_eq!(app.settings.value["showDetails"], true);
        assert!(app.quick_notification.is_none());
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
        tx.send(prepare_project(
            Project::new(),
            ldtk_core::FilesystemSnapshot::default(),
        ))
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
    fn completed_save_detects_external_edits_before_the_result_is_installed() {
        check_saved_document_watches(false);
    }

    #[test]
    fn completed_save_keeps_document_watches_with_unreadable_artwork() {
        check_saved_document_watches(true);
    }

    fn check_saved_document_watches(unreadable_artwork: bool) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let artwork = dir.path().join("Atlas.png");
        std::fs::write(&artwork, b"original artwork").unwrap();
        let mut project = Project::new();
        project.add_tileset(&artwork, 16, 16, 16).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let external = dir
            .path()
            .join(project.external_levels.keys().next().unwrap());

        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.project.as_mut().unwrap().json["defaultLevelBgColor"] = "#112233".into();
        app.dirty = true;
        app.save_async_at(path.clone(), app.file_snapshot.clone());
        let saved = app
            .saving
            .take()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap()
            .unwrap();

        if unreadable_artwork {
            std::fs::remove_file(&artwork).unwrap();
            std::fs::create_dir(&artwork).unwrap();
        }
        for document in [&path, &external] {
            let mut bytes = std::fs::read(document).unwrap();
            bytes.push(b'\n');
            std::fs::write(document, bytes).unwrap();
        }
        let (sender, receiver) = mpsc::channel();
        assert!(sender.send(Ok(saved)).is_ok());
        app.saving = Some(receiver);
        app.poll_services(&ctx);

        assert!(!app.dirty, "the editor's save did finish");
        assert_eq!(
            app.project.as_ref().unwrap().json["defaultLevelBgColor"],
            "#112233"
        );
        let mut changed = vec![path.clone(), external];
        changed.sort();
        assert_eq!(app.changed_document_files(), changed);
        app.clear_save_progress();
        app.save_document(&ctx, path);
        assert!(app.saving.is_none());
        assert_eq!(app.external_changes, Some(changed));
    }

    #[test]
    fn completed_save_shows_a_localized_title_and_the_project_file_stem() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("雨.林.ldtk");
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.last_chance = Some(("Identifier style changed".into(), Instant::now()));
        app.save_async_at(path.clone(), None);
        assert!(app.last_chance.is_none());

        let until = Instant::now() + std::time::Duration::from_secs(5);
        while app.saving.is_some() && Instant::now() < until {
            app.poll_services(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.saving.is_none());
        assert!(Project::load(&path).is_ok());
        app.clear_save_progress();

        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        for (language, title) in [
            (locale::Language::English, "Saved project"),
            (locale::Language::SimplifiedChinese, "项目已保存"),
        ] {
            locale::set(&ctx, language);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| app.canvas_notifications(ctx, screen),
            );
            let text = |label| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos),
                    _ => None,
                })
            };
            let title = text(title).expect("successful save title");
            let name = text("雨.林").expect("file stem as notification subtitle");
            assert!(name.x > title.x && name.y > title.y);
            assert!(text("Saved 1 files").is_none());
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
        app.save_async_at(directory.path().join("Game.ldtk"), None);

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
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        for (backup, original) in [(backup_a, a), (backup_b, b)] {
            let key = project_dialogs::backup_original_key(&backup);
            app.backup_async(&ctx, backup, original.clone(), false);
            wait(&mut app, &ctx);
            assert_eq!(ctx.data_mut(|d| d.get_temp::<PathBuf>(key)), Some(original));
            assert!(app.project.as_ref().unwrap().is_backup());
        }
    }

    #[test]
    fn backup_preview_confirms_before_restoring_the_linked_original() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("Backup.ldtk");
        let mut project = Project::new();
        project.json["levels"][0]["identifier"] = serde_json::json!("Archived");
        project.save(&original).unwrap();
        let backup = project.backup().unwrap().unwrap().join("Backup.ldtk");
        project.json["levels"][0]["identifier"] = serde_json::json!("Current");
        project.save(&original).unwrap();

        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(Project::load(&backup).unwrap());
        app.repaint = Some(ctx.clone());
        assert!(app.backup_original(&ctx).is_none());
        let archived = std::fs::read(&backup).unwrap();
        app.save(false);
        assert!(matches!(
            app.canvas_notices.last().map(|entry| &entry.notice),
            Some(notifications::CanvasNotice::CannotSaveBackup)
        ));
        assert!(!app.is_busy());
        assert!(!EditorApp::backup_restore_open(&ctx));
        assert_eq!(std::fs::read(&backup).unwrap(), archived);
        app.set_details(false);
        app.set_auto_layer_rendering(false, false);
        app.backup_async(&ctx, backup, original.clone(), false);
        wait(&mut app, &ctx);
        assert!(!app.details && !app.render_auto);
        assert_eq!(app.backup_original(&ctx).as_ref(), Some(&original));
        app.save(false);
        assert!(!app.is_busy());
        assert!(!EditorApp::backup_restore_open(&ctx));
        for (key, expected) in [(Key::Escape, "Current"), (Key::Enter, "Archived")] {
            app.confirm_preview_restore(&ctx);
            assert!(EditorApp::backup_restore_open(&ctx));
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                app.preview_backup_restore_dialog(ctx)
            });
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| app.preview_backup_restore_dialog(ctx),
            );
            assert!(!EditorApp::backup_restore_open(&ctx));
            wait(&mut app, &ctx);
            assert_eq!(
                Project::load(&original).unwrap().json["levels"][0]["identifier"],
                expected
            );
        }
        assert!(!app.project.as_ref().unwrap().is_backup());
        assert!(!app.details && !app.render_auto);
    }
}
