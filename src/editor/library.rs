use super::*;
mod home_layout;

fn palette_search(ui: &mut egui::Ui, search: &mut String, columns: &mut usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.;
        let width = (ui.available_width() - 43.).max(30.);
        ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::same(100);
        ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::same(100);
        ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::same(100);
        let response = ui.add_sized(
            [width, 33.],
            egui::TextEdit::singleline(search)
                .hint_text("Search…")
                .margin(egui::Margin {
                    left: 28,
                    right: 8,
                    top: 8,
                    bottom: 8,
                }),
        );
        egui::Image::new(egui::include_image!("../../assets/icons/search.svg")).paint_at(
            ui,
            Rect::from_center_size(
                egui::pos2(response.rect.left() + 16., response.rect.center().y),
                Vec2::splat(17.),
            ),
        );
        ui.spacing_mut().button_padding = egui::vec2(2., 2.);
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
        ui.menu_button(
            egui::Image::new(egui::include_image!("../../assets/icons/gridView.svg"))
                .fit_to_exact_size(Vec2::splat(20.)),
            |ui| {
                for n in 1..=10 {
                    if ui
                        .selectable_label(
                            *columns == n,
                            if n == 1 {
                                "List".to_owned()
                            } else {
                                format!("{n} columns")
                            },
                        )
                        .clicked()
                    {
                        *columns = n;
                        ui.close();
                    }
                }
            },
        );
    });
}
fn palette_group(
    ui: &mut egui::Ui,
    mut open: bool,
    title: &str,
    color: Color32,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    egui::Frame::new()
        .stroke(Stroke::new(1_f32, theme::LIGHT))
        .inner_margin(4)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 17.), Sense::click());
            ui.painter()
                .rect_filled(rect, 0., Color32::from_rgb(115, 127, 157));
            egui::Image::new(if open {
                egui::include_image!("../../assets/icons/expanded.svg")
            } else {
                egui::include_image!("../../assets/icons/collapsed.svg")
            })
            .paint_at(
                ui,
                Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(15.)),
            );
            ui.painter().text(
                rect.left_center() + egui::vec2(24., 0.),
                Align2::LEFT_CENTER,
                title,
                FontId::new(12., egui::FontFamily::Name("panel-title".into())),
                color,
            );
            if response.clicked() {
                open = !open;
            }
            if open {
                ui.add_space(5.);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(5, 0))
                    .show(ui, body);
                ui.add_space(1.);
            }
        });
    ui.add_space(4.);
    open
}

#[derive(Clone)]
struct DirectoryColor {
    path: PathBuf,
    rgb: [u8; 3],
    first: bool,
}
fn directory_color_key() -> egui::Id {
    egui::Id::new("recent-directory-color")
}
impl EditorApp {
    pub(super) fn directory_color_menu(&mut self, ui: &mut egui::Ui, path: &std::path::Path) {
        if ui.button("Assign custom color").clicked() {
            let color = self.settings.dir_color(path);
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    directory_color_key(),
                    DirectoryColor {
                        path: path.to_owned(),
                        rgb: [(color >> 16) as u8, (color >> 8) as u8, color as u8],
                        first: true,
                    },
                )
            });
            ui.close();
        }
        if self.settings.value["recentDirColors"]
            .as_array()
            .is_some_and(|colors| {
                colors
                    .iter()
                    .any(|color| color["path"].as_str() == Some(path.to_string_lossy().as_ref()))
            })
            && ui.button("Reset assigned color").clicked()
        {
            self.settings.force_dir_color(path, None);
            ui.close();
        }
    }
    fn recent_directory_color_dialog(&mut self, ctx: &egui::Context) {
        let id = directory_color_key();
        let Some(mut state) = ctx.data_mut(|d| d.get_temp::<DirectoryColor>(id)) else {
            return;
        };
        let complete = panels::source_colors::directory_color_dialog(
            ctx,
            id.with("picker"),
            &mut state.rgb,
            state.first,
        );
        state.first = false;
        if complete {
            let [r, g, b] = state.rgb;
            self.settings.force_dir_color(
                &state.path,
                Some(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)),
            );
            ctx.data_mut(|d| d.remove::<DirectoryColor>(id));
        } else {
            ctx.data_mut(|d| d.insert_temp(id, state));
        }
    }
}

#[derive(Clone, Copy)]
struct AtlasGrid {
    grid: f32,
    spacing: f32,
    padding: f32,
    columns: i32,
    rows: i32,
}
impl AtlasGrid {
    fn new(def: &Value, size: Vec2) -> Self {
        let grid = number(def, "tileGridSize", 16.).max(1.);
        let spacing = number(def, "spacing", 0.).max(0.);
        let padding = number(def, "padding", 0.).max(0.);
        Self {
            grid,
            spacing,
            padding,
            columns: ((size.x - 2. * padding) / (grid + spacing)).ceil().max(0.) as i32,
            rows: ((size.y - 2. * padding) / (grid + spacing)).ceil().max(0.) as i32,
        }
    }
    fn cell(self, p: Vec2) -> Option<[i32; 2]> {
        let p = p - Vec2::splat(self.padding);
        if p.x < 0. || p.y < 0. {
            return None;
        }
        let stride = self.grid + self.spacing;
        let cell = [(p.x / stride).floor() as i32, (p.y / stride).floor() as i32];
        (cell[0] < self.columns && cell[1] < self.rows).then_some(cell)
    }
    fn rectangle(self, selection: [i32; 4]) -> Rect {
        let [x, y, w, h] = selection;
        Rect::from_min_size(
            egui::pos2(
                self.padding + x as f32 * (self.grid + self.spacing),
                self.padding + y as f32 * (self.grid + self.spacing),
            ),
            egui::vec2(
                w as f32 * (self.grid + self.spacing) - self.spacing,
                h as f32 * (self.grid + self.spacing) - self.spacing,
            ),
        )
    }
}
fn selection_between(a: [i32; 2], b: [i32; 2]) -> [i32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        (a[0] - b[0]).abs() + 1,
        (a[1] - b[1]).abs() + 1,
    ]
}

