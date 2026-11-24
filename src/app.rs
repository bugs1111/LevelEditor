#[path = "editor/app_log.rs"]
mod app_log;
#[path = "editor/bake_dialog.rs"]
mod bake_dialog;
#[path = "editor/bleep.rs"]
pub(crate) mod bleep;
#[path = "editor/brush.rs"]
mod brush;
#[path = "editor/canvas_status.rs"]
mod canvas_status;
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
#[path = "editor/file_dialog.rs"]
mod file_dialog;
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
#[path = "editor/locale.rs"]
mod locale;
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
#[path = "editor/save_progress.rs"]
mod save_progress;
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
    viewport::{CameraMotion, Viewport, stroke_cells},
};
use eframe::egui::{
    self, Align2, Color32, FontId, Key, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use file_dialog::FileDialog;
use ldtk_core::{History, LevelKey, Project};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
    time::Instant,
};

fn window_title(
    project: Option<(&Path, Option<&str>)>,
    dirty: bool,
    progress: Option<(usize, usize)>,
) -> String {
    let mut title = String::new();
    if let Some((done, total)) = progress {
        let filled = done
            .min(total)
            .saturating_mul(5)
            .checked_div(total)
            .unwrap_or(5);
        title.push_str(&"▰".repeat(filled));
        title.push_str(&"▱".repeat(5 - filled));
        title.push_str("  ");
    }
    if let Some((path, level)) = project
        && let Some(name) = path.file_stem()
    {
        title.push_str(&name.to_string_lossy());
        if dirty {
            title.push_str(" [UNSAVED]");
        }
        if let Some(level) = level {
            title.push_str("  @ ");
            title.push_str(level);
        }
        title.push_str("    --    ");
    }
    title.push_str("LevelEditor");
    title
}

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

