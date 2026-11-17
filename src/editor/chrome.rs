use super::*;

fn shortcut_label(key: &str) -> &str {
    match key {
        "SHIFT" => "⇧",
        "CTRL" if cfg!(target_os = "macos") => "⌘",
        _ => key,
    }
}

fn shortcut_width(ui: &egui::Ui, key: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            shortcut_label(key).into(),
            FontId::new(13.2, egui::FontFamily::Name("heading".into())),
            Color32::WHITE,
        )
        .size()
        .x
        .max(16.)
        + 12.
}

fn paint_shortcut(ui: &egui::Ui, rect: Rect, key: &str, shadow: bool) {
    let cap = rect.shrink(1.);
    if let Ok(egui::load::TexturePoll::Ready { texture }) =
        egui::Image::new(egui::include_image!("../../assets/img/keycap.svg"))
            .texture_options(egui::TextureOptions::LINEAR)
            .fit_to_exact_size(Vec2::splat(32.))
            .load_for_size(ui.ctx(), Vec2::splat(32.))
    {
        // Preserve the CSS radius, bottom border and outside shadow at any key width.
        let bounds = Rect::from_min_max(cap.min - egui::vec2(2., 2.), cap.max + egui::vec2(2., 6.));
        let xs = [
            bounds.left(),
            bounds.left() + 4.,
            bounds.right() - 4.,
            bounds.right(),
        ];
        let ys = [
            bounds.top(),
            bounds.top() + 4.,
            bounds.bottom() - 12.,
            bounds.bottom(),
        ];
        let us = [0., 4. / 32., 28. / 32., 1.];
        let vs = [0., 4. / 32., 20. / 32., 1.];
        let mut mesh = egui::Mesh::with_texture(texture.id);
        for y in 0..3 {
            for x in 0..3 {
                mesh.add_rect_with_uv(
                    Rect::from_min_max(egui::pos2(xs[x], ys[y]), egui::pos2(xs[x + 1], ys[y + 1])),
                    Rect::from_min_max(egui::pos2(us[x], vs[y]), egui::pos2(us[x + 1], vs[y + 1])),
                    Color32::WHITE,
                );
            }
        }
        if shadow {
            ui.painter().add(mesh);
        } else {
            ui.painter().with_clip_rect(cap).add(mesh);
        }
    }
    let font = FontId::new(13.2, egui::FontFamily::Name("heading".into()));
    let pos = cap.center() - egui::vec2(0., 2.);
    ui.painter().text(
        pos + egui::vec2(0., 1.),
        Align2::CENTER_CENTER,
        shortcut_label(key),
        font.clone(),
        Color32::from_black_alpha(128),
    );
    ui.painter().text(
        pos,
        Align2::CENTER_CENTER,
        shortcut_label(key),
        font,
        Color32::WHITE,
    );
}

