use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
struct Wizard {
    layer: i64,
    group: Option<i64>,
    name: String,
    main: i64,
    other: i64,
    break_on_match: bool,
    selected: Option<usize>,
    fragments: BTreeMap<usize, Vec<i64>>,
}
#[derive(Clone)]
struct Fragment {
    name: &'static str,
    x: usize,
    y: usize,
    pattern: [i64; 9],
}
fn flip(pattern: [i64; 9], x: bool, y: bool) -> [i64; 9] {
    std::array::from_fn(|i| {
        pattern[(if x { 2 - i % 3 } else { i % 3 }) + 3 * (if y { 2 - i / 3 } else { i / 3 })]
    })
}
fn fragments() -> Vec<Fragment> {
    let mut out = Vec::new();
    let mut add = |name, x, y, rows: [&str; 3], fx, fy| {
        let mut p = [0; 9];
        for (i, c) in rows.join("").chars().enumerate() {
            p[i] = match c {
                'o' => 1,
                'x' => -1,
                _ => 0,
            };
        }
        out.push(Fragment {
            name,
            x,
            y,
            pattern: flip(p, fx, fy),
        });
    };
    add("Single", 3, 1, ["-x-", "xox", "-x-"], false, false);
    add("Cross", 3, 2, ["xox", "ooo", "xox"], false, false);
    for (name, x, y, fx, fy) in [
        ("Turn NW", 2, 3, false, false),
        ("Turn NE", 3, 3, true, false),
        ("Turn SE", 3, 4, true, true),
        ("Turn SW", 2, 4, false, true),
    ] {
        add(name, x, y, ["-x-", "xoo", "-ox"], fx, fy);
    }
    add("Diagonal SW NE", 10, 2, ["xoo", "ooo", "oox"], false, false);
    add("Diagonal NW SE", 10, 3, ["oox", "ooo", "xoo"], false, false);
    for (name, x, y, rows, fx, fy) in [
        ("Corner NW to N", 6, 2, ["-ox", "xoo", "-oo"], false, false),
        ("Corner NW to W", 6, 0, ["-x-", "ooo", "xoo"], false, false),
        (
            "Corner NW to NW",
            10,
            0,
            ["xox", "ooo", "xoo"],
            false,
            false,
        ),
        ("Corner NE to N", 7, 2, ["-ox", "xoo", "-oo"], true, false),
        ("Corner NE to E", 7, 0, ["-x-", "ooo", "xoo"], true, false),
        ("Corner NE to NE", 11, 0, ["xox", "ooo", "xoo"], true, false),
        ("Corner SE to E", 7, 1, ["-x-", "ooo", "xoo"], true, true),
        ("Corner SE to S", 7, 3, ["-ox", "xoo", "-oo"], true, true),
        ("Corner SE to SE", 11, 1, ["xox", "ooo", "xoo"], true, true),
        ("Corner SW to S", 6, 3, ["-ox", "xoo", "-oo"], false, true),
        ("Corner SW to W", 6, 1, ["-x-", "ooo", "xoo"], false, true),
        ("Corner SW to SW", 10, 1, ["xox", "ooo", "xoo"], false, true),
        ("T cross N", 8, 3, ["xox", "ooo", "-x-"], false, false),
        ("T cross E", 8, 2, ["xo-", "oox", "xo-"], true, false),
        ("T cross S", 9, 2, ["xox", "ooo", "-x-"], false, true),
        ("T cross W", 9, 3, ["xo-", "oox", "xo-"], false, false),
        ("T wall W", 8, 0, ["xoo", "ooo", "xoo"], false, false),
        ("T wall E", 9, 0, ["xoo", "ooo", "xoo"], true, false),
        ("T wall S", 8, 1, ["xox", "ooo", "ooo"], false, true),
        ("T wall N", 9, 1, ["xox", "ooo", "ooo"], false, false),
        ("Horizontal W", 3, 0, ["-x-", "xo-", "-x-"], false, false),
        ("Horizontal E", 5, 0, ["-x-", "xo-", "-x-"], true, false),
        (
            "Horizontal middle",
            4,
            0,
            ["-x-", "-o-", "-x-"],
            false,
            false,
        ),
        ("Vertical N", 4, 1, ["-x-", "xox", "---"], false, false),
        ("Vertical S", 4, 3, ["-x-", "xox", "---"], false, true),
        ("Vertical middle", 4, 2, ["---", "xox", "---"], false, false),
        ("Outer corner NW", 0, 0, ["-x-", "xo-", "---"], false, false),
        ("Outer corner NE", 2, 0, ["-x-", "xo-", "---"], true, false),
        ("Outer corner SE", 2, 2, ["-x-", "xo-", "---"], true, true),
        ("Outer corner SW", 0, 2, ["-x-", "xo-", "---"], false, true),
        ("Wall N", 1, 0, ["-x-", "-o-", "---"], false, false),
        ("Wall E", 2, 1, ["---", "xo-", "---"], true, false),
        ("Wall S", 1, 2, ["-x-", "-o-", "---"], false, true),
        ("Wall W", 0, 1, ["---", "xo-", "---"], false, false),
        ("Inner corner NW", 0, 3, ["xo-", "oo-", "---"], false, false),
        ("Inner corner NE", 1, 3, ["xo-", "oo-", "---"], true, false),
        ("Inner corner SE", 1, 4, ["xo-", "oo-", "---"], true, true),
        ("Inner corner SW", 0, 4, ["xo-", "oo-", "---"], false, true),
        ("Full", 1, 1, ["---", "-o-", "---"], false, false),
    ] {
        add(name, x, y, rows, fx, fy);
    }
    out
}
fn alternative(
    all: &[Fragment],
    assigned: &BTreeMap<usize, Vec<i64>>,
    index: usize,
) -> Option<(usize, bool, bool)> {
    for (fx, fy) in [(true, false), (false, true), (true, true)] {
        let flipped = flip(all[index].pattern, fx, fy);
        if let Some((i, _)) = all
            .iter()
            .enumerate()
            .find(|(i, f)| *i != index && f.pattern == flipped && assigned.contains_key(i))
        {
            return Some((i, fx, fy));
        }
    }
    None
}
fn rules(w: &Wizard) -> Vec<Value> {
    let all = fragments();
    let mut result = Vec::new();
    for (i, fragment) in all.iter().enumerate() {
        let Some(ids) = w.fragments.get(&i).filter(|v| !v.is_empty()) else {
            continue;
        };
        let mut fx = false;
        let mut fy = false;
        for (j, _) in all.iter().enumerate() {
            if !w.fragments.contains_key(&j)
                && let Some((source, x, y)) = alternative(&all, &w.fragments, j)
                && source == i
            {
                fx |= x;
                fy |= y;
            }
        }
        let p = fragment.pattern.map(|v| match v {
            1 => w.main,
            -1 => {
                if w.other > 0 {
                    w.other
                } else {
                    -w.main
                }
            }
            _ => 0,
        });
        let (size, pattern) = if p.iter().enumerate().all(|(i, v)| i == 4 || *v == 0) {
            (1, vec![p[4]])
        } else {
            (3, p.to_vec())
        };
        result.push(json!({"size":size,"pattern":pattern,"tileRectsIds":ids.iter().map(|id|vec![*id]).collect::<Vec<_>>(),"tileMode":"Single","flipX":fx,"flipY":fy,"breakOnMatch":w.break_on_match,"outOfBoundsValue":w.main}));
    }
    result
}

