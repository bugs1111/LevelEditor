//! Shared native rendering of source form help and button treatments.
use super::*;

pub(in crate::app) fn css_ease_remaining(progress: f32) -> f32 {
    // Chromium's default CSS ease is cubic-bezier(0.25, 0.1, 0.25, 1).
    let (mut low, mut high) = (0., 1.);
    for _ in 0..12 {
        let t: f32 = (low + high) * 0.5;
        let x = 0.75 * (1. - t) * t + t.powi(3);
        if x < progress {
            low = t;
        } else {
            high = t;
        }
    }
    let t: f32 = (low + high) * 0.5;
    (1. - t).powi(2) * (1. + 1.7 * t)
}

pub(in crate::app) fn edit_text(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    widget: impl FnOnce(&mut egui::Ui, &mut String, egui::Id) -> egui::Response,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let (original, mut draft) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .filter(|(original, _)| original == current)
        .unwrap_or_else(|| (current.into(), current.into()));
    ui_commit::prepare_editor(ui.ctx(), id);
    let response = widget(ui, &mut draft, id);
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

#[derive(Clone, Copy)]
pub(in crate::app) enum NumberStyle {
    Plain { placeholder: Option<&'static str> },
    Percent,
    PercentSlider { min: f64, max: f64 },
}

pub(in crate::app) fn step_number(ui: &mut egui::Ui, id: egui::Id, draft: &mut String) -> bool {
    if !ui.is_enabled() || !ui.memory(|memory| memory.has_focus(id)) {
        return false;
    }
    let step = ui.input_mut(|input| {
        let up = input.count_and_consume_key(egui::Modifiers::NONE, Key::ArrowUp);
        let down = input.count_and_consume_key(egui::Modifiers::NONE, Key::ArrowDown);
        up as f64 - down as f64
    });
    if step == 0. {
        return false;
    }
    let value = draft.trim().parse::<f64>().ok().filter(|value| value.is_finite());
    *draft = (value.unwrap_or(0.) + step).to_string();
    true
}

pub(in crate::app) fn select_text_input(ctx: &egui::Context, id: egui::Id, text: &str) {
    use egui::text::{CCursor, CCursorRange};
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(0),
            CCursor::new(text.chars().count()),
        )));
        state.store(ctx, id);
    }
}

pub(in crate::app) fn number_input(
    ui: &mut egui::Ui,
    key: &str,
    current: &str,
    style: NumberStyle,
) -> Option<String> {
    let percent = !matches!(style, NumberStyle::Plain { .. });
    let slider = matches!(style, NumberStyle::PercentSlider { .. });
    let mut immediate = None;
    let mut previewing = false;
    let id = ui.make_persistent_id(key);
    let typed = edit_text(ui, key, current, |ui, draft, id| {
        let gesture = id.with("slider-start");
        let (pressed, held, released, position) = ui.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.primary_pressed(),
                pointer.primary_down(),
                pointer.primary_released(),
                pointer.interact_pos(),
            )
        });
        let distance = ui
            .data(|data| data.get_temp::<f32>(gesture))
            .zip(position)
            .map_or(0., |(start, current)| current.x - start);
        let mut preview = match style {
            NumberStyle::PercentSlider { min, max }
                if ui.is_enabled() && (held || released) && distance.abs() >= 3. =>
            {
                let start = current.parse::<f64>().unwrap_or(0.);
                Some(
                    (start + f64::from(distance) * 0.4 + 0.5)
                        .floor()
                        .clamp(min, max)
                        .to_string(),
                )
            }
            _ => None,
        };
        previewing = preview.is_some();
        if !previewing && step_number(ui, id, draft) {
            immediate = Some(draft.clone());
        }
        let text = preview.as_mut().unwrap_or(draft);
        let mut edit = egui::TextEdit::singleline(text)
            .id(id)
            .font(FontId::proportional(13.333))
            .vertical_align(egui::Align::Center)
            .margin(egui::Margin {
                left: 8,
                right: if percent { 22 } else { 8 },
                top: 0,
                bottom: 0,
            })
            .interactive(!previewing);
        let glow = previewing.then(|| shadow(ui));
        if previewing {
            edit = edit.text_color(theme::ACCENT);
        }
        let placeholder = match style {
            NumberStyle::Plain { placeholder } => placeholder,
            _ => None,
        };
        if let Some(placeholder) = placeholder {
            edit = edit.hint_text(locale::tr(ui, placeholder));
        }
        let width = if percent || placeholder.is_some() {
            90.
        } else {
            60.
        };
        let response = ui.add_sized([width, 33.], edit);
        if slider && response.enabled() {
            if pressed
                && response.is_pointer_button_down_on()
                && let Some(position) = position
            {
                ui.data_mut(|data| data.insert_temp(gesture, position.x));
            }
        }
        if percent {
            let (icon, size) = if slider && response.enabled() {
                (egui::include_image!("../../assets/icons/slide.svg"), 18.)
            } else {
                (
                    egui::include_image!("../../assets/icons/percent.svg"),
                    13.333,
                )
            };
            egui::Image::new(icon).paint_at(
                ui,
                Rect::from_center_size(
                    response.rect.right_center() - egui::vec2(4. + size / 2., 0.),
                    Vec2::splat(size),
                ),
            );
        }
        if let Some(glow) = glow {
            ui.painter().set(
                glow,
                egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: 8,
                    spread: 0,
                    color: Color32::from_rgb(213, 105, 50),
                }
                .as_shape(response.rect, 0.),
            );
            let rect = response.rect.shrink(0.5);
            ui.painter().extend(egui::Shape::dashed_line(
                &[
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ],
                Stroke::new(1., theme::ACCENT),
                3.,
                2.,
            ));
        }
        if !held {
            ui.data_mut(|data| data.remove::<f32>(gesture));
        }
        if released && distance.abs() > 3. && previewing {
            immediate = preview;
            ui.memory_mut(|memory| memory.surrender_focus(id));
        } else if !previewing && (response.gained_focus() || slider && response.clicked()) {
            select_text_input(ui.ctx(), id, draft);
        }
        if slider {
            response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        } else {
            response
        }
    });
    if immediate.is_some() {
        ui.data_mut(|data| data.remove::<(String, String)>(id));
        immediate
    } else if previewing {
        None
    } else {
        typed
    }
}

