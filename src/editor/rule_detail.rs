use super::*;
use crate::app::chrome::ActionTooltip as _;

#[path = "rule_value_palette.rs"]
mod rule_value_palette;
use rule_value_palette::rule_value_group;
#[path = "rule_guidance.rs"]
mod rule_guidance;

const ANY_VALUE: i64 = 1_000_001;
const EDITOR_SIZE: usize = 9;

pub(super) enum RuleCellStyle {
    Editor { center: bool, hovered: bool },
    Preview(f32),
}

pub(super) fn rule_has_unknown_conditions(source: &Value, pattern: &[Value]) -> bool {
    pattern
        .iter()
        .filter_map(Value::as_i64)
        .any(|value| match value.saturating_abs() {
            0 | ANY_VALUE => false,
            id @ 1_000.. => rule_value_group(source, id / 1_000 - 1).is_none(),
            id => !source["intGridValues"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["value"] == id),
        })
}

#[derive(Clone, Copy)]
struct PatternStroke {
    button: egui::PointerButton,
    initial: i64,
}

impl PatternStroke {
    fn apply(self, current: i64, brush: i64) -> i64 {
        match self.button {
            egui::PointerButton::Primary if self.initial >= 0 => brush,
            egui::PointerButton::Primary => current.max(0),
            egui::PointerButton::Secondary if self.initial == 0 => -brush,
            _ => 0,
        }
    }
}

pub(in crate::app) struct RuleDraft {
    layer: i64,
    group: i64,
    rule: i64,
    data: Value,
    value: i64,
    stroke: Option<PatternStroke>,
    output_pick: Option<usize>,
    help: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RuleOptions {
    GroupColor,
    GroupIcon,
    Advanced,
    RequireGroup,
    Offsets,
    Modulo,
    Noise,
}

type RuleOptionsState = (i64, i64, Option<i64>, RuleOptions, bool);

pub(super) struct RuleAtlas {
    texture: egui::TextureHandle,
    columns: i64,
    grid: f32,
    stride: f32,
    padding: f32,
}

impl RuleAtlas {
    pub(super) fn stamp_preview_size(&self, cell: f32, ids: &[Value]) -> Vec2 {
        let mut span = Vec2::splat(cell);
        if let Some(first) = ids.first().and_then(Value::as_i64) {
            for id in ids.iter().filter_map(Value::as_i64) {
                if id % self.columns != first % self.columns {
                    span.x = cell * 1.9;
                }
                if id / self.columns != first / self.columns {
                    span.y = cell * 1.9;
                }
            }
        }
        span
    }

    pub(super) fn paint_ids(&self, ui: &egui::Ui, rect: Rect, ids: &[Value]) {
        let tiles: Vec<_> = ids
            .iter()
            .filter_map(Value::as_i64)
            .map(|id| (id % self.columns, id / self.columns))
            .collect();
        let Some(min_x) = tiles.iter().map(|p| p.0).min() else {
            return;
        };
        let min_y = tiles.iter().map(|p| p.1).min().unwrap();
        let max_x = tiles.iter().map(|p| p.0).max().unwrap();
        let max_y = tiles.iter().map(|p| p.1).max().unwrap();
        let cells = egui::vec2((max_x - min_x + 1) as f32, (max_y - min_y + 1) as f32);
        let step = (rect.width() / cells.x).min(rect.height() / cells.y);
        let origin = rect.center() - cells * step * 0.5;
        for (x, y) in tiles {
            self.paint_pixels(
                ui,
                Rect::from_min_size(
                    origin + egui::vec2((x - min_x) as f32, (y - min_y) as f32) * step,
                    Vec2::splat(step),
                ),
                [
                    self.padding + self.stride * x as f32,
                    self.padding + self.stride * y as f32,
                    self.grid,
                    self.grid,
                ],
            );
        }
    }

    pub(super) fn paint_rect(&self, ui: &egui::Ui, rect: Rect, tile: &Value) {
        let [x, y, w, h] = [
            number(tile, "x", 0.),
            number(tile, "y", 0.),
            number(tile, "w", 16.),
            number(tile, "h", 16.),
        ];
        let ratio = (rect.width() / w.max(1.)).min(rect.height() / h.max(1.));
        self.paint_pixels(
            ui,
            Rect::from_center_size(rect.center(), egui::vec2(w, h) * ratio),
            [x, y, w, h],
        );
    }

    pub(super) fn paint_fragment(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        id: i64,
        mirrored: Option<(bool, bool)>,
    ) {
        let x = self.padding + self.stride * (id % self.columns) as f32;
        let y = self.padding + self.stride * (id / self.columns) as f32;
        let size = self.texture.size_vec2();
        let mut uv = Rect::from_min_size(
            egui::pos2(x / size.x, y / size.y),
            egui::vec2(self.grid / size.x, self.grid / size.y),
        );
        let tint = if let Some((x, y)) = mirrored {
            if x {
                std::mem::swap(&mut uv.min.x, &mut uv.max.x);
            }
            if y {
                std::mem::swap(&mut uv.min.y, &mut uv.max.y);
            }
            Color32::from_white_alpha(102)
        } else {
            Color32::WHITE
        };
        ui.painter().image(self.texture.id(), rect, uv, tint);
    }

    fn paint_pixels(&self, ui: &egui::Ui, rect: Rect, [x, y, w, h]: [f32; 4]) {
        let size = self.texture.size_vec2();
        let uv = Rect::from_min_max(
            egui::pos2(x / size.x, y / size.y),
            egui::pos2((x + w) / size.x, (y + h) / size.y),
        );
        ui.painter()
            .image(self.texture.id(), rect, uv, Color32::WHITE);
    }
}

fn source_def<'p>(project: &'p Project, def: &'p Value) -> &'p Value {
    def["autoSourceLayerDefUid"]
        .as_i64()
        .and_then(|id| project.layer_def(id))
        .unwrap_or(def)
}

fn choose_initial_value(rule: &Value, source: &Value) -> i64 {
    let mut counts = std::collections::BTreeMap::new();
    let mut best = None;
    for v in rule["pattern"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
    {
        let id = v.saturating_abs();
        if id == 0 || id == ANY_VALUE {
            continue;
        }
        let n = counts.entry(id).or_insert(0);
        *n += 1;
        let n = *n;
        if best.is_none_or(|current| counts[&current] < n) {
            best = Some(id);
        }
    }
    best.or_else(|| source["intGridValues"][0]["value"].as_i64())
        .unwrap_or(ANY_VALUE)
}

fn group(def: &Value, id: i64) -> Option<&Value> {
    def["autoRuleGroups"]
        .as_array()?
        .iter()
        .find(|g| g["uid"] == id)
}
fn rule(group: &Value, id: i64) -> Option<&Value> {
    group["rules"].as_array()?.iter().find(|r| r["uid"] == id)
}
fn rule_options_id() -> egui::Id {
    egui::Id::new("rule-options")
}

fn rule_options_anchor_id() -> egui::Id {
    rule_options_id().with("anchor")
}

fn rule_settings_modal(ctx: &egui::Context, size: Vec2, bottom: i8) -> egui::Modal {
    let id = rule_options_id();
    let screen = ctx.content_rect();
    let pos = ctx
        .data(|d| d.get_temp::<Rect>(rule_options_anchor_id()))
        .map_or(screen.left_top() + egui::vec2(0., 16.), |anchor| {
            let x = if anchor.left() - screen.left() >= screen.width() * 0.6 {
                anchor.right() - size.x
            } else {
                anchor.left()
            };
            let y = if anchor.top() - screen.top() >= screen.height() * 0.7 {
                anchor.top() - size.y
            } else {
                anchor.bottom()
            };
            egui::pos2(
                x.clamp(screen.left(), (screen.right() - size.x).max(screen.left())),
                y.clamp(screen.top(), (screen.bottom() - size.y).max(screen.top())),
            )
        });
    egui::Modal::new(id)
        .area(
            egui::Area::new(id)
                .kind(egui::UiKind::Modal)
                .sense(Sense::hover())
                .order(egui::Order::Foreground)
                .fixed_pos(pos),
        )
        .frame(
            project_dialogs::dialog_frame(0)
                .corner_radius(0.)
                .stroke(Stroke::new(1_f32, theme::LIGHT))
                .inner_margin(egui::Margin {
                    left: 20,
                    right: 20,
                    top: 20,
                    bottom,
                }),
        )
        .backdrop_color(Color32::TRANSPARENT)
}

const OFFSET_AXES: [[&str; 3]; 2] = [
    ["tileXOffset", "tileRandomXMin", "tileRandomXMax"],
    ["tileYOffset", "tileRandomYMin", "tileRandomYMax"],
];

fn offset_input(
    ui: &mut egui::Ui,
    key: &str,
    value: i64,
    bounds: (i64, i64),
    width: f32,
) -> Option<i64> {
    let current = if value == 0 {
        String::new()
    } else {
        value.to_string()
    };
    source_controls::number_input_sized(
        ui,
        key,
        &current,
        source_controls::NumberStyle::IntegerSlider {
            min: bounds.0 as f64,
            max: bounds.1 as f64,
            speed: 0.05808,
            placeholder: Some("0"),
        },
        width,
    )
    .map(|draft| (draft.trim().parse::<f64>().unwrap_or(0.) as i64).clamp(bounds.0, bounds.1))
}

fn offset_label(ui: &mut egui::Ui, text: &str, width: f32, align: egui::Align) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 33.), Sense::hover());
    let (position, alignment) = if align == egui::Align::Max {
        (
            rect.right_center() - egui::vec2(0., 4.),
            Align2::RIGHT_CENTER,
        )
    } else {
        (rect.center(), Align2::CENTER_CENTER)
    };
    ui.painter().with_clip_rect(rect).text(
        position,
        alignment,
        locale::tr(ui, text),
        FontId::proportional(13.333),
        theme::TEXT,
    );
}

