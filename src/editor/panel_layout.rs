use super::*;
use std::collections::BTreeMap;

const LIST_WIDTH: f32 = 250.;
const SOURCE_ORANGE: Color32 = Color32::from_rgb(235, 78, 0);
const GROUP_BAR: Color32 = Color32::from_rgb(117, 130, 164);
fn active_id() -> egui::Id {
    egui::Id::new("source-panel-layout-active")
}
fn panel_kind(panel: Panel) -> Option<&'static str> {
    match panel {
        Panel::Entities => Some("entities"),
        Panel::Layers | Panel::Rules => Some("layers"),
        Panel::Enums => Some("enums"),
        Panel::Tilesets => Some("tilesets"),
        _ => None,
    }
}
fn title(panel: Panel) -> &'static str {
    match panel {
        Panel::Entities => "PROJECT ENTITIES",
        Panel::Layers => "PROJECT LAYERS",
        Panel::Enums => "PROJECT ENUMS",
        Panel::Tilesets => "PROJECT TILESETS",
        Panel::Rules => "AUTO-LAYER RULES",
        Panel::Fields => "CUSTOM FIELDS",
        Panel::Project => "PROJECT SETTINGS",
        Panel::World => "WORLD SETTINGS",
        Panel::Level => "LEVEL SETTINGS",
        Panel::Export => "PROJECT EXPORT",
        Panel::Help => "HELP",
        _ => "",
    }
}
fn image(panel: Panel) -> egui::ImageSource<'static> {
    match panel {
        Panel::Entities => egui::include_image!("../../assets/icons/entity.png"),
        Panel::Layers | Panel::Rules => egui::include_image!("../../assets/icons/layer.png"),
        Panel::Enums => egui::include_image!("../../assets/icons/enum.png"),
        Panel::Tilesets => egui::include_image!("../../assets/icons/tile.png"),
        Panel::World => egui::include_image!("../../assets/icons/world.svg"),
        Panel::Level => egui::include_image!("../../assets/icons/level.svg"),
        Panel::Help => egui::include_image!("../../assets/icons/help.svg"),
        _ => egui::include_image!("../../assets/icons/projectSettings.png"),
    }
}
fn orange(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
            .fill(SOURCE_ORANGE)
            .corner_radius(2.),
    )
}
fn source_menu(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(5., 6.);
        ui.spacing_mut().interact_size.x = 18.;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_stroke =
            Stroke::new(1_f32, Color32::from_black_alpha(45));
        let response = ui.menu_button(" ", body).response;
        for y in [-4., 0., 4.] {
            ui.painter()
                .circle_filled(response.rect.center() + egui::vec2(0., y), 1., theme::TEXT);
        }
    });
}
fn tint(color: Color32, toward: Color32, ratio: f32) -> Color32 {
    Color32::from_rgb(
        (color.r() as f32 * (1. - ratio) + toward.r() as f32 * ratio) as u8,
        (color.g() as f32 * (1. - ratio) + toward.g() as f32 * ratio) as u8,
        (color.b() as f32 * (1. - ratio) + toward.b() as f32 * ratio) as u8,
    )
}
fn source_file_types(raw: &str) -> Value {
    let tokens = raw
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | '.'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Value::Null;
    }
    let mut values = Vec::<String>::new();
    for token in tokens {
        let extension = token
            .strip_prefix('.')
            .unwrap_or(token)
            .to_ascii_lowercase();
        if !extension.is_empty()
            && !extension.contains("..")
            && extension.bytes().any(|c| c.is_ascii_alphanumeric())
        {
            let value = format!(".{extension}");
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    json!(values)
}

fn row(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    row_width(ui, label, 124., body);
}
fn row_width(ui: &mut egui::Ui, label: &str, label_width: f32, body: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 30.),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                source_controls::property_label(ui, label, label);
            },
        );
        ui.add_space(10.);
        ui.allocate_ui_with_layout(
            egui::vec2((ui.available_width() - 2.).max(40.), 30.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                body(ui);
            },
        );
    });
}
fn source_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    source_controls::checkbox(ui, value, label)
}

