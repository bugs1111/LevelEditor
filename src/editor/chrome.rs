use super::*;

const SIDEBAR_WIDTH: f32 = 300.;

fn short_world_name(name: &str) -> String {
    const MAX_LEN: usize = 6;
    if name.chars().count() <= MAX_LEN {
        return name.to_owned();
    }
    let mut short = String::with_capacity(MAX_LEN);
    let mut picks = 2;
    let mut uppercase_next = false;
    let mut tail = name;
    let mut count = 0;
    for (offset, ch) in name.char_indices() {
        if ch == '_' {
            picks = 2;
            uppercase_next = true;
            continue;
        }
        if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
            picks = 2;
        }
        if picks == 0 {
            continue;
        }
        short.push(if uppercase_next {
            ch.to_ascii_uppercase()
        } else {
            ch
        });
        picks -= 1;
        uppercase_next = false;
        tail = &name[offset + ch.len_utf8()..];
        count += 1;
        if count == MAX_LEN {
            return short;
        }
    }
    short.extend(tail.chars().take(MAX_LEN - count));
    short
}

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
    paint_keycap(ui, rect, shortcut_label(key), Color32::WHITE, shadow);
}

pub(super) fn paint_keycap(ui: &egui::Ui, rect: Rect, label: &str, tint: Color32, shadow: bool) {
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
                    tint,
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
        label,
        font.clone(),
        Color32::from_black_alpha(128),
    );
    ui.painter()
        .text(pos, Align2::CENTER_CENTER, label, font, Color32::WHITE);
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
        let text = locale::tr(ui, text);
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
            .exact_width(SIDEBAR_WIDTH)
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
                                if ui.button(locale::tr(ui, "New project")).clicked() {
                                    self.request(ctx, Action::New);
                                    ui.close();
                                }
                                if ui.button(locale::tr(ui, "Open…")).clicked() {
                                    self.request(ctx, Action::Browse);
                                    ui.close();
                                }
                                if ui.button(locale::tr(ui, "Save")).clicked() {
                                    self.save(false);
                                    ui.close();
                                }
                                if ui.button(locale::tr(ui, "Save as…")).clicked() {
                                    self.save(true);
                                    ui.close();
                                }
                                if ui.button(locale::tr(ui, "Export…")).clicked() {
                                    self.show_panel(Some(Panel::Export));
                                    ui.close();
                                }
                            });
                        } else if panel == Panel::World {
                            response.context_menu(|ui| {
                                if ui.button(locale::tr(ui, "World settings")).clicked() {
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
                let project_iid = self.project.as_ref().unwrap().json["iid"]
                    .as_str()
                    .map(str::to_owned);
                egui::ScrollArea::vertical()
                    .id_salt(("source-layer-list", project_iid.as_deref()))
                    .max_height(250.)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_min_height(100.);
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
                        if !tags.is_empty() {
                            let mut selected = self.settings.ui_state_int(
                                "LayerUIFilter",
                                project_iid.as_deref(),
                                -1,
                            );
                            let old = selected;
                            if tags.len() > 3 {
                                egui::ComboBox::from_id_salt("layer-filter")
                                    .selected_text(
                                        tags.get(selected as usize)
                                            .map(String::as_str)
                                            .unwrap_or(locale::tr(ui, "- Show all layers -")),
                                    )
                                    .width(285.)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut selected,
                                            -1,
                                            locale::tr(ui, "- Show all layers -"),
                                        );
                                        for (index, tag) in tags.iter().enumerate() {
                                            ui.selectable_value(&mut selected, index as i64, tag);
                                        }
                                    });
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    ui.selectable_value(&mut selected, -1, locale::tr(ui, "All"));
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
                                            let def =
                                                p.layer_def(layer["layerDefUid"].as_i64()?)?;
                                            (def["hideInList"] != true
                                                && def["uiFilterTags"].as_array().is_some_and(
                                                    |tags| tags.iter().any(|tag| tag == &filter),
                                                ))
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
                                    text(l, "__identifier", locale::tr(ui, "Layer")).to_owned(),
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
                                "AutoLayer" => {
                                    egui::include_image!("../../assets/icons/autoLayer.svg")
                                }
                                _ => egui::include_image!("../../assets/icons/tile.png"),
                            };
                            egui::Image::new(image).tint(foreground).paint_at(
                                ui,
                                Rect::from_min_size(
                                    rect.min + egui::vec2(29.0, 6.0),
                                    Vec2::splat(20.0),
                                ),
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
                                self.toggle_layer_visibility(
                                    key,
                                    *index,
                                    ui.input(|i| i.modifiers.shift),
                                );
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
                                    locale::tr(ui, "RULES"),
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
                                && !eye.contains(
                                    ui.input(|i| i.pointer.interact_pos().unwrap_or_default()),
                                )
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
                                    if ui.button(locale::tr(ui, "Edit layer settings")).clicked() {
                                        self.definition_uid = Some(*uid);
                                        self.show_panel(Some(Panel::Layers));
                                        ui.close();
                                    }
                                    if auto && ui.button(locale::tr(ui, "Edit rules")).clicked() {
                                        self.definition_uid = Some(*uid);
                                        self.show_panel(Some(Panel::Rules));
                                        ui.close();
                                    }
                                });
                            ui.add_space(1.);
                        }
                    });
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
                    let chip_height = if matches!(kind.as_str(), "Entities" | "IntGrid") {
                        21.
                    } else {
                        23.5
                    };
                    egui::Frame::new()
                        .fill(Color32::from_rgb(39, 64, 48))
                        .stroke(Stroke::new(1_f32, Color32::from_rgb(94, 171, 107)))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4., 0.);
                                for (keys, label) in hints {
                                    let translated = locale::tr(ui, label);
                                    let font = FontId::new(
                                        14.,
                                        egui::FontFamily::Name("panel-title".into()),
                                    );
                                    let label_width = if translated == *label {
                                        label.len() as f32 * 6.5
                                    } else {
                                        ui.painter()
                                            .layout_no_wrap(
                                                translated.into(),
                                                font.clone(),
                                                Color32::WHITE,
                                            )
                                            .size()
                                            .x
                                    };
                                    let width =
                                        keys.iter().map(|key| shortcut_width(ui, key)).sum::<f32>()
                                            + 16.
                                            + label_width
                                            + 6.;
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(width, chip_height),
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
                                            egui::vec2(w, chip_height),
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
                                        translated,
                                        font,
                                        Color32::from_rgb(121, 221, 138),
                                    );
                                }
                            });
                        });
                    ui.add_space(16.);
                }
                self.library_palette(ctx, ui);
            });
    }

    pub(super) fn world_switcher(&mut self, ctx: &egui::Context) {
        let Some(project) = &self.project else {
            return;
        };
        let worlds = canvas::values(&project.json["worlds"]);
        if worlds.len() <= 1 {
            return;
        }
        let origin = ctx.content_rect().left_top() + egui::vec2(SIDEBAR_WIDTH + 2., 4.);
        let mut requested = None;
        egui::Area::new(egui::Id::new("world-switcher"))
            .fixed_pos(origin)
            .order(egui::Order::Foreground)
            .constrain(false)
            .show(ctx, |ui| {
                if self.is_busy() || self.pending.is_some() {
                    ui.disable();
                }
                egui::Frame::new()
                    .fill(theme::DARK)
                    .corner_radius(3)
                    .inner_margin(2)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        ui.horizontal(|ui| {
                            for (index, world) in worlds.iter().enumerate() {
                                let name = text(world, "identifier", "World");
                                let active = self.active_world == Some(index);
                                let (rect, response) =
                                    ui.allocate_exact_size(egui::vec2(42., 26.), Sense::click());
                                let (fill, border, text_color) = if active {
                                    (theme::ACCENT, theme::LIGHT, Color32::BLACK)
                                } else if response.hovered() {
                                    (Color32::TRANSPARENT, Color32::WHITE, Color32::WHITE)
                                } else {
                                    (Color32::TRANSPARENT, theme::LIGHT, theme::ACCENT)
                                };
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::SelectableLabel,
                                        ui.is_enabled(),
                                        active,
                                        name,
                                    )
                                });
                                ui.painter().rect(
                                    rect,
                                    3.,
                                    fill,
                                    Stroke::new(1., border),
                                    StrokeKind::Inside,
                                );
                                ui.painter().with_clip_rect(rect.shrink(1.)).text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    short_world_name(name),
                                    FontId::new(12., egui::FontFamily::Name("heading".into())),
                                    text_color,
                                );
                                if response
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    requested = Some(index);
                                }
                            }
                        });
                    });
            });
        if let Some(index) = requested {
            self.navigate_world(Some(index));
        }
    }

    pub(super) fn canvas_toolbar(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.project.is_none() {
            return;
        }
        egui::Area::new(egui::Id::new("editing-options"))
            .fixed_pos(rect.left_top() + egui::vec2(0.0, 46.0))
            .order(egui::Order::Foreground)
            .interactable(self.camera_drag.is_none())
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                ui.set_width(34.0);
                let label_font = FontId::new(12., egui::FontFamily::Name("panel-title".into()));
                ui.add(
                    egui::Label::new(
                        RichText::new(locale::tr(ui, "Visuals"))
                            .font(label_font.clone())
                            .color(Color32::from_rgb(94, 146, 240)),
                    )
                    .extend(),
                )
                .on_hover_ui(|ui| {
                    let cache = self.assets.stats();
                    ui.set_max_width(ui.spacing().tooltip_width);
                    ui.label(locale::format(
                        ui,
                        "{zoom}% · {cells} visible cells · {cpu} ms CPU\n{textures} textures · {memory} MiB · {pending} pending",
                        &[
                            ("zoom", &format!("{:.0}", self.viewport.zoom * 100.0)),
                            ("cells", &self.visible_tiles.to_string()),
                            ("cpu", &format!("{:.2}", self.frame_time)),
                            ("textures", &cache.textures.to_string()),
                            ("memory", &format!("{:.1}", cache.bytes as f64 / 1_048_576.0)),
                            ("pending", &cache.pending.to_string()),
                        ],
                    ));
                });
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
                ui.add(
                    egui::Label::new(
                        RichText::new(locale::tr(ui, "Editing"))
                            .font(label_font)
                            .color(Color32::from_rgb(207, 221, 64)),
                    )
                    .extend(),
                );
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
        self.world_layers_hud(ctx, rect);
    }

    fn world_layers_hud(&mut self, ctx: &egui::Context, canvas: Rect) {
        let Some((min, max)) = self
            .world_depth_bounds()
            .filter(|(min, max)| self.world && min < max)
        else {
            return;
        };
        const TITLE_HEIGHT: f32 = 27.;
        const ROW_HEIGHT: f32 = 28.;
        const ROW_STEP: f32 = 20.;
        const LABEL_FONT: f32 = 13.333;
        let row_count = max.abs_diff(min).saturating_add(1);
        let panning = self.camera_drag.is_some();
        let mut next_depth = None;
        // The source World wrapper contributes its one-pixel seam before the 40px offset.
        egui::Area::new(egui::Id::new("world-depths"))
            .fixed_pos(canvas.left_top() + egui::vec2(41., 46.))
            .constrain(false)
            .order(egui::Order::Foreground)
            .interactable(!panning)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::DARK)
                    .inner_margin(3)
                    .corner_radius(4)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 2,
                        spread: 0,
                        color: Color32::from_black_alpha(102),
                    })
                    .show(ui, |ui| {
                        let font = FontId::proportional(LABEL_FONT);
                        let label_width = [min, max]
                            .into_iter()
                            .map(|depth| ui.painter().layout_no_wrap(depth.to_string(), font.clone(), theme::LIGHT).size().x)
                            .fold(LABEL_FONT, f32::max) + 8.;
                        ui.set_width(103_f32.max(61. + label_width));
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() * 0.7 - 6.).max(1.))
                            .auto_shrink([false, true])
                            .scroll_source(egui::scroll_area::ScrollSource {
                                scroll_bar: !panning,
                                drag: false,
                                mouse_wheel: !panning,
                            })
                            .show_viewport(ui, |ui, viewport| {
                                let origin = ui.max_rect().min;
                                let width = ui.available_width();
                                ui.set_min_size(egui::vec2(width, TITLE_HEIGHT + ROW_HEIGHT - ROW_STEP + ROW_STEP * row_count as f32));
                                if viewport.min.y <= TITLE_HEIGHT {
                                    let title_rect = Rect::from_min_size(origin + egui::vec2(0., 3.), egui::vec2(width - 16., 16.));
                                    ui.put(title_rect, egui::Label::new(RichText::new(locale::tr(ui, "World layers"))
                                        .strong().size(11.733).color(theme::LIGHT)));
                                    let hint = Rect::from_min_size(title_rect.right_top(), Vec2::splat(16.));
                                    let help_color = Color32::from_rgb(94, 171, 107);
                                    ui.painter().circle_stroke(hint.center(), 7., Stroke::new(2., help_color));
                                    ui.painter().text(hint.center(), Align2::CENTER_CENTER, "?", FontId::proportional(11.), help_color);
                                    if !panning {
                                        let response = ui.interact(hint, ui.id().with("depth-help"), Sense::hover())
                                            .on_hover_cursor(egui::CursorIcon::Help);
                                        source_tooltip(&response, "World layers are created or removed automatically, as you arrange your levels using the \"Change depth\" buttons (ie. move level above/beneath).", &[], true);
                                    }
                                }

                                let start = ((viewport.min.y - TITLE_HEIGHT - ROW_HEIGHT) / ROW_STEP).floor().max(0.) as u64;
                                let end = (((viewport.max.y - TITLE_HEIGHT) / ROW_STEP).ceil() as u64).min(row_count);
                                let rows = start..end;
                                let active_index = (min..=max).contains(&self.world_depth)
                                    .then(|| max.abs_diff(self.world_depth))
                                    .filter(|index| rows.contains(index));
                                // Paint the selected layer above both of its overlapping neighbors.
                                for index in rows.filter(|index| Some(*index) != active_index).chain(active_index) {
                                    let depth = (i128::from(max) - i128::from(index)) as i64;
                                    let active = depth == self.world_depth;
                                    let row = Rect::from_min_size(origin + egui::vec2(0., TITLE_HEIGHT + ROW_STEP * index as f32),
                                        egui::vec2(width, ROW_HEIGHT));
                                    let mut hovered = false;
                                    if !active && !panning {
                                        let hit_rect = if depth == min { row } else {
                                            Rect::from_min_size(row.min, egui::vec2(width, ROW_STEP))
                                        };
                                        let response = ui.interact(hit_rect, ui.id().with(("depth", depth)), Sense::click())
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        hovered = response.hovered();
                                        if response.clicked() {
                                            next_depth = Some(depth);
                                        }
                                    }
                                    if hovered {
                                        ui.painter().rect_filled(row.translate(egui::vec2(2., 0.)), 4., theme::MEDIUM);
                                    }
                                    paint_world_depth_row(ui, row, depth, depth == min, index == 0, active, hovered, &font);
                                }
                            });
                    });
            });
        if let Some(depth) = next_depth {
            self.select_world_depth(depth);
        }
    }
}

