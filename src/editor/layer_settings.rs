//! Common layer settings from the pinned LDtk EditLayerDefs form.
use super::source_controls::{FormSelectAppearance, TilesetChoice, tileset_picker};
use super::world_level_panel::form_text_input;
use super::*;
use crate::app::chrome::ActionTooltip as _;

#[path = "intgrid_settings.rs"]
mod intgrid_settings;

fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let id = egui::Id::new(("layer-settings-section", title));
    let open = ui.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let title = locale::tr(ui, title).to_uppercase();
    let open = section(ui, &title, open, |ui| {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.;
                body(ui);
            });
    });
    ui.data_mut(|d| d.insert_temp(id, open));
    open
}
fn layer_row(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    // The row helper adds a 10-point gutter to the source's 160-point track.
    row_width(ui, label, 150., body);
}

fn unit(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(locale::tr(ui, label)).color(Color32::from_white_alpha(128)));
}

pub(super) fn layer_type_icon(kind: &str) -> egui::ImageSource<'static> {
    match kind {
        "Entities" => egui::include_image!("../../assets/icons/entity.png"),
        "IntGrid" => egui::include_image!("../../assets/icons/intGrid.png"),
        "AutoLayer" => egui::include_image!("../../assets/icons/autoLayer.svg"),
        _ => egui::include_image!("../../assets/icons/tile.png"),
    }
}

fn checkbox(ui: &mut egui::Ui, def: &Value, patch: &mut Value, key: &str, label: &str) {
    let mut value = def[key]
        .as_bool()
        .unwrap_or(matches!(key, "renderInWorldView" | "canSelectWhenInactive"));
    if source_controls::checkbox(ui, &mut value, label).changed() {
        patch[key] = json!(value);
    }
}
fn integer(ui: &mut egui::Ui, def: &Value, patch: &mut Value, key: &str, min: i64, max: i64) {
    let current = def[key].as_i64().unwrap_or(0);
    let display = if key.starts_with("guideGrid") && current == 0 {
        String::new()
    } else {
        current.to_string()
    };
    let placeholder = match key {
        "guideGridWid" => Some("(width)"),
        "guideGridHei" => Some("(height)"),
        _ => None,
    };
    let style = source_controls::NumberStyle::Plain { placeholder };
    if let Some(draft) = source_controls::number_input(ui, key, &display, style) {
        let value = if draft.trim().is_empty() {
            Some(0)
        } else {
            draft.trim().parse::<i64>().ok()
        };
        if let Some(value) = value {
            let mut value = value.clamp(min, max);
            if key.starts_with("guideGrid") && value <= 1 {
                value = 0;
            }
            patch[key] = json!(value);
        }
    }
}
fn percent(ui: &mut egui::Ui, def: &Value, patch: &mut Value, key: &str, min: f64, max: f64) {
    let current = def[key]
        .as_f64()
        .unwrap_or(if key == "guideOpacity" { 0.33 } else { 0. });
    let display = format!("{:.0}", (current * 100. + 0.5).floor());
    let style = match key {
        "displayOpacity" | "inactiveOpacity" | "guideOpacity" => {
            source_controls::NumberStyle::PercentSlider { min, max }
        }
        _ => source_controls::NumberStyle::Percent,
    };
    if let Some(draft) = source_controls::number_input(ui, key, &display, style) {
        let value = if key == "parallaxFactorY" && draft.trim().is_empty() {
            def["parallaxFactorX"].as_f64().map(|x| x * 100.)
        } else {
            Some(
                draft
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.),
            )
        };
        if let Some(value) = value.filter(|n| n.is_finite()) {
            patch[key] = json!((value.clamp(min, max) + 0.5).floor() / 100.);
        }
    }
}

fn layer_color(ui: &mut egui::Ui, def: &Value, patch: &mut Value, key: &str) -> bool {
    ui.spacing_mut().item_spacing.x = 4.;
    let mut color = def[key]
        .as_str()
        .map(theme::color)
        .map(|c| [c.r(), c.g(), c.b()]);
    if source_colors::form_color_editor(ui, &mut color).changed()
        && let Some([r, g, b]) = color
    {
        patch[key] = json!(format!("#{r:02X}{g:02X}{b:02X}"));
    }
    color.is_some()
}

