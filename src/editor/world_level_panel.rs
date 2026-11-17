//! Source WorldPanel and LevelInstancePanel forms.
use super::*;

const WORLD_LAYOUTS: [(&str, &str); 4] = [
    ("Free", "2D free map - Freely positioned in space"),
    (
        "GridVania",
        "GridVania - Levels are positioned inside a large world-scale grid",
    ),
    ("LinearHorizontal", "Horizontal - One level after the other"),
    ("LinearVertical", "Vertical - One level after the other"),
];
const BACKGROUND_POSITIONS: [(&str, &str); 5] = [
    ("Unscaled", "Not scaled"),
    ("Contain", "Fit inside (keep aspect ratio)"),
    ("Cover", "Cover level (keep aspect ratio)"),
    ("CoverDirty", "Cover (dirty scaling)"),
    ("Repeat", "Repeat"),
];

fn icon_button(
    ui: &mut egui::Ui,
    image: egui::ImageSource<'static>,
    help: &str,
    fill: Color32,
    size: Vec2,
) -> egui::Response {
    let button =
        egui::Button::image(egui::Image::new(image).fit_to_exact_size(egui::vec2(16., 16.)))
            .fill(fill);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(4., 2.);
        source_controls::add_sized_button(ui, size, button).on_hover_text(help)
    })
    .inner
}