fn source_form(ui: &mut egui::Ui, value: &Value, properties: &[P]) -> Value {
    let mut patch = json!({});
    let mut start = 0;
    for (index, property) in properties.iter().enumerate() {
        if matches!(property.2, W::Bool) {
            if start < index {
                patch.as_object_mut().unwrap().extend(
                    form(ui, value, &properties[start..index])
                        .as_object()
                        .unwrap()
                        .clone(),
                );
            }
            row(ui, property.1, |ui| {
                let mut selected = value[property.0].as_bool().unwrap_or(false);
                if source_checkbox(ui, &mut selected, "").changed() {
                    patch[property.0] = json!(selected);
                }
            });
            start = index + 1;
        }
    }
    if start < properties.len() {
        patch.as_object_mut().unwrap().extend(
            form(ui, value, &properties[start..])
                .as_object()
                .unwrap()
                .clone(),
        );
    }
    patch
}
fn source_text(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let (mut original, mut draft) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .unwrap_or_else(|| (current.into(), current.into()));
    if original != current {
        original = current.into();
        draft = current.into();
    }
    let response = ui.add_sized([width, 33.], egui::TextEdit::singleline(&mut draft).id(id));
    let commit = (response.lost_focus()
        || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
        && draft != current;
    ui.data_mut(|d| d.insert_temp(id, (original, draft.clone())));
    commit.then_some(draft)
}
fn section(
    ui: &mut egui::Ui,
    title: &str,
    mut open: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    egui::Frame::new()
        .stroke(Stroke::new(1_f32, theme::LIGHT))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.), Sense::click());
            ui.painter().rect_filled(rect, 0., GROUP_BAR);
            let arrow = if open {
                [
                    egui::vec2(7., -3.),
                    egui::vec2(12., 2.),
                    egui::vec2(17., -3.),
                ]
            } else {
                [egui::vec2(9., -5.), egui::vec2(14., 0.), egui::vec2(9., 5.)]
            };
            for line in arrow.windows(2) {
                ui.painter().line_segment(
                    [rect.left_center() + line[0], rect.left_center() + line[1]],
                    Stroke::new(2_f32, Color32::WHITE),
                );
            }
            ui.painter().text(
                rect.left_center() + egui::vec2(24., 0.),
                Align2::LEFT_CENTER,
                title,
                FontId::new(17., egui::FontFamily::Name("panel-title".into())),
                if open { Color32::BLACK } else { theme::ACCENT },
            );
            if response.clicked() {
                open = !open;
            }
            if open {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(0, 4))
                    .show(ui, body);
            }
        });
    open
}
impl EditorApp {
    pub(super) fn source_panel_active(&self, ui: &egui::Ui) -> bool {
        ui.ctx()
            .data_mut(|d| d.get_temp::<bool>(active_id()))
            .unwrap_or(false)
    }
    pub(in crate::app) fn source_panel(&mut self, ctx: &egui::Context, panel: Panel) -> bool {
        self.prepare_color_context(ctx);
        if !matches!(
            panel,
            Panel::Entities
                | Panel::Layers
                | Panel::Enums
                | Panel::Tilesets
                | Panel::Rules
                | Panel::Fields
                | Panel::Project
                | Panel::World
                | Panel::Level
                | Panel::Export
                | Panel::Help
        ) {
            return false;
        }
        if let Some(kind) = panel_kind(panel)
            && let Some(project) = self.project.as_ref()
        {
            let defs = project.json["defs"][kind]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let allowed = |def: &&Value| {
                panel != Panel::Rules || def["type"] == "IntGrid" || def["type"] == "AutoLayer"
            };
            if panel == Panel::Enums {
                let internal = defs
                    .iter()
                    .any(|d| d["uid"].as_i64() == self.definition_uid);
                let external = project.json["defs"]["externalEnums"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|d| d["uid"].as_i64() == self.definition_uid));
                if !internal && !external {
                    self.definition_uid = defs
                        .first()
                        .and_then(|d| d["uid"].as_i64())
                        .or_else(|| project.json["defs"]["externalEnums"][0]["uid"].as_i64());
                }
                let external = project.json["defs"]["externalEnums"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|d| d["uid"].as_i64() == self.definition_uid));
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("source-external-enum"), external));
            } else if !defs
                .iter()
                .filter(allowed)
                .any(|d| d["uid"].as_i64() == self.definition_uid)
            {
                let preferred = if panel == Panel::Entities {
                    self.entity_uid
                } else {
                    self.selected
                        .and_then(|key| project.layer(key, self.layer))
                        .and_then(|li| li["layerDefUid"].as_i64())
                };
                self.definition_uid = preferred
                    .filter(|uid| defs.iter().filter(allowed).any(|d| d["uid"] == *uid))
                    .or_else(|| defs.iter().find(allowed).and_then(|d| d["uid"].as_i64()));
            }
        }
        let screen = ctx.content_rect();
        let width = 1000_f32.min(screen.width());
        let height = (screen.height() - 34.).max(100.);
        let body_height = (height - 69.).max(40.);
        let mut close = false;
        let dimmer = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("source-panel-dimmer"),
        ));
        dimmer.rect_filled(
            Rect::from_min_max(egui::pos2(width, 0.), screen.max),
            0.,
            Color32::from_black_alpha(100),
        );
        dimmer.rect_filled(
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 34.)),
            0.,
            Color32::from_black_alpha(100),
        );
        egui::Area::new(egui::Id::new("source-definition-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0., 34.))
            .constrain(false)
            .show(ctx, |ui| {
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(14.));
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Button, FontId::proportional(14.));
                ui.set_width(width);
                ui.set_max_width(width);
                ui.set_min_height(height);
                ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                egui::Frame::new()
                    .fill(theme::MEDIUM)
                    .corner_radius(4.)
                    .show(ui, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(width, 53.), Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            egui::CornerRadius {
                                nw: 4,
                                ne: 4,
                                sw: 0,
                                se: 0,
                            },
                            theme::LIGHT,
                        );
                        ui.put(
                            Rect::from_min_size(rect.min + egui::vec2(20., 6.5), Vec2::splat(40.)),
                            egui::Image::new(image(panel)).fit_to_exact_size(Vec2::splat(40.)),
                        );
                        ui.painter().text(
                            rect.min + egui::vec2(72., 26.5),
                            Align2::LEFT_CENTER,
                            title(panel),
                            FontId::new(20., egui::FontFamily::Name("panel-title".into())),
                            Color32::WHITE,
                        );
                        close = ui
                            .put(
                                Rect::from_min_size(
                                    egui::pos2(rect.right() - 57., rect.top() + 8.),
                                    Vec2::splat(36.),
                                ),
                                egui::Button::new(
                                    RichText::new("×").size(28.).color(Color32::WHITE),
                                )
                                .fill(theme::LIGHT)
                                .stroke(Stroke::new(1_f32, theme::DARK)),
                            )
                            .clicked();
                        egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 20,
                                right: 20,
                                top: 16,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                if panel == Panel::Help {
                                    ui.set_width((width - 40.).max(80.));
                                    egui::ScrollArea::vertical()
                                        .id_salt("source-help-body")
                                        .max_height(body_height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| self.help_contents(ui));
                                    return;
                                }
                                ui.horizontal_top(|ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(LIST_WIDTH.min(width * 0.28), body_height),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.set_width(LIST_WIDTH.min(width * 0.28));
                                            ui.set_max_width(LIST_WIDTH.min(width * 0.28));
                                            ui.set_min_height(body_height);
                                            self.source_sidebar(ui, panel, body_height);
                                        },
                                    );
                                    ui.add_space(16.);
                                    let right =
                                        (width - 40. - LIST_WIDTH.min(width * 0.28) - 16.).max(80.);
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(right, body_height),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.set_width(right);
                                            ui.set_max_width(right);
                                            ui.spacing_mut().item_spacing = egui::vec2(6., 8.);
                                            ui.ctx().data_mut(|d| d.insert_temp(active_id(), true));
                                            egui::ScrollArea::vertical()
                                                .id_salt(("source-panel-body", title(panel)))
                                                .max_height(body_height)
                                                .auto_shrink([false, false])
                                                .show(ui, |ui| {
                                                    ui.set_width((right - 10.).max(60.));
                                                    if panel == Panel::Entities {
                                                        self.source_entity_form(ui);
                                                    } else if panel == Panel::Enums
                                                        && ui
                                                            .ctx()
                                                            .data_mut(|d| {
                                                                d.get_temp::<bool>(egui::Id::new(
                                                                    "source-external-enum",
                                                                ))
                                                            })
                                                            .unwrap_or(false)
                                                    {
                                                        self.source_external_enum_form(ui);
                                                    } else {
                                                        self.properties(ui, panel);
                                                    }
                                                });
                                            ui.ctx()
                                                .data_mut(|d| d.insert_temp(active_id(), false));
                                        },
                                    );
                                });
                            });
                    });
            });
        self.rule_wizard(ctx);
        self.entity_transfer_dialog(ctx);
        self.group_remap_dialog(ctx);
        self.group_policy_dialog(ctx);
        self.source_field_type_picker(ctx);
        self.source_layer_type_picker(ctx);
        if close {
            self.show_panel(None);
        }
        true
    }
    fn source_sidebar(&mut self, ui: &mut egui::Ui, panel: Panel, height: f32) {
        ui.set_width(LIST_WIDTH.min(ui.available_width()));
        ui.set_max_width(LIST_WIDTH);
        ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
        if panel == Panel::Rules {
            self.source_rule_list(ui, height);
            return;
        }
        if let Some(kind) = panel_kind(panel) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                if orange(
                    ui,
                    "⊞",
                    egui::vec2(
                        if panel == Panel::Entities {
                            165.
                        } else {
                            ui.available_width()
                        },
                        41.,
                    ),
                )
                .on_hover_text("Create definition")
                .clicked()
                {
                    if panel == Panel::Layers {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("source-layer-type-picker"), true)
                        });
                    } else {
                        self.source_create_definition(kind, None);
                    }
                }
                if panel == Panel::Entities {
                    ui.scope(|ui| {
                        ui.spacing_mut().interact_size.y = 41.;
                        ui.spacing_mut().button_padding = egui::vec2(6., 10.);
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = SOURCE_ORANGE;
                        ui.visuals_mut().widgets.inactive.bg_fill = SOURCE_ORANGE;
                        ui.menu_button(RichText::new("PRESETS").color(Color32::WHITE), |ui| {
                            if source_controls::button(ui, "Rectangle region").clicked() {
                                self.source_create_definition("entities", Some(false));
                                ui.close();
                            }
                            if source_controls::button(ui, "Circle region").clicked() {
                                self.source_create_definition("entities", Some(true));
                                ui.close();
                            }
                        });
                    });
                }
            });
            if panel == Panel::Enums {
                self.import_enum_menu(ui);
            }
            let filter_id = ui.make_persistent_id(("source-definition-search", kind));
            let mut filter = ui
                .ctx()
                .data_mut(|d| d.get_temp::<String>(filter_id))
                .unwrap_or_default();
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1_f32, theme::LIGHT))
                .corner_radius(20.)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(egui::include_image!("../../assets/icons/search.svg"))
                                .fit_to_exact_size(Vec2::splat(16.)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut filter)
                                .frame(false)
                                .hint_text("Search…")
                                .desired_width(ui.available_width()),
                        );
                    });
                });
            ui.ctx()
                .data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let definitions = self
                .project
                .as_ref()
                .and_then(|p| p.json["defs"][kind].as_array())
                .cloned()
                .unwrap_or_default();
            let mut groups = BTreeMap::<String, Vec<Value>>::new();
            for def in definitions {
                if !text(&def, "identifier", "")
                    .to_lowercase()
                    .contains(&filter.to_lowercase())
                {
                    continue;
                }
                if panel == Panel::Rules && def["type"] != "IntGrid" && def["type"] != "AutoLayer" {
                    continue;
                }
                let tags = def["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if tags.is_empty() {
                    groups
                        .entry(if panel == Panel::Entities {
                            "Other".into()
                        } else {
                            kind.to_owned()
                        })
                        .or_default()
                        .push(def);
                } else {
                    for tag in tags {
                        groups.entry(tag).or_default().push(def.clone());
                    }
                }
            }
            egui::ScrollArea::vertical()
                .id_salt(("source-definition-list", kind))
                .max_height((height - 88.).max(40.))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (name, definitions) in groups {
                        ui.set_width(LIST_WIDTH);
                        self.source_definition_group(ui, kind, &name, &definitions, false);
                    }
                    if panel == Panel::Enums {
                        let external = self
                            .project
                            .as_ref()
                            .and_then(|p| p.json["defs"]["externalEnums"].as_array())
                            .cloned()
                            .unwrap_or_default();
                        let mut sources = BTreeMap::<String, Vec<Value>>::new();
                        for def in external {
                            if text(&def, "identifier", "")
                                .to_lowercase()
                                .contains(&filter.to_lowercase())
                            {
                                sources
                                    .entry(
                                        text(&def, "externalRelPath", "External enums").to_owned(),
                                    )
                                    .or_default()
                                    .push(def);
                            }
                        }
                        for (path, defs) in sources {
                            let name = path.rsplit(['/', '\\']).next().unwrap_or(&path);
                            self.source_definition_group(ui, "externalEnums", name, &defs, true);
                        }
                    }
                });
        } else if panel == Panel::Fields {
            self.source_field_list(ui, self.field_owner, height);
        } else {
            for (label, target) in [
                ("Project settings", Panel::Project),
                ("Worlds", Panel::World),
                ("Levels", Panel::Level),
                ("Layers", Panel::Layers),
                ("Entities", Panel::Entities),
                ("Enums", Panel::Enums),
                ("Tilesets", Panel::Tilesets),
                ("Auto-layer rules", Panel::Rules),
                ("Export", Panel::Export),
            ] {
                if source_controls::add_sized_button(
                    ui,
                    [ui.available_width(), 36.],
                    egui::Button::new(label).selected(panel == target),
                )
                .clicked()
                {
                    self.show_panel(Some(target));
                }
            }
            if panel == Panel::Level || panel == Panel::World {
                let levels = self
                    .project
                    .as_ref()
                    .map(|p| {
                        p.level_keys()
                            .into_iter()
                            .filter_map(|key| {
                                p.level(key)
                                    .map(|l| (key, text(l, "identifier", "Level").to_owned()))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                ui.separator();
                for (key, name) in levels {
                    if ui
                        .selectable_label(self.selected == Some(key), name)
                        .clicked()
                    {
                        self.navigate_level(key, true);
                    }
                }
            }
        }
    }
    fn source_definition_group(
        &mut self,
        ui: &mut egui::Ui,
        kind: &str,
        name: &str,
        definitions: &[Value],
        external: bool,
    ) {
        ui.set_width(LIST_WIDTH);
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(4)
            .show(ui, |ui| {
                let source_kind = match kind {
                    "entities" => "entity",
                    "tilesets" => "tileset",
                    "enums" => "enum",
                    _ => kind,
                };
                let key = format!(
                    "{}_{}_tag_{}",
                    self.project
                        .as_ref()
                        .map(|p| text(&p.json, "iid", ""))
                        .unwrap_or(""),
                    source_kind,
                    if name == "Other" { "null" } else { name }
                );
                let mut open = self.settings.ui_state_int(&key, None, 1) != 0;
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.), Sense::click());
                ui.painter().rect_filled(rect, 0., GROUP_BAR);
                let points = if open {
                    [
                        egui::vec2(7., -3.),
                        egui::vec2(12., 2.),
                        egui::vec2(17., -3.),
                    ]
                } else {
                    [egui::vec2(9., -5.), egui::vec2(14., 0.), egui::vec2(9., 5.)]
                };
                for line in points.windows(2) {
                    ui.painter().line_segment(
                        [rect.left_center() + line[0], rect.left_center() + line[1]],
                        Stroke::new(2_f32, Color32::WHITE),
                    );
                }
                ui.painter().text(
                    rect.left_center() + egui::vec2(24., 0.),
                    Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    FontId::new(14., egui::FontFamily::Name("panel-title".into())),
                    theme::ACCENT,
                );
                if response.clicked() {
                    open = !open;
                    self.settings.set_ui_state_int(&key, None, i64::from(open));
                }
                if open {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(4, 4))
                        .show(ui, |ui| {
                            for def in definitions {
                                let uid = def["uid"].as_i64().unwrap_or(-1);
                                let selected = self.definition_uid == Some(uid);
                                let color = theme::color(text(def, "color", "#687487"));
                                let fill = if selected {
                                    tint(color, Color32::WHITE, 0.65)
                                } else {
                                    tint(theme::MEDIUM, color, 0.22)
                                };
                                let item = egui::Frame::new()
                                    .fill(fill)
                                    .inner_margin(egui::Margin::symmetric(4, 0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let handle = ui.add(
                                                egui::Label::new(
                                                    RichText::new("≡").color(theme::TEXT),
                                                )
                                                .sense(Sense::drag()),
                                            );
                                            if !external {
                                                handle.dnd_set_drag_payload((kind.to_owned(), uid));
                                            }
                                            let (rect, _) = ui.allocate_exact_size(
                                                Vec2::splat(32.),
                                                Sense::hover(),
                                            );
                                            self.source_definition_thumbnail(
                                                ui, def, kind, rect, color,
                                            );
                                            let available = (ui.available_width() - 42.).max(20.);
                                            let response = source_controls::add_sized_button(
                                                ui,
                                                [available, 32.],
                                                egui::Button::new(
                                                    RichText::new(text(
                                                        def,
                                                        "identifier",
                                                        "Definition",
                                                    ))
                                                    .color(if selected {
                                                        Color32::BLACK
                                                    } else {
                                                        tint(color, Color32::WHITE, 0.4)
                                                    }),
                                                )
                                                .truncate()
                                                .frame(false),
                                            );
                                            if response.clicked() {
                                                self.clipboard_focus(
                                                    ui.ctx(),
                                                    ClipTarget::Definition {
                                                        kind: kind.to_owned(),
                                                        uid: Some(uid),
                                                    },
                                                );
                                                self.definition_uid = Some(uid);
                                                if kind == "entities" {
                                                    self.entity_uid = Some(uid);
                                                    self.field_owner = Some(uid);
                                                    self.field_uid = None;
                                                }
                                                ui.ctx().data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new("source-external-enum"),
                                                        external,
                                                    )
                                                });
                                            }
                                            source_menu(ui, |ui| {
                                                if external {
                                                    if source_controls::button(ui, "Sync source…")
                                                        .clicked()
                                                    {
                                                        let base = self
                                                            .project
                                                            .as_ref()
                                                            .unwrap()
                                                            .path
                                                            .as_ref()
                                                            .and_then(|p| p.parent())
                                                            .unwrap_or(std::path::Path::new("."));
                                                        self.begin_enum_sync(
                                                            ui.ctx(),
                                                            base.join(text(
                                                                def,
                                                                "externalRelPath",
                                                                "",
                                                            )),
                                                        );
                                                        ui.close();
                                                    }
                                                } else {
                                                    self.clipboard_menu(
                                                        ui,
                                                        ClipTarget::Definition {
                                                            kind: kind.to_owned(),
                                                            uid: Some(uid),
                                                        },
                                                    );
                                                    if source_controls::button(ui, "Move up")
                                                        .clicked()
                                                    {
                                                        let index =
                                                            self.definition_index(kind, uid);
                                                        self.source_reorder_definition(
                                                            kind,
                                                            uid,
                                                            index.saturating_sub(1),
                                                        );
                                                        ui.close();
                                                    }
                                                    if source_controls::button(ui, "Move down")
                                                        .clicked()
                                                    {
                                                        let index =
                                                            self.definition_index(kind, uid);
                                                        let count = self
                                                            .project
                                                            .as_ref()
                                                            .unwrap()
                                                            .json["defs"][kind]
                                                            .as_array()
                                                            .map_or(0, Vec::len);
                                                        self.source_reorder_definition(
                                                            kind,
                                                            uid,
                                                            (index + 1)
                                                                .min(count.saturating_sub(1)),
                                                        );
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        });
                                    });
                                if let Some(payload) =
                                    item.response.dnd_hover_payload::<(String, i64)>()
                                    && payload.0 == kind
                                    && payload.1 != uid
                                {
                                    ui.painter().hline(
                                        item.response.rect.x_range(),
                                        item.response.rect.top(),
                                        Stroke::new(2_f32, theme::ACCENT),
                                    );
                                }
                                if let Some(payload) =
                                    item.response.dnd_release_payload::<(String, i64)>()
                                    && payload.0 == kind
                                    && payload.1 != uid
                                {
                                    self.source_reorder_definition(
                                        kind,
                                        payload.1,
                                        self.definition_index(kind, uid),
                                    );
                                }
                                ui.add_space(3.);
                            }
                        });
                }
            });
        ui.add_space(8.);
    }
    fn source_reorder_definition(&mut self, kind: &str, uid: i64, index: usize) {
        self.edit("Definition reordered", |p| {
            p.reorder_definition(kind, uid, index)
        });
    }
    pub(super) fn source_definition_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
        kind: &str,
        rect: Rect,
        color: Color32,
    ) {
        let tile = if def["uiTileRect"].is_object() {
            &def["uiTileRect"]
        } else {
            &def["tileRect"]
        };
        let atlas = tile["tilesetUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.tileset(uid))
            .cloned();
        if let Some(atlas) = atlas {
            let texture = if atlas["embedAtlas"].is_null() {
                atlas["relPath"].as_str().and_then(|rel| {
                    self.assets.texture(
                        ui.ctx(),
                        &canvas::asset_path(self.project.as_ref().unwrap(), rel),
                    )
                })
            } else {
                self.assets.embedded(ui.ctx())
            };
            if let Some(texture) = texture {
                let size = texture.size_vec2();
                let uv = Rect::from_min_max(
                    egui::pos2(
                        number(tile, "x", 0.) / size.x,
                        number(tile, "y", 0.) / size.y,
                    ),
                    egui::pos2(
                        (number(tile, "x", 0.) + number(tile, "w", 16.)) / size.x,
                        (number(tile, "y", 0.) + number(tile, "h", 16.)) / size.y,
                    ),
                );
                ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
                ui.painter().rect_stroke(
                    rect,
                    0.,
                    Stroke::new(1_f32, color.gamma_multiply(0.6)),
                    StrokeKind::Inside,
                );
                return;
            }
        }
        ui.painter().rect_filled(
            rect.shrink(2.),
            0.,
            color.gamma_multiply(if kind == "entities" { 0.5 } else { 0.8 }),
        );
        ui.painter().rect_stroke(
            rect.shrink(2.),
            0.,
            Stroke::new(2_f32, color),
            StrokeKind::Inside,
        );
    }
    fn source_create_definition(&mut self, kind: &str, preset: Option<bool>) {
        if kind == "tilesets" {
            if let Some(path) = self.pick_project_file(
                "PickImage",
                &["png", "jpg", "jpeg", "gif", "ase", "aseprite"],
            ) {
                match ldtk_core::exports::load_image_rgba(&path) {
                    Ok(image) => {
                        let grid = self.new_grid;
                        if let Some(uid) = self.edit("Tileset imported", |p| {
                            p.add_tileset(&path, grid, image.width() as i32, image.height() as i32)
                        }) {
                            self.definition_uid = Some(uid);
                        }
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_error = true;
                    }
                }
            }
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let base = match (kind, preset) {
            ("entities", Some(true)) => "CircleRegion",
            ("entities", Some(false)) => "RectRegion",
            ("entities", None) => "Entity",
            ("layers", _) => "Layer",
            _ => "Enum",
        };
        let name = unique_name(
            base,
            project.json["defs"][kind]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| text(d, "identifier", "")),
        );
        let used = project.json["defs"][if kind == "entities" {
            "entities"
        } else {
            "enums"
        }]
        .as_array()
        .cloned()
        .unwrap_or_default();
        let color = self.suggested_color(&used);
        let result=match kind{"entities"=>self.edit("Entity created",|p|{let uid=p.add_entity_def(&name,number(&p.json,"defaultEntityWidth",16.)as i32,number(&p.json,"defaultEntityHeight",16.)as i32)?;p.update_entity_definition(uid,&json!({"color":format!("#{color:06X}")}))?;if let Some(circle)=preset{p.update_entity_definition(uid,&json!({"hollow":true,"resizableX":true,"resizableY":true,"keepAspectRatio":circle,"pivotX":if circle{0.5}else{0.},"pivotY":if circle{0.5}else{0.},"renderMode":if circle{"Ellipse"}else{"Rectangle"},"tags":["region"]}))?;}Ok(uid)}),"layers"=>{let kind=self.new_kind.clone();let grid=self.new_grid;self.edit("Layer created",|p|p.add_layer(&name,&kind,grid))},_=>self.edit("Enum created",|p|p.add_enum_definition(&name,&[]))};
        if let Some(uid) = result {
            self.definition_uid = Some(uid);
            if kind == "entities" {
                self.entity_uid = Some(uid);
                self.field_owner = Some(uid);
                self.field_uid = None;
            }
        }
    }
    fn source_external_enum_form(&mut self, ui: &mut egui::Ui) {
        let Some(def) = self
            .project
            .as_ref()
            .and_then(|p| p.json["defs"]["externalEnums"].as_array())
            .and_then(|a| a.iter().find(|d| d["uid"].as_i64() == self.definition_uid))
            .cloned()
        else {
            return;
        };
        ui.heading(text(&def, "identifier", "External enum"));
        ui.label(text(&def, "externalRelPath", ""));
        if source_controls::button(ui, "Synchronize source…").clicked() {
            let base = self
                .project
                .as_ref()
                .unwrap()
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .unwrap_or(std::path::Path::new("."));
            self.begin_enum_sync(ui.ctx(), base.join(text(&def, "externalRelPath", "")));
        }
        let remove_id = egui::Id::new(("remove-external-source", def["uid"].as_i64()));
        if source_controls::button(ui, "Remove this external source…").clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(remove_id, true));
        }
        if ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(remove_id))
            .unwrap_or(false)
        {
            let mut remove = false;
            let mut cancel = false;
            egui::Modal::new(remove_id).show(ui.ctx(),|ui|{ui.heading("Remove external enum source");ui.label("Removing this source removes all enums imported from the same file and their custom fields in every level. The source file stays on disk.");ui.horizontal(|ui|{if source_controls::button(ui,"Remove all enums from this source").clicked(){remove=true;}
if source_controls::button(ui,"Cancel").clicked(){cancel=true;}});});
            if remove {
                let rel = text(&def, "externalRelPath", "");
                self.edit_recoverable("External source enums deleted", |p| {
                    p.remove_external_enum_source(rel)
                });
                self.definition_uid = None;
            }
            if remove || cancel {
                ui.ctx().data_mut(|d| d.remove::<bool>(remove_id));
            }
        }
        for value in def["values"].as_array().into_iter().flatten() {
            ui.label(text(value, "id", ""));
        }
    }
    fn source_entity_form(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let key = ui.make_persistent_id(("entity-options-atlas", self.definition_uid));
        self.with_local_atlas_state(&ctx, key, |app| app.source_entity_form_inner(ui));
    }
    fn source_entity_form_inner(&mut self, ui: &mut egui::Ui) {
        let Some(def) = self.definition_picker(ui, "entities") else {
            ui.label("Create an entity using the + button.");
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        if self.field_owner != Some(uid) {
            self.field_owner = Some(uid);
            self.field_uid = None;
        }
        let mut patch = json!({});
        row_width(ui, "Entity identifier", 108., |ui| {
            if let Some(name) = source_text(
                ui,
                ("entity-name", uid),
                text(&def, "identifier", ""),
                (ui.available_width() - 150.).max(80.),
            ) {
                patch["identifier"] = json!(name);
            }
            let mut show = def["showName"].as_bool().unwrap_or(true);
            if source_checkbox(ui, &mut show, "Show identifier").changed() {
                patch["showName"] = json!(show);
            }
        });
        ui.add_space(5.);
        let mut atlas = ui
            .ctx()
            .data_mut(|d| d.get_temp::<i64>(egui::Id::new(("entity-atlas", uid))))
            .or_else(|| def["tilesetId"].as_i64())
            .or_else(|| def["tileRect"]["tilesetUid"].as_i64())
            .unwrap_or(-1);
        let mut advanced = ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(egui::Id::new(("entity-advanced", uid))))
            .unwrap_or(false);
        let settings_open = self.settings.ui_state_int("extraEntityDetails", None, 1) != 0;
        let settings_next = section(ui, "ENTITY SETTINGS", settings_open, |ui| {
            ui.spacing_mut().item_spacing.y = 12.;
            row(ui, "Misc options", |ui| {
                for (key, label) in [
                    ("exportToToc", "Add to JSON Table-of-content"),
                    ("allowOutOfBounds", "Can be out of level bounds"),
                ] {
                    let mut value = def[key].as_bool().unwrap_or(false);
                    if source_checkbox(ui, &mut value, label).changed() {
                        patch[key] = json!(value);
                    }
                }
            });
            row(ui, "Documentation", |ui| {
                let mut doc = text(&def, "doc", "")
                    .replace("\\\\", "\\")
                    .replace("\\n", "\n");
                if ui
                    .add_sized(
                        [ui.available_width(), 32.],
                        egui::TextEdit::multiline(&mut doc).desired_rows(1),
                    )
                    .changed()
                {
                    patch["doc"] = json!(doc.replace('\\', "\\\\").replace('\n', "\\n"));
                }
            });
            row(ui, "Tags", |ui| {
                let mut tags = def["tags"].as_array().cloned().unwrap_or_default();
                let mut remove = None;
                for (index, tag) in tags.iter().enumerate() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(165, 119, 79))
                        .corner_radius(7.)
                        .inner_margin(egui::Margin::symmetric(5, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(tag.as_str().unwrap_or(""))
                                        .italics()
                                        .color(Color32::WHITE),
                                );
                                if source_controls::small_button(ui, "×").clicked() {
                                    remove = Some(index);
                                }
                            });
                        });
                }
                if let Some(index) = remove {
                    tags.remove(index);
                    patch["tags"] = json!(tags);
                }
                ui.menu_button("+", |ui| {
                    let key = ui.make_persistent_id(("new-entity-tag", uid));
                    let mut value = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<String>(key))
                        .unwrap_or_default();
                    ui.text_edit_singleline(&mut value);
                    ui.ctx().data_mut(|d| d.insert_temp(key, value.clone()));
                    if source_controls::button(ui, "Add tag").clicked() && !value.trim().is_empty()
                    {
                        let value = json!(value.trim());
                        if !tags.contains(&value) {
                            tags.push(value);
                        }
                        patch["tags"] = json!(tags);
                        ui.close();
                    }
                });
                ui.menu_button("⌄", |ui| {
                    let existing = self.project.as_ref().unwrap().json["defs"]["entities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|d| d["tags"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .collect::<std::collections::BTreeSet<_>>();
                    for tag in existing {
                        let mut active = tags.contains(&json!(tag));
                        if ui.checkbox(&mut active, tag).changed() {
                            tags.retain(|v| v != tag);
                            if active {
                                tags.push(json!(tag));
                            }
                            patch["tags"] = json!(tags);
                        }
                    }
                });
            });
            row(ui, "Size", |ui| {
                for (key, label) in [("width", ""), ("height", "×")] {
                    if !label.is_empty() {
                        ui.label(label);
                    }
                    let mut value = def[key].as_i64().unwrap_or(16);
                    if ui
                        .add_sized(
                            [60., 30.],
                            egui::DragValue::new(&mut value).range(1..=65536),
                        )
                        .changed()
                    {
                        patch[key] = json!(value);
                    }
                }
            });
            if def["resizableX"] == true || def["resizableY"] == true {
                row(ui, "", |ui| {
                    for (key, label, enabled) in [
                        ("minWidth", "Min:", def["resizableX"] == true),
                        ("minHeight", "×", def["resizableY"] == true),
                        ("maxWidth", "Max:", def["resizableX"] == true),
                        ("maxHeight", "×", def["resizableY"] == true),
                    ] {
                        ui.label(label);
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.scope(|ui| {
                                ui.spacing_mut().text_edit_width = 45.;
                                let current =
                                    def[key].as_i64().map(|v| v.to_string()).unwrap_or_default();
                                let id = ui.id().with((uid, key));
                                let mut draft = ui
                                    .data_mut(|d| d.get_temp::<String>(id))
                                    .unwrap_or(current.clone());
                                let response = ui.add_sized(
                                    [48., 30.],
                                    egui::TextEdit::singleline(&mut draft).hint_text("None"),
                                );
                                if response.lost_focus()
                                    || response.has_focus()
                                        && ui.input(|i| i.key_pressed(Key::Enter))
                                {
                                    if draft.trim().is_empty() {
                                        patch[key] = Value::Null;
                                    } else if let Ok(n) = draft.parse::<i64>() {
                                        patch[key] = if n > 0 { json!(n) } else { Value::Null };
                                    }
                                }
                                ui.data_mut(|d| d.insert_temp(id, draft));
                            });
                        });
                    }
                });
            }
            row(ui, "", |ui| {
                ui.label("Resizable:");
                for (key, label) in [
                    ("resizableX", "Width"),
                    ("resizableY", "Height"),
                    ("keepAspectRatio", "Keep aspect ratio"),
                ] {
                    let mut value = def[key].as_bool().unwrap_or(false);
                    if source_checkbox(ui, &mut value, label).changed() {
                        patch[key] = json!(value);
                    }
                }
            });
            row(ui, "Editor visual", |ui| {
                let mut rgb = enum_color(&def);
                if source_colors::color_editor(ui, &mut rgb).changed() {
                    patch["color"] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                }
                let options = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut visual = if text(&def, "renderMode", "Rectangle") == "Tile" {
                    atlas
                } else {
                    match text(&def, "renderMode", "Rectangle") {
                        "Ellipse" => -2,
                        "Cross" => -3,
                        _ => -1,
                    }
                };
                let old_visual = visual;
                let name = if visual < 0 {
                    match visual {
                        -2 => "Ellipse",
                        -3 => "Cross",
                        _ => "Rectangle",
                    }
                } else {
                    options
                        .iter()
                        .find(|d| d["uid"] == visual)
                        .map(|d| {
                            if d["embedAtlas"].is_null() {
                                text(d, "identifier", "Tileset")
                            } else {
                                "Internal icons by FinalBossB"
                            }
                        })
                        .unwrap_or("Choose tileset")
                };
                egui::ComboBox::from_id_salt(("source-entity-atlas", uid))
                    .selected_text(name)
                    .width(200.)
                    .show_ui(ui, |ui| {
                        for (id, name) in [(-1, "Rectangle"), (-2, "Ellipse"), (-3, "Cross")] {
                            ui.selectable_value(&mut visual, id, name);
                        }
                        ui.separator();
                        for d in &options {
                            ui.selectable_value(
                                &mut visual,
                                d["uid"].as_i64().unwrap_or(-1),
                                text(d, "identifier", "Tileset"),
                            );
                        }
                    });
                if visual != old_visual {
                    patch["tileRect"] = Value::Null;
                    patch["renderMode"] = json!(if visual >= 0 {
                        "Tile"
                    } else {
                        match visual {
                            -2 => "Ellipse",
                            -3 => "Cross",
                            _ => "Rectangle",
                        }
                    });
                    patch["tilesetId"] = if visual >= 0 {
                        json!(visual)
                    } else {
                        Value::Null
                    };
                    if visual >= 0 {
                        atlas = visual;
                    }
                    if (visual >= 0) != (old_visual >= 0) {
                        patch["tileOpacity"] = json!(1);
                        patch["fillOpacity"] = json!(if visual >= 0 { 0.08 } else { 1. });
                        patch["lineOpacity"] = json!(if visual >= 0 { 0. } else { 1. });
                    }
                }
                if visual < 0 {
                    return;
                }
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                self.source_definition_thumbnail(
                    ui,
                    &def,
                    "entities",
                    rect,
                    theme::color(text(&def, "color", "#FFFFFF")),
                );
                if response.clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new(("entity-tile-picker", uid)), true)
                    });
                }
                let mut mode = text(&def, "tileRenderMode", "FitInside").to_owned();
                let old = mode.clone();
                egui::ComboBox::from_id_salt(("source-entity-fit", uid))
                    .selected_text(match mode.as_str() {
                        "FitInside" => "Fit inside bounds",
                        "Stretch" => "Stretch",
                        "Cover" => "Cover bounds",
                        "Repeat" => "Repeat",
                        "NineSlice" => "Nine slice",
                        _ => &mode,
                    })
                    .width(178.)
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("FitInside", "Fit inside bounds"),
                            ("Stretch", "Stretch"),
                            ("Cover", "Cover bounds"),
                            ("Repeat", "Repeat"),
                            ("FullSizeCropped", "Full size cropped"),
                            ("FullSizeUncropped", "Full size uncropped"),
                            ("NineSlice", "Nine slice"),
                        ] {
                            ui.selectable_value(&mut mode, value.to_owned(), label);
                        }
                    });
                if mode != old {
                    patch["tileRenderMode"] = json!(mode);
                }
            });
            row(ui, "", |ui| {
                for (key, label) in [
                    ("tileOpacity", "Tile="),
                    ("fillOpacity", "Fill="),
                    ("lineOpacity", "Line="),
                ] {
                    ui.label(label);
                    let mut percent = number(&def, key, 1.) * 100.;
                    if ui
                        .add(
                            egui::DragValue::new(&mut percent)
                                .range(0.0..=100.0)
                                .suffix(" %"),
                        )
                        .changed()
                    {
                        patch[key] = json!(percent / 100.);
                    }
                }
                let mut hollow = def["hollow"].as_bool().unwrap_or(false);
                if source_checkbox(ui, &mut hollow, "Hollow").changed() {
                    patch["hollow"] = json!(hollow);
                }
            });
            if text(&def, "tileRenderMode", "") == "NineSlice" {
                row(ui, "9-slices borders", |ui| {
                    let mut borders = def["nineSliceBorders"]
                        .as_array()
                        .cloned()
                        .unwrap_or_else(|| vec![json!(2); 4]);
                    borders.resize(4, json!(2));
                    let mut any = false;
                    for (i, label) in ["Top", "Right", "Bottom", "Left"].iter().enumerate() {
                        ui.label(*label);
                        let mut n = borders[i].as_i64().unwrap_or(2);
                        if ui
                            .add_sized([42., 30.], egui::DragValue::new(&mut n).range(0..=65536))
                            .changed()
                        {
                            borders[i] = json!(n);
                            any = true;
                        }
                    }
                    if any {
                        patch["nineSliceBorders"] = json!(borders);
                    }
                });
            }
            row(ui, "Alt UI icon", |ui| {
                let mut chosen = def["uiTileRect"]["tilesetUid"].as_i64().unwrap_or(-1);
                let previous = chosen;
                let options = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt(("entity-ui-tileset", uid))
                    .width(240.)
                    .selected_text(
                        options
                            .iter()
                            .find(|d| d["uid"] == chosen)
                            .map(|d| text(d, "identifier", "Tileset"))
                            .unwrap_or("Use default editor visual"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut chosen, -1, "Use default editor visual");
                        for def in &options {
                            ui.selectable_value(
                                &mut chosen,
                                def["uid"].as_i64().unwrap_or(-1),
                                text(def, "identifier", "Tileset"),
                            );
                        }
                    });
                if previous != chosen {
                    patch["uiTileRect"] = if chosen < 0 {
                        Value::Null
                    } else {
                        json!({"tilesetUid":chosen,"x":0,"y":0,"w":0,"h":0})
                    };
                }
                if chosen >= 0 && source_controls::button(ui, "Pick tile…").clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::new(("entity-ui-picker", uid)), true)
                    });
                }
            });
            row(ui, "Pivot point", |ui| {
                let (grid, _) = ui.allocate_exact_size(Vec2::splat(60.), Sense::hover());
                for y in 0..3 {
                    for x in 0..3 {
                        let px = x as f64 / 2.;
                        let py = y as f64 / 2.;
                        let selected = def["pivotX"].as_f64() == Some(px)
                            && def["pivotY"].as_f64() == Some(py);
                        let rect = Rect::from_min_size(
                            grid.min + egui::vec2(x as f32 * 20., y as f32 * 20.),
                            Vec2::splat(20.),
                        );
                        ui.painter().rect_filled(
                            rect,
                            0.,
                            if selected {
                                theme::ACCENT
                            } else {
                                theme::MEDIUM
                            },
                        );
                        ui.painter().rect_stroke(
                            rect,
                            0.,
                            Stroke::new(1_f32, GROUP_BAR),
                            StrokeKind::Inside,
                        );
                        if ui
                            .interact(rect, ui.id().with(("pivot", x, y)), Sense::click())
                            .clicked()
                        {
                            patch["pivotX"] = json!(px);
                            patch["pivotY"] = json!(py);
                        }
                    }
                }
                if ui
                    .link(
                        RichText::new(if advanced {
                            "Hide advanced options"
                        } else {
                            "Show advanced options"
                        })
                        .color(theme::ACCENT)
                        .underline(),
                    )
                    .clicked()
                {
                    advanced = !advanced;
                }
            });
            if advanced {
                row(ui, "", |ui| {
                    ui.label("Float:");
                    for (key, size) in [
                        ("pivotX", number(&def, "width", 16.)),
                        ("pivotY", number(&def, "height", 16.)),
                    ] {
                        let mut value = number(&def, key, 0.);
                        if ui
                            .add_sized(
                                [64., 30.],
                                egui::DragValue::new(&mut value)
                                    .speed(0.01)
                                    .range(0.0..=1.0),
                            )
                            .changed()
                        {
                            patch[key] = json!(value);
                        }
                        let _ = size;
                    }
                    ui.label("OR Pixels:");
                    for (key, size) in [
                        ("pivotX", number(&def, "width", 16.)),
                        ("pivotY", number(&def, "height", 16.)),
                    ] {
                        let mut value = number(&def, key, 0.) * size;
                        if ui
                            .add_sized(
                                [64., 30.],
                                egui::DragValue::new(&mut value).range(0.0..=size),
                            )
                            .changed()
                        {
                            patch[key] = json!(value / size.max(1.));
                        }
                    }
                });
            }
            row(ui, "Max count", |ui| {
                let mut count = def["maxCount"].as_i64().unwrap_or(0);
                if ui
                    .add_sized(
                        [60., 30.],
                        egui::DragValue::new(&mut count).range(0..=1_000_000),
                    )
                    .changed()
                {
                    patch["maxCount"] = json!(count);
                }
                let mut scope = text(&def, "limitScope", "PerLevel").to_owned();
                let old = scope.clone();
                egui::ComboBox::from_id_salt(("source-limit-scope", uid))
                    .selected_text(match scope.as_str() {
                        "PerWorld" => "in the world",
                        "PerLayer" => "in the layer",
                        _ => "in the level",
                    })
                    .width(92.)
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("PerLevel", "in the level"),
                            ("PerLayer", "in the layer"),
                            ("PerWorld", "in the world"),
                        ] {
                            ui.selectable_value(&mut scope, value.to_owned(), label);
                        }
                    });
                if scope != old {
                    patch["limitScope"] = json!(scope);
                }
                ui.label(", and");
                let mut behavior = text(&def, "limitBehavior", "MoveLastOne").to_owned();
                let old = behavior.clone();
                egui::ComboBox::from_id_salt(("source-limit-behavior", uid))
                    .selected_text(match behavior.as_str() {
                        "MoveLastOne" => "move the last one",
                        "PreventAdding" => "prevent adding",
                        _ => "discard old ones",
                    })
                    .width(108.)
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            ("MoveLastOne", "move the last one"),
                            ("PreventAdding", "prevent adding"),
                            ("DiscardOldOnes", "discard old ones"),
                        ] {
                            ui.selectable_value(&mut behavior, value.to_owned(), label);
                        }
                    });
                if old != behavior {
                    patch["limitBehavior"] = json!(behavior);
                }
                ui.add(egui::Label::new("when max is reached.").truncate())
                    .on_hover_text("when max is reached.");
            });
        });
        if settings_next != settings_open {
            self.settings
                .set_ui_state_int("extraEntityDetails", None, i64::from(settings_next));
        }
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::new(("entity-advanced", uid)), advanced);
            d.insert_temp(egui::Id::new(("entity-atlas", uid)), atlas);
        });
        if changed(&patch) {
            self.edit("Entity definition changed", |p| {
                p.update_entity_definition(uid, &patch)
            });
        }
        let picker_id = egui::Id::new(("entity-tile-picker", uid));
        let mut picker = ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(picker_id))
            .unwrap_or(false);
        let picker_ctx = ui.ctx().clone();
        self.with_local_atlas_state(&picker_ctx, picker_id, |app| {
            if picker {
                let mut apply = false;
                egui::Window::new("Entity tile")
                    .id(picker_id)
                    .open(&mut picker)
                    .default_width(440.)
                    .show(ui.ctx(), |ui| {
                        if atlas >= 0 {
                            app.atlas_rect_picker(&ui.ctx().clone(), ui, atlas);
                            if source_controls::button(ui, "Use selected tile").clicked() {
                                apply = true;
                            }
                        } else {
                            ui.label("Choose a tileset in Editor visual first.");
                        }
                    });
                if apply && let Some(rect) = app.selected_tile_rect_for(atlas) {
                    app.edit("Entity tile changed", |p| {
                        p.update_entity_definition(
                            uid,
                            &json!({"tilesetId":atlas,"tileRect":rect,"renderMode":"Tile"}),
                        )
                    });
                    picker = false;
                }
                ui.ctx().data_mut(|d| d.insert_temp(picker_id, picker));
            }
        });
        let ui_picker_id = egui::Id::new(("entity-ui-picker", uid));
        let mut ui_picker = ui
            .ctx()
            .data_mut(|d| d.get_temp::<bool>(ui_picker_id))
            .unwrap_or(false);
        let picker_ctx = ui.ctx().clone();
        self.with_local_atlas_state(&picker_ctx, ui_picker_id, |app| {
            if ui_picker {
                let mut apply = false;
                let ui_atlas = def["uiTileRect"]["tilesetUid"].as_i64().unwrap_or(-1);
                egui::Window::new("Alternate UI icon")
                    .id(ui_picker_id)
                    .open(&mut ui_picker)
                    .default_width(440.)
                    .show(ui.ctx(), |ui| {
                        if ui_atlas >= 0 {
                            app.atlas_rect_picker(&ui.ctx().clone(), ui, ui_atlas);
                            if source_controls::button(ui, "Use selected tile").clicked() {
                                apply = true;
                            }
                        }
                    });
                if apply && let Some(rect) = app.selected_tile_rect_for(ui_atlas) {
                    app.edit("Alternate UI icon changed", |p| {
                        p.update_entity_definition(uid, &json!({"uiTileRect":rect}))
                    });
                    ui_picker = false;
                }
                ui.ctx()
                    .data_mut(|d| d.insert_temp(ui_picker_id, ui_picker));
            }
        });
        ui.add_space(24.);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(200., 400.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(200.);
                    ui.set_max_width(200.);
                    self.source_field_list(ui, Some(uid), 400.);
                },
            );
            ui.add_space(8.);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 400.),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(
                        RichText::new("GENERAL SETTINGS")
                            .size(18.)
                            .color(theme::ACCENT),
                    );
                    ui.separator();
                    ui.spacing_mut().text_edit_width = 170.;
                    self.field_properties(ui);
                },
            );
        });
    }
    fn source_field_list(&mut self, ui: &mut egui::Ui, owner: Option<i64>, height: f32) {
        let fields = self
            .project
            .as_ref()
            .map(|p| {
                owner
                    .and_then(|uid| {
                        p.json["defs"]["entities"]
                            .as_array()?
                            .iter()
                            .find(|e| e["uid"] == uid)
                    })
                    .map(|e| e["fieldDefs"].clone())
                    .unwrap_or_else(|| p.json["defs"]["levelFields"].clone())
            })
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !fields.iter().any(|f| f["uid"].as_i64() == self.field_uid) {
            self.field_uid = fields.first().and_then(|f| f["uid"].as_i64());
        }
        ui.horizontal(|ui| {
            for (array, label) in [(false, "+ SINGLE VALUE"), (true, "+ ARRAY")] {
                if orange(
                    ui,
                    label,
                    egui::vec2(
                        if array {
                            70.
                        } else {
                            (ui.available_width() - 74.).max(90.)
                        },
                        34.,
                    ),
                )
                .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("source-field-type-picker"),
                            (owner, array, None::<i64>, None::<String>),
                        )
                    });
                }
            }
        });
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(4)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("source-field-list", owner))
                    .max_height((height - 42.).max(40.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, field) in fields.iter().enumerate() {
                            let uid = field["uid"].as_i64().unwrap_or(-1);
                            let selected = self.field_uid == Some(uid);
                            let kind = text(field, "__type", "Int")
                                .trim_start_matches("Array<")
                                .trim_end_matches('>');
                            let (badge, color) = match kind {
                                "Int" | "Float" => ("123", "#50b3cb"),
                                "Bool" => ("", "#cd88dd"),
                                "String" | "Multilines" => ("Abc", "#bd5f32"),
                                "Color" => ("#", "#99d367"),
                                "Tile" => ("Tile", "#99d367"),
                                "Point" => ("XY", "#7779c9"),
                                "FilePath" => ("File", "#7779c9"),
                                "EntityRef" => ("Ref", "#7779c9"),
                                _ => ("Enu", "#ff4b4b"),
                            };
                            let item = egui::Frame::new()
                                .fill(if selected {
                                    theme::ACCENT
                                } else {
                                    theme::MEDIUM
                                })
                                .inner_margin(egui::Margin::symmetric(5, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new("≡").color(
                                                if selected {
                                                    Color32::BLACK
                                                } else {
                                                    theme::TEXT
                                                },
                                            ))
                                            .sense(Sense::drag()),
                                        )
                                        .dnd_set_drag_payload((owner, uid));
                                        if source_controls::add_sized_button(
                                            ui,
                                            [(ui.available_width() - 94.).max(25.), 30.],
                                            egui::Button::new(
                                                RichText::new(text(field, "identifier", "Field"))
                                                    .color(if selected {
                                                        Color32::BLACK
                                                    } else {
                                                        Color32::WHITE
                                                    }),
                                            )
                                            .truncate()
                                            .frame(false),
                                        )
                                        .clicked()
                                        {
                                            self.clipboard_focus(
                                                ui.ctx(),
                                                ClipTarget::Field {
                                                    owner,
                                                    uid: Some(uid),
                                                },
                                            );
                                            self.field_uid = Some(uid);
                                        }
                                        egui::Frame::new()
                                            .fill(theme::color(color).gamma_multiply(0.7))
                                            .corner_radius(3.)
                                            .inner_margin(4)
                                            .show(ui, |ui| {
                                                if kind == "Bool" {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        egui::vec2(20., 20.),
                                                        Sense::hover(),
                                                    );
                                                    ui.painter().add(egui::epaint::Shape::line(
                                                        vec![
                                                            rect.min + egui::vec2(3., 10.),
                                                            rect.min + egui::vec2(8., 15.),
                                                            rect.min + egui::vec2(17., 5.),
                                                        ],
                                                        Stroke::new(3_f32, Color32::WHITE),
                                                    ));
                                                } else {
                                                    ui.label(
                                                        RichText::new(badge).color(Color32::WHITE),
                                                    );
                                                }
                                            });
                                        source_menu(ui, |ui| {
                                            self.clipboard_menu(
                                                ui,
                                                ClipTarget::Field {
                                                    owner,
                                                    uid: Some(uid),
                                                },
                                            );
                                            if source_controls::add_enabled_button(
                                                ui,
                                                index > 0,
                                                egui::Button::new("Move up"),
                                            )
                                            .clicked()
                                            {
                                                self.edit("Field reordered", |p| {
                                                    p.reorder_field_definition(uid, index - 1)
                                                });
                                                ui.close();
                                            }
                                            if source_controls::add_enabled_button(
                                                ui,
                                                index + 1 < fields.len(),
                                                egui::Button::new("Move down"),
                                            )
                                            .clicked()
                                            {
                                                self.edit("Field reordered", |p| {
                                                    p.reorder_field_definition(uid, index + 1)
                                                });
                                                ui.close();
                                            }
                                        });
                                    });
                                });
                            if let Some(payload) =
                                item.response.dnd_hover_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                ui.painter().hline(
                                    item.response.rect.x_range(),
                                    item.response.rect.top(),
                                    Stroke::new(2_f32, theme::ACCENT),
                                );
                            }
                            if let Some(payload) =
                                item.response.dnd_release_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                self.edit("Field reordered", |p| {
                                    p.reorder_field_definition(payload.1, index)
                                });
                            }
                            ui.add_space(2.);
                        }
                    });
            });
    }
}