fn wizard_group_name(values: &[Value], id: i64) -> String {
    values
        .iter()
        .find(|value| value["value"] == id)
        .and_then(|value| value["identifier"].as_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| format!("Rules for #{id}"), str::to_owned)
}

fn wizard_value(ui: &mut egui::Ui, values: &[Value], value: &mut i64, outer: bool) -> bool {
    let label = if outer {
        "Outer IntGrid value"
    } else {
        "Main IntGrid value"
    };
    ui.label(locale::tr(ui, label));
    ui.add_space(4.);
    let empty = locale::tr(
        ui,
        if outer {
            "< Any other IntGrid value >"
        } else {
            "< PICK A VALUE >"
        },
    );
    let (selected, fill) = values
        .iter()
        .find(|option| option["value"] == *value)
        .map_or_else(
            || (empty.to_owned(), theme::MEDIUM),
            |option| {
                let name = text(option, "identifier", locale::tr(ui, "Unnamed"));
                let color = theme::color(text(option, "color", "#000000"));
                (
                    format!("#{value}    {name}"),
                    color.lerp_to_gamma(Color32::BLACK, 0.4),
                )
            },
        );
    let (rect, button) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        0.,
        if button.hovered() {
            theme::ACCENT
        } else {
            fill
        },
    );
    let mut x = rect.left() + 8.;
    if let Some(option) = values.iter().find(|option| option["value"] == *value) {
        painter.rect_filled(
            Rect::from_min_size(rect.min + Vec2::splat(1.), Vec2::splat(30.)),
            0.,
            theme::color(text(option, "color", "#000000")),
        );
        x += 36.;
    } else {
        painter.rect_stroke(rect, 0., Stroke::new(1_f32, theme::LIGHT), StrokeKind::Inside);
    }
    painter.text(
        egui::pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        selected,
        FontId::proportional(if *value > 0 { 14.667 } else { 11. }),
        if button.hovered() {
            theme::DARK
        } else {
            Color32::WHITE
        },
    );
    let old = *value;
    egui::Popup::menu(&button.on_hover_cursor(egui::CursorIcon::PointingHand))
        .width(rect.width())
        .show(|ui| {
            if outer && ui.selectable_value(value, 0, empty).clicked() {
                ui.close();
            }
            for option in values {
                if let Some(id) = option["value"].as_i64() {
                    let name = text(option, "identifier", locale::tr(ui, "Unnamed"));
                    if ui
                        .selectable_value(value, id, format!("#{id}    {name}"))
                        .clicked()
                    {
                        ui.close();
                    }
                }
            }
        });
    old != *value
}