fn source_tooltip(response: &egui::Response, text: &str, keys: &[&str], right: bool) {
    if !response.hovered() || !response.enabled() {
        return;
    }
    let mut tooltip =
        egui::Tooltip::for_widget(response)
            .width(0.)
            .gap(if right { 10. } else { 8. });
    tooltip.popup = tooltip
        .popup
        .align(if right {
            egui::emath::RectAlign::RIGHT_START
        } else if response.rect.top() >= 40. {
            egui::emath::RectAlign::TOP_START
        } else {
            egui::emath::RectAlign::BOTTOM_START
        })
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(60, 97, 72))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(18, 12))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 8],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(77),
                }),
        );
    tooltip.show(|ui| {
        let (heading, body) = text
            .strip_prefix("**")
            .and_then(|text| text.split_once("**"))
            .map_or((None, text), |(heading, body)| (Some(heading), body.trim()));
        let body = ui.painter().layout(
            body.replace("\\n", "\n\n"),
            FontId::proportional(14.666667),
            Color32::from_rgb(121, 221, 138),
            250.,
        );
        let heading = heading.map(|heading| {
            ui.painter().layout_no_wrap(
                heading.into(),
                FontId::new(17.6, egui::FontFamily::Name("heading".into())),
                Color32::WHITE,
            )
        });
        let heading_height = heading.as_ref().map_or(0., |heading| heading.size().y + 4.);
        let text_size = egui::vec2(
            heading
                .as_ref()
                .map_or(body.size().x, |heading| heading.size().x.max(body.size().x)),
            body.size().y + heading_height,
        );
        let key_widths = keys
            .iter()
            .map(|key| shortcut_width(ui, key))
            .collect::<Vec<_>>();
        let keys_width = key_widths.iter().sum::<f32>();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(
                text_size.x
                    + if keys.is_empty() {
                        0.
                    } else {
                        20. + keys_width
                    },
                text_size.y.max(23.5),
            ),
            Sense::hover(),
        );
        let text_pos = rect.left_center() - egui::vec2(0., text_size.y * 0.5);
        if let Some(heading) = heading {
            ui.painter().galley_with_override_text_color(
                text_pos + egui::vec2(0., 1.),
                heading.clone(),
                Color32::from_black_alpha(128),
            );
            ui.painter().galley(text_pos, heading, Color32::WHITE);
        }
        let body_pos = text_pos + egui::vec2(0., heading_height);
        ui.painter().galley_with_override_text_color(
            body_pos + egui::vec2(0., 1.),
            body.clone(),
            Color32::from_black_alpha(128),
        );
        ui.painter()
            .galley(body_pos, body, Color32::from_rgb(121, 221, 138));
        let mut x = rect.right() - keys_width;
        for (key, width) in keys.iter().zip(key_widths) {
            paint_shortcut(
                ui,
                Rect::from_min_size(
                    egui::pos2(x, rect.center().y - 11.75),
                    egui::vec2(width, 23.5),
                ),
                key,
                true,
            );
            x += width;
        }
    });
}