impl EditorApp {
    pub(super) fn source_field_properties(&mut self, ui: &mut egui::Ui, def: &Value) {
        let uid = def["uid"].as_i64().unwrap();
        let public = text(def, "__type", "Int");
        let kind = public.trim_start_matches("Array<").trim_end_matches('>');
        let array = def["isArray"] == true;
        row(ui, "Type", |ui| {
            ui.label(public);
            if source_controls::button(ui, "Convert…").clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("source-field-type-picker"),
                        (self.field_owner, array, Some(uid), None::<String>),
                    )
                });
            }
        });
        let mut properties = vec![
            P("identifier", "Field identifier", W::Text),
            P("doc", "Documentation", W::Multiline),
        ];
        if self.field_owner.is_some() {
            properties.push(P("exportToToc", "Export to ToC", W::Bool));
        }
        if matches!(kind, "String" | "Multilines") || kind.contains("Enum.") {
            properties.push(P("searchable", "Searchable", W::Bool));
        }
        let mut patch = source_form(ui, def, &properties);
        ui.add_space(12.);
        ui.label(
            RichText::new("VALUE SPECIFICATIONS")
                .color(theme::ACCENT)
                .size(18.),
        );
        ui.separator();
        if !matches!(kind, "Point" | "EntityRef") {
            ui.label("Default value");
            if let Ok(instance) = self.project.as_ref().unwrap().make_field_instance(def)
                && let Some(value) = scalar_editor(
                    ui,
                    self.project.as_ref().unwrap(),
                    kind,
                    &instance["__value"],
                    def,
                )
            {
                self.edit("Field default changed", |p| p.set_field_default(uid, value));
            }
        }
        let mut properties = Vec::new();
        if kind != "Bool" {
            properties.push(P(
                "canBeNull",
                if array {
                    "Can contain nulls"
                } else if matches!(kind, "Tile" | "FilePath" | "Point" | "EntityRef") {
                    "Is optional"
                } else {
                    "Can be null"
                },
                W::Bool,
            ));
        }
        if matches!(kind, "Int" | "Float") {
            properties.extend([
                P("min", "Minimum", W::Float(-1e9, 1e9)),
                P("max", "Maximum", W::Float(-1e9, 1e9)),
            ]);
        }
        if kind == "FilePath" {
            properties.push(P("acceptFileTypes", "File extensions", W::Text));
        }
        if kind == "String" {
            properties.push(P("regex", "Regex check", W::Text));
        }
        if kind == "Multilines" {
            let previous = def["textLanguageMode"].as_str().map(str::to_owned);
            let mut language = previous.clone();
            row(ui, "Language", |ui| {
                egui::ComboBox::from_id_salt(("field-language", uid))
                    .selected_text(language.as_deref().unwrap_or("Plain text"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut language, None, "Plain text");
                        for (mode, label) in [
                            ("LangJson", "JSON"),
                            ("LangXml", "XML/HTML"),
                            ("LangMarkdown", "Markdown"),
                            ("LangRuby", "Ruby"),
                            ("LangPython", "Python"),
                            ("LangHaxe", "Haxe"),
                            ("LangJS", "Javascript"),
                            ("LangLua", "Lua"),
                            ("LangC", "C/C++/C#"),
                            ("LangLog", "Log file"),
                        ] {
                            ui.selectable_value(&mut language, Some(mode.to_owned()), label);
                        }
                    });
            });
            if language != previous {
                patch["textLanguageMode"] = language.map(Value::String).unwrap_or(Value::Null);
            }
        }
        if kind == "EntityRef" {
            properties.extend([
                P("symmetricalRef", "Symmetrical", W::Bool),
                P("allowOutOfLevelRef", "In any level", W::Bool),
                P("autoChainRef", "Auto-chain", W::Bool),
                P(
                    "allowedRefs",
                    "Allowed references",
                    W::Choice(&["Any", "OnlySame", "OnlyTags", "OnlySpecificEntity"]),
                ),
            ]);
        }
        if array {
            properties.extend([
                P(
                    "arrayMinLength",
                    "Minimum array length",
                    W::Int(0, 1_000_000),
                ),
                P(
                    "arrayMaxLength",
                    "Maximum array length",
                    W::Int(0, 1_000_000),
                ),
            ]);
        }
        let mut specs = def.clone();
        if kind == "FilePath" {
            specs["acceptFileTypes"] = json!(
                def["acceptFileTypes"]
                    .as_array()
                    .map(|values| values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("  "))
                    .unwrap_or_default()
            );
        }
        if let Some(obj) = source_form(ui, &specs, &properties).as_object() {
            patch.as_object_mut().unwrap().extend(obj.clone());
        }
        if kind == "FilePath"
            && let Some(raw) = patch["acceptFileTypes"].as_str()
        {
            patch["acceptFileTypes"] = source_file_types(raw);
        }
        if kind == "Tile" {
            let mut atlas = def["tilesetUid"].as_i64().unwrap_or(-1);
            self.tileset_choice(ui, "Tileset", &mut atlas);
            if def["tilesetUid"].as_i64() != Some(atlas) && atlas >= 0 {
                patch["tilesetUid"] = json!(atlas);
            }
        }
        if kind == "EntityRef"
            && def["allowedRefs"] == "OnlyTags"
            && let Some(tags) = tags(ui, "Allowed reference tags", &def["allowedRefTags"])
        {
            patch["allowedRefTags"] = tags;
        }
        if kind == "EntityRef" && def["allowedRefs"] == "OnlySpecificEntity" {
            let mut entity = def["allowedRefsEntityUid"].as_i64().unwrap_or(-1);
            egui::ComboBox::from_id_salt(("allowed-entity", uid))
                .selected_text(
                    self.project.as_ref().unwrap().json["defs"]["entities"]
                        .as_array()
                        .and_then(|a| a.iter().find(|d| d["uid"] == entity))
                        .map(|d| text(d, "identifier", "Entity"))
                        .unwrap_or("Choose entity"),
                )
                .show_ui(ui, |ui| {
                    for entity_def in self.project.as_ref().unwrap().json["defs"]["entities"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if let Some(uid) = entity_def["uid"].as_i64() {
                            ui.selectable_value(
                                &mut entity,
                                uid,
                                text(entity_def, "identifier", "Entity"),
                            );
                        }
                    }
                });
            if entity >= 0 && def["allowedRefsEntityUid"].as_i64() != Some(entity) {
                patch["allowedRefsEntityUid"] = json!(entity);
            }
        }
        ui.add_space(12.);
        ui.label(RichText::new("DISPLAY").color(theme::ACCENT).size(18.));
        ui.separator();
        let mut modes = vec!["Hidden", "ValueOnly", "NameAndValue"];
        if array {
            modes.extend(["ArrayCountNoLabel", "ArrayCountWithLabel"]);
        }
        if kind == "Point" {
            modes.extend(["Points", "PointStar", "PointPath", "PointPathLoop"]);
        }
        if kind == "EntityRef" {
            modes.extend(["RefLinkBetweenPivots", "RefLinkBetweenCenters"]);
        }
        if kind == "Tile" {
            modes.extend(["EntityTile", "LevelTile"]);
        }
        if matches!(kind, "Int" | "Float") {
            modes.extend(["RadiusPx", "RadiusGrid"]);
        }
        let mut mode = text(def, "editorDisplayMode", "Hidden").to_owned();
        egui::ComboBox::from_id_salt(("field-display-mode", uid))
            .selected_text(&mode)
            .show_ui(ui, |ui| {
                for value in modes {
                    ui.selectable_value(&mut mode, value.to_owned(), value);
                }
            });
        if mode != text(def, "editorDisplayMode", "Hidden") {
            patch["editorDisplayMode"] = json!(mode);
        }
        let mut properties = vec![
            P(
                "editorDisplayPos",
                "Position",
                W::Choice(&["Above", "Center", "Beneath"]),
            ),
            P("editorShowInWorld", "Show in world view", W::Bool),
            P("editorDisplayScale", "Display scale", W::Float(0.1, 10.)),
            P("editorDisplayColor", "Display color", W::Color),
            P("editorAlwaysShow", "Always show", W::Bool),
        ];
        if kind == "EntityRef" {
            properties.push(P(
                "editorLinkStyle",
                "Link style",
                W::Choice(&[
                    "StraightArrow",
                    "CurvedArrow",
                    "ZigZag",
                    "DashedLine",
                    "ArrowsLine",
                    "StraightDottedLine",
                ]),
            ));
        }
        if matches!(kind, "Int" | "Float") {
            properties.extend([
                P("editorTextPrefix", "Prefix", W::Text),
                P("editorTextSuffix", "Suffix", W::Text),
            ]);
        }
        if matches!(kind, "String" | "Multilines") {
            properties.push(P("editorCutLongValues", "Only show excerpt", W::Bool));
        }
        if kind == "Color" || kind.contains("Enum.") {
            properties.push(P("useForSmartColor", "Override parent color", W::Bool));
        }
        if let Some(obj) = source_form(ui, def, &properties).as_object() {
            patch.as_object_mut().unwrap().extend(obj.clone());
        }
        if changed(&patch) {
            self.edit("Field definition changed", |p| {
                p.update_field_definition(uid, &patch)
            });
        }
    }
    fn source_field_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-field-type-picker");
        let Some((owner, array, convert, mut chosen)) =
            ctx.data_mut(|d| d.get_temp::<(Option<i64>, bool, Option<i64>, Option<String>)>(id))
        else {
            return;
        };
        let mut open = true;
        let mut dismiss = false;
        let mut apply = None;
        egui::Window::new(if convert.is_some(){"Convert field type"}else if array{"Create array field"}else{"Create single value field"}).id(id).open(&mut open).resizable(false).show(ctx,|ui|{
            ui.horizontal_wrapped(|ui|{for kind in ["Int","Float","Bool","String","Multilines","Color","FilePath","Tile","EntityRef","Point"]{if owner.is_none()&&matches!(kind,"EntityRef"|"Point"){continue;}
if source_controls::add_sized_button(ui,[95.,44.],egui::Button::new(kind)).clicked(){chosen=Some(kind.into());if convert.is_none(){apply=chosen.clone();}}}});
            ui.separator();ui.label("Enums");let p=self.project.as_ref().unwrap();for group in ["enums","externalEnums"]{for def in p.json["defs"][group].as_array().into_iter().flatten(){let name=text(def,"identifier","Enum");if source_controls::button(ui,name).clicked(){chosen=Some(format!("{}.{}",if group=="enums"{"LocalEnum"}else{"ExternEnum"},name));if convert.is_none(){apply=chosen.clone();}}}}
            if p.json["defs"]["enums"].as_array().is_none_or(Vec::is_empty)&&p.json["defs"]["externalEnums"].as_array().is_none_or(Vec::is_empty){ui.label("Create an enum in the Enum panel first.");if source_controls::button(ui,"Open enum panel").clicked(){self.show_panel(Some(Panel::Enums));dismiss=true;}}
            if convert.is_some()&&let Some(kind)=chosen.as_ref(){ui.separator();ui.colored_label(theme::ACCENT,"Converting changes existing values and defaults. Values incompatible with the chosen type will be cleared.");ui.label(format!("Convert to {kind}"));if source_controls::button(ui,"Convert all values").clicked(){apply=chosen.clone();}}
        });
        if dismiss {
            open = false;
        }
        if let Some(kind) = apply {
            if let Some(uid) = convert {
                if self
                    .edit_recoverable("Field type converted", |p| {
                        p.convert_field_type(uid, &kind, array, true)
                    })
                    .is_some()
                {
                    open = false;
                }
            } else {
                let fields = self.project.as_ref().unwrap();
                let fields = owner
                    .and_then(|uid| {
                        fields.json["defs"]["entities"]
                            .as_array()
                            .and_then(|a| a.iter().find(|d| d["uid"] == uid))
                    })
                    .map(|d| &d["fieldDefs"])
                    .unwrap_or(&fields.json["defs"]["levelFields"]);
                let base = kind.split('.').next_back().unwrap_or(&kind);
                let name = unique_name(
                    base,
                    fields
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|d| text(d, "identifier", "")),
                );
                if let Some(uid) = self.edit("Field created", |p| {
                    p.add_field_definition(owner, &name, &kind, array)
                }) {
                    self.field_uid = Some(uid);
                    open = false;
                }
            }
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, (owner, array, convert, chosen)));
        } else {
            ctx.data_mut(|d| d.remove::<(Option<i64>, bool, Option<i64>, Option<String>)>(id));
        }
    }
}