impl EditorApp {
    pub(super) fn begin_rule_wizard(&self, ctx: &egui::Context, layer: i64, group: Option<i64>) {
        let Some(def) = self.project.as_ref().and_then(|p| p.layer_def(layer)) else {
            return;
        };
        let mut w = Wizard {
            layer,
            group,
            name: String::new(),
            main: 0,
            other: 0,
            break_on_match: true,
            selected: None,
            fragments: BTreeMap::new(),
        };
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(def);
        if source["intGridValues"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && let Some(v) = source["intGridValues"][0]["value"].as_i64()
        {
            w.main = v;
            w.name = wizard_group_name(source["intGridValues"].as_array().unwrap(), v);
        }
        if let Some(group) = group.and_then(|uid| {
            def["autoRuleGroups"]
                .as_array()?
                .iter()
                .find(|g| g["uid"] == uid)
        }) {
            w.name = text(group, "name", "Auto rules").into();
            let source_rules = group["rules"].as_array().cloned().unwrap_or_default();
            w.main = source_rules
                .iter()
                .find_map(|r| {
                    let n = r["size"].as_u64()? as usize;
                    let v = r["pattern"][n * n / 2].as_i64()?;
                    (v > 0).then_some(v)
                })
                .unwrap_or(w.main);
            w.other = source_rules
                .iter()
                .flat_map(|r| r["pattern"].as_array().into_iter().flatten())
                .filter_map(Value::as_i64)
                .map(i64::abs)
                .find(|v| *v != w.main && *v > 0 && *v < 1000001)
                .unwrap_or(0);
            w.break_on_match = source_rules
                .first()
                .and_then(|r| r["breakOnMatch"].as_bool())
                .unwrap_or(true);
            let all = fragments();
            for r in source_rules {
                let raw = r["pattern"].as_array().cloned().unwrap_or_default();
                let mut p = [0; 9];
                if raw.len() == 1 {
                    p[4] = 1
                } else if raw.len() == 9 {
                    for (i, v) in raw.iter().enumerate() {
                        let v = v.as_i64().unwrap_or(0);
                        p[i] = if v == w.main {
                            1
                        } else if v == w.other && v > 0 || v == -w.main {
                            -1
                        } else {
                            0
                        };
                    }
                } else {
                    continue;
                }
                if let Some((i, _)) = all.iter().enumerate().find(|(_, f)| f.pattern == p) {
                    let ids = r["tileRectsIds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| r[0].as_i64())
                        .collect::<Vec<_>>();
                    if !ids.is_empty() {
                        w.fragments.insert(i, ids);
                    }
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("source-rule-wizard"), w));
    }
    pub(super) fn rule_wizard(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-rule-wizard");
        if let Some(state) = ctx.data_mut(|d| d.get_temp::<Wizard>(id)) {
            self.with_local_atlas_state(ctx, id.with((state.layer, state.group)), |app| {
                app.rule_wizard_inner(ctx)
            });
        }
    }

    fn wizard_fragment_grid(
        &mut self,
        ui: &mut egui::Ui,
        atlas_uid: Option<i64>,
        wizard: &mut Wizard,
        fragments: &[Fragment],
        cell: f32,
    ) {
        let stride = cell + 1.;
        let (grid, _) = ui.allocate_exact_size(
            egui::vec2(12. * stride + 1., 5. * stride + 1.),
            Sense::hover(),
        );
        egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .uv(Rect::from_min_size(egui::Pos2::ZERO, grid.size() / 8.))
            .paint_at(ui, grid);
        ui.painter()
            .rect_stroke(grid, 0., Stroke::new(1_f32, theme::LIGHT), StrokeKind::Inside);
        let atlas = self.rule_atlas(ui.ctx(), atlas_uid);
        let id = ui.make_persistent_id("wizard-fragments");
        for (i, fragment) in fragments.iter().enumerate() {
            let rect = Rect::from_min_size(
                grid.min
                    + egui::vec2(
                        fragment.x as f32 * stride + 1.,
                        fragment.y as f32 * stride + 1.,
                    ),
                Vec2::splat(cell),
            );
            let response = ui.interact(rect, id.with(i), Sense::click());
            let defined = wizard.fragments.get(&i);
            let mirrored = if defined.is_none() {
                alternative(fragments, &wizard.fragments, i)
            } else {
                None
            };
            let ids = defined.or_else(|| mirrored.and_then(|(j, _, _)| wizard.fragments.get(&j)));
            ui.painter().rect_filled(rect, 0., Color32::BLACK);
            if let Some(tile) = ids.and_then(|ids| ids.first())
                && let Some(atlas) = &atlas
            {
                atlas.paint_fragment(ui, rect, *tile, mirrored.map(|(_, x, y)| (x, y)));
            }
            let (x, y) = FRAGMENT_ICONS[i];
            let min = egui::pos2(x as f32 / 176., y as f32 / 208.);
            egui::Image::new(egui::include_image!("../../assets/icons/wallFragments.png"))
                .uv(Rect::from_min_size(min, egui::vec2(16. / 176., 16. / 208.)))
                .tint(Color32::from_white_alpha(if ids.is_some() {
                    28
                } else {
                    102
                }))
                .texture_options(egui::TextureOptions::NEAREST)
                .paint_at(ui, rect);
            if wizard.selected == Some(i) || defined.is_some() {
                let width = if wizard.selected == Some(i) { 3. } else { 1_f32 };
                ui.painter().rect_stroke(
                    rect,
                    0.,
                    Stroke::new(width, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            let clear = response.secondary_clicked() || response.middle_clicked();
            if response.clicked() || clear {
                wizard.selected = Some(i);
                if clear {
                    wizard.fragments.remove(&i);
                }
                if let Some(atlas) = atlas_uid {
                    let ids = wizard
                        .fragments
                        .get(&i)
                        .map_or_else(Vec::new, |ids| ids.iter().map(|id| *id as i32).collect());
                    if ids.is_empty() {
                        self.atlas_selection = Some((atlas, ids));
                    } else {
                        self.set_atlas_selection(atlas, ids);
                    }
                }
            }
            response.on_hover_text(fragment.name);
        }
        let important = Rect::from_min_size(grid.min, egui::vec2(6. * stride, grid.height()));
        ui.painter().rect_stroke(
            important,
            1.,
            Stroke::new(1_f32, source_controls::HELP_GREEN),
            StrokeKind::Inside,
        );
    }

    fn rule_wizard_inner(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-rule-wizard");
        let Some(mut w) = ctx.data_mut(|d| d.get_temp::<Wizard>(id)) else {
            return;
        };
        let Some(def) = self
            .project
            .as_ref()
            .and_then(|p| p.layer_def(w.layer))
            .cloned()
        else {
            ctx.data_mut(|d| d.remove::<Wizard>(id));
            return;
        };
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(&def)
            .clone();
        let atlas = def["tilesetDefUid"].as_i64();
        let all = fragments();
        let viewport = ctx.content_rect().size();
        let width = (viewport.x * 0.9).min((viewport.x - 48.).max(0.));
        let height = viewport.y * 0.72;
        let cell = if viewport.x < 900. { 16. } else { 32. };
        let left_width = 12. * (cell + 1.) + 1.;
        let mut apply = false;
        let mut cancel = false;
        let modal = egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(153))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                project_dialogs::dialog_title(
                    ui,
                    "AUTO-LAYER RULES ASSISTANT",
                    width + 40.,
                    theme::LIGHT,
                );
                egui::Frame::new()
                    .inner_margin(egui::Margin { left: 20, right: 20, top: 16, bottom: 24 })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.;
                            ui.allocate_ui_with_layout(
                                egui::vec2(left_width, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(left_width);
                                    egui::ScrollArea::vertical()
                                        .max_height(height)
                                        .show(ui, |ui| {
                                            ui.set_width(left_width);
                                            source_controls::form_heading(ui, "1. IntGrid values");
                                            ui.add_space(5.);
                                            let values = source["intGridValues"]
                                                .as_array()
                                                .map(Vec::as_slice)
                                                .unwrap_or_default();
                                            if wizard_value(ui, values, &mut w.main, false) {
                                                if w.main == w.other {
                                                    w.other = 0;
                                                }
                                                if w.group.is_none() {
                                                    w.name = wizard_group_name(values, w.main);
                                                }
                                            }
                                            ui.add_space(18.);
                                            wizard_value(ui, values, &mut w.other, true);
                                            source_controls::form_heading(ui, "2. Group of rules name");
                                            ui.add_space(2.);
                                            ui.add_sized(
                                                [189., 33.],
                                                egui::TextEdit::singleline(&mut w.name)
                                                    .margin(egui::Margin::symmetric(8, 7)),
                                            );
                                            source_controls::form_heading(ui, "3. Auto-rendered tiles");
                                            ui.add_space(3.);
                                            self.wizard_fragment_grid(ui, atlas, &mut w, &all, cell);
                                            source_controls::form_heading(ui, "4. Options");
                                            ui.add_space(6.);
                                            let mut stack = !w.break_on_match;
                                            ui.horizontal(|ui| {
                                                ui.label(locale::tr(ui, "Allow rules to stack"));
                                                source_controls::checkbox(ui, &mut stack, "");
                                            });
                                            w.break_on_match = !stack;
                                        });
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2((width - left_width - 8.).max(32.), height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    let Some(atlas) = atlas else {
                                        ui.colored_label(
                                            theme::ACCENT,
                                            locale::tr(ui, "Assign an output tileset in the layer settings first."),
                                        );
                                        return;
                                    };
                                    if w.selected.is_none() {
                                        self.atlas_selection = Some((atlas, Vec::new()));
                                    }
                                    let before = self.selected_tile_ids_for(atlas);
                                    self.atlas_rule_wizard_picker(ctx, ui, atlas);
                                    let after = self.selected_tile_ids_for(atlas);
                                    if let Some(i) = w.selected
                                        && before != after
                                    {
                                        if after.is_empty() {
                                            w.fragments.remove(&i);
                                        } else {
                                            w.fragments.insert(i, after.into_iter().map(i64::from).collect());
                                        }
                                    }
                                },
                            );
                        });
                        ui.add_space(17.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 1.;
                            let size = egui::vec2((width - 1.) * 0.5, 35.);
                            let action = if w.group.is_some() { "UPDATE RULES" } else { "CREATE RULES" };
                            apply = project_dialogs::button(ui, action, size, false, 13.333).clicked();
                            cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                        });
                    });
            });
        let mut open = !cancel && !modal.should_close();
        if apply && (w.main == 0 || w.name.trim().is_empty()) {
            self.status = locale::tr_ctx(
                ctx,
                if w.main == 0 {
                    "You need to pick an IntGrid value."
                } else {
                    "Name this group of rules."
                },
            )
            .into();
            self.status_error = true;
            apply = false;
        }
        if apply {
            let patches = rules(&w);
            if let Some(gid) = self.edit("Wizard rules generated", |p| {
                p.apply_auto_rule_wizard(w.layer, w.group, &w.name, &patches)
            }) {
                self.rule_group_uid = Some(gid);
                self.rule_uid = None;
                open = false;
            }
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, w));
        } else {
            ctx.data_mut(|d| d.remove::<Wizard>(id));
        }
    }
}