struct DeferredAutoPaint {
    level: LevelKey,
    deadline: f64,
    rules: ldtk_core::DeferredAutoLayers,
    prior_selection: Option<Value>,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum StartupWindow {
    NotYetMapped,
    Mapping(Instant),
    Maximizing(Instant),
    EnteringFullscreen(Instant),
}

type OperationProgress = Arc<Mutex<Option<(usize, usize)>>>;

pub struct EditorApp {
    project: Option<Project>,
    history: History,
    selected: Option<LevelKey>,
    world_selection: Vec<String>,
    world_hover: Option<LevelKey>,
    active_world: Option<usize>,
    layer: usize,
    viewport: Viewport,
    camera_motion: CameraMotion,
    assets: Assets,
    hidden: HashSet<i64>,
    layer_visibility_press: Option<visibility::LayerVisibilityPress>,
    layer_context_menu: Option<(i64, egui::Pos2)>,
    world: bool,
    grid: bool,
    details: bool,
    single_layer: bool,
    zen: bool,
    zen_sidebar: chrome::ZenSidebar,
    int_value: i32,
    tile_id: i32,
    entity_uid: Option<i64>,
    entity_index: Option<usize>,
    flips: i32,
    dirty: bool,
    fit: bool,
    stroke: bool,
    stroke_erase: bool,
    stroke_waiting_for_level: bool,
    last_cell: Option<[i32; 2]>,
    brush_preview_waits_for_pointer: bool,
    entity_eraser_preview: bool,
    neighbor_level_press: bool,
    async_paint: Option<canvas::AsyncPaintPreview>,
    deferred_auto_paint: Option<DeferredAutoPaint>,
    stroke_was_dirty: bool,
    panel: Option<Panel>,
    pending: Option<Action>,
    suggest_backups: bool,
    quick_notification: Option<(String, Option<egui::ImageSource<'static>>, Instant)>,
    #[cfg(test)]
    quick_notification_test_age: Option<std::time::Duration>,
    canvas_notices: Vec<notifications::CanvasNoticeEntry>,
    loading: Option<Receiver<Result<services::LoadedProject, services::LoadFailure>>>,
    loading_progress: Option<OperationProgress>,
    saving_progress: Option<Arc<Mutex<save_progress::SaveProgress>>>,
    exporting_progress: Option<OperationProgress>,
    saving: Option<Receiver<ldtk_core::Result<services::SavedProject>>>,
    save_attempt: Option<PathBuf>,
    save_failure: Option<services::SaveFailure>,
    application_write: Option<project_dialogs::ApplicationWrite>,
    exporting: Option<Receiver<Result<ldtk_core::exports::ExportReport, String>>>,
    external_changes: Option<Vec<PathBuf>>,
    status: String,
    published_window_title: String,
    #[cfg(target_os = "linux")]
    startup_window: Option<StartupWindow>,
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
    open_rule_groups: HashSet<(i64, i64)>,
    rule_uid: Option<i64>,
    last_rule_list_row: Option<i64>,
    rule_draft: Option<panels::RuleDraft>,
    rule_hover: Option<(i64, i64, Option<i64>)>,
    command_palette: bool,
    search: String,
    status_error: bool,
    file_snapshot: Option<ldtk_core::FilesystemSnapshot>,
    external_enum_reloads: std::collections::VecDeque<PathBuf>,
    pending_enum_sync: Option<panels::PendingSync>,
    pending_enum_removal: Option<String>,
    layer_bake: Option<bake_dialog::LayerBake>,
    int_grid_removal: Option<(i64, panels::IntGridRemoval)>,
    pending_auto_layer_rebuild: bool,
    rebuilding_auto_layers: Option<Receiver<ldtk_core::Result<Project>>>,
    auto_layer_rebuild_progress: Option<OperationProgress>,
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
    search_show_all: bool,
    search_entries: Option<search::SearchIndex>,
    space_pressed: Option<Instant>,
    space_dragged: bool,
    space_fit_pending: bool,
    camera_drag: Option<tools::CameraDrag>,
    painted_cells: HashSet<[i32; 2]>,
    random_state: u64,
    world_depth: i64,
    level_form_depth_override: Option<LevelKey>,
    point_pick: Option<field_tools::FieldPick>,
    reference_pick: Option<field_tools::FieldPick>,
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
        self.camera_motion.cancel();
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
            tools::Gesture::WorldRectangle { .. } => "WorldRectangle",
            tools::Gesture::WorldGroupMove { .. } => "WorldGroupMove",
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
        #[cfg(target_os = "linux")]
        {
            app.startup_window = Some(StartupWindow::NotYetMapped);
        }
        canvas::configure_gpu(cc.wgpu_render_state.as_ref());
        app.repaint = Some(cc.egui_ctx.clone());
        crate::platform_files::set_context(&cc.egui_ctx);
        app.settings = settings::Settings::load();
        locale::set(&cc.egui_ctx, app.settings.language());
        app.random_state ^= std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        app.apply_initial_preferences(&cc.egui_ctx);
        if !cfg!(target_os = "linux") && app.settings.bool("startFullScreen") {
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
                        .filter(|path| path.exists())
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

    fn apply_initial_preferences(&mut self, ctx: &egui::Context) {
        self.grid = self.settings.bool("grid");
        self.details = self.settings.bool("showDetails");
        self.zen = self.settings.bool("zenMode");
        self.single_layer = self.settings.bool("singleLayerMode");
        self.tile_stacking = self.settings.bool("tileStacking");
        self.tile_tags = self.settings.bool("tileEnumOverlays");
        // Before the first pass, set the scale so eframe measures the first
        // frame at that scale. A deferred zoom would reuse the default rect.
        ctx.options_mut(|options| {
            options.zoom_factor = self.settings.float("appUiScale", 1.0);
            options.zoom_with_keyboard = false;
        });
    }

    fn empty() -> Self {
        Self {
            project: None,
            history: History::default(),
            selected: None,
            world_selection: Vec::new(),
            world_hover: None,
            active_world: None,
            layer: 0,
            viewport: Viewport::default(),
            camera_motion: CameraMotion::default(),
            assets: Assets::default(),
            hidden: HashSet::new(),
            layer_visibility_press: None,
            layer_context_menu: None,
            world: false,
            grid: true,
            details: true,
            single_layer: false,
            zen: false,
            zen_sidebar: chrome::ZenSidebar::default(),
            int_value: 1,
            tile_id: 0,
            entity_uid: None,
            entity_index: None,
            flips: 0,
            dirty: false,
            fit: true,
            stroke: false,
            stroke_erase: false,
            stroke_waiting_for_level: false,
            last_cell: None,
            brush_preview_waits_for_pointer: false,
            entity_eraser_preview: false,
            neighbor_level_press: false,
            async_paint: None,
            deferred_auto_paint: None,
            stroke_was_dirty: false,
            panel: None,
            pending: None,
            suggest_backups: false,
            quick_notification: None,
            #[cfg(test)]
            quick_notification_test_age: None,
            canvas_notices: Vec::new(),
            loading: None,
            loading_progress: None,
            saving_progress: None,
            exporting_progress: None,
            saving: None,
            save_attempt: None,
            save_failure: None,
            application_write: None,
            exporting: None,
            external_changes: None,
            status: "Open a project or create a new one.".into(),
            published_window_title: String::new(),
            #[cfg(target_os = "linux")]
            startup_window: None,
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
            open_rule_groups: HashSet::new(),
            rule_uid: None,
            last_rule_list_row: None,
            rule_draft: None,
            rule_hover: None,
            command_palette: false,
            search: String::new(),
            status_error: false,
            file_snapshot: None,
            external_enum_reloads: Default::default(),
            pending_enum_sync: None,
            pending_enum_removal: None,
            layer_bake: None,
            int_grid_removal: None,
            pending_auto_layer_rebuild: false,
            rebuilding_auto_layers: None,
            auto_layer_rebuild_progress: None,
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
            search_show_all: false,
            search_entries: None,
            space_pressed: None,
            space_dragged: false,
            space_fit_pending: false,
            camera_drag: None,
            painted_cells: HashSet::new(),
            random_state: 0x6a09e667f3bcc909,
            world_depth: 0,
            level_form_depth_override: None,
            point_pick: None,
            reference_pick: None,
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
        let progress = Arc::new(Mutex::new(Some((0, 1))));
        self.loading_progress = Some(progress.clone());
        self.status = format!("Opening {}…", path.display());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let project = Project::load_with_progress(&path, |done, total| {
                    *progress.lock().unwrap_or_else(|e| e.into_inner()) = Some((done, total));
                    ctx.request_repaint();
                })
                .map_err(|error| {
                    let file_read_missing = matches!(
                        &error,
                        ldtk_core::Error::Io { source, .. }
                            if source.kind() == std::io::ErrorKind::NotFound
                    );
                    if file_read_missing
                        && std::fs::metadata(&path)
                            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    {
                        services::LoadFailure::MissingProject(path)
                    } else {
                        services::LoadFailure::Other(error.to_string())
                    }
                })?;
                let loaded = services::prepare_project(project, Vec::new())
                    .map_err(services::LoadFailure::Other)?;
                let mut progress = progress.lock().unwrap_or_else(|e| e.into_inner());
                let total = progress.map_or(1, |(_, total)| total);
                *progress = Some((total, total));
                Ok(loaded)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    #[cfg(test)]
    fn set_project(&mut self, project: Project) {
        let snapshot = project.filesystem_snapshot().ok();
        self.install_project(project, snapshot);
    }

    fn install_project(
        &mut self,
        project: Project,
        snapshot: Option<ldtk_core::FilesystemSnapshot>,
    ) {
        self.open_rule_groups = project.json["defs"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|layer| {
                let [group] = layer["autoRuleGroups"].as_array()?.as_slice() else {
                    return None;
                };
                (group["usesWizard"] != true)
                    .then_some((layer["uid"].as_i64()?, group["uid"].as_i64()?))
            })
            .collect();
        self.layer_visibility_press = None;
        self.layer_context_menu = None;
        if let Some(ctx) = &self.repaint {
            bleep::reset_error_scans(ctx);
        }
        self.zen_sidebar = chrome::ZenSidebar::default();
        self.command_palette = false;
        self.search_focus = false;
        self.search_entries = None;
        self.world_selection.clear();
        self.world_hover = None;
        let closing_settings = self.panel == Some(Panel::Settings);
        self.suggest_backups = project_dialogs::suggest_backups(&project);
        self.quick_notification = None;
        self.canvas_notices.clear();
        self.selected = project.iter_level_keys().next();
        if self.settings.bool("openLastProject")
            && project.path.as_ref().is_some_and(|p| {
                self.settings.value["lastProject"]["filePath"].as_str() == p.to_str()
            })
            && let Some(uid) = self.settings.value["lastProject"]["levelUid"].as_i64()
        {
            self.selected = project
                .iter_level_keys()
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
        self.camera_drag = None;
        self.space_fit_pending = false;
        self.project = Some(project);
        self.layer = self
            .visible_layer_indices(false)
            .first()
            .copied()
            .unwrap_or(0);
        self.history = History::default();
        if let Some(project) = self.project.as_ref().filter(|p| p.path.is_none()) {
            for key in project.level_keys() {
                if let Some(level) = project.level(key) {
                    self.history.mark_level_modified(level);
                }
            }
        }
        self.assets.clear();
        self.resource_checks.clear();
        self.external_enum_reloads.clear();
        self.pending_enum_sync = None;
        self.pending_enum_removal = None;
        self.layer_bake = None;
        self.int_grid_removal = None;
        self.pending_auto_layer_rebuild = false;
        self.rebuilding_auto_layers = None;
        self.auto_layer_rebuild_progress = None;
        self.resource_missing.clear();
        self.hidden.clear();
        self.dirty = false;
        self.clear_stroke_controls();
        self.brush_preview_waits_for_pointer = false;
        self.entity_eraser_preview = false;
        self.neighbor_level_press = false;
        self.async_paint = None;
        self.deferred_auto_paint = None;
        self.stroke_was_dirty = false;
        self.fit = true;
        self.camera_motion.cancel();
        self.panel = None;
        self.level_form_depth_override = None;
        self.entity_index = None;
        self.selection = None;
        self.stamp = None;
        self.gesture = None;
        self.world_context = world_context::WorldContext::default();
        self.level_resize = None;
        self.definition_uid = None;
        self.rule_draft = None;
        self.rule_hover = None;
        self.last_rule_list_row = None;
        self.field_uid = None;
        self.point_pick = None;
        self.reference_pick = None;
        self.chain_entity = None;
        self.selection_group = None;
        self.last_chance = None;
        self.atlas_selection = None;
        self.preview_tile = None;
        self.file_snapshot = snapshot;
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

    fn settle_deferred_auto_paint(&mut self) -> bool {
        let Some(mut pending) = self.deferred_auto_paint.take() else {
            return true;
        };
        let Some(project) = &mut self.project else {
            return true;
        };
        match project.settle_auto_layers(pending.level, &mut pending.rules) {
            Ok(()) => {
                if !self.stroke {
                    self.dirty |= self.history.commit(project);
                }
                true
            }
            Err(error) => {
                self.history.cancel(project);
                self.dirty = self.stroke_was_dirty;
                self.async_paint = None;
                self.clear_stroke_controls();
                if let Some(group) = pending.prior_selection {
                    self.set_selection_group(group);
                }
                self.status = error.to_string();
                self.status_error = true;
                false
            }
        }
    }

    fn clear_stroke_controls(&mut self) {
        self.stroke = false;
        self.stroke_waiting_for_level = false;
        self.last_cell = None;
        self.painted_cells.clear();
    }

    fn release_stroke(&mut self) {
        if self.deferred_auto_paint.is_some() {
            // Keep the snapshot open until the last generated output is available.
            self.clear_stroke_controls();
        } else {
            self.finish_stroke();
        }
    }

    fn finish_stroke(&mut self) {
        if self.settle_deferred_auto_paint()
            && self.stroke
            && let Some(project) = &self.project
        {
            self.dirty |= self.history.commit(project);
        }
        self.clear_stroke_controls();
    }

    fn request(&mut self, ctx: &egui::Context, action: Action) {
        if world_form_dialog::is_open(ctx) || Self::definition_removal_open(ctx) {
            return;
        }
        if self.defer_ui_action(ctx, ui_commit::DeferredUiAction::Request(action.clone())) {
            return;
        }
        if self.is_busy() || self.pending.is_some() {
            return;
        }
        if matches!(action, Action::Close | Action::Quit) && self.world_context.menu_open() {
            self.show_panel(None);
        } else {
            self.finish_gesture();
            self.finish_stroke();
        }
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
                self.quick_notification = None;
                self.canvas_notices.clear();
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
        let mut dialog = FileDialog::new().add_filter("Supported file types", &["ldtk"]);
        let browse_from = dir
            .or_else(|| self.settings.ui_dir(None, "NewProject", None))
            .or_else(|| self.settings.default_project_dialog_dir());
        if let Some(dir) = &browse_from {
            dialog = dialog.set_directory(dir);
        }
        if let Some(mut path) = dialog.save_file() {
            path.set_extension("ldtk");
            if let Some(dir) = path.parent() {
                self.settings.store_ui_dir(None, "NewProject", dir);
            }
            self.check_application_write(
                ctx,
                project_dialogs::ApplicationWrite::Create { path, browse_from },
            );
        }
    }

    fn browse_project_dialog(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        let mut dialog = FileDialog::new().add_filter("Supported file types", &["ldtk", "json"]);
        if let Some(dir) = dir.or_else(|| self.settings.default_project_dialog_dir()) {
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
        if self.is_busy() || !self.settle_deferred_auto_paint() {
            return false;
        }
        if !save_as && self.project.as_ref().is_some_and(Project::is_backup) {
            self.report_canvas_notice(notifications::CanvasNotice::CannotSaveBackup);
            return false;
        }
        self.finish_gesture();
        self.finish_stroke();
        let Some(project) = &self.project else {
            return false;
        };
        if let Some(path) = project.path.clone().filter(|_| !save_as) {
            self.check_application_write(&ctx, project_dialogs::ApplicationWrite::Save(path));
        } else if let Some(path) = Self::pick_save_path(project.path.as_deref()) {
            self.save_to(&ctx, path);
        }
        false
    }

    fn pick_save_path(project_path: Option<&Path>) -> Option<PathBuf> {
        let mut dialog = FileDialog::new().add_filter("Supported file types", &["ldtk", "json"]);
        if let Some(path) = project_path {
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
        } else {
            dialog = dialog.set_file_name("World.ldtk");
        }
        dialog.save_file()
    }

    fn save_to(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.check_application_write(ctx, project_dialogs::ApplicationWrite::SaveAs(path));
    }

    fn save_document(&mut self, ctx: &egui::Context, path: PathBuf) {
        let conflicts = self.changed_document_files();
        if !conflicts.is_empty() {
            self.external_changes = Some(conflicts);
            return;
        }
        if self.reject_conflicting_save_folder(&path) {
            ctx.request_repaint();
            return;
        }
        self.queue_project_hooks(ctx, "BeforeSave", hooks::Continuation::Save(path));
    }

    fn canvas_input(&mut self, ctx: &egui::Context, rect: Rect, response: &egui::Response) {
        self.update_point_picker_context(ctx);
        let now = ctx.input(|i| i.time);
        if self.deferred_auto_paint.as_ref().is_some_and(|pending| {
            self.world || self.selected != Some(pending.level) || now >= pending.deadline
        }) {
            self.settle_deferred_auto_paint();
        }
        if self.async_paint.as_ref().is_some_and(|preview| {
            self.world || self.selected != Some(preview.level) || now >= preview.deadline
        }) {
            self.async_paint = None;
        }
        let menu_was_open = self.world_context.menu_open();
        let context_consumed = self.world_context_menu(ctx, rect, response);
        // In level view the outside right press opens the menu after it
        // starts the brush, so that stroke can still enter the level.
        let erasing_with_menu = !self.world
            && self.world_context.menu_open()
            && (self.stroke && self.stroke_erase
                || !menu_was_open
                    && self
                        .project
                        .as_ref()
                        .zip(self.selected)
                        .and_then(|(project, key)| project.layer(key, self.layer))
                        .is_some_and(|layer| {
                            matches!(layer["__type"].as_str(), Some("Tiles" | "IntGrid"))
                        }));
        if !context_consumed || erasing_with_menu {
            if self.stroke
                && ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::PointerMoved(_)))
                })
            {
                let deadline = now + brush::ASYNC_RENDER_DELAY;
                if let Some(preview) = &mut self.async_paint {
                    preview.deadline = deadline;
                }
                if let Some(pending) = &mut self.deferred_auto_paint {
                    pending.deadline = deadline;
                }
            }
            self.canvas_controls(ctx, rect, response);
        } else {
            self.camera_drag = None;
        }
        if std::mem::take(&mut self.space_fit_pending)
            && !self.space_dragged
            && self.camera_drag.is_none()
        {
            self.animate_camera_fit();
        }
        self.keyboard_zoom(ctx, rect);
        let level_origin = if self.world && self.camera_motion.is_active() {
            self.project
                .as_ref()
                .zip(self.selected)
                .map_or([0.; 2], |(project, key)| {
                    navigation::level_world_origin(project, key)
                })
        } else {
            [0.; 2]
        };
        let pointer_activity = self.camera_pointer_activity(ctx, rect, response);
        if let Some(dt) = self.camera_motion.window_dt(ctx, pointer_activity)
            && self.camera_motion.tick(
                &mut self.viewport,
                rect,
                level_origin,
                ctx.pixels_per_point(),
                ctx.cumulative_frame_nr(),
                dt,
            )
        {
            ctx.request_repaint();
        }
    }

    fn camera_pointer_activity(
        &self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return false;
        };
        let density = ctx.pixels_per_point();
        // LDtk wakes the canvas for movement over a level or during a drag.
        ctx.input(|input| {
            input.events.iter().any(|event| {
                let egui::Event::PointerMoved(pos) = event else {
                    return false;
                };
                if self.camera_drag.is_some() || self.stroke || self.gesture.is_some() {
                    return true;
                }
                if !response.hovered() || !response.interact_rect.contains(*pos) {
                    return false;
                }
                let (local, world) = self.canvas_pointer_pixels(project, key, rect, *pos, density);
                !self.world && entity_tools::placement_in_level(project, key, local)
                    || self.level_at_world(world).is_some()
            })
        })
    }

    fn draw_project_canvas(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        key: LevelKey,
    ) {
        let Some(project) = &self.project else {
            return;
        };
        let deadline = self
            .async_paint
            .iter()
            .map(|preview| preview.deadline)
            .chain(
                self.deferred_auto_paint
                    .iter()
                    .map(|pending| pending.deadline),
            )
            .min_by(f64::total_cmp);
        if let Some(deadline) = deadline {
            let now = ctx.input(|i| i.time);
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((deadline - now).max(0.)));
        }
        self.visible_tiles = canvas::draw(
            painter,
            rect,
            project,
            &self.viewport,
            &RenderOptions {
                selected: key,
                active_layer: self.layer,
                world: self.world,
                world_multi_selection: !self.world_selection.is_empty(),
                world_hover: self.world_hover,
                grid: self.grid,
                details: self.details,
                single_layer: self.single_layer,
                hidden: &self.hidden,
                modified_levels: Some(self.history.modified_levels()),
                render_auto: self.render_auto,
                tile_tags: self.tile_tags,
                ui_scale: self.settings.float("editorUiScale", 1.0),
                single_layer_intensity: self.settings.float("singleLayerModeIntensity", 0.75),
                fields_table: self.settings.value["fieldsRender"] == "FR_Table",
                world_depth: Some(self.world_depth),
                nearby_dist: (64.0 * self.settings.float("nearbyTilesRenderingDist", 1.0)).ceil(),
                parallax: true,
                rulers: true,
                guides: self.grid,
                async_paint: self.async_paint.as_ref(),
            },
            self.linear_drag_preview(),
            &mut self.assets,
        );
    }

    fn erase_entity(&mut self, local: [f32; 2]) {
        if !self.details {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        self.chain_entity = None;
        self.selection_group = None;
        let index = project
            .layer(key, self.layer)
            .and_then(|l| hit_entity(project, l, local).map(|e| e.0));
        if let Some(index) = index {
            let entity = project.layer(key, self.layer).unwrap()["entityInstances"][index].clone();
            let snapshot = self.capture_entity_render(key, self.layer);
            self.history
                .mark_entity_change(project, key, self.layer, &entity);
            let project = self.project.as_mut().unwrap();
            match project.remove_entity(key, self.layer, index) {
                Ok(changed) => {
                    self.dirty |= changed;
                    if changed {
                        self.defer_entity_render(snapshot);
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
    }

    fn place_entity(&mut self, local: [f32; 2], level_pixel: [f32; 2]) {
        if !self.details {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &self.project else {
            return;
        };
        let Some(uid) = self.entity_uid else {
            return;
        };
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
        let placement = if self.grid { local } else { level_pixel };
        let pos = ldtk_core::snapped_entity_position(def, placement, grid as i32, self.grid);
        let cursor = level_pixel.map(|pixel| pixel as i32);
        let snapshot = self.capture_entity_render(key, self.layer);
        let project = self.project.as_mut().unwrap();
        match project.place_entity_with_outcome(key, self.layer, uid, cursor, pos) {
            Ok(ldtk_core::EntityPlacement::Placed) => {
                self.dirty = true;
                if let Some(index) = project
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
                    self.defer_entity_render(snapshot);
                    self.entity_index = Some(index);
                    self.try_chain_to(key, self.layer, index, true, self.entity_uid);
                }
            }
            Ok(ldtk_core::EntityPlacement::Ignored) => {}
            Ok(ldtk_core::EntityPlacement::LimitReached {
                maximum,
                identifier,
            }) => {
                self.report_entity_limit(maximum, identifier);
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context, level_point_picker_was_open: bool) {
        if self.is_busy() {
            if self.save_failure.is_some() && self.panel == Some(Panel::Entity) {
                self.show_entity_inspector(ctx);
            }
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
        if panel == Panel::World && self.zen && !self.zen_sidebar.revealed {
            return;
        }
        if panel == Panel::Entity {
            self.show_entity_inspector(ctx);
            return;
        }
        if panel == Panel::Settings {
            self.show_settings_dialog(ctx);
            return;
        }
        if self.source_panel_with_canvas_input(ctx, panel, level_point_picker_was_open) {
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
    fn uncovered_canvas_rect(&self, mut rect: Rect) -> Rect {
        if self.zen {
            let inset = if self.zen_sidebar.revealed && !self.world {
                chrome::SIDEBAR_WIDTH
            } else if !self.zen_sidebar.revealed {
                chrome::ZEN_EDGE_WIDTH
            } else {
                0.
            };
            rect.min.x = (rect.left() + inset).min(rect.right());
        }
        if let Some(panel @ (Panel::World | Panel::Rules)) = self.panel
            && (panel != Panel::World || !self.zen || self.zen_sidebar.revealed)
        {
            rect.min.x = rect
                .left()
                .max(panels::source_panel_width(panel).min(rect.right()));
        }
        rect
    }

    #[cfg(target_os = "linux")]
    fn update_startup_window(&mut self, ctx: &egui::Context) {
        use StartupWindow::{EnteringFullscreen, Mapping, Maximizing, NotYetMapped};
        let Some(stage) = self.startup_window else {
            return;
        };
        let (focused, maximized, fullscreen) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.focused == Some(true),
                viewport.maximized == Some(true),
                viewport.fullscreen == Some(true),
            )
        });
        let now = Instant::now();
        // Window managers without maximization support must still get an initial view.
        let wait = std::time::Duration::from_millis(250);
        let finish_maximizing = || {
            self.settings.bool("startFullScreen").then(|| {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                EnteringFullscreen(now + wait)
            })
        };
        self.startup_window = match stage {
            NotYetMapped => Some(Mapping(now + wait)),
            Mapping(_) if focused && maximized => finish_maximizing(),
            Mapping(_) if focused => {
                // X11 can discard maximization requested before eframe maps the window.
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
                Some(Maximizing(now + wait))
            }
            Mapping(deadline) | Maximizing(deadline) if maximized || now >= deadline => {
                finish_maximizing()
            }
            EnteringFullscreen(deadline) if fullscreen || now >= deadline => None,
            _ => Some(stage),
        };
        if self.startup_window.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn update_frame(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        locale::set(ctx, self.settings.language());
        #[cfg(target_os = "linux")]
        self.update_startup_window(ctx);
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
        #[cfg(target_os = "macos")]
        if !self.is_busy()
            && !world_form_dialog::is_open(ctx)
            && !Self::definition_removal_open(ctx)
            && crate::platform_files::take_quit_request()
        {
            self.request(ctx, Action::Quit);
        }
        if !self.is_busy()
            && !self.allow_close
            && self.pending.is_none()
            && !self.ui_commit.pending()
            && !world_form_dialog::is_open(ctx)
            && !Self::definition_removal_open(ctx)
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
        if self.brush_preview_waits_for_pointer
            && ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.hover_pos().is_none())
        {
            self.brush_preview_waits_for_pointer = false;
        }
        self.shortcuts(ctx);
        self.sidebar(ctx);
        self.world_switcher(ctx);
        self.normalize_palette();
        // A canvas click can finish picking before the level panel handles that click.
        let level_point_picker_was_open = self.picking_point_in_level_panel();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::DARK))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                let canvas_id = ui.advance_cursor_after_rect(rect);
                // The world panel overlays the canvas. Only the hit region and
                // floating controls move around it; the camera uses the full rect.
                let response = ui.interact(
                    self.uncovered_canvas_rect(rect),
                    canvas_id,
                    Sense::click_and_drag(),
                );
                entity_inspector::remember_canvas(ctx, rect);
                if self.project.is_some() {
                    self.canvas_input(ctx, rect, &response);
                    self.update_world_hover(ctx, rect, response.hovered());
                }
                let picker = canvas::begin_picker_filter(
                    ui.painter(),
                    self.world_context.picking
                        || self.selected.is_some() && self.picking_entity_ref(),
                );
                if self.project.is_some() {
                    if let Some(key) = self.selected {
                        self.draw_project_canvas(ctx, ui.painter(), rect, key);
                        self.draw_rule_hover(ctx, ui.painter(), rect, key);
                        self.draw_perlin_preview(ctx, ui.painter(), rect, key);
                        self.draw_world_selection(ui, rect);
                        self.draw_brush_preview(ui, rect, response.hovered());
                        self.draw_tool_overlay(ui, rect);
                        self.draw_field_pick_targets(ui, rect);
                        self.draw_level_resize_preview(ui, rect, response.hovered());
                        self.draw_resize_handles(ui, rect);
                        self.draw_bleeps(ui, rect);
                    }
                    self.world_context_overlay(ui, rect);
                    canvas::finish_picker_filter(ui.painter(), picker, rect);
                    if let Some(key) = self.selected {
                        if self.panel != Some(Panel::Rules) {
                            self.canvas_toolbar(ctx, self.uncovered_canvas_rect(rect));
                        }
                        self.draw_canvas_status(ui, rect, &response, key);
                    }
                } else {
                    self.library_home(ctx, ui, rect);
                }
                self.canvas_notifications(ctx, rect);
            });
        if self.panel == Some(Panel::World) && !self.world {
            self.show_panel(None);
        }
        self.dialogs(ctx, level_point_picker_was_open);
        self.preview_backup_restore_dialog(ctx);
        self.world_form_dialog(ctx);
        self.definition_removal_dialog(ctx);
        self.resource_dialog(ctx);
        self.service_dialogs(ctx);
        self.palette_dialog(ctx);
        self.command_dialog(ctx);
        self.trust_dialog(ctx);
        self.enum_sync_dialog(ctx);
        self.enum_value_removal_dialog(ctx);
        self.layer_bake_dialog(ctx);
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
        let saving = self
            .saving_progress
            .as_ref()
            .map(|progress| progress.lock().unwrap_or_else(|e| e.into_inner()).clone());
        let now = Instant::now();
        if let (Some(progress), Some(path)) = (&saving, &self.save_attempt) {
            progress.show(ctx, path, now);
        }
        let progress = saving
            .as_ref()
            .and_then(|progress| progress.title_progress(now))
            .or_else(|| {
                self.loading_progress
                    .as_ref()
                    .and_then(|progress| *progress.lock().unwrap_or_else(|e| e.into_inner()))
            })
            .or_else(|| {
                self.exporting_progress
                    .as_ref()
                    .and_then(|progress| *progress.lock().unwrap_or_else(|e| e.into_inner()))
            })
            .or_else(|| {
                self.auto_layer_rebuild_progress
                    .as_ref()
                    .and_then(|progress| *progress.lock().unwrap_or_else(|e| e.into_inner()))
            });
        let project = self.project.as_ref().and_then(|project| {
            let path = project.path.as_deref()?;
            let level = self
                .selected
                .and_then(|key| project.level(key))
                .and_then(|level| level["identifier"].as_str());
            Some((path, level))
        });
        let title = window_title(project, self.dirty, progress);
        if title != self.published_window_title {
            self.published_window_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        // Keep idle polling alive while input, animations and background jobs can wake us sooner.
        let foreground =
            ctx.input(|input| input.focused && input.viewport().minimized != Some(true));
        ctx.request_repaint_after(std::time::Duration::from_millis(if foreground {
            83
        } else {
            500
        }));
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
        panels::source_colors::capture_color_clipboard(ctx, input);
        self.capture_clipboard_preview(ctx, input);
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
    let entities = layer["entityInstances"].as_array()?;
    let def = layer["layerDefUid"]
        .as_i64()
        .and_then(|uid| project.layer_def(uid))
        .unwrap_or(&Value::Null);
    canvas::entity_display_order(layer, def)
        .into_iter()
        .rev()
        .map(|index| (index, &entities[index]))
        .find(|(_, entity)| selection_click::entity_is_over(project, entity, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_154_title_minibar_precedes_the_project_title() {
        assert_eq!(
            window_title(None, false, Some((1, 4))),
            "▰▱▱▱▱  LevelEditor"
        );
        assert_eq!(window_title(None, false, None), "LevelEditor");
        assert_eq!(
            window_title(None, false, Some((9, 4))),
            "▰▰▰▰▰  LevelEditor"
        );
        assert_eq!(
            window_title(None, false, Some((0, 0))),
            "▰▰▰▰▰  LevelEditor"
        );
        let project = Some((Path::new("maps/冒险.ldtk"), Some("Town")));
        assert_eq!(
            window_title(project, true, Some((2, 5))),
            "▰▰▱▱▱  冒险 [UNSAVED]  @ Town    --    LevelEditor"
        );
        assert_eq!(
            window_title(project, false, None),
            "冒险  @ Town    --    LevelEditor"
        );
    }

    #[test]
    fn preview_154_only_changed_window_titles_request_a_viewport_update() {
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let titles = |app: &mut EditorApp| {
            ctx.run(egui::RawInput::default(), |ctx| {
                eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
            })
            .viewport_output
            .remove(&egui::ViewportId::ROOT)
            .unwrap()
            .commands
            .into_iter()
            .filter_map(|command| match command {
                egui::ViewportCommand::Title(title) => Some(title),
                _ => None,
            })
            .collect::<Vec<_>>()
        };

        assert_eq!(titles(&mut app), ["LevelEditor"]);
        assert!(titles(&mut app).is_empty());
        app.exporting_progress = Some(Arc::new(Mutex::new(Some((2, 10)))));
        assert_eq!(titles(&mut app), ["▰▱▱▱▱  LevelEditor"]);
        *app.exporting_progress.as_ref().unwrap().lock().unwrap() = Some((3, 10));
        assert!(titles(&mut app).is_empty());
        *app.exporting_progress.as_ref().unwrap().lock().unwrap() = Some((4, 10));
        assert_eq!(titles(&mut app), ["▰▰▱▱▱  LevelEditor"]);
        app.exporting_progress = None;
        assert_eq!(titles(&mut app), ["LevelEditor"]);
    }

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

    #[test]
    fn passive_mouse_motion_keeps_the_idle_world_zoom_interval() {
        for (inside, resume) in [(false, 7.11), (false, 7.71), (true, 7.11)] {
            let mut app = editor();
            app.viewport.zoom = 900. / 336.;
            let project = app.project.as_mut().unwrap();
            let other = project.add_level(None, "Other", 128, 128).unwrap();
            project.move_level(other, 368, 320, 0).unwrap();
            let ctx = egui::Context::default();
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1140., 900.));
            let frame = |app: &mut EditorApp, time, events| {
                let _ = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(rect),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ctx, |ui| {
                                let (rect, response) =
                                    ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                                app.canvas_input(ctx, rect, &response);
                            });
                    },
                );
            };
            frame(&mut app, 0., vec![]);
            frame(&mut app, 7., vec![]);
            let pointer = if inside {
                rect.center()
            } else {
                egui::pos2(68., 840.)
            };
            frame(
                &mut app,
                7.015,
                vec![
                    egui::Event::MouseMoved(egui::vec2(1., 1.)),
                    egui::Event::PointerMoved(pointer),
                ],
            );
            let original_zoom = app.viewport.zoom;
            app.set_world_view(true);
            frame(&mut app, 7.03, vec![]);
            assert_eq!(app.viewport.zoom == original_zoom, !inside);
            if !inside {
                frame(&mut app, resume, vec![]);
                let target = 0.8 * 1140. / (496. * 1.1 + 300.);
                assert!((app.viewport.zoom - target).abs() < 0.0001);
            }
        }
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
    fn canvas_status_reports_hovered_cell_and_all_three_coordinate_spaces() {
        let mut app = editor();
        let project = app.project.as_mut().unwrap();
        let key = app.selected.unwrap();
        project.level_mut(key).unwrap()["worldX"] = 160.into();
        project.level_mut(key).unwrap()["worldY"] = (-48).into();
        project.json["defs"]["layers"][0]["intGridValues"][0]["identifier"] = "Wall".into();
        project.paint_int_grid(key, 0, 8, 8, 1).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let update = |app: &mut EditorApp, pointer: egui::Pos2, pressed: Option<bool>| {
            let mut events = vec![egui::Event::PointerMoved(pointer)];
            if let Some(pressed) = pressed {
                events.push(egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_max(
                        egui::Pos2::ZERO,
                        egui::pos2(1440., 900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| app.update_frame(ctx, &mut eframe::Frame::_new_kittest()),
            )
        };
        let contains = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
            })
        };
        update(&mut app, egui::pos2(870., 450.), None);
        let output = update(&mut app, egui::pos2(870., 450.), None);
        for label in [
            "Wall (1) (IntGrid)",
            "Grid = 8,8",
            "Level = 128,128",
            "World = 288,80",
        ] {
            assert!(
                contains(&output, label),
                "missing canvas status label {label}"
            );
        }
        app.viewport.zoom = 900. / 336.;
        let output = update(&mut app, egui::pos2(743., 343.), None);
        assert!(contains(&output, "Level = 80,88"));
        assert!(contains(&output, "World = 240,40"));
        let edge = egui::pos2(741., 450.);
        let output = update(&mut app, edge, None);
        assert!(contains(&output, "Grid = 5,8"));
        update(&mut app, edge, Some(true));
        update(&mut app, edge, Some(false));
        let layer = app.project.as_ref().unwrap().layer(key, 0).unwrap();
        assert_eq!(layer["intGridCsv"][8 * 16 + 4], 0);
        assert_eq!(layer["intGridCsv"][8 * 16 + 5], 1);

        let project = app.project.as_mut().unwrap();
        project.json["defs"]["layers"][0]["parallaxFactorY"] = serde_json::json!(0.25);
        project.json["defs"]["layers"][0]["parallaxScaling"] = false.into();
        let layer = project.layer_mut(key, 0).unwrap();
        layer["pxOffsetY"] = (-6).into();
        layer["__pxTotalOffsetY"] = (-6).into();
        let offset_edge = egui::pos2(870., 131.);
        let output = update(&mut app, offset_edge, None);
        assert!(contains(&output, "Level = 128,9"));
        assert!(contains(&output, "Grid = 8,1"));
        update(&mut app, offset_edge, Some(true));
        update(&mut app, offset_edge, Some(false));
        let layer = app.project.as_ref().unwrap().layer(key, 0).unwrap();
        assert_eq!(layer["intGridCsv"][8], 0);
        assert_eq!(layer["intGridCsv"][16 + 8], 1);

        let output = update(&mut app, egui::pos2(160., 330.), None);
        assert!(!contains(&output, "World = 288,80"));

        app.show_panel(Some(Panel::World));
        assert_eq!(app.panel, Some(Panel::World));
        let output = update(&mut app, egui::pos2(870., 450.), None);
        let status = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text().starts_with("World = ") => {
                Some(text.galley.text())
            }
            _ => None,
        });
        assert_eq!(status, None, "World panel must hide canvas status");
    }

    #[test]
    fn panel_mask_dismisses_on_press_without_starting_canvas_input() {
        for button in [
            egui::PointerButton::Primary,
            egui::PointerButton::Middle,
            egui::PointerButton::Secondary,
        ] {
            let mut app = editor();
            app.panel = Some(Panel::Level);
            let key = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            project.paint_int_grid(key, 0, 8, 8, 1).unwrap();
            let original = project.json.clone();
            let center = app.viewport.center;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let update = |app: &mut EditorApp, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440., 900.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| app.update_frame(ctx, &mut eframe::Frame::_new_kittest()),
                )
            };
            let start = egui::pos2(876., 456.);
            let end = egui::pos2(912., 464.);
            let press = |pos, button, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            update(&mut app, vec![]);
            update(&mut app, vec![egui::Event::PointerMoved(start)]);
            update(&mut app, vec![press(start, button, true)]);
            assert!(app.panel.is_none(), "{button:?} must close on press");
            update(&mut app, vec![egui::Event::PointerMoved(end)]);
            update(&mut app, vec![press(end, button, false)]);
            assert_eq!(app.project.as_ref().unwrap().json, original);
            assert_eq!(app.viewport.center, center);
            assert!(!app.stroke && app.gesture.is_none() && app.camera_drag.is_none());

            update(
                &mut app,
                vec![press(end, egui::PointerButton::Primary, true)],
            );
            update(
                &mut app,
                vec![press(end, egui::PointerButton::Primary, false)],
            );
            assert_ne!(app.project.as_ref().unwrap().json, original);
        }
    }

