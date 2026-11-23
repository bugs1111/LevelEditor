use super::*;

pub(super) fn rule_value_group(source: &Value, uid: i64) -> Option<&Value> {
    source["intGridValuesGroups"]
        .as_array()?
        .iter()
        .find(|group| group["uid"] == uid)
}

fn alpha(color: Color32, opacity: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), opacity)
}

fn group_heading(
    ui: &mut egui::Ui,
    title: &str,
    height: f32,
    color: Option<Color32>,
    active: bool,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    let fill = if active {
        Color32::WHITE
    } else {
        color.map_or(Color32::TRANSPARENT, |c| alpha(c, 82))
    };
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 2., fill);
    egui::Image::new(egui::include_image!("../../assets/icons/folderClose.svg"))
        .tint(if active {
            theme::DARK
        } else {
            Color32::from_white_alpha(102)
        })
        .paint_at(
            ui,
            Rect::from_center_size(rect.left_center() + egui::vec2(10., 0.), Vec2::splat(14.)),
        );
    let ink = if active {
        Color32::BLACK
    } else {
        Color32::from_rgb(115, 127, 157)
    };
    painter.text(
        rect.left_center() + egui::vec2(20., 0.),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.),
        ink,
    );
    if response.hovered() {
        painter.rect_stroke(
            rect,
            2.,
            Stroke::new(1_f32, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    response.clicked()
}

fn any_condition(ui: &mut egui::Ui, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 31.), Sense::click());
    egui::Image::new(egui::include_image!("../../assets/img/any.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 16.))
        .paint_at(ui, rect);
    let text = RichText::new(locale::tr(ui, "\"Any value\" / \"No value\""))
        .italics()
        .color(Color32::WHITE);
    let galley =
        egui::WidgetText::from(text).into_galley(ui, None, rect.width(), egui::TextStyle::Body);
    ui.painter().galley(
        rect.left_center() + egui::vec2(8., -galley.size().y / 2.),
        galley,
        Color32::WHITE,
    );
    if selected || response.hovered() {
        ui.painter().rect_stroke(
            rect,
            0.,
            Stroke::new(if selected { 2. } else { 1_f32 }, Color32::WHITE),
            StrokeKind::Inside,
        );
    }
    response.clicked()
}

impl EditorApp {
    pub(super) fn rule_value_tile(&mut self, ui: &egui::Ui, rect: Rect, value: &Value) {
        if let Some(project) = self.project.as_ref()
            && value["tile"].is_object()
        {
            library::paint_tile(
                ui.ctx(),
                &ui.painter().with_clip_rect(rect),
                rect,
                project,
                &value["tile"],
                &mut self.assets,
            );
        }
    }

    fn rule_value_card(
        &mut self,
        ui: &mut egui::Ui,
        value: &Value,
        current: &mut i64,
        width: f32,
        columns: usize,
        group_selected: bool,
    ) {
        let Some(id) = value["value"].as_i64() else {
            return;
        };
        let list = columns == 1;
        let selected = *current == id;
        let height = [32., 80., 60., 50., 50., 40., 35., 30., 30., 25.][columns - 1];
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width, height + if list { 0. } else { 22. }),
            Sense::click(),
        );
        let swatch =
            Rect::from_min_size(rect.min, egui::vec2(if list { 32. } else { width }, height));
        let color = value["color"]
            .as_str()
            .map(theme::color)
            .unwrap_or(theme::LIGHT);
        let tint = if selected || group_selected {
            color
        } else {
            color.lerp_to_gamma(Color32::BLACK, 0.2)
        };
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 0., tint);
        painter.rect(
            swatch,
            2.,
            tint.lerp_to_gamma(Color32::BLACK, 0.5),
            Stroke::new(1_f32, tint.lerp_to_gamma(Color32::WHITE, 0.2)),
            StrokeKind::Inside,
        );
        self.rule_value_tile(ui, swatch.shrink(2.), value);
        let label = value["identifier"]
            .as_str()
            .map_or_else(|| id.to_string(), str::to_owned);
        let light = color.r() as f32 * 0.299 + color.g() as f32 * 0.587 + color.b() as f32 * 0.114;
        let foreground = if light >= 152. {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        let (position, align) = if list {
            (
                swatch.right_center() + egui::vec2(8., 0.),
                Align2::LEFT_CENTER,
            )
        } else {
            (
                rect.center_bottom() - egui::vec2(0., 11.),
                Align2::CENTER_CENTER,
            )
        };
        painter.text(
            position,
            align,
            label,
            FontId::proportional(11.333),
            foreground,
        );
        if selected || response.hovered() {
            painter.rect_stroke(
                rect,
                0.,
                Stroke::new(if selected { 2. } else { 1_f32 }, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
        if response.clicked() {
            *current = id;
        }
    }

    fn rule_value_groups(
        &mut self,
        ui: &mut egui::Ui,
        source: &Value,
        selected: &mut i64,
        columns: usize,
    ) {
        let groups = source["intGridValuesGroups"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let values = source["intGridValues"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let headers = std::iter::once((0, None)).chain(
            groups
                .iter()
                .filter_map(|g| Some((g["uid"].as_i64()?, Some(g)))),
        );
        let dense = columns >= 5;
        for (uid, group) in headers {
            let members = values
                .iter()
                .filter(|value| value["groupUid"].as_i64().unwrap_or(0) == uid)
                .collect::<Vec<_>>();
            if members.is_empty() {
                continue;
            }
            let condition = (uid + 1) * 1000;
            let color = group.and_then(|g| g["color"].as_str()).map(theme::color);
            if !groups.is_empty() {
                let title = group.map_or_else(
                    || locale::tr(ui, "Ungrouped").to_owned(),
                    |g| {
                        g["identifier"]
                            .as_str()
                            .map_or_else(
                                || locale::format(ui, "Group {uid}", &[("uid", &uid.to_string())]),
                                str::to_owned,
                            )
                    },
                );
                if group_heading(
                    ui,
                    &title,
                    if dense { 19. } else { 21. },
                    color,
                    *selected == condition,
                ) {
                    *selected = condition;
                }
            }
            let active = *selected == condition;
            let fill = match color {
                Some(color) => alpha(color, if active { 128 } else { 51 }),
                None if active => theme::MEDIUM,
                None => Color32::TRANSPARENT,
            };
            let body = egui::Frame::new()
                .fill(fill)
                .inner_margin(if dense { 4 } else { 8 })
                .show(ui, |ui| {
                    let available = ui.available_width();
                    ui.set_min_width(available);
                    let width = ((available - 2. * (columns - 1) as f32) / columns as f32).max(1.);
                    ui.spacing_mut().item_spacing = Vec2::splat(2.);
                    for row in members.chunks(columns) {
                        ui.horizontal(|ui| {
                            for value in row {
                                self.rule_value_card(ui, value, selected, width, columns, active);
                            }
                        });
                    }
                });
            if active {
                ui.painter().rect_stroke(
                    body.response.rect,
                    0.,
                    Stroke::new(2_f32, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            ui.add_space(8.);
        }
    }

    pub(super) fn rule_value_palette(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut RuleDraft,
        source: &Value,
        width: f32,
        min_height: f32,
    ) -> f32 {
        let project = self
            .project
            .as_ref()
            .and_then(|p| p.json["iid"].as_str())
            .map(str::to_owned);
        let key = format!("RuleValuesColumns_{}", draft.layer);
        let columns = self
            .settings
            .ui_state_int(&key, project.as_deref(), 5)
            .clamp(1, 10) as usize;
        let palette = egui::Frame::new()
            .fill(theme::DARK)
            .corner_radius(3)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(width + 32.);
                    ui.set_min_height(min_height + 32.);
                    ui.spacing_mut().item_spacing.y = 0.;
                    ui.spacing_mut().scroll = egui::style::ScrollStyle {
                        bar_width: 10.,
                        bar_inner_margin: 0.,
                        bar_outer_margin: 0.,
                        ..egui::style::ScrollStyle::solid()
                    };
                    let visuals = ui.visuals_mut();
                    visuals.extreme_bg_color = theme::DARK;
                    for widget in [
                        &mut visuals.widgets.inactive,
                        &mut visuals.widgets.hovered,
                        &mut visuals.widgets.active,
                    ] {
                        widget.bg_fill = Color32::from_rgb(115, 127, 157);
                        widget.corner_radius = 0.into();
                    }
                    egui::ScrollArea::vertical()
                        .id_salt(("rule-values", draft.layer))
                        .max_height(400.)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Frame::new().inner_margin(16).show(ui, |ui| {
                                self.rule_value_groups(ui, source, &mut draft.value, columns);
                                if any_condition(ui, draft.value == ANY_VALUE) {
                                    draft.value = ANY_VALUE;
                                }
                            });
                        })
                })
                .inner
            });
        let rect = palette.response.rect;
        let hit = Rect::from_min_size(
            egui::pos2(palette.inner.inner_rect.right() - 26., rect.top() + 2.),
            egui::vec2(25., 27.),
        );
        let button = ui
            .interact(hit, ui.id().with("rule-values-layout"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(locale::tr(ui, "Value palette layout"));
        ui.painter().rect_filled(
            hit,
            2.,
            if button.hovered() {
                Color32::from_rgb(235, 78, 0)
            } else {
                theme::DARK
            },
        );
        egui::Image::new(egui::include_image!("../../assets/icons/gridView.svg"))
            .paint_at(ui, Rect::from_center_size(hit.center(), Vec2::splat(18.)));
        if let Some(columns) = source_controls::palette_columns_menu(&button) {
            if columns == 1 {
                self.settings.delete_ui_state(&key, project.as_deref());
            } else {
                self.settings
                    .set_ui_state_int(&key, project.as_deref(), columns as i64);
            }
        }
        rect.height()
    }
}
