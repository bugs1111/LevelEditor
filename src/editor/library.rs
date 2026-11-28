use super::*;
use crate::app::chrome::ActionTooltip as _;
use serde_json::json;
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
                .hint_text(locale::tr(ui, "Search…"))
                .vertical_align(egui::Align::Center)
                .margin(egui::Margin {
                    left: 28,
                    right: 8,
                    top: 0,
                    bottom: 0,
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
        let image = egui::Image::new(egui::include_image!("../../assets/icons/gridView.svg"))
            .fit_to_exact_size(Vec2::splat(20.));
        let button = ui.add(egui::Button::image(image));
        if let Some(next) = panels::source_controls::palette_columns_menu(&button) {
            *columns = next;
        }
    });
}
fn palette_group(
    ui: &mut egui::Ui,
    mut open: bool,
    title: &str,
    color: Color32,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    ui.add_space(2.);
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
        egui::Frame::new()
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(4)
            .show(ui, body);
        ui.add_space(8.);
    } else {
        ui.add_space(2.);
    }
    open
}

fn palette_item(
    painter: &egui::Painter,
    rect: Rect,
    color: Color32,
    selected: bool,
    hovered: bool,
    list: bool,
) -> Color32 {
    let fill = if selected {
        color.lerp_to_gamma(Color32::WHITE, 0.66)
    } else {
        color.lerp_to_gamma(Color32::from_rgb(49, 56, 67), 0.72)
    };
    painter.rect_filled(rect, 0., fill);
    if list {
        painter.rect_filled(
            Rect::from_min_size(rect.min, egui::vec2(6., rect.height())),
            0.,
            color,
        );
    }
    if selected || hovered {
        painter.rect_stroke(
            rect,
            0.,
            Stroke::new(1_f32, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    if selected {
        Color32::BLACK
    } else {
        color.lerp_to_gamma(Color32::WHITE, 0.3)
    }
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
    pub(super) fn edit_recent_directory_color(&mut self, ctx: &egui::Context, path: &Path) {
        let (color, _) = self.settings.dir_color(path);
        ctx.data_mut(|data| {
            data.insert_temp(
                directory_color_key(),
                DirectoryColor {
                    path: path.to_owned(),
                    rgb: [(color >> 16) as u8, (color >> 8) as u8, color as u8],
                    first: true,
                },
            );
        });
    }
    fn recent_directory_color_dialog(&mut self, ctx: &egui::Context) {
        let id = directory_color_key();
        let Some(mut state) = ctx.data_mut(|d| d.get_temp::<DirectoryColor>(id)) else {
            return;
        };
        let complete = panels::source_colors::standalone_color_dialog(
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

    fn selection_center(self, ids: &[i32]) -> Option<Vec2> {
        if ids.is_empty() || self.columns <= 0 {
            return None;
        }
        let sum = ids.iter().fold(Vec2::ZERO, |sum, id| {
            sum + egui::vec2((id % self.columns) as f32, (id / self.columns) as f32)
        });
        let center = sum / ids.len() as f32 + Vec2::splat(0.5);
        Some(Vec2::splat(self.padding) + center * (self.grid + self.spacing))
    }
}

pub(super) fn saved_selection_for(tileset: &Value, tile: i32) -> Option<&Value> {
    tileset["savedSelections"]
        .as_array()?
        .iter()
        .find(|selection| {
            selection["ids"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| *id == tile))
        })
}

pub(super) fn saved_selection_ids(selection: &Value) -> Vec<i32> {
    selection["ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .filter_map(|id| i32::try_from(id).ok())
        .collect()
}

fn paint_palette_tile_cursor(
    ui: &egui::Ui,
    atlas: Rect,
    grid: AtlasGrid,
    zoom: f32,
    ids: &[i32],
    random: bool,
    preview: bool,
) {
    let color = match (random, preview) {
        (true, true) => Color32::from_rgba_unmultiplied(144, 238, 144, 128),
        (true, false) => Color32::from_rgb(144, 238, 144),
        (false, true) => Color32::from_white_alpha(128),
        (false, false) => Color32::from_rgb(243, 228, 164),
    };
    let selected: std::collections::HashSet<_> = ids.iter().copied().collect();
    let thickness = zoom.max(0.5);
    for &id in ids {
        if grid.columns <= 0 || id < 0 || id / grid.columns >= grid.rows {
            continue;
        }
        let tile = grid.rectangle([id % grid.columns, id / grid.columns, 1, 1]);
        let target = Rect::from_min_size(atlas.min + tile.min.to_vec2() * zoom, tile.size() * zoom);
        if !ui.is_rect_visible(target) {
            continue;
        }
        let edge = target.shrink(thickness * 0.5);
        let vertical = |x| [egui::pos2(x, target.top()), egui::pos2(x, target.bottom())];
        let horizontal = |y| [egui::pos2(target.left(), y), egui::pos2(target.right(), y)];
        for (neighbor, points) in [
            (id - 1, vertical(edge.left())),
            (id + 1, vertical(edge.right())),
            (id - grid.columns, horizontal(edge.top())),
            (id + grid.columns, horizontal(edge.bottom())),
        ] {
            if random && preview {
                ui.painter().extend(egui::Shape::dotted_line(
                    &points,
                    color,
                    thickness * 2.,
                    thickness * 0.5,
                ));
            } else if random || !selected.contains(&neighbor) {
                ui.painter()
                    .line_segment(points, Stroke::new(thickness, color));
            }
        }
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
enum AtlasPickerMode<'a> {
    Tiles,
    Definition,
    Rectangle,
    Icon { opening: bool },
    RuleWizard,
    RuleRemap { tiles: &'a [i32], offset: [i32; 2] },
    RuleOutput { stamp: bool, opening: bool },
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
        let Some(layer) = project
            .layer(key, self.layer)
            .filter(|layer| layer["__type"] != "AutoLayer")
        else {
            let mut rect = ui.available_rect_before_wrap();
            rect.min.y += 1.0;
            if rect.is_positive() {
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                    .paint_at(ui, rect);
                ui.painter().hline(
                    rect.x_range(),
                    rect.top(),
                    Stroke::new(1_f32, theme::MEDIUM),
                );
            }
            return;
        };
        let tiles = layer["__type"] == "Tiles";
        let atlas_uid = if tiles {
            layer["overrideTilesetUid"]
                .as_i64()
                .or_else(|| layer["__tilesetDefUid"].as_i64())
                .filter(|uid| project.tileset(*uid).is_some())
        } else {
            None
        };
        if tiles && atlas_uid.is_none() {
            let mut palette = ui.available_rect_before_wrap();
            palette.max.x -= 2.;
            if palette.is_positive() {
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, palette.size() / 8.))
                    .paint_at(ui, palette);
                let width = (palette.width() - 100.).max(40.).min(palette.width());
                let galley = ui.painter().layout(
                    locale::tr(ui, "This tile layer has no Tileset.").to_owned(),
                    FontId::proportional(14.667),
                    Color32::PLACEHOLDER,
                    (width - 18.).max(1.),
                );
                let rect = Rect::from_center_size(
                    palette.center(),
                    egui::vec2(width, galley.size().y + 18.),
                );
                project_dialogs::paint_warning(
                    &ui.painter()
                        .with_clip_rect(palette.intersect(ui.clip_rect())),
                    rect,
                    galley,
                    Color32::from_black_alpha(168),
                    Color32::from_rgb(227, 77, 0),
                );
            }
            return;
        }
        let layer_uid = layer["layerDefUid"].as_i64().unwrap_or(-1);
        let project_iid = project.json["iid"].as_str();
        let search_keys = match text(layer, "__type", "") {
            "Entities" => Some(("entity-palette-search", "EntityPaletteColumns")),
            "IntGrid" => Some(("intgrid-search", "IntGridPaletteColumns")),
            _ => None,
        };
        let scroll = egui::ScrollArea::vertical();
        let (search, columns, scroll, outer_margin) =
            if let Some((search_key, column_key)) = search_keys {
                let search_id = ui.make_persistent_id((search_key, project_iid, layer_uid));
                let mut search = ui
                    .data_mut(|d| d.get_temp::<String>(search_id))
                    .unwrap_or_default();
                let columns_key = format!("{column_key}_{layer_uid}");
                let previous_columns = self
                    .settings
                    .ui_state_int(&columns_key, project_iid, 1)
                    .clamp(1, 10) as usize;
                let mut columns = previous_columns;
                palette_search(ui, &mut search, &mut columns);
                if columns != previous_columns {
                    self.settings
                        .set_ui_state_int(&columns_key, project_iid, columns as i64);
                }
                ui.data_mut(|d| d.insert_temp(search_id, search.clone()));
                ui.spacing_mut().scroll = egui::style::ScrollStyle {
                    bar_width: 10.,
                    bar_inner_margin: 0.,
                    bar_outer_margin: 0.,
                    ..egui::style::ScrollStyle::solid()
                };
                ui.visuals_mut().clip_rect_margin = 0.;
                (
                    search.to_lowercase(),
                    columns,
                    scroll.id_salt(("palette-items", project_iid, layer_uid)),
                    // Four points of list padding plus its one-point border. The source
                    // list overlaps the toolbar by two points and ends at the sidebar border.
                    egui::Margin {
                        left: 5,
                        right: 7,
                        top: 3,
                        bottom: 5,
                    },
                )
            } else {
                (
                    String::new(),
                    1,
                    scroll.id_salt("palette-scroll"),
                    egui::Margin::ZERO,
                )
            };
        scroll.show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(outer_margin)
                .show(ui, |ui| {
                    let project = self.project.as_ref().expect("selected project");
                    let layer = project.layer(key, self.layer).expect("selected layer");
                    let layer_type = text(layer, "__type", "");
                    let mut open_definition = None;
                    match layer_type {
                        "IntGrid" => {
                            if let Some(def) = project.layer_def(layer_uid)
                                && let Some(values) = def["intGridValues"].as_array()
                            {
                                let mut groups =
                                    vec![(0, locale::tr(ui, "Ungrouped").to_owned(), None)];
                                groups.extend(
                                    def["intGridValuesGroups"]
                                        .as_array()
                                        .into_iter()
                                        .flatten()
                                        .map(|g| {
                                            (
                                                g["uid"].as_i64().unwrap_or(0),
                                                text(g, "identifier", locale::tr(ui, "Group"))
                                                    .to_owned(),
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
                                    let draw = |ui: &mut egui::Ui| {
                                        let width = if columns == 1 {
                                            ui.available_width()
                                        } else {
                                            (ui.available_width() - (columns - 1) as f32)
                                                / columns as f32
                                        };
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(1., 1.);
                                            for value in &group {
                                                let id =
                                                    value["value"].as_i64().unwrap_or(1) as i32;
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2(
                                                        width,
                                                        if columns == 1 {
                                                            46.
                                                        } else {
                                                            width + 18.
                                                        },
                                                    ),
                                                    Sense::click(),
                                                );
                                                let selected = self.int_value == id;
                                                let color =
                                                    theme::color(text(value, "color", "#ffcc00"));
                                                let text_color = palette_item(
                                                    ui.painter(),
                                                    rect,
                                                    color,
                                                    selected,
                                                    response.hovered(),
                                                    columns == 1,
                                                );
                                                let icon = Rect::from_center_size(
                                                    if columns == 1 {
                                                        egui::pos2(
                                                            rect.left() + 33.,
                                                            rect.center().y,
                                                        )
                                                    } else {
                                                        egui::pos2(
                                                            rect.center().x,
                                                            rect.top() + width / 2.,
                                                        )
                                                    },
                                                    Vec2::splat(if columns == 1 {
                                                        34.
                                                    } else {
                                                        (width - 4.).max(4.)
                                                    }),
                                                );
                                                let border = if columns == 1 {
                                                    Stroke::new(
                                                        1_f32,
                                                        color.lerp_to_gamma(Color32::WHITE, 0.2),
                                                    )
                                                } else {
                                                    Stroke::NONE
                                                };
                                                ui.painter().rect(
                                                    icon,
                                                    2.,
                                                    color.lerp_to_gamma(Color32::BLACK, 0.5),
                                                    border,
                                                    StrokeKind::Inside,
                                                );
                                                if !value["tile"].is_null() {
                                                    paint_tile(
                                                        ctx,
                                                        ui.painter(),
                                                        icon.shrink(border.width),
                                                        project,
                                                        &value["tile"],
                                                        &mut self.assets,
                                                    );
                                                }
                                                if let Some(name) = value["identifier"].as_str() {
                                                    ui.painter().text(
                                                        if columns == 1 {
                                                            egui::pos2(
                                                                rect.left() + 58.,
                                                                rect.center().y,
                                                            )
                                                        } else {
                                                            egui::pos2(
                                                                rect.center().x,
                                                                rect.bottom() - 9.,
                                                            )
                                                        },
                                                        if columns == 1 {
                                                            Align2::LEFT_CENTER
                                                        } else {
                                                            Align2::CENTER_CENTER
                                                        },
                                                        name,
                                                        FontId::proportional(if columns == 1 {
                                                            15.
                                                        } else {
                                                            10.
                                                        }),
                                                        text_color,
                                                    );
                                                }
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
                                        egui::Frame::new().inner_margin(4).show(ui, draw);
                                    }
                                }
                            }
                        }
                        "Entities" => {
                            if let Some(entities) = project.json["defs"]["entities"].as_array() {
                                let layer_def =
                                    project.layer_def(layer_uid).unwrap_or(&Value::Null);
                                let definitions = entities
                                    .iter()
                                    .filter(|e| entity_allowed(e, layer_def))
                                    .collect::<Vec<_>>();
                                if !definitions
                                    .iter()
                                    .any(|e| e["uid"].as_i64() == self.entity_uid)
                                {
                                    self.entity_uid =
                                        definitions.first().and_then(|e| e["uid"].as_i64());
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
                                                || text(d, "identifier", "")
                                                    .to_lowercase()
                                                    .contains(&search)
                                        })
                                        .collect::<Vec<_>>();
                                    if group.is_empty() {
                                        continue;
                                    }
                                    let draw = |ui: &mut egui::Ui| {
                                        let width = if columns == 1 {
                                            ui.available_width()
                                        } else {
                                            (ui.available_width() - (columns - 1) as f32)
                                                / columns as f32
                                        };
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(1., 1.);
                                            for def in &group {
                                                let uid = def["uid"].as_i64().unwrap_or(-1);
                                                let color =
                                                    theme::color(text(def, "color", "#ffcc00"));
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2(
                                                        width,
                                                        if columns == 1 {
                                                            44.
                                                        } else {
                                                            width + 20.
                                                        },
                                                    ),
                                                    Sense::click(),
                                                );
                                                let selected = self.entity_uid == Some(uid);
                                                let text_color = palette_item(
                                                    ui.painter(),
                                                    rect,
                                                    color,
                                                    selected,
                                                    response.hovered(),
                                                    columns == 1,
                                                );
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
                                                        egui::pos2(
                                                            rect.left() + 62.,
                                                            rect.center().y,
                                                        )
                                                    } else {
                                                        egui::pos2(
                                                            rect.center().x,
                                                            rect.bottom() - 10.,
                                                        )
                                                    },
                                                    if columns == 1 {
                                                        Align2::LEFT_CENTER
                                                    } else {
                                                        Align2::CENTER_CENTER
                                                    },
                                                    text(
                                                        def,
                                                        "identifier",
                                                        locale::tr(ui, "Entity"),
                                                    ),
                                                    FontId::proportional(if columns == 1 {
                                                        16.
                                                    } else {
                                                        11.
                                                    }),
                                                    text_color,
                                                );
                                                if response.clicked() {
                                                    self.entity_uid = Some(uid);
                                                }
                                                response
                                                    .clone()
                                                    .detail_tooltip(text(def, "doc", ""));
                                                egui::Popup::context_menu(&response)
                                                    .frame(panels::source_controls::context_menu_frame(ui.style()))
                                                    .show(|ui| {
                                                        if panels::source_controls::context_menu_item(ui, "Edit entity definition").clicked() {
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
                                        let group_open =
                                            self.settings.ui_state_int(&group_key, None, 1) != 0;
                                        let heading = if tag.is_empty() {
                                            locale::tr(ui, "UNTAGGED").into()
                                        } else {
                                            tag.to_uppercase()
                                        };
                                        let group_next = palette_group(
                                            ui,
                                            group_open,
                                            &heading,
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
                                        egui::Frame::new().inner_margin(4).show(ui, draw);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                    if let Some(uid) = open_definition {
                        self.definition_uid = Some(uid);
                        self.show_panel(Some(Panel::Entities));
                    }
                    if let Some(uid) = atlas_uid {
                        self.restore_canvas_atlas_memory(ctx, uid);
                        self.tile_palette(ctx, ui, uid);
                        self.remember_canvas_atlas_memory(ctx, uid);
                    }
                });
        });
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
    pub(super) fn restore_canvas_atlas_memory(&mut self, ctx: &egui::Context, uid: i64) {
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
                let saved = saved_selection_for(self.project.as_ref()?.tileset(uid)?, 0)?;
                Some((saved_selection_ids(saved), saved["mode"] == "Random"))
            })
            .unwrap_or_else(|| (vec![0], false));
        if !self.set_atlas_selection(uid, memory.0) {
            self.set_atlas_selection(uid, vec![0]);
        }
        self.random_tiles = memory.1;
        ctx.data_mut(|d| d.insert_temp(current, source));
    }
    pub(super) fn remember_canvas_atlas_memory(&self, ctx: &egui::Context, uid: i64) {
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
                .and_then(|tileset| saved_selection_for(tileset, ids[0]))
        {
            let saved_ids = saved_selection_ids(saved);
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
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::Tiles);
    }
    pub(super) fn definition_atlas_picker(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
    ) {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::Definition);
    }

    pub(super) fn set_tileset_custom_data(&mut self, uid: i64, tile: i32, data: &str) {
        let Some(tileset) = self
            .project
            .as_ref()
            .and_then(|project| project.tileset(uid))
        else {
            return;
        };
        let data = data.trim();
        if (!data.is_empty()).then_some(data) == tile_custom_data(tileset, tile).as_deref() {
            return;
        }
        let mut entries = tileset["customData"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        entries.retain(|entry| entry["tileId"] != tile);
        if !data.is_empty() {
            entries.push(json!({"tileId":tile,"data":ldtk_core::ldtk_string::escape(data)}));
            entries.sort_by_key(|entry| entry["tileId"].as_i64());
        }
        self.edit("Tile data changed", |project| {
            project.update_tileset_definition(uid, &json!({"customData":entries}))
        });
    }

    fn tileset_enum_tools(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        definition: &Value,
        active: &mut Option<String>,
    ) {
        ui.painter().rect_filled(rect, 0., theme::DARK);
        let area = rect.shrink(5.);
        let mut tools = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("tileset-enum-tools")
                .max_rect(area),
        );
        tools.set_clip_rect(rect.intersect(ui.clip_rect()));
        tools.set_width(area.width());
        tools.spacing_mut().item_spacing.y = 1.;
        egui::ScrollArea::vertical()
            .max_height(area.height())
            .auto_shrink([false, true])
            .show(&mut tools, |ui| {
                let values = definition["values"].as_array().into_iter().flatten();
                for value in std::iter::once(None).chain(values.map(Some)) {
                    let name = value.and_then(|value| value["id"].as_str());
                    let label = name.unwrap_or_else(|| locale::tr(ui, "Custom data"));
                    let (row, response) = ui
                        .allocate_exact_size(egui::vec2(ui.available_width(), 27.), Sense::click());
                    let color = value.map(|value| {
                        let [r, g, b] = panels::enum_color(value);
                        Color32::from_rgb(r, g, b)
                    });
                    ui.painter().rect_filled(
                        row,
                        0.,
                        color.map_or(theme::MEDIUM, |color| mix_color(color, Color32::BLACK, 0.4)),
                    );
                    let mut label_x = row.left() + 8.;
                    if let Some(color) = color {
                        ui.painter().rect_filled(
                            Rect::from_min_size(row.min, egui::vec2(5., row.height())),
                            0.,
                            color,
                        );
                        label_x += 5.;
                    }
                    if let Some(value) = value
                        && let Some(icon_atlas) = definition["iconTilesetUid"].as_i64()
                        && value["tileRect"].is_object()
                    {
                        let mut icon = value["tileRect"].clone();
                        icon["tilesetUid"] = json!(icon_atlas);
                        let box_rect = Rect::from_min_size(
                            egui::pos2(label_x, row.center().y - 8.),
                            Vec2::splat(16.),
                        );
                        if let Some(project) = self.project.as_ref()
                            && paint_tile(
                                ui.ctx(),
                                ui.painter(),
                                box_rect,
                                project,
                                &icon,
                                &mut self.assets,
                            )
                        {
                            label_x += 20.;
                        }
                    }
                    let ink = if name.is_some() {
                        Color32::WHITE
                    } else {
                        theme::TEXT
                    };
                    let text = egui::WidgetText::from(RichText::new(label).color(ink));
                    let text = text.into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        (row.right() - label_x - 4.).max(0.),
                        FontId::proportional(14.),
                    );
                    ui.painter().galley(
                        egui::pos2(label_x, row.center().y - text.size().y / 2.),
                        text,
                        ink,
                    );
                    let selected = active.as_deref() == name;
                    if selected || response.hovered() {
                        ui.painter().rect_stroke(
                            row,
                            0.,
                            Stroke::new(
                                2_f32,
                                Color32::from_white_alpha(if selected { 255 } else { 166 }),
                            ),
                            StrokeKind::Inside,
                        );
                    }
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            selected,
                            label,
                        )
                    });
                    if response.clicked() {
                        *active = name.map(str::to_owned);
                    }
                }
            });
    }

    pub(super) fn mark_tileset_tiles(&mut self, uid: i64, value: &str, tiles: &[i32], add: bool) {
        let Some(tileset) = self
            .project
            .as_ref()
            .and_then(|project| project.tileset(uid))
        else {
            return;
        };
        let mut entries = tileset["enumTags"].as_array().cloned().unwrap_or_default();
        let index = entries
            .iter()
            .position(|entry| entry["enumValueId"] == value);
        let before = index
            .into_iter()
            .flat_map(|index| entries[index]["tileIds"].as_array().into_iter().flatten())
            .filter_map(Value::as_i64)
            .collect::<std::collections::BTreeSet<_>>();
        let mut marked = before.clone();
        if add {
            marked.extend(tiles.iter().copied().map(i64::from));
        } else {
            for &id in tiles {
                marked.remove(&i64::from(id));
            }
        }
        if marked == before {
            return;
        }
        let entry = json!({"enumValueId":value,"tileIds":marked});
        if let Some(index) = index {
            entries[index] = entry;
        } else {
            entries.push(entry);
        }
        self.edit("Tile tag changed", |project| {
            project.update_tileset_definition(uid, &json!({"enumTags":entries}))
        });
    }
    pub(super) fn atlas_rect_picker(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, uid: i64) {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::Rectangle);
    }
    pub(super) fn atlas_icon_picker(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        opening: bool,
    ) {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::Icon { opening });
    }
    pub(super) fn atlas_rule_output_picker(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        stamp: bool,
        opening: bool,
    ) -> bool {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::RuleOutput { stamp, opening })
    }
    pub(super) fn atlas_rule_wizard_picker(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
    ) {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::RuleWizard);
    }
    pub(super) fn atlas_rule_remap_picker(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        tiles: &[i32],
        offset: [i32; 2],
    ) -> bool {
        self.atlas_picker_mode(ctx, ui, uid, AtlasPickerMode::RuleRemap { tiles, offset })
    }
    fn atlas_picker_mode(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        mode: AtlasPickerMode<'_>,
    ) -> bool {
        let rule_picker = matches!(mode, AtlasPickerMode::RuleOutput { .. });
        let wizard = matches!(mode, AtlasPickerMode::RuleWizard);
        let remap = matches!(mode, AtlasPickerMode::RuleRemap { .. });
        let definition_preview = matches!(mode, AtlasPickerMode::Definition);
        let rectangle_only = !matches!(
            mode,
            AtlasPickerMode::Tiles
                | AtlasPickerMode::Definition
                | AtlasPickerMode::RuleWizard
                | AtlasPickerMode::RuleOutput { stamp: false, .. }
        );
        let icon_picker = matches!(mode, AtlasPickerMode::Icon { .. });
        let focus_selection = rule_picker || icon_picker;
        let modal_picker = focus_selection || wizard || remap;
        let focus_id = ui.make_persistent_id(("modal-atlas-focus", uid));
        if matches!(
            mode,
            AtlasPickerMode::RuleOutput { opening: true, .. }
                | AtlasPickerMode::Icon { opening: true }
        ) {
            ui.data_mut(|d| d.insert_temp(focus_id, true));
        }
        let Some(project) = &self.project else {
            return false;
        };
        let Some(def) = project.tileset(uid).cloned() else {
            ui.weak(locale::tr(ui, "Choose a tileset"));
            return false;
        };
        let read_only_preview = definition_preview && !def["embedAtlas"].is_null();
        let tagger = definition_preview && !read_only_preview;
        let marking_enum = def["tagsSourceEnumUid"]
            .as_i64()
            .filter(|_| tagger)
            .and_then(|uid| {
                ["enums", "externalEnums"]
                    .into_iter()
                    .flat_map(|kind| project.json["defs"][kind].as_array().into_iter().flatten())
                    .find(|definition| definition["uid"] == uid)
            })
            .cloned();
        let canvas = palette_popout::canvas_view(ctx);
        let fitted = canvas.is_some_and(|view| view.fit);
        if canvas.is_none() && !modal_picker && !definition_preview {
            ui.label(text(&def, "identifier", locale::tr(ui, "Tileset")));
        }
        let texture = if let Some(path) = def["relPath"].as_str() {
            self.assets.texture(ctx, &canvas::asset_path(project, path))
        } else if !def["embedAtlas"].is_null() {
            self.assets.embedded(ctx)
        } else {
            ui.weak(locale::tr(ui, "Assign an atlas image in Tilesets."));
            return false;
        };
        let Some(texture) = texture else {
            if let Some(path) = def["relPath"].as_str() {
                let path = canvas::asset_path(project, path);
                if let Some(error) = self.assets.errors.get(&path).cloned() {
                    ui.colored_label(Color32::LIGHT_RED, error);
                    if panels::source_controls::button(ui, "Reload atlas").clicked() {
                        self.assets.invalidate(&path);
                    }
                    return false;
                }
            }
            ui.spinner();
            return false;
        };
        let natural = texture.size_vec2();
        let grid = AtlasGrid::new(&def, natural);
        let zoom_id = ui.make_persistent_id(("atlas-zoom", uid));
        let mut zoom = ui
            .data_mut(|d| d.get_temp::<f32>(zoom_id))
            .unwrap_or_else(|| {
                if remap {
                    (ui.available_width() / natural.x)
                        .min(ui.available_height() / natural.y)
                        .max(0.01)
                } else {
                    3.
                }
            });
        if fitted {
            zoom = (ui.available_width() / natural.x)
                .min(ui.available_height().max(8.) / natural.y)
                .max(0.01);
        }
        let scroll_id = ui.make_persistent_id(egui::Id::new(("atlas-picker", uid)));
        let viewport_id = zoom_id.with("viewport");
        let marking_id = zoom_id.with((
            "enum-marking",
            text(&project.json, "iid", ""),
            def["tagsSourceEnumUid"].as_i64(),
        ));
        let data_dialog = zoom_id.with(("tile-custom-data", text(&project.json, "iid", "")));
        let mut active_mark = marking_enum.as_ref().and_then(|definition| {
            ui.data_mut(|data| data.get_temp::<String>(marking_id))
                .filter(|value| {
                    definition["values"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|item| item["id"] == *value)
                })
        });
        let pan_id = zoom_id.with("pan");
        let mut pan = ui.data_mut(|d| d.get_temp::<egui::PointerButton>(pan_id));
        let preview = (definition_preview || wizard || remap || canvas.is_some()).then(|| {
            let size = egui::vec2(ui.available_width(), ui.available_height().max(80.));
            let (_, frame) = ui.allocate_space(size);
            let mut view = frame;
            if definition_preview || wizard || remap {
                ui.painter().rect(
                    frame,
                    0.,
                    theme::DARK,
                    Stroke::new(1_f32, if wizard { theme::LIGHT } else { Color32::WHITE }),
                    StrokeKind::Inside,
                );
                view = frame.shrink(if wizard { 4. } else { 1. });
            }
            if let Some(definition) = &marking_enum {
                let tools =
                    Rect::from_min_max(egui::pos2(view.right() - 130., view.top()), view.max);
                view.max.x = tools.left();
                self.tileset_enum_tools(ui, tools, definition, &mut active_mark);
                ui.data_mut(|data| {
                    if let Some(mark) = &active_mark {
                        data.insert_temp(marking_id, mark.clone());
                    } else {
                        data.remove::<String>(marking_id);
                    }
                });
            }
            let response = ui.interact(view, viewport_id, Sense::click_and_drag());
            (view, response)
        });
        let viewport = preview
            .as_ref()
            .map(|(rect, _)| *rect)
            .or_else(|| ui.data_mut(|d| d.get_temp::<Rect>(viewport_id)));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let previous_scroll = egui::scroll_area::State::load(ctx, scroll_id);
        let mut state = previous_scroll.unwrap_or_default();
        let frame = ctx.cumulative_frame_nr();
        let palette_opening = canvas.is_some()
            && ui.data_mut(|data| {
                let key = zoom_id.with("last-palette-frame");
                let previous = data.get_temp::<u64>(key);
                data.insert_temp(key, frame);
                previous.is_none_or(|frame_seen| frame_seen + 1 < frame)
            });
        // Leave room to center the tiles even when the brush is near the atlas edge.
        let mut image_margin = Vec2::ZERO;
        let mut viewport_size = Vec2::ZERO;
        if fitted {
            state.offset = Vec2::ZERO;
        } else if focus_selection {
            viewport_size = ui.available_size();
            image_margin = viewport_size * egui::vec2(0.5, 0.25);
            if ui
                .data_mut(|d| d.remove_temp::<bool>(focus_id))
                .unwrap_or(false)
            {
                if let Some(center) = grid.selection_center(&self.selected_tile_ids_for(uid)) {
                    state.offset = center * zoom;
                } else if rule_picker || previous_scroll.is_none() {
                    state.offset = image_margin;
                }
            }
        }
        let hovered = viewport.zip(pointer).filter(|(rect, pointer)| {
            rect.contains(*pointer)
                && ui.clip_rect().contains(*pointer)
                && (preview.is_none() || ui.rect_contains_pointer(*rect))
        });
        let mut consume_wheel = false;
        if definition_preview {
            let wheel_id = zoom_id.with("wheel-captured");
            let wheel_event = ui.input(|input| {
                input.events.iter().any(
                    |event| matches!(event, egui::Event::MouseWheel { delta, .. } if delta.y != 0.),
                )
            });
            consume_wheel = ui.data_mut(|data| {
                if wheel_event {
                    data.insert_temp(wheel_id, hovered.is_some());
                }
                data.get_temp::<bool>(wheel_id).unwrap_or(false)
            });
        }
        if let Some((viewport, pointer)) = hovered {
            let mut zoom_at_pointer = |delta: f32| {
                let previous = zoom;
                zoom = (zoom * (1. + delta * 0.001)).clamp(0.5, 6.);
                let anchor = pointer - viewport.min;
                state.offset = (state.offset + anchor) * (zoom / previous) - anchor;
            };
            let delta = ui.input(|i| i.raw_scroll_delta.y);
            if definition_preview {
                ui.input(|input| {
                    for event in &input.events {
                        if let egui::Event::MouseWheel { unit, delta, .. } = event {
                            // Chromium reports one discrete wheel step as 120 logical pixels.
                            let factor = if *unit == egui::MouseWheelUnit::Line {
                                120.
                            } else {
                                1.
                            };
                            zoom_at_pointer(delta.y * factor);
                        }
                    }
                });
            } else if delta != 0. && !fitted {
                zoom_at_pointer(delta);
            }
            consume_wheel |= delta != 0.;
            let preview_pan = read_only_preview
                .then(|| {
                    ui.input(|i| {
                        [egui::PointerButton::Primary, egui::PointerButton::Secondary]
                            .into_iter()
                            .find(|button| i.pointer.button_pressed(*button))
                    })
                })
                .flatten();
            if let Some(button) = preview_pan {
                pan = Some(button);
            } else if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle)) && !fitted
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
        if consume_wheel {
            // Keep the smoothed tail with the widget that received the wheel event.
            ui.input_mut(|i| {
                i.smooth_scroll_delta = Vec2::ZERO;
                i.raw_scroll_delta = Vec2::ZERO;
            });
        }
        let panning = pan.is_some();
        if let Some(button) = pan {
            if ui.input(|i| i.pointer.button_down(button)) {
                state.offset -= ui.input(|i| i.pointer.delta());
            } else {
                pan = None;
            }
        }
        if let (Some(canvas), Some(viewport)) = (canvas, viewport) {
            let target_id = zoom_id.with("navigation-target");
            let focus = palette_popout::take_focus(ctx, uid);
            let interrupted = hovered.is_some()
                && (consume_wheel || ui.input(|input| input.pointer.any_pressed()));
            let mut target = ui.data_mut(|data| data.remove_temp::<Vec2>(target_id));
            if fitted || interrupted {
                target = None;
            } else if palette_opening || focus.is_some() {
                let extent = if palette_opening {
                    canvas.sidebar_size
                } else {
                    viewport.size()
                };
                target = grid
                    .selection_center(&self.selected_tile_ids_for(uid))
                    .map(|center| center * zoom - extent * egui::vec2(0.5, 0.25));
                if (palette_opening || focus == Some(true))
                    && let Some(offset) = target.take()
                {
                    state.offset = offset;
                }
            }
            if let Some(target) = target {
                let speed = ui.input(|input| (0.38 * input.stable_dt * 60.).min(1.));
                state.offset += (target - state.offset) * speed;
                for axis in 0..2 {
                    if (target[axis] - state.offset[axis]).abs() <= zoom {
                        state.offset[axis] = target[axis];
                    }
                }
                if state.offset != target {
                    ui.data_mut(|data| data.insert_temp(target_id, target));
                    ctx.request_repaint();
                }
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
        let max_height = if canvas.is_some() || modal_picker {
            ui.available_height().max(8.)
        } else {
            600.
        };
        let mut picked = false;
        let mut data_action = None;
        let custom_mode = tagger && active_mark.is_none();
        let mut paint_atlas = |ui: &mut egui::Ui, rect: Rect, response: egui::Response| {
            if tagger {
                response.clone().on_hover_cursor(if custom_mode {
                    egui::CursorIcon::PointingHand
                } else {
                    egui::CursorIcon::Crosshair
                });
            }
            let (background, repeat) = if modal_picker || definition_preview || canvas.is_some() {
                ui.painter().rect_filled(rect, 0., theme::MEDIUM);
                (
                    egui::include_image!("../../assets/img/transparency.png"),
                    natural / 8.,
                )
            } else {
                (
                    egui::include_image!("../../assets/img/stripes.png"),
                    rect.size() / 8.,
                )
            };
            egui::Image::new(background)
                .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                .uv(Rect::from_min_size(egui::Pos2::ZERO, repeat))
                .paint_at(ui, rect);
            ui.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::WHITE,
            );
            if let Some(definition) = &marking_enum
                && grid.columns > 0
                && grid.rows > 0
            {
                let tile_rect = |x, y| {
                    let source = grid.rectangle([x, y, 1, 1]);
                    Rect::from_min_size(
                        rect.min + source.min.to_vec2() * zoom,
                        source.size() * zoom,
                    )
                };
                let tags = def["enumTags"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                if let Some(mark) = &active_mark {
                    let marked: std::collections::HashSet<_> = tags
                        .iter()
                        .filter(|tag| tag["enumValueId"] == *mark)
                        .flat_map(|tag| tag["tileIds"].as_array().into_iter().flatten())
                        .filter_map(Value::as_i64)
                        .collect();
                    let visible = ui.clip_rect().intersect(rect);
                    let stride = grid.grid + grid.spacing;
                    let min =
                        ((visible.min - rect.min) / zoom - Vec2::splat(grid.padding)) / stride;
                    let max =
                        ((visible.max - rect.min) / zoom - Vec2::splat(grid.padding)) / stride;
                    let left = (min.x.floor() as i32).clamp(0, grid.columns);
                    let right = (max.x.ceil() as i32).clamp(0, grid.columns);
                    let top = (min.y.floor() as i32).clamp(0, grid.rows);
                    let bottom = (max.y.ceil() as i32).clamp(0, grid.rows);
                    for y in top..bottom {
                        for x in left..right {
                            let id = i64::from(y) * i64::from(grid.columns) + i64::from(x);
                            if !marked.contains(&id) {
                                ui.painter().rect_filled(
                                    tile_rect(x, y),
                                    0.,
                                    Color32::from_black_alpha(76),
                                );
                            }
                        }
                    }
                }
                let tile_count = i64::from(grid.columns) * i64::from(grid.rows);
                let width = (grid.grid / 16.).floor().max(1.) * zoom;
                for value in definition["values"].as_array().into_iter().flatten() {
                    if active_mark
                        .as_ref()
                        .is_some_and(|mark| value["id"] != *mark)
                    {
                        continue;
                    }
                    let [r, g, b] = panels::enum_color(value);
                    let color = Color32::from_rgb(r, g, b);
                    let ids = tags
                        .iter()
                        .filter(|tag| tag["enumValueId"] == value["id"])
                        .flat_map(|tag| tag["tileIds"].as_array().into_iter().flatten())
                        .filter_map(Value::as_i64);
                    for id in ids.filter(|id| (0..tile_count).contains(id)) {
                        let columns = i64::from(grid.columns);
                        let tile = tile_rect((id % columns) as i32, (id / columns) as i32);
                        if ui.is_rect_visible(tile) {
                            ui.painter().rect_stroke(
                                tile,
                                0.,
                                Stroke::new(width * 2., Color32::BLACK),
                                StrokeKind::Inside,
                            );
                            ui.painter().rect_stroke(
                                tile,
                                0.,
                                Stroke::new(width, color),
                                StrokeKind::Inside,
                            );
                        }
                    }
                }
            }
            if tagger && grid.columns > 0 {
                let (icon, size) = if grid.grid < 16. {
                    (
                        egui::include_image!("../../assets/icons/tile-data-small.png"),
                        Vec2::splat(4.),
                    )
                } else {
                    (
                        egui::include_image!("../../assets/icons/tile-data.png"),
                        egui::vec2(6., 7.),
                    )
                };
                let size = size * (grid.grid / 32.).floor().max(1.) * zoom;
                for entry in def["customData"].as_array().into_iter().flatten() {
                    if let Some(id) = entry["tileId"]
                        .as_i64()
                        .and_then(|id| i32::try_from(id).ok())
                        && id >= 0
                        && id / grid.columns < grid.rows
                        && entry["data"].is_string()
                    {
                        let source = grid.rectangle([id % grid.columns, id / grid.columns, 1, 1]);
                        let target =
                            Rect::from_min_size(rect.min + source.min.to_vec2() * zoom, size);
                        if ui.is_rect_visible(target) {
                            egui::Image::new(icon.clone())
                                .texture_options(egui::TextureOptions::NEAREST)
                                .paint_at(ui, target);
                        }
                    }
                }
            }
            let drag_id = ui.make_persistent_id(("atlas-selection-drag", uid));
            let mut drag = ui.data_mut(|d| d.get_temp::<AtlasDrag>(drag_id));
            if panning {
                drag = None;
            }
            if focus_selection
                && !panning
                && response.contains_pointer()
                && let Some(pos) = pointer
                && !rect.contains(pos)
                && ui.input(|i| i.pointer.primary_pressed())
            {
                ui.close();
            }
            if !read_only_preview
                && !panning
                && response.contains_pointer()
                && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                && let Some(cell) = grid.cell((pos - rect.min) / zoom)
            {
                for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
                    if ui.input(|i| i.pointer.button_pressed(button)) {
                        let secondary_rectangle =
                            rectangle_only && button == egui::PointerButton::Secondary;
                        if secondary_rectangle {
                            self.atlas_selection = Some((uid, vec![]));
                        }
                        // TileRectAndClose still selects the dragged rectangle on release.
                        drag = if secondary_rectangle && matches!(mode, AtlasPickerMode::Rectangle)
                        {
                            None
                        } else {
                            Some(AtlasDrag {
                                start: cell,
                                button,
                            })
                        };
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
                let selection = if custom_mode || remap {
                    [cell[0], cell[1], 1, 1]
                } else {
                    selection_between(active.start, cell)
                };
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
                    if custom_mode {
                        data_action = Some((cell[0] + cell[1] * grid.columns, active.button));
                    } else {
                        let ids: Vec<_> = (selection[0]..selection[0] + selection[2])
                            .flat_map(|x| {
                                (selection[1]..selection[1] + selection[3])
                                    .map(move |y| x + y * grid.columns)
                            })
                            .collect();
                        if let Some(value) = &active_mark {
                            self.mark_tileset_tiles(
                                uid,
                                value,
                                &ids,
                                active.button == egui::PointerButton::Primary,
                            );
                            ui.ctx().request_repaint();
                        } else if rectangle_only {
                            picked = self.set_atlas_selection(uid, ids);
                        } else {
                            let mut modifiers = ui.input(|i| i.modifiers);
                            if wizard {
                                modifiers.shift |= modifiers.ctrl || modifiers.command;
                                modifiers.ctrl = false;
                                modifiers.command = false;
                            }
                            self.apply_atlas_mouse_selection(
                                uid,
                                ids,
                                active.button == egui::PointerButton::Secondary,
                                modifiers,
                                self.panel.is_none() && !rule_picker && !wizard,
                            );
                        }
                    }
                    drag = None;
                } else if !ui.input(|i| i.pointer.button_down(active.button)) {
                    drag = None;
                }
            }
            if let Some(drag) = &drag {
                ui.data_mut(|d| d.insert_temp(drag_id, drag.clone()));
            } else {
                ui.data_mut(|d| d.remove::<AtlasDrag>(drag_id));
            }
            if self.tile_tags && marking_enum.is_none() {
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
            if canvas.is_some() {
                let ids = self.selected_tile_ids_for(uid);
                paint_palette_tile_cursor(ui, rect, grid, zoom, &ids, self.random_tiles, false);
            }
            if let AtlasPickerMode::RuleRemap { tiles, offset } = mode {
                let valid = grid.columns > 0
                    && tiles.iter().all(|tile| {
                        (0..grid.columns).contains(&(tile % grid.columns + offset[0]))
                            && (0..grid.rows).contains(&(tile / grid.columns + offset[1]))
                    });
                let target = |x, y| {
                    let cell = grid.rectangle([x, y, 1, 1]);
                    Rect::from_min_size(rect.min + cell.min.to_vec2() * zoom, cell.size() * zoom)
                };
                for &tile in tiles.iter().filter(|_| grid.columns > 0) {
                    let (x, y) = (tile % grid.columns, tile / grid.columns);
                    let (to_x, to_y) = (x + offset[0], y + offset[1]);
                    let from = target(x, y);
                    if offset != [0, 0] {
                        ui.painter().rect_stroke(
                            from,
                            0.,
                            Stroke::new(2_f32, Color32::DARK_GREEN),
                            StrokeKind::Inside,
                        );
                    }
                    if (0..grid.columns).contains(&to_x) && (0..grid.rows).contains(&to_y) {
                        let to = target(to_x, to_y);
                        let ink = if valid { Color32::GREEN } else { Color32::RED };
                        ui.painter().rect_stroke(
                            to,
                            0.,
                            Stroke::new(2_f32, ink),
                            StrokeKind::Inside,
                        );
                        if offset != [0, 0] {
                            ui.painter().arrow(
                                from.center(),
                                to.center() - from.center(),
                                Stroke::new(1_f32, if valid { Color32::WHITE } else { ink }),
                            );
                        }
                    }
                }
            }
            let selections = if definition_preview || remap || canvas.is_some() {
                Vec::new()
            } else if let Some((selected_uid, ids)) = &self.atlas_selection
                && *selected_uid == uid
            {
                if rule_picker && rectangle_only && !ids.is_empty() {
                    vec![self.tile_rect]
                } else {
                    ids.iter()
                        .filter(|id| **id >= 0 && **id < grid.columns * grid.rows)
                        .map(|id| [*id % grid.columns.max(1), *id / grid.columns.max(1), 1, 1])
                        .collect::<Vec<_>>()
                }
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
                    Stroke::new(
                        if rule_picker { 3. } else { 2_f32 },
                        if rule_picker {
                            Color32::from_rgb(153, 255, 153)
                        } else {
                            theme::ACCENT
                        },
                    ),
                    StrokeKind::Inside,
                );
            }
            if !read_only_preview
                && let Some(pos) = response.hover_pos()
                && let Some(cell) = grid.cell((pos - rect.min) / zoom)
            {
                let id = cell[0] + cell[1] * grid.columns;
                if canvas.is_some() {
                    if !panning && drag.is_none() && !ui.input(|input| input.key_down(Key::Space)) {
                        let (ids, random) = saved_selection_for(&def, id).map_or_else(
                            || (vec![id], true),
                            |saved| (saved_selection_ids(saved), saved["mode"] == "Random"),
                        );
                        paint_palette_tile_cursor(ui, rect, grid, zoom, &ids, random, true);
                    }
                    let label = format!("#{id}");
                    let font = FontId::proportional(11.);
                    let ink = Color32::from_white_alpha(178);
                    let text = ui.painter().layout_no_wrap(label, font, ink);
                    let corner = ui.clip_rect().right_top();
                    let badge = Rect::from_min_size(
                        corner - egui::vec2(text.size().x + 8., 0.),
                        text.size() + egui::vec2(8., 2.),
                    );
                    ui.painter().rect_filled(badge, 0., theme::DARK);
                    ui.painter()
                        .galley(badge.min + egui::vec2(4., 1.), text, ink);
                } else if tagger {
                    if !panning && drag.is_none() {
                        let label = locale::format(ui, "Tile {id}", &[("id", &id.to_string())]);
                        let tags = def["enumTags"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|tag| {
                                tag["tileIds"]
                                    .as_array()?
                                    .iter()
                                    .any(|value| *value == id)
                                    .then(|| tag["enumValueId"].as_str())
                                    .flatten()
                            })
                            .collect::<Vec<_>>()
                            .join(" + ");
                        let data = tile_custom_data(&def, id)
                            .map(|data| format!("\"{data}\""))
                            .unwrap_or_default();
                        chrome::pointer_detail_tooltip(ctx, pos, label, &[tags, data]);
                    }
                } else {
                    response.clone().detail_tooltip(locale::format(
                        ui,
                        "Tile #{id}  ({x}, {y})",
                        &[
                            ("id", &id.to_string()),
                            ("x", &cell[0].to_string()),
                            ("y", &cell[1].to_string()),
                        ],
                    ));
                }
            }
        };
        if let Some((viewport, response)) = preview {
            let mut content = ui.new_child(egui::UiBuilder::new().max_rect(viewport));
            content.set_clip_rect(viewport.intersect(ui.clip_rect()));
            let rect = Rect::from_min_size(viewport.min - state.offset, natural * zoom);
            paint_atlas(&mut content, rect, response);
        } else {
            let scroll = egui::ScrollArea::both()
                .scroll_source(egui::scroll_area::ScrollSource {
                    scroll_bar: true,
                    drag: false,
                    mouse_wheel: false,
                })
                .id_salt(("atlas-picker", uid))
                .max_height(max_height)
                .auto_shrink([!modal_picker, !modal_picker])
                .scroll_bar_visibility(if focus_selection {
                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden
                } else {
                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                })
                .show(ui, |ui| {
                    let (content, response) = ui.allocate_exact_size(
                        natural * zoom + viewport_size,
                        Sense::click_and_drag(),
                    );
                    let rect = Rect::from_min_size(content.min + image_margin, natural * zoom);
                    paint_atlas(ui, rect, response);
                });
            ui.data_mut(|d| d.insert_temp(viewport_id, scroll.inner_rect));
        }
        if canvas.is_none() && !modal_picker && !definition_preview {
            ui.small(locale::format(
                ui,
                "Tile #{id} · {width} × {height} tiles",
                &[
                    ("id", &self.tile_id.to_string()),
                    ("width", &self.tile_rect[2].to_string()),
                    ("height", &self.tile_rect[3].to_string()),
                ],
            ));
        }
        if tagger {
            if let Some(action) = data_action {
                ui.data_mut(|data| data.insert_temp(data_dialog, action));
                if action.1 == egui::PointerButton::Primary {
                    let title = locale::format(
                        ui,
                        "Tile {id} custom data",
                        &[("id", &action.0.to_string())],
                    );
                    text_editor::open(
                        ctx,
                        data_dialog,
                        tile_custom_data(&def, action.0)
                            .as_deref()
                            .unwrap_or_default(),
                        &title,
                        "LangJson",
                        false,
                    );
                }
            }
            if let Some((tile, button)) =
                ui.data_mut(|data| data.get_temp::<(i32, egui::PointerButton)>(data_dialog))
            {
                if button == egui::PointerButton::Primary {
                    let source = grid.rectangle([
                        tile % grid.columns.max(1),
                        tile / grid.columns.max(1),
                        1,
                        1,
                    ]);
                    let uv = Rect::from_min_max(
                        (source.min.to_vec2() / natural).to_pos2(),
                        (source.max.to_vec2() / natural).to_pos2(),
                    );
                    if let Some(draft) = text_editor::show_with_header(ui, data_dialog, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.;
                            panels::source_controls::help_card(
                                ui,
                                ui.available_width() - 72.,
                                64.,
                                |ui| {
                                    panels::source_controls::help_paragraph(
                                        ui,
                                        TILE_CUSTOM_DATA_HELP,
                                    );
                                },
                            );
                            ui.add(egui::Image::new((texture.id(), Vec2::splat(64.))).uv(uv));
                        });
                        ui.add_space(8.);
                    }) {
                        self.set_tileset_custom_data(uid, tile, &draft);
                    }
                    if !text_editor::is_open(ctx) {
                        ui.data_mut(|data| data.remove::<(i32, egui::PointerButton)>(data_dialog));
                    }
                } else {
                    let title = locale::format(
                        ui,
                        "Clear custom data for tile {id}?",
                        &[("id", &tile.to_string())],
                    );
                    let modal = egui::Modal::new(data_dialog)
                        .frame(
                            project_dialogs::dialog_frame(20).fill(Color32::from_rgb(121, 17, 5)),
                        )
                        .backdrop_color(Color32::from_black_alpha(140))
                        .show(ctx, |ui| {
                            let font = FontId::proportional(14.667);
                            let width = ui
                                .painter()
                                .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE)
                                .size()
                                .x
                                .clamp(100., 360.);
                            ui.set_width(width);
                            ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                            ui.label(RichText::new(title).font(font).color(Color32::WHITE));
                            ui.add_space(16.);
                            ui.horizontal(|ui| {
                                let size = egui::vec2((width - 2.) * 0.5, 35.);
                                let confirm =
                                    project_dialogs::button(ui, "CONFIRM", size, false, 13.333)
                                        .clicked();
                                let cancel =
                                    project_dialogs::button(ui, "CANCEL", size, true, 13.333)
                                        .clicked();
                                (confirm, cancel)
                            })
                            .inner
                        });
                    let confirm = modal.inner.0
                        || modal.is_top_modal
                            && ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, Key::Enter)
                            });
                    if confirm || modal.inner.1 || modal.should_close() {
                        ui.data_mut(|data| data.remove::<(i32, egui::PointerButton)>(data_dialog));
                        if confirm {
                            self.set_tileset_custom_data(uid, tile, "");
                        }
                    }
                }
            }
        }
        picked
    }
}

