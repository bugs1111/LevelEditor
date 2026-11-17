#[path = "editor/app_log.rs"]
mod app_log;
#[path = "editor/bleep.rs"]
pub(crate) mod bleep;
#[path = "editor/brush.rs"]
mod brush;
#[path = "editor/chrome.rs"]
mod chrome;
#[path = "editor/command_args.rs"]
mod command_args;
#[path = "editor/command_tools.rs"]
mod command_tools;
#[path = "editor/crash.rs"]
mod crash;
#[path = "editor/entity_chain_preview.rs"]
mod entity_chain_preview;
#[path = "editor/entity_erase.rs"]
mod entity_erase;
#[path = "editor/entity_inspector.rs"]
mod entity_inspector;
#[path = "editor/entity_tools.rs"]
mod entity_tools;
#[path = "editor/field_tools.rs"]
mod field_tools;
#[path = "editor/focus.rs"]
mod focus;
#[cfg(test)]
#[path = "gpu_surface_tests.rs"]
mod gpu_surface_tests;
#[path = "editor/help.rs"]
mod help;
#[path = "editor/home_files.rs"]
mod home_files;
#[path = "editor/hooks.rs"]
mod hooks;
#[path = "editor/level_navigation.rs"]
mod level_navigation;
#[path = "editor/level_resize.rs"]
mod level_resize;
#[path = "editor/library.rs"]
mod library;
#[cfg(test)]
#[path = "editor/modal_workflow_tests.rs"]
mod modal_workflow_tests;
#[path = "editor/navigation.rs"]
mod navigation;
#[path = "editor/notifications.rs"]
mod notifications;
#[path = "editor/palette_popout.rs"]
mod palette_popout;
#[path = "editor/panels.rs"]
mod panels;
#[path = "editor/point_tools.rs"]
mod point_tools;
#[path = "editor/preview.rs"]
mod preview;
#[path = "editor/project_dialogs.rs"]
mod project_dialogs;
#[path = "editor/resize_tools.rs"]
mod resize_tools;
#[path = "editor/resources.rs"]
mod resources;
#[path = "editor/search.rs"]
mod search;
#[path = "editor/selection_click.rs"]
mod selection_click;
#[path = "editor/selection_render.rs"]
mod selection_render;
#[path = "editor/selection_tools.rs"]
mod selection_tools;
#[path = "editor/services.rs"]
mod services;
#[path = "editor/settings.rs"]
mod settings;
#[path = "editor/shortcuts.rs"]
mod shortcuts;
#[path = "editor/text_editor.rs"]
mod text_editor;
#[path = "editor/tools.rs"]
mod tools;
#[path = "editor/ui_commit.rs"]
mod ui_commit;
#[path = "editor/ui_fields.rs"]
mod ui_fields;
#[path = "editor/update_dialog.rs"]
mod update_dialog;
#[path = "editor/visibility.rs"]
mod visibility;
#[path = "editor/world_context.rs"]
mod world_context;
#[path = "editor/world_form_dialog.rs"]
mod world_form_dialog;
#[path = "editor/world_tools.rs"]
mod world_tools;

use crate::{
    assets::Assets,
    canvas::{self, RenderOptions, number, text},
    theme,
    viewport::{Viewport, stroke_cells},
};
use eframe::egui::{
    self, Align2, Color32, FontId, Key, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use ldtk_core::{History, LevelKey, Project};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Instant,
};

fn resource_path(relative: &str) -> PathBuf {
    resource_path_in(relative, std::env::current_exe().ok().as_deref())
}

