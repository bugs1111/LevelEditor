use super::*;
use crate::app::panels::rule_detail::{
    RuleAtlas, RuleCellStyle, RuleOptions, rule_has_unknown_conditions,
};

const RULE_CONTENT_HEIGHT: f32 = 54.;
const GROUP_HEIGHT: f32 = 34.;

fn toggle(ui: &mut egui::Ui, enabled: bool, optional: bool, tip: &str, size: Vec2) -> egui::Response {
    let (symbol, tint) = match (optional, enabled) {
        (true, true) => (
            egui::include_image!("../../assets/icons/visible.png"),
            Color32::WHITE,
        ),
        (true, false) => (
            egui::include_image!("../../assets/icons/hidden.png"),
            Color32::WHITE,
        ),
        (false, true) => (
            egui::include_image!("../../assets/icons/toggleOn.svg"),
            Color32::from_rgb(157, 207, 96),
        ),
        (false, false) => (
            egui::include_image!("../../assets/icons/toggleOff.svg"),
            Color32::from_rgb(155, 112, 101),
        ),
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if optional && response.hovered() {
        ui.painter()
            .rect_filled(rect, 0., Color32::from_white_alpha(87));
    }
    egui::Image::new(symbol)
        .tint(tint)
        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.)));
    response.on_hover_text(locale::tr(ui, tip))
}

fn rule_drag(ui: &mut egui::Ui, width: f32, height: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::drag());
    ui.painter().rect_filled(
        rect,
        0.,
        if response.hovered() {
            theme::ACCENT
        } else {
            Color32::TRANSPARENT
        },
    );
    egui::Image::new(egui::include_image!("../../assets/icons/drag.svg"))
        .tint(if response.hovered() {
            theme::DARK
        } else {
            Color32::from_white_alpha(192)
        })
        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(14.)));
    response.on_hover_and_drag_cursor(egui::CursorIcon::Grab)
}

fn compact_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    size: [f32; 2],
    fill: Color32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(3., 2.);
        source_controls::add_sized_button(
            ui,
            size,
            egui::Button::new(label).fill(fill).corner_radius(2.),
        )
    })
    .inner
}

fn rule_icon_button(
    ui: &mut egui::Ui,
    image: egui::ImageSource<'static>,
    size: [f32; 2],
    fill: Color32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(3., 2.);
        source_controls::add_sized_button(
            ui,
            size,
            egui::Button::image(egui::Image::new(image).fit_to_exact_size(Vec2::splat(16.)))
                .fill(fill)
                .corner_radius(2.),
        )
    })
    .inner
}

fn rule_popup_frame(ui: &egui::Ui) -> egui::Frame {
    source_controls::context_menu_frame(ui.style())
}

fn rule_menu_row(
    ui: &mut egui::Ui,
    label: &str,
    detail: &str,
    icon: Option<egui::ImageSource<'static>>,
    has_tile: bool,
) -> egui::Response {
    let row_height = if icon.is_some() || has_tile { 40. } else { 32. };
    let height = row_height + 14. * detail.lines().count() as f32;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 0., theme::ACCENT);
    }
    painter.hline(rect.x_range(), rect.max.y, Stroke::new(1_f32, theme::MEDIUM));
    if let Some(image) = &icon {
        let icon_rect = rule_menu_icon_rect(rect);
        let icon_rect = if detail.is_empty() {
            icon_rect
        } else {
            icon_rect.translate(egui::vec2(0., rect.min.y + 15. - rect.center().y))
        };
        egui::Image::new(image.clone())
            .tint(theme::TEXT)
            .paint_at(ui, icon_rect);
    }
    let x = rect.min.x
        + match (icon.is_some(), has_tile) {
            (true, true) => 70.,
            (false, false) => 20.,
            _ => 50.,
        };
    painter.text(
        egui::pos2(
            x,
            if detail.is_empty() {
                rect.center().y - 7.
            } else {
                rect.min.y + 7.
            },
        ),
        Align2::LEFT_TOP,
        label.to_uppercase(),
        FontId::proportional(13.),
        if response.hovered() {
            theme::DARK
        } else {
            theme::ACCENT
        },
    );
    if !detail.is_empty() {
        painter.text(
            egui::pos2(rect.min.x + 20., rect.min.y + 22.),
            Align2::LEFT_TOP,
            detail,
            FontId::proportional(13.),
            if response.hovered() {
                theme::DARK
            } else {
                Color32::from_rgb(115, 136, 177)
            },
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn rule_menu_icon_rect(rect: Rect) -> Rect {
    Rect::from_center_size(rect.left_center() + egui::vec2(31., 0.), Vec2::splat(18.))
}

fn rule_menu_separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.), Sense::hover());
    ui.painter()
        .rect_filled(rect, 0., Color32::from_black_alpha(85));
}

fn layer_biome_enum<'a>(project: &'a Project, layer: &Value) -> Option<&'a Value> {
    let field_uid = layer["biomeFieldUid"].as_i64()?;
    let field = project.json["defs"]["levelFields"]
        .as_array()?
        .iter()
        .find(|field| field["uid"] == field_uid)?;
    let enum_uid = field["type"].as_str().and_then(|kind| {
        kind.strip_prefix("F_Enum(")?
            .strip_suffix(')')?
            .parse::<i64>()
            .ok()
    });
    let enum_name = text(field, "__type", "")
        .trim_end_matches('>')
        .split_once("Enum.")
        .map(|(_, name)| name);
    ["enums", "externalEnums"]
        .iter()
        .filter_map(|&key| project.json["defs"][key].as_array())
        .flatten()
        .find(|value| match enum_uid {
            Some(uid) => value["uid"] == uid,
            None => enum_name.is_some_and(|name| value["identifier"] == name),
        })
}

struct RulesView {
    uid: i64,
    key: LevelKey,
    index: usize,
    project: String,
    def: Value,
    instance: Value,
    source: Value,
    atlas: Option<RuleAtlas>,
    biome: Option<Value>,
}
impl RulesView {
    fn group_key(&self, group: i64) -> egui::Id {
        egui::Id::new(("rule-group-open", &self.project, self.uid, group))
    }

    fn insert_hover_id(&self) -> egui::Id {
        egui::Id::new(("rule-insert-hover", &self.project, self.uid))
    }
}

struct RuleInsertion {
    group: i64,
    rule: i64,
    index: usize,
    row: Rect,
}

impl RuleInsertion {
    fn buttons(&self) -> [Rect; 2] {
        [self.row.top() - 8., self.row.bottom() - 10.].map(|y| {
            Rect::from_min_size(egui::pos2(self.row.right() - 20., y), Vec2::splat(18.))
        })
    }
}

