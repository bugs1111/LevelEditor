//! Common layer settings from the pinned LDtk EditLayerDefs form.
use super::world_level_panel::text_input;
use super::*;

fn group(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let id = ui.make_persistent_id(title);
    let open = ui.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let open = section(ui, title, open, |ui| {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.;
                body(ui);
            });
    });
    ui.data_mut(|d| d.insert_temp(id, open));
}
fn checkbox(ui: &mut egui::Ui, def: &Value, patch: &mut Value, key: &str, label: &str) {
    let mut value = def[key].as_bool().unwrap_or(false);
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
    if let Some(draft) = text_input(ui, key, &display, 56.) {
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
    let current = def[key].as_f64().unwrap_or(0.);
    let display = format!("{:.0}", (current * 100. + 0.5).floor());
    if let Some(draft) = text_input(ui, key, &display, 56.) {
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
    ui.label("%");
}

impl EditorApp {
    pub(in super::super) fn layer_tileset_assignment(&mut self, ui: &mut egui::Ui, def: &Value) {
        let uid = def["uid"].as_i64().unwrap();
        let options = remap_options(ui, ("layer", uid), true, ldtk_core::TilesetRemap::TileIds);
        let mut selected = def["tilesetDefUid"].as_i64().unwrap_or(-1);
        let previous = selected;
        self.tileset_choice(ui, "Tileset", &mut selected);
        if selected != previous
            && let Some(discarded) = self.edit_recoverable("Tileset assigned", |p| {
                p.assign_tileset_with_policy(
                    uid,
                    (selected >= 0).then_some(selected),
                    options.mode,
                    options.clear_overrides,
                    options.allow_loss,
                )
            })
            && discarded > 0
        {
            self.status = format!("Tileset assigned; removed {discarded} references");
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
            group(ui, "General layer settings", |ui| {
                row(ui, "Type", |ui| {
                    let icon = match kind {
                        "Entities" => egui::include_image!("../../assets/icons/entity.png"),
                        "IntGrid" => egui::include_image!("../../assets/icons/intGrid.png"),
                        "AutoLayer" => egui::include_image!("../../assets/icons/autoLayer.svg"),
                        _ => egui::include_image!("../../assets/icons/tile.png"),
                    };
                    ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.)));
                    ui.label(kind);
                });
                for (key, label) in [("identifier", "Layer identifier"), ("doc", "Documentation")] {
                    row(ui, label, |ui| {
                        if let Some(value) =
                            text_input(ui, key, text(def, key, ""), ui.available_width())
                        {
                            patch[key] = json!(value);
                        }
                    });
                }
                if kind == "Tiles" {
                    row(ui, "Default tileset", |ui| {
                        ui.vertical(|ui| self.layer_tileset_assignment(ui, def));
                    });
                }
                row(ui, "Grid size", |ui| {
                    integer(ui, def, &mut patch, "gridSize", 1, 1024);
                    ui.label("px");
                });
                row(ui, "Offsets", |ui| {
                    ui.label("X=");
                    integer(ui, def, &mut patch, "pxOffsetX", -32768, 32767);
                    ui.label("px, Y=");
                    integer(ui, def, &mut patch, "pxOffsetY", -32768, 32767);
                    ui.label("px");
                });
                row(ui, "Opacity", |ui| {
                    ui.label("Base:");
                    percent(ui, def, &mut patch, "displayOpacity", 10., 100.);
                    ui.label("Inactive:");
                    percent(ui, def, &mut patch, "inactiveOpacity", 0., 100.);
                });
                row(ui, "Parallax", |ui| {
                    ui.label("X=");
                    percent(ui, def, &mut patch, "parallaxFactorX", -100., 100.);
                    ui.label(", Y=");
                    ui.add_enabled_ui(!def["parallaxScaling"].as_bool().unwrap_or(false), |ui| {
                        ui.scope(|ui| {
                            if def["parallaxFactorX"] == def["parallaxFactorY"] {
                                ui.visuals_mut().override_text_color = Some(theme::LIGHT);
                            }
                            percent(ui, def, &mut patch, "parallaxFactorY", -100., 100.);
                        });
                    });
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
            });
            ui.add_space(12.);
            group(ui, "Display options", |ui| {
                row(ui, "Color in layers list", |ui| {
                    let mut color = def["uiColor"]
                        .as_str()
                        .map(theme::color)
                        .map(|c| [c.r(), c.g(), c.b()]);
                    if source_colors::nullable_color_editor(ui, &mut color).changed() {
                        patch["uiColor"] = color
                            .map(|c| json!(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])))
                            .unwrap_or(Value::Null);
                    }
                    if color.is_some() && source_controls::small_button(ui, "Reset").clicked() {
                        patch["uiColor"] = Value::Null;
                    }
                });
                row(ui, "Visual options", |ui| {
                    ui.vertical(|ui| {
                        checkbox(ui, def, &mut patch, "hideInList", "Hide in list");
                        if kind != "Entities" {
                            checkbox(
                                ui,
                                def,
                                &mut patch,
                                "renderInWorldView",
                                "Show in world view",
                            );
                        }
                        if kind != "AutoLayer" {
                            checkbox(
                                ui,
                                def,
                                &mut patch,
                                "canSelectWhenInactive",
                                "Allow selections when inactive",
                            );
                        }
                    });
                });
                row(ui, "Optional \"guide\" grid", |ui| {
                    integer(ui, def, &mut patch, "guideGridWid", 0, 1024);
                    ui.label("x");
                    integer(ui, def, &mut patch, "guideGridHei", 0, 1024);
                    ui.label("px");
                });
                row(ui, "UI tags", |ui| {
                    if let Some(value) = tags(ui, "", &def["uiFilterTags"]) {
                        patch["uiFilterTags"] = value;
                    }
                });
            });
            if kind == "Entities" || kind == "Tiles" {
                ui.add_space(12.);
                group(ui, &format!("{kind} settings"), |ui| {
                    if kind == "Entities" {
                        row(ui, "Hide entity fields when inactive", |ui| {
                            checkbox(ui, def, &mut patch, "hideFieldsWhenInactive", "");
                        });
                    } else {
                        row(ui, "Tile pivot point", |ui| {
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
}