    #[test]
    fn world_panel_blocks_input_but_does_not_shift_the_canvas_transform() {
        let mut app = editor();
        app.world = true;
        app.panel = Some(Panel::World);
        app.settings.value["autoWorldModeSwitch"] = serde_json::json!("Never");
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1440., 900.));
        let canvas = Rect::from_min_max(egui::pos2(300., 0.), screen.max);
        let pointer = egui::pos2(680., 380.);
        let world_point =
            app.viewport
                .to_world(pointer.into(), canvas.min.into(), canvas.size().into());
        let mut frame = eframe::Frame::_new_kittest();
        let mut update = |app: &mut EditorApp, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| app.update_frame(ctx, &mut frame),
            );
        };
        let wheel = |position| {
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., 100.),
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        update(&mut app, Vec::new());
        update(&mut app, wheel(egui::pos2(350., 300.)));
        assert_eq!(app.viewport.zoom, 1., "panel must block covered input");
        update(&mut app, wheel(pointer));
        assert!((app.viewport.zoom - 1.15).abs() < 0.001);
        let after = app
            .viewport
            .to_world(pointer.into(), canvas.min.into(), canvas.size().into());
        assert!(
            (after[0] - world_point[0]).abs() * app.viewport.zoom <= 2.
                && (after[1] - world_point[1]).abs() * app.viewport.zoom <= 2.,
            "world mouse anchor shifted from {world_point:?} to {after:?}"
        );
    }

    #[test]
    fn space_secondary_zoom_keeps_its_press_anchor_and_uses_the_release_position() {
        for (density, batched, world) in [
            (1., false, false),
            (2., false, false),
            (1., true, false),
            (1., true, true),
        ] {
            let mut app = editor();
            app.world = world;
            app.panel = world.then_some(Panel::World);
            app.viewport.zoom = 2.;
            app.settings.value["autoWorldModeSwitch"] = serde_json::json!("Never");
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let screen = Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1440., 900.));
            let canvas = Rect::from_min_max(egui::pos2(300., 0.), screen.max);
            let start = egui::pos2(680., 500.);
            let end = egui::pos2(if world { 350. } else { 735. }, 460.);
            let before =
                app.viewport
                    .to_world(start.into(), canvas.min.into(), canvas.size().into());
            let mut frame = eframe::Frame::_new_kittest();
            let mut update = |app: &mut EditorApp, events| {
                let mut input = egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(density);
                let _ = ctx.run(input, |ctx| app.update_frame(ctx, &mut frame));
            };
            let space = |pressed| egui::Event::Key {
                key: Key::Space,
                physical_key: Some(Key::Space),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let right = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            update(&mut app, vec![egui::Event::PointerMoved(start)]);
            if batched {
                update(
                    &mut app,
                    vec![
                        space(true),
                        right(start, true),
                        right(end, false),
                        space(false),
                    ],
                );
            } else {
                update(&mut app, vec![space(true), right(start, true)]);
                update(&mut app, vec![egui::Event::PointerMoved(end)]);
                update(&mut app, vec![right(end, false), space(false)]);
            }
            let expected = 2. + 0.48 * density;
            assert!(
                (app.viewport.zoom - expected).abs() < 0.001,
                "DPR {density}, batched {batched}: {} != {expected}",
                app.viewport.zoom
            );
            let after =
                app.viewport
                    .to_world(start.into(), canvas.min.into(), canvas.size().into());
            assert!(
                (after[0] - before[0]).abs() * app.viewport.zoom <= 2.
                    && (after[1] - before[1]).abs() * app.viewport.zoom <= 2.,
                "drag anchor shifted from {before:?} to {after:?}"
            );
            assert!(!app.camera_motion.is_active());
            assert!(app.camera_drag.is_none());
        }
    }

    #[test]
    fn accepted_camera_pan_continues_over_a_panel_until_mouse_release() {
        for (button, world, density, expected) in [
            (egui::PointerButton::Middle, true, 1., [428., 198.]),
            (egui::PointerButton::Primary, true, 1., [428., 198.]),
            (egui::PointerButton::Middle, false, 2., [481., 456.]),
        ] {
            let mut app = editor();
            app.world = world;
            app.panel = world.then_some(Panel::World);
            if !world {
                app.viewport.center = [460., 440.];
            }
            let before = app.project.as_ref().unwrap().json.clone();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut frame = eframe::Frame::_new_kittest();
            let mut update = |app: &mut EditorApp, events| {
                let mut input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    events,
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(density);
                let _ = ctx.run(input, |ctx| app.update_frame(ctx, &mut frame));
            };
            let start = egui::pos2(650., 300.);
            let inside = egui::pos2(520., 260.);
            let over_panel = egui::pos2(350., 230.);
            let space = |pressed| egui::Event::Key {
                key: Key::Space,
                physical_key: Some(Key::Space),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            update(&mut app, vec![egui::Event::PointerMoved(start)]);
            if button == egui::PointerButton::Middle {
                update(&mut app, vec![pointer(start, true)]);
                update(&mut app, vec![egui::Event::PointerMoved(inside)]);
                update(&mut app, vec![pointer(over_panel, false)]);
            } else {
                update(
                    &mut app,
                    vec![
                        space(true),
                        pointer(start, true),
                        egui::Event::PointerMoved(inside),
                        space(false),
                        pointer(over_panel, false),
                    ],
                );
            }

            assert_eq!(app.viewport.center, expected, "{button:?}, DPR {density}");
            assert_eq!(app.viewport.zoom, 1., "{button:?}");
            assert_eq!(app.world, world);
            assert!(app.camera_drag.is_none());
            assert_eq!(app.project.as_ref().unwrap().json, before);
        }
    }

    #[test]
    fn fractional_zoom_pan_uses_the_snapped_canvas_origin() {
        let mut app = editor();
        app.viewport.center = [128.2; 2];
        app.viewport.zoom = 1.4;
        let ctx = egui::Context::default();
        let start = egui::pos2(52., 96.);
        let end = egui::pos2(53., 96.);
        let modifiers = egui::Modifiers::NONE;
        let middle = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Middle,
            pressed,
            modifiers,
        };
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start)],
            modifiers,
        );
        canvas_frame(&mut app, &ctx, vec![middle(start, true)], modifiers);
        assert!(app.camera_drag.is_some());
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(end), middle(end, false)],
            modifiers,
        );
        // Both positions round to world x=74 from the snapped root at x=-59.
        assert_eq!(app.viewport.center, [128.2; 2]);
        assert!(app.camera_drag.is_none());
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
            tools::rectangle_tiles([0, 0], [0, 0, 5, 3], [2, 5, 2, 2], 2),
            vec![
                ([0, 0], [2, 5]),
                ([2, 0], [3, 5]),
                ([4, 0], [2, 5]),
                ([0, 2], [2, 6]),
                ([2, 2], [3, 6]),
                ([4, 2], [2, 6])
            ]
        );
        assert_eq!(
            tools::rectangle_tiles([-1, -1], [0, 0, 4, 4], [2, 5, 2, 2], 2),
            vec![
                ([1, 1], [3, 6]),
                ([3, 1], [2, 6]),
                ([1, 3], [3, 5]),
                ([3, 3], [2, 5])
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
        let ctx = egui::Context::default();
        app.navigate_palette(&ctx, 0, 1);
        assert_eq!(app.int_value, 2);
        assert_eq!(app.viewport.center, before);
        app.navigate_palette(&ctx, -1, 0);
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
        let key = app.selected.unwrap();
        for x in [8., 24.] {
            app.history.begin(app.project.as_ref().unwrap(), key);
            app.stroke = true;
            app.place_entity([x, 8.], [x, 8.]);
            app.finish_stroke();
        }
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
        app.undo_action(false);
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 2);
        assert!(entities[0]["fieldInstances"][0]["__value"].is_null());
        app.undo_action(false);
        let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
        assert_eq!(entities.as_array().unwrap().len(), 1);
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
    fn entity_drag_preserves_pointer_cell_phase_and_uses_the_scaled_grid() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        let key = project.level_keys()[0];
        let view = Viewport::default();
        assert_eq!(
            entity_tools::drag_delta(&project, key, 0, &view, [[7., 8.], [17., 8.]], true),
            [16., 0.]
        );
        assert_eq!(
            entity_tools::drag_delta(&project, key, 0, &view, [[1., 1.], [15., 1.]], true),
            [0., 0.]
        );
        project
            .update_layer_properties(
                layer,
                &serde_json::json!({"parallaxFactorX":0.5,"parallaxScaling":true}),
            )
            .unwrap();
        assert_eq!(
            entity_tools::drag_delta(&project, key, 0, &view, [[7., 8.], [9., 8.]], true),
            [8., 0.]
        );
        project
            .update_layer_properties(layer, &serde_json::json!({"pxOffsetX":20}))
            .unwrap();
        assert_eq!(
            entity_tools::drag_delta(&project, key, 0, &view, [[27., 8.], [29., 8.]], true),
            [8., 0.]
        );
    }

    #[test]
    fn preview_154_entity_drag_without_grid_preserves_pixel_delta() {
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, def, 64, 80).unwrap();
        app.set_project(p);
        app.grid = false;
        app.history
            .begin_layer(app.project.as_ref().unwrap(), key, 0);
        app.stroke = true;
        app.gesture = Some(tools::Gesture::EntityMove {
            key,
            layer: 0,
            index: 0,
            press: egui::pos2(64., 80.),
            start: [64., 80.],
            end: [83., 83.],
            origin: [64, 80],
            moved: true,
        });
        app.finish_gesture();
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"],
            serde_json::json!([83, 83])
        );
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"],
            serde_json::json!([64, 80])
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
    fn tiles_only_canvas_edits_int_grid_only_when_its_definition_has_an_atlas() {
        let mut app = editor();
        let ctx = egui::Context::default();
        let key = app.selected.unwrap();
        app.details = false;
        app.int_value = 1;
        let ground = app.project.as_ref().unwrap().json["defs"]["layers"][0]["uid"]
            .as_i64()
            .unwrap();
        let point = egui::pos2(72., 72.);
        let modifiers = egui::Modifiers::NONE;
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(256., 256.));
        let cursor = ctx
            .run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events: vec![egui::Event::PointerMoved(point)],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.draw_brush_preview(ui, rect, true));
                },
            )
            .platform_output
            .cursor_icon;
        assert_eq!(cursor, egui::CursorIcon::NotAllowed);
        let click = |app: &mut EditorApp| {
            canvas_frame(app, &ctx, vec![egui::Event::PointerMoved(point)], modifiers);
            for pressed in [true, false] {
                canvas_frame(
                    app,
                    &ctx,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }],
                    modifiers,
                );
            }
        };
        click(&mut app);
        assert!(!app.dirty);
        let p = app.project.as_mut().unwrap();
        let atlas = p.add_tileset(Path::new("atlas.png"), 16, 16, 16).unwrap();
        p.assign_tileset(ground, atlas).unwrap();
        assert_eq!(
            p.layer_def(ground).unwrap()["autoRuleGroups"],
            serde_json::json!([])
        );
        click(&mut app);
        let layer = app.project.as_ref().unwrap().layer(key, 0).unwrap();
        assert!(
            layer["intGridCsv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|cell| cell == 1)
        );
        assert!(app.dirty);
    }

    #[test]
    fn alt_picks_visible_tiles_even_when_the_active_layer_is_hidden_by_details() {
        let mut app = editor();
        let project = app.project.as_mut().unwrap();
        let tiles = project.add_layer("Tiles", "Tiles", 16).unwrap();
        let atlas = project
            .add_tileset(Path::new("atlas.png"), 16, 16, 16)
            .unwrap();
        project.assign_tileset(tiles, atlas).unwrap();
        let key = app.selected.unwrap();
        project
            .paint_tile(key, 0, 4, 4, 0, [0, 0], 0, false)
            .unwrap();
        app.layer = 1;
        app.details = false;
        let ctx = egui::Context::default();
        let pos = egui::pos2(72., 72.);
        let modifiers = egui::Modifiers::ALT;
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
        let selection = app
            .selection_group
            .as_ref()
            .expect("picked the visible tile");
        assert_eq!(selection["layers"][0]["layerDefUid"], tiles);
        assert!(!app.dirty);
    }

    #[test]
    fn async_intgrid_renders_hints_until_idle_and_undo_clears_them() {
        let mut app = editor();
        let ctx = egui::Context::default();
        app.repaint = Some(ctx.clone());
        app.grid = false;
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let uid = project.layer(key, 0).unwrap()["layerDefUid"]
            .as_i64()
            .unwrap();
        project.update_layer_properties(uid, &serde_json::json!({
            "useAsyncRender": true,
            "intGridValues": [{"value":1,"identifier":null,"color":"#32A064","tile":null,"groupUid":0}],
        })).unwrap();
        let pointer = egui::pos2(40., 40.);
        let draw = |app: &mut EditorApp, time, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(256., 256.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                        app.draw_project_canvas(ctx, ui.painter(), rect, key);
                    });
                },
            )
        };
        let button = |button, pressed| egui::Event::PointerButton {
            pos: pointer,
            button,
            pressed,
            modifiers: Default::default(),
        };
        let fill_at_pointer = |out: &egui::FullOutput, fill| {
            out.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(r) if r.rect.contains(pointer) && r.fill == fill)
            })
        };
        let cell = |app: &EditorApp| {
            let layer = app.project.as_ref().unwrap().layer(key, 0).unwrap();
            let width = layer["__cWid"].as_u64().unwrap() as usize;
            layer["intGridCsv"][2 * width + 2].as_i64().unwrap()
        };
        let paint = Color32::from_rgb(50, 160, 100);
        let paint_hint = Color32::from_rgb_additive(35, 112, 70);
        let erase_hint = Color32::from_rgb_additive(179, 0, 0);

        draw(&mut app, 1., vec![egui::Event::PointerMoved(pointer)]);
        let output = draw(
            &mut app,
            2.,
            vec![button(egui::PointerButton::Primary, true)],
        );
        assert_eq!(cell(&app), 1, "paint edits the document immediately");
        assert!(fill_at_pointer(&output, paint_hint));
        assert!(!fill_at_pointer(&output, paint));
        let output = draw(
            &mut app,
            2.2,
            vec![egui::Event::PointerMoved(pointer + egui::vec2(1., 0.))],
        );
        assert!(fill_at_pointer(&output, paint_hint));
        let output = draw(&mut app, 2.449, vec![]);
        assert!(
            fill_at_pointer(&output, paint_hint),
            "same-cell motion renews the delay"
        );
        let output = draw(&mut app, 2.451, vec![]);
        assert!(app.stroke, "the pointer is still held");
        assert!(!fill_at_pointer(&output, paint_hint));
        assert!(fill_at_pointer(&output, paint));
        draw(
            &mut app,
            2.5,
            vec![button(egui::PointerButton::Primary, false)],
        );

        let output = draw(
            &mut app,
            3.,
            vec![button(egui::PointerButton::Secondary, true)],
        );
        assert_eq!(cell(&app), 0, "erase edits the document immediately");
        assert!(
            fill_at_pointer(&output, paint),
            "erase retains the previous render during the delay"
        );
        assert!(fill_at_pointer(&output, erase_hint));
        let output = draw(
            &mut app,
            3.05,
            vec![button(egui::PointerButton::Secondary, false)],
        );
        assert!(
            fill_at_pointer(&output, erase_hint),
            "release keeps the pending render"
        );
        app.undo_action(false);
        let output = draw(&mut app, 3.06, vec![]);
        assert_eq!(cell(&app), 1);
        assert!(fill_at_pointer(&output, paint));
        assert!(!fill_at_pointer(&output, erase_hint));
        app.undo_action(true);
        let output = draw(&mut app, 3.07, vec![]);
        assert_eq!(cell(&app), 0);
        assert!(!fill_at_pointer(&output, paint));
        assert!(!fill_at_pointer(&output, erase_hint));
    }

    #[test]
    fn async_generated_rules_partition_targets_and_keep_history_and_saves_current() {
        use serde_json::json;
        let temp = tempfile::tempdir().unwrap();
        let mut app = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        let source = project.layer(key, 0).unwrap()["layerDefUid"]
            .as_i64()
            .unwrap();
        let atlas = project
            .add_tileset(Path::new("Atlas.png"), 16, 16, 16)
            .unwrap();
        let mut targets = Vec::new();
        for (name, delay, rule) in [("Immediate", false, 80), ("Deferred", true, 100)] {
            let uid = project.add_layer(name, "AutoLayer", 16).unwrap();
            project.assign_tileset(uid, atlas).unwrap();
            project.update_layer_properties(uid, &json!({
                "autoSourceLayerDefUid":source,
                "useAsyncRender":delay,
                "autoRuleGroups":[{"uid":rule-1,"active":true,"rules":[
                    {"uid":rule,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1,"breakOnMatch":true}
                ]}]
            })).unwrap();
            targets.push(uid);
        }
        app.layer = 2;
        let source_cell = |app: &EditorApp| {
            app.project.as_ref().unwrap().layer(key, 2).unwrap()["intGridCsv"][34].clone()
        };
        let counts = |p: &Project| {
            targets
                .iter()
                .map(|uid| {
                    p.level(key).unwrap()["layerInstances"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|layer| layer["layerDefUid"] == *uid)
                        .unwrap()["autoLayerTiles"]
                        .as_array()
                        .unwrap()
                        .len()
                })
                .collect::<Vec<_>>()
        };
        let ctx = egui::Context::default();
        app.repaint = Some(ctx.clone());
        let pointer = egui::pos2(40., 40.);
        let frame = |app: &mut EditorApp, time, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(256., 256.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                        app.draw_project_canvas(ctx, ui.painter(), rect, key);
                    });
                },
            );
        };
        let button = |button, pressed| egui::Event::PointerButton {
            pos: pointer,
            button,
            pressed,
            modifiers: Default::default(),
        };
        use egui::PointerButton::{Primary, Secondary};
        frame(&mut app, 1., vec![egui::Event::PointerMoved(pointer)]);
        frame(&mut app, 2., vec![button(Primary, true)]);
        assert_eq!(source_cell(&app), 1);
        assert_eq!(counts(app.project.as_ref().unwrap()), [1, 0]);
        frame(
            &mut app,
            2.2,
            vec![egui::Event::PointerMoved(pointer + egui::vec2(1., 0.))],
        );
        frame(&mut app, 2.21, vec![button(Primary, false)]);
        frame(&mut app, 2.449, vec![]);
        assert_eq!(
            counts(app.project.as_ref().unwrap()),
            [1, 0],
            "release leaves the scheduled target pending"
        );
        frame(&mut app, 2.451, vec![]);
        assert_eq!(counts(app.project.as_ref().unwrap()), [1, 1]);
        app.undo_action(false);
        assert_eq!(counts(app.project.as_ref().unwrap()), [0, 0]);
        app.undo_action(true);
        assert_eq!(counts(app.project.as_ref().unwrap()), [1, 1]);

        app.project
            .as_mut()
            .unwrap()
            .update_layer_properties(source, &json!({"useAsyncRender":true}))
            .unwrap();
        frame(&mut app, 3., vec![button(Secondary, true)]);
        frame(&mut app, 3.05, vec![button(Secondary, false)]);
        assert_eq!(source_cell(&app), 0);
        assert_eq!(
            counts(app.project.as_ref().unwrap()),
            [1, 1],
            "source flag delays both targets"
        );
        let path = temp.path().join("Deferred.ldtk");
        app.save_async_at(path.clone());
        app.saving
            .as_ref()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(
            counts(&Project::load(path).unwrap()),
            [0, 0],
            "early save serializes settled caches"
        );

        app.saving = None;
        let project = app.project.as_mut().unwrap();
        let def = project.json["defs"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|def| def["uid"] == targets[1])
            .unwrap();
        def["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([]);
        let before = project.json.clone();
        frame(&mut app, 4., vec![button(Primary, true)]);
        assert_eq!(source_cell(&app), 1);
        app.undo_action(false);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(app.status.contains("invalid auto-layer rule pattern"));
        assert!(
            app.history.can_undo(),
            "the previous valid stroke remains in history"
        );
    }

    #[test]
    fn async_group_drag_partitions_rules_and_keeps_flagged_layers_before_settle() {
        use serde_json::json;
        for copy in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let atlas_path = temp.path().join("atlas.png");
            let atlas_pixels =
                image::RgbaImage::from_pixel(16, 16, image::Rgba([190, 10, 20, 255]));
            atlas_pixels.save(&atlas_path).unwrap();
            let mut app = editor();
            let key = app.selected.unwrap();
            let project = app.project.as_mut().unwrap();
            let source = project.layer(key, 0).unwrap()["layerDefUid"]
                .as_i64()
                .unwrap();
            project.update_layer_properties(source, &json!({
                "useAsyncRender":copy,
                "intGridValues":[{"value":1,"identifier":null,"color":"#32A064","tile":null,"groupUid":0}],
            })).unwrap();
            let tiles = project.add_layer("Manual", "Tiles", 16).unwrap();
            let atlas = project.add_tileset(&atlas_path, 16, 16, 16).unwrap();
            project.assign_tileset(tiles, atlas).unwrap();
            project
                .update_layer_properties(tiles, &json!({"useAsyncRender":true}))
                .unwrap();
            let targets = ["Immediate", "Deferred"].map(|name| {
                let delayed = name == "Deferred";
                let uid = project.add_layer(name, "AutoLayer", 16).unwrap();
                project.assign_tileset(uid, atlas).unwrap();
                let rule = if delayed {100} else {80};
                project.update_layer_properties(uid, &json!({
                    "autoSourceLayerDefUid":source, "useAsyncRender":delayed,
                    "autoRuleGroups":[{"uid":rule-1,"active":true,"rules":[
                        {"uid":rule,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1,"breakOnMatch":true}
                    ]}]
                })).unwrap();
                uid
            });
            let layer_index = |p: &Project, uid| {
                canvas::values(&p.level(key).unwrap()["layerInstances"])
                    .iter()
                    .position(|layer| layer["layerDefUid"] == uid)
                    .unwrap()
            };
            let source_index = layer_index(project, source);
            let tiles_index = layer_index(project, tiles);
            project.paint_int_grid(key, source_index, 2, 2, 1).unwrap();
            project
                .paint_tile_stacked(key, tiles_index, 3, 3, 0, [0, 0], 0, false, true)
                .unwrap();
            let project_before = project.json.clone();
            let generated = |p: &Project, uid| {
                canvas::values(&p.layer(key, layer_index(p, uid)).unwrap()["autoLayerTiles"])
                    .iter()
                    .map(|tile| tile["px"][0].as_i64().unwrap())
                    .collect::<Vec<_>>()
            };
            app.layer = source_index;
            app.fit = false;
            app.grid = false;
            app.viewport = Viewport {
                center: [128.; 2],
                zoom: 1.,
            };
            let ctx = egui::Context::default();
            app.repaint = Some(ctx.clone());
            app.assets
                .store_file_image(&ctx, &atlas_path, atlas_pixels)
                .unwrap();
            let texture = app.assets.texture(&ctx, &atlas_path).unwrap().id();
            let grid_at = |output: &egui::FullOutput, point| {
                output.shapes.iter().any(|s| {
                matches!(&s.shape,egui::Shape::Rect(r) if r.fill == Color32::from_rgb(50,160,100) && r.rect.contains(point))
            })
            };
            let tile_at = |output: &egui::FullOutput, point| {
                output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh),
                        _ => None,
                    })
                    .flat_map(|mesh| mesh.vertices.as_chunks::<4>().0)
                    .any(|quad| Rect::from_two_pos(quad[0].pos, quad[3].pos).contains(point))
            };
            app.capture_selection([2, 2, 2, 2]);
            assert_eq!(app.selection_group.as_ref().unwrap()["elementCount"], 2);
            let modifiers = egui::Modifiers {
                command: copy,
                alt: copy,
                ..Default::default()
            };
            let frame = |app: &mut EditorApp, time, events| {
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        events,
                        modifiers,
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(256., 256.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (rect, response) = ui
                                .allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                            app.draw_project_canvas(ctx, ui.painter(), rect, key);
                        });
                    },
                )
            };
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            let start = egui::pos2(40., 40.);
            let end = egui::pos2(72., 40.);
            let old_tile = egui::pos2(56., 56.);
            let new_tile = egui::pos2(88., 56.);
            frame(&mut app, 1., vec![egui::Event::PointerMoved(start)]);
            frame(&mut app, 1.01, vec![button(start, true)]);
            frame(&mut app, 1.02, vec![egui::Event::PointerMoved(end)]);
            let rendered = frame(&mut app, 1.08, vec![button(end, false)]);
            let project = app.project.as_ref().unwrap();
            assert_eq!(
                project.layer(key, source_index).unwrap()["intGridCsv"][36],
                1
            );
            assert_eq!(
                generated(project, targets[0]),
                if copy { vec![32] } else { vec![64] }
            );
            assert_eq!(generated(project, targets[1]), [32]);
            assert_eq!(grid_at(&rendered, start), copy);
            assert_eq!(grid_at(&rendered, end), !copy);
            assert!(tile_at(&rendered, old_tile));
            assert!(!tile_at(&rendered, new_tile));
            assert!(app.async_paint.as_ref().unwrap().hints.is_empty());
            assert!(
                !app.history.can_undo(),
                "generated work is part of the single pending edit"
            );

            let expected = if copy { vec![32, 64] } else { vec![64] };
            if copy {
                let path = temp.path().join("copy.ldtk");
                app.save_async_at(path.clone());
                app.saving
                    .take()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                let saved = Project::load(path).unwrap();
                assert_eq!(generated(&saved, targets[0]), expected);
                assert_eq!(generated(&saved, targets[1]), expected);
            } else {
                assert!(tile_at(&frame(&mut app, 1.329, vec![]), old_tile));
                assert_eq!(generated(app.project.as_ref().unwrap(), targets[1]), [32]);
            }
            let output = frame(&mut app, 1.331, vec![]);
            assert!(tile_at(&output, new_tile));
            assert_eq!(tile_at(&output, old_tile), copy);
            assert!(grid_at(&output, end));
            assert_eq!(
                generated(app.project.as_ref().unwrap(), targets[1]),
                expected
            );
            if !copy {
                let selection_before_failure = app.selection_group.clone();
                let project = app.project.as_mut().unwrap();
                let target = project.json["defs"]["layers"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|def| def["uid"] == targets[1])
                    .unwrap();
                target["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([]);
                let before_failure = project.json.clone();
                frame(&mut app, 2.0, vec![button(end, true)]);
                frame(&mut app, 2.01, vec![egui::Event::PointerMoved(start)]);
                frame(&mut app, 2.05, vec![button(start, false)]);
                assert!(app.deferred_auto_paint.is_some());
                assert_ne!(app.selection_group, selection_before_failure);
                frame(&mut app, 2.301, vec![]);
                assert_eq!(app.project.as_ref().unwrap().json, before_failure);
                assert_eq!(app.selection_group, selection_before_failure);
                assert!(app.status.contains("invalid auto-layer rule pattern"));
            }
            app.undo_action(false);
            let project = app.project.as_ref().unwrap();
            assert_eq!(project.json["levels"], project_before["levels"]);
            assert!(!app.history.can_undo());
        }
    }

    #[test]
    fn async_tiles_delay_the_ordered_render_but_save_and_undo_current_tiles() {
        let temp = tempfile::tempdir().unwrap();
        let atlas_path = temp.path().join("atlas.png");
        let ctx = egui::Context::default();
        let mut project = Project::new();
        let key = project.level_keys()[0];
        let layer = project.add_layer("Tiles", "Tiles", 16).unwrap();
        let atlas = project.add_tileset(&atlas_path, 16, 48, 16).unwrap();
        project.assign_tileset(layer, atlas).unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"useAsyncRender":true}))
            .unwrap();
        // Tile 1's cached average is green, distinct from the decoded texture's blue.
        project.json["defs"]["tilesets"][0]["cachedPixelData"] =
            serde_json::json!({"opaqueTiles":"110","averageColors":"ff00f0f0880f"});
        project
            .paint_tile_stacked(key, 0, 3, 2, 0, [0, 0], 0, false, true)
            .unwrap();
        project
            .paint_tile_stacked(key, 0, 3, 2, 2, [32, 0], 0, false, true)
            .unwrap();

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.fit = false;
        app.grid = false;
        app.viewport = Viewport {
            center: [128., 128.],
            zoom: 1.,
        };
        app.repaint = Some(ctx.clone());
        app.tile_rect = [0, 0, 1, 1];
        app.tile_stacking = false;
        let pixels = image::RgbaImage::from_fn(48, 16, |x, _| match x / 16 {
            0 => image::Rgba([255, 0, 0, 255]),
            1 => image::Rgba([0, 0, 255, 255]),
            _ => image::Rgba([128, 0, 255, 128]),
        });
        pixels.save(&atlas_path).unwrap();
        app.assets
            .store_file_image(&ctx, &atlas_path, pixels)
            .unwrap();
        let texture = app.assets.texture(&ctx, &atlas_path).unwrap().id();

        let paint_cell = egui::pos2(40., 40.);
        let stack_cell = egui::pos2(56., 40.);
        let draw = |app: &mut EditorApp, time, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(256., 256.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                        app.draw_project_canvas(ctx, ui.painter(), rect, key);
                    });
                },
            )
        };
        let button = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        };
        let painted = |out: &egui::FullOutput, point| {
            out.shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh),
                    _ => None,
                })
                .flat_map(|mesh| mesh.vertices.as_chunks::<4>().0)
                .filter(|quad| Rect::from_two_pos(quad[0].pos, quad[3].pos).contains(point))
                .map(|quad| (quad[0].uv.x * 3.).round() as i64)
                .collect::<Vec<_>>()
        };
        let hint = |out: &egui::FullOutput, point, color| {
            out.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(r) if r.rect.contains(point) && r.fill == color)
            })
        };
        let saved_tiles = |project: &Project, x| {
            project.layer(key, 0).unwrap()["gridTiles"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|tile| tile["px"][0] == x)
                .map(|tile| tile["t"].as_i64().unwrap())
                .collect::<Vec<_>>()
        };

        use egui::PointerButton::{Primary, Secondary};
        draw(&mut app, 1., vec![egui::Event::PointerMoved(paint_cell)]);
        let output = draw(&mut app, 2., vec![button(paint_cell, Primary, true)]);
        assert_eq!(saved_tiles(app.project.as_ref().unwrap(), 32), [0]);
        assert!(
            painted(&output, paint_cell).is_empty(),
            "old empty cell still renders"
        );
        assert!(hint(
            &output,
            paint_cell,
            Color32::from_rgb_additive(179, 0, 0)
        ));
        draw(&mut app, 2.04, vec![button(paint_cell, Primary, false)]);
        let output = draw(&mut app, 2.251, vec![]);
        assert_eq!(painted(&output, paint_cell), [0]);

        app.tile_rect = [1, 0, 1, 1];
        let output = draw(&mut app, 3., vec![button(paint_cell, Primary, true)]);
        assert_eq!(saved_tiles(app.project.as_ref().unwrap(), 32), [1]);
        assert_eq!(painted(&output, paint_cell), [0]);
        assert!(hint(
            &output,
            paint_cell,
            Color32::from_rgb_additive(0, 179, 0)
        ));
        draw(&mut app, 3.04, vec![button(paint_cell, Primary, false)]);
        let output = draw(&mut app, 3.251, vec![]);
        assert_eq!(painted(&output, paint_cell), [1]);

        app.tile_stacking = true;
        draw(&mut app, 3.9, vec![egui::Event::PointerMoved(stack_cell)]);
        let output = draw(&mut app, 4., vec![button(stack_cell, Secondary, true)]);
        assert_eq!(saved_tiles(app.project.as_ref().unwrap(), 48), [0]);
        assert_eq!(
            painted(&output, stack_cell),
            [0, 2],
            "old stack stays ordered"
        );
        let erase_hint = Color32::from_rgb_additive(179, 0, 0);
        assert!(hint(&output, stack_cell, erase_hint));
        let output = draw(&mut app, 4.04, vec![button(stack_cell, Secondary, false)]);
        assert_eq!(painted(&output, stack_cell), [0, 2]);
        assert!(hint(&output, stack_cell, erase_hint));

        let save_path = temp.path().join("project.ldtk");
        app.project
            .as_ref()
            .unwrap()
            .clone()
            .save(&save_path)
            .unwrap();
        let saved = Project::load(save_path).unwrap();
        assert_eq!(saved_tiles(&saved, 48), [0]);
        app.undo_action(false);
        let output = draw(&mut app, 4.05, vec![]);
        assert_eq!(painted(&output, stack_cell), [0, 2]);
        assert!(!hint(&output, stack_cell, erase_hint));
        app.undo_action(true);
        let output = draw(&mut app, 4.06, vec![]);
        assert_eq!(painted(&output, stack_cell), [0]);
        assert!(!hint(&output, stack_cell, erase_hint));
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
    fn rectangle_brush_crosses_level_edges_without_clamping_the_cursor_or_filling() {
        let inside = egui::pos2(132., 168.);
        let outside = egui::pos2(120., 168.);
        for (start, end, preview_end) in [(inside, outside, [-1, 2]), (outside, inside, [0, 2])] {
            let mut app = editor();
            let ctx = egui::Context::default();
            let canvas = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(512., 512.));
            let modifiers = egui::Modifiers {
                shift: true,
                ..Default::default()
            };
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            let frame = |app: &mut EditorApp, events| {
                let _ = ctx.run(
                    egui::RawInput {
                        events,
                        modifiers,
                        screen_rect: Some(canvas),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ctx, |ui| {
                                let (rect, response) =
                                    ui.allocate_exact_size(canvas.size(), Sense::click_and_drag());
                                app.canvas_controls(ctx, rect, &response);
                            });
                    },
                );
            };
            frame(&mut app, vec![egui::Event::PointerMoved(start)]);
            frame(&mut app, vec![button(start, true)]);
            frame(&mut app, vec![egui::Event::PointerMoved(end)]);
            assert!(
                matches!(app.gesture, Some(tools::Gesture::Rectangle { end, .. }) if end == preview_end)
            );
            frame(&mut app, vec![button(end, false)]);
            let project = app.project.as_ref().unwrap();
            let cells = project.layer(app.selected.unwrap(), 0).unwrap()["intGridCsv"]
                .as_array()
                .unwrap();
            assert_eq!(cells[32], 1);
            assert_eq!(cells.iter().filter(|&cell| cell == 1).count(), 1);
        }
    }

    #[test]
    fn free_brush_enters_from_outside_the_level_and_interpolates_the_boundary_cells() {
        let start = egui::pos2(40., 56.);
        let end = egui::pos2(118., 104.);
        let modifiers = egui::Modifiers::NONE;
        let boundary_cells = [[0, 4], [1, 4], [2, 5], [3, 5], [4, 6]];
        let painted = |app: &EditorApp| {
            let project = app.project.as_ref().unwrap();
            let cells = project.layer(app.selected.unwrap(), 0).unwrap()["intGridCsv"]
                .as_array()
                .unwrap();
            cells
                .iter()
                .enumerate()
                .filter_map(|(i, value)| (value == 1).then_some([i as i32 % 16, i as i32 / 16]))
                .collect::<Vec<_>>()
        };
        for (pointer_button, before, after) in [
            (egui::PointerButton::Primary, &[][..], &boundary_cells[..]),
            (egui::PointerButton::Secondary, &boundary_cells[..], &[][..]),
        ] {
            let mut app = editor();
            app.viewport.center[0] = 80.;
            let ctx = egui::Context::default();
            let key = app.selected.unwrap();
            let cells = before.iter().map(|&cell| (cell, 1)).collect::<Vec<_>>();
            app.project
                .as_mut()
                .unwrap()
                .paint_int_grid_cells(key, 0, &cells)
                .unwrap();
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: pointer_button,
                pressed,
                modifiers,
            };
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start)],
                modifiers,
            );
            canvas_frame(&mut app, &ctx, vec![button(start, true)], modifiers);
            assert_eq!(
                app.world_context.menu_open(),
                pointer_button == egui::PointerButton::Secondary
            );
            assert_eq!(painted(&app), before);
            assert!(!app.dirty);
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(end)],
                modifiers,
            );
            canvas_frame(&mut app, &ctx, vec![button(end, false)], modifiers);
            assert_eq!(painted(&app), after, "{pointer_button:?}");
            app.undo_action(false);
            assert_eq!(painted(&app), before);
        }
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
    fn blocked_entity_placement_shows_the_localized_limit_notice_without_a_history_entry() {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let actor = p.add_entity_def("Actor", 16, 16).unwrap();
        p.update_entity_definition(
            actor,
            &serde_json::json!({"maxCount":1,"limitBehavior":"PreventAdding","limitScope":"PerLevel"}),
        )
        .unwrap();
        let key = p.level_keys()[0];
        assert!(p.add_entity(key, 0, actor, 16, 16).unwrap());
        let before = p.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.entity_uid = Some(actor);
        app.history.begin(app.project.as_ref().unwrap(), key);
        app.place_entity([100., 100.], [100., 100.]);
        assert!(!app.history.commit(app.project.as_ref().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.dirty);
        assert!(
            !app.status_error,
            "the original uses a transient error notification"
        );

        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let draw = |app: &mut EditorApp| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| app.canvas_notifications(ctx, rect),
            )
        };
        draw(&mut app);
        for (language, message) in [
            (
                locale::Language::English,
                "You cannot have more than 1 Actor.",
            ),
            (
                locale::Language::SimplifiedChinese,
                "Actor 的数量不能超过 1 个。",
            ),
        ] {
            locale::set(&ctx, language);
            assert!(
                draw(&mut app).shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == message)
                }),
                "{message}"
            );
        }
    }

    #[test]
    fn dragging_entities_and_points_outside_reports_the_removed_name_and_can_undo() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let def = project.add_entity_def("Actor", 16, 16).unwrap();
        let field = project
            .add_field_definition(Some(def), "Target", "Point", true)
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, def, 80, 88).unwrap();
        let iid = project.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        project
            .set_field_value(
                key,
                Some(&iid),
                field,
                serde_json::json!([null, {"cx":10,"cy":8}]),
            )
            .unwrap();
        let mut app = EditorApp::empty();
        app.grid = false;
        for (start, end, name, alt) in [
            ([80., 88.], [-40., 88.], "Actor", false),
            ([80., 88.], [-40., 88.], "Actor", true),
            ([168., 136.], [-24., 136.], "Target", false),
        ] {
            app.set_project(project.clone());
            app.fit = false;
            app.viewport = Viewport {
                center: [128.; 2],
                zoom: 1.,
            };
            let ctx = egui::Context::default();
            let modifiers = egui::Modifiers {
                alt,
                ..Default::default()
            };
            let pos = egui::pos2(start[0], start[1]);
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
            let pos = egui::pos2(end[0], end[1]);
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(pos)],
                modifiers,
            );
            assert!(app.canvas_notices.is_empty(), "warning before release");
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
            let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
            if name == "Actor" {
                assert_eq!(entities, &serde_json::json!([]));
            } else {
                assert_eq!(entities[0]["iid"], iid);
                assert_eq!(
                    entities[0]["fieldInstances"][0]["__value"],
                    serde_json::json!([null])
                );
            }
            let rect = ctx.content_rect();
            let draw = |app: &mut EditorApp| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ctx| app.canvas_notifications(ctx, rect),
                )
            };
            draw(&mut app);
            let message = format!("Out-of-bounds entity removed: {name}");
            assert!(
                draw(&mut app).shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == message)
                }),
                "{message}"
            );
            app.undo_action(false);
            assert_eq!(
                app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"],
                project.layer(key, 0).unwrap()["entityInstances"]
            );
        }
    }

    #[test]
    fn point_click_drag_and_right_delete_do_not_create_or_delete_an_entity() {
        let mut p = Project::new();
        p.add_layer("Entities", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        let other = p.add_entity_def("Other", 16, 16).unwrap();
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
        app.entity_uid = Some(other);
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
        assert_eq!(
            app.entity_uid,
            Some(uid),
            "selecting the Point picks its owner brush"
        );
        assert_eq!(app.panel, Some(Panel::Entity));
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
        app.entity_uid = Some(other);
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
        assert_eq!(
            app.entity_uid,
            Some(uid),
            "erasing the Point selects its owner"
        );
    }

    #[test]
    fn async_entities_keep_the_old_render_during_placement_removal_and_transforms() {
        for resizable in [false, true] {
            let mut project = Project::new();
            let layer_uid = project.add_layer("Entities", "Entities", 16).unwrap();
            project
                .update_layer_properties(layer_uid, &serde_json::json!({"useAsyncRender":true}))
                .unwrap();
            let uid = project.add_entity_def("Actor", 16, 16).unwrap();
            project
                .update_entity_definition(
                    uid,
                    &serde_json::json!({
                        "color":"#22AA77", "fillOpacity":1, "lineOpacity":0, "pivotX":0, "pivotY":0,
                        "resizableX":resizable, "resizableY":resizable,
                    }),
                )
                .unwrap();
            let mut app = EditorApp::empty();
            app.set_project(project);
            app.entity_uid = Some(uid);
            app.fit = false;
            app.grid = false;
            app.viewport = Viewport {
                center: [128.; 2],
                zoom: 1.,
            };
            let key = app.selected.unwrap();
            let ctx = egui::Context::default();
            app.repaint = Some(ctx.clone());
            let draw = |app: &mut EditorApp, time, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(256., 256.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (rect, response) = ui
                                .allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                            app.draw_project_canvas(ctx, ui.painter(), rect, key);
                            app.draw_resize_handles(ui, rect);
                            app.draw_tool_overlay(ui, rect);
                        });
                    },
                )
            };
            let entities = |app: &EditorApp| {
                app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"].clone()
            };
            let visible_entity = |output: &egui::FullOutput| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(34, 170, 119) => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
            };
            let button = |pos, button, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let primary = egui::PointerButton::Primary;
            let secondary = egui::PointerButton::Secondary;
            let placement = egui::pos2(40., 40.);
            draw(&mut app, 1.0, vec![egui::Event::PointerMoved(placement)]);
            let out = draw(&mut app, 1.01, vec![button(placement, primary, true)]);
            assert_eq!(entities(&app).as_array().unwrap().len(), 1);
            assert_eq!(app.entity_index, Some(0));
            assert!(
                visible_entity(&out).is_none(),
                "placement painted before the timer"
            );
            assert!(app.async_paint.as_ref().unwrap().hints.is_empty());
            draw(&mut app, 1.02, vec![button(placement, primary, false)]);
            assert!(visible_entity(&draw(&mut app, 1.259, vec![])).is_none());
            let old = visible_entity(&draw(&mut app, 1.261, vec![])).unwrap();

            let handle_offset = egui::vec2(4., 5.);
            let resized_handle_shift = egui::vec2(-1., 0.);
            let start = if resizable {
                old.right_bottom() + handle_offset
            } else {
                old.center()
            };
            let end = start + egui::vec2(64., 32.);
            draw(&mut app, 2.0, vec![egui::Event::PointerMoved(start)]);
            draw(&mut app, 2.01, vec![button(start, primary, true)]);
            let initial_position = entities(&app)[0]["px"].clone();
            if resizable {
                let out = draw(
                    &mut app,
                    2.015,
                    vec![egui::Event::PointerMoved(end - egui::vec2(8., 4.))],
                );
                assert_eq!(visible_entity(&out), Some(old));
                assert_eq!(entities(&app)[0]["width"], 71);
            }
            let out = draw(&mut app, 2.02, vec![egui::Event::PointerMoved(end)]);
            assert_eq!(visible_entity(&out), Some(old));
            if resizable {
                assert_eq!(entities(&app)[0]["width"], 79);
                assert!(out.shapes.iter().any(
                    |shape| matches!(&shape.shape, egui::Shape::Circle(circle)
                    if circle.fill == Color32::from_rgba_unmultiplied(255, 204, 0, 191)
                        && circle.center == end + resized_handle_shift)
                ));
            } else {
                let ghost_fill = Color32::from_rgb(34, 170, 119).gamma_multiply(0.5);
                assert!(
                    out.shapes
                        .iter()
                        .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.fill == ghost_fill && rect.rect.center() == end))
                );
                assert_eq!(entities(&app)[0]["px"], initial_position);
            }
            let out = draw(&mut app, 2.10, vec![button(end, primary, false)]);
            assert_eq!(visible_entity(&out), Some(old));
            assert_eq!(app.entity_index, Some(0));
            if !resizable {
                assert_ne!(entities(&app)[0]["px"], initial_position);
            }
            assert!(app.async_paint.as_ref().unwrap().hints.is_empty());
            let deadline = if resizable { 2.27 } else { 2.35 };
            assert_eq!(
                visible_entity(&draw(&mut app, deadline - 0.001, vec![])),
                Some(old)
            );
            let next = visible_entity(&draw(&mut app, deadline + 0.001, vec![])).unwrap();
            if resizable {
                assert_eq!(
                    next.right_bottom() + handle_offset,
                    end + resized_handle_shift
                );
            } else {
                assert_eq!(next.center(), end);
            }

            let hit = next.center();
            draw(&mut app, 3.0, vec![egui::Event::PointerMoved(hit)]);
            let out = draw(&mut app, 3.01, vec![button(hit, secondary, true)]);
            assert!(entities(&app).as_array().unwrap().is_empty());
            assert!(app.entity_index.is_none());
            assert_eq!(visible_entity(&out), Some(next));
            assert!(app.async_paint.as_ref().unwrap().hints.is_empty());
            draw(&mut app, 3.02, vec![button(hit, secondary, false)]);
            assert_eq!(visible_entity(&draw(&mut app, 3.259, vec![])), Some(next));
            assert!(visible_entity(&draw(&mut app, 3.261, vec![])).is_none());
            app.undo_action(false);
            assert_eq!(visible_entity(&draw(&mut app, 3.3, vec![])), Some(next));
        }
    }

    #[test]
    fn async_point_move_and_eraser_retain_the_pre_edit_marker() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"useAsyncRender":true}))
            .unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        let field = project
            .add_field_definition(Some(actor), "Target", "Point", false)
            .unwrap();
        project
            .update_field_definition(field, &serde_json::json!({"editorDisplayMode":"Points"}))
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 16, 16).unwrap();
        let iid = project.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let initial = serde_json::json!({"cx":4,"cy":4});
        let moved = serde_json::json!({"cx":6,"cy":4});
        project
            .set_field_value(key, Some(&iid), field, initial.clone())
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.entity_uid = Some(actor);
        app.fit = false;
        app.viewport = Viewport {
            center: [128.; 2],
            zoom: 1.,
        };
        let ctx = egui::Context::default();
        app.repaint = Some(ctx.clone());
        let start = egui::pos2(72., 72.);
        let end = egui::pos2(104., 72.);
        let frame = |app: &mut EditorApp, time, events| {
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(256., 256.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                        app.canvas_input(ctx, rect, &response);
                        app.draw_project_canvas(ctx, ui.painter(), rect, key);
                    });
                },
            )
        };
        let point_at = |out: &egui::FullOutput, center| {
            out.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Path(p)
                if p.points.len()==4 && p.points[0]==center-egui::vec2(0.,4.) && p.points[2]==center+egui::vec2(0.,4.)))
        };
        let button = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let primary = egui::PointerButton::Primary;
        let secondary = egui::PointerButton::Secondary;
        let document = |app: &EditorApp| {
            app.project.as_ref().unwrap().layer(key,0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"].clone()
        };

        assert!(point_at(
            &frame(&mut app, 1.0, vec![egui::Event::PointerMoved(start)]),
            start
        ));
        frame(&mut app, 1.01, vec![button(start, primary, true)]);
        frame(&mut app, 1.02, vec![egui::Event::PointerMoved(end)]);
        let pending = frame(&mut app, 1.08, vec![button(end, primary, false)]);
        assert_eq!(document(&app), moved);
        assert!(point_at(&pending, start));
        assert!(!point_at(&pending, end));
        assert!(point_at(&frame(&mut app, 1.329, vec![]), start));
        let settled = frame(&mut app, 1.331, vec![]);
        assert!(point_at(&settled, end));
        assert!(!point_at(&settled, start));

        let pending = frame(&mut app, 2.0, vec![button(end, secondary, true)]);
        assert!(document(&app).is_null());
        assert_eq!(app.entity_index, Some(0));
        assert!(point_at(&pending, end));
        frame(&mut app, 2.01, vec![button(end, secondary, false)]);
        assert!(point_at(&frame(&mut app, 2.249, vec![]), end));
        assert!(!point_at(&frame(&mut app, 2.251, vec![]), end));
        app.undo_action(false);
        assert_eq!(document(&app), moved);
        app.undo_action(false);
        assert_eq!(document(&app), initial);
    }

    #[test]
    fn creating_an_entity_immediately_drags_or_resizes_with_one_undo() {
        for (resizable, shift) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut p = Project::new();
            p.add_layer("Entities", "Entities", 16).unwrap();
            let uid = p.add_entity_def("Actor", 16, 16).unwrap();
            p.update_entity_definition(
                uid,
                &serde_json::json!({"resizableX":resizable,"resizableY":resizable,"pivotX":0.5,"pivotY":1}),
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
            let modifiers = egui::Modifiers {
                shift,
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
                assert_eq!(entity["px"][0], if shift { 40 } else { 104 });
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
    fn selection_drag_threshold_uses_screen_motion_before_release() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        project
            .update_layer_properties(
                layer,
                &serde_json::json!({"parallaxFactorX":0.25,"parallaxFactorY":0.25,"parallaxScaling":true}),
            )
            .unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(actor, &serde_json::json!({"pivotX":0,"pivotY":0}))
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 64, 80).unwrap();
        project.add_entity(key, 0, actor, 104, 104).unwrap();
        let modifiers = egui::Modifiers::NONE;
        for (select_all, start, end, motion, expected_x) in [
            (false, 14., 23., true, 64),
            (false, 17., 27., true, 67),
            (false, 17., 29., false, 64),
            (true, 14., 23., true, 64),
            (true, 17., 27., true, 67),
            (true, 17., 29., false, 64),
        ] {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.fit = false;
            app.grid = false;
            app.viewport.center = [128.; 2];
            app.viewport.zoom = 900. / 336.;
            if select_all {
                app.select_all();
            }
            let ctx = egui::Context::default();
            let start = egui::pos2(start, 42.);
            let end = egui::pos2(end, 42.);
            let click = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            };
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start)],
                modifiers,
            );
            canvas_frame(&mut app, &ctx, vec![click(start, true)], modifiers);
            assert!(if select_all {
                matches!(app.gesture, Some(tools::Gesture::SelectionMove { .. }))
            } else {
                matches!(app.gesture, Some(tools::Gesture::EntityMove { .. }))
            });
            let mut events = Vec::new();
            if motion {
                events.push(egui::Event::PointerMoved(end));
            }
            events.push(click(end, false));
            events.push(egui::Event::PointerMoved(egui::pos2(70., 42.)));
            canvas_frame(&mut app, &ctx, events, modifiers);
            let entities = &app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"];
            assert_eq!(
                entities[0]["px"],
                serde_json::json!([expected_x, 80]),
                "select_all={select_all}, end={end:?}"
            );
            let second_x = if select_all {
                104 + expected_x - 64
            } else {
                104
            };
            assert_eq!(entities[1]["px"], serde_json::json!([second_x, 104]));
            assert_eq!(app.dirty, expected_x != 64);
        }
    }

    #[test]
    fn parallax_entity_drag_uses_rounded_level_pixels_or_scaled_grid_steps() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        project
            .update_layer_properties(
                layer,
                &serde_json::json!({
                    "parallaxFactorX":0.25,"parallaxFactorY":0.25,"parallaxScaling":true
                }),
            )
            .unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(actor, &serde_json::json!({"pivotX":0,"pivotY":0}))
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 64, 80).unwrap();
        for (grid, command, expected) in [
            (false, false, [78, 96]),
            (true, false, [76, 92]),
            (false, true, [78, 96]),
            (true, true, [76, 92]),
        ] {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.fit = false;
            app.grid = grid;
            app.viewport.center = [128.; 2];
            app.viewport.zoom = 1.;
            let ctx = egui::Context::default();
            let start = egui::pos2(86., 96.);
            let end = egui::pos2(100., 112.);
            let modifiers = egui::Modifiers {
                ctrl: command && !cfg!(target_os = "macos"),
                mac_cmd: command && cfg!(target_os = "macos"),
                command,
                ..Default::default()
            };
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(start)],
                modifiers,
            );
            for (pos, pressed) in [(start, true), (end, false)] {
                canvas_frame(
                    &mut app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers,
                        },
                    ],
                    modifiers,
                );
            }
            let position = |app: &EditorApp| {
                app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"]
                    .clone()
            };
            assert_eq!(
                position(&app),
                serde_json::json!(expected),
                "grid={grid}, command={command}"
            );
            app.undo_action(false);
            assert_eq!(position(&app), serde_json::json!([64, 80]));
        }
    }

    #[test]
    fn parallax_resize_uses_level_handles_and_keeps_an_entity_outside_the_scaled_grid() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        project
            .update_layer_properties(
                layer,
                &serde_json::json!({
                    "parallaxFactorX":0.25,"parallaxFactorY":0.25,"parallaxScaling":true
                }),
            )
            .unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(
                actor,
                &serde_json::json!({"resizableX":true,"pivotX":0,"pivotY":0}),
            )
            .unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 200, 154).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.fit = false;
        app.grid = false;
        app.entity_index = Some(0);
        app.viewport.center = [128.; 2];
        app.viewport.zoom = 1.;
        let ctx = egui::Context::default();
        let start = egui::pos2(220., 162.);
        let end = egui::pos2(239., 162.);
        let modifiers = egui::Modifiers::NONE;
        canvas_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start)],
            modifiers,
        );
        for (pos, pressed) in [(start, true), (end, false)] {
            canvas_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    },
                ],
                modifiers,
            );
        }
        let entity = |app: &EditorApp| {
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0].clone()
        };
        assert_eq!(entity(&app)["px"], serde_json::json!([200, 154]));
        assert_eq!(entity(&app)["width"], 34);
        app.undo_action(false);
        assert_eq!(entity(&app)["width"], 16);
        app.undo_action(true);
        assert_eq!(entity(&app)["width"], 34);
    }

    #[test]
    fn entity_placement_uses_level_bounds_on_a_scaled_parallax_layer() {
        let mut project = Project::new();
        let layer = project.add_layer("Actors", "Entities", 16).unwrap();
        project
            .update_layer_properties(
                layer,
                &serde_json::json!({
                    "parallaxFactorX":0.25,"parallaxFactorY":0.25,"parallaxScaling":true
                }),
            )
            .unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        project
            .update_entity_definition(actor, &serde_json::json!({"pivotX":0.5,"pivotY":0.5}))
            .unwrap();
        let key = project.level_keys()[0];
        let modifiers = egui::Modifiers::NONE;
        for (grid, x, expected) in [
            (false, 98., [98, 154]),
            (false, 12., [12, 154]),
            (true, 12., [-24, 168]),
            (false, 240., [240, 154]),
            (true, 240., [280, 168]),
        ] {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.fit = false;
            app.grid = grid;
            app.entity_uid = Some(actor);
            app.viewport = Viewport {
                center: [128.; 2],
                zoom: 1.,
            };
            let ctx = egui::Context::default();
            let pos = egui::pos2(x, 154.);
            canvas_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(pos)],
                modifiers,
            );
            for pressed in [true, false] {
                canvas_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers,
                    }],
                    modifiers,
                );
            }
            assert_eq!(
                app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"][0]["px"],
                serde_json::json!(expected),
                "grid={grid}, x={x}"
            );
            if expected[0] < 0 || expected[0] >= 256 {
                for pressed in [true, false] {
                    canvas_frame(
                        &mut app,
                        &ctx,
                        vec![egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers,
                        }],
                        modifiers,
                    );
                }
                let layer = app.project.as_ref().unwrap().layer(key, 0).unwrap();
                assert_eq!(layer["entityInstances"], serde_json::json!([]));
            }
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
            press: egui::Pos2::ZERO,
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
