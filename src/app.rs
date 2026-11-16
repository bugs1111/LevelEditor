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
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::Instant,
};

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Paint,
    Erase,
    Fill,
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Project,
    Level,
    Layers,
    Entities,
    Enums,
    Tilesets,
    Help,
    Entity,
}

enum Action {
    New,
    Browse,
    Open(PathBuf),
    Quit,
}

pub struct EditorApp {
    project: Option<Project>,
    history: History,
    selected: Option<LevelKey>,
    layer: usize,
    viewport: Viewport,
    assets: Assets,
    hidden: HashSet<i64>,
    world: bool,
    grid: bool,
    details: bool,
    single_layer: bool,
    zen: bool,
    tool: Tool,
    int_value: i32,
    tile_id: i32,
    entity_uid: Option<i64>,
    entity_index: Option<usize>,
    flips: i32,
    dirty: bool,
    fit: bool,
    stroke: bool,
    last_cell: Option<[i32; 2]>,
    panel: Option<Panel>,
    pending: Option<Action>,
    loading: Option<Receiver<Result<Project, String>>>,
    status: String,
    allow_close: bool,
    frame_time: f32,
    visible_tiles: usize,
    new_name: String,
    new_kind: String,
    new_grid: i32,
}

impl EditorApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let mut app = Self::empty();
        if let Some(path) = path {
            app.open_async(&cc.egui_ctx, path);
        }
        app
    }

    fn empty() -> Self {
        Self {
            project: None,
            history: History::default(),
            selected: None,
            layer: 0,
            viewport: Viewport::default(),
            assets: Assets::default(),
            hidden: HashSet::new(),
            world: false,
            grid: true,
            details: true,
            single_layer: false,
            zen: false,
            tool: Tool::Paint,
            int_value: 1,
            tile_id: 0,
            entity_uid: None,
            entity_index: None,
            flips: 0,
            dirty: false,
            fit: true,
            stroke: false,
            last_cell: None,
            panel: None,
            pending: None,
            loading: None,
            status: "Open a project or create a new one.".into(),
            allow_close: false,
            frame_time: 0.0,
            visible_tiles: 0,
            new_name: String::new(),
            new_kind: "IntGrid".into(),
            new_grid: 16,
        }
    }

    fn open_async(&mut self, ctx: &egui::Context, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.loading = Some(rx);
        self.status = format!("Opening {}…", path.display());
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = Project::load(&path).map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn set_project(&mut self, project: Project) {
        self.selected = project.level_keys().first().copied();
        self.project = Some(project);
        self.layer = 0;
        self.history = History::default();
        self.assets = Assets::default();
        self.hidden.clear();
        self.dirty = false;
        self.stroke = false;
        self.last_cell = None;
        self.fit = true;
        self.panel = None;
        self.entity_index = None;
        self.status =
            "Project opened. Left: draw · Right: erase · Space + drag: pan · Wheel: zoom".into();
    }

    fn finish_stroke(&mut self) {
        if self.stroke {
            if let Some(project) = &self.project {
                self.dirty |= self.history.commit(project);
            }
            self.stroke = false;
            self.last_cell = None;
        }
    }

    fn request(&mut self, ctx: &egui::Context, action: Action) {
        if self.loading.is_some() || self.pending.is_some() {
            return;
        }
        self.finish_stroke();
        if self.dirty {
            self.pending = Some(action);
        } else {
            self.execute(ctx, action);
        }
    }

    fn execute(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New => {
                self.set_project(Project::new());
                self.status = "New project. Add layers in the Layers panel (L).".into();
                self.panel = Some(Panel::Layers);
            }
            Action::Browse => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("LDtk project", &["ldtk"])
                    .pick_file()
                {
                    self.open_async(ctx, path);
                }
            }
            Action::Open(path) => self.open_async(ctx, path),
            Action::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn save(&mut self, save_as: bool) -> bool {
        self.finish_stroke();
        let Some(project) = &mut self.project else {
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
        match project.save(&path) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() || self.loading.is_some() || self.pending.is_some() {
            return;
        }
        let input = ctx.input(|i| (i.modifiers, i.events.clone()));
        for event in input.1 {
            let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } = event
            else {
                continue;
            };
            if input.0.command {
                match key {
                    Key::S => {
                        self.save(input.0.shift);
                    }
                    Key::O => self.request(ctx, Action::Browse),
                    Key::N => self.request(ctx, Action::New),
                    Key::Z => {
                        self.finish_stroke();
                        if let Some(p) = &mut self.project {
                            let changed = if input.0.shift {
                                self.history.redo(p)
                            } else {
                                self.history.undo(p)
                            };
                            self.dirty |= changed;
                        }
                    }
                    Key::Y => {
                        self.finish_stroke();
                        if let Some(p) = &mut self.project {
                            self.dirty |= self.history.redo(p);
                        }
                    }
                    Key::H => self.details = !self.details,
                    _ => {}
                }
                continue;
            }
            match key {
                Key::G => self.grid = !self.grid,
                Key::Tab => self.zen = !self.zen,
                Key::A if input.0.shift => self.single_layer = !self.single_layer,
                Key::X => self.flips ^= 1,
                Key::Y => self.flips ^= 2,
                Key::F => self.fit = true,
                Key::Backtick => {
                    self.world = !self.world;
                    self.fit = true;
                }
                Key::P => self.panel = Some(Panel::Project),
                Key::C => self.panel = Some(Panel::Level),
                Key::L => self.panel = Some(Panel::Layers),
                Key::E => self.panel = Some(Panel::Entities),
                Key::U => self.panel = Some(Panel::Enums),
                Key::T => self.panel = Some(Panel::Tilesets),
                Key::H => self.panel = Some(Panel::Help),
                Key::Escape => {
                    self.panel = None;
                    self.entity_index = None;
                }
                Key::Num1
                | Key::Num2
                | Key::Num3
                | Key::Num4
                | Key::Num5
                | Key::Num6
                | Key::Num7
                | Key::Num8
                | Key::Num9
                | Key::Num0 => {
                    let index = match key {
                        Key::Num1 => 0,
                        Key::Num2 => 1,
                        Key::Num3 => 2,
                        Key::Num4 => 3,
                        Key::Num5 => 4,
                        Key::Num6 => 5,
                        Key::Num7 => 6,
                        Key::Num8 => 7,
                        Key::Num9 => 8,
                        _ => 9,
                    };
                    if self
                        .selected
                        .and_then(|k| self.project.as_ref()?.layer(k, index))
                        .is_some()
                    {
                        self.layer = index;
                        self.entity_index = None;
                    }
                }
                _ => {}
            }
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("filebar")
            .exact_height(38.0)
            .frame(egui::Frame::new().fill(theme::MEDIUM).inner_margin(6.0))
            .show(ctx, |ui| {
                if self.loading.is_some() || self.pending.is_some() {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("LDtk")
                            .strong()
                            .size(22.0)
                            .color(theme::ACCENT),
                    );
                    ui.separator();
                    if ui.button("New").clicked() {
                        self.request(ctx, Action::New);
                    }
                    if ui.button("Open…").clicked() {
                        self.request(ctx, Action::Browse);
                    }
                    if ui
                        .add_enabled(self.project.is_some(), egui::Button::new("Save"))
                        .clicked()
                    {
                        self.save(false);
                    }
                    if ui
                        .add_enabled(self.project.is_some(), egui::Button::new("Save as…"))
                        .clicked()
                    {
                        self.save(true);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.finish_stroke();
                        if let Some(p) = &mut self.project {
                            self.dirty |= self.history.undo(p);
                        }
                    }
                    if ui
                        .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.finish_stroke();
                        if let Some(p) = &mut self.project {
                            self.dirty |= self.history.redo(p);
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let name = self
                            .project
                            .as_ref()
                            .and_then(|p| p.path.as_ref())
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Untitled".into());
                        ui.label(format!("{}{}", if self.dirty { "● " } else { "" }, name));
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        if self.zen || self.project.is_none() {
            return;
        }
        egui::SidePanel::left("main-panel")
            .exact_width(300.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::DARK).inner_margin(10.0))
            .show(ctx, |ui| {
                if self.loading.is_some() || self.pending.is_some() {
                    ui.disable();
                }
                ui.horizontal_wrapped(|ui| {
                    for (label, panel, icon) in [
                        (
                            "Project settings (P)",
                            Panel::Project,
                            egui::include_image!("../assets/icons/projectSettings.png"),
                        ),
                        (
                            "Current level (C)",
                            Panel::Level,
                            egui::include_image!("../assets/icons/level.svg"),
                        ),
                        (
                            "Layers (L)",
                            Panel::Layers,
                            egui::include_image!("../assets/icons/layer.png"),
                        ),
                        (
                            "Entities (E)",
                            Panel::Entities,
                            egui::include_image!("../assets/icons/entity.png"),
                        ),
                        (
                            "Enums (U)",
                            Panel::Enums,
                            egui::include_image!("../assets/icons/enum.png"),
                        ),
                        (
                            "Tilesets (T)",
                            Panel::Tilesets,
                            egui::include_image!("../assets/icons/tile.png"),
                        ),
                        (
                            "Help (H)",
                            Panel::Help,
                            egui::include_image!("../assets/icons/help.svg"),
                        ),
                    ] {
                        if ui
                            .add(
                                egui::Button::image(
                                    egui::Image::new(icon)
                                        .fit_to_exact_size(egui::vec2(20.0, 20.0)),
                                )
                                .selected(self.panel == Some(panel)),
                            )
                            .on_hover_text(label)
                            .clicked()
                        {
                            self.panel = if self.panel == Some(panel) {
                                None
                            } else {
                                Some(panel)
                            };
                        }
                    }
                });
                ui.separator();
                let keys = self.project.as_ref().unwrap().level_keys();
                let current_name = self
                    .selected
                    .and_then(|key| self.project.as_ref()?.level(key))
                    .map(|l| text(l, "identifier", "Level"))
                    .unwrap_or("No level")
                    .to_owned();
                egui::ComboBox::from_id_salt("level-picker")
                    .selected_text(current_name)
                    .width(265.0)
                    .show_ui(ui, |ui| {
                        for key in keys {
                            let name = self
                                .project
                                .as_ref()
                                .unwrap()
                                .level(key)
                                .map(|l| text(l, "identifier", "Level"))
                                .unwrap_or("Level");
                            let name = if let Some(world) = key.world {
                                format!(
                                    "{} / {name}",
                                    text(
                                        &self.project.as_ref().unwrap().json["worlds"][world],
                                        "identifier",
                                        "World"
                                    )
                                )
                            } else {
                                name.to_owned()
                            };
                            if ui
                                .selectable_label(self.selected == Some(key), name)
                                .clicked()
                            {
                                self.finish_stroke();
                                self.selected = Some(key);
                                self.layer = 0;
                                self.entity_index = None;
                                self.fit = true;
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.world, "World view  ~").clicked() {
                        self.world = !self.world;
                        self.fit = true;
                    }
                    if ui.button("Fit  F").clicked() {
                        self.fit = true;
                    }
                });
                ui.add_space(7.0);
                ui.label(RichText::new("LAYERS").color(theme::ACCENT));
                let layers = self
                    .selected
                    .and_then(|key| self.project.as_ref()?.level(key)?["layerInstances"].as_array())
                    .map(|layers| {
                        layers
                            .iter()
                            .map(|layer| {
                                (
                                    layer["layerDefUid"].as_i64().unwrap_or(-1),
                                    text(layer, "__type", "").to_owned(),
                                    text(layer, "__identifier", "Layer").to_owned(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for (index, (uid, kind, name)) in layers.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let mut visible = !self.hidden.contains(uid);
                        if ui.checkbox(&mut visible, "").changed() {
                            if visible {
                                self.hidden.remove(uid);
                            } else {
                                self.hidden.insert(*uid);
                            }
                        }
                        let marker = match kind.as_str() {
                            "IntGrid" => "▦",
                            "Entities" => "◆",
                            "AutoLayer" => "▧",
                            _ => "▣",
                        };
                        let label = format!("{}  {}  {}", index + 1, marker, name);
                        if ui
                            .add_sized(
                                [225.0, 29.0],
                                egui::Button::new(label).selected(self.layer == index),
                            )
                            .clicked()
                        {
                            self.finish_stroke();
                            self.layer = index;
                            self.entity_index = None;
                        }
                    });
                }
                if layers.is_empty() {
                    ui.label("No layers. Open Layers (L) to add one.");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tool, Tool::Paint, "Draw");
                    ui.selectable_value(&mut self.tool, Tool::Erase, "Erase");
                    ui.selectable_value(&mut self.tool, Tool::Fill, "Fill");
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.grid, "Grid  G");
                    ui.checkbox(&mut self.details, "Details");
                });
                ui.checkbox(&mut self.single_layer, "Single layer  Shift+A");
                ui.small("Left: draw · Right: erase · Alt: pick");
                ui.small("Space + drag: pan · Wheel: zoom");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("palette-scroll")
                    .show(ui, |ui| self.palette(ctx, ui));
            });
    }

    fn palette(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(key) = self.selected else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let layer_type = text(layer, "__type", "");
        ui.label(RichText::new("PALETTE").color(theme::ACCENT));
        if layer_type == "IntGrid" {
            if let Some(def) = project.layer_def(layer["layerDefUid"].as_i64().unwrap_or(-1)) {
                if let Some(values) = def["intGridValues"].as_array() {
                    for value in values {
                        let id = value["value"].as_i64().unwrap_or(1) as i32;
                        let label = format!("  {}  {}", id, text(value, "identifier", "Value"));
                        let button = egui::Button::new(
                            RichText::new(label)
                                .color(theme::color(text(value, "color", "#ffcc00"))),
                        )
                        .selected(self.int_value == id);
                        if ui.add_sized([270.0, 32.0], button).clicked() {
                            self.int_value = id;
                        }
                    }
                }
                if def["autoRuleGroups"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
                {
                    ui.add_space(8.0);
                    ui.small("Auto-layer rules are preserved. Rule regeneration is still in development.");
                }
            }
        } else if layer_type == "Entities" {
            if let Some(entities) = project.json["defs"]["entities"].as_array() {
                for def in entities {
                    let uid = def["uid"].as_i64().unwrap_or(-1);
                    if self.entity_uid.is_none() {
                        self.entity_uid = Some(uid);
                    }
                    let label = RichText::new(text(def, "identifier", "Entity"))
                        .color(theme::color(text(def, "color", "#ffcc00")));
                    if ui
                        .add_sized(
                            [270.0, 31.0],
                            egui::Button::new(label).selected(self.entity_uid == Some(uid)),
                        )
                        .clicked()
                    {
                        self.entity_uid = Some(uid);
                    }
                }
            }
        } else if layer_type == "Tiles" || layer_type == "AutoLayer" {
            if layer_type == "AutoLayer" {
                ui.small("Generated layer. Edit its source IntGrid to change tiles.");
            }
            let Some(tileset) = layer["__tilesetDefUid"]
                .as_i64()
                .and_then(|uid| project.tileset(uid))
            else {
                ui.label("Assign a tileset in Layers.");
                return;
            };
            let Some(relative) = tileset["relPath"].as_str() else {
                ui.label("Embedded atlas support is pending.");
                return;
            };
            ui.label(text(tileset, "identifier", "Tileset"));
            ui.horizontal(|ui| {
                ui.label("Flip:");
                if ui.selectable_label(self.flips & 1 != 0, "X").clicked() {
                    self.flips ^= 1;
                }
                if ui.selectable_label(self.flips & 2 != 0, "Y").clicked() {
                    self.flips ^= 2;
                }
            });
            if let Some(texture) = self
                .assets
                .texture(ctx, &canvas::asset_path(project, relative))
            {
                let natural = texture.size_vec2();
                let scale = (270.0 / natural.x).min(2.0);
                let size = natural * scale;
                let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                let grid = number(tileset, "tileGridSize", 16.0);
                let spacing = number(tileset, "spacing", 0.0);
                let padding = number(tileset, "padding", 0.0);
                let columns = number(
                    tileset,
                    "__cWid",
                    ((natural.x - 2.0 * padding + spacing) / (grid + spacing)).floor(),
                )
                .max(1.0) as i32;
                if (response.clicked() || response.dragged())
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let p = (pos - rect.min) / scale;
                    let x = ((p.x - padding) / (grid + spacing)).floor() as i32;
                    let y = ((p.y - padding) / (grid + spacing)).floor() as i32;
                    if x >= 0 && y >= 0 && x < columns {
                        self.tile_id = x + y * columns;
                        self.flips = 0;
                    }
                }
                let x = self.tile_id % columns;
                let y = self.tile_id / columns;
                let marker = Rect::from_min_size(
                    rect.min
                        + egui::vec2(
                            padding + x as f32 * (grid + spacing),
                            padding + y as f32 * (grid + spacing),
                        ) * scale,
                    Vec2::splat(grid * scale),
                );
                ui.painter().rect_stroke(
                    marker,
                    0.0,
                    Stroke::new(2.0_f32, theme::ACCENT),
                    StrokeKind::Inside,
                );
                ui.small(format!("Tile #{}", self.tile_id));
            }
        }
    }

    fn canvas_input(&mut self, ctx: &egui::Context, rect: Rect, response: &egui::Response) {
        if self.project.is_none() || self.pending.is_some() || self.loading.is_some() {
            return;
        }
        if self.fit
            && let (Some(project), Some(key)) = (&self.project, self.selected)
        {
            self.viewport
                .fit(canvas::bounds(project, key, self.world), rect.size().into());
            self.fit = false;
        }
        let (pointer, delta, scroll, space, alt, control, primary, secondary, middle, pressed) =
            ctx.input(|i| {
                (
                    i.pointer.hover_pos(),
                    i.pointer.delta(),
                    i.raw_scroll_delta.y,
                    i.key_down(Key::Space),
                    i.modifiers.alt,
                    i.modifiers.ctrl,
                    i.pointer.primary_down(),
                    i.pointer.secondary_down(),
                    i.pointer.middle_down(),
                    i.pointer.any_pressed(),
                )
            });
        let Some(pointer) = pointer else {
            return;
        };
        if !rect.contains(pointer) {
            self.last_cell = None;
            return;
        }
        if response.hovered() && scroll != 0.0 {
            self.viewport.zoom_at(
                (scroll * 0.002).exp(),
                pointer.into(),
                rect.min.into(),
                rect.size().into(),
            );
        }
        if middle || (space && primary) {
            self.viewport.center[0] -= delta.x / self.viewport.zoom;
            self.viewport.center[1] -= delta.y / self.viewport.zoom;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            return;
        }
        if self.world {
            if response.double_clicked()
                && let (Some(project), Some(key)) = (&self.project, self.selected)
            {
                let world =
                    self.viewport
                        .to_world(pointer.into(), rect.min.into(), rect.size().into());
                for (candidate, b) in canvas::placements(project, key) {
                    if world[0] >= b[0] && world[0] < b[2] && world[1] >= b[1] && world[1] < b[3] {
                        self.selected = Some(candidate);
                        self.world = false;
                        self.layer = 0;
                        self.fit = true;
                        break;
                    }
                }
            }
            return;
        }
        if self.panel.is_some() && ctx.is_pointer_over_area() && !response.hovered() {
            return;
        }
        if !(primary || secondary) || space {
            return;
        }
        let Some(key) = self.selected else {
            return;
        };
        let project = self.project.as_ref().unwrap();
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let grid = number(layer, "__gridSize", 16.0).max(1.0);
        let world = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        let local = [
            world[0] - number(layer, "__pxTotalOffsetX", 0.0),
            world[1] - number(layer, "__pxTotalOffsetY", 0.0),
        ];
        let cell = [
            (local[0] / grid).floor() as i32,
            (local[1] / grid).floor() as i32,
        ];
        if cell[0] < 0
            || cell[1] < 0
            || cell[0] >= number(layer, "__cWid", 0.0) as i32
            || cell[1] >= number(layer, "__cHei", 0.0) as i32
        {
            self.last_cell = None;
            return;
        }
        if alt && pressed {
            self.pick(cell, local);
            return;
        }
        if alt {
            return;
        }
        let erase = secondary || control || self.tool == Tool::Erase;
        if !self.stroke {
            if !pressed || !response.hovered() {
                return;
            }
            self.history.begin(project, key);
            self.stroke = true;
        }
        let kind = text(layer, "__type", "").to_owned();
        if self.tool == Tool::Fill && pressed && kind == "IntGrid" {
            self.fill(cell, erase);
            self.last_cell = Some(cell);
            return;
        }
        if self.tool == Tool::Fill {
            return;
        }
        if kind == "Entities" {
            if pressed {
                self.edit_entity(local, erase);
            }
        } else if self.last_cell != Some(cell) {
            for point in stroke_cells(self.last_cell.unwrap_or(cell), cell) {
                self.paint_cell(point, erase);
            }
        }
        self.last_cell = Some(cell);
    }

    fn pick(&mut self, cell: [i32; 2], local: [f32; 2]) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|key| project.layer(key, self.layer)) else {
            return;
        };
        match text(layer, "__type", "") {
            "IntGrid" => {
                let width = number(layer, "__cWid", 1.0) as usize;
                self.int_value = layer["intGridCsv"]
                    [(cell[0] as usize) + (cell[1] as usize) * width]
                    .as_i64()
                    .unwrap_or(0) as i32;
            }
            "Tiles" => {
                let grid = number(layer, "__gridSize", 16.0);
                if let Some(tile) = layer["gridTiles"].as_array().and_then(|tiles| {
                    tiles.iter().rev().find(|t| {
                        t["px"][0].as_f64() == Some((cell[0] as f32 * grid) as f64)
                            && t["px"][1].as_f64() == Some((cell[1] as f32 * grid) as f64)
                    })
                }) {
                    self.tile_id = tile["t"].as_i64().unwrap_or(0) as i32;
                    self.flips = tile["f"].as_i64().unwrap_or(0) as i32;
                }
            }
            "Entities" => {
                if let Some((index, entity)) = hit_entity(layer, local) {
                    self.entity_index = Some(index);
                    self.entity_uid = entity["defUid"].as_i64();
                    self.panel = Some(Panel::Entity);
                }
            }
            _ => {}
        }
    }

    fn paint_cell(&mut self, cell: [i32; 2], erase: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &mut self.project else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let result = match text(layer, "__type", "") {
            "IntGrid" => project.paint_int_grid(
                key,
                self.layer,
                cell[0],
                cell[1],
                if erase { 0 } else { self.int_value },
            ),
            "Tiles" => {
                let uid = layer["__tilesetDefUid"].as_i64().unwrap_or(-1);
                let Some(tileset) = project.tileset(uid) else {
                    self.status = "Assign a tileset before drawing.".into();
                    return;
                };
                let grid = number(tileset, "tileGridSize", 16.0) as i32;
                let spacing = number(tileset, "spacing", 0.0) as i32;
                let padding = number(tileset, "padding", 0.0) as i32;
                let columns = number(tileset, "__cWid", 1.0).max(1.0) as i32;
                let src = [
                    padding + (self.tile_id % columns) * (grid + spacing),
                    padding + (self.tile_id / columns) * (grid + spacing),
                ];
                project.paint_tile(
                    key,
                    self.layer,
                    cell[0],
                    cell[1],
                    self.tile_id,
                    src,
                    self.flips,
                    erase,
                )
            }
            _ => return,
        };
        match result {
            Ok(changed) => self.dirty |= changed,
            Err(e) => self.status = e.to_string(),
        }
    }

    fn fill(&mut self, start: [i32; 2], erase: bool) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(layer) = self.selected.and_then(|key| project.layer(key, self.layer)) else {
            return;
        };
        let width = number(layer, "__cWid", 0.0) as i32;
        let height = number(layer, "__cHei", 0.0) as i32;
        let cells = layer["intGridCsv"].as_array().cloned().unwrap_or_default();
        let target = cells
            .get((start[0] + start[1] * width) as usize)
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let replacement = if erase { 0 } else { self.int_value as i64 };
        if target == replacement {
            return;
        }
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(cell) = stack.pop() {
            if cell[0] < 0
                || cell[1] < 0
                || cell[0] >= width
                || cell[1] >= height
                || !seen.insert(cell)
            {
                continue;
            }
            if cells
                .get((cell[0] + cell[1] * width) as usize)
                .and_then(Value::as_i64)
                .unwrap_or(0)
                != target
            {
                continue;
            }
            self.paint_cell(cell, erase);
            stack.extend([
                [cell[0] - 1, cell[1]],
                [cell[0] + 1, cell[1]],
                [cell[0], cell[1] - 1],
                [cell[0], cell[1] + 1],
            ]);
        }
    }

    fn edit_entity(&mut self, local: [f32; 2], erase: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(project) = &mut self.project else {
            return;
        };
        if erase {
            let index = project
                .layer(key, self.layer)
                .and_then(|l| hit_entity(l, local).map(|e| e.0));
            if let Some(index) = index {
                match project.remove_entity(key, self.layer, index) {
                    Ok(changed) => self.dirty |= changed,
                    Err(e) => self.status = e.to_string(),
                }
            }
        } else if let Some(uid) = self.entity_uid {
            let grid = project
                .layer(key, self.layer)
                .map(|l| number(l, "__gridSize", 16.0))
                .unwrap_or(16.0);
            let pos = if self.grid {
                [
                    (local[0] / grid).floor() * grid,
                    (local[1] / grid).floor() * grid,
                ]
            } else {
                local
            };
            match project.add_entity(key, self.layer, uid, pos[0] as i32, pos[1] as i32) {
                Ok(changed) => self.dirty |= changed,
                Err(e) => self.status = e.to_string(),
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.loading.is_some() {
            return;
        }
        if self.pending.is_some() {
            egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
                ui.heading("Unsaved changes");
                ui.label("Save your project before continuing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked()
                        && self.save(false)
                        && let Some(action) = self.pending.take()
                    {
                        self.execute(ctx, action);
                    }
                    if ui.button("Discard changes").clicked()
                        && let Some(action) = self.pending.take()
                    {
                        self.execute(ctx, action);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending = None;
                    }
                });
            });
            return;
        }
        let Some(panel) = self.panel else {
            return;
        };
        let title = match panel {
            Panel::Project => "Project settings",
            Panel::Level => "Current level",
            Panel::Layers => "Layers",
            Panel::Entities => "Entities",
            Panel::Enums => "Enums",
            Panel::Tilesets => "Tilesets",
            Panel::Help => "Help & shortcuts",
            Panel::Entity => "Entity instance",
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
                            ui.heading("LDtk • Rust");
                            ui.label("Native Rust port — in development");
                            ui.separator();
                            for line in [
                                "Left mouse: draw  ·  Right mouse: erase",
                                "Alt + click: pick a cell, tile or entity",
                                "Space + drag / middle mouse: pan",
                                "Mouse wheel: zoom under cursor",
                                "F: fit  ·  ~: world view  ·  double click: enter level",
                                "1–0: select layer  ·  G: grid  ·  Tab: hide sidebar",
                                "X / Y: flip tile  ·  Shift+A: single layer",
                                "Cmd/Ctrl+S: save  ·  Cmd/Ctrl+Shift+S: save as",
                                "Cmd/Ctrl+Z: undo  ·  Cmd/Ctrl+Y: redo",
                            ] {
                                ui.label(line);
                            }
                            return;
                        }
                        self.properties(ui, panel);
                    });
            });
        if !open {
            self.panel = None;
        }
    }

    fn properties(&mut self, ui: &mut egui::Ui, panel: Panel) {
        let Some(project) = &mut self.project else {
            return;
        };
        let mut changed = false;
        match panel {
            Panel::Project => {
                changed |= string_field(
                    ui,
                    "Level name pattern",
                    &mut project.json,
                    "levelNamePattern",
                );
                changed |= integer_field(
                    ui,
                    "Default grid",
                    &mut project.json,
                    "defaultGridSize",
                    1..=1024,
                );
                changed |= integer_field(
                    ui,
                    "Default width",
                    &mut project.json,
                    "defaultLevelWidth",
                    1..=65536,
                );
                changed |= integer_field(
                    ui,
                    "Default height",
                    &mut project.json,
                    "defaultLevelHeight",
                    1..=65536,
                );
                changed |= color_field(ui, "Background", &mut project.json, "bgColor");
                changed |= bool_field(ui, "Minify JSON", &mut project.json, "minifyJson");
            }
            Panel::Level => {
                let definitions = project.json["defs"]["levelFields"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if let Some(level) = self.selected.and_then(|key| project.level_mut(key)) {
                    changed |= string_field(ui, "Identifier", level, "identifier");
                    ui.label(format!(
                        "Size: {} × {}",
                        number(level, "pxWid", 0.0),
                        number(level, "pxHei", 0.0)
                    ));
                    ui.small("Level resizing and field definitions are still in development.");
                    changed |= color_field(ui, "Background", level, "__bgColor");
                    if changed {
                        level["bgColor"] = level["__bgColor"].clone();
                    }
                    ui.separator();
                    if let Some(fields) = level["fieldInstances"].as_array_mut() {
                        for field in fields {
                            let definition =
                                definitions.iter().find(|d| d["uid"] == field["defUid"]);
                            changed |= field_input(ui, field, definition);
                        }
                    }
                }
            }
            Panel::Layers => {
                ui.label("Layer definitions");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.new_name);
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("new-layer-kind")
                        .selected_text(&self.new_kind)
                        .show_ui(ui, |ui| {
                            for kind in ["IntGrid", "Tiles", "Entities"] {
                                ui.selectable_value(&mut self.new_kind, kind.to_owned(), kind);
                            }
                        });
                    ui.label("Grid");
                    ui.add(egui::DragValue::new(&mut self.new_grid).range(1..=1024));
                    if ui.button("Add layer").clicked() {
                        let name = if self.new_name.trim().is_empty() {
                            format!(
                                "Layer_{}",
                                project.json["defs"]["layers"]
                                    .as_array()
                                    .map_or(0, Vec::len)
                            )
                        } else {
                            self.new_name.clone()
                        };
                        match project.add_layer(&name, &self.new_kind, self.new_grid) {
                            Ok(uid) => {
                                self.history = History::default();
                                changed = true;
                                self.new_name.clear();
                                if let Some(layers) = self
                                    .selected
                                    .and_then(|k| project.level(k)?["layerInstances"].as_array())
                                {
                                    self.layer = layers
                                        .iter()
                                        .position(|l| l["layerDefUid"].as_i64() == Some(uid))
                                        .unwrap_or(0);
                                }
                            }
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                });
                ui.separator();
                let tilesets = project.json["defs"]["tilesets"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|v| {
                                (
                                    v["uid"].as_i64().unwrap_or(-1),
                                    text(v, "identifier", "Tileset").to_owned(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut assignment = None;
                if let Some(layers) = project.json["defs"]["layers"].as_array_mut() {
                    for layer in layers {
                        ui.collapsing(
                            format!(
                                "{} · {}",
                                text(layer, "identifier", "Layer"),
                                text(layer, "type", "")
                            ),
                            |ui| {
                                ui.label(format!("Grid: {} px", number(layer, "gridSize", 16.0)));
                                ui.label(format!(
                                    "Opacity: {:.0}%",
                                    number(layer, "displayOpacity", 1.0) * 100.0
                                ));
                                if let Some(groups) = layer["autoRuleGroups"].as_array() {
                                    ui.label(format!("{} rule groups", groups.len()));
                                }
                                if text(layer, "type", "") == "Tiles" {
                                    let current = layer["tilesetDefUid"].as_i64().unwrap_or(-1);
                                    let mut selected = current;
                                    let name = tilesets
                                        .iter()
                                        .find(|(uid, _)| *uid == current)
                                        .map(|(_, n)| n.as_str())
                                        .unwrap_or("Select tileset…");
                                    egui::ComboBox::from_id_salt((
                                        "assign-atlas",
                                        layer["uid"].as_i64(),
                                    ))
                                    .selected_text(name)
                                    .show_ui(ui, |ui| {
                                        for (uid, name) in &tilesets {
                                            ui.selectable_value(&mut selected, *uid, name);
                                        }
                                    });
                                    if selected != current {
                                        assignment =
                                            Some((layer["uid"].as_i64().unwrap_or(-1), selected));
                                    }
                                }
                                if let Some(values) = layer["intGridValues"].as_array_mut() {
                                    for value in values.iter_mut() {
                                        changed |= color_field(
                                            ui,
                                            &format!("Value {}", value["value"]),
                                            value,
                                            "color",
                                        );
                                    }
                                }
                            },
                        );
                    }
                }
                if let Some((layer, tileset)) = assignment {
                    match project.assign_tileset(layer, tileset) {
                        Ok(()) => changed = true,
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
            Panel::Entities => {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.new_name);
                });
                if ui.button("Add entity definition").clicked() {
                    let name = if self.new_name.trim().is_empty() {
                        format!(
                            "Entity_{}",
                            project.json["defs"]["entities"]
                                .as_array()
                                .map_or(0, Vec::len)
                        )
                    } else {
                        self.new_name.clone()
                    };
                    match project.add_entity_def(&name, self.new_grid, self.new_grid) {
                        Ok(uid) => {
                            self.entity_uid = Some(uid);
                            changed = true;
                            self.new_name.clear();
                            self.history = History::default();
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
                ui.separator();
                if let Some(defs) = project.json["defs"]["entities"].as_array() {
                    for def in defs {
                        ui.collapsing(text(def, "identifier", "Entity"), |ui| {
                            ui.label(format!(
                                "Size: {} × {}",
                                number(def, "width", 16.0),
                                number(def, "height", 16.0)
                            ));
                            if let Some(fields) = def["fieldDefs"].as_array() {
                                for field in fields {
                                    ui.label(format!(
                                        "{}: {}",
                                        text(field, "identifier", "Field"),
                                        text(field, "__type", "")
                                    ));
                                }
                            }
                        });
                    }
                }
                ui.small("Definition editing is still in development.");
            }
            Panel::Enums => {
                for kind in ["enums", "externalEnums"] {
                    if let Some(defs) = project.json["defs"][kind].as_array() {
                        for def in defs {
                            ui.collapsing(text(def, "identifier", "Enum"), |ui| {
                                if let Some(values) = def["values"].as_array() {
                                    for value in values {
                                        ui.label(text(value, "id", "Value"));
                                    }
                                }
                            });
                        }
                    }
                }
                ui.small("Enum editing and external synchronization are still in development.");
            }
            Panel::Tilesets => {
                ui.horizontal(|ui| {
                    ui.label("Tile size");
                    ui.add(egui::DragValue::new(&mut self.new_grid).range(1..=1024));
                    if ui.button("Import image…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Tileset image", &["png", "jpg", "jpeg", "gif"])
                            .pick_file()
                    {
                        match image::image_dimensions(&path) {
                            Ok((width, height)) => match project.add_tileset(
                                &path,
                                self.new_grid,
                                width as i32,
                                height as i32,
                            ) {
                                Ok(_) => {
                                    changed = true;
                                    self.history = History::default();
                                }
                                Err(e) => self.status = e.to_string(),
                            },
                            Err(e) => self.status = format!("Cannot load tileset: {e}"),
                        }
                    }
                });
                ui.separator();
                if let Some(defs) = project.json["defs"]["tilesets"].as_array() {
                    for def in defs {
                        ui.collapsing(text(def, "identifier", "Tileset"), |ui| {
                            ui.label(text(def, "relPath", "Embedded atlas"));
                            ui.label(format!(
                                "{} × {} px · {} px grid",
                                number(def, "pxWid", 0.0),
                                number(def, "pxHei", 0.0),
                                number(def, "tileGridSize", 16.0)
                            ));
                        });
                    }
                }
                ui.small("Tileset import and definition editing are still in development.");
            }
            Panel::Entity => {
                let definitions = self
                    .selected
                    .and_then(|key| project.layer(key, self.layer))
                    .and_then(|l| l["entityInstances"].as_array())
                    .and_then(|a| self.entity_index.and_then(|i| a.get(i)))
                    .and_then(|e| e["defUid"].as_i64())
                    .and_then(|uid| {
                        project.json["defs"]["entities"]
                            .as_array()?
                            .iter()
                            .find(|d| d["uid"].as_i64() == Some(uid))
                    })
                    .and_then(|d| d["fieldDefs"].as_array())
                    .cloned()
                    .unwrap_or_default();
                let toc_entity = self
                    .selected
                    .and_then(|key| project.layer(key, self.layer))
                    .and_then(|l| l["entityInstances"].as_array())
                    .and_then(|a| self.entity_index.and_then(|i| a.get(i)))
                    .and_then(|e| e["defUid"].as_i64())
                    .is_some_and(|uid| {
                        project.json["defs"]["entities"]
                            .as_array()
                            .is_some_and(|defs| {
                                defs.iter().any(|d| {
                                    d["uid"].as_i64() == Some(uid)
                                        && d["exportToToc"].as_bool() == Some(true)
                                })
                            })
                    });
                if let Some(entity) = self
                    .selected
                    .and_then(|key| project.layer_mut(key, self.layer))
                    .and_then(|l| l["entityInstances"].as_array_mut())
                    .and_then(|a| self.entity_index.and_then(|i| a.get_mut(i)))
                {
                    ui.heading(text(entity, "__identifier", "Entity"));
                    ui.label(format!(
                        "Position: {}, {}",
                        entity["px"][0], entity["px"][1]
                    ));
                    if let Some(fields) = entity["fieldInstances"].as_array_mut() {
                        ui.add_enabled_ui(!toc_entity, |ui| {
                            for field in fields {
                                let definition =
                                    definitions.iter().find(|d| d["uid"] == field["defUid"]);
                                changed |= field_input(ui, field, definition);
                            }
                        });
                    }
                } else {
                    ui.label("Alt + click an entity to inspect it.");
                }
            }
            Panel::Help => {}
        }
        if changed {
            self.mark_property_change();
        }
    }

    fn mark_property_change(&mut self) {
        self.dirty = true;
        self.history = History::default();
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
            "Entities" => {
                let defs = project.json["defs"]["entities"].as_array();
                if !defs.is_some_and(|ds| ds.iter().any(|d| d["uid"].as_i64() == self.entity_uid)) {
                    self.entity_uid = defs
                        .and_then(|ds| ds.first())
                        .and_then(|d| d["uid"].as_i64());
                }
            }
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

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let started = Instant::now();
        self.assets.poll(ctx);
        if let Some(result) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            match result {
                Ok(project) => self.set_project(project),
                Err(e) => self.status = format!("Open failed: {e}"),
            }
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.finish_stroke();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Action::Quit);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.request(ctx, Action::Open(path));
        }
        self.shortcuts(ctx);
        self.top_bar(ctx);
        egui::TopBottomPanel::bottom("status")
            .exact_height(27.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(format!(
                            "{:.0}% · {} visible · CPU {:.2} ms",
                            self.viewport.zoom * 100.0,
                            self.visible_tiles,
                            self.frame_time
                        ));
                        if !self.assets.errors.is_empty() {
                            ui.colored_label(
                                theme::ACCENT,
                                format!("{} missing assets", self.assets.errors.len()),
                            )
                            .on_hover_text(
                                self.assets
                                    .errors
                                    .iter()
                                    .map(|(p, e)| format!("{}: {e}", p.display()))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                        }
                    });
                });
            });
        self.sidebar(ctx);
        self.normalize_palette();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::DARK))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
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
                            },
                            &mut self.assets,
                        );
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
                } else {
                    let area = Rect::from_center_size(rect.center(), egui::vec2(430.0, 330.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new("LDtk")
                                    .size(84.0)
                                    .strong()
                                    .color(theme::ACCENT),
                            );
                            ui.heading("LEVEL DESIGNER TOOLKIT");
                            ui.label("Native Rust edition · in development");
                            ui.add_space(28.0);
                            if ui
                                .add_sized([270.0, 40.0], egui::Button::new("Open project…"))
                                .clicked()
                            {
                                self.request(ctx, Action::Browse);
                            }
                            if ui
                                .add_sized([270.0, 40.0], egui::Button::new("Create project"))
                                .clicked()
                            {
                                self.request(ctx, Action::New);
                            }
                            let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("examples/topdown/Typical_TopDown_example.ldtk");
                            if sample.exists()
                                && ui
                                    .add_sized(
                                        [270.0, 40.0],
                                        egui::Button::new("Open example world"),
                                    )
                                    .clicked()
                            {
                                self.request(ctx, Action::Open(sample));
                            }
                        });
                    });
                }
                if self.loading.is_some() {
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Opening project…",
                        FontId::proportional(24.0),
                        theme::ACCENT,
                    );
                }
            });
        self.dialogs(ctx);
        let title = format!("{}LDtk • Rust", if self.dirty { "● " } else { "" });
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.frame_time = frame
            .info()
            .cpu_usage
            .map(|s| s * 1000.0)
            .unwrap_or(started.elapsed().as_secs_f32() * 1000.0);
    }
}