fn offset_icon(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    tint: Color32,
    label: &str,
) -> egui::Response {
    let button = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = Vec2::splat(3.);
            ui.add_sized(
                [27., 32.],
                egui::Button::image(
                    egui::Image::new(icon)
                        .fit_to_exact_size(Vec2::splat(17.))
                        .tint(tint),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE),
            )
        })
        .inner;
    button.action_tooltip(locale::tr(ui, label))
}

const MODULO_FIELDS: [&str; 4] = ["xModulo", "yModulo", "xOffset", "yOffset"];
const MODULO_PREVIEW_WIDTH: f32 = 260.;

fn modulo_grid(steps: [i64; 2]) -> ([i64; 2], Vec2, f32) {
    let cells = steps.map(|step| (step.clamp(1, 40) + 3).max(8));
    let width = (MODULO_PREVIEW_WIDTH - 4. - (cells[0] - 1) as f32 * 2.) / cells[0] as f32;
    let margins = 4. + (cells[1] - 1) as f32 * 2.;
    let height = (margins + cells[1] as f32 * (width + 2.)).max(133.);
    let size = egui::vec2(width, (height - margins) / cells[1] as f32);
    (cells, size, height)
}

fn modulo_preview(ui: &mut egui::Ui, [x_step, y_step, x_offset, y_offset]: [i64; 4]) {
    let ([columns, rows], size, height) = modulo_grid([x_step, y_step]);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(MODULO_PREVIEW_WIDTH, height), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        0.,
        Stroke::new(1_f32, theme::LIGHT),
        StrokeKind::Inside,
    );
    let cell_stroke = Stroke::new(1_f32, Color32::from_rgb(115, 127, 157));
    for y in 0..rows {
        for x in 0..columns {
            let origin = rect.min
                + Vec2::splat(2.)
                + egui::vec2(x as f32 * (size.x + 2.), y as f32 * (size.y + 2.));
            let cell = Rect::from_min_size(origin, size);
            let active = (x - x_offset) % x_step.max(1) == 0 && (y - y_offset) % y_step.max(1) == 0;
            let fill = if active {
                Color32::from_rgb(60, 97, 72)
            } else {
                theme::LIGHT
            };
            ui.painter().rect_filled(cell, 0., fill);
            if active {
                egui::Image::new(egui::include_image!("../../assets/icons/active.png")).paint_at(
                    ui,
                    Rect::from_center_size(cell.center(), Vec2::splat(size.x)),
                );
            } else {
                egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, cell.size() / 8.))
                    .paint_at(ui, cell);
            }
            ui.painter()
                .rect_stroke(cell, 0., cell_stroke, StrokeKind::Inside);
        }
    }
}

fn modulo_field(
    ui: &mut egui::Ui,
    index: usize,
    label: &str,
    current: i64,
    opening: bool,
) -> Option<i64> {
    ui.allocate_ui_with_layout(
        egui::vec2(100., 50.),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(100.);
            ui.spacing_mut().item_spacing.y = 0.;
            ui.label(
                RichText::new(locale::tr(ui, label))
                    .size(14.667)
                    .color(Color32::WHITE),
            );
            if current == 1 {
                ui.visuals_mut().override_text_color = Some(theme::LIGHT);
            }
            let key = MODULO_FIELDS[index];
            let current = current.to_string();
            let (min, max) = if index < 2 { (1, 40) } else { (-4096, 4096) };
            let edit = ui.make_persistent_id(key);
            if opening {
                ui.memory_mut(|memory| memory.request_focus(edit));
            }
            let draft = source_controls::number_input_sized(
                ui,
                key,
                &current,
                source_controls::NumberStyle::IntegerSlider {
                    min: min as f64,
                    max: max as f64,
                    speed: 0.02904,
                    placeholder: None,
                },
                100.,
            );
            if opening {
                source_controls::select_text_input(ui.ctx(), edit, &current);
            }
            draft.map(|text| (text.trim().parse::<f64>().unwrap_or(0.) as i64).clamp(min, max))
        },
    )
    .inner
}

impl EditorApp {
    pub(super) fn rule_atlas(
        &mut self,
        ctx: &egui::Context,
        uid: Option<i64>,
    ) -> Option<RuleAtlas> {
        let project = self.project.as_ref()?;
        let atlas = project.tileset(uid?)?;
        let texture = if let Some(rel) = atlas["relPath"].as_str() {
            self.assets
                .texture(ctx, &canvas::asset_path(project, rel))?
        } else if atlas["embedAtlas"] == "LdtkIcons" {
            self.assets.embedded(ctx)?
        } else {
            return None;
        };
        let grid = number(atlas, "tileGridSize", 16.);
        Some(RuleAtlas {
            texture,
            columns: atlas["__cWid"].as_i64().unwrap_or(1).max(1),
            grid,
            stride: grid + number(atlas, "spacing", 0.),
            padding: number(atlas, "padding", 0.),
        })
    }

    fn rule_instance_atlas(&self, def: &Value) -> Option<i64> {
        let layer = def["uid"].as_i64()?;
        self.selected
            .and_then(|k| {
                self.project.as_ref()?.level(k)?["layerInstances"]
                    .as_array()?
                    .iter()
                    .find(|li| li["layerDefUid"] == layer)?["overrideTilesetUid"]
                    .as_i64()
            })
            .or_else(|| def["tilesetDefUid"].as_i64())
    }

    pub(super) fn insert_rule(&mut self, layer: i64, group_id: i64, index: usize) -> bool {
        let Some(uid) = self.edit("Rule created", |p| {
            p.edit_document(|p| {
                let id = p.add_auto_rule(layer, group_id, 3)?;
                p.reorder_auto_rule(layer, group_id, id, index)?;
                Ok(id)
            })
        }) else {
            return false;
        };
        self.last_rule_list_row = Some(uid);
        self.begin_rule_edit(layer, group_id, uid);
        true
    }