fn text_input(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let (original, mut draft) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .filter(|(original, _)| original == current)
        .unwrap_or_else(|| (current.into(), current.into()));
    ui_commit::prepare_editor(ui.ctx(), id);
    let response = ui.add_sized(
        [width.max(20.), 33.],
        egui::TextEdit::singleline(&mut draft).id(id),
    );
    ui_commit::track_editor(ui.ctx(), &response);
    let commit = (response.lost_focus()
        || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
        && draft != current;
    ui.data_mut(|d| {
        if commit {
            d.remove::<(String, String)>(id);
        } else {
            d.insert_temp(id, (original, draft.clone()));
        }
    });
    commit.then_some(draft)
}

fn int_input(
    ui: &mut egui::Ui,
    key: &'static str,
    current: &Value,
    minimum: i64,
    maximum: i64,
    patch: &mut Value,
) {
    if let Some(draft) = text_input(ui, key, &current.as_i64().unwrap_or(0).to_string(), 56.)
        && let Ok(number) = draft.trim().parse::<i64>()
    {
        patch[key] = json!(number.clamp(minimum, maximum));
    }
}

fn iid_row(ui: &mut egui::Ui, iid: &str, width: f32, help: &str) {
    row_width(ui, "IID", width, |ui| {
        let mut value = iid.to_owned();
        ui.add_sized(
            [(ui.available_width() - 36.).max(20.), 33.],
            egui::TextEdit::singleline(&mut value)
                .interactive(false)
                .font(egui::TextStyle::Small),
        );
        if icon_button(
            ui,
            egui::include_image!("../../assets/icons/copy.svg"),
            help,
            theme::LIGHT,
            egui::vec2(30., 33.),
        )
        .clicked()
        {
            ui.ctx().copy_text(iid.to_owned());
        }
    });
}

fn group(ui: &mut egui::Ui, key: &'static str, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let id = ui.make_persistent_id(key);
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

impl EditorApp {
    pub(super) fn source_world_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else { return };
        let multi = project.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.contains(&json!("MultiWorlds")));
        let current_world = self.active_world;
        let world = current_world
            .and_then(|i| project.json["worlds"].get(i))
            .unwrap_or(&project.json)
            .clone();
        let name = text(&world, "identifier", "World").to_owned();
        let iid = if current_world.is_some() {
            text(&world, "iid", "")
        } else {
            text(&world, "dummyWorldIid", "")
        }
        .to_owned();
        let default_grid = number(&project.json, "defaultGridSize", 16.) as i64;
        if multi {
            let worlds = project.json["worlds"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, w)| (i, text(w, "identifier", "World").to_owned()))
                .collect::<Vec<_>>();
            ui.horizontal(|ui| {
                let mut selected = current_world;
                egui::ComboBox::from_id_salt("world-properties")
                    .width((ui.available_width() - 72.).max(40.))
                    .truncate()
                    .selected_text(&name)
                    .show_ui(ui, |ui| {
                        for (index, name) in &worlds {
                            ui.selectable_value(&mut selected, Some(*index), name);
                        }
                    });
                if selected != current_world {
                    self.navigate_world(selected);
                }
                if icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/add.svg"),
                    "Create a new world",
                    SOURCE_ORANGE,
                    egui::vec2(30., 30.),
                )
                .clicked()
                {
                    self.create_form_world();
                }
                if icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/delete.svg"),
                    "Delete current world",
                    Color32::from_rgb(137, 37, 43),
                    egui::vec2(30., 30.),
                )
                .clicked()
                    && let Some(world) = current_world
                {
                    self.queue_world_form_action(
                        ui.ctx(),
                        world_form_dialog::WorldFormAction::DeleteWorld(world),
                    );
                }
            });
            ui.add_space(8.);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.;
            let create_width = (ui.available_width() - 72.).max(40.);
            let enabled = !multi || current_world.is_some();
            ui.add_enabled_ui(enabled, |ui| {
                let button = egui::Button::image_and_text(
                    egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                        .fit_to_exact_size(egui::vec2(16., 16.)),
                    RichText::new("CREATE A NEW LEVEL").color(Color32::WHITE),
                )
                .fill(SOURCE_ORANGE);
                if source_controls::add_sized_button(ui, [create_width, 42.], button).clicked() {
                    self.start_level_spot_picker();
                }
            });
            ui.add_enabled_ui(self.selected.is_some(), |ui| {
                if icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/copy.svg"),
                    "Duplicate current level",
                    SOURCE_ORANGE,
                    egui::vec2(30., 42.),
                )
                .clicked()
                    && let Some(key) = self.selected
                {
                    self.queue_world_form_action(
                        ui.ctx(),
                        world_form_dialog::WorldFormAction::DuplicateLevel(key),
                    );
                }
                if icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/delete.svg"),
                    "Delete current level",
                    Color32::from_rgb(137, 37, 43),
                    egui::vec2(30., 42.),
                )
                .clicked()
                    && let Some(key) = self.selected
                {
                    self.queue_world_form_action(
                        ui.ctx(),
                        world_form_dialog::WorldFormAction::DeleteLevel(key),
                    );
                }
            });
        });
        ui.add_space(16.);
        let mut patch = json!({});
        group(
            ui,
            "worldPanel_world",
            &format!("WORLD SETTINGS: \"{name}\""),
            |ui| {
                iid_row(ui, &iid, 106., "Copy world IID");
                row_width(ui, "World identifier", 106., |ui| {
                    ui.add_enabled_ui(multi, |ui| {
                        if let Some(next) = text_input(ui, "worldId", &name, ui.available_width()) {
                            patch["identifier"] = json!(next);
                        }
                    });
                });
                row_width(ui, "World layout", 106., |ui| {
                    let current = text(&world, "worldLayout", "Free");
                    let mut layout = current.to_owned();
                    egui::ComboBox::from_id_salt("worldLayout")
                        .width(ui.available_width())
                        .truncate()
                        .selected_text(
                            WORLD_LAYOUTS
                                .iter()
                                .find(|(v, _)| *v == current)
                                .map_or(current, |(_, label)| *label),
                        )
                        .show_ui(ui, |ui| {
                            for (value, label) in WORLD_LAYOUTS {
                                ui.selectable_value(&mut layout, value.into(), label);
                            }
                        });
                    if layout != current {
                        patch["worldLayout"] = json!(layout);
                    }
                });
                if world["worldLayout"] == "GridVania" {
                    row_width(ui, "Gridvania grid size", 106., |ui| {
                        int_input(
                            ui,
                            "worldGridWidth",
                            &world["worldGridWidth"],
                            1,
                            65536,
                            &mut patch,
                        );
                        ui.label("x");
                        int_input(
                            ui,
                            "worldGridHeight",
                            &world["worldGridHeight"],
                            1,
                            65536,
                            &mut patch,
                        );
                        ui.label("px");
                    });
                }
                row_width(ui, "Default \"new level\" size", 106., |ui| {
                    int_input(
                        ui,
                        "defaultLevelWidth",
                        &world["defaultLevelWidth"],
                        default_grid,
                        9999,
                        &mut patch,
                    );
                    ui.label("x");
                    int_input(
                        ui,
                        "defaultLevelHeight",
                        &world["defaultLevelHeight"],
                        default_grid,
                        9999,
                        &mut patch,
                    );
                    ui.label("px");
                });
            },
        );
        if changed(&patch) {
            self.queue_world_form_action(
                ui.ctx(),
                world_form_dialog::WorldFormAction::Settings {
                    world: current_world,
                    patch,
                },
            );
        }
        ui.add_space(16.);
        self.source_level_form(ui);
    }

    pub(super) fn source_level_form(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.selected else { return };
        let Some(mut level) = self.project.as_ref().and_then(|p| p.level(key)).cloned() else {
            return;
        };
        let saved_position = [
            number(&level, "worldX", 0.) as i32,
            number(&level, "worldY", 0.) as i32,
        ];
        let project = self.project.as_ref().unwrap();
        let world = key
            .world
            .and_then(|index| project.json["worlds"].get(index))
            .unwrap_or(&project.json);
        if text(world, "worldLayout", "Free").starts_with("Linear")
            && let Some((_, bounds)) = canvas::placements(project, key)
                .into_iter()
                .find(|(candidate, _)| *candidate == key)
        {
            level["worldX"] = json!(bounds[0] as i64);
            level["worldY"] = json!(bounds[1] as i64);
        }
        let default_grid =
            number(&self.project.as_ref().unwrap().json, "defaultGridSize", 16.) as i64;
        let default_background = text(
            &self.project.as_ref().unwrap().json,
            "defaultLevelBgColor",
            "#696A79",
        )
        .to_owned();
        let name = text(&level, "identifier", "Level");
        let mut patch = json!({});
        let mut depth_move = 0;
        group(
            ui,
            "levelPanel_level",
            &format!("LEVEL PROPERTIES: \"{name}\""),
            |ui| {
                iid_row(ui, text(&level, "iid", ""), 98., "Copy level IID");
                row_width(ui, "Level identifier", 98., |ui| {
                    let mut auto = level["useAutoIdentifier"].as_bool().unwrap_or(false);
                    let width = (ui.available_width() - 66.).max(20.);
                    ui.add_enabled_ui(!auto, |ui| {
                        if let Some(value) = text_input(ui, "identifier", name, width) {
                            patch["identifier"] = json!(value);
                        }
                    });
                    if source_checkbox(ui, &mut auto, "auto").changed() {
                        patch["useAutoIdentifier"] = json!(auto);
                    }
                });
                row_width(ui, "World depth", 98., |ui| {
                    int_input(
                        ui,
                        "worldDepth",
                        &level["worldDepth"],
                        i32::MIN.into(),
                        i32::MAX.into(),
                        &mut patch,
                    );
                    let depths = self
                        .project
                        .as_ref()
                        .unwrap()
                        .level_keys()
                        .into_iter()
                        .filter(|other| other.world == key.world && *other != key)
                        .filter_map(|other| {
                            self.project.as_ref().unwrap().level(other)?["worldDepth"].as_i64()
                        })
                        .collect::<Vec<_>>();
                    let depth = level["worldDepth"].as_i64().unwrap_or(0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        ui.add_enabled_ui(depths.iter().any(|d| *d >= depth), |ui| {
                            if icon_button(
                                ui,
                                egui::include_image!("../../assets/icons/up.svg"),
                                "Move level further",
                                theme::LIGHT,
                                egui::vec2(30., 20.),
                            )
                            .clicked()
                            {
                                depth_move = 1;
                            }
                        });
                        ui.add_enabled_ui(depths.iter().any(|d| *d <= depth), |ui| {
                            if icon_button(
                                ui,
                                egui::include_image!("../../assets/icons/down.svg"),
                                "Move level closer",
                                theme::LIGHT,
                                egui::vec2(30., 20.),
                            )
                            .clicked()
                            {
                                depth_move = -1;
                            }
                        });
                    });
                });
                row_width(ui, "World coords", 98., |ui| {
                    int_input(
                        ui,
                        "worldX",
                        &level["worldX"],
                        i32::MIN.into(),
                        i32::MAX.into(),
                        &mut patch,
                    );
                    ui.label(",");
                    int_input(
                        ui,
                        "worldY",
                        &level["worldY"],
                        i32::MIN.into(),
                        i32::MAX.into(),
                        &mut patch,
                    );
                    ui.label("px");
                });
                row_width(ui, "Size", 98., |ui| {
                    int_input(ui, "pxWid", &level["pxWid"], default_grid, 4096, &mut patch);
                    ui.label("x");
                    int_input(ui, "pxHei", &level["pxHei"], default_grid, 4096, &mut patch);
                    ui.label("px");
                });
                row_width(ui, "Bg color", 98., |ui| {
                    let col = theme::color(text(&level, "bgColor", &default_background));
                    let mut rgb = [col.r(), col.g(), col.b()];
                    if source_colors::color_editor_tag(ui, &mut rgb, Some("bg")).changed() {
                        let color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                        patch["bgColor"] = if color.eq_ignore_ascii_case(&default_background) {
                            Value::Null
                        } else {
                            json!(color)
                        };
                    }
                    if level["bgColor"].is_null() {
                        ui.label("(default)");
                    } else if ui.link("Use default").clicked() {
                        patch["bgColor"] = Value::Null;
                    }
                });
                row_width(ui, "Bg image", 98., |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let path = level["bgRelPath"].as_str();
                            let label = path
                                .and_then(|p| std::path::Path::new(p).file_name())
                                .and_then(|p| p.to_str())
                                .unwrap_or("[ No image ]");
                            if source_controls::add_sized_button(
                                ui,
                                [
                                    (ui.available_width() - if path.is_some() { 36. } else { 0. })
                                        .max(20.),
                                    30.,
                                ],
                                egui::Button::new(label),
                            )
                            .on_hover_text(path.unwrap_or("Pick a background image"))
                            .clicked()
                                && let Some(path) = ui_fields::pick_file(
                                    ui,
                                    "PickImage",
                                    &["png", "gif", "jpg", "jpeg", "aseprite", "ase"],
                                )
                            {
                                patch["bgRelPath"] = json!(path.to_string_lossy());
                            }
                            if path.is_some()
                                && source_controls::small_button(ui, "×")
                                    .on_hover_text("Stop using this image")
                                    .clicked()
                            {
                                patch["bgRelPath"] = Value::Null;
                            }
                        });
                        if let Some(path) = level["bgRelPath"].as_str() {
                            let current = text(&level, "bgPos", "Cover");
                            let mut position = current.to_owned();
                            egui::ComboBox::from_id_salt("bgPos")
                                .width(ui.available_width())
                                .truncate()
                                .selected_text(
                                    BACKGROUND_POSITIONS
                                        .iter()
                                        .find(|(v, _)| *v == current)
                                        .map_or(current, |(_, label)| *label),
                                )
                                .show_ui(ui, |ui| {
                                    for (value, label) in BACKGROUND_POSITIONS {
                                        ui.selectable_value(&mut position, value.into(), label);
                                    }
                                });
                            if position != current {
                                patch["bgPos"] = json!(position);
                            }
                            let path = canvas::asset_path(self.project.as_ref().unwrap(), path);
                            if let Some(texture) = self.assets.texture(ui.ctx(), &path)
                                && let Some(pivot) = ui_fields::pivot_editor(
                                    ui,
                                    "background-pivot",
                                    [
                                        number(&level, "bgPivotX", 0.5),
                                        number(&level, "bgPivotY", 0.5),
                                    ],
                                    texture.size_vec2().into(),
                                )
                            {
                                patch["bgPivotX"] = json!(pivot[0]);
                                patch["bgPivotY"] = json!(pivot[1]);
                            }
                        }
                    });
                });
            },
        );
        if changed(&patch) {
            let resizing = patch.get("pxWid").is_some() || patch.get("pxHei").is_some();
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                snap_level_form_patch(p, key, &mut patch)?;
                relative_path_patch(p, &mut patch, "bgRelPath")?;
                normalize_background_patch(&level, &mut patch);
                if let Some(identifier) = patch["identifier"].as_str() {
                    let mut name = p.cleanup_identifier(identifier);
                    if name.is_empty() || name == "_" {
                        name = p.cleanup_identifier("Unnamed");
                    }
                    patch["identifier"] = json!(unique_name(
                        &name,
                        p.level_keys()
                            .into_iter()
                            .filter(|other| *other != key)
                            .filter_map(|other| p.level(other)?["identifier"].as_str())
                    ));
                }
                if let Some(path) = patch["bgRelPath"].as_str() {
                    ldtk_core::exports::load_image_rgba(&canvas::asset_path(p, path))?;
                }
                if resizing {
                    p.resize_level(
                        key,
                        patch["pxWid"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxWid", 256.) as i64)
                            as i32,
                        patch["pxHei"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxHei", 256.) as i64)
                            as i32,
                        0,
                        0,
                    )?;
                }
                let target = p
                    .level_mut(key)
                    .ok_or_else(|| ldtk_core::Error::Invalid("level missing".into()))?;
                apply_patch(target, &patch);
                p.refresh_derived_data()?;
                p.rebuild_auto_layers(key)
            };
            if resizing {
                self.edit_level_recoverable("Level resized", key, operation);
            } else {
                self.edit("Level properties changed", operation);
            }
        }
        if depth_move != 0 {
            self.edit("Level depth changed", |p| {
                let depth = level["worldDepth"].as_i64().unwrap_or(0) as i32 + depth_move;
                p.move_level(key, saved_position[0], saved_position[1], depth)?;
                if depth_move > 0 {
                    let keys = p
                        .level_keys()
                        .into_iter()
                        .filter(|other| other.world == key.world)
                        .collect::<Vec<_>>();
                    let lowest = keys
                        .iter()
                        .filter_map(|other| p.level(*other)?["worldDepth"].as_i64())
                        .min()
                        .unwrap_or(0);
                    if lowest > 0 {
                        for other in keys {
                            let l = p.level(other).unwrap().clone();
                            p.move_level(
                                other,
                                number(&l, "worldX", 0.) as i32,
                                number(&l, "worldY", 0.) as i32,
                                number(&l, "worldDepth", 0.) as i32 - lowest as i32,
                            )?;
                        }
                    }
                }
                Ok(())
            });
            self.world_depth = self
                .project
                .as_ref()
                .and_then(|p| p.level(key))
                .and_then(|l| l["worldDepth"].as_i64())
                .unwrap_or(0);
        }
        ui.add_space(16.);
        group(ui, "levelPanel_fields", "CUSTOM FIELDS", |ui| {
            if level["fieldInstances"].as_array().is_none_or(Vec::is_empty) {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.), Sense::hover());
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                    .paint_at(ui, rect);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "No custom field.",
                    FontId::proportional(12.667),
                    theme::LIGHT,
                );
            } else {
                self.instance_fields(ui, key, None, &level);
            }
            if source_controls::add_sized_button(
                ui,
                [ui.available_width(), 36.],
                egui::Button::new(RichText::new("EDIT CUSTOM FIELDS").color(Color32::WHITE))
                    .fill(SOURCE_ORANGE),
            )
            .clicked()
            {
                self.field_owner = None;
                self.field_uid = None;
                self.show_panel(Some(Panel::Fields));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> (EditorApp, egui::Context) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.repaint = Some(ctx.clone());
        (app, ctx)
    }
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700., 1500.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut form = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(Rect::from_min_size(
                                ui.next_widget_position(),
                                egui::vec2(if panel == Panel::World { 368. } else { 450. }, 1450.),
                            ))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    if panel == Panel::World {
                        app.source_world_form(&mut form);
                    } else {
                        app.source_level_form(&mut form);
                    }
                });
                app.world_form_dialog(ctx);
            },
        )
    }
    fn label(output: &egui::FullOutput, expected: &str) -> egui::Pos2 {
        fn find(shape: &egui::Shape, expected: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text.replace('\n', " ") == expected => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, expected)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, expected))
            .unwrap_or_else(|| panic!("missing source form label {expected}"))
    }
    fn click_at(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        pos: egui::Pos2,
    ) -> egui::FullOutput {
        frame(app, ctx, panel, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                panel,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        frame(app, ctx, panel, vec![])
    }
    fn click(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        expected: &str,
    ) -> egui::FullOutput {
        frame(app, ctx, panel, vec![]);
        let output = frame(app, ctx, panel, vec![]);
        click_at(app, ctx, panel, label(&output, expected))
    }
    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    fn replace_text(app: &mut EditorApp, ctx: &egui::Context, panel: Panel, value: &str) {
        frame(
            app,
            ctx,
            panel,
            vec![key(
                Key::A,
                egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            )],
        );
        frame(app, ctx, panel, vec![egui::Event::Text(value.into())]);
        frame(
            app,
            ctx,
            panel,
            vec![key(Key::Enter, egui::Modifiers::NONE)],
        );
    }
    #[test]
    fn source_world_embeds_world_level_and_custom_field_groups() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&output, "CREATE A NEW LEVEL");
        label(&output, "WORLD SETTINGS: \"World\"");
        label(&output, "LEVEL PROPERTIES: \"Level_0\"");
        label(&output, "CUSTOM FIELDS");
        label(&output, "Default \"new level\" size");
        label(&output, "World coords");
    }
    #[test]
    fn source_level_uses_paired_coords_size_and_default_background() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        for expected in [
            "World coords",
            "Size",
            "Bg color",
            "Bg image",
            "(default)",
            "EDIT CUSTOM FIELDS",
        ] {
            label(&output, expected);
        }
    }
    #[test]
    fn source_linear_level_coords_show_runtime_positions_without_editing_saved_sentinels() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["worldLayout"] = json!("LinearHorizontal");
        let key = app.selected.unwrap();
        let level = app.project.as_mut().unwrap().level_mut(key).unwrap();
        level["worldX"] = json!(-1);
        level["worldY"] = json!(-1);
        let original = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "-1")
        }));
        label(&output, "0");
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
    #[test]
    fn source_linear_depth_arrow_preserves_saved_position_sentinels() {
        let (mut app, ctx) = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_level(None, "Other", 256, 256).unwrap();
        project.json["worldLayout"] = json!("LinearHorizontal");
        let level = project.level_mut(key).unwrap();
        level["worldX"] = json!(-1);
        level["worldY"] = json!(-1);
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let pos = egui::pos2(201., label(&output, "World depth").y - 12.);
        click_at(&mut app, &ctx, Panel::Level, pos);
        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["worldDepth"], 1, "the arrow must change depth");
        assert_eq!(
            (level["worldX"].clone(), level["worldY"].clone()),
            (json!(-1), json!(-1))
        );
    }
    #[test]
    fn source_world_duplicate_cancel_and_confirm_use_existing_transaction() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        let pos = egui::pos2(325., label(&output, "CREATE A NEW LEVEL").y);
        click_at(&mut app, &ctx, Panel::World, pos);
        assert!(world_form_dialog::is_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        click(&mut app, &ctx, Panel::World, "Cancel");
        assert_eq!(app.project.as_ref().unwrap().json, before);
        click_at(&mut app, &ctx, Panel::World, pos);
        click(&mut app, &ctx, Panel::World, "Confirm");
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level_keys().len(), 2);
        let level = project.level(app.selected.unwrap()).unwrap();
        assert_eq!(
            (level["worldX"].as_i64(), level["worldY"].as_i64()),
            (Some(64), Some(64))
        );
    }
    #[test]
    fn source_level_iid_copy_uses_clipboard_and_does_not_edit() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let iid = before["levels"][0]["iid"].as_str().unwrap();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let pos = egui::pos2(441., label(&output, "IID").y);
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let output = frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command,egui::OutputCommand::CopyText(text) if text==iid))
        );
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
    #[test]
    fn source_world_layout_confirmation_uses_friendly_labels_and_conditional_grid() {
        let (mut app, ctx) = editor();
        let project = app.project.as_mut().unwrap();
        project.add_level(None, "Second", 256, 256).unwrap();
        project.add_level(None, "Third", 256, 256).unwrap();
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[1].1);
        assert!(world_form_dialog::is_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().json["worldLayout"], "Free");
        click(&mut app, &ctx, Panel::World, "Cancel");
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[1].1);
        click(&mut app, &ctx, Panel::World, "Confirm");
        assert_eq!(
            app.project.as_ref().unwrap().json["worldLayout"],
            "GridVania"
        );
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&output, "Gridvania grid size");
        assert!(app.last_chance.is_some());
    }
    #[test]
    fn source_level_size_entry_clamps_minimum_and_preserves_recovery() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let pos = egui::pos2(156., label(&output, "Size").y);
        click_at(&mut app, &ctx, Panel::Level, pos);
        replace_text(&mut app, &ctx, Panel::Level, "1");
        assert_eq!(app.project.as_ref().unwrap().json["levels"][0]["pxWid"], 16);
        assert!(app.last_chance.is_some());
        app.restore_last_chance();
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["pxWid"],
            256
        );
    }
    #[test]
    fn source_identifier_draft_is_tracked_and_commits_before_panel_switch() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["levels"][0]["useAutoIdentifier"] = json!(false);
        click(&mut app, &ctx, Panel::Level, "Level_0");
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![key(
                Key::A,
                egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            )],
        );
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![egui::Event::Text("Draft level".into())],
        );
        assert!(ui_commit::has_pending_ui_edit(&ctx));
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "Level_0"
        );
        app.show_panel(Some(Panel::Help));
        frame(&mut app, &ctx, Panel::Level, vec![]);
        frame(&mut app, &ctx, Panel::Level, vec![]);
        app.flush_ui_action(&ctx);
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "Draft_level"
        );
        assert_eq!(app.panel, Some(Panel::Help));
    }
}
