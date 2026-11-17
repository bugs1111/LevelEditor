//! Shared native rendering of source form help and button treatments.
use super::*;
use std::sync::LazyLock;
static HELP: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../assets/forms/source-help.json"))
        .expect("bundled source form help")
});
fn help_text(key: &str, label: &str) -> Option<&'static str> {
    let alias = match key {
        "editorDisplayMode" => "displayMode",
        "editorDisplayPos" => "displayPos",
        "identifier" => "eName",
        "uiColor" => "uiColor",
        "tileGridSize" => "gridSize",
        _ => key,
    };
    HELP[alias]
        .as_str()
        .or_else(|| HELP[key].as_str())
        .or_else(|| HELP[label].as_str())
}
pub(in crate::app) const PROPERTY_LABEL_WIDTH: f32 = 124.;
pub(in crate::app) fn property_label(ui: &mut egui::Ui, label: &str, key: &str) {
    property_label_with_width(ui, label, key, PROPERTY_LABEL_WIDTH);
}
pub(in crate::app) fn property_label_with_width(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_width: f32,
) {
    property_label_layout(ui, label, key, label_width, true);
}
pub(in crate::app) fn property_label_nowrap(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_width: f32,
) {
    property_label_layout(ui, label, key, label_width, false);
}
fn property_label_layout(ui: &mut egui::Ui, label: &str, key: &str, label_width: f32, wrap: bool) {
    let info = HELP["__info__"][key]
        .as_str()
        .or_else(|| HELP["__info__"][label].as_str());
    let width = ui.available_width().min(label_width);
    let reserve = if info.is_some() { 32. } else { 8. };
    let text_width = (width - reserve).max(0.);
    let font = FontId::proportional(12.667);
    let color = ui.visuals().text_color();
    // CSS dt labels wrap at spaces, without splitting an individual word.
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in label.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if wrap
            && !line.is_empty()
            && ui
                .painter()
                .layout_no_wrap(candidate.clone(), font.clone(), color)
                .size()
                .x
                > text_width
        {
            lines.push(line);
            line = word.to_owned();
        } else {
            line = candidate;
        }
    }
    lines.push(line);
    let galley = ui.painter().layout_no_wrap(lines.join("\n"), font, color);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, galley.size().y.max(30.)), Sense::hover());
    let right = rect.right() - reserve;
    ui.painter()
        .with_clip_rect(
            Rect::from_min_max(rect.min, egui::pos2(right, rect.bottom()))
                .intersect(ui.clip_rect()),
        )
        .galley(
            egui::pos2(
                right - galley.size().x,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            color,
        );
    if let Some(help) = info {
        let badge = Rect::from_center_size(
            egui::pos2(rect.right() - 12., rect.center().y),
            egui::vec2(24., 16.),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(badge)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        help_badge(&mut child, help);
    } else if let Some(help) = help_text(key, label) {
        help_tooltip(response, help);
    }
}

fn help_tooltip(response: egui::Response, help: &str) -> egui::Response {
    response.on_hover_ui(|ui| {
        egui::Frame::new()
            .fill(Color32::from_rgb(60, 97, 72))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_max_width(250.);
                let mut job = egui::text::LayoutJob::default();
                for (index, part) in help.replace("\\n", "\n").split("**").enumerate() {
                    job.append(
                        part,
                        0.,
                        egui::TextFormat {
                            font_id: if index % 2 == 1 {
                                FontId::new(15., egui::FontFamily::Name("heading".into()))
                            } else {
                                FontId::proportional(15.)
                            },
                            color: if index % 2 == 1 {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(121, 221, 138)
                            },
                            ..Default::default()
                        },
                    );
                }
                ui.label(job);
            });
    })
}
fn help_badge(ui: &mut egui::Ui, help: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24., 16.), Sense::hover());
    let hovered = response.hovered();
    ui.painter().rect_filled(
        rect,
        8.,
        if hovered {
            Color32::from_rgb(121, 221, 138)
        } else {
            Color32::from_rgba_unmultiplied(84, 93, 115, 115)
        },
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "?",
        FontId::new(12., egui::FontFamily::Name("heading".into())),
        if hovered {
            Color32::WHITE
        } else {
            Color32::from_rgba_unmultiplied(30, 34, 41, 115)
        },
    );
    help_tooltip(response.on_hover_cursor(egui::CursorIcon::Help), help)
}

fn disabled_hatch(ui: &egui::Ui, rect: Rect) {
    egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
        .paint_at(ui, rect);
}