const TILE_CUSTOM_DATA_HELP: &str = "You can enter any kind of data here, which will be associated to this tile and stored in the project JSON. This data could either be numbers, text, JSON, XML etc. Basically, any tile related info you would like to pass to your game engine.";

pub(super) fn tile_custom_data(tileset: &Value, tile: i32) -> Option<std::borrow::Cow<'_, str>> {
    tileset["customData"]
        .as_array()?
        .iter()
        .find(|entry| entry["tileId"] == tile)?["data"]
        .as_str()
        .map(ldtk_core::ldtk_string::tile_data_text)
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
pub(super) fn paint_tile(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: Rect,
    project: &Project,
    tile: &Value,
    assets: &mut Assets,
) -> bool {
    let Some((texture, uv, size)) = preview_tile_texture(ctx, project, tile, assets) else {
        return false;
    };
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let dst = Rect::from_center_size(rect.center(), size * scale);
    painter.image(texture.id(), dst, uv, Color32::WHITE);
    true
}

fn preview_tile_texture(
    ctx: &egui::Context,
    project: &Project,
    tile: &Value,
    assets: &mut Assets,
) -> Option<(egui::TextureHandle, Rect, Vec2)> {
    let def = tile["tilesetUid"]
        .as_i64()
        .and_then(|id| project.tileset(id))?;
    let texture = if let Some(path) = def["relPath"].as_str() {
        assets.texture(ctx, &canvas::asset_path(project, path))
    } else if !def["embedAtlas"].is_null() {
        assets.embedded(ctx)
    } else {
        None
    }?;
    let size = texture.size_vec2();
    let src = Rect::from_min_size(
        egui::pos2(number(tile, "x", 0.), number(tile, "y", 0.)),
        egui::vec2(number(tile, "w", 16.), number(tile, "h", 16.)),
    );
    if !src.is_positive() || !Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(src) {
        return None;
    }
    let uv = Rect::from_min_max(
        egui::pos2(src.left() / size.x, src.top() / size.y),
        egui::pos2(src.right() / size.x, src.bottom() / size.y),
    );
    Some((texture, uv, src.size()))
}