    pub(super) fn begin_rule_edit(&mut self, layer: i64, group_id: i64, uid: i64) {
        let Some(project) = &self.project else { return };
        let Some(def) = project.layer_def(layer) else {
            return;
        };
        let Some(rule) = group(def, group_id).and_then(|g| rule(g, uid)) else {
            return;
        };
        let value = choose_initial_value(rule, source_def(project, def));
        let mut data = rule.clone();
        data["pattern"] = json!(resize_rule_pattern(rule, EDITOR_SIZE));
        data["size"] = json!(EDITOR_SIZE);
        self.rule_group_uid = Some(group_id);
        self.rule_uid = Some(uid);
        self.rule_draft = Some(RuleDraft {
            layer,
            group: group_id,
            rule: uid,
            data,
            value,
            stroke: None,
            output_pick: None,
            help: false,
        });
    }

    pub(in crate::app) fn finish_rule_edit(&mut self) {
        let Some(mut draft) = self.rule_draft.take() else {
            return;
        };
        let RuleDraft {
            layer,
            group: group_id,
            rule: uid,
            ..
        } = draft;
        ldtk_core::trim_auto_rule_pattern(&mut draft.data);
        let empty = draft.data["pattern"]
            .as_array()
            .is_some_and(|a| a.iter().all(|v| *v == 0))
            && draft.data["tileRectsIds"]
                .as_array()
                .is_none_or(Vec::is_empty);
        let Some(current) = self
            .project
            .as_ref()
            .and_then(|p| p.layer_def(layer))
            .and_then(|def| group(def, group_id))
            .and_then(|g| rule(g, uid))
        else {
            return;
        };
        if empty {
            self.edit("Empty rule removed", |p| {
                p.remove_auto_rule(layer, group_id, uid)
            });
            self.rule_uid = None;
            if self.last_rule_list_row == Some(uid) {
                self.last_rule_list_row = None;
            }
        } else if *current != draft.data {
            let mut patch = draft.data;
            if let Some(patch) = patch.as_object_mut() {
                patch.remove("uid");
                patch.remove("tileIds");
            }
            self.edit("Rule updated", |p| {
                p.update_auto_rule(layer, group_id, uid, &patch)
            });
        }
    }

    pub(super) fn rule_detail_dialog(&mut self, ctx: &egui::Context) {
        if let Some(mut draft) = self.rule_draft.take() {
            let data = self.project.as_ref().and_then(|p| {
                let def = p.layer_def(draft.layer)?;
                Some((source_def(p, def).clone(), self.rule_instance_atlas(def)))
            });
            let Some((source, atlas_id)) = data else {
                return;
            };
            let atlas = self.rule_atlas(ctx, atlas_id);
            let screen = ctx.content_rect();
            let width = if draft.help {
                (screen.width() - 104.).clamp(100., rule_guidance::WIDTH)
            } else {
                (screen.width() - 108.).clamp(330., rule_guidance::EDITOR_WIDTH)
            };
            let id = egui::Id::new(("auto-rule-editor", draft.rule));
            let modal = egui::Modal::new(id)
                .area(
                    egui::Area::new(id)
                        .kind(egui::UiKind::Modal)
                        .sense(Sense::hover())
                        .order(egui::Order::Foreground)
                        .pivot(Align2::LEFT_CENTER)
                        .fixed_pos(egui::pos2(screen.left() + 80., screen.center().y)),
                )
                .frame(project_dialogs::dialog_frame(12))
                .backdrop_color(Color32::TRANSPARENT)
                .show(ctx, |ui| {
                    let guided = draft.help;
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing = egui::vec2(8., 8.);
                    let mut editor = |ui: &mut egui::Ui| {
                        self.rule_editor_contents(
                            ui,
                            &mut draft,
                            &source,
                            atlas.as_ref(),
                            atlas_id.is_some(),
                        )
                    };
                    if guided {
                        let height = (screen.height() - 32.).max(100.);
                        ui.set_max_height(height);
                        let guidance = egui::ScrollArea::both()
                            .id_salt("rule-guided-scroll")
                            .max_width(width)
                            .max_height(height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| rule_guidance::show(ui, editor));
                        ui.painter()
                            .with_clip_rect(guidance.inner_rect.expand(8.))
                            .rect_stroke(
                                guidance.inner,
                                0.,
                                Stroke::new(1_f32, theme::HELP_TEXT),
                                StrokeKind::Inside,
                            );
                    } else {
                        editor(ui);
                    }
                });
            let selecting_output = draft.output_pick.is_some();
            if selecting_output && let Some(atlas) = atlas_id {
                self.rule_output_picker(ctx, atlas, &mut draft);
            }
            self.rule_draft = Some(draft);
            if !selecting_output && modal.should_close() {
                self.finish_rule_edit();
            }
        } else {
            self.rule_options_dialog(ctx);
        }
    }

