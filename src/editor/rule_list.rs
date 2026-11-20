use super::*;
use crate::app::panels::rule_detail::{RuleAtlas, RuleOptions};

const ROW_HEIGHT: f32 = 63.;
const GROUP_HEIGHT: f32 = 35.;

fn toggle(ui: &mut egui::Ui, enabled: bool, tip: &str, width: f32, height: f32) -> egui::Response {
    let symbol = if enabled {
        egui::include_image!("../../assets/icons/toggleOn.svg")
    } else {
        egui::include_image!("../../assets/icons/toggleOff.svg")
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    egui::Image::new(symbol)
        .tint(if enabled {
            Color32::from_rgb(157, 207, 96)
        } else {
            Color32::from_rgb(155, 112, 101)
        })
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
    egui::Frame::popup(ui.style())
        .fill(theme::DARK)
        .stroke(Stroke::new(1., theme::LIGHT))
        .corner_radius(2.)
        .inner_margin(0.)
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
    painter.hline(rect.x_range(), rect.max.y, Stroke::new(1., theme::MEDIUM));
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
        self.rule_list_header(ui, &view, atlas_uid, ready);
        ui.add_space(8.);
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
            .stroke(Stroke::new(1., theme::LIGHT))
            .show(ui, |ui| {
                let scroll = egui::ScrollArea::vertical()
                    .id_salt(("rule-list", uid))
                    .max_height((height - 88.).max(40.))
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
                        for (index, group) in groups.iter().enumerate() {
                            let default_open = groups.len() == 1 && group["usesWizard"] != true;
                            self.rule_group_row(ui, &view, group, index, default_open);
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
                        Stroke::new(1., theme::ACCENT),
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
                            ui.data_mut(|data| {
                                data.insert_temp(view.group_key(gid), true);
                                data.insert_temp(rename_group_id(), (view.uid, gid));
                            });
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
            egui::ComboBox::from_id_salt(("rule-instance-tileset", view.uid))
                .width(width)
                .selected_text(
                    current
                        .map(name)
                        .unwrap_or_else(|| locale::tr(ui, "-- Select a tileset --").into()),
                )
                .show_ui(ui, |ui| {
                    for d in sorted {
                        if compatible(d) {
                            ui.selectable_value(&mut picked, d["uid"].as_i64(), name(d));
                        } else {
                            ui.add_enabled(
                                false,
                                egui::Button::selectable(
                                    false,
                                    locale::format(ui, "{name} (INCOMPATIBLE SIZE)", &[("name", &name(d))]),
                                ),
                            );
                        }
                    }
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
        default_open: bool,
    ) {
        let Some(gid) = group["uid"].as_i64() else {
            return;
        };
        let group_key = view.group_key(gid);
        let mut open = ui
            .data(|d| d.get_temp::<bool>(group_key))
            .unwrap_or(default_open);
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
            ((required.len() as f32 * 25. - 4.).max(24.)).min(108.)
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
                color.gamma_multiply(0.58)
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    rule_drag(ui, 28., GROUP_HEIGHT).dnd_set_drag_payload((view.uid, gid));
                    let toggle_tip = if optional {
                        "Toggle this optional group in this level"
                    } else {
                        "Toggle rule group"
                    };
                    if toggle(ui, active, toggle_tip, 35., GROUP_HEIGHT).clicked() {
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
                            ui.painter()
                                .rect_filled(rect, 0., Color32::from_white_alpha(38));
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
                        ui.painter_at(rect).text(
                            egui::pos2(text_x, rect.center().y),
                            Align2::LEFT_CENTER,
                            text(group, "name", "Group"),
                            FontId::proportional(15.),
                            Color32::WHITE,
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
                    let menu = compact_button(ui, "⋮", [27., GROUP_HEIGHT], color);
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
        if add {
            if self.insert_rule(view.uid, gid, 0) {
                open = true;
            }
        }
        ui.data_mut(|d| d.insert_temp(group_key, open));
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
        if group["usesWizard"] == true {
            if source_controls::button(ui, "Edit rules").clicked() {
                self.begin_rule_wizard(ui.ctx(), view.uid, Some(gid));
            }
            if ui.link(locale::tr(ui, "Switch to advanced mode")).clicked() {
                self.set_rule_options(ui.ctx(), view.uid, gid, None, RuleOptions::Advanced);
            }
            ui.small(locale::tr(ui, "The rules in this group are managed by the Assistant editor."));
        }
        for (index, rule) in group["rules"].as_array().into_iter().flatten().enumerate() {
            self.rule_row(ui, view, group, gid, rule, index);
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
                .then(|| egui::include_image!("../../assets/icons/pickIcon.svg")),
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
                Stroke::new(1., Color32::from_white_alpha(100)),
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
        group: &Value,
        gid: i64,
        rule: &Value,
        index: usize,
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
        let row = egui::Frame::new()
            .fill(if active {
                Color32::from_rgb(32, 37, 46)
            } else {
                Color32::from_rgb(47, 33, 29)
            })
            .stroke(Stroke::new(1., theme::MEDIUM))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    rule_drag(ui, 28., ROW_HEIGHT - 2.).dnd_set_drag_payload((view.uid, gid, rid));
                    if toggle(
                        ui,
                        active,
                        "Enable or disable this rule",
                        25.,
                        ROW_HEIGHT - 2.,
                    )
                    .clicked()
                    {
                        patch["active"] = json!(!active);
                    }
                    ui.add_space(8.);
                    let (preview, response) =
                        ui.allocate_exact_size(Vec2::splat(56.), Sense::click());
                    draw_pattern_preview(ui, preview, &view.source, rule);
                    if let Some(atlas) = &view.atlas
                        && let Some(ids) = rule["tileRectsIds"][0].as_array()
                    {
                        let rect = Rect::from_center_size(preview.center(), Vec2::splat(33.));
                        ui.painter()
                            .rect_filled(rect, 1., Color32::from_black_alpha(154));
                        ui.painter().rect_stroke(
                            rect,
                            1.,
                            Stroke::new(1., Color32::WHITE),
                            StrokeKind::Inside,
                        );
                        atlas.paint_ids(ui, rect.shrink(2.), ids);
                    }
                    if response.hovered() {
                        self.rule_hover = Some((view.uid, gid, Some(rid)));
                    }
                    if response.clicked() && group["usesWizard"] != true {
                        self.clipboard_focus(ui.ctx(), target.clone());
                        self.begin_rule_edit(view.uid, gid, rid);
                    }
                    response.context_menu(|ui| self.clipboard_menu(ui, target.clone()));
                    ui.add_space(18.);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2., 2.);
                        ui.horizontal(|ui| {
                            for (key, value, prefix, scale) in [
                                ("chance", rule["chance"].as_f64().unwrap_or(1.), "⚄", 100.),
                                ("alpha", rule["alpha"].as_f64().unwrap_or(1.), "◒", 1.),
                            ] {
                                let mut adjusted = value * scale;
                                let minimum = if key == "alpha" { 0.01 } else { 0. };
                                if ui
                                    .add_sized(
                                        [65., 24.],
                                        egui::DragValue::new(&mut adjusted)
                                            .prefix(format!("{prefix} "))
                                            .speed(if scale == 100. { 1. } else { 0.01 })
                                            .range(minimum..=scale)
                                            .max_decimals(if scale == 100. { 1 } else { 2 }),
                                    )
                                    .on_hover_text(locale::tr(ui, if key == "chance" {
                                        "Random probability (%)"
                                    } else {
                                        "Tile opacity"
                                    }))
                                    .changed()
                                {
                                    patch[key] = json!(adjusted / scale);
                                }
                            }
                            if rule_icon_button(
                                ui,
                                egui::include_image!("../../assets/icons/stop.svg"),
                                [25., 24.],
                                flag_fill(rule["breakOnMatch"] == true),
                            )
                            .on_hover_text(locale::tr(ui, "Stop on match"))
                            .clicked()
                            {
                                patch["breakOnMatch"] = json!(rule["breakOnMatch"] != true);
                            }
                        });
                        ui.horizontal(|ui| {
                            for (key, label) in [("flipX", "X"), ("flipY", "Y")] {
                                if compact_button(
                                    ui,
                                    label,
                                    [20., 25.],
                                    flag_fill(rule[key] == true),
                                )
                                .on_hover_text(locale::tr(ui, if key == "flipX" {
                                    "Horizontal symmetry"
                                } else {
                                    "Vertical symmetry"
                                }))
                                .clicked()
                                {
                                    patch[key] = json!(rule[key] != true);
                                }
                            }
                            if rule_icon_button(
                                ui,
                                egui::include_image!("../../assets/icons/offset.svg"),
                                [41., 25.],
                                flag_fill(
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
                                ),
                            )
                            .on_hover_text(locale::tr(ui, "Randomize position"))
                            .clicked()
                            {
                                self.set_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    Some(rid),
                                    RuleOptions::Offsets,
                                );
                            }
                            let x = rule["xModulo"].as_i64().unwrap_or(1);
                            let y = rule["yModulo"].as_i64().unwrap_or(1);
                            if compact_button(
                                ui,
                                format!("{x}-{y}"),
                                [44., 25.],
                                flag_fill(x != 1 || y != 1),
                            )
                            .on_hover_text(locale::tr(ui, "Modulo and checkerboard options"))
                            .clicked()
                            {
                                self.set_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    Some(rid),
                                    RuleOptions::Modulo,
                                );
                            }
                            let noise = compact_button(
                                ui,
                                "P",
                                [25., 25.],
                                flag_fill(rule["perlinActive"] == true),
                            )
                            .on_hover_text(locale::tr(ui, "Perlin filter. Right-click to customize."));
                            if noise.clicked() {
                                patch["perlinActive"] = json!(rule["perlinActive"] != true);
                            }
                            if noise.secondary_clicked() {
                                self.set_rule_options(
                                    ui.ctx(),
                                    view.uid,
                                    gid,
                                    Some(rid),
                                    RuleOptions::Noise,
                                );
                            }
                        });
                    });
                    ui.add_space(3.);
                    let menu = compact_button(ui, "⋮", [33., 32.], theme::DARK);
                    egui::Popup::menu(&menu)
                        .align(egui::RectAlign::BOTTOM_END)
                        .show(|ui| {
                            self.clipboard_menu(ui, target.clone());
                            if group["usesWizard"] != true {
                                if ui.button(locale::tr(ui, "Create new rule before")).clicked() {
                                    self.insert_rule(view.uid, gid, index);
                                    ui.close();
                                }
                                if ui.button(locale::tr(ui, "Create new rule after")).clicked() {
                                    self.insert_rule(view.uid, gid, index + 1);
                                    ui.close();
                                }
                            }
                        });
                })
            })
            .response;
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
fn flag_fill(on: bool) -> Color32 {
    if on {
        theme::MEDIUM
    } else {
        Color32::from_rgb(63, 34, 28)
    }
}

fn draw_pattern_preview(ui: &egui::Ui, rect: Rect, source: &Value, rule: &Value) {
    let size = rule["size"].as_u64().unwrap_or(1).clamp(1, 9) as usize;
    let cell = rect.width() / size as f32;
    for index in 0..size * size {
        let value = rule["pattern"][index].as_i64().unwrap_or(0);
        let color = source["intGridValues"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|v| v["value"].as_i64() == Some(value.abs()))
            .and_then(|v| v["color"].as_str())
            .map(theme::color)
            .unwrap_or(theme::MEDIUM);
        let fill = if value == 0 {
            theme::DARK
        } else {
            color.gamma_multiply(0.75)
        };
        let target = Rect::from_min_size(
            rect.min + egui::vec2((index % size) as f32 * cell, (index / size) as f32 * cell),
            Vec2::splat(cell),
        )
        .shrink(0.5);
        ui.painter().rect_filled(target, 0., fill);
        ui.painter().rect_stroke(
            target,
            0.,
            Stroke::new(1., theme::MEDIUM),
            StrokeKind::Inside,
        );
    }
}

impl EditorApp {
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
                Stroke::new(1., Color32::from_rgb(255, 0, 255)),
                StrokeKind::Inside,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