fn resource_path_in(relative: &str, executable: Option<&Path>) -> PathBuf {
    if let Some(parent) = executable.and_then(Path::parent) {
        for root in [parent.join("../Resources"), parent.join("resources")] {
            let candidate = root.join(relative);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

pub fn native_gpu_config() -> eframe::egui_wgpu::WgpuConfiguration {
    let setup = eframe::egui_wgpu::WgpuSetupCreateNew {
        power_preference: if settings::Settings::load().bool("useBestGPU") {
            eframe::wgpu::PowerPreference::HighPerformance
        } else {
            eframe::wgpu::PowerPreference::LowPower
        },
        ..Default::default()
    };
    let mut config = eframe::egui_wgpu::WgpuConfiguration {
        desired_maximum_frame_latency: Some(1),
        wgpu_setup: setup.into(),
        ..Default::default()
    };
    let original = config.on_surface_error.clone();
    config.on_surface_error = std::sync::Arc::new(move |error| {
        // egui-wgpu 0.33 uploads before acquiring the surface. Skipping a
        // failed frame must still submit those writes to release staging buffers.
        canvas::submit_pending_gpu_uploads();
        original(error)
    });
    config
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Panel {
    Project,
    Level,
    Layers,
    Entities,
    Enums,
    Tilesets,
    Help,
    Entity,
    World,
    Fields,
    Rules,
    Settings,
    Export,
}

#[derive(Clone)]
enum Action {
    InstallUpdate,
    New,
    NewInDir(PathBuf),
    Browse,
    BrowseInDir(PathBuf),
    BrowseOgmo,
    Open(PathBuf),
    ImportOgmo(PathBuf),
    OpenBackup { backup: PathBuf, original: PathBuf },
    RestoreBackup { backup: PathBuf, original: PathBuf },
    RenameProject(String),
    Quit,
    Close,
}

pub struct EditorApp {
    project: Option<Project>,
    history: History,
    selected: Option<LevelKey>,
    active_world: Option<usize>,
    layer: usize,
    viewport: Viewport,
    assets: Assets,
    hidden: HashSet<i64>,
    world: bool,
    grid: bool,
    details: bool,
    single_layer: bool,
    zen: bool,
    int_value: i32,
    tile_id: i32,
    entity_uid: Option<i64>,
    entity_index: Option<usize>,
    flips: i32,
    dirty: bool,
    fit: bool,
    stroke: bool,
    stroke_erase: bool,
    last_cell: Option<[i32; 2]>,
    panel: Option<Panel>,
    pending: Option<Action>,
    suggest_backups: bool,
    layer_notification: Option<(String, String, Instant)>,
    loading: Option<Receiver<Result<services::LoadedProject, String>>>,
    saving: Option<Receiver<Result<services::SavedProject, String>>>,
    exporting: Option<Receiver<Result<ldtk_core::exports::ExportReport, String>>>,
    external_changes: Option<Vec<PathBuf>>,
    status: String,
    allow_close: bool,
    frame_time: f32,
    visible_tiles: usize,
    new_name: String,
    new_kind: String,
    new_grid: i32,
    settings: settings::Settings,
    clipboard: Option<Value>,
    selection: Option<[i32; 4]>,
    stamp: Option<Value>,
    random_tiles: bool,
    tile_stacking: bool,
    render_auto: bool,
    tile_tags: bool,
    gesture: Option<tools::Gesture>,
    tile_rect: [i32; 4],
    atlas_selection: Option<(i64, Vec<i32>)>,
    definition_uid: Option<i64>,
    field_owner: Option<i64>,
    field_uid: Option<i64>,
    rule_group_uid: Option<i64>,
    rule_uid: Option<i64>,
    command_palette: bool,
    search: String,
    status_error: bool,
    file_snapshot: Option<ldtk_core::FilesystemSnapshot>,
    last_file_check: Instant,
    command_result: Option<Receiver<command_tools::CommandEvent>>,
    command_cancel: Option<mpsc::Sender<()>>,
    command_output: std::collections::VecDeque<(String, bool)>,
    command_open: bool,
    command_title: String,
    command_cancelled: bool,
    hook_flow: Option<hooks::HookFlow>,
    resource_checks: std::collections::HashMap<PathBuf, Vec<i64>>,
    repaint: Option<egui::Context>,
    search_index: usize,
    search_focus: bool,
    space_pressed: Option<Instant>,
    space_dragged: bool,
    painted_cells: HashSet<[i32; 2]>,
    random_state: u64,
    world_depth: i64,
    field_pick: Option<field_tools::FieldPick>,
    chain_entity: Option<String>,
    selection_group: Option<Value>,
    crash: Option<(String, Option<PathBuf>)>,
    last_chance: Option<(String, Instant)>,
    resource_missing: Vec<(PathBuf, Vec<i64>, String)>,
    level_resize: Option<level_resize::LevelResize>,
    preview_tile: Option<i32>,
    clicktrap: bool,
    focus_frames: Option<u8>,
    ui_commit: ui_commit::UiCommit,
    world_context: world_context::WorldContext,
}

impl EditorApp {
    #[allow(
        dead_code,
        reason = "Native acceptance harness uses the shared app module"
    )]
    pub fn set_capture_view(&mut self, center: [f32; 2], zoom: f32, world: bool) {
        self.viewport.center = center;
        self.viewport.zoom = zoom;
        self.world = world;
        self.fit = false;
    }
    #[allow(
        dead_code,
        reason = "Read-only evidence from native acceptance harness"
    )]
    pub fn capture_state(&self) -> Value {
        use std::hash::{Hash, Hasher};
        let layer = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer));
        let mut digest = std::collections::hash_map::DefaultHasher::new();
        layer.map(|l| l["intGridCsv"].to_string()).hash(&mut digest);
        let gesture = self.gesture.as_ref().map(|gesture| match gesture {
            tools::Gesture::Rectangle { .. } => "Rectangle",
            tools::Gesture::SelectionMove { .. } => "SelectionMove",
            tools::Gesture::EntityMove { .. } => "EntityMove",
            tools::Gesture::EntityResize { .. } => "EntityResize",
            tools::Gesture::LevelMove { .. } => "LevelMove",
        });
        serde_json::json!({"panel":self.panel.map(|panel|format!("{panel:?}")),"gesture":gesture,"stroke":self.stroke,"ready":self.capture_ready(),"clicktrap":self.clicktrap,"world":self.world,"dirty":self.dirty,"view":{"center":self.viewport.center,"zoom":self.viewport.zoom},"level":self.selected.and_then(|key|self.project.as_ref()?.level(key)).map(|l|l["identifier"].clone()),"layer":layer.map(|l|l["__identifier"].clone()),"layerType":layer.map(|l|l["__type"].clone()),"intGridNonzero":layer.and_then(|l|l["intGridCsv"].as_array()).map(|cells|cells.iter().filter(|v|v.as_i64().is_some_and(|v|v!=0)).count()),"intGridChecksum":format!("{:x}",digest.finish()),"entityCount":layer.and_then(|l|l["entityInstances"].as_array()).map(Vec::len)})
    }
    pub fn capture_ready(&self) -> bool {
        self.project.is_some()
            && !self.is_busy()
            && self.assets.stats().pending == 0
            && self.resource_checks.is_empty()
    }
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let mut app = Self::empty();
        canvas::configure_gpu(cc.wgpu_render_state.as_ref());
        app.repaint = Some(cc.egui_ctx.clone());
        crate::platform_files::set_context(&cc.egui_ctx);
        app.settings = settings::Settings::load();
        app.random_state ^= std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        app.grid = app.settings.bool("grid");
        app.details = app.settings.bool("showDetails");
        app.zen = app.settings.bool("zenMode");
        app.single_layer = app.settings.bool("singleLayerMode");
        app.tile_stacking = app.settings.bool("tileStacking");
        app.tile_tags = app.settings.bool("tileEnumOverlays");
        cc.egui_ctx
            .set_zoom_factor(app.settings.float("appUiScale", 1.0));
        if app.settings.bool("startFullScreen") {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
        let path = path.or_else(|| {
            app.settings
                .bool("openLastProject")
                .then(|| {
                    app.settings.value["lastProject"]["filePath"]
                        .as_str()
                        .map(PathBuf::from)
                })
                .flatten()
        });
        if let Some(path) = path {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ogmo"))
            {
                app.import_ogmo_async(&cc.egui_ctx, path);
            } else {
                app.open_async(&cc.egui_ctx, path);
            }
        }
        app.initialize_updater(&cc.egui_ctx);
        app
    }

    fn empty() -> Self {
        Self {
            project: None,
            history: History::default(),
            selected: None,
            active_world: None,
            layer: 0,
            viewport: Viewport::default(),
            assets: Assets::default(),
            hidden: HashSet::new(),
            world: false,
            grid: true,
            details: true,
            single_layer: false,
            zen: false,
            int_value: 1,
            tile_id: 0,
            entity_uid: None,
            entity_index: None,
            flips: 0,
            dirty: false,
            fit: true,
            stroke: false,
            stroke_erase: false,
            last_cell: None,
            panel: None,
            pending: None,
            suggest_backups: false,
            layer_notification: None,
            loading: None,
            saving: None,
            exporting: None,
            external_changes: None,
            status: "Open a project or create a new one.".into(),
            allow_close: false,
            frame_time: 0.0,
            visible_tiles: 0,
            new_name: String::new(),
            new_kind: "IntGrid".into(),
            new_grid: 16,
            settings: settings::Settings::default(),
            clipboard: None,
            selection: None,
            stamp: None,
            random_tiles: false,
            tile_stacking: true,
            render_auto: true,
            tile_tags: false,
            gesture: None,
            tile_rect: [0, 0, 1, 1],
            atlas_selection: None,
            definition_uid: None,
            field_owner: None,
            field_uid: None,
            rule_group_uid: None,
            rule_uid: None,
            command_palette: false,
            search: String::new(),
            status_error: false,
            file_snapshot: None,
            last_file_check: Instant::now(),
            command_result: None,
            command_cancel: None,
            command_output: Default::default(),
            command_open: false,
            command_title: String::new(),
            command_cancelled: false,
            hook_flow: None,
            resource_checks: Default::default(),
            repaint: None,
            search_index: 0,
            search_focus: false,
            space_pressed: None,
            space_dragged: false,
            painted_cells: HashSet::new(),
            random_state: 0x6a09e667f3bcc909,
            world_depth: 0,
            field_pick: None,
            chain_entity: None,
            selection_group: None,
            crash: None,
            last_chance: None,
            resource_missing: Vec::new(),
            level_resize: None,
            preview_tile: None,
            clicktrap: false,
            focus_frames: None,
            ui_commit: ui_commit::UiCommit::default(),
            world_context: world_context::WorldContext::default(),
        }
    }

    fn open_async(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.status = format!("Opening {}…", path.display());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let project = Project::load(&path).map_err(|e| e.to_string())?;
                let loaded = services::prepare_project(project, Vec::new())?;
                Ok(loaded)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn set_project(&mut self, project: Project) {
        let closing_settings = self.panel == Some(Panel::Settings);
        self.suggest_backups = project_dialogs::suggest_backups(&project);
        self.layer_notification = None;
        self.selected = project.level_keys().first().copied();
        if self.settings.bool("openLastProject")
            && project.path.as_ref().is_some_and(|p| {
                self.settings.value["lastProject"]["filePath"].as_str() == p.to_str()
            })
            && let Some(uid) = self.settings.value["lastProject"]["levelUid"].as_i64()
        {
            self.selected = project
                .level_keys()
                .into_iter()
                .find(|key| {
                    project
                        .level(*key)
                        .is_some_and(|l| l["uid"].as_i64() == Some(uid))
                })
                .or(self.selected);
        }
        self.world = false;

        self.active_world = self.selected.and_then(|k| k.world).or_else(|| {
            project.json["worlds"]
                .as_array()
                .filter(|a| !a.is_empty())
                .map(|_| 0)
        });
        self.world_depth = self
            .selected
            .and_then(|key| project.level(key))
            .and_then(|l| l["worldDepth"].as_i64())
            .unwrap_or(0);
        self.project = Some(project);
        self.layer = 0;
        self.history = History::default();
        self.assets.clear();
        self.resource_checks.clear();
        self.resource_missing.clear();
        self.hidden.clear();
        self.dirty = false;
        self.stroke = false;
        self.last_cell = None;
        self.fit = true;
        self.panel = None;
        self.entity_index = None;
        self.selection = None;
        self.stamp = None;
        self.gesture = None;
        self.world_context = world_context::WorldContext::default();
        self.level_resize = None;
        self.definition_uid = None;
        self.field_uid = None;
        self.field_pick = None;
        self.chain_entity = None;
        self.selection_group = None;
        self.last_chance = None;
        self.atlas_selection = None;
        self.preview_tile = None;
        self.file_snapshot = self
            .project
            .as_ref()
            .and_then(|p| p.filesystem_snapshot().ok());
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
        self.status =
            "Project opened. Left: draw · Right: erase · Space + drag: pan · Wheel: zoom".into();
        if closing_settings {
            self.finish_settings_dialog(&self.repaint.clone().unwrap_or_default());
        }
    }

    fn finish_stroke(&mut self) {
        if self.stroke {
            if let Some(project) = &self.project {
                self.dirty |= self.history.commit(project);
            }
            self.stroke = false;
            self.last_cell = None;
        }
        self.painted_cells.clear();
    }

    fn request(&mut self, ctx: &egui::Context, action: Action) {
        if world_form_dialog::is_open(ctx) {
            return;
        }
        if self.defer_ui_action(ctx, ui_commit::DeferredUiAction::Request(action.clone())) {
            return;
        }
        if self.is_busy() || self.pending.is_some() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        if self.dirty {
            self.pending = Some(action);
        } else {
            self.execute(ctx, action);
        }
    }

    fn execute(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::InstallUpdate => self.install_pending_update(ctx),
            Action::New => {
                self.new_project_dialog(ctx, None);
            }
            Action::NewInDir(dir) => self.new_project_dialog(ctx, Some(dir)),
            Action::BrowseInDir(dir) => self.browse_project_dialog(ctx, Some(dir)),
            Action::Browse => self.browse_project_dialog(ctx, None),
            Action::Open(path) => self.open_async(ctx, path),
            Action::BrowseOgmo => self.browse_ogmo(ctx),
            Action::ImportOgmo(path) => self.import_ogmo_async(ctx, path),
            Action::OpenBackup { backup, original } => {
                self.backup_async(ctx, backup, original, false)
            }
            Action::RestoreBackup { backup, original } => {
                self.backup_async(ctx, backup, original, true)
            }
            Action::RenameProject(name) => self.rename_async(ctx, name),
            Action::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Close => {
                if self.panel == Some(Panel::Settings) {
                    self.finish_settings_dialog(ctx);
                }
                self.project = None;
                self.suggest_backups = false;
                self.layer_notification = None;
                self.selected = None;
                self.active_world = None;
                self.dirty = false;
                self.history = History::default();
                self.panel = None;
                self.stamp = None;
                self.selection = None;
            }
        }
    }

    fn new_project_dialog(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("LDtk project", &["ldtk"])
            .set_file_name("World.ldtk");
        if let Some(dir) = dir.or_else(|| self.settings.ui_dir(None, "NewProject", None)) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(mut path) = dialog.save_file() {
            path.set_extension("ldtk");
            if let Some(dir) = path.parent() {
                self.settings.store_ui_dir(None, "NewProject", dir);
            }
            self.create_project_async(ctx, path);
        }
    }

    fn browse_project_dialog(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        let mut dialog = rfd::FileDialog::new().add_filter("LDtk project", &["ldtk", "json"]);
        if let Some(dir) = dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.open_async(ctx, path);
        }
    }

    fn save(&mut self, save_as: bool) -> bool {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(&ctx, ui_commit::DeferredUiAction::Save(save_as)) {
            return false;
        }
        if self.is_busy() {
            return false;
        }
        self.finish_gesture();
        self.finish_stroke();
        let Some(project) = &self.project else {
            return false;
        };
        let path = if !save_as && project.path.is_some() {
            project.path.clone()
        } else {
            let mut dialog = rfd::FileDialog::new().add_filter("LDtk project", &["ldtk"]);
            if let Some(path) = &project.path {
                if let Some(parent) = path.parent() {
                    dialog = dialog.set_directory(parent);
                }
                if let Some(name) = path.file_name() {
                    dialog = dialog.set_file_name(name.to_string_lossy());
                }
            } else {
                dialog = dialog.set_file_name("World.ldtk");
            }
            dialog.save_file()
        };
        let Some(path) = path else {
            return false;
        };
        let conflicts = self.changed_document_files();
        if !conflicts.is_empty() {
            self.external_changes = Some(conflicts);
            return false;
        }
        let ctx = self.repaint.clone().unwrap_or_default();
        self.queue_project_hooks(&ctx, "BeforeSave", hooks::Continuation::Save(path));
        false
    }

    fn canvas_input(&mut self, ctx: &egui::Context, rect: Rect, response: &egui::Response) {
        if !self.world_context_menu(ctx, rect, response) {
            self.canvas_controls(ctx, rect, response);
        }
    }

    fn edit_entity(&mut self, local: [f32; 2], erase: bool) {
        if !self.details {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &mut self.project else {
            return;
        };
        if erase {
            self.chain_entity = None;
            self.selection_group = None;
            let index = project
                .layer(key, self.layer)
                .and_then(|l| hit_entity(project, l, local).map(|e| e.0));
            if let Some(index) = index {
                let entity =
                    project.layer(key, self.layer).unwrap()["entityInstances"][index].clone();
                self.history
                    .mark_entity_change(project, key, self.layer, &entity);
                match project.remove_entity(key, self.layer, index) {
                    Ok(changed) => {
                        self.dirty |= changed;
                        if changed {
                            self.bleep_entity(key, self.layer, &entity);
                            self.entity_index = None;
                            if self.panel == Some(Panel::Entity) {
                                self.panel = None;
                            }
                        }
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
        } else if let Some(uid) = self.entity_uid {
            let grid = project
                .layer(key, self.layer)
                .map(|l| number(l, "__gridSize", 16.0))
                .unwrap_or(16.0);
            let Some(def) = project.json["defs"]["entities"]
                .as_array()
                .and_then(|a| a.iter().find(|d| d["uid"].as_i64() == Some(uid)))
            else {
                return;
            };
            let pos = ldtk_core::snapped_entity_position(def, local, grid as i32, self.grid);
            let prevent_message = format!(
                "You cannot have more than {} {}.",
                number(def, "maxCount", 0.) as i32,
                text(def, "identifier", "entities")
            );
            match project.add_entity(key, self.layer, uid, pos[0], pos[1]) {
                Ok(changed) => {
                    self.dirty |= changed;
                    if !changed {
                        self.status = prevent_message;
                        self.status_error = true;
                    }
                    if changed
                        && let Some(index) = project
                            .layer(key, self.layer)
                            .and_then(|l| l["entityInstances"].as_array())
                            .map(|a| a.len().saturating_sub(1))
                    {
                        self.history.mark_entity_change(
                            project,
                            key,
                            self.layer,
                            &project.layer(key, self.layer).unwrap()["entityInstances"][index],
                        );
                        self.entity_index = Some(index);
                        self.try_chain_to(key, self.layer, index, true, self.entity_uid);
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.is_busy() {
            return;
        }
        if self.pending.is_some() {
            self.show_unsaved_changes_dialog(ctx);
            return;
        }
        if self.suggest_backups {
            self.show_backup_suggestion(ctx);
            return;
        }
        let Some(panel) = self.panel else {
            return;
        };
        if panel == Panel::Entity {
            self.show_entity_inspector(ctx);
            return;
        }
        if panel == Panel::Settings {
            self.show_settings_dialog(ctx);
            return;
        }
        if self.source_panel(ctx, panel) {
            return;
        }
        let title = match panel {
            Panel::Project => "Project settings",
            Panel::Level => "Current level",
            Panel::Layers => "Layers",
            Panel::Entities => "Entities",
            Panel::Enums => "Enums",
            Panel::Tilesets => "Tilesets",
            Panel::Help => "Help & shortcuts",
            Panel::Entity => "Entity instance",
            Panel::World => "World",
            Panel::Fields => "Custom fields",
            Panel::Rules => "Auto-layer rules",
            Panel::Settings => "App settings",
            Panel::Export => "Export & commands",
        };
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .default_width(420.0)
            .default_pos([340.0, 70.0])
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(650.0)
                    .show(ui, |ui| {
                        if panel == Panel::Help {
                            self.help_contents(ui);
                            return;
                        }
                        self.properties(ui, panel);
                    });
            });
        if !open {
            self.panel = None;
        }
    }

    fn normalize_palette(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|key| project.layer(key, self.layer)) else {
            return;
        };
        match text(layer, "__type", "") {
            "IntGrid" => {
                let values = layer["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid))
                    .and_then(|d| d["intGridValues"].as_array());
                if self.int_value != 0
                    && !values.is_some_and(|vs| {
                        vs.iter()
                            .any(|v| v["value"].as_i64() == Some(self.int_value as i64))
                    })
                {
                    self.int_value = values
                        .and_then(|vs| vs.first())
                        .and_then(|v| v["value"].as_i64())
                        .unwrap_or(0) as i32;
                }
            }
            "Entities" => self.normalize_entity_palette(),
            "Tiles" => {
                if let Some(def) = layer["__tilesetDefUid"]
                    .as_i64()
                    .and_then(|uid| project.tileset(uid))
                {
                    let count = number(def, "__cWid", 0.0) * number(def, "__cHei", 0.0);
                    if self.tile_id < 0 || self.tile_id as f32 >= count {
                        self.tile_id = 0;
                        self.flips = 0;
                    }
                }
            }
            _ => {}
        }
    }
}

impl EditorApp {
    fn run_command(&mut self, command: String, cwd: Option<PathBuf>) {
        let ctx = self.repaint.clone().unwrap_or_default();
        self.queue_hook_commands(
            &ctx,
            std::collections::VecDeque::from([command]),
            cwd,
            hooks::Continuation::None,
        );
    }
}

impl EditorApp {
    fn crash_fallback(&self) -> PathBuf {
        self.settings
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("crash-recovery")
    }
    fn update_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.update_installing(ctx) {
            self.update_dialog(ctx);
            return;
        }
        let started = Instant::now();
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("source-colorblind-palette"),
                self.settings.bool("colorBlind"),
            )
        });
        self.assets.poll(ctx);
        self.poll_command(ctx);
        self.poll_hooks(ctx);
        self.poll_services(ctx);
        self.pointer_lifecycle(ctx);
        if !self.is_busy()
            && self.pending.is_none()
            && !self.ui_commit.pending()
            && !world_form_dialog::is_open(ctx)
            && !world_form_dialog::is_open(ctx)
            && let Some(path) = crate::platform_files::take_open_file()
        {
            self.request(ctx, Action::Open(path));
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.dirty || ui_commit::has_pending_ui_edit(ctx))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request(ctx, Action::Quit);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            let ogmo = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ogmo"));
            self.request(
                ctx,
                if ogmo {
                    Action::ImportOgmo(path)
                } else {
                    Action::Open(path)
                },
            );
        }
        self.shortcuts(ctx);
        self.sidebar(ctx);
        self.normalize_palette();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::DARK))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let rect = if self.panel == Some(Panel::World) {
                    Rect::from_min_max(
                        egui::pos2(
                            rect.left()
                                .max(panels::source_panel_width(Panel::World).min(rect.right())),
                            rect.top(),
                        ),
                        rect.max,
                    )
                } else {
                    rect
                };
                entity_inspector::remember_canvas(ctx, rect);
                if self.project.is_some() {
                    self.canvas_input(ctx, rect, &response);
                    if let (Some(project), Some(key)) = (&self.project, self.selected) {
                        self.visible_tiles = canvas::draw(
                            ctx,
                            ui.painter(),
                            rect,
                            project,
                            &self.viewport,
                            &RenderOptions {
                                selected: key,
                                active_layer: self.layer,
                                world: self.world,
                                grid: self.grid,
                                details: self.details,
                                single_layer: self.single_layer,
                                hidden: &self.hidden,
                                render_auto: self.render_auto,
                                tile_tags: self.tile_tags,
                                ui_scale: self.settings.float("editorUiScale", 1.0),
                                single_layer_intensity: self
                                    .settings
                                    .float("singleLayerModeIntensity", 0.75),
                                fields_table: self.settings.value["fieldsRender"] == "FR_Table",
                                world_depth: Some(self.world_depth),
                                nearby_dist: (64.0
                                    * self.settings.float("nearbyTilesRenderingDist", 1.0))
                                .ceil(),
                                parallax: true,
                                rulers: true,
                                guides: self.grid,
                            },
                            &mut self.assets,
                        );
                        self.draw_brush_preview(ui, rect, response.hovered());
                        self.draw_tool_overlay(ui, rect);
                        self.draw_field_pick_targets(ui, rect);
                        self.draw_level_resize_preview(ui, rect);
                        self.draw_resize_handles(ui, rect);
                        self.draw_bleeps(ui, rect);
                        self.canvas_toolbar(ctx, rect);
                        if response.hovered()
                            && !self.world
                            && let Some(pointer) = response.hover_pos()
                        {
                            let p = self.viewport.to_world(
                                pointer.into(),
                                rect.min.into(),
                                rect.size().into(),
                            );
                            ui.painter().text(
                                rect.right_bottom() - egui::vec2(12.0, 12.0),
                                Align2::RIGHT_BOTTOM,
                                format!("{}, {} px", p[0].floor(), p[1].floor()),
                                FontId::monospace(12.0),
                                theme::TEXT,
                            );
                        }
                    }
                    self.world_context_overlay(ui, rect);
                } else {
                    self.library_home(ctx, ui, rect);
                }
                self.canvas_notifications(ctx, rect);
            });
        if self.panel == Some(Panel::World) && !self.world {
            self.show_panel(None);
        }
        self.dialogs(ctx);
        self.world_form_dialog(ctx);
        self.service_dialogs(ctx);
        self.palette_dialog(ctx);
        self.command_dialog(ctx);
        self.trust_dialog(ctx);
        self.enum_sync_dialog(ctx);
        self.resource_dialog(ctx);
        self.changelog_dialog(ctx);
        self.update_dialog(ctx);
        self.remember_selected_level();
        self.focus_overlay(ctx);
        self.flush_ui_action(ctx);
        app_log::record(
            ctx,
            self.settings.path.as_deref(),
            &self.status,
            self.status_error,
        );
        let title = format!("{}LevelEditor", if self.dirty { "● " } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.frame_time = frame
            .info()
            .cpu_usage
            .map(|s| s * 1000.0)
            .unwrap_or(started.elapsed().as_secs_f32() * 1000.0);
        crate::updater::mark_started();
    }
}