impl EditorApp {
    fn source_layer_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-layer-type-picker");
        let mut open = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        let mut chosen = None;
        egui::Window::new("Create a layer")
            .id(id)
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                for (kind, description) in [
                    (
                        "IntGrid",
                        "Integer grids for collisions, game data and automatic tiles.",
                    ),
                    (
                        "Entities",
                        "Instances of gameplay objects, such as the player or items.",
                    ),
                    ("Tiles", "Image tiles picked from a tileset."),
                    (
                        "AutoLayer",
                        "Tiles rendered automatically using rules and a source IntGrid layer.",
                    ),
                ] {
                    if source_controls::add_sized_button(ui, [140., 42.], egui::Button::new(kind))
                        .clicked()
                    {
                        chosen = Some(kind);
                    }
                    ui.label(description);
                    ui.add_space(8.);
                }
            });
        if let Some(kind) = chosen {
            self.new_kind = kind.into();
            self.source_create_definition("layers", None);
            open = false;
        }
        ctx.data_mut(|d| d.insert_temp(id, open));
    }
    fn source_rule_list(&mut self, ui: &mut egui::Ui, height: f32) {
        let definitions = self
            .project
            .as_ref()
            .and_then(|p| p.json["defs"]["layers"].as_array())
            .cloned()
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("source-rule-layer")
            .width(230.)
            .selected_text(
                definitions
                    .iter()
                    .find(|d| d["uid"].as_i64() == self.definition_uid)
                    .map(|d| text(d, "identifier", "Layer"))
                    .unwrap_or("Choose a layer"),
            )
            .show_ui(ui, |ui| {
                for d in &definitions {
                    if matches!(text(d, "type", ""), "IntGrid" | "AutoLayer") {
                        let uid = d["uid"].as_i64();
                        if ui
                            .selectable_value(
                                &mut self.definition_uid,
                                uid,
                                text(d, "identifier", "Layer"),
                            )
                            .changed()
                        {
                            self.rule_group_uid = None;
                            self.rule_uid = None;
                        }
                    }
                }
            });
        let Some(layer) = self.definition_uid else {
            return;
        };
        ui.horizontal(|ui| {
            if orange(ui, "+ GROUP", egui::vec2(155., 41.)).clicked()
                && let Some(uid) = self.edit("Rule group created", |p| {
                    p.add_auto_rule_group(layer, "New group")
                })
            {
                self.rule_group_uid = Some(uid);
                self.rule_uid = None;
            }
            if orange(ui, "WIZARD", egui::vec2(89., 41.)).clicked() {
                self.begin_rule_wizard(ui.ctx(), layer, None);
            }
        });
        let groups = definitions
            .iter()
            .find(|d| d["uid"] == layer)
            .and_then(|d| d["autoRuleGroups"].as_array())
            .cloned()
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("source-rule-groups")
            .max_height((height - 85.).max(40.))
            .show(ui, |ui| {
                for (index, group) in groups.iter().enumerate() {
                    let gid = group["uid"].as_i64().unwrap_or(-1);
                    let open_id = egui::Id::new(("source-rule-group-expanded", layer, gid));
                    let mut expanded = ui
                        .ctx()
                        .data_mut(|d| d.get_temp::<bool>(open_id))
                        .unwrap_or(false);
                    ui.set_width(250.);
                    let item = egui::Frame::new()
                        .fill(theme::DARK)
                        .stroke(Stroke::new(1_f32, theme::LIGHT))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new("≡").sense(Sense::drag()))
                                    .dnd_set_drag_payload((layer, gid));
                                if source_controls::add_sized_button(
                                    ui,
                                    [170., 28.],
                                    egui::Button::new(text(group, "name", "Group"))
                                        .selected(self.rule_group_uid == Some(gid))
                                        .truncate(),
                                )
                                .clicked()
                                {
                                    expanded = !expanded;
                                    self.rule_group_uid = Some(gid);
                                    self.rule_uid = None;
                                    self.clipboard_focus(
                                        ui.ctx(),
                                        ClipTarget::Rule {
                                            layer,
                                            group: gid,
                                            rule: None,
                                        },
                                    );
                                }
                                source_menu(ui, |ui| {
                                    self.clipboard_menu(
                                        ui,
                                        ClipTarget::Rule {
                                            layer,
                                            group: gid,
                                            rule: None,
                                        },
                                    );
                                    if ui
                                        .button("Edit out-of-bounds policy for all rules…")
                                        .clicked()
                                    {
                                        self.begin_group_policy(ui.ctx(), layer, gid);
                                        ui.close();
                                    }
                                    if source_controls::button(ui, "Duplicate and remap…").clicked()
                                    {
                                        self.begin_group_remap(ui.ctx(), layer, gid);
                                        ui.close();
                                    }
                                    if ui
                                        .button(if expanded { "Collapse" } else { "Expand" })
                                        .clicked()
                                    {
                                        expanded = !expanded;
                                        ui.close();
                                    }
                                    if source_controls::button(ui, "Edit with wizard…").clicked()
                                    {
                                        self.begin_rule_wizard(ui.ctx(), layer, Some(gid));
                                        ui.close();
                                    }
                                });
                            });
                            if expanded {
                                for (rule_index, rule) in
                                    group["rules"].as_array().into_iter().flatten().enumerate()
                                {
                                    let rid = rule["uid"].as_i64().unwrap_or(-1);
                                    let response = source_controls::add_sized_button(
                                        ui,
                                        [236., 32.],
                                        egui::Button::new(format!(
                                            "≡ Rule {}  {}",
                                            rule_index + 1,
                                            if rule["active"] == false {
                                                "(disabled)"
                                            } else {
                                                ""
                                            }
                                        ))
                                        .selected(
                                            self.rule_uid == Some(rid)
                                                && self.rule_group_uid == Some(gid),
                                        )
                                        .sense(Sense::click_and_drag()),
                                    );
                                    response.dnd_set_drag_payload((layer, gid, rid));
                                    if response.clicked() {
                                        self.rule_group_uid = Some(gid);
                                        self.rule_uid = Some(rid);
                                        self.clipboard_focus(
                                            ui.ctx(),
                                            ClipTarget::Rule {
                                                layer,
                                                group: gid,
                                                rule: Some(rid),
                                            },
                                        );
                                    }
                                    if let Some(payload) =
                                        response.dnd_release_payload::<(i64, i64, i64)>()
                                        && payload.0 == layer
                                        && payload.2 != rid
                                    {
                                        self.edit("Rule reordered", |p| {
                                            p.move_auto_rule_to_group(
                                                layer, payload.1, gid, payload.2, rule_index,
                                            )
                                        });
                                    }
                                    response.context_menu(|ui| {
                                        self.clipboard_menu(
                                            ui,
                                            ClipTarget::Rule {
                                                layer,
                                                group: gid,
                                                rule: Some(rid),
                                            },
                                        )
                                    });
                                }
                                if source_controls::add_sized_button(
                                    ui,
                                    [236., 28.],
                                    egui::Button::new("+ RULE"),
                                )
                                .clicked()
                                    && let Some(uid) = self
                                        .edit("Rule created", |p| p.add_auto_rule(layer, gid, 3))
                                {
                                    self.rule_group_uid = Some(gid);
                                    self.rule_uid = Some(uid);
                                }
                            }
                        });
                    ui.ctx().data_mut(|d| d.insert_temp(open_id, expanded));
                    if let Some(payload) = item.response.dnd_release_payload::<(i64, i64, i64)>()
                        && payload.0 == layer
                        && payload.1 != gid
                    {
                        let count = group["rules"].as_array().map_or(0, Vec::len);
                        self.edit("Rule moved to group", |p| {
                            p.move_auto_rule_to_group(layer, payload.1, gid, payload.2, count)
                        });
                    }
                    if let Some(payload) = item.response.dnd_release_payload::<(i64, i64)>()
                        && payload.0 == layer
                        && payload.1 != gid
                    {
                        self.edit("Rule group reordered", |p| {
                            p.reorder_auto_rule_group(layer, payload.1, index)
                        });
                    }
                    ui.add_space(8.);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draw_fields(
        app: &mut EditorApp,
        ctx: &egui::Context,
        owner: i64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(200.);
                    app.source_field_list(ui, Some(owner), 400.);
                });
                app.source_field_type_picker(ctx);
            },
        )
    }
    fn text_center(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_center(s, label)),
            _ => None,
        }
    }
    fn click_field(app: &mut EditorApp, ctx: &egui::Context, owner: i64, pos: egui::Pos2) {
        for pressed in [true, false] {
            draw_fields(
                app,
                ctx,
                owner,
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
    }
    #[test]
    fn file_extension_editor_preserves_source_array_schema_and_normalization() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Asset", "FilePath", false)
            .unwrap();
        let types = source_file_types(" .PNG, jpg; PNG ..bad ...bad __ * .tar.gz ");
        assert_eq!(types, json!([".png", ".jpg", "..bad", ".tar.gz"]));
        p.update_field_definition(uid, &json!({"acceptFileTypes":types}))
            .unwrap();
        assert_eq!(
            p.json["defs"]["levelFields"][0]["acceptFileTypes"],
            json!([".png", ".jpg", "..bad", ".tar.gz"])
        );
        assert_eq!(source_file_types("???"), Value::Null);
        assert_eq!(source_file_types("__ ..."), json!([]));
    }
    #[test]
    fn native_add_field_chooses_type_before_mutating_and_can_undo() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let owner = p.add_entity_def("Actor", 16, 16).unwrap();
        app.set_project(p);
        let before = app.project.as_ref().unwrap().json.clone();
        let output = draw_fields(&mut app, &ctx, owner, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text_center(&s.shape, "+ SINGLE VALUE"))
            .unwrap();
        click_field(&mut app, &ctx, owner, pos);
        assert_eq!(
            app.project.as_ref().unwrap().json,
            before,
            "opening type picker must not create an Int field"
        );
        draw_fields(&mut app, &ctx, owner, vec![]);
        let output = draw_fields(&mut app, &ctx, owner, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text_center(&s.shape, "Float"))
            .expect("source type chooser");
        click_field(&mut app, &ctx, owner, pos);
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0]["__type"],
            "Float"
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
    #[test]
    fn sparse_saved_selection_ids_do_not_expand_to_the_bounding_rectangle() {
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] =
            json!([{"uid":10,"tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4}]);
        app.set_project(p);
        app.tile_rect = [0, 0, 3, 3];
        app.atlas_selection = Some((10, vec![0, 2, 8, 10]));
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 2, 8, 10]);
        app.atlas_selection = None;
        assert_eq!(
            app.selected_tile_ids_for(10),
            vec![0, 1, 2, 4, 5, 6, 8, 9, 10]
        );
    }
    #[test]
    fn nested_field_form_labels_remain_fully_readable_inside_their_third_column() {
        fn texts<'a>(
            shape: &'a egui::Shape,
            clip: Rect,
            items: &mut Vec<(Rect, &'a egui::epaint::TextShape)>,
        ) {
            match shape {
                egui::Shape::Text(text) => items.push((clip, text)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, clip, items)
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::load(resource_path("examples/original/Entities.ldtk")).unwrap();
        project.migrate_to_current().unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.settings.set_ui_state_int("extraEntityDetails", None, 0);
        let mut time = 0.;
        let mut draw = |app: &mut EditorApp| {
            time += 1.;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Entities);
                },
            )
        };
        draw(&mut app);
        draw(&mut app);
        let output = draw(&mut app);
        let mut items = Vec::new();
        for shape in &output.shapes {
            texts(&shape.shape, shape.clip_rect, &mut items);
        }
        let general = items
            .iter()
            .find(|(_, text)| text.galley.job.text == "GENERAL SETTINGS")
            .unwrap()
            .1;
        for label in ["Field identifier", "Documentation", "Minimum", "Maximum"] {
            let (clip, text) = items
                .iter()
                .find(|(_, text)| {
                    text.pos.y > general.pos.y
                        && text
                            .galley
                            .job
                            .text
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            == label
                })
                .unwrap_or_else(|| panic!("missing third-column label {label}"));
            let bounds = Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                bounds.left() >= general.pos.x - 0.1,
                "{label} escapes left into the field list: {bounds:?}, third column {}",
                general.pos.x
            );
            assert!(
                clip.expand(0.1).contains_rect(bounds),
                "{label} is clipped instead of readable: text {bounds:?}, clip {clip:?}"
            );
            assert!(text.galley.rows.len() <= 2, "{label} wraps inside words");
            let reserve = if label == "Maximum" { 8. } else { 32. };
            assert!(
                bounds.right() <= general.pos.x + 124. - reserve + 0.1,
                "{label} overlaps its label-cell padding/info pill: {bounds:?}"
            );
            assert!(
                (text.galley.job.sections[0].format.font_id.size - 12.667).abs() < 0.001,
                "source dt labels use 9.5pt"
            );
        }
        let body = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == theme::MEDIUM
                        && rect.rect.left() == 0.
                        && rect.rect.height() > 500. =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            body.right() <= 1000.,
            "third column body must stay under the original 1000-point header: {body:?}"
        );
    }
    #[test]
    fn source_entity_panel_has_original_header_geometry_and_integrated_fields() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/original/Test_file_for_API_showing_all_features.ldtk");
        let mut project = Project::load(path).unwrap();
        project.migrate_to_current().unwrap();
        app.set_project(project);
        let original = app.project.as_ref().unwrap().json.clone();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.source_panel(ctx, Panel::Entities);
            },
        );
        let output = ctx.run(
            egui::RawInput {
                time: Some(1.0),
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                assert!(app.source_panel(ctx, Panel::Entities));
            },
        );
        let header = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Rect(shape)
                    if shape.fill == theme::LIGHT
                        && shape.rect.width() > 900.
                        && shape.rect.height() == 53. =>
                {
                    Some(shape.rect)
                }
                _ => None,
            })
            .next()
            .expect("original full-width panel header");
        assert_eq!(
            header,
            Rect::from_min_size(egui::pos2(0., 34.), egui::vec2(1000., 53.))
        );
        let body = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Rect(r)
                    if r.fill == theme::MEDIUM && r.rect.left() == 0. && r.rect.height() > 500. =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .next()
            .expect("source panel body");
        assert!(
            body.right() <= 1000.,
            "body expanded beyond header: {body:?}"
        );
        assert_eq!(app.field_owner, app.definition_uid);
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
}