fn reset_layer_color(ui: &mut egui::Ui, patch: &mut Value, key: &str) {
    let response = ui.add_sized(
        [54., 32.],
        egui::Button::image(
            egui::Image::new(egui::include_image!("../../assets/icons/reset.svg"))
                .fit_to_exact_size(Vec2::splat(22.)),
        )
        .frame(false),
    );
    let label = locale::tr(ui, "Reset assigned color");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    if response.action_tooltip(label).clicked() {
        patch[key] = Value::Null;
    }
}

impl EditorApp {
    pub(in super::super) fn layer_tileset_assignment(&mut self, ui: &mut egui::Ui, def: &Value) {
        let uid = def["uid"].as_i64().unwrap();
        let project = self.project.as_ref().unwrap();
        let tilesets = project.json["defs"]["tilesets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let current = def["tilesetDefUid"].as_i64();
        let tile_size = current
            .and_then(|atlas| project.tileset(atlas))
            .and_then(|tileset| tileset["tileGridSize"].as_i64());
        let mut open_tilesets = false;
        let selected = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.;
                let total_width = ui.available_width();
                let size_label = tile_size.map(|size| {
                    locale::format(ui, "{size}px tiles", &[("size", &size.to_string())])
                });
                let size_galley = size_label.map(|label| {
                    ui.painter().layout_no_wrap(
                        label,
                        FontId::proportional(14.),
                        Color32::from_white_alpha(128),
                    )
                });
                let width = if tilesets.is_empty() {
                    total_width * 0.5 - 4.
                } else if let Some(galley) = &size_galley {
                    total_width - galley.size().x - 3.
                } else {
                    total_width
                };
                let selected = tileset_picker(
                    ui,
                    width,
                    tilesets,
                    current,
                    "-- Tileset required --",
                    false,
                );
                if tilesets.is_empty() {
                    open_tilesets = orange(
                        ui,
                        "CREATE NEW TILESET",
                        egui::vec2(ui.available_width(), 29.),
                    )
                    .clicked();
                } else if let Some(galley) = size_galley {
                    ui.add(egui::Label::new(galley));
                }
                selected
            })
            .inner;
        if let Some(tile_size) = tile_size.filter(|size| *size > 0)
            && let Some(grid_size) = def["gridSize"].as_i64().filter(|size| *size > 0)
            && tile_size % grid_size != 0
        {
            let width = ui.available_width();
            let warning = locale::format(
                ui,
                "Warning: the TILESET grid ({tileset}px) differs from the LAYER grid ({layer}px), and the values aren't multiples, which can lead to unexpected behaviors when adding a group of tiles.",
                &[
                    ("tileset", &tile_size.to_string()),
                    ("layer", &grid_size.to_string()),
                ],
            );
            let galley = ui.painter().layout(
                warning,
                FontId::proportional(14.667),
                Color32::PLACEHOLDER,
                (width - 18.).max(1.),
            );
            ui.add_space(4. - ui.spacing().item_spacing.y);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(width, galley.size().y + 18.), Sense::hover());
            project_dialogs::paint_warning(
                ui.painter(),
                rect,
                galley,
                Color32::from_black_alpha(168),
                Color32::WHITE,
            );
        }
        let options = if tilesets.is_empty() {
            RemapOptions {
                mode: ldtk_core::TilesetRemap::TileIds,
                allow_loss: false,
                clear_overrides: false,
            }
        } else {
            remap_options(ui, ("layer", uid), true, ldtk_core::TilesetRemap::TileIds)
        };
        if let Some(selected) = selected {
            self.apply_layer_tileset(def, selected, options);
        }
        if open_tilesets {
            self.show_panel(Some(Panel::Tilesets));
        }
    }

    fn apply_layer_tileset(&mut self, def: &Value, atlas: TilesetChoice, options: RemapOptions) {
        let uid = def["uid"].as_i64().unwrap();
        let has_auto_rules = matches!(def["type"].as_str(), Some("AutoLayer" | "IntGrid"))
            && def["autoRuleGroups"]
                .as_array()
                .is_some_and(|groups| !groups.is_empty());
        let assign = |p: &mut Project| {
            let atlas = match atlas {
                TilesetChoice::Unassigned => None,
                TilesetChoice::Existing(uid) => Some(uid),
                TilesetChoice::EmbeddedIcons => Some(p.ensure_embedded_icons()?),
            };
            p.assign_tileset_with_policy(
                uid,
                atlas,
                options.mode,
                options.clear_overrides,
                options.allow_loss,
            )
        };
        let discarded = if has_auto_rules {
            self.edit_recoverable("Changed auto-layer tileset", assign)
        } else {
            self.edit("Tileset assigned", assign)
        };
        if let Some(discarded) = discarded
            && discarded > 0
        {
            self.status = format!("Tileset assigned; removed {discarded} references");
        }
    }

    fn layer_tag_row(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
        label: &str,
        field: tag_editor::Field<'_>,
    ) {
        let uid = def["uid"].as_i64().unwrap();
        let key = field.key();
        let mut change = None;
        layer_row(ui, label, |ui| {
            change = tag_editor::show(ui, self.project.as_ref().unwrap(), field, uid, &def[key]);
        });
        if let Some(change) = change {
            self.edit("Layer tags changed", |p| match change {
                tag_editor::Change::Set(value) => {
                    p.update_layer_properties(uid, &json!({key:value}))
                }
                tag_editor::Change::Rename {
                    old,
                    new,
                    everywhere,
                } => p.rename_definition_tag("layers", uid, &old, &new, everywhere),
            });
        }
    }

    pub(in super::super) fn source_layer_settings(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
    ) -> Value {
        let mut patch = json!({});
        let kind = text(def, "type", "");
        ui.push_id(("layer-settings", def["uid"].as_i64()), |ui| {
            let general_open = group(ui, "General layer settings", |ui| {
                row_width_with_label(
                    ui,
                    150.,
                    |ui| {
                        ui.add_space(8.);
                        ui.label(RichText::new(locale::tr(ui, "Type")).size(12.667));
                    },
                    |ui| {
                        let muted = Color32::from_white_alpha(128);
                        let icon_size = 36.667;
                        ui.add_space(4.);
                        ui.add(
                            egui::Image::new(layer_type_icon(kind))
                                .fit_to_exact_size(Vec2::splat(icon_size))
                                .tint(muted),
                        );
                        let description = match kind {
                            "IntGrid" => "Integer grid",
                            "AutoLayer" => "Auto-layer",
                            "Tiles" => "Tiles",
                            _ => "Entities",
                        };
                        ui.label(
                            RichText::new(locale::tr(ui, description))
                                .size(16.)
                                .color(muted),
                        );
                        ui.add_space(11.);
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), icon_size),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 2.;
                                let width = ((ui.available_width() - 2.) / 2.).max(24.);
                                for (label, url) in [
                                    (
                                        "Tile layer tutorial",
                                        "https://ldtk.io/docs/tutorials/tile-layers",
                                    ),
                                    (
                                        "Auto-layer tutorial",
                                        "https://ldtk.io/docs/tutorials/auto-layers",
                                    ),
                                ] {
                                    source_controls::form_help_link(ui, width, label, url);
                                }
                            },
                        );
                    },
                );
                ui.add_space(9.);
                for (key, label) in [("identifier", "Layer identifier"), ("doc", "Documentation")] {
                    layer_row(ui, label, |ui| {
                        if let Some(value) =
                            form_text_input(ui, key, text(def, key, ""), ui.available_width())
                        {
                            patch[key] = json!(value);
                        }
                    });
                }
                if kind == "Tiles" {
                    layer_row(ui, "Default tileset", |ui| {
                        ui.vertical(|ui| self.layer_tileset_assignment(ui, def));
                    });
                }
                layer_row(ui, "Grid size", |ui| {
                    ui.add_enabled_ui(kind != "AutoLayer", |ui| {
                        integer(ui, def, &mut patch, "gridSize", 1, 1024);
                    });
                    unit(ui, "px");
                });
                layer_row(ui, "Offsets", |ui| {
                    unit(ui, "X=");
                    integer(ui, def, &mut patch, "pxOffsetX", -32768, 32767);
                    unit(ui, "px, Y=");
                    integer(ui, def, &mut patch, "pxOffsetY", -32768, 32767);
                    unit(ui, "px");
                });
                layer_row(ui, "Opacity", |ui| {
                    unit(ui, "Base:");
                    percent(ui, def, &mut patch, "displayOpacity", 10., 100.);
                    unit(ui, "%");
                    unit(ui, "Inactive:");
                    percent(ui, def, &mut patch, "inactiveOpacity", 0., 100.);
                    unit(ui, "%");
                });
                layer_row(ui, "Parallax", |ui| {
                    unit(ui, "X=");
                    percent(ui, def, &mut patch, "parallaxFactorX", -100., 100.);
                    unit(ui, "%, Y=");
                    ui.add_enabled_ui(!def["parallaxScaling"].as_bool().unwrap_or(false), |ui| {
                        ui.scope(|ui| {
                            if def["parallaxFactorX"] == def["parallaxFactorY"] {
                                ui.visuals_mut().override_text_color = Some(theme::LIGHT);
                            }
                            percent(ui, def, &mut patch, "parallaxFactorY", -100., 100.);
                        });
                    });
                    unit(ui, "%");
                    checkbox(ui, def, &mut patch, "parallaxScaling", "Scale ?");
                });
                // Source X edits follow Y while the axes are equal. Scaling always requires equal factors.
                if (patch.get("parallaxFactorX").is_some()
                    && def["parallaxFactorX"] == def["parallaxFactorY"])
                    || patch["parallaxScaling"] == true
                {
                    patch["parallaxFactorY"] = patch
                        .get("parallaxFactorX")
                        .unwrap_or(&def["parallaxFactorX"])
                        .clone();
                }
                // The source form retains the last row's bottom margin.
                ui.expand_to_include_y(ui.cursor().top());
            });
            if general_open {
                ui.add_space(6.);
            }
            let display_open = group(ui, "Display options", |ui| {
                layer_row(ui, "Color in layers list", |ui| {
                    if layer_color(ui, def, &mut patch, "uiColor") {
                        reset_layer_color(ui, &mut patch, "uiColor");
                    }
                });
                layer_row(ui, "Visual options", |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(3., 9.);
                    ui.add_space(4.);
                    ui.columns(2, |columns| {
                        for column in &mut *columns {
                            column.add_space(3.);
                        }
                        checkbox(
                            &mut columns[0],
                            def,
                            &mut patch,
                            "hideInList",
                            "Hide in list",
                        );
                        checkbox(
                            &mut columns[1],
                            def,
                            &mut patch,
                            "renderInWorldView",
                            "Show in world view",
                        );
                        checkbox(
                            &mut columns[0],
                            def,
                            &mut patch,
                            "canSelectWhenInactive",
                            "Allow selections when inactive",
                        );
                        checkbox(
                            &mut columns[1],
                            def,
                            &mut patch,
                            "useAsyncRender",
                            "Asynchronous render",
                        );
                    });
                });
                ui.add_space(3.);
                layer_row(ui, "Optional \"guide\" grid", |ui| {
                    ui.spacing_mut().item_spacing.x = 3.;
                    integer(ui, def, &mut patch, "guideGridWid", 0, 1024);
                    unit(ui, "x");
                    integer(ui, def, &mut patch, "guideGridHei", 0, 1024);
                    unit(ui, "px");
                });
                layer_row(ui, "Guide grid color", |ui| {
                    if layer_color(ui, def, &mut patch, "guideColor") {
                        unit(ui, "Opacity:");
                        percent(ui, def, &mut patch, "guideOpacity", 0., 100.);
                        unit(ui, "%");
                        reset_layer_color(ui, &mut patch, "guideColor");
                    }
                });
                self.layer_tag_row(ui, def, "UI tags", tag_editor::Field::LayerUi);
            });
            if display_open {
                ui.add_space(6.);
            }
            if kind == "Entities" || kind == "Tiles" {
                group(ui, &format!("{kind} settings"), |ui| {
                    if kind == "Entities" {
                        layer_row(ui, "Entity depth sorting", |ui| {
                            let choices = [
                                (
                                    "Manual",
                                    "Manual (CTRL-UP/CTRL-DOWN to change an entity depth)",
                                ),
                                ("YAscending", "Y ascending (higher Y in front)"),
                                ("YDescending", "Y descending (lower Y in front)"),
                            ];
                            let current = text(def, "entityDepthSorting", "Manual");
                            let selected = choices
                                .iter()
                                .position(|(value, _)| *value == current)
                                .unwrap_or(0);
                            let options =
                                choices.map(|(_, label)| (locale::tr(ui, label), Color32::WHITE));
                            let response = source_controls::searchable_select_button(
                                ui,
                                ui.available_width(),
                                options[selected].0,
                            );
                            if let Some((index, _)) = source_controls::searchable_select_popup(
                                &response,
                                response.id.with("entity-depth-sorting"),
                                selected,
                                &options,
                            ) && choices[index].0 != current
                            {
                                patch["entityDepthSorting"] = json!(choices[index].0);
                            }
                        });
                        layer_row(ui, "Hide entity fields when inactive", |ui| {
                            checkbox(ui, def, &mut patch, "hideFieldsWhenInactive", "");
                        });
                        self.layer_tag_row(
                            ui,
                            def,
                            "Required tags",
                            tag_editor::Field::RequiredEntities,
                        );
                        self.layer_tag_row(
                            ui,
                            def,
                            "Excluded tags",
                            tag_editor::Field::ExcludedEntities,
                        );
                        layer_row(ui, "Advanced operations", |ui| {
                            let label = locale::tr(ui, "Move existing entities to another layer")
                                .to_uppercase();
                            let size = egui::vec2(ui.available_width(), 33.);
                            if crate::app::project_dialogs::button(ui, &label, size, false, 13.333)
                                .clicked()
                            {
                                self.begin_entity_transfer(ui.ctx(), def["uid"].as_i64().unwrap());
                            }
                        });
                    } else {
                        layer_row(ui, "Tile pivot point", |ui| {
                            let pivot =
                                [number(def, "tilePivotX", 0.), number(def, "tilePivotY", 0.)];
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::splat(64.), Sense::click());
                            ui.painter().rect_filled(rect, 0., theme::DARK);
                            ui.painter().rect_stroke(
                                rect,
                                0.,
                                Stroke::new(1_f32, theme::LIGHT),
                                StrokeKind::Inside,
                            );
                            for y in 0..3 {
                                for x in 0..3 {
                                    let point = rect.min
                                        + egui::vec2(8. + 24. * x as f32, 8. + 24. * y as f32);
                                    let selected = pivot == [x as f32 * 0.5, y as f32 * 0.5];
                                    ui.painter().circle_filled(
                                        point,
                                        if selected { 5. } else { 3. },
                                        if selected {
                                            theme::ACCENT
                                        } else {
                                            theme::LIGHT
                                        },
                                    );
                                }
                            }
                            if response.clicked()
                                && let Some(pos) = response.interact_pointer_pos()
                            {
                                patch["tilePivotX"] = json!(
                                    ((pos.x - rect.left() - 8.) / 24.).round().clamp(0., 2.) * 0.5
                                );
                                patch["tilePivotY"] = json!(
                                    ((pos.y - rect.top() - 8.) / 24.).round().clamp(0., 2.) * 0.5
                                );
                            }
                        });
                    }
                });
            }
        });
        patch
    }

    pub(in super::super) fn source_generated_layer_settings(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
    ) {
        let intgrid = def["type"] == "IntGrid";
        group(
            ui,
            if intgrid {
                "IntGrid settings"
            } else {
                "Autolayer settings"
            },
            |ui| {
                if intgrid {
                    self.int_grid_values(ui, def);
                }
                self.auto_layer_controls(ui, def);
            },
        );
    }

    fn auto_layer_controls(&mut self, ui: &mut egui::Ui, def: &Value) {
        let uid = def["uid"].as_i64().unwrap();
        let project = self.project.as_ref().unwrap();
        let layers = project.json["defs"]["layers"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let fields = project.json["defs"]["levelFields"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let tilesets = project.json["defs"]["tilesets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let auto = def["type"] == "AutoLayer";
        let atlas = def["tilesetDefUid"].as_i64();
        let mut new_atlas = None;
        let source = def["autoSourceLayerDefUid"].as_i64();
        let mut patch = json!({});
        let mut edit_rules = false;
        let mut bake = false;
        if auto {
            layer_row(ui, "Auto-Layer source", |ui| {
                ui.vertical(|ui| {
                    let placeholder = locale::tr(ui, "-- Select an IntGrid layer --");
                    let mut candidates = vec![(None, placeholder)];
                    candidates.extend(
                        layers
                            .iter()
                            .filter(|layer| layer["type"] == "IntGrid")
                            .filter_map(|layer| {
                                Some((
                                    Some(layer["uid"].as_i64()?),
                                    text(layer, "identifier", locale::tr(ui, "Layer")),
                                ))
                            }),
                    );
                    let current = candidates.iter().position(|(id, _)| *id == source);
                    let options: Vec<_> =
                        candidates.iter().map(|(_, label)| (*label, None)).collect();
                    let title = current.map_or(placeholder, |index| options[index].0);
                    if let Some(index) = source_controls::form_select(
                        ui,
                        ("auto-layer-source", uid),
                        29.,
                        &options,
                        current,
                        title,
                        if source.is_none() {
                            FormSelectAppearance::Required
                        } else {
                            FormSelectAppearance::Normal
                        },
                    ) && candidates[index].0 != source
                    {
                        patch["autoSourceLayerDefUid"] = json!(candidates[index].0);
                    }
                    let green = Color32::from_rgb(101, 178, 117);
                    let width = ui.available_width();
                    let text = ui.painter().layout(
                        locale::tr(
                            ui,
                            "This auto-layer will use the INTGRID layer above to apply its rules.",
                        )
                        .into(),
                        FontId::new(15., egui::FontFamily::Name("panel-title".into())),
                        green,
                        (width - 72.).max(40.),
                    );
                    ui.add_space(3.);
                    let (hint, _) = ui.allocate_exact_size(
                        egui::vec2(width, (text.size().y + 12.).max(41.)),
                        Sense::hover(),
                    );
                    let painter = ui.painter_at(hint);
                    let border = hint.shrink(0.5);
                    painter.extend(egui::Shape::dashed_line(
                        &[
                            border.left_top(),
                            border.right_top(),
                            border.right_bottom(),
                            border.left_bottom(),
                            border.left_top(),
                        ],
                        Stroke::new(1_f32, green),
                        3.,
                        3.,
                    ));
                    let icon = hint.left_center() + egui::vec2(30., 10.);
                    painter.circle_filled(icon, 20., green);
                    painter.text(
                        icon,
                        Align2::CENTER_CENTER,
                        "?",
                        FontId::proportional(28.),
                        theme::MEDIUM,
                    );
                    painter.galley(
                        hint.left_center() + egui::vec2(62., -text.size().y / 2.),
                        text,
                        green,
                    );
                    ui.add_space(9.);
                });
            });
        }
        layer_row(ui, "Auto-layer tileset", |ui| {
            let can_edit_rules = atlas.is_some() && (!auto || source.is_some());
            let width = ui.available_width() - if can_edit_rules { 104. } else { 0. };
            new_atlas = tileset_picker(
                ui,
                width,
                tilesets,
                atlas,
                "-- Select a tileset --",
                auto && atlas.is_none(),
            );
            edit_rules = can_edit_rules && orange(ui, "EDIT RULES", egui::vec2(98., 29.)).clicked();
        });
        layer_row(ui, "Biome enum", |ui| {
            let biome_fields = fields
                .iter()
                .filter_map(|field| {
                    let kind = text(field, "__type", "");
                    let (_, name) = kind.split_once("Enum.")?;
                    let name = name.trim_end_matches('>');
                    Some((
                        field["uid"].as_i64()?,
                        format!("{} ({name})", text(field, "identifier", "Biome")),
                    ))
                })
                .collect::<Vec<_>>();
            let old = def["biomeFieldUid"].as_i64();
            let mut selected = old;
            let response = ui.add_enabled_ui(!biome_fields.is_empty(), |ui| {
                source_controls::form_combo(("auto-layer-biome", uid))
                    .width(ui.available_width())
                    .selected_text(
                        biome_fields
                            .iter()
                            .find(|(id, _)| Some(*id) == old)
                            .map_or_else(|| locale::tr(ui, "No biome enum"), |(_, label)| label),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected, None, locale::tr(ui, "No biome enum"));
                        for (id, label) in &biome_fields {
                            ui.selectable_value(&mut selected, Some(*id), label);
                        }
                    })
                    .response
            });
            if biome_fields.is_empty() {
                source_controls::disabled_hatch(ui, response.inner.rect);
            }
            if selected != old {
                patch["biomeFieldUid"] = json!(selected);
            }
        });
        if auto || atlas.is_some() {
            layer_row(
                ui,
                "Prevent auto-tiling here if another layer contains a value",
                |ui| {
                    ui.vertical(|ui| {
                        if let Some(blockers) = auto_layer_blockers(ui, def, layers, uid) {
                            patch["layerUidsPreventingAutoTilingHere"] = json!(blockers);
                        }
                    });
                },
            );
            layer_row(ui, "Advanced operations", |ui| {
                let icon = egui::Image::new(egui::include_image!("../../assets/icons/flatten.svg"))
                    .fit_to_exact_size(egui::vec2(16., 18.));
                bake = source_controls::add_sized_button(
                    ui,
                    egui::vec2(ui.available_width(), 35.),
                    egui::Button::image_and_text(icon, locale::tr(ui, "BAKE AUTO-LAYER"))
                        .fill(SOURCE_ORANGE)
                        .corner_radius(2.),
                )
                .clicked();
            });
        }
        let options = remap_options(ui, ("layer", uid), true, ldtk_core::TilesetRemap::TileIds);
        if let Some(atlas) = new_atlas {
            self.apply_layer_tileset(def, atlas, options);
        }
        if changed(&patch) {
            let apply = |p: &mut Project| p.update_layer_properties(uid, &patch);
            if patch.get("biomeFieldUid").is_some()
                && def["autoRuleGroups"]
                    .as_array()
                    .is_some_and(|groups| !groups.is_empty())
            {
                self.edit_recoverable("Auto-layer biome changed", apply);
            } else {
                self.edit("Auto-layer settings changed", apply);
            }
        }
        if edit_rules {
            let index = self.selected.and_then(|key| {
                self.project.as_ref()?.level(key)?["layerInstances"]
                    .as_array()?
                    .iter()
                    .position(|layer| layer["layerDefUid"] == uid)
            });
            if let Some(index) = index {
                self.switch_layer(index);
            }
            self.definition_uid = Some(uid);
            self.rule_uid = None;
            self.rule_group_uid = None;
            self.show_panel(Some(Panel::Rules));
        }
        if bake {
            self.begin_layer_bake(uid);
        }
    }
}