impl EditorApp {
    pub(in crate::app::panels) fn rule_list(&mut self, ui: &mut egui::Ui, height: f32) {
        self.rule_hover = None;
        let (Some(key), Some(uid), Some(project)) =
            (self.selected, self.definition_uid, self.project.as_ref())
        else {
            return;
        };
        let Some(def) = project.layer_def(uid).cloned() else {
            return;
        };
        let Some((index, instance)) = project
            .level(key)
            .and_then(|level| level["layerInstances"].as_array())
            .and_then(|layers| {
                layers
                    .iter()
                    .enumerate()
                    .find(|(_, li)| li["layerDefUid"] == uid)
            })
        else {
            return;
        };
        let source = if def["type"] == "IntGrid" {
            Some(&def)
        } else {
            def["autoSourceLayerDefUid"]
                .as_i64()
                .and_then(|id| project.layer_def(id))
        };
        let atlas_uid = instance["overrideTilesetUid"]
            .as_i64()
            .or(def["tilesetDefUid"].as_i64());
        let ready = source.is_some()
            && def["tilesetDefUid"]
                .as_i64()
                .and_then(|id| project.tileset(id))
                .is_some();
        let mut view = RulesView {
            uid,
            key,
            index,
            project: text(&project.json, "iid", "").to_owned(),
            instance: instance.clone(),
            source: source.cloned().unwrap_or(Value::Null),
            biome: layer_biome_enum(project, &def).cloned(),
            def,
            atlas: None,
        };
        view.atlas = self.rule_atlas(ui.ctx(), atlas_uid);
        let body_bottom = ui.cursor().min.y + height;
        self.rule_list_header(ui, &view, atlas_uid, ready);
        ui.add_space(9.);
        let list_height = (body_bottom - ui.cursor().min.y - 2.).max(40.);
        let groups = view.def["autoRuleGroups"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !groups
            .iter()
            .any(|group| group["uid"].as_i64() == self.rule_group_uid)
        {
            self.rule_group_uid = groups.first().and_then(|g| g["uid"].as_i64());
            self.rule_uid = None;
        }
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .show(ui, |ui| {
                ui.spacing_mut().scroll = source_controls::scroll_style();
                let scroll = egui::ScrollArea::vertical()
                    .id_salt(("rule-list", uid))
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.;
                        if !ready {
                            ui.colored_label(
                                Color32::LIGHT_RED,
                                locale::tr(ui, "The current layer settings prevent its rules to work."),
                            );
                            if source_controls::button(ui, "Open layer settings").clicked() {
                                self.show_panel(Some(Panel::Layers));
                            }
                            for group in groups {
                                ui.label(format!(
                                    "{}   {} rule(s)",
                                    text(group, "name", "Group"),
                                    group["rules"].as_array().map_or(0, Vec::len)
                                ));
                            }
                            return false;
                        }
                        let mut insertion = None;
                        for (index, group) in groups.iter().enumerate() {
                            self.rule_group_row(ui, &view, group, index, &mut insertion);
                        }
                        let insertion_id = view.insert_hover_id();
                        if let Some(insertion) = insertion {
                            ui.data_mut(|data| data.insert_temp(insertion_id, insertion.rule));
                            for (index, (rect, tip)) in insertion
                                .buttons()
                                .into_iter()
                                .zip(["Create new rule before", "Create new rule after"])
                                .enumerate()
                            {
                                let response = ui.interact(
                                    rect,
                                    insertion_id.with((insertion.rule, index)),
                                    Sense::click(),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, locale::tr(ui, tip))
                                });
                                let opacity = if response.hovered() { 1. } else { 0.6 };
                                ui.painter()
                                    .rect_filled(rect, 0., SOURCE_ORANGE.gamma_multiply(opacity));
                                egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                                    .tint(Color32::from_white_alpha((255. * opacity) as u8))
                                    .paint_at(ui, rect.shrink(1.));
                                if response.on_hover_text(locale::tr(ui, tip)).clicked() {
                                    self.insert_rule(view.uid, insertion.group, insertion.index + index);
                                }
                            }
                        } else {
                            ui.data_mut(|data| data.remove::<i64>(insertion_id));
                        }
                        let mut background = ui.available_rect_before_wrap();
                        background.max.y = ui.clip_rect().max.y;
                        if !background.is_positive() {
                            return false;
                        }
                        let response = ui.interact(
                            background,
                            ui.id().with("rule-list-background"),
                            Sense::click(),
                        );
                        egui::Popup::context_menu(&response)
                            .gap(10.)
                            .frame(rule_popup_frame(ui))
                            .show(|ui| self.rule_list_paste_menu(ui, uid));
                        response.context_menu_opened()
                    });
                if scroll.inner {
                    let rect = scroll.inner_rect;
                    ui.painter().extend(egui::Shape::dashed_line(
                        &[
                            rect.left_top(),
                            rect.right_top(),
                            rect.right_bottom(),
                            rect.left_bottom(),
                            rect.left_top(),
                        ],
                        Stroke::new(1_f32, theme::ACCENT),
                        3.,
                        2.,
                    ));
                }
            });
    }

    fn rule_list_header(
        &mut self,
        ui: &mut egui::Ui,
        view: &RulesView,
        atlas: Option<i64>,
        ready: bool,
    ) {
        let width = ui.available_width();
        ui.add_enabled_ui(ready, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.;
                let create = ui
                    .scope(|ui| {
                        ui.spacing_mut().button_padding = Vec2::splat(3.);
                        source_controls::add_sized_button(
                            ui,
                            [(width - 174.).max(68.), 40.],
                            egui::Button::image_and_text(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/icons/add.svg"
                                ))
                                .fit_to_exact_size(Vec2::splat(16.)),
                                RichText::new(locale::tr(ui, "GROUP")).size(12.).color(Color32::WHITE),
                            )
                            .fill(SOURCE_ORANGE),
                        )
                    })
                    .inner;
                egui::Popup::menu(&create).show(|ui| {
                    if ui.button(locale::tr(ui, "Use assistant (recommended)")).clicked() {
                        self.begin_rule_wizard(ui.ctx(), view.uid, None);
                        ui.close();
                    }
                    if ui.button(locale::tr(ui, "Create an empty group")).clicked() {
                        if let Some(gid) = self.edit("Rule group created", |p| {
                            p.edit_document(|p| {
                                let group = p.add_auto_rule_group(view.uid, "New group")?;
                                p.reorder_auto_rule_group(view.uid, group, 0)?;
                                Ok(group)
                            })
                        }) {
                            self.open_rule_groups.insert((view.uid, gid));
                            ui.data_mut(|data| data.insert_temp(rename_group_id(), (view.uid, gid)));
                            self.rule_group_uid = Some(gid);
                            self.rule_uid = None;
                        }
                        ui.close();
                    }
                });
                if compact_button(
                    ui,
                    RichText::new(locale::tr(ui, "RANDOMIZE")).size(12.).color(Color32::WHITE),
                    [94., 40.],
                    SOURCE_ORANGE,
                )
                .on_hover_text(locale::tr(ui, "Change the random seed of the current layer."))
                .clicked()
                {
                    self.edit_level("Layer seed randomized", view.key, |p| {
                        p.randomize_auto_layer(view.key, view.index)
                    });
                }
                egui::Frame::new()
                    .fill(theme::DARK)
                    .corner_radius(2.)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().interact_size.y = 36.;
                        ui.spacing_mut().icon_width = 24.;
                        ui.spacing_mut().icon_spacing = 3.;
                        ui.style_mut().override_font_id = Some(FontId::proportional(12.));
                        ui.visuals_mut().override_text_color =
                            Some(Color32::from_rgb(120, 131, 157));
                        source_controls::checkbox(ui, &mut self.render_auto, "Render")
                            .on_hover_text(locale::tr(ui, "Show or hide generated auto-layer tiles (Shift-R)."));
                    });
            });
            ui.add_space(2.);
            let Some(project) = self.project.as_ref() else {
                return;
            };
            let tilesets = project.json["defs"]["tilesets"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let current = atlas.and_then(|uid| project.tileset(uid));
            let default = locale::tr(ui, " (DEFAULT)");
            let fallback = locale::tr(ui, "Tileset");
            let name = |d: &Value| {
                let suffix = if d["uid"] == view.def["tilesetDefUid"] {
                    default
                } else {
                    ""
                };
                format!("{}{suffix}", text(d, "identifier", fallback))
            };
            let compatible = |d: &Value| {
                current.is_some_and(|reference| {
                    ["__cWid", "__cHei", "tileGridSize"]
                        .iter()
                        .all(|&key| reference[key] == d[key])
                })
            };
            let mut sorted = tilesets.iter().collect::<Vec<_>>();
            sorted.sort_by_key(|d| (!compatible(d), d["uid"].as_i64().unwrap_or(-1)));
            let mut picked = atlas;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                ui.add_space(1.);
                ui.spacing_mut().interact_size.y = 29.;
                ui.spacing_mut().button_padding = egui::vec2(10., 6.);
                let widgets = &mut ui.visuals_mut().widgets;
                for style in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.open] {
                    style.weak_bg_fill = theme::LIGHT;
                    style.bg_stroke = Stroke::new(1_f32, Color32::from_rgb(115, 127, 157));
                    style.corner_radius = egui::CornerRadius::same(3);
                    style.expansion = 0.;
                }
                let selected = current
                    .map(name)
                    .unwrap_or_else(|| locale::tr(ui, "-- Select a tileset --").into());
                let picker = egui::ComboBox::from_id_salt(("rule-instance-tileset", view.uid))
                    .width(width - 2.)
                    .selected_text(RichText::new(selected).size(13.333).color(Color32::WHITE))
                    .icon(|ui, rect, _, _| {
                        let center = rect.center();
                        ui.painter().add(egui::Shape::line(
                            vec![
                                center + egui::vec2(-4., -2.),
                                center + egui::vec2(0., 2.),
                                center + egui::vec2(4., -2.),
                            ],
                            Stroke::new(1.5_f32, Color32::WHITE),
                        ));
                    })
                    .show_ui(ui, |ui| {
                        for d in sorted {
                            if compatible(d) {
                                ui.selectable_value(&mut picked, d["uid"].as_i64(), name(d));
                            } else {
                                ui.add_enabled(
                                    false,
                                    egui::Button::selectable(
                                        false,
                                        locale::format(
                                            ui,
                                            "{name} (INCOMPATIBLE SIZE)",
                                            &[("name", &name(d))],
                                        ),
                                    ),
                                );
                            }
                        }
                    });
                let bounds = picker.response.rect;
                ui.painter().rect_filled(
                    Rect::from_min_size(bounds.left_bottom(), egui::vec2(bounds.width(), 2.)),
                    egui::CornerRadius { sw: 2, se: 2, ..Default::default() },
                    Color32::from_black_alpha(77),
                );
            });
            if picked != atlas {
                self.edit_level("Layer instance tileset changed", view.key, |p| {
                    p.set_layer_tileset_override(
                        view.key,
                        view.index,
                        picked,
                        ldtk_core::TilesetRemap::TileIds,
                        false,
                    )
                    .map(|_| ())
                });
            }
        });
    }

    fn rule_group_row(
        &mut self,
        ui: &mut egui::Ui,
        view: &RulesView,
        group: &Value,
        index: usize,
        insertion: &mut Option<RuleInsertion>,
    ) {
        let Some(gid) = group["uid"].as_i64() else {
            return;
        };
        let group_key = (view.uid, gid);
        let mut open = self.open_rule_groups.contains(&group_key);
        let optional = group["isOptional"] == true;
        let active = if optional {
            view.instance["optionalRules"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(gid)))
        } else {
            group["active"] != false
        };
        let tint = group["color"].as_str().map(theme::color);
        let color = tint.map_or(theme::LIGHT, |c| theme::MEDIUM.lerp_to_gamma(c, 0.7));
        let edit_mode =
            ui.data(|data| data.get_temp::<(i64, i64)>(rename_group_id())) == Some((view.uid, gid));
        let mut rename = false;
        let mut menu_requested = false;
        let mut add = false;
        let biome_id = view.group_key(gid).with("biome-menu");
        let required = group["requiredBiomeValues"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let biome_width = if view.def["biomeFieldUid"].is_null() {
            0.
        } else {
            (required.len() as f32 * 25. - 4.).clamp(24., 108.)
        };
        let mut biome_anchor = None;
        let target = ClipTarget::Rule {
            layer: view.uid,
            group: gid,
            rule: None,
        };
        let header = egui::Frame::new()
            .fill(if active {
                color
            } else {
                Color32::from_rgb(39, 25, 30)
            })
            .show(ui, |ui| {
                if !active {
                    let rect = Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), GROUP_HEIGHT),
                    );
                    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                        .tint(Color32::from_white_alpha(89))
                        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                        .paint_at(ui, rect);
                    ui.multiply_opacity(0.35);
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    rule_drag(ui, 28., GROUP_HEIGHT).dnd_set_drag_payload((view.uid, gid));
                    let toggle_tip = if optional {
                        "Toggle this optional group in this level"
                    } else {
                        "Toggle rule group"
                    };
                    if toggle(ui, active, optional, toggle_tip, egui::vec2(35., GROUP_HEIGHT))
                        .clicked()
                    {
                        if optional {
                            self.edit_level("Optional rule group toggled", view.key, |p| {
                                let li = p.layer_mut(view.key, view.index).unwrap();
                                let options =
                                    li["optionalRules"].as_array().cloned().unwrap_or_default();
                                let mut options: Vec<_> =
                                    options.into_iter().filter(|v| *v != gid).collect();
                                if !active {
                                    options.push(json!(gid));
                                }
                                li["optionalRules"] = json!(options);
                                p.rebuild_auto_layer(view.key, view.uid)
                            });
                        } else {
                            self.edit("Rule group toggled", |p| {
                                p.update_auto_rule_group(view.uid, gid, &json!({"active":!active}))
                            });
                        }
                    }
                    let width = (ui.available_width() - 54. - biome_width).max(32.);
                    if edit_mode {
                        let mut done = false;
                        let name = super::source_controls::edit_text(
                            ui,
                            (view.uid, gid, "rename"),
                            text(group, "name", "Group"),
                            |ui, draft, id| {
                                let focus = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                                if !focus {
                                    ui.memory_mut(|m| m.request_focus(id));
                                    ui.data_mut(|d| d.insert_temp(id, true));
                                }
                                let response = ui.add_sized(
                                    [width, GROUP_HEIGHT],
                                    egui::TextEdit::singleline(draft).id(id),
                                );
                                done = response.lost_focus()
                                    || ui.input(|i| {
                                        i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)
                                    });
                                if done {
                                    ui.data_mut(|d| d.remove::<bool>(id));
                                }
                                response
                            },
                        );
                        if done {
                            ui.data_mut(|d| d.remove::<(i64, i64)>(rename_group_id()));
                        }
                        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                            self.edit("Rule group renamed", |p| {
                                p.update_auto_rule_group(view.uid, gid, &json!({"name":name}))
                            });
                        }
                    } else {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(width, GROUP_HEIGHT), Sense::click());
                        if response.hovered() {
                            if !optional {
                                ui.painter()
                                    .rect_filled(rect, 0., Color32::from_white_alpha(38));
                            }
                            if active {
                                self.rule_hover = Some((view.uid, gid, None));
                            }
                        }
                        let mut text_x = rect.min.x + 8.;
                        if let Some(atlas) = &view.atlas
                            && group["icon"].is_object()
                        {
                            atlas.paint_rect(
                                ui,
                                Rect::from_center_size(
                                    rect.left_center() + egui::vec2(19., 0.),
                                    Vec2::splat(28.),
                                ),
                                &group["icon"],
                            );
                            text_x += 32.;
                        }
                        let name = RichText::new(text(group, "name", "Group")).size(14.667);
                        let name = if active {
                            name.family(egui::FontFamily::Name("body-bold".into()))
                        } else {
                            name.italics()
                        };
                        let text_color = if active || response.hovered() {
                            Color32::WHITE
                        } else {
                            SOURCE_ORANGE
                        };
                        let galley = egui::WidgetText::from(name.color(text_color)).into_galley(
                            ui,
                            Some(egui::TextWrapMode::Truncate),
                            (rect.right() - text_x).max(0.),
                            egui::TextStyle::Body,
                        );
                        ui.painter_at(rect).galley(
                            egui::pos2(text_x, rect.center().y - galley.size().y * 0.5),
                            galley,
                            text_color,
                        );
                        if response.clicked() {
                            open = !open;
                            self.rule_group_uid = Some(gid);
                            self.rule_uid = None;
                            self.clipboard_focus(ui.ctx(), target.clone());
                        }
                        egui::Popup::context_menu(&response)
                            .gap(10.)
                            .frame(rule_popup_frame(ui))
                            .show(|ui| {
                                self.rule_group_menu(ui, view, group, gid, &target, &mut rename)
                            });
                    }
                    if biome_width > 0. {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(biome_width, GROUP_HEIGHT),
                            Sense::click(),
                        );
                        self.paint_group_biomes(ui, view, group, rect, required);
                        if response.on_hover_text(locale::tr(ui, "Assign biome enum")).clicked() {
                            let frame = ui.ctx().cumulative_frame_nr();
                            ui.data_mut(|data| data.insert_temp(biome_id, frame));
                        }
                        biome_anchor = Some(rect);
                    }
                    if group["usesWizard"] == true {
                        if compact_button(ui, "EDIT", [27., GROUP_HEIGHT], SOURCE_ORANGE).clicked()
                        {
                            self.begin_rule_wizard(ui.ctx(), view.uid, Some(gid));
                        }
                    } else {
                        add = rule_icon_button(
                            ui,
                            egui::include_image!("../../assets/icons/add.svg"),
                            [27., GROUP_HEIGHT],
                            SOURCE_ORANGE,
                        )
                        .on_hover_text(locale::tr(ui, "Add rule"))
                        .clicked();
                    }
                    let menu = compact_button(
                        ui,
                        "⋮",
                        [27., GROUP_HEIGHT],
                        if active { color } else { Color32::TRANSPARENT },
                    );
                    egui::Popup::menu(&menu)
                        .align(egui::RectAlign::BOTTOM_END)
                        .gap(2.)
                        .frame(rule_popup_frame(ui))
                        .show(|ui| {
                            self.rule_group_menu(ui, view, group, gid, &target, &mut rename)
                        });
                    menu_requested = menu.clicked();
                })
            });
        let frame = ui.ctx().cumulative_frame_nr();
        if ui
            .data(|d| d.get_temp::<u64>(biome_id))
            .is_some_and(|last| frame <= last + 1)
        {
            let anchor = biome_anchor.unwrap_or_else(|| {
                Rect::from_min_size(ui.ctx().content_rect().min + egui::vec2(0., 6.), Vec2::ZERO)
            });
            self.rule_group_biome_menu(ui, view, group, gid, biome_id, anchor);
        }
        if menu_requested {
            self.rule_group_uid = Some(gid);
            self.rule_uid = None;
        }
        if rename {
            ui.data_mut(|d| d.insert_temp(rename_group_id(), (view.uid, gid)));
        }
        if add && self.insert_rule(view.uid, gid, 0) {
            open = true;
        }
        if open {
            self.open_rule_groups.insert(group_key);
        } else {
            self.open_rule_groups.remove(&group_key);
        }
        if let Some(payload) = header.response.dnd_release_payload::<(i64, i64)>()
            && payload.0 == view.uid
            && payload.1 != gid
        {
            self.edit("Rule group reordered", |p| {
                p.reorder_auto_rule_group(view.uid, payload.1, index)
            });
        }
        if let Some(payload) = header.response.dnd_release_payload::<(i64, i64, i64)>()
            && payload.0 == view.uid
            && payload.1 != gid
        {
            self.edit("Rule moved", |p| {
                p.move_auto_rule_to_group(view.uid, payload.1, gid, payload.2, 0)
            });
        }
        if !open {
            return;
        }
        let managed = group["usesWizard"] == true;
        if managed {
            egui::Frame::new()
                .fill(theme::MEDIUM)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.;
                    let width = ui.available_width();
                    let edit = RichText::new(locale::tr(ui, "Edit rules").to_uppercase())
                        .size(13.333)
                        .color(Color32::WHITE);
                    if compact_button(ui, edit, [width, 35.], SOURCE_ORANGE).clicked()
                    {
                        self.begin_rule_wizard(ui.ctx(), view.uid, Some(gid));
                    }
                    let advanced = RichText::new(locale::tr(ui, "Switch to advanced mode"))
                        .size(10.667)
                        .color(theme::ACCENT)
                        .underline();
                    if ui
                        .add_sized([width, 21.], egui::Label::new(advanced).sense(Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.set_rule_options(ui.ctx(), view.uid, gid, None, RuleOptions::Advanced);
                    }
                    ui.add_space(7.);
                    source_controls::help_card(ui, width, 62., |ui| {
                        source_controls::help_paragraph(
                            ui,
                            "The rules in this group are managed by the Assistant editor.",
                        );
                    });
                });
        }
        for (index, rule) in group["rules"].as_array().into_iter().flatten().enumerate() {
            if index != 0 {
                ui.add_space(1.);
            }
            self.rule_row(ui, view, gid, (index, rule), managed, insertion);
        }
        ui.add_space(7.);
    }

    fn rule_group_menu(
        &mut self,
        ui: &mut egui::Ui,
        view: &RulesView,
        group: &Value,
        gid: i64,
        target: &ClipTarget,
        rename: &mut bool,
    ) {
        ui.set_width(390.);
        ui.spacing_mut().item_spacing.y = 0.;
        self.clipboard_menu(ui, target.clone());
        rule_menu_separator(ui);
        if rule_menu_row(ui, locale::tr(ui, "Rename"), "", None, false).clicked() {
            *rename = true;
            ui.close();
        }
        if rule_menu_row(
            ui,
            locale::tr(ui, "Duplicate and remap"),
            locale::tr(ui, "Duplicate the group, and optionally remap IntGrid\nIDs and tiles"),
            None,
            false,
        )
        .clicked()
        {
            self.begin_group_remap(ui.ctx(), view.uid, gid);
            ui.close();
        }
        rule_menu_separator(ui);
        if rule_menu_row(
            ui,
            locale::tr(ui, "Assign group color"),
            "",
            Some(egui::include_image!("../../assets/icons/color.svg")),
            false,
        )
        .clicked()
        {
            self.set_rule_options(ui.ctx(), view.uid, gid, None, RuleOptions::GroupColor);
            ui.close();
        }
        let atlas = view.atlas.as_ref().filter(|_| group["icon"].is_object());
        let icon = rule_menu_row(
            ui,
            locale::tr(ui, "Assign group icon"),
            "",
            atlas
                .is_none()
                .then_some(egui::include_image!("../../assets/icons/pickIcon.svg")),
            atlas.is_some(),
        );
        if let Some(atlas) = atlas {
            atlas.paint_rect(ui, rule_menu_icon_rect(icon.rect), &group["icon"]);
        }
        if icon.clicked() {
            self.set_rule_options(ui.ctx(), view.uid, gid, None, RuleOptions::GroupIcon);
            ui.close();
        }
        if !group["icon"].is_null()
            && rule_menu_row(
                ui,
                locale::tr(ui, "Remove group icon"),
                "",
                Some(egui::include_image!("../../assets/icons/deleteIcon.svg")),
                false,
            )
            .clicked()
        {
            self.edit("Rule group icon removed", |p| {
                p.update_auto_rule_group(view.uid, gid, &json!({"icon":null}))
            });
            ui.close();
        }
        rule_menu_separator(ui);
        if rule_menu_row(
            ui,
            locale::tr(ui, "Edit \"out-of-bounds\" policy for all rules"),
            "",
            Some(egui::include_image!("../../assets/icons/outOfBounds.svg")),
            false,
        )
        .clicked()
        {
            self.begin_group_policy(ui.ctx(), view.uid, gid);
            ui.close();
        }
        if group["usesWizard"] == true
            && rule_menu_row(
                ui,
                locale::tr(ui, "Edit rules using the Assistant"),
                "",
                Some(egui::include_image!("../../assets/icons/wizard.svg")),
                false,
            )
            .clicked()
        {
            self.begin_rule_wizard(ui.ctx(), view.uid, Some(gid));
            ui.close();
        }
        if rule_menu_row(ui, locale::tr(ui, "Assign biome enum"), "", None, false).clicked() {
            let frame = ui.ctx().cumulative_frame_nr();
            ui.data_mut(|d| d.insert_temp(view.group_key(gid).with("biome-menu"), frame));
            ui.close();
        }
        let optional = group["isOptional"] == true;
        let response = if optional {
            rule_menu_row(ui, locale::tr(ui, "Disable optional state"), "", None, false)
        } else {
            rule_menu_row(
                ui,
                locale::tr(ui, "Turn into an optional group"),
                locale::tr(ui, "An optional group is disabled everywhere by\ndefault and can be enabled manually in specific levels."),
                Some(egui::include_image!("../../assets/icons/optional.svg")),
                false,
            )
        };
        if response.clicked() {
            if optional {
                self.set_rule_options(ui.ctx(), view.uid, gid, None, RuleOptions::RequireGroup);
            } else {
                self.edit("Rule group made optional", |p| {
                    p.update_auto_rule_group(
                        view.uid,
                        gid,
                        &json!({"isOptional":true,"active":true}),
                    )
                });
            }
            ui.close();
        }
    }

    fn paint_group_biomes(
        &mut self,
        ui: &egui::Ui,
        view: &RulesView,
        group: &Value,
        rect: Rect,
        required: &[Value],
    ) {
        if required.is_empty() {
            ui.painter().rect_stroke(
                Rect::from_center_size(rect.center(), Vec2::splat(14.)),
                2.,
                Stroke::new(1_f32, Color32::from_white_alpha(100)),
                StrokeKind::Inside,
            );
            return;
        }
        let values = view.biome.as_ref().and_then(|def| def["values"].as_array());
        let op = if group["biomeRequirementMode"] == 1 {
            "+"
        } else {
            "/"
        };
        for (i, id) in required.iter().take(4).enumerate() {
            let point = rect.left_center() + egui::vec2(11. + 25. * i as f32, 0.);
            let tile = values
                .and_then(|values| values.iter().find(|value| &value["id"] == id))
                .map(|value| &value["tileRect"]);
            if let Some(tile) = tile {
                self.paint_biome_tile(ui, Rect::from_center_size(point, Vec2::splat(18.)), tile);
            }
            if i + 1 < required.len().min(4) {
                ui.painter_at(rect).text(
                    point + egui::vec2(12.5, 0.),
                    Align2::CENTER_CENTER,
                    op,
                    FontId::proportional(12.),
                    theme::TEXT,
                );
            }
        }
    }

    fn paint_biome_tile(&mut self, ui: &egui::Ui, rect: Rect, tile: &Value) {
        if let Some(atlas) = tile["tilesetUid"]
            .as_i64()
            .and_then(|uid| self.rule_atlas(ui.ctx(), Some(uid)))
        {
            atlas.paint_rect(ui, rect, tile);
        }
    }

    fn rule_group_biome_menu(
        &mut self,
        ui: &mut egui::Ui,
        view: &RulesView,
        group: &Value,
        gid: i64,
        id: egui::Id,
        anchor: Rect,
    ) {
        let ctx = ui.ctx().clone();
        let mut open = true;
        egui::Popup::new(id, ctx.clone(), anchor, ui.layer_id())
            .kind(egui::PopupKind::Menu)
            .align(egui::RectAlign::BOTTOM_START)
            .gap(if view.biome.is_some() { -3. } else { 10. })
            .frame(rule_popup_frame(ui))
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let Some(def) = &view.biome else {
                    ui.set_width(266.);
                    if rule_menu_row(
                        ui,
                        locale::tr(ui, "Missing biome enum"),
                        locale::tr(ui, "Click to select your level biome enum"),
                        None,
                        false,
                    )
                    .clicked()
                    {
                        self.show_panel(Some(Panel::Layers));
                        ui.close();
                    }
                    return;
                };
                let values = def["values"].as_array().map(Vec::as_slice).unwrap_or(&[]);
                let longest_label = ui.fonts_mut(|fonts| {
                    values
                        .iter()
                        .map(|value| {
                            fonts
                                .layout_no_wrap(
                                    text(value, "id", "").to_uppercase(),
                                    FontId::proportional(13.),
                                    theme::TEXT,
                                )
                                .size()
                                .x
                        })
                        .fold(0., f32::max)
                });
                let has_conditions = group["requiredBiomeValues"]
                    .as_array()
                    .is_some_and(|v| v.len() > 1);
                ui.set_width((longest_label + 90.).max(if has_conditions { 330. } else { 150. }));
                let mut show_items = |ui: &mut egui::Ui| {
                    if values.is_empty() {
                        ui.add_space(7.);
                        ui.weak(locale::format(ui, "Enum {name} has no value", &[("name", text(def, "identifier", ""))]));
                        ui.add_space(7.);
                    }
                    let before: Vec<_> = group["requiredBiomeValues"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let mut required = before.clone();
                    let previous_mode = group["biomeRequirementMode"].as_i64().unwrap_or(0);
                    let mut mode = previous_mode;
                    let check = |selected| {
                        Some(if selected {
                            egui::include_image!("../../assets/icons/checkboxOn.svg")
                        } else {
                            egui::include_image!("../../assets/icons/checkboxOff.svg")
                        })
                    };
                    if rule_menu_row(ui, locale::tr(ui, "Any biome"), "", check(required.is_empty()), false)
                        .clicked()
                    {
                        required.clear();
                    }
                    for value in values {
                        let name = text(value, "id", "");
                        let enabled = required.contains(&name);
                        let tile = &value["tileRect"];
                        let response =
                            rule_menu_row(ui, name, "", check(enabled), tile.is_object());
                        if tile.is_object() {
                            let rect =
                                rule_menu_icon_rect(response.rect).translate(egui::vec2(20., 0.));
                            self.paint_biome_tile(ui, rect, tile);
                        }
                        if response.clicked() {
                            if enabled {
                                required.retain(|item| *item != name);
                            } else {
                                required.push(name);
                            }
                        }
                    }
                    if required.len() > 1 {
                        rule_menu_separator(ui);
                        ui.add_space(4.);
                        ui.label(locale::tr(ui, "Condition type"));
                        for (value, name, detail) in [
                            (0, "OR", "At least one of the checked values is required."),
                            (1, "AND", "All checked values are required."),
                        ] {
                            if rule_menu_row(ui, locale::tr(ui, name), locale::tr(ui, detail), check(mode == value), false)
                                .clicked()
                            {
                                mode = value;
                            }
                        }
                    }
                    if required != before || mode != previous_mode {
                        required.sort_by_cached_key(|name| name.to_lowercase());
                        self.edit("Rule group biome changed", |p| {
                            p.update_auto_rule_group(
                                view.uid,
                                gid,
                                &json!({
                                    "requiredBiomeValues":required, "biomeRequirementMode":mode,
                                }),
                            )
                        });
                    }
                };
                let max_height = ctx.content_rect().height() * 0.72;
                let rows_height = 40. * (values.len() + 1) as f32;
                if rows_height + 160. > max_height {
                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .show(ui, show_items);
                } else {
                    show_items(ui);
                }
            });
        let frame = ctx.cumulative_frame_nr();
        ui.data_mut(|data| {
            if open {
                data.insert_temp(id, frame);
            } else {
                data.remove::<u64>(id);
            }
        });
    }

    fn rule_row(
        &mut self,
        ui: &mut egui::Ui,
        view: &RulesView,
        gid: i64,
        (index, rule): (usize, &Value),
        managed: bool,
        insertion: &mut Option<RuleInsertion>,
    ) {
        let Some(rid) = rule["uid"].as_i64() else {
            return;
        };
        let active = rule["active"] != false;
        let target = ClipTarget::Rule {
            layer: view.uid,
            group: gid,
            rule: Some(rid),
        };
        let mut patch = json!({});
        let width = ui.available_width();
        let pattern = rule["pattern"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let layout = (!rule_has_unknown_conditions(&view.source, pattern))
            .then(|| PatternPreview::new(rule, pattern));
        let preview_height = layout.as_ref().map_or(39., |layout| layout.span.y);
        let content_height = RULE_CONTENT_HEIGHT.max(preview_height);
        let row = egui::Frame::new()
            .fill(if active { theme::DARK } else { Color32::from_rgba_unmultiplied(121, 17, 5, 77) })
            .inner_margin(3)
            .corner_radius(3.)
            .show(ui, |ui| {
                ui.set_min_width((width - 6.).max(0.));
                if !active {
                    let rect = Rect::from_min_size(ui.cursor().min - Vec2::splat(3.), egui::vec2(width, content_height + 6.));
                    egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
                        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                        .tint(Color32::from_white_alpha(153))
                        .paint_at(ui, rect);
                    ui.multiply_opacity(0.6);
                }
                if managed {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    rule_drag(ui, 25., content_height).dnd_set_drag_payload((view.uid, gid, rid));
                    ui.add_space(2.);
                    if toggle(
                        ui,
                        active,
                        false,
                        "Enable or disable this rule",
                        egui::vec2(21., content_height),
                    )
                    .clicked()
                    {
                        patch["active"] = json!(!active);
                    }
                    ui.add_space(2.);
                    let (preview, response) =
                        ui.allocate_exact_size(egui::vec2(72., preview_height), Sense::click());
                    self.paint_pattern_preview(ui, preview, view, rule, layout.as_ref());
                    if response.hovered() {
                        ui.painter().rect_stroke(
                            preview,
                            0.,
                            Stroke::new(1_f32, theme::ACCENT),
                            StrokeKind::Outside,
                        );
                    }
                    if !managed && response.hovered() {
                        self.rule_hover = Some((view.uid, gid, Some(rid)));
                    }
                    if response.clicked() {
                        self.clipboard_focus(ui.ctx(), target.clone());
                        self.begin_rule_edit(view.uid, gid, rid);
                    }
                    response.context_menu(|ui| self.clipboard_menu(ui, target.clone()));
                    ui.add_space(10.);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2., 2.);
                        ui.horizontal(|ui| {
                            for (key, chance) in [("chance", true), ("alpha", false)] {
                                let scale = if chance { 100. } else { 1. };
                                let mut value = rule[key].as_f64().unwrap_or(1.) * scale;
                                if rule_number(ui, &mut value, chance)
                                    .on_hover_text(locale::tr(ui, if chance {
                                        "Random probability (%)"
                                    } else {
                                        "Tile opacity"
                                    }))
                                    .changed()
                                {
                                    patch[key] = json!(value / scale);
                                }
                            }
                            if rule_flag(
                                ui,
                                26.,
                                rule["breakOnMatch"] == true,
                                Color32::from_rgb(0, 157, 255),
                                RuleFlag::Icon(egui::include_image!("../../assets/icons/stop.svg")),
                            )
                            .on_hover_text(locale::tr(ui, "Stop on match"))
                            .clicked()
                            {
                                patch["breakOnMatch"] = json!(rule["breakOnMatch"] != true);
                            }
                        });
                        ui.horizontal(|ui| {
                            for (key, label) in [("flipX", "X"), ("flipY", "Y")] {
                                if rule_flag(
                                    ui,
                                    20.5,
                                    rule[key] == true,
                                    Color32::from_rgb(207, 221, 64),
                                    RuleFlag::Text(label),
                                )
                                .on_hover_text(locale::tr(ui, if key == "flipX" {
                                    "Horizontal symmetry"
                                } else {
                                    "Vertical symmetry"
                                }))
                                .clicked()
                                {
                                    if ldtk_core::auto_rule_pattern_is_symmetric(rule, key == "flipX") {
                                        self.report_canvas_notice(
                                            notifications::CanvasNotice::SymmetricRule,
                                        );
                                    } else {
                                        patch[key] = json!(rule[key] != true);
                                    }
                                }
                            }
                            let offsets = rule_flag(
                                ui,
                                43.,
                                [
                                        "tileXOffset",
                                        "tileYOffset",
                                        "tileRandomXMin",
                                        "tileRandomXMax",
                                        "tileRandomYMin",
                                        "tileRandomYMax",
                                    ]
                                    .iter()
                                    .any(|&k| rule[k].as_i64().unwrap_or(0) != 0),
                                Color32::from_rgb(207, 221, 64),
                                RuleFlag::Icon(egui::include_image!("../../assets/icons/offset.svg")),
                            )
                            .on_hover_text(locale::tr(ui, "Randomize position"));
                            if offsets.clicked() {
                                self.set_anchored_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    rid,
                                    RuleOptions::Offsets,
                                    Some(offsets.rect),
                                );
                            }
                            let x = rule["xModulo"].as_i64().unwrap_or(1);
                            let y = rule["yModulo"].as_i64().unwrap_or(1);
                            let (rect, modulo) = ui.allocate_exact_size(egui::vec2(43., 26.), Sense::click());
                            let (fill, ink) = if modulo.hovered() {
                                (SOURCE_ORANGE, theme::DARK)
                            } else if x == 1 && y == 1 {
                                (Color32::from_rgba_unmultiplied(121, 17, 5, 102), SOURCE_ORANGE.gamma_multiply(0.4))
                            } else {
                                (theme::MEDIUM, Color32::WHITE)
                            };
                            ui.painter().rect_filled(rect, 4., fill);
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("{x}-{y}"), FontId::proportional(12.), ink);
                            let modulo = modulo.on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(locale::tr(ui, "Modulo and checkerboard options"));
                            if modulo.clicked() {
                                self.set_anchored_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    rid,
                                    RuleOptions::Modulo,
                                    Some(modulo.rect),
                                );
                            }
                            let noise = rule_flag(
                                ui,
                                26.,
                                rule["perlinActive"] == true,
                                Color32::from_rgb(207, 221, 64),
                                RuleFlag::Text("P"),
                            )
                            .on_hover_text(locale::tr(ui, "Perlin filter. Right-click to customize."));
                            if noise.clicked() {
                                patch["perlinActive"] = json!(rule["perlinActive"] != true);
                            }
                            if noise.secondary_clicked() {
                                let was_active = rule["perlinActive"] == true;
                                if !was_active {
                                    patch["perlinActive"] = json!(true);
                                }
                                self.set_anchored_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    rid,
                                    RuleOptions::Noise,
                                    was_active.then_some(noise.rect),
                                );
                            }
                        });
                    });
                    ui.add_space(3.);
                    let menu = rule_icon_button(
                        ui,
                        egui::include_image!("../../assets/icons/contextMenu.svg"),
                        [33., 32.],
                        Color32::from_black_alpha(25),
                    );
                    egui::Popup::menu(&menu)
                        .align(egui::RectAlign::BOTTOM_END)
                        .frame(rule_popup_frame(ui))
                        .show(|ui| {
                            self.clipboard_menu(ui, target.clone());
                        });
                })
            })
            .response;
        if !managed {
            let hovered = ui.rect_contains_pointer(row.rect);
            if hovered && ui.input(|input| input.pointer.any_pressed()) {
                self.last_rule_list_row = Some(rid);
            }
            if self.rule_draft.is_none() && ui.memory(|memory| memory.top_modal_layer().is_none()) {
                let target = RuleInsertion {
                    group: gid,
                    rule: rid,
                    index,
                    row: row.rect,
                };
                let previous = ui.data(|data| data.get_temp::<i64>(view.insert_hover_id()));
                let on_button = previous == Some(rid)
                    && target
                        .buttons()
                        .into_iter()
                        .any(|rect| ui.rect_contains_pointer(rect));
                if on_button || hovered && insertion.is_none() {
                    *insertion = Some(target);
                }
            }
        }
        if self.last_rule_list_row == Some(rid) {
            let rect = row.rect.shrink(0.5);
            ui.painter().extend(egui::Shape::dashed_line(
                &[
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ],
                Stroke::new(1_f32, Color32::WHITE),
                3.,
                2.,
            ));
        }
        if changed(&patch) {
            self.edit("Rule changed", |p| {
                p.update_auto_rule(view.uid, gid, rid, &patch)
            });
        }
        if let Some(payload) = row.dnd_release_payload::<(i64, i64, i64)>()
            && payload.0 == view.uid
            && payload.2 != rid
        {
            self.edit("Rule reordered", |p| {
                p.move_auto_rule_to_group(view.uid, payload.1, gid, payload.2, index)
            });
        }
    }
}