impl Drop for EditorApp {
    fn drop(&mut self) {
        if std::thread::panicking() && self.crash.is_none() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.capture_crash(
                    "Native runtime panicked outside the editor update".into(),
                    self.crash_fallback(),
                )
            }));
        }
        if let Some(cancel) = &self.command_cancel {
            let _ = cancel.send(());
        }
    }
}

impl eframe::App for EditorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.ui_commit_raw_input(ctx, input);
    }
    fn on_exit(&mut self) {
        if let Some(cancel) = &self.command_cancel {
            let _ = cancel.send(());
        }
        let until = Instant::now() + std::time::Duration::from_secs(1);
        if let Some(ctx) = self.repaint.clone() {
            self.shutdown_updater(&ctx);
            while self.command_result.is_some() && Instant::now() < until {
                self.poll_command(&ctx);
                std::thread::yield_now();
            }
        }
        self.persist_preferences();
        app_log::flush();
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.crash.is_some() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("LDtk encountered an unexpected error.");
            });
            self.crash_dialog(ctx);
            return;
        }
        if let Err(error) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.update_frame(ctx, frame)
        })) {
            let message = error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "Unexpected native error".into());
            self.capture_crash(message, self.crash_fallback());
            ctx.request_repaint();
        }
    }
}

fn hit_entity<'a>(
    project: &Project,
    layer: &'a Value,
    pos: [f32; 2],
) -> Option<(usize, &'a Value)> {
    layer["entityInstances"]
        .as_array()?
        .iter()
        .enumerate()
        .rev()
        .find(|(_, entity)| selection_click::entity_is_over(project, entity, pos))
}