#[derive(Clone)]
struct GroupRemap {
    layer: i64,
    group: i64,
    original: String,
    name: Option<String>,
    remaps: BTreeMap<i64, i64>,
    offset: [i32; 2],
    tiles: Vec<i32>,
}
impl EditorApp {
    pub(super) fn begin_group_remap(&self, ctx: &egui::Context, layer: i64, group: i64) {
        let Some(project) = &self.project else {
            return;
        };
        let Ok(data) = project.copy_auto_rule(layer, group, None) else {
            return;
        };
        let mut remaps = BTreeMap::new();
        let mut tiles = std::collections::BTreeSet::new();
        for rule in data.value["rules"].as_array().into_iter().flatten() {
            for value in rule["pattern"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                let value = value.saturating_abs();
                if value > 0 && value != 1000001 {
                    remaps.insert(value, value);
                }
            }
            tiles.extend(
                rule["tileRectsIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|rect| rect.as_array().into_iter().flatten())
                    .filter_map(|id| i32::try_from(id.as_i64()?).ok()),
            );
        }
        let state = GroupRemap {
            layer,
            group,
            original: text(&data.value, "name", "Group").to_owned(),
            name: None,
            remaps,
            offset: [0, 0],
            tiles: tiles.into_iter().collect(),
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("source-group-remap"), state));
    }