fn rule_group_members(group: &Value, rid: Option<i64>) -> impl Iterator<Item = i64> {
    group["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(move |r| rid.is_none_or(|id| r["uid"] == id))
        .filter_map(|r| r["uid"].as_i64())
}

fn rename_group_id() -> egui::Id {
    egui::Id::new("rename-rule-group")
}
enum RuleFlag<'a> {
    Text(&'a str),
    Icon(egui::ImageSource<'static>),
}

fn rule_flag(
    ui: &mut egui::Ui,
    width: f32,
    enabled: bool,
    ink: Color32,
    content: RuleFlag<'_>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.), Sense::click());
    let painter = ui.painter();
    if enabled {
        painter.rect(
            rect,
            4.,
            if response.hovered() {
                theme::LIGHT
            } else {
                theme::MEDIUM
            },
            Stroke::new(1_f32, ink),
            StrokeKind::Inside,
        );
    } else {
        painter.rect_filled(rect, 4., Color32::from_rgba_unmultiplied(121, 17, 5, 102));
        let edge = rect.shrink(0.5);
        painter.extend(egui::Shape::dashed_line(
            &[
                edge.left_top(),
                edge.right_top(),
                edge.right_bottom(),
                edge.left_bottom(),
                edge.left_top(),
            ],
            Stroke::new(1_f32, SOURCE_ORANGE.gamma_multiply(0.4)),
            3.,
            2.,
        ));
    }
    match content {
        RuleFlag::Text(text) => {
            let ink = if response.hovered() {
                Color32::WHITE
            } else if enabled {
                ink
            } else {
                SOURCE_ORANGE.gamma_multiply(0.4)
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(10.667),
                ink,
            );
        }
        RuleFlag::Icon(icon) => {
            egui::Image::new(icon)
                .tint(Color32::from_white_alpha(if enabled { 255 } else { 51 }))
                .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.)));
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn rule_number(ui: &mut egui::Ui, number: &mut f64, chance: bool) -> egui::Response {
    let (rect, click) = ui.allocate_exact_size(egui::vec2(66., 26.), Sense::click());
    let fill = ui.painter().add(egui::Shape::Noop);
    let mut area = rect;
    area.min.x += 22.;
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
    ));
    field.set_clip_rect(field.clip_rect().intersect(area));
    field.spacing_mut().button_padding = Vec2::splat(1.);
    field.style_mut().override_font_id = Some(FontId::proportional(12.));
    let ink = if chance && *number == 0. {
        SOURCE_ORANGE
    } else {
        Color32::WHITE
    };
    field.visuals_mut().override_text_color = Some(ink);
    let widgets = &mut field.visuals_mut().widgets;
    for widget in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        widget.weak_bg_fill = Color32::TRANSPARENT;
        widget.bg_fill = Color32::TRANSPARENT;
        widget.bg_stroke = Stroke::NONE;
    }
    let max = if chance { 100. } else { 1. };
    let at_max = *number >= max;
    let input_id = field.next_auto_id();
    if click.clicked()
        && click
            .interact_pointer_pos()
            .is_some_and(|point| point.x < area.left())
    {
        field.memory_mut(|memory| memory.request_focus(input_id));
    }
    let editing = field.memory(|memory| memory.has_focus(input_id));
    if at_max && !editing {
        field.multiply_opacity(0.25);
    }
    let response = field.add(
        egui::DragValue::new(number)
            .speed(if chance { 1. } else { 0.01 })
            .range((if chance { 1. } else { 0.01 })..=max)
            .clamp_existing_to_range(false)
            .custom_formatter(|value, _| {
                format!("{value:.2}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_owned()
            }),
    );
    let focused = editing || response.has_focus() || response.dragged();
    let alpha = if at_max && !focused { 64 } else { 255 };
    let shape = egui::Shape::rect_filled(
        rect,
        4.,
        if focused { theme::DARK } else { theme::MEDIUM }.gamma_multiply(f32::from(alpha) / 255.),
    );
    ui.painter().set(fill, shape);
    if focused {
        ui.painter()
            .rect_stroke(rect, 4., Stroke::new(1_f32, SOURCE_ORANGE), StrokeKind::Inside);
    }
    let icon = if chance {
        egui::include_image!("../../assets/icons/random.svg")
    } else {
        egui::include_image!("../../assets/icons/alpha.svg")
    };
    egui::Image::new(icon)
        .tint(Color32::from_white_alpha(alpha))
        .paint_at(
            ui,
            Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(18.)),
        );
    response
}