fn icon(
    ui: &mut egui::Ui,
    source: egui::ImageSource<'static>,
    size: Vec2,
    fill: Color32,
    tip: &str,
    keys: &[&str],
    tip_right: bool,
) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = Vec2::ZERO;
            ui.add_sized(
                size,
                egui::Button::image(egui::Image::new(source).fit_to_exact_size(Vec2::splat(22.0)))
                    .fill(fill)
                    .corner_radius(0)
                    .stroke(Stroke::NONE),
            )
        })
        .inner;
    source_tooltip(&response, tip, keys, tip_right);
    response
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
                    for (panel, image, fill, tip, keys) in [
                        (
                            Panel::Project,
                            egui::include_image!("../../assets/icons/projectSettings.png"),
                            Color32::from_rgb(142, 78, 48),
                            "Project settings",
                            &["P"][..],
                        ),
                        (
                            Panel::World,
                            egui::include_image!("../../assets/icons/world.svg"),
                            blue,
                            "World view",
                            &[if self.settings.value["navigationKeys"] == "Zqsd" {
                                "²"
                            } else {
                                "~"
                            }][..],
                        ),
                        (
                            Panel::Level,
                            egui::include_image!("../../assets/icons/level.svg"),
                            blue,
                            "Current level settings",
                            &["C"][..],
                        ),
                        (
                            Panel::Layers,
                            egui::include_image!("../../assets/icons/layer.png"),
                            orange,
                            "Layers",
                            &["L"][..],
                        ),
                        (
                            Panel::Entities,
                            egui::include_image!("../../assets/icons/entity.png"),
                            orange,
                            "Entities",
                            &["E"][..],
                        ),
                        (
                            Panel::Enums,
                            egui::include_image!("../../assets/icons/enum.png"),
                            orange,
                            "Enums",
                            &["U"][..],
                        ),
                        (
                            Panel::Tilesets,
                            egui::include_image!("../../assets/icons/tile.png"),
                            orange,
                            "Tilesets",
                            &["T"][..],
                        ),
                        (
                            Panel::Help,
                            egui::include_image!("../../assets/icons/help.svg"),
                            Color32::from_rgb(70, 140, 80),
                            "Help and shortcuts",
                            &["H"][..],
                        ),
                        (
                            Panel::Settings,
                            egui::include_image!("../../assets/icons/settings.svg"),
                            theme::LIGHT,
                            "App settings",
                            &["F12"][..],
                        ),
                    ] {
                        let fill = if self.panel == Some(panel) {
                            theme::LIGHT
                        } else {
                            fill
                        };
                        let response =
                            icon(ui, image, egui::vec2(30.0, 36.0), fill, tip, keys, false);
                        if response.clicked() {
                            self.toggle_toolbar_panel(panel);
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
                        "Close project",
                        &["CTRL", "W"],
                        false,
                    )
                    .clicked()
                    {
                        self.request(ctx, Action::Close);
                    }
                });
                // The close button can remove the project during this frame.
                if self.project.is_none() {
                    return;
                }
                ui.add_space(4.);
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
                    "Entities" => &[(&["ALT"], "Pick"), (&["CTRL", "ALT"], "Copy")],
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
                                    let width =
                                        keys.iter().map(|key| shortcut_width(ui, key)).sum::<f32>()
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
                                        let w = shortcut_width(ui, key);
                                        let cap = Rect::from_min_size(
                                            egui::pos2(x, rect.top()),
                                            egui::vec2(w, 23.5),
                                        );
                                        paint_shortcut(ui, cap, key, false);
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
                for (source, enabled, tip, keys, action) in [
                    (
                        egui::include_image!("../../assets/icons/zen.svg"),
                        self.zen,
                        "**Zen mode** This mode automatically hides the left panel to give more editing space.",
                        &["TAB"][..],
                        0,
                    ),
                    (
                        egui::include_image!("../../assets/icons/gridOn.svg"),
                        self.grid,
                        "**Grid** Show the layer grid. In supported layer types, hiding the grid will also allow free positioning of elements.",
                        &["G"][..],
                        1,
                    ),
                    (
                        egui::include_image!("../../assets/icons/showDetailsOn.svg"),
                        self.details,
                        "**Show/hide details** When enabled, shows all elements in layers (tiles, entities, intGrid etc.), or only tiles otherwise.",
                        if cfg!(target_os = "macos") { &["SHIFT", "H"][..] } else { &["CTRL", "H"][..] },
                        2,
                    ),
                    (
                        egui::include_image!("../../assets/icons/singleLayer.svg"),
                        self.single_layer,
                        "**Single layer mode** Enable this to fade all other layers, and focus on active layer only. NOTE: this will also affect the Selection tool (ALT+click).",
                        &["SHIFT", "A"][..],
                        3,
                    ),
                    (
                        egui::include_image!("../../assets/icons/autoLayer.svg"),
                        self.render_auto,
                        "**Render auto-layers** Enable or disable the rendering of tiles in all Auto-Layers. If disabled, only the underlying IntGrid values will be displayed.",
                        &["SHIFT", "R"][..],
                        4,
                    ),
                    (
                        egui::include_image!("../../assets/icons/enum.png"),
                        self.tile_tags,
                        "**Show/hide enum tags** When enabled, shows a color overlay on tiles that have an Enum tag attached to them (see the Tilesets panel to tag tiles using Enums).\\nOnly applies to Tiles layers and Auto-layers.",
                        &[][..],
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
                        keys,
                        true,
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
                    "**Select empty spaces** If this option is enabled when you select a level area with ALT+SHIFT+click, the selection will also include EMPTY grid cells.",
                    &["SHIFT", "E"],
                    true,
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
                    "**Tile stacking** Only applies to Tiles layers. If this option is enabled, you will be able to stack multiple tiles in a single grid cell.",
                    &["SHIFT", "T"],
                    true,
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

    fn app_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            focused: true,
            modifiers: events
                .iter()
                .find_map(|event| match event {
                    egui::Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default(),
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        let output = ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest())
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
        output
    }

    fn app_click(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        for pressed in [true, false] {
            app_frame(
                app,
                ctx,
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

    fn source_key_texture(ctx: &egui::Context) -> egui::TextureId {
        let image = egui::Image::new(egui::include_image!("../../assets/img/keycap.svg"))
            .texture_options(egui::TextureOptions::LINEAR)
            .fit_to_exact_size(Vec2::splat(32.));
        let until = Instant::now() + std::time::Duration::from_secs(1);
        loop {
            if let Some(id) = image
                .load_for_size(ctx, Vec2::splat(32.))
                .unwrap()
                .texture_id()
            {
                return id;
            }
            assert!(Instant::now() < until, "source keycap asset did not load");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn rendered_shortcut_background_matches_source_css_translucency() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let until = Instant::now() + std::time::Duration::from_secs(1);
        loop {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(28., 23.5), Sense::hover());
                    paint_shortcut(ui, rect, "W", true);
                });
            });
            if let Some(texture) = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.vertices.len() == 36 => Some(mesh.texture_id),
                _ => None,
            }) {
                let delta = output
                    .textures_delta
                    .set
                    .iter()
                    .find_map(|(id, delta)| (*id == texture).then_some(delta))
                    .expect("the painted keycap must upload its real background texture");
                assert_eq!(
                    delta.options.magnification,
                    egui::TextureFilter::Linear,
                    "rounded key edges must stay smooth when the mesh stretches"
                );
                let egui::ImageData::Color(image) = &delta.image;
                let center = image[(image.width() / 2, image.height() / 3)].to_srgba_unmultiplied();
                for (actual, expected) in center.into_iter().zip([225u8, 214, 187, 140]) {
                    assert!(
                        actual.abs_diff(expected) <= 2,
                        "the source key background must be translucent beige: {center:?}"
                    );
                }
                let border =
                    image[(image.width() / 2, image.height() * 3 / 4)].to_srgba_unmultiplied();
                assert!(
                    border[3].abs_diff(203) <= 2,
                    "the source 4px bottom border overlays the translucent background: {border:?}"
                );
                break;
            }
            assert!(Instant::now() < until, "keycap did not render");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn toolbar_hover_shows_source_tooltip_with_shortcut_keycaps() {
        for (pos, label, keys) in [
            (egui::pos2(45., 18.), "World view", vec!["~"]),
            (
                egui::pos2(285., 18.),
                "Close project",
                vec![
                    if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "CTRL"
                    },
                    "W",
                ],
            ),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| {
                style.interaction.tooltip_delay = 0.;
                style.animation_time = 0.;
            });
            let texture = source_key_texture(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app_frame(&mut app, &ctx, vec![]);
            let mut output = None;
            for _ in 0..4 {
                output = Some(app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(pos)],
                ));
            }
            let output = output.unwrap();
            let tooltip = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("hover must show the source label {label:?}"));
            assert!(
                output.shapes.iter().any(|shape| {
                    let is_source_popup = |shape: &egui::Shape| {
                        matches!(shape, egui::Shape::Rect(rect)
                        if rect.fill == Color32::from_rgb(60, 97, 72)
                            && rect.rect.contains_rect(tooltip))
                    };
                    is_source_popup(&shape.shape)
                        || matches!(&shape.shape, egui::Shape::Vec(shapes)
                        if shapes.iter().any(is_source_popup))
                }),
                "the tooltip must use the source green popup"
            );
            for key in &keys {
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == *key)
                }), "missing shortcut symbol {key:?}");
            }
            assert_eq!(
                output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(&shape.shape, egui::Shape::Mesh(mesh)
                        if mesh.texture_id == texture && mesh.vertices.len() == 36)
                    })
                    .count(),
                keys.len(),
                "each shortcut key must paint the CSS keycap background as nine slices"
            );
        }
    }

    #[test]
    fn sidebar_guide_shows_source_shift_symbol_on_the_same_keycap_asset() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let texture = source_key_texture(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/original/Typical_TopDown_example.ldtk"),
            )
            .unwrap(),
        );
        app.layer = app
            .project
            .as_ref()
            .unwrap()
            .level(app.selected.unwrap())
            .unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|layer| layer["__type"] == "Tiles")
            .unwrap();
        let mut output = None;
        for _ in 0..3 {
            output = Some(app_frame(&mut app, &ctx, vec![]));
        }
        let output = output.unwrap();
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "⇧")
            }),
            "the rectangle guide must show the source Shift symbol"
        );
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == texture && mesh.vertices.len() == 36)
                })
                .count(),
            2,
            "the rectangle and picker guides must use source keycaps"
        );
    }

    #[test]
    fn close_project_button_returns_home_without_panicking_in_level_and_world_view() {
        for world in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.world = world;
            app_frame(&mut app, &ctx, vec![]);
            app_click(&mut app, &ctx, egui::pos2(285., 18.));
            app_frame(&mut app, &ctx, vec![]);
            assert!(app.project.is_none(), "close button must return to Home");
            assert!(app.selected.is_none() && app.pending.is_none());
        }
    }

    #[test]
    fn close_project_button_preserves_unsaved_save_discard_and_cancel() {
        for choice in ["YES", "NO", "CANCEL"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("project.ldtk");
            let mut project = Project::new();
            project.save(&path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let key = project.level_keys()[0];
            project.level_mut(key).unwrap()["identifier"] = "Unsaved".into();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            app.set_project(project);
            app.dirty = true;
            app_frame(&mut app, &ctx, vec![]);
            app_click(&mut app, &ctx, egui::pos2(285., 18.));
            assert!(app.project.is_some() && app.dirty);
            assert!(matches!(app.pending, Some(Action::Close)));
            let output = app_frame(&mut app, &ctx, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == choice => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing unsaved choice {choice}"));
            app_click(&mut app, &ctx, pos);
            let until = Instant::now() + std::time::Duration::from_secs(3);
            while (app.is_busy() || app.pending.is_some()) && Instant::now() < until {
                app_frame(&mut app, &ctx, vec![]);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            app_frame(&mut app, &ctx, vec![]);
            assert!(app.pending.is_none() && !app.is_busy(), "{choice}");
            if choice == "CANCEL" {
                assert!(app.project.is_some() && app.dirty);
                assert_eq!(
                    app.project.as_ref().unwrap().level(key).unwrap()["identifier"],
                    "Unsaved"
                );
            } else {
                assert!(app.project.is_none(), "{choice} must close the project");
            }
            if choice == "YES" {
                assert_eq!(
                    Project::load(&path).unwrap().level(key).unwrap()["identifier"],
                    "Unsaved"
                );
            } else {
                assert_eq!(std::fs::read(path).unwrap(), before);
            }
        }
    }

    #[test]
    fn native_window_close_keeps_the_project_and_guards_unsaved_changes() {
        for dirty in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.dirty = dirty;
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                focused: true,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let output = ctx.run(input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest())
            });
            assert!(app.crash.is_none() && app.project.is_some());
            assert_eq!(matches!(app.pending, Some(Action::Quit)), dirty);
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::CancelClose)),
                dirty
            );
        }
    }

    #[test]
    fn command_w_closes_only_the_project_when_modifiers_are_released_in_the_same_frame() {
        for (has_project, dirty) in [(true, false), (true, true), (false, false)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            if has_project {
                app.set_project(Project::new());
            }
            app.dirty = dirty;
            app_frame(&mut app, &ctx, vec![]);
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                focused: true,
                modifiers: egui::Modifiers::NONE,
                events: [true, false]
                    .into_iter()
                    .map(|pressed| egui::Event::Key {
                        key: Key::W,
                        physical_key: Some(Key::W),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            command: true,
                            mac_cmd: cfg!(target_os = "macos"),
                            ctrl: !cfg!(target_os = "macos"),
                            ..Default::default()
                        },
                    })
                    .collect(),
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let output = ctx.run(input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest())
            });
            assert!(!app.allow_close);
            assert!(
                !output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::Close))
            );
            if dirty {
                assert!(matches!(app.pending, Some(Action::Close)));
                assert!(app.project.is_some());
            } else {
                assert!(app.project.is_none(), "Cmd/Ctrl+W must return to Home");
            }
            assert!(app.crash.is_none());
        }
    }

    #[test]
    fn world_toolbar_opens_a_floating_form_over_the_existing_layer_sidebar() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project
            .add_layer("Original_palette", "IntGrid", 16)
            .unwrap();
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.panel, Some(Panel::World));
        assert!(app.world);
        let panel = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
            .expect("World must open the same floating panel layer as other toolbar forms")
            .rect();
        assert_eq!(panel.left(), 0.);
        assert_eq!(panel.top(), 34.);
        assert_eq!(panel.width(), 400.);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Original_palette"))
        }), "the existing layer sidebar must remain behind the World panel");
        assert_eq!(app.project.as_ref().unwrap().json, original);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.panel.is_none() && !app.world);
    }

    #[test]
    fn toolbar_buttons_toggle_and_switch_floating_panels_without_editing_the_project() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        let original = app.project.as_ref().unwrap().json.clone();
        app_frame(&mut app, &ctx, vec![]);
        for (index, panel) in [
            Panel::Project,
            Panel::World,
            Panel::Level,
            Panel::Layers,
            Panel::Entities,
            Panel::Enums,
            Panel::Tilesets,
            Panel::Help,
            Panel::Settings,
        ]
        .into_iter()
        .enumerate()
        {
            let pos = egui::pos2(index as f32 * 30. + 15., 18.);
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.panel, Some(panel), "toolbar {panel:?}");
            assert_eq!(app.world, panel == Panel::World);
            if panel != Panel::Settings {
                let rect = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
                    .unwrap()
                    .rect();
                assert_eq!(rect.min, egui::pos2(0., 34.), "{panel:?}");
                assert_eq!(rect.width(), panels::source_panel_width(panel), "{panel:?}");
            }
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
            assert!(
                app.panel.is_none() && !app.world,
                "same toolbar button closes {panel:?}"
            );
            // Leave this panel open to exercise switching on the next iteration.
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn floating_panel_mask_blocks_paint_and_world_panel_leaves_the_canvas_live() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project.add_layer("Paint", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        let original = app.project.as_ref().unwrap().json.clone();
        app_click(&mut app, &ctx, egui::pos2(75., 18.));
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(1300., 500.));
        assert!(
            app.panel.is_none(),
            "clicking the modal mask closes the level panel"
        );
        assert_eq!(
            app.project.as_ref().unwrap().json,
            original,
            "masked clicks must not paint"
        );
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(1300., 500.));
        assert_eq!(app.panel, Some(Panel::World));
        assert!(
            app.world,
            "WorldPanel has no modal canvas mask in the source"
        );
        assert_eq!(app.project.as_ref().unwrap().json, original);
        app_click(&mut app, &ctx, egui::pos2(361., 60.));
        assert!(
            app.panel.is_none() && !app.world,
            "closing WorldPanel exits world mode"
        );
    }

    #[test]
    fn world_shortcut_and_escape_follow_the_floating_panel_lifecycle() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        for (key, shift, expected) in [
            (Key::W, true, Some(Panel::World)),
            (Key::P, false, Some(Panel::Project)),
            (Key::P, false, None),
            (Key::Backtick, false, Some(Panel::World)),
            (Key::Escape, false, None),
        ] {
            for pressed in [true, false] {
                app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            shift,
                            ..Default::default()
                        },
                    }],
                );
            }
            assert_eq!(app.panel, expected, "{key:?}");
            assert_eq!(app.world, expected == Some(Panel::World));
        }
    }

    #[test]
    fn cancelling_level_placement_keeps_world_panel_open() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "CREATE A NEW LEVEL" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        app_click(&mut app, &ctx, pos);
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.world_context.picking);
        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(app.panel, Some(Panel::World));
        assert!(
            app.world,
            "Escape cancels placement before closing WorldPanel"
        );
        assert!(!app.world_context.picking);
    }

    #[test]
    fn modal_panel_mask_blocks_the_world_context_menu() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(75., 18.));
        app_frame(&mut app, &ctx, vec![]);
        for pressed in [true, false] {
            app_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(egui::pos2(1400., 880.)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(1400., 880.),
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let output = app_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.panel, Some(Panel::Level));
        assert!(
            !output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "New level")
            }),
            "the dimmed canvas must not open a level-creation context menu"
        );
    }

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
