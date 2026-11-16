use super::*;

fn icon(
    ui: &mut egui::Ui,
    source: egui::ImageSource<'static>,
    size: Vec2,
    fill: Color32,
    tip: &str,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::ZERO;
        ui.add_sized(
            size,
            egui::Button::image(egui::Image::new(source).fit_to_exact_size(Vec2::splat(22.0)))
                .fill(fill)
                .corner_radius(0)
                .stroke(Stroke::NONE),
        )
    })
    .inner
    .on_hover_text(tip)
}

impl EditorApp {
    pub(super) fn visible_layer_indices(&self) -> Vec<usize> {
        let Some(project) = &self.project else {
            return vec![];
        };
        let Some(level) = self.selected.and_then(|k| project.level(k)) else {
            return vec![];
        };
        let filter = self.settings.value["layerFilter"].as_str().unwrap_or("");
        level["layerInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(i, l)| {
                let d = l["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid));
                *i == self.layer
                    || d.is_some_and(|d| {
                        d["hideInList"] != true
                            && (filter.is_empty()
                                || d["uiFilterTags"]
                                    .as_array()
                                    .is_some_and(|a| a.iter().any(|t| t == filter)))
                    })
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn sidebar(&mut self, ctx: &egui::Context) {
        if self.zen || self.project.is_none() {
            return;
        }
        egui::SidePanel::left("main-panel")
            .exact_width(300.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::DARK))
            .show(ctx, |ui| {
                if self.is_busy() || self.pending.is_some() {
                    ui.disable();
                }
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    let orange = Color32::from_rgb(232, 84, 0);
                    let blue = Color32::from_rgb(44, 128, 179);
                    for (panel, image, fill, tip) in [
                        (
                            Panel::Project,
                            egui::include_image!("../../assets/icons/projectSettings.png"),
                            Color32::from_rgb(142, 78, 48),
                            "Project settings · P",
                        ),
                        (
                            Panel::World,
                            egui::include_image!("../../assets/icons/world.svg"),
                            blue,
                            "World view · Shift W",
                        ),
                        (
                            Panel::Level,
                            egui::include_image!("../../assets/icons/level.svg"),
                            blue,
                            "Current level · C",
                        ),
                        (
                            Panel::Layers,
                            egui::include_image!("../../assets/icons/layer.png"),
                            orange,
                            "Layers · L",
                        ),
                        (
                            Panel::Entities,
                            egui::include_image!("../../assets/icons/entity.png"),
                            orange,
                            "Entities · E",
                        ),
                        (
                            Panel::Enums,
                            egui::include_image!("../../assets/icons/enum.png"),
                            orange,
                            "Enums · U",
                        ),
                        (
                            Panel::Tilesets,
                            egui::include_image!("../../assets/icons/tile.png"),
                            orange,
                            "Tilesets · T",
                        ),
                        (
                            Panel::Help,
                            egui::include_image!("../../assets/icons/help.svg"),
                            Color32::from_rgb(70, 140, 80),
                            "Help · H",
                        ),
                        (
                            Panel::Settings,
                            egui::include_image!("../../assets/icons/settings.svg"),
                            theme::LIGHT,
                            "Settings · F12",
                        ),
                    ] {
                        let response = icon(ui, image, egui::vec2(30.0, 36.0), fill, tip);
                        if response.clicked() {
                            self.finish_gesture();
                            if panel == Panel::World {
                                self.world = !self.world;
                                self.fit = true;
                            } else {
                                self.show_panel(if self.panel == Some(panel) {
                                    None
                                } else {
                                    Some(panel)
                                });
                            }
                        }
                        if panel == Panel::Project {
                            response.context_menu(|ui| {
                                if ui.button("New project").clicked() {
                                    self.request(ctx, Action::New);
                                    ui.close();
                                }
                                if ui.button("Open…").clicked() {
                                    self.request(ctx, Action::Browse);
                                    ui.close();
                                }
                                if ui.button("Save").clicked() {
                                    self.save(false);
                                    ui.close();
                                }
                                if ui.button("Save as…").clicked() {
                                    self.save(true);
                                    ui.close();
                                }
                                if ui.button("Export…").clicked() {
                                    self.show_panel(Some(Panel::Export));
                                    ui.close();
                                }
                            });
                        } else if panel == Panel::World {
                            response.context_menu(|ui| {
                                if ui.button("World settings").clicked() {
                                    self.show_panel(Some(Panel::World));
                                    ui.close();
                                }
                            });
                        }
                    }
                    if icon(
                        ui,
                        egui::include_image!("../../assets/icons/exit.svg"),
                        egui::vec2(30.0, 36.0),
                        theme::MEDIUM,
                        "Close project · Cmd/Ctrl W",
                    )
                    .clicked()
                    {
                        self.request(ctx, Action::Close);
                    }
                });
                ui.add_space(4.);
                if self.world {
                    self.world_navigation(ui);
                    return;
                }
                let tags = self.project.as_ref().unwrap().json["defs"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|d| {
                        d["uiFilterTags"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                    })
                    .map(str::to_owned)
                    .collect::<std::collections::BTreeSet<_>>();
                let mut tags = tags.into_iter().collect::<Vec<_>>();
                tags.sort_by_key(|tag| tag.to_lowercase());
                let project_iid = self.project.as_ref().unwrap().json["iid"]
                    .as_str()
                    .map(str::to_owned);
                if !tags.is_empty() {
                    let mut selected =
                        self.settings
                            .ui_state_int("LayerUIFilter", project_iid.as_deref(), -1);
                    let old = selected;
                    if tags.len() > 3 {
                        egui::ComboBox::from_id_salt("layer-filter")
                            .selected_text(
                                tags.get(selected as usize)
                                    .map(String::as_str)
                                    .unwrap_or("- Show all layers -"),
                            )
                            .width(285.)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected, -1, "- Show all layers -");
                                for (index, tag) in tags.iter().enumerate() {
                                    ui.selectable_value(&mut selected, index as i64, tag);
                                }
                            });
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut selected, -1, "All");
                            for (index, tag) in tags.iter().enumerate() {
                                ui.selectable_value(&mut selected, index as i64, tag);
                            }
                        });
                    }
                    if selected != old {
                        if selected < 0 {
                            self.settings
                                .delete_ui_state("LayerUIFilter", project_iid.as_deref());
                        } else {
                            self.settings.set_ui_state_int(
                                "LayerUIFilter",
                                project_iid.as_deref(),
                                selected,
                            );
                        }
                    }
                    let filter = tags.get(selected as usize).cloned().unwrap_or_default();
                    self.settings.value["layerFilter"] = filter.clone().into();
                    if selected != old && !filter.is_empty() {
                        let first = self.project.as_ref().and_then(|p| {
                            p.level(self.selected?)?["layerInstances"]
                                .as_array()?
                                .iter()
                                .enumerate()
                                .find_map(|(index, layer)| {
                                    let def = p.layer_def(layer["layerDefUid"].as_i64()?)?;
                                    (def["hideInList"] != true
                                        && def["uiFilterTags"].as_array().is_some_and(|tags| {
                                            tags.iter().any(|tag| tag == &filter)
                                        }))
                                    .then_some(index)
                                })
                        });
                        if let Some(index) = first {
                            self.switch_layer(index);
                        }
                    }
                } else {
                    self.settings.value["layerFilter"] = "".into();
                }
                let layers = self
                    .visible_layer_indices()
                    .into_iter()
                    .filter_map(|i| {
                        let p = self.project.as_ref()?;
                        let l = p.layer(self.selected?, i)?;
                        let uid = l["layerDefUid"].as_i64()?;
                        let d = p.layer_def(uid)?;
                        Some((
                            i,
                            uid,
                            text(l, "__identifier", "Layer").to_owned(),
                            text(l, "__type", "").to_owned(),
                            d.clone(),
                        ))
                    })
                    .collect::<Vec<_>>();
                for (shortcut, (index, uid, name, kind, def)) in layers.iter().enumerate() {
                    let active = self.layer == *index;
                    let color = def["uiColor"]
                        .as_str()
                        .map(theme::color)
                        .unwrap_or(theme::LIGHT);
                    let fill = if active {
                        color
                    } else {
                        Color32::from_rgb(
                            ((u16::from(color.r()) + u16::from(theme::DARK.r())) / 2) as u8,
                            ((u16::from(color.g()) + u16::from(theme::DARK.g())) / 2) as u8,
                            ((u16::from(color.b()) + u16::from(theme::DARK.b())) / 2) as u8,
                        )
                    };
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 31.5),
                        Sense::click(),
                    );
                    ui.painter().rect_filled(rect, 0.0, fill);
                    if active {
                        ui.painter().rect_stroke(
                            rect.shrink(1.0),
                            0.0,
                            Stroke::new(1.0_f32, Color32::WHITE),
                            StrokeKind::Inside,
                        );
                    }
                    let foreground = if active {
                        Color32::WHITE
                    } else {
                        color.lerp_to_gamma(Color32::WHITE, 0.3)
                    };
                    ui.painter().text(
                        rect.left_center() + egui::vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("F{}", shortcut + 1),
                        FontId::proportional(11.0),
                        foreground,
                    );
                    let image = match kind.as_str() {
                        "Entities" => egui::include_image!("../../assets/icons/entity.png"),
                        "IntGrid" => egui::include_image!("../../assets/icons/intGrid.png"),
                        "AutoLayer" => egui::include_image!("../../assets/icons/autoLayer.svg"),
                        _ => egui::include_image!("../../assets/icons/tile.png"),
                    };
                    egui::Image::new(image).tint(foreground).paint_at(
                        ui,
                        Rect::from_min_size(rect.min + egui::vec2(29.0, 6.0), Vec2::splat(20.0)),
                    );
                    ui.painter().text(
                        rect.left_center() + egui::vec2(57.0, 0.0),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(14.0),
                        foreground,
                    );
                    let eye = Rect::from_center_size(
                        rect.right_center() - egui::vec2(15.0, 0.0),
                        Vec2::splat(23.0),
                    );
                    let eye_response =
                        ui.interact(eye, ui.id().with(("visible", uid)), Sense::click());
                    egui::Image::new(
                        if !self
                            .selected
                            .is_some_and(|key| self.layer_visible(key, *index))
                        {
                            egui::include_image!("../../assets/icons/hidden.png")
                        } else {
                            egui::include_image!("../../assets/icons/visible.png")
                        },
                    )
                    .paint_at(ui, eye);
                    if eye_response.clicked()
                        && let Some(key) = self.selected
                    {
                        self.toggle_layer_visibility(key, *index, ui.input(|i| i.modifiers.shift));
                    }
                    let auto = kind == "AutoLayer"
                        || def["autoRuleGroups"]
                            .as_array()
                            .is_some_and(|a| !a.is_empty());
                    let rule_rect = Rect::from_center_size(
                        rect.right_center() - egui::vec2(51.0, 0.0),
                        egui::vec2(43.0, 26.0),
                    );
                    if auto {
                        let r = ui.interact(
                            rule_rect,
                            ui.id().with(("layer-rules", uid)),
                            Sense::click(),
                        );
                        ui.painter().rect_filled(
                            rule_rect,
                            3.,
                            if r.hovered() {
                                theme::LIGHT
                            } else {
                                Color32::from_rgb(84, 93, 115)
                            },
                        );
                        ui.painter().text(
                            rule_rect.center(),
                            Align2::CENTER_CENTER,
                            "RULES",
                            FontId::proportional(11.),
                            Color32::WHITE,
                        );
                        if r.clicked() {
                            self.switch_layer(*index);
                            self.definition_uid = Some(*uid);
                            self.show_panel(Some(Panel::Rules));
                        }
                    }
                    if response.clicked()
                        && !eye.contains(ui.input(|i| i.pointer.interact_pos().unwrap_or_default()))
                        && !(auto
                            && rule_rect.contains(
                                ui.input(|i| i.pointer.interact_pos().unwrap_or_default()),
                            ))
                    {
                        self.switch_layer(*index);
                    }
                    response
                        .on_hover_text(text(def, "doc", ""))
                        .context_menu(|ui| {
                            if ui.button("Edit layer settings").clicked() {
                                self.definition_uid = Some(*uid);
                                self.show_panel(Some(Panel::Layers));
                                ui.close();
                            }
                            if auto && ui.button("Edit rules").clicked() {
                                self.definition_uid = Some(*uid);
                                self.show_panel(Some(Panel::Rules));
                                ui.close();
                            }
                        });
                    ui.add_space(1.);
                }
                let kind = self
                    .selected
                    .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
                    .map(|layer| text(layer, "__type", "").to_owned())
                    .unwrap_or_default();
                let hints: &[(&[&str], &str)] = match kind.as_str() {
                    "Entities" => &[
                        (&["ALT"], "Pick"),
                        (
                            &[
                                if cfg!(target_os = "macos") {
                                    "⌘"
                                } else {
                                    "CTRL"
                                },
                                "ALT",
                            ],
                            "Copy",
                        ),
                    ],
                    "IntGrid" | "Tiles" => &[(&["SHIFT"], "Rectangle"), (&["ALT"], "Pick")],
                    _ => &[],
                };
                if !hints.is_empty() {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(39, 64, 48))
                        .stroke(Stroke::new(1_f32, Color32::from_rgb(94, 171, 107)))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4., 0.);
                                for (keys, label) in hints {
                                    let width = keys
                                        .iter()
                                        .map(|key| if key.len() > 3 { 38. } else { 27. })
                                        .sum::<f32>()
                                        + 16.
                                        + label.len() as f32 * 6.5
                                        + 6.;
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(width, 23.5),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        2.,
                                        Color32::from_rgb(60, 97, 72),
                                    );
                                    let mut x = rect.left();
                                    for key in *keys {
                                        let w = if key.len() > 3 { 38. } else { 27. };
                                        let cap = Rect::from_min_size(
                                            egui::pos2(x, rect.top()),
                                            egui::vec2(w - 1., 20.),
                                        );
                                        ui.painter().rect_filled(
                                            cap,
                                            2.,
                                            Color32::from_rgb(146, 154, 126),
                                        );
                                        ui.painter().text(
                                            cap.center(),
                                            Align2::CENTER_CENTER,
                                            key,
                                            FontId::new(
                                                13.,
                                                egui::FontFamily::Name("panel-title".into()),
                                            ),
                                            Color32::WHITE,
                                        );
                                        x += w;
                                    }
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/mouseLeft.png"
                                    ))
                                    .paint_at(
                                        ui,
                                        Rect::from_min_size(
                                            egui::pos2(x, rect.top() + 1.),
                                            Vec2::splat(16.),
                                        ),
                                    );
                                    ui.painter().text(
                                        egui::pos2(x + 17., rect.center().y),
                                        Align2::LEFT_CENTER,
                                        label,
                                        FontId::new(
                                            14.,
                                            egui::FontFamily::Name("panel-title".into()),
                                        ),
                                        Color32::from_rgb(121, 221, 138),
                                    );
                                }
                            });
                        });
                    ui.add_space(16.);
                }
                egui::ScrollArea::vertical()
                    .id_salt("palette-scroll")
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(0)
                            .show(ui, |ui| self.library_palette(ctx, ui));
                    });
            });
    }

    fn world_navigation(&mut self, ui: &mut egui::Ui) {
        let worlds = self.project.as_ref().unwrap().json["worlds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .enumerate()
                    .map(|(i, w)| (Some(i), text(w, "identifier", "World").to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (world, name) in worlds {
            if ui
                .selectable_label(self.active_world == world, name)
                .clicked()
            {
                self.navigate_world(world);
            }
        }
        ui.horizontal(|ui| {
            if ui.button("World settings").clicked() {
                self.show_panel(Some(Panel::World));
            }
            if ui.button("Fit · Space").clicked() {
                self.fit = true;
            }
        });
        let levels = self
            .project
            .as_ref()
            .unwrap()
            .level_keys()
            .into_iter()
            .filter(|k| k.world == self.active_world)
            .filter_map(|key| {
                self.project
                    .as_ref()?
                    .level(key)
                    .map(|l| (key, text(l, "identifier", "Level").to_owned()))
            })
            .collect::<Vec<_>>();
        for (key, name) in levels {
            let response = ui.selectable_label(self.selected == Some(key), name);
            if response.clicked() {
                self.navigate_level(key, false);
            }
            if response.double_clicked() {
                self.world = false;
                self.fit = true;
            }
        }
        ui.separator();
        ui.label(format!("World layer {}", self.world_depth));
        ui.horizontal(|ui| {
            if ui.button("Below").clicked() {
                self.world_depth -= 1;
            }
            if ui.button("Above").clicked() {
                self.world_depth += 1;
            }
        });
    }

    pub(super) fn canvas_toolbar(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.project.is_none() {
            return;
        }
        egui::Area::new(egui::Id::new("editing-options"))
            .fixed_pos(rect.left_top() + egui::vec2(0.0, 46.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                ui.set_width(34.0);
                let cache=self.assets.stats();
                ui.add(egui::Label::new(RichText::new("Visuals").font(FontId::new(12.,egui::FontFamily::Name("panel-title".into()))).color(Color32::from_rgb(94,146,240))).extend()).on_hover_text(format!("{:.0}% · {} visible cells · {:.2} ms CPU\n{} textures · {:.1} MiB · {} pending",self.viewport.zoom*100.0,self.visible_tiles,self.frame_time,cache.textures,cache.bytes as f64/1_048_576.0,cache.pending));
                for (source, enabled, tip, action) in [
                    (
                        egui::include_image!("../../assets/icons/zen.svg"),
                        self.zen,
                        "Zen mode · Tab",
                        0,
                    ),
                    (
                        egui::include_image!("../../assets/icons/gridOn.svg"),
                        self.grid,
                        "Grid · G",
                        1,
                    ),
                    (
                        egui::include_image!("../../assets/icons/showDetailsOn.svg"),
                        self.details,
                        "Details · Shift H",
                        2,
                    ),
                    (
                        egui::include_image!("../../assets/icons/singleLayer.svg"),
                        self.single_layer,
                        "Single layer · Shift A",
                        3,
                    ),
                    (
                        egui::include_image!("../../assets/icons/autoLayer.svg"),
                        self.render_auto,
                        "Auto layers · Shift R",
                        4,
                    ),
                    (
                        egui::include_image!("../../assets/icons/enum.png"),
                        self.tile_tags,
                        "Tile enum overlays",
                        5,
                    ),
                ] {
                    if icon(
                        ui,
                        source,
                        Vec2::splat(34.0),
                        if enabled {
                            Color32::from_rgb(94, 146, 240)
                        } else {
                            Color32::from_rgb(34, 50, 87)
                        },
                        tip,
                    )
                    .clicked()
                    {
                        match action {
                            0 => self.zen = !self.zen,
                            1 => self.grid = !self.grid,
                            2 => self.set_details(!self.details),
                            3 => self.set_single_layer_mode(!self.single_layer),
                            4 => self.render_auto = !self.render_auto,
                            _ => self.tile_tags = !self.tile_tags,
                        }
                    }
                }
                ui.add_space(6.0);
                ui.add(egui::Label::new(RichText::new("Editing").font(FontId::new(12.,egui::FontFamily::Name("panel-title".into()))).color(Color32::from_rgb(207,221,64))).extend());
                if icon(
                    ui,
                    egui::include_image!("../../assets/icons/emptySpaceOn.png"),
                    Vec2::splat(34.0),
                    if self.settings.bool("emptySpaceSelection") {
                        Color32::from_rgb(110, 144, 57)
                    } else {
                        theme::MEDIUM
                    },
                    "Select empty spaces · Shift E",
                )
                .clicked()
                {
                    self.set_empty_selection(!self.settings.bool("emptySpaceSelection"));
                }
                if icon(
                    ui,
                    egui::include_image!("../../assets/icons/stack.svg"),
                    Vec2::splat(34.0),
                    if self.tile_stacking {
                        Color32::from_rgb(110, 144, 57)
                    } else {
                        theme::MEDIUM
                    },
                    "Tile stacking · Shift T",
                )
                .clicked()
                {
                    self.tile_stacking = !self.tile_stacking;
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn five_layer_sidebar_matches_source_rows_guide_and_atlas_top() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        ctx.set_pixels_per_point(2.);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/original/Typical_TopDown_example.ldtk"),
            )
            .unwrap(),
        );
        let project = app.project.as_ref().unwrap();
        let key = app.selected.unwrap();
        app.layer = project.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["__type"] == "Tiles")
            .unwrap();
        let iid = project.json["iid"].as_str().unwrap().to_owned();
        let uid = project.layer(key, app.layer).unwrap()["__tilesetDefUid"]
            .as_i64()
            .unwrap();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| app.sidebar(ctx),
            ));
        }
        let area = egui::AreaState::load(
            &ctx,
            egui::Id::new(("tile-palette-popout", &iid, uid)).with("content"),
        )
        .unwrap();
        assert_eq!(ctx.available_rect().left(), 300.);
        assert_eq!(
            area.rect().top(),
            281.,
            "source five-row sidebar starts its atlas at y281"
        );
        let shapes = output.unwrap().shapes;
        let guide = shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == Color32::from_rgb(39, 64, 48) => Some(r.rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            guide,
            Rect::from_min_size(egui::pos2(0., 202.5), egui::vec2(300., 33.5))
        );
        let rows = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.width() == 300. && r.rect.height() == 31.5 => {
                    Some(r.rect)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 5);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.top(), 40. + 32.5 * i as f32);
        }
    }
    #[test]
    fn source_sidebar_keeps_three_hundred_points_and_rules_label_visible() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/original/Entities.ldtk"),
            )
            .unwrap(),
        );
        let mut left = 0.;
        let mut out = None;
        for _ in 0..2 {
            out = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.sidebar(ctx);
                    left = ctx.available_rect().left();
                },
            ));
        }
        assert_eq!(left, 300., "palette controls must not widen sidebar");
        assert!(out.unwrap().shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.job.text=="RULES")),"RULES label must be painted without ellipsis or wrapping");
    }
}