struct PatternPreview<'a> {
    pattern: &'a [Value],
    size: usize,
    scale: f32,
    output_scale: f32,
    cell_span: f32,
    columns: [f32; 9],
    rows: [f32; 9],
    span: Vec2,
}

impl<'a> PatternPreview<'a> {
    fn new(rule: &Value, pattern: &'a [Value]) -> Self {
        let size = rule["size"].as_u64().unwrap_or(1).clamp(1, 9) as usize;
        let (scale, output_scale) = match size {
            1 => (1., 1.),
            3 => (0.5, 1.),
            5 => (0.3, 0.6),
            7 => (0.2, 0.6),
            9 => (0.15, 0.45),
            _ => (1.5 / size as f32, 0.6),
        };
        let center = size / 2;
        let cell_span = 32. * scale;
        let mut columns = [cell_span; 9];
        let mut rows = [cell_span; 9];
        let value_at = |x, y| {
            pattern
                .get(y * size + x)
                .and_then(Value::as_i64)
                .unwrap_or(0)
        };
        for (y, row) in rows[..size].iter_mut().enumerate() {
            for (x, column) in columns[..size].iter_mut().enumerate() {
                if value_at(x, y) != 0 || (x == center && y == center) {
                    *column = cell_span + 2.;
                    *row = cell_span + 2.;
                }
            }
        }
        let span = egui::vec2(columns[..size].iter().sum(), rows[..size].iter().sum());
        Self {
            scale,
            output_scale,
            pattern,
            size,
            cell_span,
            columns,
            rows,
            span: span + Vec2::splat(size as f32 + 1.),
        }
    }
}