pub(in crate::app) fn palette_columns_menu(button: &egui::Response) -> Option<usize> {
    columns_menu(button, None)
}

pub(in crate::app) fn select_picker_columns_menu(
    button: &egui::Response,
    id: egui::Id,
) -> Option<usize> {
    columns_menu(button, Some(id))
}

fn columns_menu(button: &egui::Response, picker_id: Option<egui::Id>) -> Option<usize> {
    let popup = egui::Popup::menu(button);
    let popup = match picker_id {
        Some(id) => popup.id(id),
        None => popup,
    };
    popup
        .frame(
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1., theme::LIGHT))
                .shadow(egui::Shadow {
                    offset: [0, 3],
                    blur: 10,
                    spread: 1,
                    color: Color32::from_black_alpha(180),
                }),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let mut choice = None;
            for columns in 1..=10 {
                if columns == 2 && picker_id.is_some() {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(158., 33.), Sense::hover());
                    ui.painter().rect_filled(rect, 0., theme::MEDIUM);
                    ui.painter().text(
                        rect.left_center() + egui::vec2(22., 0.),
                        Align2::LEFT_CENTER,
                        locale::tr(ui, "Grid view"),
                        FontId::proportional(14.),
                        Color32::WHITE,
                    );
                }
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(158., 39.), Sense::click());
                let label = match columns {
                    1 if picker_id.is_some() => locale::tr(ui, "List view").into(),
                    1 => locale::tr(ui, "List").into(),
                    _ => locale::format(
                        ui,
                        "{columns} columns",
                        &[("columns", &columns.to_string())],
                    ),
                };
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                });
                if response.hovered() || response.has_focus() {
                    ui.painter().rect_filled(rect, 0., theme::MEDIUM);
                }
                let icon = if columns == 1 {
                    egui::include_image!("../../assets/icons/listView.svg")
                } else {
                    egui::include_image!("../../assets/icons/gridView.svg")
                };
                egui::Image::new(icon)
                    .tint(Color32::from_gray(180))
                    .paint_at(
                        ui,
                        Rect::from_center_size(
                            rect.left_center() + egui::vec2(31., 0.),
                            Vec2::splat(17.),
                        ),
                    );
                ui.painter().text(
                    rect.left_center() + egui::vec2(50., 0.),
                    Align2::LEFT_CENTER,
                    label.to_uppercase(),
                    FontId::proportional(14.),
                    theme::ACCENT,
                );
                if columns < 10 {
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom(),
                        Stroke::new(1., theme::MEDIUM),
                    );
                }
                if response.clicked() {
                    choice = Some(columns);
                }
            }
            if choice.is_some() {
                ui.close();
            }
            choice
        })
        .and_then(|menu| menu.inner)
}