pub(in crate::app) fn decorate(ui: &egui::Ui, response: &egui::Response) {
    let rect = response.rect;
    if !response.enabled() {
        disabled_hatch(ui, rect);
        ui.painter().rect_stroke(
            rect,
            2.,
            Stroke::new(1_f32, Color32::from_rgb(115, 127, 157)),
            StrokeKind::Inside,
        );
        return;
    }
    ui.painter().rect_stroke(
        rect.shrink(0.5),
        0.,
        Stroke::new(1_f32, Color32::from_white_alpha(56)),
        StrokeKind::Inside,
    );
    ui.painter().line_segment(
        [
            rect.left_bottom() - egui::vec2(0., 1.),
            rect.right_bottom() - egui::vec2(0., 1.),
        ],
        Stroke::new(2_f32, Color32::from_black_alpha(77)),
    );
}
fn shadow(ui: &mut egui::Ui) -> egui::layers::ShapeIdx {
    ui.painter().add(egui::Shape::Noop)
}
fn finish(ui: &egui::Ui, index: egui::layers::ShapeIdx, response: &egui::Response) {
    if response.enabled() {
        ui.painter().set(
            index,
            egui::epaint::Shadow {
                offset: [0, 2],
                blur: 2,
                spread: 0,
                color: Color32::from_black_alpha(77),
            }
            .as_shape(response.rect, 0.),
        );
    }
    decorate(ui, response);
}
pub(in crate::app) fn button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let index = shadow(ui);
    let response = ui.button(label);
    finish(ui, index, &response);
    response
}
pub(in crate::app) fn small_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let index = shadow(ui);
    let response = ui.small_button(label);
    finish(ui, index, &response);
    response
}
pub(in crate::app) fn add_sized_button(
    ui: &mut egui::Ui,
    size: impl Into<Vec2>,
    button: egui::Button<'_>,
) -> egui::Response {
    let index = shadow(ui);
    let response = ui.add_sized(size, button);
    finish(ui, index, &response);
    response
}
pub(in crate::app) fn add_enabled_button(
    ui: &mut egui::Ui,
    enabled: bool,
    button: egui::Button<'_>,
) -> egui::Response {
    let index = shadow(ui);
    let response = ui.add_enabled(enabled, button);
    finish(ui, index, &response);
    response
}
pub(in crate::app) fn checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().icon_width = 24.;
        ui.spacing_mut().icon_width_inner = 18.;
        let fill = if *value {
            Color32::from_rgb(0, 117, 255)
        } else {
            Color32::WHITE
        };
        let states = &mut ui.visuals_mut().widgets;
        for state in [
            &mut states.inactive,
            &mut states.hovered,
            &mut states.active,
        ] {
            state.bg_fill = fill;
            state.fg_stroke = Stroke::new(3_f32, Color32::WHITE);
            state.bg_stroke = Stroke::new(1_f32, Color32::GRAY);
            state.corner_radius = 2.into();
        }
        let response = ui.checkbox(value, label);
        let response = if let Some(help) = help_text(label, label) {
            help_tooltip(response, help)
        } else {
            response
        };
        if !response.enabled() {
            disabled_hatch(ui, response.rect);
        }
        response
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrow_property_label_cannot_paint_over_the_previous_column() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let cell = Rect::from_min_size(egui::pos2(240., 40.), egui::vec2(108., 80.));
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 300.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(cell), |ui| {
                        property_label(ui, "Documentation", "Documentation")
                    });
                });
            },
        );
        let (shape, clip) = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Documentation" => {
                    Some((text, shape.clip_rect))
                }
                _ => None,
            })
            .unwrap();
        let visible = Rect::from_min_size(shape.pos, shape.galley.size()).intersect(clip);
        assert!(
            visible.left() >= cell.left(),
            "label paints outside its column: {visible:?}"
        );
        assert!(
            visible.right() <= cell.right() - 32.,
            "label paints over its help badge: {visible:?}"
        );
    }
    #[test]
    fn rtl_form_label_keeps_documentation_on_one_line_and_help_on_the_right() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(124., 30.),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| property_label(ui, "Documentation", "Documentation"),
                );
            });
        });
        let label = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Documentation" => Some(t),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            label.galley.rows.len(),
            1,
            "source label must not wrap inside a word"
        );
        let badge = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.size() == egui::vec2(24., 16.) => Some(r.rect),
                _ => None,
            })
            .unwrap();
        assert!(
            badge.left() >= label.pos.x + label.galley.size().x,
            "source info follows label even inside RTL owner"
        );
    }
    #[test]
    fn source_info_pill_and_checkbox_title_preserve_click_target() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 300.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| property_label(ui, "Tags", "Tags"));
            },
        );
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.size()==egui::vec2(24.,16.))),"source explicit info must have its 24×16 pill");
        let mut value = false;
        let mut checkbox_rect = Rect::NOTHING;
        let mut raw_width = 0.;
        let mut changed = false;
        let mut frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.spacing_mut().icon_width = 24.;
                        ui.spacing_mut().icon_width_inner = 18.;
                        raw_width = ui.checkbox(&mut false, "Hollow").rect.width();
                        property_label(ui, "Tags", "Tags");
                        let response = checkbox(ui, &mut value, "Hollow");
                        checkbox_rect = response.rect;
                        changed = response.changed();
                    });
                },
            );
            (checkbox_rect, raw_width, changed, value)
        };
        let (rect, width, _, _) = frame(vec![]);
        assert!(
            (rect.width() - width).abs() < 0.1,
            "source title tooltip must not add a badge to checkboxes"
        );
        assert_eq!(
            HELP["__info__"]["Tags"],
            "Tags are used to group entities in custom categories. Multiple tags can be assigned to a single entity. You may also use tags in Layers settings to filter which entities are allowed and which are not. Example: add an \"Actor\" tag to some entities and create an Entity layer that only accepts \"Actors\" entities in it."
        );
        let point = rect.left_center() + egui::vec2(12., 0.);
        for pressed in [true, false] {
            let (_, _, changed, value) = frame(vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]);
            if !pressed {
                assert!(
                    changed && value,
                    "shared help wrapper must preserve checkbox edit response"
                );
            }
        }
    }
}