#[derive(Clone)]
struct PickerState {
    tile_id: i32,
    tile_rect: [i32; 4],
    ids: Option<(i64, Vec<i32>)>,
    flips: i32,
    random: bool,
}
impl Default for PickerState {
    fn default() -> Self {
        Self {
            tile_id: 0,
            tile_rect: [0, 0, 1, 1],
            ids: None,
            flips: 0,
            random: false,
        }
    }
}
impl PickerState {
    fn take(app: &mut EditorApp) -> Self {
        Self {
            tile_id: app.tile_id,
            tile_rect: app.tile_rect,
            ids: app.atlas_selection.take(),
            flips: app.flips,
            random: app.random_tiles,
        }
    }
    fn install(self, app: &mut EditorApp) {
        app.tile_id = self.tile_id;
        app.tile_rect = self.tile_rect;
        app.atlas_selection = self.ids;
        app.flips = self.flips;
        app.random_tiles = self.random;
    }
}
#[derive(Clone)]
struct AtlasDrag {
    start: [i32; 2],
    button: egui::PointerButton,
}
impl EditorApp {
    pub(super) fn with_local_atlas_state<R>(
        &mut self,
        ctx: &egui::Context,
        key: egui::Id,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let project = self
            .project
            .as_ref()
            .and_then(|p| p.json["iid"].as_str())
            .unwrap_or("")
            .to_owned();
        let id = key.with(("picker-state", project));
        let canvas = PickerState::take(self);
        ctx.data_mut(|d| d.get_temp::<PickerState>(id))
            .unwrap_or_default()
            .install(self);
        let result = body(self);
        let selected = PickerState::take(self);
        ctx.data_mut(|d| d.insert_temp(id, selected));
        canvas.install(self);
        result
    }

