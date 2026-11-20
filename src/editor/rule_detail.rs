use super::*;

#[path = "rule_value_palette.rs"]
mod rule_value_palette;
use rule_value_palette::rule_value_group;
#[path = "rule_guidance.rs"]
mod rule_guidance;

const ANY_VALUE: i64 = 1_000_001;
const EDITOR_SIZE: usize = 9;

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

fn trim_pattern(data: &mut Value) {
    let mut size = data["size"].as_u64().unwrap_or(1) as usize;
    while size > 1 {
        let pattern = data["pattern"].as_array().unwrap();
        if (0..size).any(|i| {
            pattern[i] != 0
                || pattern[(size - 1) * size + i] != 0
                || pattern[i * size] != 0
                || pattern[i * size + size - 1] != 0
        }) {
            break;
        }
        size -= 2;
        data["pattern"] = json!(resize_rule_pattern(data, size));
        data["size"] = json!(size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        trim_pattern(&mut draft.data);
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
                                Stroke::new(1., rule_guidance::GREEN),
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
                            let mode = egui::ComboBox::from_id_salt("rule-output-mode")
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
                                mode.response.on_hover_text(locale::tr(ui, "Individual tiles randomly picks from the selected tiles. Rectangles of tiles places a whole rectangle in each matching cell."));
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
                                    Stroke::new(1., theme::LIGHT),
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
                                    r.on_hover_text(locale::tr(ui, "Choose tiles by clicking or dragging in the tileset."));
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
                                        .on_hover_text(locale::tr(ui, "Choose a rectangle"))
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
                                if can_pick && space.on_hover_text(locale::tr(ui, "Choose tile(s)")).clicked() {
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
                            index == EDITOR_SIZE * EDITOR_SIZE / 2
                                && draft.data["tileMode"] != "Stamp",
                            cell == Some(index),
                        );
                    }
                    if draft.data["tileMode"] == "Stamp" {
                        let cell = pattern_cell - 4.;
                        let mut span = egui::Vec2::splat(cell);
                        if let (Some(atlas), Some(ids)) =
                            (atlas, draft.data["tileRectsIds"][0].as_array())
                            && let Some(first) = ids.first().and_then(Value::as_i64)
                        {
                            let columns = atlas.columns;
                            for id in ids.iter().filter_map(Value::as_i64) {
                                if id % columns != first % columns {
                                    span.x = cell * 1.9;
                                }
                                if id / columns != first / columns {
                                    span.y = cell * 1.9;
                                }
                            }
                        }
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
                            Stroke::new(2., theme::ACCENT),
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
                egui::Button::new(RichText::new(locale::tr(ui, "Enable help")).color(rule_guidance::GREEN))
                    .fill(rule_guidance::GREEN_DARK),
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
                            let menu = egui::ComboBox::from_id_salt("rule-outside")
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
                                menu.response.on_hover_text(locale::tr(ui, "Choose the IntGrid value used when inspecting a cell outside the layer bounds. The default skips rules that need to read outside the level."));
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

    fn paint_rule_cell(
        &mut self,
        ui: &egui::Ui,
        rect: Rect,
        source: &Value,
        value: i64,
        center: bool,
        hovered: bool,
    ) {
        let painter = ui.painter();
        let id = value.saturating_abs();
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
                    FontId::proportional(21.),
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    rect.shrink(3.),
                    1.,
                    Stroke::new(1., Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            ANY_VALUE => {}
            1_000.. => {
                let color = rule_value_group(source, id / 1_000 - 1)
                    .and_then(|group| group["color"].as_str())
                    .map(theme::color);
                if let Some(color) = color {
                    painter.rect_filled(rect, 2., color.gamma_multiply(0.9));
                }
                egui::Image::new(egui::include_image!("../../assets/icons/folderClose.svg"))
                    .paint_at(
                        ui,
                        Rect::from_center_size(rect.center(), rect.size() * 0.65),
                    );
                painter.rect_stroke(
                    rect.shrink(2.),
                    1.,
                    Stroke::new(
                        1.,
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
                    self.rule_value_tile(ui, rect.shrink(2.), value);
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
            painter.rect_stroke(
                rect.shrink(4.),
                1.,
                Stroke::new(2., Color32::from_rgb(192, 52, 37)),
                StrokeKind::Inside,
            );
        }
        let border = if hovered {
            theme::ACCENT
        } else if center {
            Color32::from_white_alpha(174)
        } else {
            theme::LIGHT.gamma_multiply(0.62)
        };
        painter.rect_stroke(rect, 2., Stroke::new(1., border), StrokeKind::Inside);
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
                        .stroke(Stroke::new(1., theme::LIGHT)),
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
        ctx.data_mut(|d| d.insert_temp(rule_options_id(), (layer, group_id, rule, options, true)));
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
        let mut close = false;
        let modal=egui::Modal::new(rule_options_id()).frame(project_dialogs::dialog_frame(16)).show(ctx, |ui| {
            ui.set_width(380.);
            match options {
                RuleOptions::Advanced|RuleOptions::RequireGroup => {
                    ui.label(locale::tr(ui, if options==RuleOptions::Advanced {
                        "Switch to advanced mode to edit these rules manually."
                    } else {"Removing the optional status clears this group's on/off state in all levels."}));
                    if source_controls::button(ui,"Confirm").clicked() {
                        let patch=if options==RuleOptions::Advanced {json!({"usesWizard":false})} else {json!({"isOptional":false})};
                        self.edit_recoverable("Rule group changed",|p|p.update_auto_rule_group(layer,gid,&patch));
                        close=true;
                    }
                }
                _ => {
                    let Some((id,rule))=rid.and_then(|id|Some((id,rule(&g,id)?))) else {return};
                    let properties:&[P] = match options {
                        RuleOptions::Offsets => &[
                            P("tileXOffset","Tile X offset",W::Int(-65536,65536)),P("tileYOffset","Tile Y offset",W::Int(-65536,65536)),
                            P("tileRandomXMin","Random X minimum",W::Int(-65536,65536)),P("tileRandomXMax","Random X maximum",W::Int(-65536,65536)),
                            P("tileRandomYMin","Random Y minimum",W::Int(-65536,65536)),P("tileRandomYMax","Random Y maximum",W::Int(-65536,65536))],
                        RuleOptions::Modulo => &[
                            P("xModulo","X modulo",W::Int(1,1024)),P("yModulo","Y modulo",W::Int(1,1024)),
                            P("xOffset","X offset",W::Int(-65536,65536)),P("yOffset","Y offset",W::Int(-65536,65536)),
                            P("checker","Checker mode",W::Choice(&["None","Horizontal","Vertical"]))],
                        _ => &[
                            P("perlinSeed","Perlin seed",W::Int(i32::MIN as i64,i32::MAX as i64)),
                            P("perlinScale","Perlin scale",W::Float(0.001,10.)),
                            P("perlinOctaves","Perlin octaves",W::Int(1,10))],
                    };
                    let patch=form(ui,rule,properties);
                    if changed(&patch) {self.edit("Rule changed",|p|p.update_auto_rule(layer,gid,id,&patch));}
                }
            }
            close |= source_controls::button(ui,if matches!(options,RuleOptions::Advanced|RuleOptions::RequireGroup) {"Cancel"} else {"Done"}).clicked();
        });
        if close || modal.should_close() {
            ctx.data_mut(|data| data.remove::<RuleOptionsState>(id));
        }
    }
}