pub(in crate::app) fn pivot_grid(ui: &mut egui::Ui, pivot: [f64; 2]) -> Option<[f64; 2]> {
    pivot_grid_sized(ui, pivot, 20.)
}

pub(super) fn pivot_grid_sized(ui: &mut egui::Ui, pivot: [f64; 2], cell: f32) -> Option<[f64; 2]> {
    let (grid, _) = ui.allocate_exact_size(Vec2::splat(cell * 3.), Sense::hover());
    let mut selected = None;
    for y in 0..3 {
        for x in 0..3 {
            let ratio = [f64::from(x) * 0.5, f64::from(y) * 0.5];
            let rect = Rect::from_min_size(
                grid.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                Vec2::splat(cell),
            );
            ui.painter().rect_filled(
                rect,
                0.,
                if pivot == ratio {
                    theme::ACCENT
                } else {
                    theme::MEDIUM
                },
            );
            ui.painter().rect_stroke(
                rect,
                0.,
                Stroke::new(1., Color32::from_rgb(117, 130, 164)),
                StrokeKind::Inside,
            );
            if ui
                .interact(rect, ui.id().with(("pivot", x, y)), Sense::click())
                .clicked()
            {
                selected = Some(ratio);
            }
        }
    }
    selected
}

pub(in crate::app) const HELP_GREEN: Color32 = Color32::from_rgb(121, 221, 138);

pub(in crate::app) fn help_paragraph(ui: &mut egui::Ui, message: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(locale::tr(ui, message))
                .font(FontId::new(16., egui::FontFamily::Name("panel-title".into())))
                .color(HELP_GREEN),
        )
        .wrap(),
    );
}

pub(in crate::app) fn help_card(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    content: impl FnOnce(&mut egui::Ui),
) -> Rect {
    let rect = egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 61,
            right: 11,
            top: 11,
            bottom: 11,
        })
        .show(ui, |ui| {
            ui.set_width(width - 72.);
            ui.set_min_height((height - 22.).max(0.));
            content(ui);
        })
        .response
        .rect;
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    ui.painter().extend(egui::Shape::dashed_line(
        &points,
        Stroke::new(1., HELP_GREEN),
        2.,
        2.,
    ));
    egui::Image::new(egui::include_image!("../../assets/icons/helpBg.svg")).paint_at(
        ui,
        Rect::from_min_size(rect.left_top() + Vec2::splat(6.), Vec2::splat(48.)),
    );
    rect
}

/// Uppercase source form title and its divider.
pub(in crate::app) fn form_heading(ui: &mut egui::Ui, name: &str) {
    ui.add_space(16.);
    form_heading_line(ui, name);
    ui.add_space(8.);
}

pub(in crate::app) fn form_heading_line(ui: &mut egui::Ui, name: &str) {
    let response = ui.label(
        RichText::new(locale::tr(ui, name).to_uppercase())
            .color(theme::ACCENT)
            .font(FontId::new(
                17.333,
                egui::FontFamily::Name("panel-title".into()),
            )),
    );
    ui.painter().hline(
        ui.max_rect().x_range(),
        response.rect.bottom() + 2.,
        Stroke::new(1_f32, theme::ACCENT),
    );
}

pub(in crate::app) fn form_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    form_row_inner(
        ui,
        label,
        key,
        egui::vec2(label_width, 30.),
        FormLabel::SingleLine,
        body,
    )
}

pub(in crate::app) fn form_row_wrapped<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    form_row_wrapped_sized(ui, label, key, egui::vec2(label_width, 30.), body)
}

pub(in crate::app) fn form_row_wrapped_sized<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_size: Vec2,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    form_row_inner(ui, label, key, label_size, FormLabel::Wrapped, body)
}

pub(in crate::app) fn form_grid_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    size: Vec2,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    form_row_inner(ui, label, key, size, FormLabel::TopAligned, body)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormLabel {
    SingleLine,
    Wrapped,
    TopAligned,
}