fn color_field(ui: &mut egui::Ui, label: &str, value: &mut Value, key: &str) -> bool {
    let c = theme::color(value[key].as_str().unwrap_or("#40465B"));
    let mut rgb = [c.r(), c.g(), c.b()];
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.color_edit_button_srgb(&mut rgb).changed();
    });
    if changed {
        value[key] = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]).into();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_resources_resolve_from_executable_without_the_source_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join("LDtk Rust.app/Contents/MacOS/ldtk-rs");
        let resources = directory.path().join("LDtk Rust.app/Contents/Resources");
        let asset = resources.join("assets/CHANGELOG.md");
        let sample = resources.join("examples/original/Entities.ldtk");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
        std::fs::create_dir_all(sample.parent().unwrap()).unwrap();
        std::fs::write(&asset, "bundled changelog").unwrap();
        std::fs::write(&sample, "bundled example").unwrap();
        assert_eq!(
            std::fs::read_to_string(resource_path_in("assets/CHANGELOG.md", Some(&executable)))
                .unwrap(),
            "bundled changelog"
        );
        assert_eq!(
            std::fs::read_to_string(resource_path_in(
                "examples/original/Entities.ldtk",
                Some(&executable)
            ))
            .unwrap(),
            "bundled example"
        );
        let portable_executable = directory.path().join("portable/ldtk-rs");
        let portable_asset = directory
            .path()
            .join("portable/resources/assets/CHANGELOG.md");
        std::fs::create_dir_all(portable_asset.parent().unwrap()).unwrap();
        std::fs::write(&portable_asset, "portable changelog").unwrap();
        assert_eq!(
            resource_path_in("assets/CHANGELOG.md", Some(&portable_executable)),
            portable_asset
        );
        assert_eq!(
            resource_path_in("assets/CHANGELOG.md", None),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/CHANGELOG.md")
        );
    }

    fn editor() -> EditorApp {
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        app.set_project(project);
        app.fit = false;
        app.viewport = Viewport {
            center: [128.0, 128.0],
            zoom: 1.0,
        };
        app
    }

    fn press_canvas(app: &mut EditorApp) {
        let ctx = egui::Context::default();
        let pointer = egui::pos2(128.0, 128.0);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256.0, 256.0),
                )),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                    app.canvas_input(ctx, rect, &response);
                });
            },
        );
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(256.0, 256.0),
            )),
            events: vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                app.canvas_input(ctx, rect, &response);
            });
        });
        app.finish_stroke();
    }

    #[test]
    fn an_unobstructed_canvas_accepts_a_paint_stroke() {
        let mut app = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        press_canvas(&mut app);
        assert!(app.project.as_ref().unwrap().json != before);
        assert!(app.history.can_undo());
    }

    #[test]
    fn unsaved_dialog_blocks_edits_to_the_underlying_map() {
        let mut app = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        app.pending = Some(Action::New);
        press_canvas(&mut app);
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn loading_another_project_freezes_current_edits() {
        let mut app = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let (_tx, rx) = mpsc::channel();
        app.loading = Some(rx);
        press_canvas(&mut app);
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn property_changes_are_undoable_without_losing_the_previous_stroke() {
        let mut app = editor();
        let key = app.selected.unwrap();
        press_canvas(&mut app);
        app.edit("Level renamed", |p| {
            p.level_mut(key).unwrap()["identifier"] = "Renamed".into();
            Ok(())
        });
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_ne!(
            app.project.as_ref().unwrap().level(key).unwrap()["identifier"],
            "Renamed"
        );
        assert!(app.history.can_undo());
    }

    #[test]
    fn switching_intgrid_layers_selects_a_value_defined_in_the_target() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let uid = project.add_layer("Other", "IntGrid", 16).unwrap();
        app.layer = project.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"].as_i64() == Some(uid))
            .unwrap();
        app.int_value = 2;
        app.normalize_palette();
        assert_eq!(app.int_value, 1);
        press_canvas(&mut app);
        let cells = app.project.as_ref().unwrap().layer(key, app.layer).unwrap()["intGridCsv"]
            .as_array()
            .unwrap();
        assert!(cells.iter().any(|v| v.as_i64() == Some(1)));
        assert!(!cells.iter().any(|v| v.as_i64() == Some(2)));
    }

    #[test]
    fn a_canvas_edit_saves_and_reopens_through_the_app_save_action() {
        let mut app = editor();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("World.ldtk");
        app.project.as_mut().unwrap().save(&path).unwrap();
        press_canvas(&mut app);
        assert!(app.dirty);
        let expected = app.project.as_ref().unwrap().json.clone();
        app.save(false);
        let ctx = egui::Context::default();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while app.saving.is_some() && Instant::now() < deadline {
            app.poll_services(&ctx);
            std::thread::yield_now();
        }
        assert!(app.saving.is_none(), "Background save did not complete");
        assert!(!app.dirty);
        let reopened = Project::load(&path).unwrap();
        assert_eq!(reopened.json, expected);
    }

    #[test]
    fn flood_fill_only_changes_connected_cells_and_undoes_as_one_stroke() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        for y in 0..16 {
            project.paint_int_grid(key, 0, 1, y, 1).unwrap();
        }
        app.int_value = 1;
        app.history.begin(project, key);
        app.stroke = true;
        app.fill([0, 0], false);
        app.finish_stroke();
        let cells = app.project.as_ref().unwrap().layer(key, 0).unwrap()["intGridCsv"]
            .as_array()
            .unwrap();
        assert_eq!(cells[0], 1);
        assert_eq!(cells[2], 0);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["intGridCsv"][0],
            0
        );
    }

    #[test]
    fn rectangle_brush_obeys_flips_and_large_tiles() {
        assert_eq!(
            tools::stamp_cells([3, 4], [2, 5, 2, 2], 2, 1),
            vec![
                ([5, 4], [2, 5]),
                ([3, 4], [3, 5]),
                ([5, 6], [2, 6]),
                ([3, 6], [3, 6])
            ]
        );
        assert_eq!(
            tools::rectangle_tiles([0, 0, 5, 3], [2, 5, 2, 2], 2),
            vec![
                ([0, 0], [2, 5]),
                ([2, 0], [3, 5]),
                ([4, 0], [2, 5]),
                ([0, 2], [2, 6]),
                ([2, 2], [3, 6]),
                ([4, 2], [2, 6])
            ]
        );
    }

    #[test]
    fn displaying_a_null_tile_field_does_not_assign_a_tile() {
        let ctx = egui::Context::default();
        let project = Project::new();
        let mut result = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                result = ui_fields::scalar_editor(
                    ui,
                    &project,
                    "Tile",
                    &Value::Null,
                    &serde_json::json!({"canBeNull":false}),
                );
            });
        });
        assert!(
            result.is_none(),
            "Displaying a field must not count as editing it"
        );
    }

    #[test]
    fn tile_bucket_matches_underlying_tiles_and_erases_entire_stacks() {
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let key = p.level_keys()[0];
        let layer = p.add_layer("Tiles", "Tiles", 16).unwrap();
        let atlas = p
            .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
            .unwrap();
        p.assign_tileset(layer, atlas).unwrap();
        p.paint_tile_stacked(key, 0, 0, 0, 0, [0, 0], 0, false, true)
            .unwrap();
        p.paint_tile_stacked(key, 0, 0, 0, 1, [16, 0], 0, false, true)
            .unwrap();
        p.paint_tile_stacked(key, 0, 1, 0, 1, [16, 0], 0, false, true)
            .unwrap();
        p.paint_tile_stacked(key, 0, 1, 0, 0, [0, 0], 0, false, true)
            .unwrap();
        app.set_project(p);
        app.fill([0, 0], true);
        assert!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn navigation_keys_choose_palette_values_without_panning() {
        let mut app = editor();
        let uid = app.project.as_ref().unwrap().json["defs"]["layers"][0]["uid"]
            .as_i64()
            .unwrap();
        app.project.as_mut().unwrap().update_layer_properties(uid,&serde_json::json!({"intGridValues":[{"value":1,"identifier":"A","color":"#FFFFFF","groupUid":0},{"value":2,"identifier":"B","color":"#FFFFFF","groupUid":0}]})).unwrap();
        let before = app.viewport.center;
        app.navigate_palette(0, 1);
        assert_eq!(app.int_value, 2);
        assert_eq!(app.viewport.center, before);
        app.navigate_palette(-1, 0);
        assert_eq!(app.int_value, 1);
    }

    #[test]
    fn consecutive_entity_placements_auto_chain_their_reference_field() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Waypoint", 16, 16).unwrap();
        p.add_field_definition(Some(uid), "Next", "EntityRef", false)
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(uid);
        app.edit_entity([8.0, 8.0], false);
        app.edit_entity([24.0, 8.0], false);
        let entities = app
            .project
            .as_ref()
            .unwrap()
            .layer(app.selected.unwrap(), 0)
            .unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"]["entityIid"],
            entities[1]["iid"]
        );
    }

    #[test]
    fn select_all_copies_and_pastes_visible_layers_together_as_one_undo() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let p = app.project.as_mut().unwrap();
        let uid = p.add_layer("Objects", "Entities", 16).unwrap();
        let entity = p.add_entity_def("Marker", 16, 16).unwrap();
        let objects = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["layerDefUid"] == uid)
            .unwrap();
        let ground = p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["__type"] == "IntGrid")
            .unwrap();
        p.paint_int_grid(key, ground, 0, 0, 1).unwrap();
        p.add_entity(key, objects, entity, 8, 8).unwrap();
        app.layer = ground;
        app.select_all();
        app.copy_selection(&egui::Context::default(), false);
        assert_eq!(app.clipboard.as_ref().unwrap()["type"], "SelectionGroup");
        app.paste_at([4, 4]);
        let p = app.project.as_ref().unwrap();
        assert_eq!(
            p.layer(key, objects).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(p.layer(key, ground).unwrap()["intGridCsv"][4 + 4 * 16], 1);
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, objects).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn entity_drag_preserves_pivot_and_pointer_cell_phase() {
        assert_eq!(
            entity_tools::drag_position([8, 16], [7.0, 8.0], [17.0, 8.0], 16.0, 1.0),
            [24, 16]
        );
        assert_eq!(
            entity_tools::drag_position([8, 16], [1.0, 1.0], [15.0, 1.0], 16.0, 1.0),
            [8, 16]
        );
        assert_eq!(
            entity_tools::drag_position([8, 16], [7.0, 8.0], [17.0, 8.0], 16.0, 0.5),
            [16, 16]
        );
    }

    fn canvas_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(256.0, 256.0),
                )),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                    app.canvas_input(ctx, rect, &response);
                });
                app.focus_overlay(ctx);
            },
        );
    }

    #[test]
    fn rectangle_commits_the_release_position_even_when_move_and_release_share_a_frame() {
        let mut app = editor();
        let ctx = egui::Context::default();
        let start = egui::pos2(40.0, 40.0);
        let end = egui::pos2(104.0, 104.0);
        let modifiers = egui::Modifiers {
            shift: true,
            ..Default::default()
        };
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start)],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                },
            ],
            modifiers,
        );
        let cells = app
            .project
            .as_ref()
            .unwrap()
            .layer(app.selected.unwrap(), 0)
            .unwrap()["intGridCsv"]
            .as_array()
            .unwrap();
        assert_eq!(
            cells.iter().filter(|v| v.as_i64() == Some(1)).count(),
            25,
            "The final rectangle must cover cells 2..=6"
        );
        assert_eq!(cells[6 + 6 * 16], 1);
        assert_eq!(cells[7 + 7 * 16], 0);
    }

    #[test]
    fn focus_or_pointer_loss_commits_once_and_does_not_resume_a_stroke() {
        for lost in [egui::Event::PointerGone, egui::Event::WindowFocused(false)] {
            let mut app = editor();
            let ctx = egui::Context::default();
            let start = egui::pos2(40.0, 40.0);
            let end = egui::pos2(104.0, 40.0);
            let modifiers = egui::Modifiers::NONE;
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start)],
                modifiers,
            );
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                }],
                modifiers,
            );
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(end)],
                modifiers,
            );
            canvas_frame(&mut app, &ctx, vec![lost], modifiers);
            assert!(!app.stroke && app.gesture.is_none());
            let before = app.project.as_ref().unwrap().json.clone();
            canvas_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::WindowFocused(true),
                    egui::Event::PointerMoved(egui::pos2(184.0, 40.0)),
                ],
                modifiers,
            );
            assert_eq!(app.project.as_ref().unwrap().json, before);
            app.undo_action(false);
            assert!(
                app.project
                    .as_ref()
                    .unwrap()
                    .layer(app.selected.unwrap(), 0)
                    .unwrap()["intGridCsv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v == 0)
            );
        }
    }

    #[test]
    fn returning_from_another_window_consumes_the_first_click() {
        let mut app = editor();
        let ctx = egui::Context::default();
        let pos = egui::pos2(40.0, 40.0);
        let modifiers = egui::Modifiers::NONE;
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos)],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::WindowFocused(false)],
            modifiers,
        );
        assert!(app.clicktrap);
        canvas_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::WindowFocused(true),
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                },
            ],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
        assert!(!app.clicktrap);
        assert!(!app.dirty);
        canvas_frame(&mut app, &ctx, Vec::new(), modifiers);
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
        assert!(app.dirty);
    }

    #[test]
    fn ordinary_stroke_includes_release_frame_motion() {
        let mut app = editor();
        let ctx = egui::Context::default();
        let start = egui::pos2(40.0, 40.0);
        let end = egui::pos2(104.0, 40.0);
        let modifiers = egui::Modifiers::NONE;
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start)],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                },
            ],
            modifiers,
        );
        assert!(!app.stroke);
        assert_eq!(
            app.project
                .as_ref()
                .unwrap()
                .layer(app.selected.unwrap(), 0)
                .unwrap()["intGridCsv"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_i64() == Some(1))
                .count(),
            5
        );
    }

    #[test]
    fn point_click_drag_and_right_delete_do_not_create_or_delete_an_entity() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let field = p
            .add_field_definition(Some(uid), "Target", "Point", false)
            .unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, uid, 16, 16).unwrap();
        let iid = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_value(key, Some(&iid), field, serde_json::json!({"cx":4,"cy":4}))
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(uid);
        app.fit = false;
        app.viewport = Viewport {
            center: [128.0, 128.0],
            zoom: 1.0,
        };
        let ctx = egui::Context::default();
        let start = egui::pos2(72.0, 72.0);
        let end = egui::pos2(104.0, 72.0);
        let modifiers = egui::Modifiers::NONE;
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start)],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                },
            ],
            modifiers,
        );
        let entities = app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(
            entities.len(),
            1,
            "Clicking a point must select it rather than create an entity"
        );
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!({"cx":6,"cy":4})
        );
        app.undo_action(false);
        let entities = app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(
            entities[0]["fieldInstances"][0]["__value"],
            serde_json::json!({"cx":4,"cy":4})
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers,
                },
            ],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
        let entities = app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0]["fieldInstances"][0]["__value"].is_null());
    }

    #[test]
    fn creating_an_entity_immediately_drags_or_resizes_with_one_undo() {
        for resizable in [false, true] {
            let mut p = Project::new();
            p.add_layer("Entities", "Entities", 16).unwrap();
            let uid = p.add_entity_def("Actor", 16, 16).unwrap();
            p.update_entity_definition(
                uid,
                &serde_json::json!({"resizableX":resizable,"resizableY":resizable}),
            )
            .unwrap();
            let mut app = EditorApp::empty();
            app.set_project(p);
            app.entity_uid = Some(uid);
            app.fit = false;
            app.viewport = Viewport {
                center: [128.0; 2],
                zoom: 1.0,
            };
            let ctx = egui::Context::default();
            let start = egui::pos2(40.0, 40.0);
            let end = egui::pos2(104.0, 40.0);
            let modifiers = egui::Modifiers::NONE;
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start)],
                modifiers,
            );
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                }],
                modifiers,
            );
            assert!(app.gesture.is_some());
            canvas_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(end),
                    egui::Event::PointerButton {
                        pos: end,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    },
                ],
                modifiers,
            );
            let entity = &app
                .project
                .as_ref()
                .unwrap()
                .layer(app.selected.unwrap(), 0)
                .unwrap()["entityInstances"][0];
            if resizable {
                assert!(entity["width"].as_i64().unwrap() > 16);
            } else {
                assert_eq!(entity["px"][0], 104);
            }
            app.undo_action(false);
            assert!(
                app.project
                    .as_ref()
                    .unwrap()
                    .layer(app.selected.unwrap(), 0)
                    .unwrap()["entityInstances"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn mixed_selection_move_uses_rendered_layer_scales_and_ignores_brush_flips() {
        let mut p = Project::new();
        let normal = p.add_layer("Normal", "IntGrid", 16).unwrap();
        let scaled = p.add_layer("Scaled", "IntGrid", 16).unwrap();
        p.update_layer_properties(scaled,&serde_json::json!({"parallaxFactorX":0.5,"parallaxFactorY":0.0,"parallaxScaling":true})).unwrap();
        let key = p.level_keys()[0];
        let layers = p.level(key).unwrap()["layerInstances"].as_array().unwrap();
        let scaled_index = layers
            .iter()
            .position(|l| l["layerDefUid"] == scaled)
            .unwrap();
        let normal_index = layers
            .iter()
            .position(|l| l["layerDefUid"] == normal)
            .unwrap();
        p.paint_int_grid(key, scaled_index, 0, 0, 1).unwrap();
        p.paint_int_grid(key, normal_index, 4, 0, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.layer = scaled_index;
        app.viewport = Viewport {
            center: [128.0; 2],
            zoom: 1.0,
        };
        app.flips = 3;
        app.capture_selection([0, 0, 4, 2]);
        assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);
        app.gesture = Some(tools::Gesture::SelectionMove {
            region: [0, 0, 4, 2],
            start: [0, 0],
            end: [2, 0],
            copy: false,
            world_start: [65.0, 1.0],
            world_end: [81.0, 1.0],
            moved: true,
        });
        app.finish_gesture();
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.layer(key, scaled_index).unwrap()["intGridCsv"][2], 1);
        assert_eq!(p.layer(key, normal_index).unwrap()["intGridCsv"][5], 1);
        assert_eq!(p.layer(key, scaled_index).unwrap()["intGridCsv"][0], 0);
        app.undo_action(false);
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.layer(key, scaled_index).unwrap()["intGridCsv"][0], 1);
        assert_eq!(p.layer(key, normal_index).unwrap()["intGridCsv"][4], 1);
    }

    #[test]
    fn reopening_last_project_restores_its_level_uid_and_persists_navigation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("World.ldtk");
        let mut p = Project::new();
        let first = p.level_keys()[0];
        let second = p.add_level(None, "Second", 256, 256).unwrap();
        p.save(&path).unwrap();
        let first_uid = p.level(first).unwrap()["uid"].clone();
        let second_uid = p.level(second).unwrap()["uid"].clone();
        let mut app = EditorApp::empty();
        app.settings.path = Some(dir.path().join("settings.json"));
        app.settings.value["openLastProject"] = true.into();
        app.settings.value["lastProject"] =
            serde_json::json!({"filePath":path,"levelUid":second_uid});
        app.set_project(p);
        assert_eq!(app.selected, Some(second));
        assert_eq!(app.settings.value["lastProject"]["levelUid"], second_uid);
        app.selected = Some(first);
        app.remember_selected_level();
        let saved: Value =
            serde_json::from_slice(&std::fs::read(app.settings.path.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(saved["lastProject"]["levelUid"], first_uid);
    }

    #[test]
    fn painting_outside_a_selection_and_visibility_modes_clear_the_old_selection() {
        let mut app = editor();
        let key = app.selected.unwrap();
        app.project
            .as_mut()
            .unwrap()
            .paint_int_grid(key, 0, 0, 0, 1)
            .unwrap();
        app.capture_selection([0, 0, 1, 1]);
        let ctx = egui::Context::default();
        let pos = egui::pos2(72.0, 72.0);
        let modifiers = egui::Modifiers::NONE;
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(pos)],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            }],
            modifiers,
        );
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            }],
            modifiers,
        );
        assert!(app.selection_group.is_none());
        app.delete_selection();
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["intGridCsv"][0],
            1
        );
        app.capture_selection([0, 0, 1, 1]);
        app.set_single_layer_mode(true);
        assert!(app.selection_group.is_none());
        app.capture_selection([0, 0, 1, 1]);
        app.set_details(false);
        assert!(app.selection_group.is_none());
        app.set_details(true);
        app.capture_selection([0, 0, 1, 1]);
        app.set_empty_selection(false);
        assert!(app.selection_group.is_none());
    }

    #[test]
    fn native_undo_is_per_level_and_structural_recovery_is_separate() {
        let mut app = editor();
        let first = app.selected.unwrap();
        let second = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Second", 256, 256)
            .unwrap();
        for key in [first, second] {
            app.selected = Some(key);
            app.edit_level("Paint", key, |p| p.paint_int_grid(key, 0, 0, 0, 1));
        }
        app.selected = Some(first);
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(first, 0).unwrap()["intGridCsv"][0],
            0
        );
        assert_eq!(
            app.project.as_ref().unwrap().layer(second, 0).unwrap()["intGridCsv"][0],
            1
        );
        let uid = app.project.as_ref().unwrap().json["defs"]["layers"][0]["uid"]
            .as_i64()
            .unwrap();
        app.edit_recoverable("Layer deleted", |p| p.remove_definition("layers", uid));
        assert!(
            !app.history
                .can_undo_level(app.project.as_ref().unwrap(), second)
        );
        app.restore_last_chance();
        assert_eq!(
            app.project.as_ref().unwrap().layer(second, 0).unwrap()["intGridCsv"][0],
            1
        );
        assert!(app.last_chance.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn closing_a_failed_before_save_command_continues_the_source_save_workflow() {
        let mut app = editor();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Commands.ldtk");
        app.project.as_mut().unwrap().json["customCommands"] = serde_json::json!([{"when":"BeforeSave","command":"python3 -c \"raise SystemExit(7)\""}]);
        app.project.as_mut().unwrap().save(&path).unwrap();
        let iid = text(&app.project.as_ref().unwrap().json, "iid", "").to_owned();
        app.settings.set_trusted(&iid, true);
        press_canvas(&mut app);
        app.save(false);
        let ctx = egui::Context::default();
        let until = Instant::now() + std::time::Duration::from_secs(3);
        while Instant::now() < until {
            app.poll_services(&ctx);
            app.poll_command(&ctx);
            app.poll_hooks(&ctx);
            if app.command_open && app.command_result.is_none() {
                break;
            }
            std::thread::yield_now();
        }
        assert!(
            app.command_open,
            "Failure output stays visible until the user closes it"
        );
        assert!(app.dirty);
        app.command_open = false;
        while Instant::now() < until && (app.dirty || app.is_busy()) {
            app.poll_hooks(&ctx);
            app.poll_services(&ctx);
            std::thread::yield_now();
        }
        assert!(
            !app.dirty,
            "Closing the command output must allow saving to continue"
        );
        assert_eq!(
            Project::load(path).unwrap().json,
            app.project.as_ref().unwrap().json
        );
    }

    #[test]
    fn every_native_properties_panel_preserves_the_official_example_until_edited() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::load(resource_path(
            "examples/original/Typical_TopDown_example.ldtk",
        ))
        .unwrap();
        project.migrate_to_current().unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let before = app.project.as_ref().unwrap().json.clone();
        for panel in [
            Panel::Project,
            Panel::World,
            Panel::Level,
            Panel::Layers,
            Panel::Entities,
            Panel::Entity,
            Panel::Fields,
            Panel::Enums,
            Panel::Tilesets,
            Panel::Rules,
            Panel::Settings,
            Panel::Export,
        ] {
            app.definition_uid = None;
            app.field_uid = None;
            app.rule_uid = None;
            app.rule_group_uid = None;
            for _ in 0..2 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.properties(ui, panel));
                    },
                );
            }
            assert_eq!(
                app.project.as_ref().unwrap().json,
                before,
                "Opening panel {} changed the document",
                panel as u8
            );
        }
    }
}