fn paint_world_depth_row(
    ui: &egui::Ui,
    row: Rect,
    depth: i64,
    bottom: bool,
    top: bool,
    active: bool,
    hovered: bool,
    font: &FontId,
) {
    let icon = if bottom {
        egui::include_image!("../../assets/icons/worldLayer_top.svg")
    } else {
        egui::include_image!("../../assets/icons/worldLayer_below.svg")
    };
    let tint = if active || hovered {
        theme::ACCENT
    } else if top {
        Color32::WHITE.gamma_multiply(0.5)
    } else {
        Color32::WHITE
    };
    let left = row.left() + if hovered { 6. } else { 4. };
    egui::Image::new(icon)
        .tint(tint)
        .uv(Rect {
            min: egui::pos2(0., 1.),
            max: egui::pos2(1., 0.),
        })
        .paint_at(
            ui,
            Rect::from_center_size(egui::pos2(left + 25., row.center().y), egui::vec2(50., 25.)),
        );
    let color = if active {
        Color32::BLACK
    } else if hovered {
        theme::TEXT
    } else {
        theme::LIGHT
    };
    let label = ui
        .painter()
        .layout_no_wrap(depth.to_string(), font.clone(), color);
    let badge = Rect::from_min_size(
        egui::pos2(left + 53., row.center().y - 7.5),
        egui::vec2(label.size().x.max(font.size) + 8., 15.),
    );
    if active {
        ui.painter().rect_filled(badge, 4., theme::ACCENT);
    }
    ui.painter()
        .galley(badge.center() - label.size() * 0.5, label, color);
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
    fn world_switcher_uses_source_labels_and_switches_without_editing() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        let mut project = Project::new();
        let away = project.add_world("awayrealm").unwrap();
        project.json["worlds"][0]["identifier"] = "Northern_Harbor_02".into();
        let first = project.add_level(Some(away), "Away_first", 256, 256).unwrap();
        let second = project.add_level(Some(away), "Away_second", 256, 256).unwrap();
        let original = project.json.clone();
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        for (label, x) in [("NoHa02", 325.), ("awayre", 369.)] {
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == label
                        && ((text.pos + text.galley.size() * 0.5)
                            .distance(egui::pos2(x, 19.))) < 1.0)
            }), "missing world tab {label}");
        }
        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert!(app.world);
        assert_eq!(app.active_world, Some(away));
        assert_eq!(app.selected, Some(first));

        app.navigate_level(second, false);
        app.zen = true;
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert_eq!(app.selected, Some(first), "Clicking the active world selects its first level");
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
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
    fn world_layers_hud_switches_empty_depth_without_editing_or_selecting_a_level() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let far = project.add_level(None, "Behind", 256, 256).unwrap();
        project.level_mut(far).unwrap()["worldDepth"] = serde_json::json!(2);
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let selected = app.selected;
        let title = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "World layers")
            })
        };
        assert!(!title(&app_frame(&mut app, &ctx, vec![])));
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(title(&output), "World mode exposes the source depth range");
        let hud = egui::AreaState::load(&ctx, egui::Id::new("world-depths"))
            .unwrap()
            .rect();
        assert_eq!(
            hud,
            Rect::from_min_size(egui::pos2(441., 46.), egui::vec2(109., 101.))
        );

        let middle = egui::pos2(473., 110.);
        assert_eq!(
            ctx.layer_id_at(middle).unwrap().id,
            egui::Id::new("world-depths")
        );
        app_click(&mut app, &ctx, middle);
        assert_eq!(app.world_depth, 1, "empty depths remain clickable");
        assert_eq!(app.selected, selected);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        let hud_id = egui::Id::new("world-depths");
        let event = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let origin = egui::pos2(900., 170.);
        app_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key {
                    key: Key::Space,
                    physical_key: Some(Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(origin),
            ],
        );
        app_frame(
            &mut app,
            &ctx,
            vec![event(origin, egui::PointerButton::Secondary, true)],
        );
        assert!(app.camera_drag.is_some(), "canvas accepts the zoom drag");
        assert_ne!(ctx.layer_id_at(middle).map(|layer| layer.id), Some(hud_id));
        app_frame(
            &mut app,
            &ctx,
            vec![event(origin, egui::PointerButton::Secondary, false)],
        );
        app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(middle)]);
        app_frame(
            &mut app,
            &ctx,
            vec![event(middle, egui::PointerButton::Primary, true)],
        );
        assert_eq!(
            ctx.layer_id_at(middle).map(|layer| layer.id),
            Some(hud_id),
            "Space+primary starting over the HUD belongs to the HUD"
        );
        app_frame(
            &mut app,
            &ctx,
            vec![event(middle, egui::PointerButton::Primary, false)],
        );

        app.project.as_mut().unwrap().level_mut(far).unwrap()["worldDepth"] =
            serde_json::json!(i32::MAX);
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(title(&output));
        assert!(
            output.shapes.len() < 1500,
            "only visible depth rows are built"
        );
        app.project.as_mut().unwrap().level_mut(far).unwrap()["worldDepth"] = serde_json::json!(0);
        assert!(!title(&app_frame(&mut app, &ctx, vec![])));
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
    fn sidebar_layer_list_keeps_source_height_limits_and_scrolls_its_rows() {
        let guide_top = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(39, 64, 48) => {
                        Some(rect.rect.top())
                    }
                    _ => None,
                })
                .expect("layer tool guide")
        };
        for (count, expected_guide_top) in [(1, 140.), (10, 290.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.set_pixels_per_point(2.);
            let mut project = Project::new();
            for index in 0..count {
                project
                    .add_layer(&format!("Layer{index}"), "Entities", 16)
                    .unwrap();
            }
            let mut app = EditorApp::empty();
            app.set_project(project);
            let hover = egui::pos2(100., 100.);
            app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(hover)]);
            let output = app_frame(&mut app, &ctx, vec![]);
            assert_eq!(
                guide_top(&output),
                expected_guide_top,
                "{count} visible layers"
            );

            if count == 10 {
                let layer_position = |output: &egui::FullOutput| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Layer0" => {
                            let center = text.pos + text.galley.rect.center().to_vec2();
                            shape.clip_rect.contains(center).then_some(center)
                        }
                        _ => None,
                    })
                };
                assert!(
                    layer_position(&output).is_none(),
                    "last row is initially clipped"
                );
                let center = app.viewport.center;
                let zoom = app.viewport.zoom;
                app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -150.),
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                let output = (0..6)
                    .map(|_| app_frame(&mut app, &ctx, vec![]))
                    .last()
                    .unwrap();
                assert_eq!(guide_top(&output), expected_guide_top);
                assert_eq!(app.viewport.center, center);
                assert_eq!(app.viewport.zoom, zoom);
                let pos = layer_position(&output).expect("scroll exposes last layer");
                app_click(&mut app, &ctx, pos);
                assert_eq!(
                    app.layer, 9,
                    "visible layer can be selected after scrolling"
                );
            }
        }
    }

    fn generated_layer_project() -> (Project, i64, i64) {
        let mut project = Project::new();
        let source = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let generated = project.add_layer("Generated", "AutoLayer", 16).unwrap();
        let atlas = project
            .add_tileset(Path::new("Atlas.png"), 32, 32, 16)
            .unwrap();
        project.assign_tileset(generated, atlas).unwrap();
        project
            .update_layer_properties(
                generated,
                &serde_json::json!({
                    "autoSourceLayerDefUid": source,
                    "autoRuleGroups": [{
                        "uid": 700, "name": "Ground", "active": true,
                        "rules": [{
                            "uid": 701, "active": true, "size": 1, "pattern": [1],
                            "tileRectsIds": [[0]], "chance": 1, "breakOnMatch": true
                        }]
                    }]
                }),
            )
            .unwrap();
        (project, generated, atlas)
    }

    #[test]
    fn automatic_layer_has_empty_palette_and_accessible_rules() {
        let (project, uid, _) = generated_layer_project();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = app_frame(&mut app, &ctx, vec![]);

        let mut rules = None;
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                let label = text.galley.text();
                assert!(!matches!(
                    label,
                    "Generated from an IntGrid layer" | "Edit source IntGrid"
                ));
                if label == "RULES" {
                    rules = Some(text.pos + text.galley.size() * 0.5);
                }
            }
        }
        app_click(&mut app, &ctx, rules.expect("automatic layer Rules button"));
        assert_eq!(app.panel, Some(Panel::Rules));
        assert_eq!(app.definition_uid, Some(uid));
        assert!(!app.dirty);
    }

    #[test]
    fn automatic_layer_does_not_start_an_editing_stroke() {
        let (project, uid, _) = generated_layer_project();
        let original = project.json.clone();
        let key = project.level_keys()[0];
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let pointer = egui::pos2(870., 450.);
        app_frame(&mut app, &ctx, vec![]);
        app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pointer)]);
        for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
            let event = |pressed| egui::Event::PointerButton {
                pos: pointer,
                button,
                pressed,
                modifiers: Default::default(),
            };
            let output = app_frame(&mut app, &ctx, vec![event(true)]);
            assert!(!app.stroke);
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::NotAllowed
            );
            app_frame(&mut app, &ctx, vec![event(false)]);
        }
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        app.switch_layer(1);
        app_click(&mut app, &ctx, pointer);
        let project = app.project.as_ref().unwrap();
        let generated = project.layer(key, 0).unwrap();
        assert_eq!(generated["layerDefUid"], uid);
        assert_eq!(generated["autoLayerTiles"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn automatic_layer_has_no_tile_selection_to_navigate_or_save() {
        let (project, _, atlas) = generated_layer_project();
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        assert!(app.set_atlas_selection(atlas, vec![0]));
        app.navigate_palette(1, 0);
        assert_eq!(app.selected_tile_ids_for(atlas), [0]);
        app.tile_selection_memory(false);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for key in [Key::R, Key::X, Key::Y] {
            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
        }
        assert!(!app.random_tiles);
        assert_eq!(app.flips, 0);
    }

    #[test]
    fn searchable_palettes_keep_their_toolbar_above_the_scrollable_rows() {
        let visible_text = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().ends_with(label) => {
                    let pos = text.pos + text.galley.rect.center().to_vec2();
                    shape.clip_rect.contains(pos).then_some(pos)
                }
                _ => None,
            })
        };
        for kind in ["Entities", "IntGrid"] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut project = Project::new();
            project.add_layer("Palette", kind, 16).unwrap();
            if kind == "Entities" {
                for id in 1..=40 {
                    project
                        .add_entity_def(&format!("Item{id:02}"), 16, 16)
                        .unwrap();
                }
            } else {
                project.json["defs"]["layers"][0]["intGridValues"] = Value::Array(
                    (1..=40)
                        .map(|id| {
                            serde_json::json!({"value":id, "identifier":format!("Item{id:02}"),
                            "color":"#22AA77","tile":null,"groupUid":0})
                        })
                        .collect(),
                );
            }

            let mut app = EditorApp::empty();
            app.set_project(project);
            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(egui::pos2(180., 400.))],
            );
            let before = app_frame(&mut app, &ctx, vec![]);
            let search = visible_text(&before, "Search…").expect("initial search toolbar");
            let first = visible_text(&before, "Item01").expect("initial first item");
            let row_height = if kind == "Entities" { 44. } else { 46. };
            let first_row = before
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.contains(first)
                            && rect.rect.height() == row_height
                            && rect.rect.width() > 250. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("source-sized first palette row");
            assert_eq!(first_row.min, egui::pos2(9., 227.), "{kind} row inset");
            assert!(visible_text(&before, "Item40").is_none());

            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -2400.),
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let after = (0..6)
                .map(|_| app_frame(&mut app, &ctx, vec![]))
                .last()
                .unwrap();
            assert!(
                visible_text(&after, "Item40").is_some(),
                "{kind} list must scroll to its last row"
            );
            assert_eq!(
                visible_text(&after, "Search…"),
                Some(search),
                "{kind} search should stay above scrolling items"
            );
        }
    }

    #[test]
    fn entity_and_intgrid_palette_rows_share_source_tints_and_outlines() {
        for (kind, row_height) in [("Entities", 44.), ("IntGrid", 46.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut project = Project::new();
            project.add_layer("Palette", kind, 16).unwrap();
            let mut app = EditorApp::empty();
            if kind == "Entities" {
                let selected = project.add_entity_def("Target", 16, 16).unwrap();
                project.add_entity_def("Other", 16, 16).unwrap();
                for entity in project.json["defs"]["entities"].as_array_mut().unwrap() {
                    entity["color"] = "#22AA77".into();
                }
                app.entity_uid = Some(selected);
            } else {
                project.json["defs"]["layers"][0]["intGridValues"] = serde_json::json!([
                    {"value":1,"identifier":"Target","color":"#22AA77","tile":null,"groupUid":0},
                    {"value":2,"identifier":null,"color":"#22AA77","tile":null,"groupUid":0}
                ]);
                app.int_value = 1;
            }
            app.set_project(project);
            app_frame(&mut app, &ctx, vec![]);
            let before = app_frame(&mut app, &ctx, vec![]);
            let has_rect = |output: &egui::FullOutput, rect, fill, stroke| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(shape)
                        if shape.rect == rect && shape.fill == fill && shape.stroke == stroke)
                })
            };
            let first_row = before
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(shape)
                        if shape.rect.min == egui::pos2(9., 227.)
                            && shape.rect.height() == row_height
                            && shape.rect.width() > 250. =>
                    {
                        Some(shape.rect)
                    }
                    _ => None,
                })
                .expect("first palette row");
            let stripe = Rect::from_min_size(first_row.min, egui::vec2(6., row_height));
            let outline = Stroke::new(1., Color32::WHITE);
            assert!(
                has_rect(
                    &before,
                    stripe,
                    Color32::from_rgb(34, 170, 119),
                    Stroke::NONE
                ),
                "{kind} row color stripe"
            );
            assert!(has_rect(&before, first_row, Color32::TRANSPARENT, outline));
            assert!(has_rect(
                &before,
                first_row,
                Color32::from_rgb(180, 226, 209),
                Stroke::NONE
            ));

            let second_row = first_row.translate(egui::vec2(0., row_height + 1.));
            let events = vec![egui::Event::PointerMoved(second_row.center())];
            app_frame(&mut app, &ctx, events);
            let hovered = app_frame(&mut app, &ctx, vec![]);
            let inactive_fill = Color32::from_rgb(45, 88, 82);
            assert!(has_rect(&before, second_row, inactive_fill, Stroke::NONE));
            assert!(!has_rect(
                &before,
                second_row,
                Color32::TRANSPARENT,
                outline
            ));
            assert!(has_rect(&hovered, second_row, inactive_fill, Stroke::NONE));
            assert!(has_rect(
                &hovered,
                second_row,
                Color32::TRANSPARENT,
                outline
            ));

            if kind == "IntGrid" {
                let labels = before.shapes.iter().filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if first_row.union(second_row).contains(shape.pos) => {
                        Some(shape.galley.text())
                    }
                    _ => None,
                });
                assert_eq!(labels.collect::<Vec<_>>(), ["Target"]);
                let swatch = Rect::from_min_size(egui::pos2(25., 233.), Vec2::splat(34.));
                assert!(has_rect(
                    &before,
                    swatch,
                    Color32::from_rgb(17, 85, 60),
                    Stroke::new(1., Color32::from_rgb(78, 187, 146))
                ));
            }
        }
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