fn paint_preview_stripes(ui: &egui::Ui, rect: Rect, scale: f32) {
    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
        .uv(Rect::from_min_size(
            egui::Pos2::ZERO,
            rect.size() / (8. * scale),
        ))
        .paint_at(ui, rect);
}

impl EditorApp {
    fn paint_pattern_preview(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        view: &RulesView,
        rule: &Value,
        layout: Option<&PatternPreview<'_>>,
    ) {
        let painter = ui.painter().clone();
        let Some(layout) = layout else {
            let warning = Rect::from_center_size(rect.center(), egui::vec2(58., 39.));
            let ink = Color32::from_rgb(227, 77, 0);
            painter.rect_filled(warning, 0., Color32::from_rgb(121, 17, 5));
            paint_preview_stripes(ui, warning, 1.);
            painter.rect_stroke(warning, 0., Stroke::new(1_f32, ink), StrokeKind::Inside);
            painter.text(
                warning.center(),
                Align2::CENTER_CENTER,
                locale::tr(ui, "Error"),
                FontId::proportional(16.),
                ink,
            );
            return;
        };

        let PatternPreview {
            pattern,
            size,
            scale,
            output_scale,
            cell_span,
            columns,
            rows,
            span,
        } = *layout;
        let center = size / 2;
        let grid = Rect::from_center_size(rect.center(), span);
        painter.rect_stroke(grid, 0., Stroke::new(1_f32, theme::MEDIUM), StrokeKind::Inside);
        let mut cells = ui.new_child(egui::UiBuilder::new().max_rect(grid));
        cells.multiply_opacity(0.5);
        let stamp = rule["tileMode"] == "Stamp";
        let (mut y, mut center_cell) = (grid.top() + 1., Rect::NOTHING);
        for (row, height) in rows[..size].iter().enumerate() {
            let mut x = grid.left() + 1.;
            for (column, width) in columns[..size].iter().enumerate() {
                let origin = egui::pos2(x, y);
                let value = pattern
                    .get(row * size + column)
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                if column == center && row == center {
                    center_cell = Rect::from_min_size(origin, Vec2::splat(cell_span + 2.));
                    if stamp {
                        cells.painter().rect_filled(center_cell, 2., theme::MEDIUM);
                    } else {
                        painter.rect_filled(center_cell, 2., theme::MEDIUM);
                        painter.extend(egui::Shape::dashed_line(
                            &[
                                center_cell.left_top(),
                                center_cell.right_top(),
                                center_cell.right_bottom(),
                                center_cell.left_bottom(),
                                center_cell.left_top(),
                            ],
                            Stroke::new(scale, Color32::WHITE),
                            3. * scale,
                            2. * scale,
                        ));
                    }
                } else {
                    let outer = cell_span + if value == 0 { 0. } else { 2. };
                    let cell = Rect::from_min_size(origin, Vec2::splat(outer));
                    self.paint_rule_cell(
                        &cells,
                        cell,
                        &view.source,
                        value,
                        RuleCellStyle::Preview(scale),
                    );
                }
                x += width + 1.;
            }
            y += height + 1.;
        }

        let ids = rule["tileRectsIds"][0]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if stamp {
            let span = view.atlas.as_ref().map_or(Vec2::splat(cell_span), |atlas| {
                atlas.stamp_preview_size(cell_span, ids)
            });
            let pivot = egui::vec2(number(rule, "pivotX", 0.), number(rule, "pivotY", 0.));
            let origin = center_cell.min + Vec2::splat(1.);
            if let (Some(atlas), Some(first)) = (&view.atlas, ids.first()) {
                let tile = Rect::from_min_size(origin, Vec2::splat(cell_span));
                atlas.paint_ids(&cells, tile, std::slice::from_ref(first));
            }
            let guide = Rect::from_min_size(origin + pivot * (Vec2::splat(cell_span) - span), span);
            cells.painter().rect_stroke(
                guide,
                0.,
                Stroke::new(2. * scale, Color32::BLACK),
                StrokeKind::Outside,
            );
            cells.painter().rect_stroke(
                guide,
                0.,
                Stroke::new(3. * scale, theme::ACCENT),
                StrokeKind::Inside,
            );
        } else if let Some(atlas) = &view.atlas {
            let tile =
                Rect::from_center_size(center_cell.center(), Vec2::splat(32. * output_scale));
            if rule["tileRectsIds"]
                .as_array()
                .is_some_and(|groups| groups.len() > 1)
            {
                painter.rect_filled(
                    tile.translate(egui::vec2(3., 3.) * output_scale),
                    2.,
                    Color32::WHITE,
                );
                painter.rect_filled(
                    tile.translate(egui::vec2(2., 2.) * output_scale),
                    2.,
                    Color32::BLACK,
                );
            }
            painter.rect_filled(tile, 2., Color32::GRAY);
            paint_preview_stripes(ui, tile.shrink(1.), output_scale);
            painter.rect_stroke(
                tile,
                2.,
                Stroke::new(output_scale, Color32::WHITE),
                StrokeKind::Inside,
            );
            if let Some(first) = ids.first() {
                atlas.paint_ids(
                    ui,
                    tile.shrink(3. * output_scale),
                    std::slice::from_ref(first),
                );
            }
        }
    }

