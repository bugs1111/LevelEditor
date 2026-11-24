use super::*;
use std::collections::BTreeMap;

// Row controls can edit the project. Snapshot the display data without copying rules or atlas caches.
struct DefinitionRow {
    uid: i64,
    identifier: String,
    tags: Vec<String>,
    color: Color32,
    layer_color: Option<Color32>,
    layer_type: String,
    tile: Value,
    embedded: bool,
}

impl DefinitionRow {
    fn from_definition(def: &Value, kind: &str) -> Self {
        let color_key = if kind == "layers" { "uiColor" } else { "color" };
        let tile = if number(&def["uiTileRect"], "w", 0.) > 0. {
            &def["uiTileRect"]
        } else {
            &def["tileRect"]
        };
        Self {
            uid: def["uid"].as_i64().unwrap_or(-1),
            identifier: text(def, "identifier", "Definition").to_owned(),
            tags: def["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            color: theme::color(text(def, color_key, "#687487")),
            layer_color: def["uiColor"].as_str().map(theme::color),
            layer_type: text(def, "type", "").to_owned(),
            tile: tile.clone(),
            embedded: !def["embedAtlas"].is_null(),
        }
    }
}

impl EditorApp {
    pub(super) fn source_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        panel: Panel,
        width: f32,
        height: f32,
    ) {
        let width = width.min(ui.available_width());
        ui.set_width(width);
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
        if panel == Panel::Rules {
            self.rule_list(ui, height);
            return;
        }
        if let Some(kind) = panel_kind(panel) {
            let tilesets = panel == Panel::Tilesets;
            let compact = matches!(panel, Panel::Layers | Panel::Tilesets);
            if compact || panel == Panel::Enums {
                ui.add_space(if compact { 3. } else { 2. });
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                let create = if compact || panel == Panel::Enums {
                    let size = if compact {
                        [ui.available_width(), 41.]
                    } else {
                        [ui.available_width() - 72., 43.]
                    };
                    source_controls::add_sized_button(
                        ui,
                        size,
                        egui::Button::image(
                            egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                                .fit_to_exact_size(Vec2::splat(20.)),
                        )
                        .fill(SOURCE_ORANGE)
                        .corner_radius(2.),
                    )
                } else {
                    let width = if panel == Panel::Entities {
                        165.
                    } else {
                        ui.available_width()
                    };
                    orange(ui, "⊞", egui::vec2(width, 41.))
                };
                if create
                    .on_hover_text(locale::tr(ui, "Create definition"))
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
                        ui.menu_button(
                            RichText::new(locale::tr(ui, "PRESETS")).color(Color32::WHITE),
                            |ui| {
                                if source_controls::button(ui, "Rectangle region").clicked() {
                                    self.source_create_definition("entities", Some(false));
                                    ui.close();
                                }
                                if source_controls::button(ui, "Circle region").clicked() {
                                    self.source_create_definition("entities", Some(true));
                                    ui.close();
                                }
                            },
                        );
                    });
                } else if panel == Panel::Enums {
                    ui.scope(|ui| {
                        ui.spacing_mut().interact_size = egui::vec2(70., 43.);
                        ui.spacing_mut().button_padding = egui::vec2(8., 6.);
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = SOURCE_ORANGE;
                        ui.visuals_mut().widgets.inactive.bg_fill = SOURCE_ORANGE;
                        self.import_enum_menu(ui, locale::tr(ui, "IMPORT"));
                    });
                }
            });
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
                                .hint_text(locale::tr(ui, "Search…"))
                                .desired_width(ui.available_width()),
                        );
                    });
                });
            ui.ctx()
                .data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let filter = filter.to_lowercase();
            let matches_filter = |def: &Value| {
                filter.is_empty() || text(def, "identifier", "").to_lowercase().contains(&filter)
            };
            let definitions = self
                .project
                .as_ref()
                .and_then(|p| p.json["defs"][kind].as_array())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let no_tilesets = tilesets && definitions.is_empty();
            let rows: Vec<_> = definitions
                .iter()
                .filter(|def| matches_filter(def))
                .map(|def| DefinitionRow::from_definition(def, kind))
                .collect();
            let mut groups = BTreeMap::<&str, Vec<&DefinitionRow>>::new();
            for row in &rows {
                if row.tags.is_empty() {
                    let group = if tilesets {
                        ""
                    } else if matches!(panel, Panel::Entities | Panel::Enums) {
                        "Other"
                    } else {
                        kind
                    };
                    groups.entry(group).or_default().push(row);
                } else {
                    for tag in &row.tags {
                        groups.entry(tag).or_default().push(row);
                    }
                }
            }
            let mut groups = groups.into_iter().collect::<Vec<_>>();
            if tilesets {
                groups.sort_by_cached_key(|(name, _)| name.to_lowercase());
            }
            let frame = if compact || panel == Panel::Enums {
                egui::Frame::new()
                    .fill(theme::DARK)
                    .stroke(Stroke::new(1_f32, theme::LIGHT))
                    .inner_margin(4)
            } else {
                egui::Frame::NONE
            };
            let list_height = (height
                - match panel {
                    Panel::Layers | Panel::Tilesets => 100.,
                    Panel::Enums => 98.,
                    _ => 88.,
                })
            .max(40.);
            frame.show(ui, |ui| {
                if compact {
                    ui.set_width((width - 10.).max(1.));
                }
                if no_tilesets {
                    let rect = Rect::from_min_size(
                        ui.next_widget_position(),
                        egui::vec2(ui.available_width(), list_height),
                    )
                    .expand(4.);
                    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                        .paint_at(ui, rect);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("source-definition-list", kind))
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if compact {
                            ui.set_min_height(list_height);
                        }
                        if no_tilesets {
                            ui.add_space(26.);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(locale::tr(ui, "No tileset"))
                                        .font(FontId::new(
                                            17.,
                                            egui::FontFamily::Name("panel-title".into()),
                                        ))
                                        .color(theme::LIGHT)
                                        .italics(),
                                );
                            });
                        }
                        let multiple_groups = groups.len() > 1;
                        for (name, definitions) in groups {
                            ui.set_width(width.min(ui.available_width()));
                            let show_header = if tilesets {
                                multiple_groups && filter.is_empty()
                            } else if panel == Panel::Enums {
                                multiple_groups
                            } else {
                                kind != "layers" || name != kind
                            };
                            self.source_definition_group(ui, kind, name, &definitions, show_header);
                        }
                        if panel == Panel::Enums {
                            let external = self
                                .project
                                .as_ref()
                                .and_then(|p| p.json["defs"]["externalEnums"].as_array())
                                .into_iter()
                                .flatten();
                            let mut sources = BTreeMap::<String, Vec<Value>>::new();
                            for def in external {
                                if matches_filter(def) {
                                    sources
                                        .entry(
                                            text(def, "externalRelPath", "External enums")
                                                .to_owned(),
                                        )
                                        .or_default()
                                        .push(def.clone());
                                }
                            }
                            for (path, defs) in sources {
                                self.source_external_enum_group(ui, &path, &defs);
                            }
                        }
                        self.definition_list_paste_menu(
                            ui,
                            ClipTarget::Definition {
                                kind: kind.to_owned(),
                                uid: None,
                            },
                        );
                    })
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
                    egui::Button::new(locale::tr(ui, label)).selected(panel == target),
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
        definitions: &[&DefinitionRow],
        show_header: bool,
    ) {
        ui.set_width(ui.available_width());
        let enums = kind == "enums";
        let layers = kind == "layers";
        let tilesets = kind == "tilesets";
        let compact = layers || tilesets;
        let grouped_tilesets = tilesets && show_header;
        let row_height = if tilesets { 27. } else { 28. };
        let menu_width = if tilesets { 18. } else { 26. };
        let shared_frame = enums || compact;
        egui::Frame::new()
            .fill(if shared_frame {
                Color32::TRANSPARENT
            } else {
                theme::DARK
            })
            .stroke(if shared_frame {
                Stroke::NONE
            } else {
                Stroke::new(1_f32, theme::LIGHT)
            })
            .inner_margin(if shared_frame { 0 } else { 4 })
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
                    if name == "Other" || tilesets && name.is_empty() {
                        "null"
                    } else {
                        name
                    }
                );
                let mut open = !show_header || self.settings.ui_state_int(&key, None, 1) != 0;
                if show_header {
                    if grouped_tilesets {
                        ui.add_space(2.);
                    }
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(
                            ui.available_width(),
                            if grouped_tilesets { 18. } else { 20. },
                        ),
                        Sense::click(),
                    );
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
                    let heading = if tilesets && name.is_empty() {
                        locale::tr(ui, "Untagged")
                    } else if name == "Other"
                        && definitions
                            .iter()
                            .all(|def| def.tags.iter().all(|tag| tag != name))
                    {
                        locale::tr(ui, if enums { "Untagged" } else { name })
                    } else {
                        name
                    };
                    ui.painter().text(
                        rect.left_center() + egui::vec2(24., 0.),
                        Align2::LEFT_CENTER,
                        heading.to_uppercase(),
                        FontId::new(14., egui::FontFamily::Name("panel-title".into())),
                        theme::ACCENT,
                    );
                    if response.clicked() {
                        open = !open;
                        self.settings.set_ui_state_int(&key, None, i64::from(open));
                    }
                }
                if open {
                    let margin = if grouped_tilesets {
                        egui::Margin {
                            left: 13,
                            right: 13,
                            top: 9,
                            bottom: 12,
                        }
                    } else {
                        egui::Margin::same(if layers { 0 } else { 4 })
                    };
                    egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                        if compact {
                            ui.spacing_mut().item_spacing = egui::vec2(8., 1.);
                        }
                        let drag_group = tilesets.then(|| name.to_owned());
                        for def in definitions {
                            let uid = def.uid;
                            let selected = self.definition_uid == Some(uid);
                            let color = def.color;
                            let layer_color = def.layer_color;
                            let fill = if layers {
                                match (selected, layer_color) {
                                    (true, Some(color)) => color,
                                    (false, Some(color)) => tint(theme::DARK, color, 0.5),
                                    (true, None) => theme::ACCENT,
                                    (false, None) => theme::MEDIUM,
                                }
                            } else if enums || tilesets {
                                if selected {
                                    theme::ACCENT
                                } else {
                                    theme::MEDIUM
                                }
                            } else if selected {
                                tint(color, Color32::WHITE, 0.65)
                            } else {
                                tint(theme::MEDIUM, color, 0.22)
                            };
                            let item = egui::Frame::new()
                                .fill(fill)
                                .inner_margin(egui::Margin::symmetric(
                                    4,
                                    if compact { 3 } else { 0 },
                                ))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let grip =
                                            egui::Label::new(RichText::new("≡").color(theme::TEXT))
                                                .sense(Sense::drag());
                                        let handle = if compact {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(16., row_height),
                                                Sense::drag(),
                                            );
                                            egui::Image::new(egui::include_image!(
                                                "../../assets/icons/dragAlpha.svg"
                                            ))
                                            .paint_at(
                                                ui,
                                                Rect::from_center_size(
                                                    rect.center(),
                                                    Vec2::splat(18.),
                                                ),
                                            );
                                            response
                                        } else if enums {
                                            ui.add_sized([19., 33.], grip)
                                        } else {
                                            ui.add(grip)
                                        };
                                        handle.dnd_set_drag_payload((
                                            kind.to_owned(),
                                            uid,
                                            drag_group.clone(),
                                        ));
                                        let label = def.identifier.as_str();
                                        let response = if compact {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(
                                                    (ui.available_width() - menu_width - 8.)
                                                        .max(32.),
                                                    row_height,
                                                ),
                                                Sense::click(),
                                            );
                                            let label_offset = if layers {
                                                self.source_definition_thumbnail(
                                                    ui,
                                                    def,
                                                    kind,
                                                    Rect::from_center_size(
                                                        rect.left_center() + egui::vec2(12., 0.),
                                                        Vec2::splat(24.),
                                                    ),
                                                    color,
                                                );
                                                32.
                                            } else if def.embedded {
                                                let icon = egui::Image::new(egui::include_image!(
                                                    "../../assets/icons/embed.svg"
                                                ));
                                                let icon = if selected {
                                                    icon.tint(Color32::BLACK)
                                                } else {
                                                    icon
                                                };
                                                icon.paint_at(
                                                    ui,
                                                    Rect::from_center_size(
                                                        rect.left_center() + egui::vec2(8., 0.),
                                                        Vec2::splat(16.),
                                                    ),
                                                );
                                                20.
                                            } else {
                                                0.
                                            };
                                            let ink = if selected {
                                                Color32::BLACK
                                            } else if let Some(color) = layer_color {
                                                tint(color, Color32::WHITE, 0.3)
                                            } else {
                                                theme::TEXT
                                            };
                                            let name = egui::WidgetText::from(
                                                RichText::new(label).color(ink),
                                            );
                                            let galley = name.into_galley(
                                                ui,
                                                Some(egui::TextWrapMode::Truncate),
                                                (rect.width() - label_offset).max(0.),
                                                FontId::proportional(14.),
                                            );
                                            ui.painter().galley(
                                                rect.left_center()
                                                    + egui::vec2(
                                                        label_offset,
                                                        -galley.size().y / 2.,
                                                    ),
                                                galley,
                                                ink,
                                            );
                                            response.widget_info(|| {
                                                egui::WidgetInfo::selected(
                                                    egui::WidgetType::SelectableLabel,
                                                    true,
                                                    selected,
                                                    label,
                                                )
                                            });
                                            response
                                        } else if enums {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(
                                                    (ui.available_width() - 21.).max(20.),
                                                    33.,
                                                ),
                                                Sense::click(),
                                            );
                                            ui.painter_at(rect).text(
                                                rect.left_center(),
                                                Align2::LEFT_CENTER,
                                                label,
                                                FontId::proportional(14.),
                                                if selected {
                                                    Color32::BLACK
                                                } else {
                                                    Color32::WHITE
                                                },
                                            );
                                            response
                                        } else {
                                            let (rect, _) = ui.allocate_exact_size(
                                                Vec2::splat(32.),
                                                Sense::hover(),
                                            );
                                            self.source_definition_thumbnail(
                                                ui, def, kind, rect, color,
                                            );
                                            let available = (ui.available_width() - 42.).max(20.);
                                            source_controls::add_sized_button(
                                                ui,
                                                [available, 32.],
                                                egui::Button::new(RichText::new(label).color(
                                                    if selected {
                                                        Color32::BLACK
                                                    } else {
                                                        tint(color, Color32::WHITE, 0.4)
                                                    },
                                                ))
                                                .truncate()
                                                .frame(false),
                                            )
                                        };
                                        if response.clicked() {
                                            self.clipboard_focus(
                                                ui.ctx(),
                                                ClipTarget::Definition {
                                                    kind: kind.to_owned(),
                                                    uid: Some(uid),
                                                },
                                            );
                                            if kind == "layers" {
                                                self.select_layer_definition(Some(uid));
                                            } else {
                                                self.definition_uid = Some(uid);
                                            }
                                            if kind == "entities" {
                                                self.entity_uid = Some(uid);
                                                self.field_owner = Some(uid);
                                                self.field_uid = None;
                                            }
                                        }
                                        source_definition_menu(
                                            ui,
                                            compact.then_some(egui::vec2(menu_width, row_height)),
                                            &response,
                                            |ui| {
                                                self.clipboard_menu(
                                                    ui,
                                                    ClipTarget::Definition {
                                                        kind: kind.to_owned(),
                                                        uid: Some(uid),
                                                    },
                                                )
                                            },
                                        );
                                    });
                                });
                            if compact {
                                let rect = item.response.rect;
                                if let Some(color) = layer_color {
                                    ui.painter().rect_filled(
                                        Rect::from_min_max(
                                            egui::pos2(rect.right() - 4., rect.top()),
                                            rect.max,
                                        ),
                                        0.,
                                        if selected { Color32::WHITE } else { color },
                                    );
                                }
                                if !selected && item.response.contains_pointer() {
                                    ui.painter().rect_stroke(
                                        rect,
                                        0.,
                                        Stroke::new(2_f32, Color32::from_white_alpha(166)),
                                        StrokeKind::Inside,
                                    );
                                }
                            }
                            if let Some(payload) = item
                                .response
                                .dnd_hover_payload::<(String, i64, Option<String>)>()
                                && payload.0 == kind
                                && payload.1 != uid
                                && payload.2 == drag_group
                            {
                                ui.painter().hline(
                                    item.response.rect.x_range(),
                                    item.response.rect.top(),
                                    Stroke::new(2_f32, theme::ACCENT),
                                );
                            }
                            if let Some(payload) = item
                                .response
                                .dnd_release_payload::<(String, i64, Option<String>)>()
                                && payload.0 == kind
                                && payload.1 != uid
                                && payload.2 == drag_group
                            {
                                self.source_reorder_definition(
                                    ui.ctx(),
                                    kind,
                                    payload.1,
                                    self.definition_index(kind, uid),
                                );
                            }
                            if !compact {
                                ui.add_space(3.);
                            }
                        }
                    });
                }
            });
        if !tilesets {
            ui.add_space(8.);
        } else if show_header {
            ui.add_space(3.);
        }
    }
    pub(super) fn source_reorder_definition(
        &mut self,
        ctx: &egui::Context,
        kind: &str,
        uid: i64,
        index: usize,
    ) {
        let reordered = self.edit("Definition reordered", |p| {
            p.reorder_definition(kind, uid, index)
        });
        if reordered.is_some() && matches!(kind, "layers" | "tilesets") {
            if kind == "layers" {
                self.select_layer_definition(Some(uid));
            } else {
                self.definition_uid = Some(uid);
            }
            self.clipboard_focus(
                ctx,
                ClipTarget::Definition {
                    kind: kind.to_owned(),
                    uid: Some(uid),
                },
            );
        }
    }
    fn source_definition_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        def: &DefinitionRow,
        kind: &str,
        rect: Rect,
        color: Color32,
    ) {
        if kind == "layers" {
            egui::Image::new(layer_settings::layer_type_icon(&def.layer_type))
                .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(24.)));
            return;
        }
        let tile = &def.tile;
        let atlas = tile["tilesetUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.tileset(uid));
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
    pub(super) fn source_create_definition(&mut self, kind: &str, preset: Option<bool>) {
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
            if kind == "layers" {
                self.select_layer_definition(Some(uid));
            } else {
                self.definition_uid = Some(uid);
            }
            if kind == "entities" {
                self.entity_uid = Some(uid);
                self.field_owner = Some(uid);
                self.field_uid = None;
            }
        }
    }
}

impl EditorApp {
    pub(super) fn source_layer_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-layer-type-picker");
        let mut open = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        let mut chosen = None;
        egui::Window::new(locale::tr_ctx(ctx, "Create a layer"))
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
                    if source_controls::add_sized_button(
                        ui,
                        [140., 42.],
                        egui::Button::new(locale::tr(ui, kind)),
                    )
                    .clicked()
                    {
                        chosen = Some(kind);
                    }
                    ui.label(locale::tr(ui, description));
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
}