fn hit_entity(layer: &Value, pos: [f32; 2]) -> Option<(usize, &Value)> {
    layer["entityInstances"]
        .as_array()?
        .iter()
        .enumerate()
        .rev()
        .find(|(_, e)| {
            let w = number(e, "width", 16.0);
            let h = number(e, "height", 16.0);
            let x = e["px"][0].as_f64().unwrap_or(0.0) as f32
                - w * e["__pivot"][0].as_f64().unwrap_or(0.0) as f32;
            let y = e["px"][1].as_f64().unwrap_or(0.0) as f32
                - h * e["__pivot"][1].as_f64().unwrap_or(0.0) as f32;
            pos[0] >= x && pos[1] >= y && pos[0] < x + w && pos[1] < y + h
        })
}

fn string_field(ui: &mut egui::Ui, label: &str, value: &mut Value, key: &str) -> bool {
    let mut current = value[key].as_str().unwrap_or("").to_owned();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.text_edit_singleline(&mut current).changed();
    });
    if changed {
        value[key] = Value::String(current);
    }
    changed
}

fn integer_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Value,
    key: &str,
    range: std::ops::RangeInclusive<i64>,
) -> bool {
    let mut current = value[key].as_i64().unwrap_or(*range.start());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui
            .add(egui::DragValue::new(&mut current).range(range))
            .changed();
    });
    if changed {
        value[key] = current.into();
    }
    changed
}