    pub(super) fn group_remap_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-group-remap");
        let Some(mut state) = ctx.data(|d| d.get_temp::<GroupRemap>(id)) else {
            return;
        };
        let open = self.with_local_atlas_state(ctx, id.with((state.layer, state.group)), |app| {
            app.group_remap_dialog_inner(ctx, id, &mut state)
        });
        if open {
            ctx.data_mut(|d| d.insert_temp(id, state));
        } else {
            ctx.data_mut(|d| d.remove::<GroupRemap>(id));
        }
    }

    fn group_remap_dialog_inner(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        state: &mut GroupRemap,
    ) -> bool {
        let Some(def) = self.project.as_ref().and_then(|p| p.layer_def(state.layer)) else {
            return false;
        };
        let atlas = def["tilesetDefUid"].as_i64();
        let columns = atlas
            .and_then(|uid| self.project.as_ref()?.tileset(uid))
            .map_or(0, |ts| number(ts, "__cWid", 0.) as i32);
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(def);
        let values = source["intGridValues"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let view = ctx.content_rect().size();
        let width = (view.x - 64.).clamp(300., 916.);
        let left = (width * 0.4).min(300.);
        let height = view.y * 0.6;
        let mut choose_name = false;
        let mut cancel = false;
        let default_name = locale::format_ctx(ctx, "{name} copy", &[("name", &state.original)]);
        egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(153))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let title = locale::format(ui, "Duplicate group: \"{name}\"", &[("name", &state.original)]);
                project_dialogs::dialog_title(ui, &title.to_uppercase(), width + 40., theme::LIGHT);
                egui::Frame::new()
                    .inner_margin(egui::Margin { left: 20, right: 20, top: 16, bottom: 24 })
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 16.;
                            ui.allocate_ui_with_layout(
                                egui::vec2(left, height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(left);
                                    source_controls::form_heading_line(ui, "Remap IntGrid values");
                                    ui.add_space(8.);
                                    source_controls::help_card(ui, left, 62., |ui| {
                                        source_controls::help_paragraph(ui, "You can remap any IntGrid value used in this group of rules.");
                                    });
                                    ui.add_space(8.);
                                    egui::ScrollArea::vertical().max_height((height - 90.).max(32.)).show(ui, |ui| {
                                        ui.spacing_mut().item_spacing.y = 2.;
                                        for (&old, new) in &mut state.remaps {
                                            remap_value_row(ui, &values, old, new, left);
                                        }
                                    });
                                },
                            );
                            let atlas_size = egui::vec2(width - left - 16., height);
                            ui.allocate_ui_with_layout(
                                atlas_size,
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(atlas_size.x);
                                    source_controls::form_heading_line(ui, "Transpose tiles");
                                    ui.add_space(8.);
                                    ui.allocate_ui_with_layout(atlas_size, egui::Layout::top_down(egui::Align::Min), |ui| {
                                        if let Some(atlas) = atlas
                                            && self.atlas_rule_remap_picker(ctx, ui, atlas, &state.tiles, state.offset)
                                            && let Some(&first) = state.tiles.first()
                                            && columns > 0
                                        {
                                            state.offset = [
                                                self.tile_id % columns - first % columns,
                                                self.tile_id / columns - first / columns,
                                            ];
                                        }
                                    });
                                },
                            );
                        });
                        let valid = self.project.as_ref().is_some_and(|project| {
                            project.remap_rule_group_clipboard(
                                state.layer, state.group, &default_name, &state.remaps, state.offset,
                            ).is_ok()
                        });
                        ui.add_space(16.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 1.;
                            let size = egui::vec2((width - 1.) * 0.5, 35.);
                            choose_name = ui.add_enabled_ui(valid, |ui| {
                                project_dialogs::button(ui, "CONFIRM", size, false, 13.333)
                            }).inner.clicked();
                            cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                        });
                    });
            });
        if cancel {
            return false;
        }
        if choose_name {
            state.name = Some(default_name);
        }
        if let Some(name) = &mut state.name {
            match remap_name_dialog(ctx, id.with("name"), name, choose_name) {
                Some(false) => state.name = None,
                Some(true) => {
                    if let Some(group) = self.edit("Rule group duplicated and remapped", |p| {
                        p.duplicate_rule_group_remapped(
                            state.layer,
                            state.group,
                            name,
                            &state.remaps,
                            state.offset,
                        )
                    }) {
                        self.rule_group_uid = Some(group);
                        self.rule_uid = None;
                        return false;
                    }
                }
                None => {}
            }
        }
        true
    }
}