    pub(super) fn library_palette(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(key) = self.selected else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let layer_uid = layer["layerDefUid"].as_i64().unwrap_or(-1);
        let layer_type = text(layer, "__type", "");
        let mut optional_change = None;
        let mut open_definition = None;
        let mut source = None;
        let mut atlas_uid = None;
        match layer_type {
            "IntGrid" => {
                if let Some(def) = project.layer_def(layer_uid) {
                    if let Some(values) = def["intGridValues"].as_array() {
                        let search_id = ui.make_persistent_id(("intgrid-search", layer_uid));
                        let mut search = ui
                            .data_mut(|d| d.get_temp::<String>(search_id))
                            .unwrap_or_default();
                        let columns_key = format!("IntGridPaletteColumns_{layer_uid}");
                        let project_iid = project.json["iid"].as_str();
                        let mut columns = self
                            .settings
                            .ui_state_int(&columns_key, project_iid, 1)
                            .clamp(1, 10) as usize;
                        let previous_columns = columns;
                        palette_search(ui, &mut search, &mut columns);
                        if columns != previous_columns {
                            self.settings.set_ui_state_int(
                                &columns_key,
                                project_iid,
                                columns as i64,
                            );
                        }
                        ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
                        let search = search.to_lowercase();
                        let mut groups = vec![(0, "Ungrouped".to_owned(), None)];
                        groups.extend(
                            def["intGridValuesGroups"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .map(|g| {
                                    (
                                        g["uid"].as_i64().unwrap_or(0),
                                        text(g, "identifier", "Group").to_owned(),
                                        g["color"].as_str().map(theme::color),
                                    )
                                }),
                        );
                        let count = groups
                            .iter()
                            .filter(|(uid, _, _)| {
                                values
                                    .iter()
                                    .any(|v| v["groupUid"].as_i64().unwrap_or(0) == *uid)
                            })
                            .count();
                        for (uid, name, group_color) in groups {
                            let group = values
                                .iter()
                                .filter(|v| v["groupUid"].as_i64().unwrap_or(0) == uid)
                                .filter(|v| {
                                    search.is_empty()
                                        || text(v, "identifier", "")
                                            .to_lowercase()
                                            .contains(&search)
                                        || v["value"].to_string().contains(&search)
                                })
                                .collect::<Vec<_>>();
                            if group.is_empty() {
                                continue;
                            }
                            let mut draw = |ui: &mut egui::Ui| {
                                let width = if columns == 1 {
                                    ui.available_width()
                                } else {
                                    (ui.available_width() - (columns - 1) as f32) / columns as f32
                                };
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(1., 1.);
                                    for value in &group {
                                        let id = value["value"].as_i64().unwrap_or(1) as i32;
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(
                                                width,
                                                if columns == 1 { 36. } else { width + 18. },
                                            ),
                                            Sense::click(),
                                        );
                                        let selected = self.int_value == id;
                                        let color = theme::color(text(value, "color", "#ffcc00"));
                                        ui.painter().rect_filled(
                                            rect,
                                            2.,
                                            if selected {
                                                mix_color(color, Color32::WHITE, 0.66)
                                            } else if response.hovered() {
                                                mix_color(color, theme::MEDIUM, 0.5)
                                            } else {
                                                mix_color(
                                                    color,
                                                    Color32::from_rgb(49, 56, 67),
                                                    0.72,
                                                )
                                            },
                                        );
                                        let icon = Rect::from_center_size(
                                            if columns == 1 {
                                                egui::pos2(rect.left() + 19., rect.center().y)
                                            } else {
                                                egui::pos2(rect.center().x, rect.top() + width / 2.)
                                            },
                                            Vec2::splat(if columns == 1 {
                                                24.
                                            } else {
                                                (width - 4.).max(4.)
                                            }),
                                        );
                                        ui.painter().rect_filled(
                                            icon,
                                            0.,
                                            theme::color(text(value, "color", "#ffcc00")),
                                        );
                                        if !value["tile"].is_null() {
                                            paint_tile(
                                                ctx,
                                                ui.painter(),
                                                icon,
                                                project,
                                                &value["tile"],
                                                &mut self.assets,
                                            );
                                        }
                                        ui.painter().text(
                                            if columns == 1 {
                                                egui::pos2(rect.left() + 39., rect.center().y)
                                            } else {
                                                egui::pos2(rect.center().x, rect.bottom() - 9.)
                                            },
                                            if columns == 1 {
                                                Align2::LEFT_CENTER
                                            } else {
                                                Align2::CENTER_CENTER
                                            },
                                            format!(
                                                "{}  {}",
                                                id,
                                                text(value, "identifier", "Value")
                                            ),
                                            FontId::proportional(if columns == 1 {
                                                15.
                                            } else {
                                                10.
                                            }),
                                            if selected {
                                                Color32::BLACK
                                            } else {
                                                mix_color(color, Color32::WHITE, 0.3)
                                            },
                                        );
                                        if response.clicked() {
                                            self.int_value = id;
                                        }
                                    }
                                });
                            };
                            if count > 1 {
                                let group_key = format!(
                                    "{}_intGridPalette_{}_group_{}",
                                    text(&project.json, "iid", ""),
                                    layer_uid,
                                    uid
                                );
                                let group_open =
                                    self.settings.ui_state_int(&group_key, None, 1) != 0;
                                let group_next = palette_group(
                                    ui,
                                    group_open,
                                    &name.to_uppercase(),
                                    group_color.unwrap_or(theme::ACCENT),
                                    draw,
                                );
                                if group_next != group_open {
                                    self.settings.set_ui_state_int(
                                        &group_key,
                                        None,
                                        i64::from(group_next),
                                    );
                                }
                            } else {
                                draw(ui);
                            }
                        }
                    }
                    let groups = def["autoRuleGroups"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    if !groups.is_empty() {
                        ui.separator();
                        ui.checkbox(&mut self.render_auto, "Display auto-layer tiles");
                    }
                    for group in groups.iter().filter(|g| g["isOptional"] == true) {
                        let uid = group["uid"].as_i64().unwrap_or(-1);
                        let mut enabled = layer["optionalRules"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|v| v.as_i64() == Some(uid)));
                        if ui
                            .checkbox(&mut enabled, text(group, "name", "Optional rules"))
                            .changed()
                        {
                            optional_change = Some((uid, enabled));
                        }
                    }
                }
            }
            "Entities" => {
                if let Some(entities) = project.json["defs"]["entities"].as_array() {
                    let layer_def = project.layer_def(layer_uid).unwrap_or(&Value::Null);
                    let search_id = ui.make_persistent_id(("entity-palette-search", layer_uid));
                    let mut search = ui
                        .data_mut(|d| d.get_temp::<String>(search_id))
                        .unwrap_or_default();
                    let columns_key = format!("EntityPaletteColumns_{layer_uid}");
                    let project_iid = project.json["iid"].as_str();
                    let mut columns = self
                        .settings
                        .ui_state_int(&columns_key, project_iid, 1)
                        .clamp(1, 10) as usize;
                    let previous_columns = columns;
                    palette_search(ui, &mut search, &mut columns);
                    if columns != previous_columns {
                        self.settings
                            .set_ui_state_int(&columns_key, project_iid, columns as i64);
                    }
                    ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
                    let search = search.to_lowercase();
                    let definitions = entities
                        .iter()
                        .filter(|e| entity_allowed(e, layer_def))
                        .collect::<Vec<_>>();
                    if !definitions
                        .iter()
                        .any(|e| e["uid"].as_i64() == self.entity_uid)
                    {
                        self.entity_uid = definitions.first().and_then(|e| e["uid"].as_i64());
                    }
                    let mut tags = definitions
                        .iter()
                        .flat_map(|d| {
                            d["tags"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                        })
                        .collect::<Vec<_>>();
                    tags.sort_by_key(|s| s.to_lowercase());
                    tags.dedup();
                    if definitions
                        .iter()
                        .any(|d| d["tags"].as_array().is_none_or(Vec::is_empty))
                    {
                        tags.insert(0, "");
                    }
                    for tag in &tags {
                        let group = definitions
                            .iter()
                            .filter(|d| {
                                if tag.is_empty() {
                                    d["tags"].as_array().is_none_or(Vec::is_empty)
                                } else {
                                    d["tags"]
                                        .as_array()
                                        .is_some_and(|a| a.iter().any(|t| t == tag))
                                }
                            })
                            .filter(|d| {
                                search.is_empty()
                                    || text(d, "identifier", "").to_lowercase().contains(&search)
                            })
                            .collect::<Vec<_>>();
                        if group.is_empty() {
                            continue;
                        }
                        let mut draw = |ui: &mut egui::Ui| {
                            let width = if columns == 1 {
                                ui.available_width()
                            } else {
                                (ui.available_width() - (columns - 1) as f32) / columns as f32
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(1., 1.);
                                for def in &group {
                                    let uid = def["uid"].as_i64().unwrap_or(-1);
                                    let color = theme::color(text(def, "color", "#ffcc00"));
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(
                                            width,
                                            if columns == 1 { 44. } else { width + 20. },
                                        ),
                                        Sense::click(),
                                    );
                                    let selected = self.entity_uid == Some(uid);
                                    ui.painter().rect_filled(
                                        rect,
                                        0.,
                                        if selected {
                                            mix_color(color, Color32::WHITE, 0.66)
                                        } else {
                                            mix_color(
                                                color,
                                                Color32::from_rgb(49, 56, 67),
                                                if response.hovered() { 0.5 } else { 0.72 },
                                            )
                                        },
                                    );
                                    ui.painter().rect_filled(
                                        Rect::from_min_size(
                                            rect.min,
                                            egui::vec2(6., rect.height()),
                                        ),
                                        0.,
                                        color,
                                    );
                                    if selected || response.hovered() {
                                        ui.painter().rect_stroke(
                                            rect,
                                            0.,
                                            Stroke::new(1.0_f32, Color32::WHITE),
                                            StrokeKind::Inside,
                                        );
                                    }
                                    let preview = Rect::from_min_size(
                                        rect.min
                                            + if columns == 1 {
                                                egui::vec2(14., 6.)
                                            } else {
                                                egui::vec2(2., 2.)
                                            },
                                        if columns == 1 {
                                            Vec2::splat(32.)
                                        } else {
                                            Vec2::splat((width - 4.).max(4.))
                                        },
                                    );
                                    canvas::entity_preview(
                                        ctx,
                                        ui.painter(),
                                        preview,
                                        project,
                                        def,
                                        &mut self.assets,
                                    );
                                    ui.painter().text(
                                        if columns == 1 {
                                            egui::pos2(rect.left() + 62., rect.center().y)
                                        } else {
                                            egui::pos2(rect.center().x, rect.bottom() - 10.)
                                        },
                                        if columns == 1 {
                                            Align2::LEFT_CENTER
                                        } else {
                                            Align2::CENTER_CENTER
                                        },
                                        text(def, "identifier", "Entity"),
                                        FontId::proportional(if columns == 1 { 16. } else { 11. }),
                                        if selected {
                                            Color32::BLACK
                                        } else {
                                            mix_color(color, Color32::WHITE, 0.3)
                                        },
                                    );
                                    if response.clicked() {
                                        self.entity_uid = Some(uid);
                                    }
                                    response.clone().on_hover_text(text(def, "doc", ""));
                                    response.context_menu(|ui| {
                                        if ui.button("Edit entity definition").clicked() {
                                            self.definition_uid = Some(uid);
                                            open_definition = Some(uid);
                                            ui.close();
                                        }
                                    });
                                }
                            });
                        };
                        if tags.len() > 1 {
                            let group_key = format!(
                                "{}_entityPalette_{}_tag_{}",
                                text(&project.json, "iid", ""),
                                layer_uid,
                                if tag.is_empty() { "null" } else { tag }
                            );
                            let group_open = self.settings.ui_state_int(&group_key, None, 1) != 0;
                            let group_next = palette_group(
                                ui,
                                group_open,
                                &if tag.is_empty() {
                                    "UNTAGGED".into()
                                } else {
                                    tag.to_uppercase()
                                },
                                theme::ACCENT,
                                draw,
                            );
                            if group_next != group_open {
                                self.settings.set_ui_state_int(
                                    &group_key,
                                    None,
                                    i64::from(group_next),
                                );
                            }
                        } else {
                            draw(ui);
                        }
                    }
                }
            }
            "Tiles" | "AutoLayer" => {
                atlas_uid = layer["overrideTilesetUid"]
                    .as_i64()
                    .or_else(|| layer["__tilesetDefUid"].as_i64());
                if layer_type == "AutoLayer" {
                    ui.label("Generated from an IntGrid layer");
                    let source_uid = project
                        .layer_def(layer_uid)
                        .and_then(|d| d["autoSourceLayerDefUid"].as_i64());
                    if let Some(uid) = source_uid
                        && ui.button("Edit source IntGrid").clicked()
                    {
                        source = project
                            .level(key)
                            .and_then(|l| l["layerInstances"].as_array())
                            .and_then(|layers| {
                                layers
                                    .iter()
                                    .position(|l| l["layerDefUid"].as_i64() == Some(uid))
                            });
                    }
                }
            }
            _ => {}
        }
        if let Some(uid) = open_definition {
            self.definition_uid = Some(uid);
            self.show_panel(Some(Panel::Entities));
        }
        if let Some((uid, enabled)) = optional_change {
            let layer_index = self.layer;
            self.edit_level("Optional auto-layer rules changed", key, |p| {
                let layer = p
                    .layer_mut(key, layer_index)
                    .ok_or_else(|| ldtk_core::Error::Invalid("Layer is missing".into()))?;
                let mut groups = layer["optionalRules"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                groups.retain(|v| v.as_i64() != Some(uid));
                if enabled {
                    groups.push(serde_json::json!(uid));
                }
                layer["optionalRules"] = serde_json::json!(groups);
                p.rebuild_auto_layers(key)
            });
        }
        if let Some(index) = source {
            self.layer = index;
        }
        if let Some(uid) = atlas_uid {
            self.restore_canvas_atlas_memory(ctx, uid);
            self.tile_palette(ctx, ui, uid);
            self.remember_canvas_atlas_memory(ctx, uid);
        }
    }

    fn canvas_atlas_memory_ids(&self, uid: i64) -> Option<(egui::Id, egui::Id, String)> {
        let project = self.project.as_ref()?;
        let atlas = project.tileset(uid)?;
        let project = text(&project.json, "iid", "");
        let source = atlas["relPath"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("embedded:{}", atlas["embedAtlas"]));
        Some((
            egui::Id::new(("canvas-atlas-current", project)),
            egui::Id::new(("canvas-atlas-memory", project, &source)),
            source,
        ))
    }
    fn restore_canvas_atlas_memory(&mut self, ctx: &egui::Context, uid: i64) {
        let Some((current, key, source)) = self.canvas_atlas_memory_ids(uid) else {
            return;
        };
        let previous = ctx.data_mut(|d| d.get_temp::<String>(current));
        if previous.as_ref() == Some(&source)
            && self
                .atlas_selection
                .as_ref()
                .is_some_and(|(atlas, _)| *atlas == uid)
        {
            return;
        }
        let memory = ctx
            .data_mut(|d| d.get_temp::<(Vec<i32>, bool)>(key))
            .or_else(|| {
                self.project.as_ref()?.tileset(uid)?["savedSelections"]
                    .as_array()?
                    .iter()
                    .find(|s| {
                        s["ids"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|id| id == 0))
                    })
                    .map(|saved| {
                        (
                            saved["ids"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_i64)
                                .filter_map(|id| i32::try_from(id).ok())
                                .collect(),
                            saved["mode"] == "Random",
                        )
                    })
            })
            .unwrap_or_else(|| (vec![0], false));
        if !self.set_atlas_selection(uid, memory.0) {
            self.set_atlas_selection(uid, vec![0]);
        }
        self.random_tiles = memory.1;
        ctx.data_mut(|d| d.insert_temp(current, source));
    }
    fn remember_canvas_atlas_memory(&self, ctx: &egui::Context, uid: i64) {
        if let Some((_, key, _)) = self.canvas_atlas_memory_ids(uid) {
            ctx.data_mut(|d| {
                d.insert_temp(key, (self.selected_tile_ids_for(uid), self.random_tiles))
            });
        }
    }
    fn apply_atlas_mouse_selection(
        &mut self,
        uid: i64,
        mut ids: Vec<i32>,
        secondary: bool,
        modifiers: egui::Modifiers,
        recall_saved: bool,
    ) {
        let previous = self.selected_tile_ids_for(uid);
        let control = modifiers.ctrl || modifiers.command;
        let subtract = secondary || control && ids.first().is_some_and(|id| previous.contains(id));
        if recall_saved
            && ids.len() == 1
            && !control
            && let Some(saved) = self
                .project
                .as_ref()
                .and_then(|p| p.tileset(uid))
                .and_then(|d| d["savedSelections"].as_array())
                .and_then(|a| {
                    a.iter().find(|s| {
                        s["ids"].as_array().is_some_and(|a| {
                            a.iter().any(|id| id.as_i64() == Some(i64::from(ids[0])))
                        })
                    })
                })
                .cloned()
        {
            let saved_ids = saved["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .filter_map(|id| i32::try_from(id).ok())
                .collect::<Vec<_>>();
            if saved_ids.len() != previous.len()
                || !previous.iter().all(|id| saved_ids.contains(id))
            {
                ids = saved_ids;
                self.random_tiles = saved["mode"] == "Random";
            }
        }
        let selected = if subtract {
            let mut remaining = previous;
            let mut index = 0;
            while index < remaining.len() && remaining.len() > 1 {
                if ids.contains(&remaining[index]) {
                    remaining.remove(index);
                } else {
                    index += 1;
                }
            }
            remaining
        } else if modifiers.shift || control {
            previous
                .into_iter()
                .chain(ids)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            ids
        };
        self.set_atlas_selection(uid, selected);
    }
    pub(super) fn atlas_picker(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, uid: i64) {
        self.atlas_picker_mode(ctx, ui, uid, false);
    }
    pub(super) fn atlas_rect_picker(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, uid: i64) {
        self.atlas_picker_mode(ctx, ui, uid, true);
    }
    fn atlas_picker_mode(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        rectangle_only: bool,
    ) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(def) = project.tileset(uid).cloned() else {
            ui.weak("Choose a tileset");
            return;
        };
        let canvas = palette_popout::canvas_view(ctx);
        if canvas.is_none() {
            ui.label(text(&def, "identifier", "Tileset"));
        }
        let texture = if let Some(path) = def["relPath"].as_str() {
            self.assets.texture(ctx, &canvas::asset_path(project, path))
        } else if !def["embedAtlas"].is_null() {
            self.assets.embedded(ctx)
        } else {
            ui.weak("Assign an atlas image in Tilesets.");
            return;
        };
        let Some(texture) = texture else {
            if let Some(path) = def["relPath"].as_str() {
                let path = canvas::asset_path(project, path);
                if let Some(error) = self.assets.errors.get(&path).cloned() {
                    ui.colored_label(Color32::LIGHT_RED, error);
                    if ui.button("Reload atlas").clicked() {
                        self.assets.invalidate(&path);
                    }
                    return;
                }
            }
            ui.spinner();
            return;
        };
        let natural = texture.size_vec2();
        let grid = AtlasGrid::new(&def, natural);
        let zoom_id = ui.make_persistent_id(("atlas-zoom", uid));
        let mut zoom = ui.data_mut(|d| d.get_temp::<f32>(zoom_id)).unwrap_or(3.);
        if canvas == Some(true) {
            zoom = (ui.available_width() / natural.x)
                .min(ui.available_height().max(8.) / natural.y)
                .max(0.01);
        }
        let scroll_id = ui.make_persistent_id(egui::Id::new(("atlas-picker", uid)));
        let viewport_id = zoom_id.with("viewport");
        let pan_id = zoom_id.with("pan");
        let mut pan = ui.data_mut(|d| d.get_temp::<egui::PointerButton>(pan_id));
        let viewport = ui.data_mut(|d| d.get_temp::<Rect>(viewport_id));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut state = egui::scroll_area::State::load(ctx, scroll_id).unwrap_or_default();
        if let Some(viewport) = viewport
            && let Some(pointer) = pointer.filter(|p| viewport.contains(*p))
        {
            let delta = ui.input(|i| i.raw_scroll_delta.y);
            if delta != 0. {
                if canvas != Some(true) {
                    let previous = zoom;
                    zoom = (zoom * (1. + delta * 0.001)).clamp(0.5, 6.);
                    let anchor = pointer - viewport.min;
                    state.offset = (state.offset + anchor) * (zoom / previous) - anchor;
                }
                ui.input_mut(|i| {
                    i.smooth_scroll_delta = Vec2::ZERO;
                    i.raw_scroll_delta = Vec2::ZERO;
                });
            }
            if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle))
                && canvas != Some(true)
            {
                pan = Some(egui::PointerButton::Middle);
            } else if ui.input(|i| i.key_down(Key::Space)) {
                for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
                    if ui.input(|i| i.pointer.button_down(button)) {
                        pan = Some(button);
                    }
                }
            }
        }
        let panning = pan.is_some();
        if let Some(button) = pan {
            if ui.input(|i| i.pointer.button_down(button)) {
                state.offset -= ui.input(|i| i.pointer.delta());
            } else {
                pan = None;
            }
        }
        ui.data_mut(|d| {
            if let Some(button) = pan {
                d.insert_temp(pan_id, button);
            } else {
                d.remove::<egui::PointerButton>(pan_id);
            }
            d.insert_temp(zoom_id, zoom);
        });
        state.store(ctx, scroll_id);
        let max_height = if canvas.is_some() {
            ui.available_height().max(8.)
        } else {
            600.
        };
        let scroll = egui::ScrollArea::both()
            .scroll_source(egui::scroll_area::ScrollSource {
                scroll_bar: true,
                drag: false,
                mouse_wheel: false,
            })
            .id_salt(("atlas-picker", uid))
            .max_height(max_height)
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(natural * zoom, Sense::click_and_drag());
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                    .paint_at(ui, rect);
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                let drag_id = ui.make_persistent_id(("atlas-selection-drag", uid));
                let mut drag = ui.data_mut(|d| d.get_temp::<AtlasDrag>(drag_id));
                if panning {
                    drag = None;
                }
                if !panning
                    && response.contains_pointer()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                    && let Some(cell) = grid.cell((pos - rect.min) / zoom)
                {
                    for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
                        if ui.input(|i| i.pointer.button_pressed(button)) {
                            if rectangle_only && button == egui::PointerButton::Secondary {
                                self.atlas_selection = Some((uid, vec![]));
                                drag = None;
                            } else {
                                drag = Some(AtlasDrag {
                                    start: cell,
                                    button,
                                });
                            }
                        }
                    }
                }
                if let Some(active) = &drag
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                    && grid.columns > 0
                    && grid.rows > 0
                {
                    let local = (pos - rect.min) / zoom - Vec2::splat(grid.padding);
                    let stride = grid.grid + grid.spacing;
                    let cell = [
                        ((local.x / stride).floor() as i32).clamp(0, grid.columns - 1),
                        ((local.y / stride).floor() as i32).clamp(0, grid.rows - 1),
                    ];
                    let selection = selection_between(active.start, cell);
                    let source = grid.rectangle(selection);
                    let target = Rect::from_min_size(
                        rect.min + source.min.to_vec2() * zoom,
                        source.size() * zoom,
                    );
                    ui.painter().rect_stroke(
                        target,
                        0.,
                        Stroke::new(
                            1_f32,
                            if active.button == egui::PointerButton::Secondary {
                                Color32::LIGHT_RED
                            } else {
                                Color32::WHITE
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    if ui.input(|i| i.pointer.button_released(active.button)) {
                        let ids = (selection[0]..selection[0] + selection[2])
                            .flat_map(|x| {
                                (selection[1]..selection[1] + selection[3])
                                    .map(move |y| x + y * grid.columns)
                            })
                            .collect();
                        let modifiers = ui.input(|i| i.modifiers);
                        if rectangle_only {
                            self.set_atlas_selection(uid, ids);
                        } else {
                            self.apply_atlas_mouse_selection(
                                uid,
                                ids,
                                active.button == egui::PointerButton::Secondary,
                                modifiers,
                                self.panel.is_none(),
                            );
                        }
                        drag = None;
                    } else if !ui.input(|i| i.pointer.button_down(active.button)) {
                        drag = None;
                    }
                }
                if let Some(drag) = drag {
                    ui.data_mut(|d| d.insert_temp(drag_id, drag));
                } else {
                    ui.data_mut(|d| d.remove::<AtlasDrag>(drag_id));
                }
                if self.tile_tags {
                    for tag in def["enumTags"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                        let label = text(tag, "enumValueId", "");
                        for id in tag["tileIds"]
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                            .iter()
                            .filter_map(Value::as_i64)
                        {
                            if grid.columns == 0 {
                                continue;
                            }
                            let source = grid.rectangle([
                                id as i32 % grid.columns,
                                id as i32 / grid.columns,
                                1,
                                1,
                            ]);
                            let target = Rect::from_min_size(
                                rect.min + source.min.to_vec2() * zoom,
                                source.size() * zoom,
                            );
                            if ui.is_rect_visible(target) {
                                ui.painter().rect_stroke(
                                    target,
                                    0.,
                                    Stroke::new(1.0_f32, theme::ACCENT),
                                    StrokeKind::Inside,
                                );
                                if target.width() >= 32. {
                                    ui.painter().text(
                                        target.center(),
                                        Align2::CENTER_CENTER,
                                        label,
                                        FontId::proportional(9.),
                                        theme::ACCENT,
                                    );
                                }
                            }
                        }
                    }
                }
                let selections = if let Some((selected_uid, ids)) = &self.atlas_selection
                    && *selected_uid == uid
                {
                    ids.iter()
                        .filter(|id| **id >= 0 && **id < grid.columns * grid.rows)
                        .map(|id| [*id % grid.columns.max(1), *id / grid.columns.max(1), 1, 1])
                        .collect::<Vec<_>>()
                } else {
                    vec![self.tile_rect]
                };
                for selection in selections {
                    let source = grid.rectangle(selection);
                    let target = Rect::from_min_size(
                        rect.min + source.min.to_vec2() * zoom,
                        source.size() * zoom,
                    );
                    ui.painter().rect_stroke(
                        target,
                        0.,
                        Stroke::new(3_f32, Color32::BLACK),
                        StrokeKind::Outside,
                    );
                    ui.painter().rect_stroke(
                        target,
                        0.,
                        Stroke::new(2_f32, theme::ACCENT),
                        StrokeKind::Inside,
                    );
                }
                if let Some(pos) = response.hover_pos()
                    && let Some(cell) = grid.cell((pos - rect.min) / zoom)
                {
                    response.clone().on_hover_text(format!(
                        "Tile #{}  ({}, {})",
                        cell[0] + cell[1] * grid.columns,
                        cell[0],
                        cell[1]
                    ));
                }
            });
        ui.data_mut(|d| d.insert_temp(viewport_id, scroll.inner_rect));
        if canvas.is_none() {
            ui.small(format!(
                "Tile #{} · {} × {} tiles",
                self.tile_id, self.tile_rect[2], self.tile_rect[3]
            ));
        }
    }
}
pub(super) fn entity_allowed(entity: &Value, layer: &Value) -> bool {
    let tags = entity["tags"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let required = layer["requiredTags"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let excluded = layer["excludedTags"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    !excluded.iter().any(|t| tags.contains(t))
        && (required.is_empty() || required.iter().any(|t| tags.contains(t)))
}
fn mix_color(a: Color32, b: Color32, r: f32) -> Color32 {
    Color32::from_rgb(
        (a.r() as f32 * (1. - r) + b.r() as f32 * r) as u8,
        (a.g() as f32 * (1. - r) + b.g() as f32 * r) as u8,
        (a.b() as f32 * (1. - r) + b.b() as f32 * r) as u8,
    )
}
fn project_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut paths = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ldtk"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}
fn paint_tile(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: Rect,
    project: &Project,
    tile: &Value,
    assets: &mut Assets,
) {
    let Some(def) = tile["tilesetUid"]
        .as_i64()
        .and_then(|id| project.tileset(id))
    else {
        return;
    };
    let texture = if let Some(path) = def["relPath"].as_str() {
        assets.texture(ctx, &canvas::asset_path(project, path))
    } else if !def["embedAtlas"].is_null() {
        assets.embedded(ctx)
    } else {
        None
    };
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let src = Rect::from_min_size(
            egui::pos2(number(tile, "x", 0.), number(tile, "y", 0.)),
            egui::vec2(number(tile, "w", 16.), number(tile, "h", 16.)),
        );
        let scale = (rect.width() / src.width()).min(rect.height() / src.height());
        let dst = Rect::from_center_size(rect.center(), src.size() * scale);
        painter.image(
            texture.id(),
            dst,
            Rect::from_min_max(
                egui::pos2(src.left() / size.x, src.top() / size.y),
                egui::pos2(src.right() / size.x, src.bottom() / size.y),
            ),
            Color32::WHITE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_picker_starts_at_three_and_wheel_space_pan_preserve_canvas_brush() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let image = ldtk_core::exports::embedded_atlas().unwrap();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = serde_json::json!([{"uid":10,"identifier":"Icons","embedAtlas":"LdtkIcons","tileGridSize":16,"pxWid":image.width(),"pxHei":image.height(),"__cWid":image.width().div_ceil(16),"__cHei":image.height().div_ceil(16)}]);
        app.set_project(p);
        app.set_atlas_selection(10, vec![1, 3]);
        app.flips = 3;
        app.random_tiles = true;
        let texture = app.assets.embedded(&ctx).unwrap().id();
        let scroll_id = std::cell::Cell::new(egui::Id::NULL);
        let run = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let mut child =
                            ui.new_child(egui::UiBuilder::new().id_salt("local-icon").max_rect(
                                Rect::from_min_size(ui.cursor().min, egui::vec2(350., 260.)),
                            ));
                        scroll_id
                            .set(child.make_persistent_id(egui::Id::new(("atlas-picker", 10_i64))));
                        app.with_local_atlas_state(ctx, egui::Id::new("local-icon-state"), |app| {
                            app.atlas_picker(ctx, &mut child, 10)
                        });
                    });
                },
            )
        };
        run(&mut app, vec![]);
        let first = run(&mut app, vec![]);
        let atlas = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Mesh(m) if m.texture_id == texture => Some(m.calc_bounds()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            atlas(&first).width(),
            image.width() as f32 * 3.,
            "source initial3x"
        );
        let pointer = egui::pos2(100., 100.);
        run(&mut app, vec![egui::Event::PointerMoved(pointer)]);
        let zoomed = run(
            &mut app,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., 100.),
                modifiers: Default::default(),
            }],
        );
        assert!((atlas(&zoomed).width() / atlas(&first).width() - 1.1).abs() < 0.001);
        let before = egui::scroll_area::State::load(&ctx, scroll_id.get())
            .unwrap()
            .offset;
        let key = |pressed| egui::Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        run(
            &mut app,
            vec![
                key(true),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        run(
            &mut app,
            vec![egui::Event::PointerMoved(pointer - egui::vec2(20., 20.))],
        );
        let after = egui::scroll_area::State::load(&ctx, scroll_id.get())
            .unwrap()
            .offset;
        assert!(
            (after.x - before.x - 20.).abs() < 0.01 && (after.y - before.y - 20.).abs() < 0.01,
            "source Space pan: {before:?}->{after:?}"
        );
        run(
            &mut app,
            vec![
                egui::Event::PointerButton {
                    pos: pointer - egui::vec2(20., 20.),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
                key(false),
            ],
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![1, 3]);
        assert_eq!(app.flips, 3);
        assert!(app.random_tiles);
    }
    #[test]
    fn canvas_atlas_memory_follows_resource_path_and_preserves_sparse_selection_mode() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = serde_json::json!([
            {"uid":10,"relPath":"atlas.png","tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4,"savedSelections":[{"ids":[0,2,8],"mode":"Random"}]},
            {"uid":11,"relPath":"other.png","tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4},
            {"uid":12,"relPath":"atlas.png","tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4}]);
        app.set_project(p);
        app.restore_canvas_atlas_memory(&ctx, 10);
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 2, 8]);
        assert!(app.random_tiles);
        app.set_atlas_selection(10, vec![3, 7]);
        app.random_tiles = false;
        app.remember_canvas_atlas_memory(&ctx, 10);
        app.restore_canvas_atlas_memory(&ctx, 11);
        assert_eq!(app.selected_tile_ids_for(11), vec![0]);
        app.set_atlas_selection(11, vec![5]);
        app.random_tiles = true;
        app.remember_canvas_atlas_memory(&ctx, 11);
        app.restore_canvas_atlas_memory(&ctx, 12);
        assert_eq!(app.selected_tile_ids_for(12), vec![3, 7]);
        assert!(!app.random_tiles);
        app.restore_canvas_atlas_memory(&ctx, 11);
        assert_eq!(app.selected_tile_ids_for(11), vec![5]);
        assert!(app.random_tiles);
    }
    #[test]
    fn definition_picker_state_is_per_widget_and_never_changes_canvas_brush() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = serde_json::json!([{"uid":10,"tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4}]);
        app.set_project(p);
        app.set_atlas_selection(10, vec![1, 3]);
        app.flips = 3;
        app.random_tiles = true;
        let first = egui::Id::new("entity-icon-picker");
        let second = egui::Id::new("rule-output-picker");
        app.with_local_atlas_state(&ctx, first, |app| {
            app.set_atlas_selection(10, vec![7]);
            app.random_tiles = false;
        });
        assert_eq!(app.selected_tile_ids_for(10), vec![1, 3]);
        assert_eq!(app.flips, 3);
        assert!(app.random_tiles);
        app.with_local_atlas_state(&ctx, second, |app| {
            assert_eq!(app.tile_rect, [0, 0, 1, 1]);
            app.set_atlas_selection(10, vec![2, 6]);
        });
        app.with_local_atlas_state(&ctx, first, |app| {
            assert_eq!(app.selected_tile_ids_for(10), vec![7])
        });
        app.with_local_atlas_state(&ctx, second, |app| {
            assert_eq!(app.selected_tile_ids_for(10), vec![2, 6])
        });
        assert_eq!(app.selected_tile_ids_for(10), vec![1, 3]);
        assert_eq!(app.flips, 3);
    }
    #[test]
    fn atlas_mouse_union_subtraction_and_saved_recall_match_source() {
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = serde_json::json!([{"uid":10,"tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4,"savedSelections":[{"ids":[0,2,8],"mode":"Random"}]}]);
        app.set_project(p);
        app.set_atlas_selection(10, vec![0, 1]);
        app.flips = 3;
        app.apply_atlas_mouse_selection(
            10,
            vec![4],
            false,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 1, 4]);
        assert_eq!(app.flips, 0);
        app.apply_atlas_mouse_selection(
            10,
            vec![0],
            false,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![1, 4]);
        app.apply_atlas_mouse_selection(10, vec![1, 4], true, Default::default(), false);
        assert_eq!(
            app.selected_tile_ids_for(10),
            vec![4],
            "subtract must preserve at least one original ID"
        );
        app.apply_atlas_mouse_selection(10, vec![2], false, Default::default(), true);
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 2, 8]);
        assert!(app.random_tiles);
        app.apply_atlas_mouse_selection(10, vec![2], false, Default::default(), true);
        assert_eq!(
            app.selected_tile_ids_for(10),
            vec![2],
            "clicking an already selected saved group picks the single tile"
        );
        app.apply_atlas_mouse_selection(
            10,
            vec![8],
            false,
            egui::Modifiers {
                command: true,
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            app.selected_tile_ids_for(10),
            vec![2, 8],
            "Ctrl/Cmd suppresses recall and unions the clicked tile"
        );
    }
    #[test]
    fn atlas_pointer_events_author_sparse_selection_and_rmb_preserves_one() {
        fn frame(
            app: &mut EditorApp,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            let modifiers = events
                .iter()
                .find_map(|event| match event {
                    egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default();
            ctx.run(
                egui::RawInput {
                    modifiers,
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.atlas_picker(ctx, ui, 10));
                },
            )
        }
        fn click(
            app: &mut EditorApp,
            ctx: &egui::Context,
            point: egui::Pos2,
            button: egui::PointerButton,
            modifiers: egui::Modifiers,
        ) {
            for pressed in [true, false] {
                frame(
                    app,
                    ctx,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button,
                            pressed,
                            modifiers,
                        },
                    ],
                );
            }
        }
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let image = ldtk_core::exports::embedded_atlas().unwrap();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = serde_json::json!([{"uid":10,"identifier":"Icons","embedAtlas":"LdtkIcons","relPath":null,"tileGridSize":16,"pxWid":image.width(),"pxHei":image.height(),"__cWid":image.width()/16,"__cHei":image.height()/16}]);
        app.set_project(p);
        let texture = app.assets.embedded(&ctx).unwrap();
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let mesh = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::epaint::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => Some(mesh),
                _ => None,
            })
            .unwrap();
        let left = mesh
            .vertices
            .iter()
            .map(|v| v.pos.x)
            .fold(f32::INFINITY, f32::min);
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.pos.y)
            .fold(f32::INFINITY, f32::min);
        let right = mesh
            .vertices
            .iter()
            .map(|v| v.pos.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let cell = (right - left) / image.width() as f32 * 16.;
        let point = |x: f32| egui::pos2(left + (x + 0.5) * cell, top + 0.5 * cell);
        click(
            &mut app,
            &ctx,
            point(1.),
            egui::PointerButton::Primary,
            Default::default(),
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![1]);
        click(
            &mut app,
            &ctx,
            point(3.),
            egui::PointerButton::Primary,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![1, 3]);
        click(
            &mut app,
            &ctx,
            point(1.),
            egui::PointerButton::Primary,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![3]);
        click(
            &mut app,
            &ctx,
            point(3.),
            egui::PointerButton::Secondary,
            Default::default(),
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![3]);
    }
    #[test]
    fn home_content_uses_supplied_canvas_after_parent_cursor_is_consumed() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (canvas, _) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                    app.library_home(ctx, ui, canvas);
                });
            },
        );
        assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text=="Examples"&&rect.contains(t.pos))),"home heading must remain inside canvas even after full-size allocation");
    }
    #[test]
    fn partial_image_edge_cell_is_clickable_with_source_ceil_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.png");
        image::RgbaImage::from_pixel(70, 45, image::Rgba([255, 255, 255, 255]))
            .save(&path)
            .unwrap();
        let ctx = egui::Context::default();
        let mut p = Project::new();
        let uid = p.add_tileset(&path, 16, 70, 45).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 700.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.assets.poll(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| app.atlas_picker(ctx, ui, uid));
                },
            )
        };
        frame(&mut app, vec![]);
        let texture = (0..100)
            .find_map(|_| {
                app.assets.poll(&ctx);
                let texture = app.assets.texture(&ctx, &path);
                if texture.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                texture
            })
            .expect("actual70x45 atlas decoded");
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        let rect = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(m) if m.texture_id == texture.id() => Some(m.calc_bounds()),
                _ => None,
            })
            .unwrap();
        let pos = rect.min + egui::vec2(68., 43.) * (rect.width() / 70.);
        for pressed in [true, false] {
            frame(
                &mut app,
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
        assert_eq!(
            app.selected_tile_ids_for(uid),
            vec![14],
            "source ceil(70/16)=5,ceil(45/16)=3; last partial cell is(4,2)"
        );
        assert_eq!(app.tile_rect, [4, 2, 1, 1]);
    }
    #[test]
    fn atlas_hit_testing_uses_source_stride_including_spacing_and_partial_cells() {
        let grid = AtlasGrid::new(
            &serde_json::json!({"tileGridSize":16,"padding":2,"spacing":1}),
            egui::vec2(37., 37.),
        );
        assert_eq!((grid.columns, grid.rows), (2, 2));
        assert_eq!(grid.cell(egui::vec2(1., 2.)), None);
        assert_eq!(grid.cell(egui::vec2(18., 2.)), Some([0, 0]));
        assert_eq!(grid.cell(egui::vec2(19., 19.)), Some([1, 1]));
        assert_eq!(grid.cell(egui::vec2(35., 19.)), Some([1, 1]));
        assert_eq!(
            grid.rectangle([0, 0, 2, 2]),
            Rect::from_min_size(egui::pos2(2., 2.), egui::vec2(33., 33.))
        );
    }
    #[test]
    fn entity_palette_honors_any_required_and_excluded_tags() {
        let e = serde_json::json!({"tags":["actor","boss"]});
        assert!(entity_allowed(
            &e,
            &serde_json::json!({"requiredTags":["actor","trigger"]})
        ));
        assert!(!entity_allowed(
            &e,
            &serde_json::json!({"excludedTags":["boss"]})
        ));
        assert!(!entity_allowed(
            &e,
            &serde_json::json!({"requiredTags":["trigger"]})
        ));
    }
    #[test]
    fn reverse_drag_produces_inclusive_rectangular_selection() {
        assert_eq!(selection_between([4, 8], [2, 5]), [2, 5, 3, 4]);
        assert_eq!(selection_between([3, 2], [3, 2]), [3, 2, 1, 1]);
    }
}