fn bool_field(ui: &mut egui::Ui, label: &str, value: &mut Value, key: &str) -> bool {
    let mut current = value[key].as_bool().unwrap_or(false);
    let changed = ui.checkbox(&mut current, label).changed();
    if changed {
        value[key] = current.into();
    }
    changed
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

fn field_input(ui: &mut egui::Ui, field: &mut Value, definition: Option<&Value>) -> bool {
    let name = text(field, "__identifier", "Field").to_owned();
    let kind = text(field, "__type", "").to_owned();
    let mut current = field["__value"].clone();
    let value = &mut current;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(name);
        match kind.as_str() {
            "Bool" => {
                let mut v = value.as_bool().unwrap_or(false);
                if ui.checkbox(&mut v, "").changed() {
                    *value = v.into();
                    changed = true;
                }
            }
            "Int" => {
                let mut v = value.as_i64().unwrap_or(0);
                if ui.add(egui::DragValue::new(&mut v)).changed() {
                    *value = v.into();
                    changed = true;
                }
            }
            "Float" => {
                let mut v = value.as_f64().unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed() {
                    *value = v.into();
                    changed = true;
                }
            }
            "String" | "Multilines" => {
                ui.label(value.as_str().unwrap_or(""));
            }
            _ => {
                ui.label(value.to_string());
            }
        }
    });
    changed && crate::fields::write_scalar(field, current, definition).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn undo_does_not_erase_a_later_property_change() {
        let mut app = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        app.history.begin(project, key);
        project.paint_int_grid(key, 0, 0, 0, 1).unwrap();
        app.history.commit(project);
        project.level_mut(key).unwrap()["identifier"] = "Renamed".into();
        app.mark_property_change();
        app.history.undo(app.project.as_mut().unwrap());
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["identifier"],
            "Renamed"
        );
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
        assert!(app.save(false));
        assert!(!app.dirty);
        let reopened = Project::load(&path).unwrap();
        assert_eq!(reopened.json, expected);
    }
}