fn remap_value_row(ui: &mut egui::Ui, values: &[Value], old: i64, new: &mut i64, width: f32) {
    let describe = |id| {
        let value = values.iter().find(|value| value["value"] == id);
        let name = value.and_then(|v| v["identifier"].as_str());
        let label = name.map_or_else(|| format!("#{id}"), |name| format!("{name} ({id})"));
        let color = value
            .and_then(|v| v["color"].as_str())
            .map_or(theme::MEDIUM, theme::color);
        (label, color)
    };
    let (old_label, old_color) = describe(old);
    let (new_label, new_color) = if *new == old {
        (locale::tr(ui, "No change").to_owned(), theme::MEDIUM)
    } else {
        describe(*new)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width * 0.44, 34.), Sense::hover());
        ui.painter().rect_filled(rect, 2., old_color);
        ui.painter().text(
            rect.left_center() + egui::vec2(8., 0.),
            Align2::LEFT_CENTER,
            old_label,
            FontId::proportional(14.667),
            Color32::WHITE,
        );
        let (arrow, _) = ui.allocate_exact_size(egui::vec2(18., 34.), Sense::hover());
        egui::Image::new(egui::include_image!("../../assets/icons/arrow_right.svg"))
            .paint_at(ui, Rect::from_center_size(arrow.center(), Vec2::splat(16.)));
        let mut label = RichText::new(new_label);
        if *new == old {
            label = label.italics();
        }
        let button = ui.add_sized(
            [width * 0.56 - 18., 34.],
            egui::Button::new(label).fill(new_color),
        );
        egui::Popup::menu(&button).show(|ui| {
            for entry in values {
                if let Some(value) = entry["value"].as_i64() {
                    let (label, _) = describe(value);
                    if ui.selectable_value(new, value, label).clicked() {
                        ui.close();
                    }
                }
            }
        });
    });
}