fn form_row_inner<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_size: Vec2,
    layout: FormLabel,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let row = |ui: &mut egui::Ui| {
        ui.spacing_mut().item_spacing.x = 8.;
        property_label_layout(ui, label, key, label_size, layout);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), label_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            body,
        )
        .inner
    };
    if layout == FormLabel::TopAligned {
        ui.horizontal_top(row)
    } else {
        ui.horizontal(row)
    }
    .inner
}

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
const PROPERTY_LABEL_FONT_SIZE: f32 = 12.667;
pub(in crate::app) fn property_label(ui: &mut egui::Ui, label: &str, key: &str) {
    property_label_with_width(ui, label, key, PROPERTY_LABEL_WIDTH);
}
pub(in crate::app) fn property_label_with_width(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_width: f32,
) {
    property_label_layout(
        ui,
        label,
        key,
        egui::vec2(label_width, 30.),
        FormLabel::Wrapped,
    );
}
pub(in crate::app) fn property_label_intrinsic_width(ui: &egui::Ui, label: &str, key: &str) -> f32 {
    let (_, reserve) = property_label_help(label, key);
    ui.painter()
        .layout_no_wrap(
            locale::tr(ui, label).to_owned(),
            FontId::proportional(PROPERTY_LABEL_FONT_SIZE),
            ui.visuals().text_color(),
        )
        .size()
        .x
        + reserve
}

fn property_label_help(label: &str, key: &str) -> (Option<&'static str>, f32) {
    let info = HELP["__info__"][key]
        .as_str()
        .or_else(|| HELP["__info__"][label].as_str());
    (info, if info.is_some() { 32. } else { 8. })
}

fn property_label_layout(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    label_size: Vec2,
    layout: FormLabel,
) {
    let wrap = layout != FormLabel::SingleLine;
    let top_aligned = layout == FormLabel::TopAligned;
    let (info, reserve) = property_label_help(label, key);
    let width = ui.available_width().min(label_size.x);
    let text_width = (width - reserve).max(0.);
    let font = FontId::proportional(PROPERTY_LABEL_FONT_SIZE);
    let color = ui.visuals().text_color();
    // CSS dt labels wrap at spaces, without splitting an individual word.
    let mut lines = Vec::new();
    let mut line = String::new();
    let displayed = locale::tr(ui, label);
    for word in displayed.split_whitespace() {
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
    let galley = if top_aligned {
        let (text, max_width) = if displayed != label {
            (displayed.to_owned(), text_width)
        } else {
            (lines.join("\n"), f32::INFINITY)
        };
        let mut job = egui::text::LayoutJob::simple(text, font, color, max_width);
        job.halign = egui::Align::RIGHT;
        job.sections[0].format.line_height = Some(15.);
        ui.painter().layout_job(job)
    } else if wrap && displayed != label {
        ui.painter()
            .layout(displayed.to_owned(), font, color, text_width)
    } else {
        ui.painter().layout_no_wrap(lines.join("\n"), font, color)
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            width,
            (galley.size().y + if top_aligned { 4. } else { 0. }).max(label_size.y),
        ),
        Sense::hover(),
    );
    let right = rect.right() - reserve;
    let position = if top_aligned {
        egui::pos2(right, rect.top() + 4.)
    } else {
        egui::pos2(
            right - galley.size().x,
            rect.center().y - galley.size().y * 0.5,
        )
    };
    ui.painter()
        .with_clip_rect(
            Rect::from_min_max(rect.min, egui::pos2(right, rect.bottom()))
                .intersect(ui.clip_rect()),
        )
        .galley(position, galley, color);
    if let Some(help) = info {
        let badge = Rect::from_center_size(
            egui::pos2(
                rect.right() - 12.,
                if top_aligned {
                    rect.top() + 12.
                } else {
                    rect.center().y
                },
            ),
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
                for (index, part) in locale::tr(ui, help)
                    .replace("\\n", "\n")
                    .split("**")
                    .enumerate()
                {
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

pub(super) fn field_instance_label(
    ui: &mut egui::Ui,
    name: &str,
    width: f32,
    help: Option<&str>,
    using_default: bool,
    has_error: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.), Sense::click());
    let reserve = if help.is_some() { 40. } else { 8. };
    let mut label = RichText::new(name).color(theme::TEXT);
    if has_error {
        label = label.color(Color32::from_rgb(255, 163, 134));
    } else if using_default {
        label = label.italics().color(theme::TEXT.gamma_multiply(0.9));
    }
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (width - reserve).max(0.),
        FontId::proportional(PROPERTY_LABEL_FONT_SIZE),
    );
    ui.painter().galley(
        egui::pos2(rect.right() - reserve - galley.size().x, rect.top() + 4.),
        galley,
        theme::TEXT,
    );
    if let Some(help) = help {
        let badge = Rect::from_min_size(
            rect.right_top() + egui::vec2(-32., 4.),
            egui::vec2(24., 16.),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(badge));
        help_badge(&mut child, help);
    }
    response.on_hover_text(name)
}

pub(in crate::app) fn context_menu_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::popup(style)
        .fill(theme::DARK)
        .stroke(Stroke::new(1., theme::LIGHT))
        .corner_radius(2.)
        .inner_margin(0.)
}