fn auto_layer_blockers(
    ui: &mut egui::Ui,
    def: &Value,
    layers: &[Value],
    uid: i64,
) -> Option<Vec<i64>> {
    let mut blockers = def["layerUidsPreventingAutoTilingHere"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            def["autoTilesKilledByOtherLayerUid"]
                .as_i64()
                .into_iter()
                .collect()
        });
    let placeholder = locale::tr(ui, "-- Add a layer --");
    let mut candidates = vec![(None, placeholder)];
    candidates.extend(layers.iter().filter_map(|layer| {
        let other = layer["uid"].as_i64()?;
        (other != uid
            && !blockers.contains(&other)
            && matches!(layer["type"].as_str(), Some("Tiles" | "IntGrid")))
        .then(|| {
            (
                Some(other),
                text(layer, "identifier", locale::tr(ui, "Layer")),
            )
        })
    }));
    let options: Vec<_> = candidates.iter().map(|(_, label)| (*label, None)).collect();
    let add = source_controls::form_select(
        ui,
        ("auto-blocker", uid),
        29.,
        &options,
        Some(0),
        placeholder,
        FormSelectAppearance::Normal,
    )
    .and_then(|index| candidates[index].0);
    let mut remove = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 4.;
        ui.add_space(3.);
        for &blocker in &blockers {
            let Some(layer) = layers.iter().find(|layer| layer["uid"] == blocker) else {
                continue;
            };
            egui::Frame::new()
                .stroke(Stroke::new(1_f32, theme::LIGHT))
                .inner_margin(3)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(layer_type_icon(text(layer, "type", "")))
                                .fit_to_exact_size(Vec2::splat(21.)),
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(215., 23.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(215.);
                                ui.add(
                                    egui::Label::new(text(layer, "identifier", "Layer")).truncate(),
                                )
                            },
                        );
                        let image =
                            egui::Image::new(egui::include_image!("../../assets/icons/delete.svg"))
                                .fit_to_exact_size(Vec2::splat(14.));
                        if source_controls::add_sized_button(
                            ui,
                            [27., 26.],
                            egui::Button::image(image).fill(SOURCE_ORANGE),
                        )
                        .action_tooltip(locale::tr(ui, "Remove blocker"))
                        .clicked()
                        {
                            remove = Some(blocker);
                        }
                    });
                });
        }
    });
    if let Some(uid) = add {
        blockers.push(uid);
    }
    if let Some(uid) = remove {
        blockers.retain(|value| *value != uid);
    }
    (add.is_some() || remove.is_some()).then_some(blockers)
}