fn remap_name_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    name: &mut String,
    opening: bool,
) -> Option<bool> {
    let mut validate = false;
    let mut cancel = false;
    let modal = egui::Modal::new(id)
        .frame(project_dialogs::dialog_frame(20))
        .backdrop_color(Color32::from_black_alpha(140))
        .show(ctx, |ui| {
            ui.set_width(360.);
            ui.add_space(12.);
            ui.label(locale::tr(ui, "Name this new group"));
            ui.add_space(8.);
            let input = ui.add_sized(
                [360., 33.],
                egui::TextEdit::singleline(name).margin(egui::Margin::symmetric(8, 7)),
            );
            if opening {
                input.request_focus();
                source_controls::select_text_input(ctx, input.id, name);
            }
            let valid = !name.trim().is_empty();
            if !valid {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    locale::tr(ui, "Please enter a valid name"),
                );
            }
            ui.add_space(if valid { 28. } else { 8. });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.;
                let size = egui::vec2(179.5, 35.);
                validate = ui
                    .add_enabled_ui(valid, |ui| {
                        project_dialogs::button(ui, "VALIDATE", size, false, 13.333)
                    })
                    .inner
                    .clicked();
                cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
            });
            validate |= valid
                && (input.has_focus() || input.lost_focus())
                && ui.input(|i| i.key_pressed(Key::Enter));
        });
    if validate {
        Some(true)
    } else if cancel || modal.should_close() {
        Some(false)
    } else {
        None
    }
}