pub(in crate::app) fn select_popup(
    response: &egui::Response,
    id: egui::Id,
    selected: usize,
    options: &[(&str, Color32)],
) -> Option<usize> {
    let last = options.len().checked_sub(1)?;
    let ctx = &response.ctx;
    let open = egui::Popup::is_id_open(ctx, id);
    let cursor_id = id.with("highlighted");
    if !open {
        ctx.data_mut(|data| data.remove::<usize>(cursor_id));
    }
    let mut choice = None;
    egui::Popup::menu(response)
        .id(id)
        .open_memory(
            ((!open && response.clicked()) || response.clicked_by(egui::PointerButton::Primary))
                .then_some(egui::SetOpenCommand::Toggle),
        )
        .width(response.rect.width())
        .frame(egui::Frame::new().fill(theme::LIGHT))
        .show(|ui| {
            let mut cursor = ui
                .data(|data| data.get_temp::<usize>(cursor_id))
                .unwrap_or(selected)
                .min(last);
            let previous = cursor;
            let confirm = ui.input_mut(|input| {
                let mut pressed = |key| input.consume_key(egui::Modifiers::NONE, key);
                cursor = if pressed(Key::Home) {
                    0
                } else if pressed(Key::End) {
                    last
                } else if pressed(Key::ArrowDown) {
                    (cursor + 1).min(last)
                } else if pressed(Key::ArrowUp) {
                    cursor.saturating_sub(1)
                } else {
                    cursor
                };
                pressed(Key::Enter)
            });
            if confirm {
                choice = Some(cursor);
                ui.close();
            }
            let pointer_moved = ui.input(|input| input.pointer.delta() != Vec2::ZERO);
            let font = FontId::proportional(14.667);
            let width = options
                .iter()
                .map(|(label, _)| {
                    ui.painter()
                        .layout_no_wrap((*label).to_owned(), font.clone(), Color32::WHITE)
                        .size()
                        .x
                        + 16.
                })
                .fold(response.rect.width(), f32::max)
                .min(ctx.content_rect().width());
            egui::ScrollArea::vertical()
                .max_height(250.)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    for (index, &(label, color)) in options.iter().enumerate() {
                        let (rect, option) =
                            ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
                        option.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                index == selected,
                                label,
                            )
                        });
                        if index == cursor && cursor != previous {
                            option.scroll_to_me(None);
                        }
                        if pointer_moved && option.hovered() {
                            cursor = index;
                        }
                        let highlighted = index == cursor;
                        if highlighted {
                            ui.painter().rect_filled(rect, 0., theme::ACCENT);
                        }
                        ui.painter().with_clip_rect(rect).text(
                            rect.left_center() + egui::vec2(8., 0.),
                            Align2::LEFT_CENTER,
                            label,
                            font.clone(),
                            if highlighted { Color32::BLACK } else { color },
                        );
                        if option.clicked() {
                            choice = Some(index);
                            ui.close();
                        }
                    }
                });
            ui.data_mut(|data| data.insert_temp(cursor_id, cursor));
        });
    choice
}

pub(in crate::app) fn disabled_hatch(ui: &egui::Ui, rect: Rect) {
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
    let response = ui.button(locale::widget(ui, label.into()));
    finish(ui, index, &response);
    response
}
pub(in crate::app) fn small_button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let index = shadow(ui);
    let response = ui.small_button(locale::widget(ui, label.into()));
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
pub(in crate::app) fn form_help_link(ui: &mut egui::Ui, width: f32, label: &str, url: &str) {
    ui.scope(|ui| {
        let normal = Color32::from_rgb(73, 141, 85);
        let hover = Color32::from_rgb(94, 171, 107);
        ui.spacing_mut().button_padding = egui::vec2(8., 2.);
        let widgets = &mut ui.visuals_mut().widgets;
        for (style, fill) in [
            (&mut widgets.inactive, normal),
            (&mut widgets.hovered, hover),
            (&mut widgets.active, hover),
        ] {
            style.weak_bg_fill = fill;
            style.bg_stroke = Stroke::NONE;
            style.corner_radius = 0.into();
        }
        let title = locale::tr(ui, label);
        let response = add_sized_button(
            ui,
            [width, 22.],
            egui::Button::new(RichText::new(title).color(Color32::WHITE).size(14.667)).truncate(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(url);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, title));
        if response.clicked() || response.middle_clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    });
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
        let response = ui.checkbox(value, locale::tr(ui, label));
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