    pub(in crate::app) fn draw_rule_hover(
        &self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        key: LevelKey,
    ) {
        if self.panel != Some(Panel::Rules)
            || self.rule_draft.is_some()
            || !self.render_auto
            || ctx.input(|i| i.modifiers.command)
        {
            return;
        }
        let Some((layer, gid, rid)) = self.rule_hover else {
            return;
        };
        let Some(project) = &self.project else { return };
        let Some(def) = project.layer_def(layer) else {
            return;
        };
        let Some(group) = def["autoRuleGroups"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|g| g["uid"] == gid)
        else {
            return;
        };
        let Some((index, li)) = project
            .level(key)
            .and_then(|l| l["layerInstances"].as_array())
            .and_then(|a| {
                a.iter()
                    .enumerate()
                    .find(|(_, li)| li["layerDefUid"] == layer)
            })
        else {
            return;
        };
        let rules: HashSet<_> = rule_group_members(group, rid).collect();
        let grid = number(li, "__gridSize", 16.);
        let painter = painter.with_clip_rect(self.uncovered_canvas_rect(rect));
        for tile in li["autoLayerTiles"].as_array().into_iter().flatten() {
            if tile["d"][0].as_i64().is_none_or(|r| !rules.contains(&r)) {
                continue;
            }
            let point = canvas::layer_to_world(
                project,
                key,
                index,
                &self.viewport,
                [
                    tile["px"][0].as_f64().unwrap_or(0.) as f32,
                    tile["px"][1].as_f64().unwrap_or(0.) as f32,
                ],
            );
            let screen = self
                .viewport
                .to_screen(point, rect.min.into(), rect.size().into());
            let box_rect = Rect::from_min_size(
                egui::pos2(screen[0], screen[1]),
                Vec2::splat(grid * self.viewport.zoom),
            );
            painter.rect_filled(
                box_rect,
                0.,
                Color32::from_rgba_unmultiplied(90, 54, 167, 153),
            );
            painter.rect_stroke(
                box_rect,
                0.,
                Stroke::new(1_f32, Color32::from_rgb(255, 0, 255)),
                StrokeKind::Inside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_row_previews_conditions_and_reports_missing_source_values() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let view = RulesView {
            uid: 4,
            key: Project::new().level_keys()[0],
            index: 0,
            project: String::new(),
            def: Value::Null,
            instance: Value::Null,
            source: json!({
                "intGridValues": [{"value": 1, "color": "#B86F50"}],
                "intGridValuesGroups": [{"uid": 1, "color": "#34683A"}],
            }),
            atlas: None,
            biome: None,
        };
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let patterns = [
                    json!([0, 1, -1, 2000, 1, -2000, 1000001, -1000001, 0]),
                    json!([0, 0, 0, 0, -999, 0, 0, 0, 0]),
                    json!([0, 0, 0, 0, 7000, 0, 0, 0, 0]),
                ];
                for (index, pattern) in patterns.into_iter().enumerate() {
                    let rule = json!({"uid": index, "size": 3, "pattern": pattern});
                    app.rule_row(ui, &view, 7, (index, &rule), false, &mut None);
                }
            });
        });
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    Some(text.galley.text())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(labels.iter().filter(|&&text| text == "Error").count(), 2);
        assert_eq!(labels.iter().filter(|&&text| text == "?").count(), 1);
    }

    #[test]
    fn group_context_menu_exposes_separate_appearance_actions_in_source_order() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let p = Project::new();
        let view = RulesView {
            uid: 4,
            key: p.level_keys()[0],
            index: 0,
            project: String::new(),
            def: Value::Null,
            instance: Value::Null,
            source: Value::Null,
            atlas: None,
            biome: None,
        };
        let target = ClipTarget::Rule {
            layer: 4,
            group: 7,
            rule: None,
        };
        let group = json!({"uid":7,"icon":{"tilesetUid":2,"x":0,"y":0,"w":16,"h":16}});
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.rule_group_menu(ui, &view, &group, 7, &target, &mut false);
            });
        });
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    Some(text.galley.text().to_ascii_uppercase())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let expected = [
            "ASSIGN GROUP COLOR",
            "ASSIGN GROUP ICON",
            "REMOVE GROUP ICON",
            "EDIT \"OUT-OF-BOUNDS\" POLICY FOR ALL RULES",
            "ASSIGN BIOME ENUM",
        ];
        let positions =
            expected.map(|text| labels.iter().position(|label| label.starts_with(text)));
        assert!(positions.iter().all(Option::is_some), "{labels:?}");
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{labels:?}"
        );
    }

    #[test]
    fn right_click_in_empty_rule_list_reads_system_clipboard() {
        let ctx = egui::Context::default();
        let mut p = Project::new();
        let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
        p.json["defs"]["tilesets"] = json!([{"uid":400,"tileGridSize":16,"pxWid":32,"pxHei":32}]);
        p.assign_tileset(layer, 400).unwrap();
        let key = p.level_keys()[0];
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.selected = Some(key);
        app.definition_uid = Some(layer);
        app.panel = Some(Panel::Rules);

        let pos = egui::pos2(30., 420.);
        let mut render = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(380., 660.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.rule_list(ui, 655.));
                },
            )
        };
        let _ = render(vec![egui::Event::PointerMoved(pos)]);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = render(vec![button(true)]);
        let output = render(vec![button(false)]);
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::RequestPaste)
        );
    }
}