const FRAGMENT_ICONS: [(u16, u16); 47] = [
    (32, 48),
    (32, 64),
    (48, 0),
    (64, 0),
    (64, 16),
    (48, 16),
    (64, 32),
    (64, 48),
    (80, 32),
    (80, 0),
    (144, 0),
    (96, 32),
    (96, 0),
    (160, 0),
    (96, 16),
    (96, 48),
    (160, 16),
    (80, 48),
    (80, 16),
    (144, 16),
    (112, 48),
    (112, 32),
    (128, 32),
    (128, 48),
    (112, 0),
    (128, 0),
    (112, 16),
    (128, 16),
    (64, 64),
    (96, 64),
    (80, 64),
    (48, 32),
    (48, 64),
    (48, 48),
    (0, 0),
    (32, 0),
    (32, 32),
    (0, 32),
    (16, 0),
    (32, 16),
    (16, 32),
    (0, 16),
    (0, 48),
    (16, 48),
    (16, 64),
    (0, 64),
    (16, 16),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_fragment_order_patterns_and_mirrored_rules() {
        let f = fragments();
        assert_eq!(f.len(), 47);
        assert_eq!(f[0].name, "Single");
        assert_eq!(f[46].name, "Full");
        let mut w = Wizard {
            layer: 1,
            group: None,
            name: "Walls".into(),
            main: 2,
            other: 0,
            break_on_match: true,
            selected: None,
            fragments: BTreeMap::new(),
        };
        w.fragments.insert(42, vec![10, 11]);
        let r = rules(&w);
        assert_eq!(r[0]["pattern"], json!([-2, 2, 0, 2, 2, 0, 0, 0, 0]));
        assert_eq!(r[0]["flipX"], true);
        assert_eq!(r[0]["flipY"], true);
        assert_eq!(r[0]["tileRectsIds"], json!([[10], [11]]));
        w.fragments.clear();
        w.fragments.insert(46, vec![0]);
        assert_eq!(rules(&w)[0]["pattern"], json!([2]));
        assert_eq!(rules(&w)[0]["size"], 1);
    }
}