pub(super) fn paint_entity_definition_icon(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: Rect,
    project: &Project,
    def: &Value,
    assets: &mut Assets,
) {
    use egui::emath::GuiRounding;

    let rect = rect.round_to_pixels(ctx.pixels_per_point());
    let ui_tile = &def["uiTileRect"];
    if number(ui_tile, "w", 0.) > 0. {
        if let Some((texture, uv, _)) = preview_tile_texture(ctx, project, ui_tile, assets) {
            painter.image(texture.id(), rect, uv, Color32::WHITE);
        }
        return;
    }

    let mode = text(def, "renderMode", "Rectangle");
    let color = theme::color(text(def, "color", "#ffcc00"));
    let fill_opacity = number(def, "fillOpacity", 1.);
    let line_opacity = number(def, "lineOpacity", 1.);
    let fill = color.gamma_multiply(fill_opacity);
    let line = color.gamma_multiply(line_opacity);
    if mode == "Tile" {
        let tile = &def["tileRect"];
        if tile.is_null() {
            return;
        }
        painter.rect_filled(rect, 0., fill);
        let padding = if fill_opacity > 0. || line_opacity > 0. {
            1.
        } else {
            0.
        };
        if let Some((texture, uv, _)) = preview_tile_texture(ctx, project, tile, assets) {
            painter.image(
                texture.id(),
                rect.shrink(padding),
                uv,
                Color32::WHITE.gamma_multiply(number(def, "tileOpacity", 1.)),
            );
        }
        painter.rect_stroke(rect, 0., Stroke::new(1_f32, line), StrokeKind::Inside);
        return;
    }

    let size = egui::vec2(
        number(def, "width", 16.).max(1.),
        number(def, "height", 16.).max(1.),
    );
    let scale = (rect.width() / size.x).min(rect.height() / size.y) * 0.9;
    let shape = Rect::from_center_size(rect.center(), size * scale);
    // The source thumbnail strokes a 32-point preview on a 3× canvas.
    let unit = rect.width() / 96.;
    let stroke = Stroke::new(unit * 2. / 3., line);
    match mode {
        "Rectangle" => {
            painter.rect(shape, 0., fill, stroke, StrokeKind::Middle);
        }
        "Ellipse" => {
            painter.add(egui::epaint::EllipseShape {
                center: shape.center(),
                radius: shape.size() * 0.5,
                fill,
                stroke,
            });
        }
        "Cross" => {
            let stroke = Stroke::new(3. * unit, line);
            painter.line_segment([shape.left_top(), shape.right_bottom()], stroke);
            painter.line_segment([shape.right_top(), shape.left_bottom()], stroke);
        }
        _ => {}
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
