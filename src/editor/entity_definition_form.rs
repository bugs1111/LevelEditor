use super::*;

impl EditorApp {
    pub(super) fn source_entity_form(&mut self, ui: &mut egui::Ui, height: f32) {
        let ctx = ui.ctx().clone();
        let key = ui.make_persistent_id(("entity-options-atlas", self.definition_uid));
        self.with_local_atlas_state(&ctx, key, |app| {
            ui.spacing_mut().scroll = source_controls::scroll_style();
            ui.visuals_mut().clip_rect_margin = 0.;
            let bottom = ui.cursor().top() + height;
            let settings_max = (height - 156.).max(height * 0.5);
            let settings = egui::ScrollArea::vertical()
                .id_salt("entity-settings")
                .max_height(settings_max)
                .auto_shrink([false, true])
                .show(ui, |ui| app.source_entity_form_inner(ui));
            let Some(uid) = settings.inner else {
                return;
            };
            ui.add_space(
                if app.settings.ui_state_int("extraEntityDetails", None, 1) == 0 {
                    10.
                } else {
                    22.
                },
            );
            let field_height = (bottom - ui.cursor().top()).max(0.);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                ui.allocate_ui_with_layout(
                    egui::vec2(FIELD_LIST_WIDTH, field_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(FIELD_LIST_WIDTH);
                        ui.set_max_width(FIELD_LIST_WIDTH);
                        app.source_field_list(ui, Some(uid), field_height);
                    },
                );
                ui.add_space(FIELD_LIST_GAP);
                let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), field_height));
                let mut field = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                field.set_clip_rect(field.clip_rect().intersect(rect));
                egui::ScrollArea::vertical()
                    .id_salt(("entity-field-properties", app.field_uid))
                    .max_height(field_height)
                    .auto_shrink([false, false])
                    .show(&mut field, |ui| {
                        ui.spacing_mut().text_edit_width = 170.;
                        app.field_properties(ui);
                    });
            });
        });
    }
    fn source_entity_form_inner(&mut self, ui: &mut egui::Ui) -> Option<i64> {
        let Some(def) = self.definition_picker(ui, "entities") else {
            ui.label(locale::tr(ui, "Create an entity using the + button."));
            return None;
        };
        let uid = def["uid"].as_i64().unwrap();
        let advanced_id = ui.make_persistent_id("entity-pivot-advanced");
        if self.field_owner != Some(uid) {
            self.field_owner = Some(uid);
            self.field_uid = None;
            ui.data_mut(|data| data.remove::<bool>(advanced_id));
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
        let atlas = def["tilesetId"]
            .as_i64()
            .or_else(|| def["tileRect"]["tilesetUid"].as_i64())
            .unwrap_or(-1);
        let settings_open = self.settings.ui_state_int("extraEntityDetails", None, 1) != 0;
        let mut tag_change = None;
        let mut editor_atlas_choice = None;
        let mut ui_atlas_choice = None;
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
                tag_change = tag_editor::show(
                    ui,
                    self.project.as_ref().unwrap(),
                    tag_editor::Field::Definition("entities"),
                    uid,
                    &def["tags"],
                );
            });
            row_width(ui, "Size", 136., |ui| {
                use source_controls::{NumberStyle, number_input_sized};
                ui.spacing_mut().item_spacing.x = 3.;
                for (key, separator) in [("width", ""), ("height", "x")] {
                    if !separator.is_empty() {
                        ui.weak(separator);
                    }
                    let current = def[key].as_i64().unwrap_or(16).to_string();
                    if let Some(draft) = number_input_sized(
                        ui,
                        key,
                        &current,
                        NumberStyle::Plain { placeholder: None },
                        60.,
                    ) {
                        patch[key] =
                            json!(world_level_panel::parse_form_int(&draft).clamp(1, 2048));
                    }
                }
                if def["resizableX"] != true && def["resizableY"] != true {
                    return;
                }
                ui.add_space(12.);
                for (group, dimensions) in [
                    (
                        "Min:",
                        [
                            ("minWidth", "maxWidth", "resizableX"),
                            ("minHeight", "maxHeight", "resizableY"),
                        ],
                    ),
                    (
                        "Max:",
                        [
                            ("maxWidth", "minWidth", "resizableX"),
                            ("maxHeight", "minHeight", "resizableY"),
                        ],
                    ),
                ] {
                    if group == "Max:" {
                        ui.add_space(12.);
                    }
                    ui.weak(locale::tr(ui, group));
                    for (index, (key, other, axis)) in dimensions.into_iter().enumerate() {
                        if index == 1 {
                            ui.weak("x");
                        }
                        let enabled = def[axis] == true;
                        let current = def[key]
                            .as_i64()
                            .map(|size| size.to_string())
                            .unwrap_or_default();
                        let edit = ui.add_enabled_ui(enabled, |ui| {
                            number_input_sized(
                                ui,
                                key,
                                &current,
                                NumberStyle::Plain {
                                    placeholder: enabled.then_some("None"),
                                },
                                60.,
                            )
                        });
                        if let Some(draft) = edit.inner {
                            let bound = patch.get(other).unwrap_or(&def[other]).as_i64();
                            let maximum = i64::from(i16::MAX);
                            let (min, max) = if key.starts_with("min") {
                                (0, bound.unwrap_or(maximum))
                            } else {
                                (bound.unwrap_or(0), maximum)
                            };
                            let value = world_level_panel::parse_form_int(&draft).max(min).min(max);
                            patch[key] = if value > 0 { json!(value) } else { Value::Null };
                        }
                    }
                }
            });
            row_width(ui, "", 136., |ui| {
                ui.weak(locale::tr(ui, "Resizable:"));
                for (axis, label, dimension, min, max) in [
                    ("resizableX", "Width", "width", "minWidth", "maxWidth"),
                    ("resizableY", "Height", "height", "minHeight", "maxHeight"),
                ] {
                    let mut enabled = def[axis].as_bool().unwrap_or(false);
                    if source_checkbox(ui, &mut enabled, label).changed() {
                        patch[axis] = json!(enabled);
                        if enabled {
                            patch[min] = patch.get(dimension).unwrap_or(&def[dimension]).clone();
                        } else {
                            patch[min] = Value::Null;
                            patch[max] = Value::Null;
                        }
                    }
                    ui.add_space(12.);
                }
                let can_lock = ["resizableX", "resizableY"]
                    .iter()
                    .all(|&axis| patch.get(axis).unwrap_or(&def[axis]) == true);
                let mut locked = def["keepAspectRatio"].as_bool().unwrap_or(false);
                if ui
                    .add_enabled_ui(can_lock, |ui| {
                        source_checkbox(ui, &mut locked, "Keep aspect ratio")
                    })
                    .inner
                    .changed()
                {
                    patch["keepAspectRatio"] = json!(locked);
                }
            });
            row_width(ui, "Editor visual", 136., |ui| {
                const SHAPES: [&str; 3] = ["Rectangle", "Ellipse", "Cross"];
                let mut rgb = enum_color(&def);
                if source_colors::color_editor_sized(ui, &mut rgb, None, egui::vec2(62., 30.))
                    .changed()
                {
                    patch["color"] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                }
                let tilesets = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let render_mode = text(&def, "renderMode", "Rectangle");
                let is_tile = render_mode == "Tile";
                let choices = source_controls::grouped_tileset_choices(ui, tilesets);
                let mut options = SHAPES.map(|shape| (locale::tr(ui, shape), None)).to_vec();
                options[0].1 = Some(locale::tr(ui, "Shapes"));
                options.extend(choices.iter().map(|(_, option)| *option));

                let current = if is_tile {
                    choices
                        .iter()
                        .position(|(choice, _)| {
                            *choice == source_controls::TilesetChoice::Existing(atlas)
                        })
                        .map(|index| index + SHAPES.len())
                } else {
                    SHAPES
                        .iter()
                        .position(|shape| *shape == render_mode)
                        .or(Some(0))
                };
                let title =
                    current.map_or_else(|| locale::tr(ui, "Choose tileset"), |i| options[i].0);
                let reserved = if is_tile { 244. } else { 0. };
                let width = (ui.available_width() - reserved - 12.).max(40.);
                let choice = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, 29.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(width);
                            source_controls::form_select(
                                ui,
                                ("source-entity-atlas", uid),
                                29.,
                                &options,
                                current,
                                title,
                                source_controls::FormSelectAppearance::Normal,
                            )
                        },
                    )
                    .inner;
                if let Some(index) = choice.filter(|index| Some(*index) != current) {
                    let mode = if let Some(shape) = SHAPES.get(index) {
                        patch["tilesetId"] = Value::Null;
                        *shape
                    } else {
                        editor_atlas_choice = Some(choices[index - SHAPES.len()].0);
                        "Tile"
                    };
                    patch["renderMode"] = json!(mode);
                    patch["tileRect"] = Value::Null;
                    let next_is_tile = mode == "Tile";
                    if next_is_tile != is_tile {
                        patch["tileOpacity"] = json!(1);
                        patch["fillOpacity"] = json!(if next_is_tile { 0.08 } else { 1. });
                        patch["lineOpacity"] = json!(if next_is_tile { 0. } else { 1. });
                    }
                }
                if !is_tile {
                    return;
                }
                if atlas >= 0 {
                    ui.push_id(("entity-editor-tile", uid), |ui| {
                        if let Some(rect) = self.value_tile_thumbnail(
                            ui,
                            Some(atlas),
                            &def["tileRect"],
                            true,
                            false,
                        ) {
                            patch["tileRect"] = rect;
                        }
                    });
                }
                let modes = [
                    ("Cover", "Cover bounds"),
                    ("FitInside", "Fit inside bounds"),
                    ("Repeat", "Repeat"),
                    ("Stretch", "Dirty stretch to bounds"),
                    ("FullSizeCropped", "Full size (cropped in bounds)"),
                    ("FullSizeUncropped", "Full size (not cropped)"),
                    ("NineSlice", "9-slices scaling"),
                ];
                let current = text(&def, "tileRenderMode", "FitInside");
                let selected = modes.iter().position(|(mode, _)| *mode == current);
                let options = modes.map(|(_, label)| (locale::tr(ui, label), Color32::WHITE));
                let title = selected.map_or(current, |index| options[index].0);
                let width = ui.available_width().min(202.);
                let button = source_controls::searchable_select_button(ui, width, title);
                if let Some((index, _)) = source_controls::searchable_select_popup(
                    &button,
                    button.id.with(("entity-tile-render-mode", uid)),
                    selected.unwrap_or(0),
                    &options,
                ) && Some(index) != selected
                {
                    patch["tileRenderMode"] = json!(modes[index].0);
                }
            });

            if patch["tileRenderMode"]
                .as_str()
                .unwrap_or_else(|| text(&def, "tileRenderMode", "FitInside"))
                == "NineSlice"
            {
                let old = &def["nineSliceBorders"];
                let mut borders = if old.as_array().is_some_and(|values| values.len() == 4) {
                    std::array::from_fn(|i| old[i].as_i64().unwrap_or(2))
                } else {
                    [2; 4]
                };
                row_width(ui, "", 136., |ui| {
                    let help = ui.add(
                        egui::Image::new(egui::include_image!("../../assets/icons/help.svg"))
                            .fit_to_exact_size(Vec2::splat(32.))
                            .tint(source_controls::HELP_GREEN),
                    );
                    source_controls::help_tooltip(
                        help.on_hover_cursor(egui::CursorIcon::Help),
                        "9-slice scaling splits the texture into nine parts. Corners keep their size, edges repeat, and the center stretches. Border sizes are in pixels.",
                        false,
                    );
                    ui.label(locale::tr(ui, "9-slices borders:"));
                    for (index, key, label) in [
                        (0, "nineSliceUp", "top="),
                        (2, "nineSliceDown", "down="),
                        (3, "nineSliceLeft", "left="),
                        (1, "nineSliceRight", "right="),
                    ] {
                        ui.label(locale::tr(ui, label));
                        if let Some(value) = source_controls::number_input_sized(
                            ui,
                            key,
                            &borders[index].to_string(),
                            source_controls::NumberStyle::Plain { placeholder: None },
                            40.,
                        ) {
                            let value = if value.trim().is_empty() {
                                if index == 0 { 1 } else { borders[0] }
                            } else {
                                world_level_panel::parse_form_int(&value)
                                    .clamp(1, i64::from(i16::MAX))
                            };
                            if index == 0 && borders.iter().all(|border| *border == borders[0]) {
                                borders.fill(value);
                            } else {
                                borders[index] = value;
                            }
                        }
                    }
                });
                let next = json!(borders);
                if *old != next && !self.project.as_ref().unwrap().is_backup() {
                    patch["nineSliceBorders"] = next;
                }
            }
            row_width(ui, "", 136., |ui| {
                let is_tile = patch["renderMode"]
                    .as_str()
                    .unwrap_or_else(|| text(&def, "renderMode", "Rectangle"))
                    == "Tile";
                let mut hollow = def["hollow"].as_bool().unwrap_or(false);
                ui.spacing_mut().item_spacing.x = 4.;
                for (key, label, enabled) in [
                    ("tileOpacity", "Tile=", is_tile),
                    ("fillOpacity", "Fill=", !hollow),
                    ("lineOpacity", "Line=", true),
                ] {
                    ui.weak(locale::tr(ui, label));
                    let value = patch[key]
                        .as_f64()
                        .or_else(|| def[key].as_f64())
                        .unwrap_or(1.);
                    let percent = (value * 100. + 0.5).floor().to_string();
                    let edit = ui.add_enabled_ui(enabled, |ui| {
                        source_controls::number_input_sized(
                            ui,
                            key,
                            &percent,
                            source_controls::NumberStyle::PercentSlider { min: 0., max: 100. },
                            60.,
                        )
                    });
                    if let Some(draft) = edit.inner {
                        let percent = draft
                            .trim()
                            .trim_end_matches('%')
                            .trim_end()
                            .parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite())
                            .unwrap_or(0.)
                            .clamp(0., 100.);
                        patch[key] = json!((percent + 0.5).floor() / 100.);
                    }
                    ui.weak("%");
                }
                if source_checkbox(ui, &mut hollow, "Hollow").changed() {
                    patch["hollow"] = json!(hollow);
                }
            });
            row_width(ui, "Alt UI icon", 136., |ui| {
                let tile = &def["uiTileRect"];
                let atlas = tile["tilesetUid"].as_i64();
                let tilesets = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let reserved = if atlas.is_some() { 38. } else { 0. };
                let width = (ui.available_width() - reserved).clamp(40., 267.);
                ui_atlas_choice = source_controls::tileset_picker(
                    ui,
                    width,
                    tilesets,
                    atlas,
                    "-- Use default editor visual --",
                    false,
                );
                if atlas.is_some() {
                    let tile = if number(tile, "w", 0.) > 0. {
                        tile
                    } else {
                        &Value::Null
                    };
                    ui.push_id(("entity-ui-tile", uid), |ui| {
                        if let Some(rect) = self.value_tile_thumbnail(ui, atlas, tile, true, false)
                        {
                            patch["uiTileRect"] = rect;
                        }
                    });
                }
            });
            row_width(ui, "Pivot point", 136., |ui| {
                use source_controls::{NumberStyle, number_input_sized};
                let pivot = ["pivotX", "pivotY"].map(|key| def[key].as_f64().unwrap_or(0.));
                let advanced = pivot.iter().any(|ratio| ![0., 0.5, 1.].contains(ratio))
                    || ui
                        .data(|data| data.get_temp::<bool>(advanced_id))
                        .unwrap_or(false);
                ui.add_space(4.);
                if let Some([x, y]) = source_controls::pivot_grid(ui, pivot)
                    && pivot != [x, y]
                {
                    patch["pivotX"] = json!(x);
                    patch["pivotY"] = json!(y);
                }
                ui.add_space(2.);
                if !advanced {
                    if ui
                        .link(
                            RichText::new(locale::tr(ui, "Show advanced options"))
                                .color(theme::ACCENT)
                                .underline(),
                        )
                        .clicked()
                    {
                        ui.data_mut(|data| data.insert_temp(advanced_id, true));
                    }
                    return;
                }
                egui::Frame::new()
                    .inner_margin(4.)
                    .stroke(Stroke::new(1_f32, theme::DARK))
                    .corner_radius(3.)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        for (label, inputs, pixels) in [
                            ("Float:", ["customFloatX", "customFloatY"], false),
                            ("Pixels:", ["customPixelX", "customPixelY"], true),
                        ] {
                            if pixels {
                                ui.separator();
                            }
                            ui.weak(locale::tr(ui, label));
                            for (axis, (key, dimension)) in
                                [("pivotX", "width"), ("pivotY", "height")]
                                    .into_iter()
                                    .enumerate()
                            {
                                if axis == 1 {
                                    ui.weak(",");
                                }
                                let size = patch
                                    .get(dimension)
                                    .unwrap_or(&def[dimension])
                                    .as_f64()
                                    .unwrap_or(16.)
                                    .max(1.);
                                let ratio = patch[key].as_f64().unwrap_or(pivot[axis]);
                                let displayed = if pixels {
                                    (ratio * size).floor()
                                } else {
                                    (ratio * 10_000.).trunc() / 10_000.
                                };
                                let style = if pixels {
                                    NumberStyle::Plain { placeholder: None }
                                } else {
                                    NumberStyle::Float
                                };
                                if let Some(draft) = number_input_sized(
                                    ui,
                                    inputs[axis],
                                    &displayed.to_string(),
                                    style,
                                    40.,
                                ) {
                                    let value = if pixels {
                                        world_level_panel::parse_form_int(&draft).clamp(-2048, 2048)
                                            as f64
                                            / size
                                    } else {
                                        draft
                                            .trim()
                                            .parse::<f64>()
                                            .ok()
                                            .filter(|value| value.is_finite())
                                            .unwrap_or(0.)
                                            .clamp(-50., 50.)
                                    };
                                    patch[key] = json!(value);
                                }
                            }
                        }
                    });
            });
            row_width(ui, "Max count", 136., |ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let mut count = def["maxCount"].as_i64().unwrap_or(0);
                let display = if count == 0 {
                    String::new()
                } else {
                    count.to_string()
                };
                if let Some(value) = source_controls::number_input_sized(
                    ui,
                    "maxCount",
                    &display,
                    source_controls::NumberStyle::Plain {
                        placeholder: Some("Unlimited"),
                    },
                    90.,
                ) {
                    count = world_level_panel::parse_form_int(&value).clamp(0, 1024);
                    patch["maxCount"] = json!(count);
                }
                let mut limit_choice =
                    |ui: &mut egui::Ui, key: &str, default: &str, choices: [(&str, &str); 3]| {
                        let current = text(&def, key, default);
                        let selected = choices
                            .iter()
                            .position(|(value, _)| *value == current)
                            .unwrap_or(choices.len());
                        let options =
                            choices.map(|(_, label)| (locale::tr(ui, label), Color32::WHITE));
                        ui.add_enabled_ui(count > 0, |ui| {
                            let title = options.get(selected).map_or(current, |option| option.0);
                            let button = source_controls::searchable_select_button(ui, 110., title);
                            if let Some((index, _)) = source_controls::searchable_select_popup(
                                &button,
                                button.id.with((key, uid)),
                                selected,
                                &options,
                            ) && choices[index].0 != current
                            {
                                patch[key] = json!(choices[index].0);
                            }
                        });
                    };
                limit_choice(
                    ui,
                    "limitScope",
                    "PerLevel",
                    [
                        ("PerLayer", "per layer"),
                        ("PerLevel", "per level"),
                        ("PerWorld", "in the world"),
                    ],
                );
                ui.add_space(4.);
                ui.weak(locale::tr(ui, ", and"));
                ui.add_space(4.);
                limit_choice(
                    ui,
                    "limitBehavior",
                    "MoveLastOne",
                    [
                        ("DiscardOldOnes", "discard older ones"),
                        ("PreventAdding", "prevent adding more"),
                        ("MoveLastOne", "move the last one instead of adding"),
                    ],
                );
                ui.add_space(4.);
                let end = locale::tr(ui, "when max is reached.");
                ui.add(egui::Label::new(RichText::new(end).weak()).truncate())
                    .on_hover_text(end);
            });
        });
        if settings_next != settings_open {
            self.settings
                .set_ui_state_int("extraEntityDetails", None, i64::from(settings_next));
        }
        if (changed(&patch) || ui_atlas_choice.is_some())
            && self
                .edit("Entity definition changed", |p| {
                    if let Some(choice) = editor_atlas_choice {
                        patch["tilesetId"] = match choice {
                            source_controls::TilesetChoice::Unassigned => Value::Null,
                            source_controls::TilesetChoice::Existing(uid) => json!(uid),
                            source_controls::TilesetChoice::EmbeddedIcons => {
                                json!(p.ensure_embedded_icons()?)
                            }
                        };
                    }
                    if let Some(choice) = ui_atlas_choice {
                        let atlas = match choice {
                            source_controls::TilesetChoice::Unassigned => None,
                            source_controls::TilesetChoice::Existing(uid) => Some(uid),
                            source_controls::TilesetChoice::EmbeddedIcons => {
                                Some(p.ensure_embedded_icons()?)
                            }
                        };
                        patch["uiTileRect"] = atlas.map_or(
                            Value::Null,
                            |uid| json!({"tilesetUid":uid,"x":0,"y":0,"w":0,"h":0}),
                        );
                    }
                    p.update_entity_definition(uid, &patch)
                })
                .is_some()
        {
            ui.data_mut(|data| data.remove::<bool>(advanced_id));
        }
        if let Some(change) = tag_change {
            self.edit("Entity tags changed", |p| match change {
                tag_editor::Change::Set(value) => {
                    p.update_entity_definition(uid, &json!({"tags":value}))
                }
                tag_editor::Change::Rename {
                    old,
                    new,
                    everywhere,
                } => p.rename_definition_tag("entities", uid, &old, &new, everywhere),
            });
        }
        Some(uid)
    }
}