    fn rule_editor_contents(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut RuleDraft,
        source: &Value,
        atlas: Option<&RuleAtlas>,
        can_pick: bool,
    ) -> rule_guidance::GuideRows {
        let guided = draft.help;
        let width = ui.available_width();
        let pattern_cell = ((width - 38.) * 0.52 / 9.).clamp(16., 36.);
        let grid_width = pattern_cell * EDITOR_SIZE as f32 + 30.;
        let palette_width = (width - grid_width - 8.).max(90.);
        let output = egui::Frame::new()
            .fill(theme::DARK)
            .inner_margin(16)
            .corner_radius(3)
            .show(ui, |ui| {
                ui.set_min_width(width - 32.);
                ui.horizontal(|ui| {
                    let mut stamp = draft.data["tileMode"] == "Stamp";
                    ui.set_min_height(if stamp { 60. } else { 56. });
                    let before = stamp;
                    ui.allocate_ui_with_layout(
                        egui::vec2(117., 30.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().button_padding.x = 5.;
                            ui.spacing_mut().icon_width = 9.;
                            ui.spacing_mut().icon_spacing = 4.;
                            let mode = source_controls::form_combo("rule-output-mode")
                                .width(117.)
                                .truncate()
                                .selected_text(
                                    RichText::new(locale::tr(ui, if stamp {
                                        "Rectangles of tiles"
                                    } else {
                                        "Individual tiles"
                                    }))
                                    .size(13.),
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut stamp, false, locale::tr(ui, "Individual tiles"));
                                    ui.selectable_value(&mut stamp, true, locale::tr(ui, "Rectangles of tiles"));
                                });
                            if guided {
                                mode.response.action_tooltip(locale::tr(ui, "Individual tiles randomly picks from the selected tiles. Rectangles of tiles places a whole rectangle in each matching cell."));
                            }
                        },
                    );
                    if stamp != before {
                        draft.data["tileMode"] = json!(if stamp { "Stamp" } else { "Single" });
                        draft.data["tileRectsIds"] = json!([]);
                    }
                    let mut remove = None;
                    let Some(outputs) = draft.data["tileRectsIds"].as_array() else {
                        return;
                    };
                    let reserve = if stamp { 76. } else { 0. };
                    egui::ScrollArea::horizontal()
                        .max_width((ui.available_width() - reserve).clamp(49., 356.))
                        .max_height(56.)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 1.;
                            for (index, ids) in outputs.iter().enumerate() {
                                let (rect, r) =
                                    ui.allocate_exact_size(Vec2::splat(49.), Sense::click());
                                ui.painter().rect_stroke(
                                    rect,
                                    1.,
                                    Stroke::new(1_f32, theme::LIGHT),
                                    StrokeKind::Inside,
                                );
                                if let (Some(atlas), Some(ids)) = (atlas, ids.as_array()) {
                                    atlas.paint_ids(ui, rect.shrink(3.), ids);
                                }
                                if can_pick && r.clicked() {
                                    draft.output_pick = Some(if stamp { index } else { 0 });
                                }
                                if stamp && (r.secondary_clicked() || r.middle_clicked()) {
                                    remove = Some(index);
                                }
                                if guided {
                                    r.action_tooltip(locale::tr(ui, "Choose tiles by clicking or dragging in the tileset."));
                                }
                            }
                            if stamp && !outputs.is_empty() {
                                if ui
                                    .add_enabled_ui(can_pick, |ui| {
                                        source_controls::add_sized_button(
                                            ui,
                                            [49., 49.],
                                            egui::Button::new("+"),
                                        )
                                        .action_tooltip(locale::tr(ui, "Choose a rectangle"))
                                    })
                                    .inner
                                    .clicked()
                                {
                                    draft.output_pick = Some(outputs.len());
                                }
                            } else {
                                let (rect, space) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width().max(24.), 49.),
                                    Sense::click(),
                                );
                                ui.painter()
                                    .rect_filled(rect, 2., Color32::from_rgb(27, 30, 37));
                                if can_pick && space.action_tooltip(locale::tr(ui, "Choose tile(s)")).clicked() {
                                    draft.output_pick = Some(0);
                                }
                            }
                        });
                    if let Some(index) = remove {
                        draft.data["tileRectsIds"]
                            .as_array_mut()
                            .unwrap()
                            .remove(index);
                    }
                    if stamp {
                        ui.add_space((ui.available_width() - 68.).max(0.));
                        let pivot = [
                            draft.data["pivotX"].as_f64().unwrap_or(0.),
                            draft.data["pivotY"].as_f64().unwrap_or(0.),
                        ];
                        if let Some([x, y]) = source_controls::pivot_grid(ui, pivot) {
                            draft.data["pivotX"] = json!(x);
                            draft.data["pivotY"] = json!(y);
                        }
                    }
                });
            });
        let pattern = ui.horizontal_top(|ui| {
            let palette_height = self.rule_value_palette(
                ui,
                draft,
                source,
                (palette_width - 32.).max(52.),
                pattern_cell * EDITOR_SIZE as f32 + 2.,
            );
            egui::Frame::new()
                .fill(theme::DARK)
                .corner_radius(3)
                .inner_margin(16)
                .show(ui, |ui| {
                    ui.set_min_height(palette_height - 32.);
                    let (grid, response) = ui.allocate_exact_size(
                        Vec2::splat(pattern_cell * EDITOR_SIZE as f32 - 2.),
                        Sense::click_and_drag(),
                    );
                    let pointer = response
                        .contains_pointer()
                        .then(|| ui.input(|i| i.pointer.latest_pos()))
                        .flatten();
                    let cell = pointer.and_then(|pointer| {
                        let pos = pointer - grid.min;
                        let x = (pos.x / pattern_cell).floor() as usize;
                        let y = (pos.y / pattern_cell).floor() as usize;
                        (grid.contains(pointer)
                            && pos.x % pattern_cell < pattern_cell - 2.
                            && pos.y % pattern_cell < pattern_cell - 2.)
                            .then_some(y * EDITOR_SIZE + x)
                    });
                    if let Some(index) = cell {
                        let current = draft.data["pattern"][index].as_i64().unwrap_or(0);
                        let pressed = ui.input(|i| {
                            [
                                egui::PointerButton::Primary,
                                egui::PointerButton::Secondary,
                                egui::PointerButton::Middle,
                            ]
                            .into_iter()
                            .find(|button| i.pointer.button_pressed(*button))
                        });
                        if let Some(button) = pressed {
                            draft.stroke = Some(PatternStroke {
                                button,
                                initial: current,
                            });
                        }
                        if let Some(stroke) = draft.stroke
                            && (pressed == Some(stroke.button)
                                || ui.input(|i| i.pointer.button_down(stroke.button)))
                        {
                            draft.data["pattern"][index] =
                                json!(stroke.apply(current, draft.value));
                        }
                    }
                    if draft
                        .stroke
                        .is_some_and(|stroke| !ui.input(|i| i.pointer.button_down(stroke.button)))
                    {
                        draft.stroke = None;
                    }
                    for (index, value) in
                        draft.data["pattern"].as_array().unwrap().iter().enumerate()
                    {
                        let rect = Rect::from_min_size(
                            grid.min
                                + egui::vec2(
                                    (index % EDITOR_SIZE) as f32 * pattern_cell,
                                    (index / EDITOR_SIZE) as f32 * pattern_cell,
                                ),
                            Vec2::splat(pattern_cell - 2.),
                        );
                        self.paint_rule_cell(
                            ui,
                            rect,
                            source,
                            value.as_i64().unwrap_or(0),
                            RuleCellStyle::Editor {
                                center: index == EDITOR_SIZE * EDITOR_SIZE / 2
                                    && draft.data["tileMode"] != "Stamp",
                                hovered: cell == Some(index),
                            },
                        );
                    }
                    if draft.data["tileMode"] == "Stamp" {
                        let cell = pattern_cell - 4.;
                        let span = match (atlas, draft.data["tileRectsIds"][0].as_array()) {
                            (Some(atlas), Some(ids)) => atlas.stamp_preview_size(cell, ids),
                            _ => Vec2::splat(cell),
                        };
                        let pivot = egui::vec2(
                            number(&draft.data, "pivotX", 0.),
                            number(&draft.data, "pivotY", 0.),
                        );
                        let origin = grid.min
                            + Vec2::splat(pattern_cell * 4. + 2.)
                            + pivot * (Vec2::splat(cell) - span);
                        ui.painter().rect_stroke(
                            Rect::from_min_size(origin, span),
                            0.,
                            Stroke::new(2_f32, theme::ACCENT),
                            StrokeKind::Inside,
                        );
                    }
                    cell.map(|index| draft.data["pattern"][index].as_i64().unwrap_or(0))
                })
                .inner
        });
        let pattern_and_explanation = if guided {
            let description = pattern
                .inner
                .and_then(|value| rule_guidance::describe(ui.ctx(), source, value));
            rule_guidance::explanation(ui, width, description).bottom()
                - pattern.response.rect.top()
        } else {
            if source_controls::add_sized_button(
                ui,
                [width, 31.],
                egui::Button::new(
                    RichText::new(locale::tr(ui, "Enable help")).color(theme::HELP_TEXT),
                )
                .fill(theme::HELP_FILL),
            )
            .clicked()
            {
                draft.help = true;
            }
            pattern.response.rect.height()
        };
        let boundary = egui::Frame::new()
            .fill(theme::DARK)
            .inner_margin(16)
            .corner_radius(3)
            .show(ui, |ui| {
                ui.set_min_width(width - 32.);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(locale::tr(ui, "\"Out-of-bounds\" policy")).size(12.).color(rule_guidance::MUTED));
                    let mut outside = draft.data["outOfBoundsValue"].as_i64();
                    let before = outside;
                    const DEFAULT: &str = "This rule should not apply when reading cells outside of layer bounds (default)";
                    let values = source["intGridValues"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                    let default = locale::tr(ui, DEFAULT);
                    let empty = locale::tr(ui, "Empty cells");
                    let pick = locale::tr(ui, "-- Pick a value --");
                    let label = |value| match value {
                        None => default.to_owned(),
                        Some(0) => empty.into(),
                        Some(v) if v < 0 => pick.into(),
                        Some(v) => values.iter().find(|entry| entry["value"] == v)
                            .and_then(|entry| entry["identifier"].as_str())
                            .map_or_else(|| v.to_string(), |name| format!("{v} - {name}")),
                    };
                    let width = ui.available_width().min(250.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 29.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let menu = source_controls::form_combo("rule-outside")
                                .width(width)
                                .truncate()
                                .selected_text(RichText::new(label(before)).size(13.333))
                                .show_ui(ui, |ui| {
                                    if before.is_some_and(|value| value < 0) {
                                        ui.selectable_value(&mut outside, before, label(before));
                                    }
                                    ui.selectable_value(&mut outside, None, default);
                                    ui.selectable_value(&mut outside, Some(0), empty);
                                    for value in values.iter().filter_map(|value| value["value"].as_i64()) {
                                        ui.selectable_value(&mut outside, Some(value), label(Some(value)));
                                    }
                                });
                            if guided {
                                menu.response.action_tooltip(locale::tr(ui, "Choose the IntGrid value used when inspecting a cell outside the layer bounds. The default skips rules that need to read outside the level."));
                            }
                        },
                    );
                    if outside != before {
                        draft.data["outOfBoundsValue"] = json!(outside);
                    }
                });
            });
        ui.painter().text(
            boundary.response.rect.right_bottom() + egui::vec2(5., 5.),
            Align2::RIGHT_CENTER,
            format!("#{}", draft.rule),
            FontId::proportional(10.667),
            rule_guidance::MUTED,
        );
        rule_guidance::GuideRows {
            output: output.response.rect.height(),
            pattern_and_explanation,
        }
    }

    pub(super) fn paint_rule_cell(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        source: &Value,
        value: i64,
        style: RuleCellStyle,
    ) {
        let (scale, center, hovered, preview) = match style {
            RuleCellStyle::Editor { center, hovered } => (1., center, hovered, false),
            RuleCellStyle::Preview(scale) => (scale, false, false, true),
        };
        let painter = ui.painter();
        let id = value.saturating_abs();
        if preview && id == 0 {
            painter.rect_filled(rect, 0., theme::MEDIUM);
            return;
        }
        let red = Color32::from_rgb(227, 77, 0);
        painter.rect_filled(rect, 2., theme::MEDIUM);
        match id {
            0 => egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                .paint_at(ui, rect),
            ANY_VALUE if value > 0 => {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "?",
                    FontId::proportional(21. * scale),
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    rect.shrink(3. * scale),
                    1.,
                    Stroke::new(scale, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            ANY_VALUE => {}
            1_000.. => {
                let color = rule_value_group(source, id / 1_000 - 1)
                    .and_then(|group| group["color"].as_str())
                    .map(theme::color);
                if let Some(color) = color {
                    painter.rect_filled(
                        rect,
                        2.,
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 230),
                    );
                }
                egui::Image::new(egui::include_image!("../../assets/icons/folderClose.svg"))
                    .paint_at(
                        ui,
                        Rect::from_center_size(rect.center(), rect.size() * 0.65),
                    );
                painter.rect_stroke(
                    rect.shrink(2. * scale),
                    1.,
                    Stroke::new(
                        scale,
                        color.map_or(Color32::WHITE, |color| {
                            color.lerp_to_gamma(Color32::WHITE, 0.6)
                        }),
                    ),
                    StrokeKind::Inside,
                );
            }
            _ => {
                if let Some(value) = source["intGridValues"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|value| value["value"] == id)
                {
                    let color = value["color"]
                        .as_str()
                        .map(theme::color)
                        .unwrap_or(theme::LIGHT);
                    painter.rect_filled(rect, 2., color);
                    self.rule_value_tile(ui, rect.shrink(2. * scale), value);
                } else if value < 0 {
                    painter.rect_filled(rect, 2., Color32::from_rgb(255, 0, 255));
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "ERR",
                        FontId::proportional(13.),
                        Color32::WHITE,
                    );
                }
            }
        }
        if value < 0 {
            let icon = if id == ANY_VALUE {
                egui::Image::new(egui::include_image!("../../assets/icons/prohibited.svg"))
                    .tint(Color32::LIGHT_RED)
            } else {
                egui::Image::new(egui::include_image!("../../assets/icons/cross.png"))
                    .texture_options(egui::TextureOptions::NEAREST)
            };
            icon.paint_at(
                ui,
                Rect::from_center_size(rect.center(), rect.size() * 0.65),
            );
            if id < 1_000 || id == ANY_VALUE {
                painter.rect_stroke(
                    rect.shrink(4. * scale),
                    1.,
                    Stroke::new(2. * scale, red),
                    StrokeKind::Inside,
                );
            }
        }
        let border = if hovered {
            theme::ACCENT
        } else if center {
            Color32::from_white_alpha(174)
        } else {
            theme::LIGHT.gamma_multiply(0.62)
        };
        painter.rect_stroke(rect, 2., Stroke::new(1_f32, border), StrokeKind::Inside);
    }

    fn rule_output_picker(&mut self, ctx: &egui::Context, atlas: i64, draft: &mut RuleDraft) {
        let Some(slot) = draft.output_pick else {
            return;
        };
        let id = egui::Id::new(("rule-output-picker", draft.rule, slot));
        let initializing = !ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        let stamp = draft.data["tileMode"] == "Stamp";
        self.with_local_atlas_state(ctx, id, |app| {
            if initializing {
                app.atlas_selection = Some((atlas, vec![]));
                if let Some(list) = draft.data["tileRectsIds"].as_array() {
                    let ids = if stamp {
                        list.get(slot)
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                    } else {
                        list.iter().filter_map(|rect| rect.get(0)).collect()
                    };
                    app.set_atlas_selection(
                        atlas,
                        ids.into_iter()
                            .filter_map(Value::as_i64)
                            .filter_map(|id| i32::try_from(id).ok())
                            .collect(),
                    );
                }
                ctx.data_mut(|d| d.insert_temp(id, true));
            }
            let viewport = ctx.content_rect();
            let modal = egui::Modal::new(id)
                .frame(
                    project_dialogs::dialog_frame(0)
                        .fill(theme::DARK)
                        .corner_radius(0.)
                        .stroke(Stroke::new(1_f32, theme::LIGHT)),
                )
                .area(
                    egui::Area::new(id)
                        .kind(egui::UiKind::Modal)
                        .sense(Sense::hover())
                        .order(egui::Order::Foreground)
                        .fixed_pos(viewport.left_top() + Vec2::splat(50.)),
                )
                .show(ctx, |ui| {
                    ui.set_width(viewport.width() * 0.9);
                    ui.set_height(viewport.height() * 0.9);
                    app.atlas_rule_output_picker(ctx, ui, atlas, stamp, initializing)
                });
            let closed = modal.should_close();
            if (stamp && modal.inner) || (!stamp && closed) {
                let ids = app.selected_tile_ids_for(atlas);
                if !ids.is_empty() {
                    let list = draft.data["tileRectsIds"].as_array_mut().unwrap();
                    if !stamp {
                        *list = ids.into_iter().map(|id| json!([id])).collect();
                    } else if slot < list.len() {
                        list[slot] = json!(ids);
                    } else {
                        list.push(json!(ids));
                    }
                }
            }
            if closed || (stamp && modal.inner) {
                draft.output_pick = None;
                ctx.data_mut(|d| d.remove::<bool>(id));
            }
        });
    }

    pub(super) fn set_rule_options(
        &mut self,
        ctx: &egui::Context,
        layer: i64,
        group_id: i64,
        rule: Option<i64>,
        options: RuleOptions,
    ) {
        ctx.data_mut(|d| {
            d.remove::<Rect>(rule_options_anchor_id());
            d.remove::<[bool; 2]>(rule_options_id().with("offset-links"));
            d.insert_temp(rule_options_id(), (layer, group_id, rule, options, true));
        });
    }

    pub(super) fn set_anchored_rule_options(
        &mut self,
        ctx: &egui::Context,
        layer: i64,
        group: i64,
        rule: i64,
        options: RuleOptions,
        anchor: Option<Rect>,
    ) {
        self.set_rule_options(ctx, layer, group, Some(rule), options);
        ctx.data_mut(|d| {
            if let Some(anchor) = anchor {
                d.insert_temp(rule_options_anchor_id(), anchor);
            } else {
                d.remove::<Rect>(rule_options_anchor_id());
            }
        });
    }

    fn offset_settings(
        &mut self,
        ctx: &egui::Context,
        layer: i64,
        gid: i64,
        rid: i64,
        rule: &Value,
    ) {
        const LIMIT: i64 = 1024;
        let id = rule_options_id();
        let links_id = id.with("offset-links");
        let mut offsets =
            OFFSET_AXES.map(|fields| fields.map(|key| rule[key].as_i64().unwrap_or(0)));
        let initial = offsets;
        let mut linked = ctx
            .data(|d| d.get_temp::<[bool; 2]>(links_id))
            .unwrap_or_else(|| offsets.map(|[_, min, max]| min == max.saturating_neg()));
        let modal = rule_settings_modal(ctx, egui::vec2(500., 275.), 21).show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0., 10.);
            for (axis, label) in ["Tile X offset", "Tile Y offset"].into_iter().enumerate() {
                ui.horizontal(|ui| {
                    offset_label(ui, label, 128., egui::Align::Max);
                    ui.add_space(12.);
                    if let Some(value) = offset_input(
                        ui,
                        OFFSET_AXES[axis][0],
                        offsets[axis][0],
                        (-LIMIT, LIMIT),
                        265.,
                    ) {
                        offsets[axis][0] = value;
                    }
                    offset_label(ui, "px", 25., egui::Align::Center);
                    if offset_icon(
                        ui,
                        egui::include_image!("../../assets/icons/delete.svg"),
                        Color32::WHITE,
                        "Reset offset",
                    )
                    .clicked()
                    {
                        offsets[axis][0] = 0;
                    }
                });
            }
            for (axis, label) in ["Tile random X offset", "Tile random Y offset"]
                .into_iter()
                .enumerate()
            {
                let [_, min_field, max_field] = OFFSET_AXES[axis];
                let [_, mut min, mut max] = offsets[axis];
                ui.horizontal(|ui| {
                    offset_label(ui, label, 128., egui::Align::Max);
                    ui.add_space(12.);
                    offset_label(ui, "min=", 45., egui::Align::Center);
                    let was_linked = linked[axis];
                    if let Some(value) = ui
                        .add_enabled_ui(!was_linked, |ui| {
                            offset_input(
                                ui,
                                min_field,
                                min,
                                (-LIMIT, LIMIT),
                                if was_linked { 41. } else { 56. },
                            )
                        })
                        .inner
                    {
                        min = value.min(max);
                    }
                    offset_label(ui, "px,", 30., egui::Align::Center);
                    let (link_icon, tint, action) = if was_linked {
                        (
                            egui::include_image!("../../assets/icons/link.svg"),
                            theme::ACCENT,
                            "Unlink range",
                        )
                    } else {
                        (
                            egui::include_image!("../../assets/icons/unlink.svg"),
                            theme::ACCENT.gamma_multiply(0.35),
                            "Link range",
                        )
                    };
                    if offset_icon(ui, link_icon, tint, action).clicked() {
                        linked[axis] = !was_linked;
                        if linked[axis] {
                            max = max.max(min);
                            min = max.saturating_neg();
                        }
                    }
                    offset_label(ui, "max=", 51., egui::Align::Center);
                    let lower = if linked[axis] { 0 } else { -LIMIT };
                    if let Some(value) = offset_input(
                        ui,
                        max_field,
                        max,
                        (lower, LIMIT),
                        if was_linked { 71. } else { 56. },
                    ) {
                        max = value.max(min);
                        if linked[axis] {
                            min = max.saturating_neg();
                        }
                    }
                    offset_label(ui, "px", 25., egui::Align::Center);
                    if offset_icon(
                        ui,
                        egui::include_image!("../../assets/icons/delete.svg"),
                        Color32::WHITE,
                        "Reset random range",
                    )
                    .clicked()
                    {
                        min = 0;
                        max = 0;
                    }
                });
                offsets[axis][1] = min;
                offsets[axis][2] = max;
            }
            ui.add_space(17.);
            let reset = egui::Button::image_and_text(
                egui::Image::new(egui::include_image!("../../assets/icons/delete.svg"))
                    .fit_to_exact_size(Vec2::splat(17.)),
                RichText::new(locale::tr(ui, "RESET ALL OFFSETS"))
                    .size(13.333)
                    .color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(121, 17, 5));
            if source_controls::add_sized_button(ui, [458., 43.], reset).clicked() {
                offsets = [[0; 3]; 2];
            }
        });
        let mut patch = json!({});
        for (keys, (before, after)) in OFFSET_AXES
            .into_iter()
            .zip(initial.into_iter().zip(offsets))
        {
            for (key, (old, new)) in keys.into_iter().zip(before.into_iter().zip(after)) {
                if old != new {
                    patch[key] = json!(new);
                }
            }
        }
        if changed(&patch) {
            self.edit("Rule changed", |p| {
                p.update_auto_rule(layer, gid, rid, &patch)
            });
        }
        let close = modal.should_close();
        ctx.data_mut(|d| {
            if close {
                d.remove::<RuleOptionsState>(id);
                d.remove::<Rect>(rule_options_anchor_id());
                d.remove::<[bool; 2]>(links_id);
            } else {
                d.insert_temp(links_id, linked);
            }
        });
    }

    fn modulo_settings(
        &mut self,
        ctx: &egui::Context,
        layer: i64,
        gid: i64,
        rid: i64,
        rule: &Value,
        opening: bool,
    ) {
        let initial = MODULO_FIELDS.map(|key| {
            rule[key]
                .as_i64()
                .unwrap_or(if key.ends_with("Modulo") { 1 } else { 0 })
        });
        let mut values = initial;
        let mut checker = text(rule, "checker", "None");
        let initial_checker = checker;
        let (_, _, preview_height) = modulo_grid([values[0], values[1]]);
        let height = preview_height.max(134.) + 306.;
        let viewport_height = ctx.content_rect().height();
        let id = rule_options_id();
        let mut laid_out = false;
        let mut numeric_input = false;
        let modal = rule_settings_modal(ctx, egui::vec2(442., height.min(viewport_height)), 46)
            .show(ctx, |ui| {
                laid_out = !ui.is_sizing_pass();
                let mut body = |ui: &mut egui::Ui| {
                    ui.set_width(400.);
                    ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                    source_controls::help_card(ui, 400., 117., |ui| {
                        source_controls::help_paragraph(
                            ui,
                            "Modulo values will allow you to limit this rule to every X columns or every Y rows. For example, if X modulo is 3, then the rule will only apply every 3 columns. If the X offset is 1, then the column counting will be offseted by 1.",
                        );
                    });
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        ui.add_space(16.);
                        if let Some(value) = modulo_field(ui, 0, "X modulo", values[0], opening) {
                            values[0] = value;
                            numeric_input = true;
                        }
                        ui.add_sized([28., 50.], egui::Label::new("+"));
                        if let Some(value) = modulo_field(ui, 2, "Offset", values[2], false) {
                            values[2] = value % values[0].max(1);
                            numeric_input = true;
                        }
                        ui.add_space(24.);
                        let reset = egui::Label::new(locale::tr(ui, "Reset"));
                        if ui.add_sized(
                            [132., 50.],
                            source_controls::underlined_link(reset),
                        ).clicked() {
                            values = [1, 1, 0, 0];
                            checker = "None";
                        }
                    });
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        modulo_preview(ui, values);
                        ui.add_space(8.);
                        ui.vertical_centered(|ui| {
                            ui.set_width(132.);
                            ui.spacing_mut().item_spacing.y = 8.;
                            let (_, _, height) = modulo_grid([values[0], values[1]]);
                            ui.add_space(((height - 134.) * 0.5).max(0.));
                            if let Some(value) = modulo_field(ui, 1, "Y modulo", values[1], false) {
                                values[1] = value;
                                numeric_input = true;
                            }
                            ui.label("+");
                            if let Some(value) = modulo_field(ui, 3, "Offset", values[3], false) {
                                values[3] = value;
                                numeric_input = true;
                            }
                        });
                    });
                    if values[..2] != initial[..2] {
                        checker = match checker {
                            "Horizontal" if values[0] == 1 => if values[1] > 1 { "Vertical" } else { "None" },
                            "Vertical" if values[1] == 1 => if values[0] > 1 { "Horizontal" } else { "None" },
                            other => other,
                        };
                    }
                    ui.add_space(16.);
                    let (line, _) = ui.allocate_exact_size(egui::vec2(400., 1.), Sense::hover());
                    ui.painter().hline(line.x_range(), line.top(), Stroke::new(1_f32, theme::LIGHT));
                    ui.add_space(8.);
                    source_controls::form_row(ui, "Checkboard mode", "checker", 148., |ui| {
                        ui.spacing_mut().interact_size.y = 30.;
                        let choices = [
                            ("None", "Off", true),
                            ("Horizontal", "Horizontally", values[0] > 1),
                            ("Vertical", "Vertically", values[1] > 1),
                        ];
                        let label = choices.iter().find(|(value, _, _)| *value == checker)
                            .map_or("Off", |(_, label, _)| *label);
                        source_controls::form_combo("checker").width(244.)
                            .selected_text(RichText::new(locale::tr(ui, label)).font(theme::TextRole::FormValue.font()))
                            .show_ui(ui, |ui| {
                                for (value, label, enabled) in choices {
                                    if ui.add_enabled(enabled, egui::Button::selectable(
                                        checker == value, locale::tr(ui, label),
                                    )).clicked() {
                                        checker = value;
                                        ui.close();
                                    }
                                }
                            });
                    });
                };
                if height > viewport_height {
                    egui::ScrollArea::vertical()
                        .max_height((viewport_height - 70.).max(100.))
                        .show(ui, body);
                } else {
                    body(ui);
                }
            });
        let mut patch = json!({});
        for (key, (before, after)) in MODULO_FIELDS
            .into_iter()
            .zip(initial.into_iter().zip(values))
        {
            if before != after {
                patch[key] = json!(after);
            }
        }
        if initial_checker != checker {
            patch["checker"] = json!(checker);
        }
        let edited = changed(&patch)
            && self
                .edit("Rule changed", |p| {
                    p.update_auto_rule(layer, gid, rid, &patch)
                })
                .is_some();
        let close = modal.should_close();
        ctx.data_mut(|data| {
            if close || numeric_input {
                data.remove::<Rect>(rule_options_anchor_id());
            }
            if close {
                data.remove::<RuleOptionsState>(id);
            } else {
                data.insert_temp(
                    id,
                    (
                        layer,
                        gid,
                        Some(rid),
                        RuleOptions::Modulo,
                        edited || numeric_input || opening && !laid_out,
                    ),
                );
            }
        });
    }

    fn perlin_settings(
        &mut self,
        ctx: &egui::Context,
        layer: i64,
        group: i64,
        rid: i64,
        rule: &Value,
    ) {
        use source_controls::NumberStyle;
        use std::hash::BuildHasher;
        let id = rule_options_id();
        let seed = rule["perlinSeed"].as_i64().unwrap_or(0);
        let scale = rule["perlinScale"].as_f64().unwrap_or(0.2) * 100.;
        let octaves = rule["perlinOctaves"].as_i64().unwrap_or(2);
        let mut patch = json!({});
        let modal = rule_settings_modal(ctx, egui::vec2(350., 187.), 46).show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.;
            for (key, label, current, style, width) in [
                (
                    "perlinSeed",
                    "Seed",
                    seed.to_string(),
                    NumberStyle::Plain { placeholder: None },
                    153.,
                ),
                (
                    "perlinScale",
                    "Scale",
                    scale.to_string(),
                    NumberStyle::PercentSlider { min: 1., max: 99. },
                    62.,
                ),
                (
                    "perlinOctaves",
                    "Octaves",
                    octaves.to_string(),
                    NumberStyle::IntegerSlider {
                        min: 1.,
                        max: 4.,
                        speed: 0.0176,
                        placeholder: None,
                    },
                    248.,
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    source_controls::property_label_with_width(ui, label, key, 60.);
                    if let Some(value) =
                        source_controls::number_input_sized(ui, key, &current, style, width)
                    {
                        let parsed = value
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite())
                            .unwrap_or(0.);
                        let value = match key {
                            "perlinScale" => json!(parsed.clamp(1., 99.) / 100.),
                            "perlinOctaves" => json!((parsed as i64).clamp(1, 4)),
                            _ => json!((parsed as i64).clamp(i16::MIN.into(), i16::MAX.into())),
                        };
                        if rule[key] != value {
                            patch[key] = value;
                        }
                    }
                    if key == "perlinSeed" {
                        ui.add_space(1.);
                        if project_dialogs::button(
                            ui,
                            "RANDOM",
                            egui::vec2(93., 32.),
                            false,
                            13.333,
                        )
                        .clicked()
                        {
                            let next = std::hash::RandomState::new().hash_one(seed) % 99_999_999;
                            patch[key] = json!(if next as i64 == seed {
                                (next + 1) % 99_999_999
                            } else {
                                next
                            });
                        }
                    } else if key == "perlinScale" {
                        ui.add_space(4.);
                        ui.label("%");
                    }
                });
            }
        });
        if changed(&patch) {
            self.edit("Rule changed", |p| {
                p.update_auto_rule(layer, group, rid, &patch)
            });
        }
        if modal.should_close() {
            ctx.data_mut(|d| {
                d.remove::<RuleOptionsState>(id);
                d.remove::<Rect>(rule_options_anchor_id());
            });
        }
    }

    pub(in crate::app) fn draw_perlin_preview(
        &self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        level: LevelKey,
    ) {
        if self.panel != Some(Panel::Rules) || self.world {
            return;
        }
        let Some((layer, gid, Some(rid), RuleOptions::Noise, _)) =
            ctx.data(|d| d.get_temp::<RuleOptionsState>(rule_options_id()))
        else {
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(rule) = project
            .layer_def(layer)
            .and_then(|def| group(def, gid))
            .and_then(|g| rule(g, rid))
        else {
            return;
        };
        let Some((index, instance)) = project
            .level(level)
            .and_then(|level| level["layerInstances"].as_array())
            .and_then(|layers| {
                layers
                    .iter()
                    .enumerate()
                    .find(|(_, li)| li["layerDefUid"] == layer)
            })
        else {
            return;
        };
        let origin = canvas::layer_to_world(project, level, index, &self.viewport, [0., 0.]);
        let origin = egui::Pos2::from(self.viewport.to_screen(
            origin,
            rect.min.into(),
            rect.size().into(),
        ));
        let step = number(instance, "__gridSize", 16.) * self.viewport.zoom;
        if step <= 0. || !step.is_finite() {
            return;
        }
        let clip = self
            .uncovered_canvas_rect(rect)
            .intersect(painter.clip_rect());
        let range = |start: f32, end: f32, origin: f32, key| {
            let count = instance[key]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(0)
                .max(0);
            (((start - origin) / step).floor() as i32).clamp(0, count)
                ..(((end - origin) / step).ceil() as i32).clamp(0, count)
        };
        let xs = range(clip.left(), clip.right(), origin.x, "__cWid");
        let ys = range(clip.top(), clip.bottom(), origin.y, "__cHei");
        let seed = instance["seed"].as_i64().unwrap_or(0) as i32;
        let painter = painter.with_clip_rect(clip);
        for y in ys {
            for x in xs.clone() {
                let color = if ldtk_core::auto_rule_perlin_value(seed, [x, y], rule) < 0. {
                    Color32::from_rgba_unmultiplied(255, 0, 0, 128)
                } else {
                    Color32::from_rgba_unmultiplied(179, 247, 0, 77)
                };
                painter.rect_filled(
                    Rect::from_min_size(
                        origin + egui::vec2(x as f32, y as f32) * step,
                        Vec2::splat(step),
                    ),
                    0.,
                    color,
                );
            }
        }
    }

    fn rule_options_dialog(&mut self, ctx: &egui::Context) {
        let id = rule_options_id();
        let Some((layer, gid, rid, options, opening)) =
            ctx.data(|d| d.get_temp::<RuleOptionsState>(id))
        else {
            return;
        };
        let values = self.project.as_ref().and_then(|p| {
            let d = p.layer_def(layer)?;
            Some((d.clone(), group(d, gid)?.clone()))
        });
        let Some((def, g)) = values else {
            ctx.data_mut(|data| data.remove::<RuleOptionsState>(id));
            return;
        };
        if matches!(
            options,
            RuleOptions::Noise | RuleOptions::Offsets | RuleOptions::Modulo
        ) {
            if let Some((rid, rule)) = rid.and_then(|rid| Some((rid, rule(&g, rid)?))) {
                match options {
                    RuleOptions::Noise => self.perlin_settings(ctx, layer, gid, rid, rule),
                    RuleOptions::Offsets => self.offset_settings(ctx, layer, gid, rid, rule),
                    _ => self.modulo_settings(ctx, layer, gid, rid, rule, opening),
                }
            } else {
                ctx.data_mut(|d| d.remove::<RuleOptionsState>(id));
            }
            return;
        }
        if matches!(options, RuleOptions::GroupColor | RuleOptions::GroupIcon) {
            let closed = if options == RuleOptions::GroupColor {
                let before = g["color"]
                    .as_str()
                    .map(theme::color)
                    .map(|c| [c.r(), c.g(), c.b()]);
                let mut color = before;
                let closed = source_colors::standalone_nullable_color_dialog(
                    ctx,
                    id.with("group-color"),
                    &mut color,
                    opening,
                );
                if closed && before != color {
                    let value = color.map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}"));
                    self.edit("Rule group color changed", |p| {
                        p.update_auto_rule_group(layer, gid, &json!({"color":value}))
                    });
                }
                closed
            } else if let Some(atlas) = self.rule_instance_atlas(&def) {
                let (icon, keep_open) = self.show_icon_atlas_picker(
                    ctx,
                    id.with(("group-icon", layer, gid)),
                    atlas,
                    &g["icon"],
                    opening,
                );
                if let Some(icon) = icon.filter(|icon| icon != &g["icon"]) {
                    self.edit("Rule group icon changed", |p| {
                        p.update_auto_rule_group(layer, gid, &json!({"icon":icon}))
                    });
                }
                !keep_open
            } else {
                self.status = "Invalid layer tileset".into();
                self.status_error = true;
                true
            };
            ctx.data_mut(|data| {
                if closed {
                    data.remove::<RuleOptionsState>(id);
                } else {
                    data.insert_temp(id, (layer, gid, rid, options, false));
                }
            });
            return;
        }
        let (title, message) = if options == RuleOptions::Advanced {
            (
                "Switch to advanced mode",
                "Switch to advanced mode to edit these rules manually.",
            )
        } else {
            (
                "Disable optional state",
                "Removing the optional status clears this group's on/off state in all levels.",
            )
        };
        let modal = project_dialogs::show_dialog(ctx, id, title, 412., |ui| {
            ui.label(locale::tr(ui, message));
            ui.horizontal_wrapped(|ui| {
                let confirm = project_dialogs::action(ui, "Confirm", true).clicked();
                let cancel = project_dialogs::action(ui, "Cancel", false).clicked();
                (confirm, cancel)
            })
            .inner
        });
        let (confirm, cancel) = modal.inner;
        if confirm {
            let patch = if options == RuleOptions::Advanced {
                json!({"usesWizard":false})
            } else {
                json!({"isOptional":false})
            };
            self.edit_recoverable("Rule group changed", |p| {
                p.update_auto_rule_group(layer, gid, &patch)
            });
        }
        if confirm || cancel || modal.should_close() {
            ctx.data_mut(|data| data.remove::<RuleOptionsState>(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_group_confirmations_use_the_shared_dialog_header_and_action_row() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        for (option, title) in [
            (RuleOptions::Advanced, "Switch to advanced mode"),
            (RuleOptions::RequireGroup, "Disable optional state"),
        ] {
            app.set_rule_options(&ctx, layer, group, None, option);
            let mut draw = || {
                ctx.run(egui::RawInput::default(), |ctx| {
                    app.rule_options_dialog(ctx)
                })
            };
            draw();
            let output = draw();
            let label = |name| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == name => {
                            Some(text.visual_bounding_rect().center())
                        }
                        _ => None,
                    })
                    .expect(name)
            };
            let heading = label(title);
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Rect(rect) if rect.fill == theme::LIGHT
                    && rect.rect.height() == 36. && rect.rect.contains(heading)
            )));
            let confirm = label("Confirm");
            let cancel = label("Cancel");
            assert!(confirm.x < cancel.x && (confirm.y - cancel.y).abs() < 2.);
        }
    }

    #[test]
    fn modulo_settings_preview_the_period_and_reset_it_as_one_edit() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let group_id = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let uid = project.add_auto_rule(layer, group_id, 1).unwrap();
        project
            .update_auto_rule(
                layer,
                group_id,
                uid,
                &json!({
                    "xModulo": 3, "yModulo": 2, "xOffset": 1, "yOffset": 0, "checker": "Horizontal"
                }),
            )
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.set_rule_options(&ctx, layer, group_id, Some(uid), RuleOptions::Modulo);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| app.rule_options_dialog(ctx),
            )
        };
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        let reset = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Reset" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Reset modulo settings");
        let active = output.shapes.iter().filter(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(cell) if cell.fill == Color32::from_rgb(60, 97, 72))
        }).count();
        assert_eq!(active, 12, "3 active columns by 4 active rows");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(reset),
                    egui::Event::PointerButton {
                        pos: reset,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let saved = app
            .project
            .as_ref()
            .unwrap()
            .copy_auto_rule(layer, group_id, Some(uid))
            .unwrap()
            .value;
        assert_eq!(saved["xModulo"], 1);
        assert_eq!(saved["yModulo"], 1);
        assert_eq!(saved["xOffset"], 0);
        assert_eq!(saved["checker"], "None");
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
    }

    #[test]
    fn random_offset_reset_clears_both_axes_in_one_edit() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let group_id = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let uid = project.add_auto_rule(layer, group_id, 1).unwrap();
        project
            .update_auto_rule(
                layer,
                group_id,
                uid,
                &json!({
                    "tileXOffset": 7, "tileYOffset": -8,
                    "tileRandomXMin": -2, "tileRandomXMax": 2,
                    "tileRandomYMin": 1, "tileRandomYMax": 5
                }),
            )
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.set_rule_options(&ctx, layer, group_id, Some(uid), RuleOptions::Offsets);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| app.rule_options_dialog(ctx),
            )
        };
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.text().eq_ignore_ascii_case("Reset all offsets") =>
                {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Reset all offsets");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let def = app.project.as_ref().unwrap().layer_def(layer).unwrap();
        let saved = rule(group(def, group_id).unwrap(), uid).unwrap();
        for key in [
            "tileXOffset",
            "tileYOffset",
            "tileRandomXMin",
            "tileRandomXMax",
            "tileRandomYMin",
            "tileRandomYMax",
        ] {
            assert_eq!(saved[key], 0, "{key}");
        }
        assert_eq!(saved["breakOnMatch"], true);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
    }

    #[test]
    fn pattern_stroke_uses_the_starting_cell_to_choose_erase_or_paint() {
        use egui::PointerButton::{Middle, Primary, Secondary};

        let stroke = |button, initial| PatternStroke { button, initial };
        assert_eq!(stroke(Secondary, 0).apply(1, 2_000), -2_000);
        assert_eq!(stroke(Secondary, 1).apply(-2_000, 2_000), 0);
        assert_eq!(stroke(Secondary, -1).apply(1, 2_000), 0);
        assert_eq!(stroke(Primary, -1).apply(-2_000, 2_000), 0);
        assert_eq!(stroke(Primary, -1).apply(1, 2_000), 1);
        assert_eq!(stroke(Primary, 1).apply(-1, ANY_VALUE), ANY_VALUE);
        assert_eq!(stroke(Middle, -1).apply(2_000, ANY_VALUE), 0);
    }

    #[test]
    fn closing_rule_editor_trims_the_draft_in_one_undoable_edit() {
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let group_id = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let uid = project.add_auto_rule(layer, group_id, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let before = app.project.as_ref().unwrap().json.clone();
        app.begin_rule_edit(layer, group_id, uid);
        let draft = app.rule_draft.as_mut().unwrap();
        assert_eq!(draft.data["size"], 9);
        draft.data["pattern"][42] = json!(1);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        app.finish_rule_edit();
        let def = app.project.as_ref().unwrap().layer_def(layer).unwrap();
        let edited = rule(group(def, group_id).unwrap(), uid).unwrap();
        assert_eq!(edited["size"], 5);
        assert_eq!(edited["pattern"][14], 1);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
}
