//! Shared native rendering of source form help and button treatments.
use super::*;
use crate::app::chrome::ActionTooltip as _;
use crate::theme::HELP_TEXT;

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

// Eight points of padding plus the one-point form border.
pub(in crate::app) const FORM_TEXT_INSET: i8 = 9;

pub(in crate::app) fn form_text_box(value: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(value)
        .font(theme::TextRole::FormValue.font())
        .margin(egui::Margin::symmetric(FORM_TEXT_INSET, 2))
        .vertical_align(egui::Align::Center)
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
    Plain {
        placeholder: Option<&'static str>,
    },
    Float,
    Percent,
    PercentSlider {
        min: f64,
        max: f64,
    },
    IntegerSlider {
        min: f64,
        max: f64,
        speed: f64,
        placeholder: Option<&'static str>,
    },
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
    let value = draft
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite());
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
    let width = if matches!(
        style,
        NumberStyle::Plain { placeholder: None } | NumberStyle::Float
    ) {
        60.
    } else {
        90.
    };
    number_input_sized(ui, key, current, style, width)
}

pub(in crate::app) fn number_input_sized(
    ui: &mut egui::Ui,
    key: &str,
    current: &str,
    style: NumberStyle,
    width: f32,
) -> Option<String> {
    let percent = matches!(
        style,
        NumberStyle::Percent | NumberStyle::PercentSlider { .. }
    );
    let slider_bounds = match style {
        NumberStyle::PercentSlider { min, max } => Some((min, max, 0.4)),
        NumberStyle::IntegerSlider {
            min, max, speed, ..
        } => Some((min, max, speed)),
        _ => None,
    };
    let slider = slider_bounds.is_some();
    let mut immediate = None;
    let mut previewing = false;
    let id = ui.make_persistent_id(key);
    let typed = edit_text(ui, key, current, |ui, draft, id| {
        let gesture = id.with("slider-start");
        let (press_origin, held, released, position) = ui.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.press_origin().filter(|_| pointer.primary_pressed()),
                pointer.primary_down(),
                pointer.primary_released(),
                pointer.interact_pos(),
            )
        });
        let distance = ui
            .data(|data| data.get_temp::<f32>(gesture))
            .zip(position)
            .map_or(0., |(start, current)| current.x - start);
        let mut preview = match slider_bounds {
            Some((min, max, speed))
                if ui.is_enabled() && (held || released) && distance.abs() >= 3. =>
            {
                let start = current.parse::<f64>().unwrap_or(0.);
                Some(
                    (start + f64::from(distance) * speed + if percent { 0.5 } else { 0. })
                        .floor()
                        .clamp(min, max)
                        .to_string(),
                )
            }
            _ => None,
        };
        previewing = preview.is_some();
        if !previewing && !matches!(style, NumberStyle::Float) && step_number(ui, id, draft) {
            immediate = Some(draft.clone());
        }
        let text = preview.as_mut().unwrap_or(draft);
        let mut edit = egui::TextEdit::singleline(text)
            .id(id)
            .font(theme::TextRole::FormValue.font())
            .vertical_align(egui::Align::Center)
            .margin(egui::Margin {
                left: 8,
                right: if percent || slider && ui.is_enabled() {
                    22
                } else {
                    8
                },
                top: 0,
                bottom: 0,
            })
            .interactive(!previewing);
        let glow = previewing.then(|| shadow(ui));
        if previewing {
            edit = edit.text_color(theme::ACCENT);
        }
        let placeholder = match style {
            NumberStyle::Plain { placeholder } | NumberStyle::IntegerSlider { placeholder, .. } => {
                placeholder
            }
            _ => None,
        };
        if let Some(placeholder) = placeholder {
            edit = edit.hint_text(locale::tr(ui, placeholder));
        }
        let response = ui.add_sized([width, 33.], edit);
        if slider
            && response.enabled()
            && response.is_pointer_button_down_on()
            && let Some(origin) = press_origin
        {
            ui.data_mut(|data| data.insert_temp(gesture, origin.x));
        }
        if percent || slider && response.enabled() {
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
                Stroke::new(1_f32, theme::ACCENT),
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
                .stroke(Stroke::new(1_f32, theme::LIGHT))
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
                        Stroke::new(1_f32, theme::MEDIUM),
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
                Stroke::new(1_f32, Color32::from_rgb(117, 130, 164)),
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

const HELP_CARD_MARGIN: egui::Margin = egui::Margin {
    left: 61,
    right: 11,
    top: 11,
    bottom: 11,
};

fn help_paragraph_font() -> FontId {
    FontId::new(16., egui::FontFamily::Name("panel-title".into()))
}

pub(in crate::app) fn help_paragraph(ui: &mut egui::Ui, message: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(locale::tr(ui, message))
                .font(help_paragraph_font())
                .color(HELP_TEXT),
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
        .inner_margin(HELP_CARD_MARGIN)
        .show(ui, |ui| {
            ui.set_width(width - 72.);
            ui.set_min_height((height - 22.).max(0.));
            content(ui);
        })
        .response
        .rect;
    paint_help_card(ui, rect, rect.top() + 30.);
    rect
}

pub(in crate::app) fn centered_help_card(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    message: &str,
) {
    let margin = HELP_CARD_MARGIN;
    let paragraph = ui.painter().layout(
        locale::tr(ui, message).into(),
        help_paragraph_font(),
        HELP_TEXT,
        (width - margin.sum().x).max(1.),
    );
    let height = height.max(paragraph.size().y + margin.sum().y);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
    paint_help_card(ui, rect, rect.center().y);
    ui.painter().galley(
        rect.left_center() + egui::vec2(f32::from(margin.left), -paragraph.size().y * 0.5),
        paragraph,
        HELP_TEXT,
    );
}

fn paint_help_card(ui: &egui::Ui, rect: Rect, icon_center_y: f32) {
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    ui.painter().extend(egui::Shape::dashed_line(
        &points,
        Stroke::new(1_f32, HELP_TEXT),
        2.,
        2.,
    ));
    egui::Image::new(egui::include_image!("../../assets/icons/helpBg.svg")).paint_at(
        ui,
        Rect::from_center_size(
            egui::pos2(rect.left() + 30., icon_center_y),
            Vec2::splat(48.),
        ),
    );
}

pub(in crate::app) fn scroll_style() -> egui::style::ScrollStyle {
    egui::style::ScrollStyle {
        bar_width: 10.,
        bar_inner_margin: 0.,
        bar_outer_margin: 0.,
        ..egui::style::ScrollStyle::solid()
    }
}

pub(in crate::app) fn scroll_visuals(ui: &mut egui::Ui) -> egui::Visuals {
    let content_visuals = ui.visuals().clone();
    let visuals = ui.visuals_mut();
    visuals.clip_rect_margin = 0.;
    visuals.extreme_bg_color = theme::MEDIUM;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(115, 127, 157);
    visuals.widgets.hovered.bg_fill = theme::TEXT;
    visuals.widgets.active.bg_fill = theme::TEXT;
    visuals.widgets.inactive.corner_radius = 0.into();
    visuals.widgets.hovered.corner_radius = 0.into();
    visuals.widgets.active.corner_radius = 0.into();
    content_visuals
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
            .font(theme::TextRole::SectionTitle.font()),
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

pub(in crate::app) fn form_row_left_aligned<R>(
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
        FormLabel::LeftAligned,
        body,
    )
}

pub(in crate::app) fn form_row_left_wrapped<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    width: f32,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    form_row_inner(
        ui,
        label,
        key,
        egui::vec2(width, 30.),
        FormLabel::LeftWrapped,
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
    LeftAligned,
    LeftWrapped,
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
        if layout == FormLabel::Wrapped {
            ui.spacing_mut().interact_size.y = label_size.y;
        }
        let highlighted = property_label_layout(ui, label, key, label_size, layout);
        let background = highlighted.then(|| ui.painter().add(egui::Shape::Noop));
        let width = ui.available_width();
        let value = ui.allocate_ui_with_layout(
            egui::vec2(width, label_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            body,
        );
        if let Some(background) = background {
            let rect = Rect::from_min_size(
                value.response.rect.min,
                egui::vec2(width, value.response.rect.height()),
            );
            ui.painter().set(
                background,
                egui::Shape::rect_filled(rect, 3., theme::HELP_FILL),
            );
            ui.painter().rect_stroke(
                rect.expand(2.),
                3.,
                Stroke::new(1_f32, Color32::from_rgb(94, 171, 107)),
                egui::StrokeKind::Outside,
            );
        }
        value.inner
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
pub(in crate::app) fn property_label(ui: &mut egui::Ui, label: &str, key: &str) {
    property_label_with_width(ui, label, key, PROPERTY_LABEL_WIDTH);
}
pub(in crate::app) fn property_label_left_aligned(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    width: f32,
) {
    property_label_layout(
        ui,
        label,
        key,
        egui::vec2(width, 24.),
        FormLabel::LeftAligned,
    );
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
            theme::TextRole::PropertyLabel.font(),
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
) -> bool {
    let wrap = matches!(
        layout,
        FormLabel::Wrapped | FormLabel::TopAligned | FormLabel::LeftWrapped
    );
    let top_aligned = layout == FormLabel::TopAligned;
    let left_aligned = matches!(layout, FormLabel::LeftAligned | FormLabel::LeftWrapped);
    let (info, reserve) = property_label_help(label, key);
    let width = ui.available_width().min(label_size.x);
    let text_width = (width - reserve).max(0.);
    let font = theme::TextRole::PropertyLabel.font();
    let color = if left_aligned {
        Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
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
    let galley = if top_aligned || layout == FormLabel::Wrapped {
        let (text, max_width) = if displayed != label {
            (displayed.to_owned(), text_width)
        } else {
            (lines.join("\n"), f32::INFINITY)
        };
        let mut job = egui::text::LayoutJob::simple(text, font, color, max_width);
        job.halign = egui::Align::RIGHT;
        if top_aligned {
            job.sections[0].format.line_height = Some(15.);
        }
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
    let position = if layout == FormLabel::LeftWrapped {
        rect.left_center() - egui::vec2(0., galley.size().y * 0.5)
    } else if left_aligned {
        rect.left_top() + egui::vec2(0., 1.)
    } else if top_aligned {
        egui::pos2(right, rect.top() + 4.)
    } else {
        egui::pos2(
            right - galley.rect.right(),
            rect.center().y - galley.size().y * 0.5,
        )
    };
    let highlighted = if let Some(help) = info {
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
        help_badge(&mut child, help, key == "simplifiedExport").hovered()
    } else {
        if let Some(help) = help_text(key, label) {
            help_tooltip(response, help, false);
        }
        false
    };
    let painter = ui.painter().with_clip_rect(
        Rect::from_min_max(rect.min, egui::pos2(right, rect.bottom())).intersect(ui.clip_rect()),
    );
    if highlighted && !left_aligned {
        painter.galley_with_override_text_color(position, galley, HELP_TEXT);
    } else {
        painter.galley(position, galley, color);
    }
    highlighted
}

pub(in crate::app) fn help_tooltip(
    response: egui::Response,
    help: &str,
    below: bool,
) -> egui::Response {
    if response.hovered() && response.ctx.input(|input| input.focused) {
        chrome::source_tooltip_popup(
            &response,
            below.then_some(egui::emath::RectAlign::BOTTOM_START),
            egui::Margin {
                left: 10,
                right: 30,
                top: 10,
                bottom: 10,
            },
        )
        .show(|ui| {
            ui.set_max_width(theme::TOOLTIP_TEXT_WIDTH);
            ui.spacing_mut().item_spacing.y = 0.;
            let text = locale::tr(ui, help).replace("\\n", "\n");
            for (row, paragraph) in text.lines().enumerate() {
                if row > 0 {
                    ui.add_space(7.);
                }
                let mut job = egui::text::LayoutJob::default();
                for (index, part) in paragraph.split("**").enumerate() {
                    let bold = index % 2 == 1;
                    job.append(
                        part,
                        0.,
                        egui::TextFormat {
                            font_id: if bold {
                                theme::TextRole::TooltipHeading.font()
                            } else {
                                theme::TextRole::TooltipBody.font()
                            },
                            color: if bold { Color32::WHITE } else { HELP_TEXT },
                            ..Default::default()
                        },
                    );
                }
                ui.label(job);
            }
        });
    }
    response
}
fn help_badge(ui: &mut egui::Ui, help: &str, below: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24., 16.), Sense::hover());
    let hovered = response.hovered();
    ui.painter().rect_filled(
        rect,
        8.,
        if hovered {
            HELP_TEXT
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
    help_tooltip(
        response.on_hover_cursor(egui::CursorIcon::Help),
        help,
        below,
    )
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
        theme::TextRole::PropertyLabel.font(),
    );
    let truncated = galley.elided;
    ui.painter().galley(
        egui::pos2(rect.right() - reserve - galley.size().x, rect.top() + 4.),
        galley,
        theme::TEXT,
    );
    let help_hovered = if let Some(help) = help {
        let badge = Rect::from_min_size(
            rect.right_top() + egui::vec2(-32., 4.),
            egui::vec2(24., 16.),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(badge));
        help_badge(&mut child, help, false).hovered()
    } else {
        false
    };
    if truncated && !help_hovered {
        response.detail_tooltip(name)
    } else {
        response
    }
}

pub(in crate::app) fn context_menu_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::popup(style)
        .fill(theme::DARK)
        .stroke(Stroke::new(1_f32, theme::LIGHT))
        .corner_radius(2.)
        .inner_margin(0.)
}

pub(in crate::app) fn context_menu_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.spacing_mut().item_spacing.y = 0.;
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(20., 0.);
        let visuals = ui.visuals_mut();
        visuals.override_text_color = None;
        let widgets = &mut visuals.widgets;
        for (state, fill, text) in [
            (&mut widgets.inactive, Color32::TRANSPARENT, theme::ACCENT),
            (&mut widgets.hovered, theme::ACCENT, theme::DARK),
            (&mut widgets.active, theme::ACCENT, theme::DARK),
        ] {
            state.weak_bg_fill = fill;
            state.fg_stroke.color = text;
            state.expansion = 0.;
        }
        ui.add(
            egui::Button::new(RichText::new(locale::tr(ui, label)).size(13.333))
                .min_size(egui::vec2(0., 31.))
                .stroke(Stroke::NONE)
                .corner_radius(0.),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
    })
    .inner
}

pub(super) const FORM_OPTION_HIGHLIGHT: Color32 = Color32::from_rgb(30, 144, 255);

pub(in crate::app) fn form_combo(salt: impl std::hash::Hash) -> egui::ComboBox {
    egui::ComboBox::from_id_salt(salt).popup_style(egui::style::StyleModifier::new(|style| {
        style.spacing.menu_margin = egui::Margin::ZERO;
        let visuals = &mut style.visuals;
        visuals.window_fill = theme::LIGHT;
        visuals.window_stroke = Stroke::new(1_f32, Color32::from_gray(118));
        visuals.menu_corner_radius = 2.into();
        visuals.popup_shadow = egui::epaint::Shadow::NONE;
        visuals.override_text_color = None;
        visuals.selection.bg_fill = FORM_OPTION_HIGHLIGHT;
        visuals.selection.stroke.color = Color32::WHITE;
        visuals.widgets.inactive.fg_stroke.color = theme::ACCENT;
        for state in [&mut visuals.widgets.hovered, &mut visuals.widgets.active] {
            state.weak_bg_fill = FORM_OPTION_HIGHLIGHT;
            state.fg_stroke.color = Color32::WHITE;
        }
    }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum FormSelectAppearance {
    Normal,
    Unassigned,
    Required,
}

fn select_navigation(ui: &egui::Ui, home_and_end: bool) -> Vec<Key> {
    ui.input_mut(|input| {
        let mut keys = Vec::new();
        input.events.retain(|event| {
            let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = event
            else {
                return true;
            };
            if modifiers.matches_logically(egui::Modifiers::NONE)
                && (matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Enter)
                    || home_and_end && matches!(key, Key::Home | Key::End))
            {
                keys.push(*key);
                false
            } else {
                true
            }
        });
        keys
    })
}

fn select_highlight(keys: &[Key], mut index: usize, last: usize) -> (usize, bool) {
    for key in keys {
        index = match key {
            Key::ArrowUp => index.saturating_sub(1),
            Key::ArrowDown => (index + 1).min(last),
            Key::Home => 0,
            Key::End => last,
            Key::Enter => return (index, true),
            _ => index,
        };
    }
    (index, false)
}

/// Each option can introduce a heading before its selectable row.
pub(in crate::app) fn form_select(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + Copy,
    height: f32,
    options: &[(&str, Option<&str>)],
    current: Option<usize>,
    name: &str,
    appearance: FormSelectAppearance,
) -> Option<usize> {
    let id = ui.make_persistent_id(egui::Id::new(salt));
    let mut selected = None;
    let highlighted_id = id.with("highlighted");
    let open = egui::ComboBox::is_open(ui.ctx(), id);
    let mut highlighted = current.unwrap_or(0);
    let mut keyboard_moved = false;
    if open {
        highlighted = ui
            .data(|data| data.get_temp(highlighted_id))
            .unwrap_or(highlighted);
        let last = options.len().saturating_sub(1);
        let keys = select_navigation(ui, true);
        let (next, enter) = select_highlight(&keys, highlighted.min(last), last);
        highlighted = next;
        keyboard_moved = keys.first().is_some_and(|key| *key != Key::Enter);
        if enter {
            selected = options.get(highlighted).map(|_| highlighted);
            egui::Popup::close_all(ui.ctx());
        }
    }
    let previous_style = ui.style().clone();
    let font = theme::TextRole::FormValue.font();
    let text_height = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let spacing = ui.spacing_mut();
    spacing.interact_size.y = height;
    spacing.button_padding = egui::vec2(9., (height - text_height.max(spacing.icon_width)) * 0.5);
    let inactive = !open && !ui.memory(|memory| memory.has_focus(id));
    let invalid = inactive && appearance == FormSelectAppearance::Required;
    let striped = inactive && appearance == FormSelectAppearance::Unassigned;
    if striped {
        let rect = Rect::from_min_size(
            ui.available_rect_before_wrap().min,
            egui::vec2(ui.available_width(), height),
        );
        ui.painter().rect_filled(rect, 3., theme::LIGHT);
        let interior = rect.shrink(1.);
        egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .uv(Rect::from_min_size(egui::Pos2::ZERO, interior.size() / 8.))
            .paint_at(ui, interior);
    }
    let widgets = &mut ui.visuals_mut().widgets;
    for (style, border) in [
        (&mut widgets.inactive, Color32::from_rgb(115, 127, 157)),
        (&mut widgets.hovered, Color32::WHITE),
        (&mut widgets.active, Color32::WHITE),
        (&mut widgets.open, Color32::WHITE),
    ] {
        style.weak_bg_fill = if invalid {
            Color32::from_rgb(121, 17, 5)
        } else if striped {
            Color32::TRANSPARENT
        } else {
            theme::LIGHT
        };
        style.bg_stroke = Stroke::new(
            1_f32,
            if invalid {
                Color32::from_rgb(227, 77, 0)
            } else {
                border
            },
        );
        style.fg_stroke.color = Color32::WHITE;
        style.expansion = 0.;
        style.corner_radius = 3.into();
    }
    const ROW_HEIGHT: f32 = 17.;
    let rows = options.len()
        + options
            .iter()
            .filter(|(_, heading)| heading.is_some())
            .count();
    let screen = ui.ctx().content_rect();
    let anchor = ui.next_widget_position().y;
    let available = (anchor - screen.top()).max(screen.bottom() - anchor - height) - 2.;
    let popup_height = (rows as f32 * ROW_HEIGHT).clamp(ROW_HEIGHT, available.max(ROW_HEIGHT));
    form_combo(salt)
        .width(ui.available_width())
        .height(popup_height)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .truncate()
        .icon(|ui, rect, visuals, _| {
            let center = rect.center();
            ui.painter().add(egui::Shape::line(
                vec![
                    center + egui::vec2(-3., -1.5),
                    center + egui::vec2(0., 1.5),
                    center + egui::vec2(3., -1.5),
                ],
                Stroke::new(1.5_f32, visuals.text_color()),
            ));
        })
        .selected_text(RichText::new(name).font(font.clone()))
        .show_ui(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let start = ui.next_widget_position();
            if let Some(pointer) = ui.input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .filter(|_| input.pointer.is_moving())
            }) && ui.clip_rect().contains(pointer)
                && pointer.y >= start.y
            {
                let row = ((pointer.y - start.y) / ROW_HEIGHT) as usize;
                let mut position = 0;
                if let Some(index) = options.iter().position(|(_, heading)| {
                    position += 1 + usize::from(heading.is_some());
                    position - 1 == row
                }) {
                    highlighted = index;
                }
            }
            // Size the open menu to its labels independently of the closed field.
            let mut width = ui.available_width();
            let mut indent = 9.;
            let rows: Vec<_> = options
                .iter()
                .map(|&(name, heading)| {
                    let heading = heading.map(|text| {
                        indent = 30.;
                        let font = theme::TextRole::OptionTitle.font();
                        let galley =
                            ui.painter()
                                .layout_no_wrap(text.to_owned(), font, Color32::WHITE);
                        width = width.max(11. + galley.size().x + 9.);
                        galley
                    });
                    let label = ui.painter().layout_no_wrap(
                        name.to_owned(),
                        font.clone(),
                        Color32::PLACEHOLDER,
                    );
                    width = width.max(indent + label.size().x + 9.);
                    (name, heading, label, indent)
                })
                .collect();
            ui.set_min_width(width.min(screen.width() - 2.));
            for (index, (name, heading, label, indent)) in rows.into_iter().enumerate() {
                if let Some(heading) = heading {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        Sense::hover(),
                    );
                    ui.painter()
                        .with_clip_rect(rect.intersect(ui.clip_rect()))
                        .galley(
                            rect.left_center() + egui::vec2(11., -heading.size().y * 0.5),
                            heading,
                            Color32::WHITE,
                        );
                }
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ROW_HEIGHT),
                    Sense::click(),
                );
                let active = index == highlighted;
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        ui.is_enabled(),
                        active,
                        name,
                    )
                });
                let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                if active {
                    painter.rect_filled(rect, 0., FORM_OPTION_HIGHLIGHT);
                    if keyboard_moved || !open {
                        response.scroll_to_me(None);
                    }
                }
                let truncated = indent + label.size().x > rect.width();
                painter.galley(
                    rect.left_center() + egui::vec2(indent, -label.size().y * 0.5),
                    label,
                    if active {
                        Color32::WHITE
                    } else {
                        theme::ACCENT
                    },
                );
                if response.clicked() {
                    selected = Some(index);
                    ui.close();
                } else if truncated {
                    response.detail_tooltip(name);
                }
            }
            ui.data_mut(|data| data.insert_temp(highlighted_id, highlighted));
        });
    ui.set_style(previous_style);
    selected
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TilesetChoice {
    Unassigned,
    Existing(i64),
    EmbeddedIcons,
}

pub(super) fn grouped_tileset_choices<'a>(
    ui: &egui::Ui,
    tilesets: &'a [Value],
) -> Vec<(TilesetChoice, (&'a str, Option<&'a str>))> {
    let internal_icons = locale::tr(ui, "⚙ Internal icons");
    let mut groups = std::collections::BTreeMap::<Option<&str>, Vec<(TilesetChoice, &str)>>::new();
    for tileset in tilesets {
        let Some(uid) = tileset["uid"].as_i64() else {
            continue;
        };
        let label = if tileset["embedAtlas"] == "LdtkIcons" {
            internal_icons
        } else {
            text(tileset, "identifier", locale::tr(ui, "Tileset"))
        };
        let choice = (TilesetChoice::Existing(uid), label);
        let mut tags = tileset["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .peekable();
        if tags.peek().is_none() {
            groups.entry(None).or_default().push(choice);
        }
        for tag in tags {
            let group = groups.entry(Some(tag)).or_default();
            if group.last() != Some(&choice) {
                group.push(choice);
            }
        }
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_cached_key(|(tag, _)| tag.map(str::to_lowercase));
    let single_group = groups.len() == 1;
    let mut candidates = Vec::new();
    for (tag, choices) in groups {
        let heading = if single_group {
            locale::tr(ui, "All tilesets")
        } else {
            tag.unwrap_or_else(|| locale::tr(ui, "Untagged"))
        };
        candidates.extend(
            choices
                .into_iter()
                .enumerate()
                .map(|(index, (choice, label))| (choice, (label, (index == 0).then_some(heading)))),
        );
    }
    if !tilesets
        .iter()
        .any(|tileset| tileset["embedAtlas"] == "LdtkIcons")
    {
        candidates.push((
            TilesetChoice::EmbeddedIcons,
            (
                internal_icons,
                Some(locale::tr(ui, "Integrated LDtk tilesets")),
            ),
        ));
    }
    candidates
}

pub(super) fn tileset_picker(
    ui: &mut egui::Ui,
    width: f32,
    tilesets: &[Value],
    current: Option<i64>,
    placeholder: &str,
    required: bool,
) -> Option<TilesetChoice> {
    let salt = ("form-tileset", placeholder);
    let placeholder = locale::tr(ui, placeholder);
    let mut candidates = vec![(TilesetChoice::Unassigned, (placeholder, None))];
    candidates.extend(grouped_tileset_choices(ui, tilesets));
    let selection = current.map_or(TilesetChoice::Unassigned, TilesetChoice::Existing);
    let index = candidates
        .iter()
        .position(|(choice, _)| *choice == selection);
    let options: Vec<_> = candidates.iter().map(|(_, label)| *label).collect();
    let title = index.map_or(placeholder, |i| options[i].0);
    let appearance = if required {
        FormSelectAppearance::Required
    } else if current.is_none() {
        FormSelectAppearance::Unassigned
    } else {
        FormSelectAppearance::Normal
    };
    let width = width.max(40.);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 29.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_max_width(width);
            form_select(ui, salt, 29., &options, index, title, appearance)
        },
    )
    .inner
    .map(|index| candidates[index].0)
    .filter(|choice| *choice != selection)
}

pub(in crate::app) fn select_popup(
    response: &egui::Response,
    id: egui::Id,
    selected: usize,
    options: &[(&str, Color32)],
) -> Option<usize> {
    select_popup_contents(response, id, selected, options, false).map(|(index, _)| index)
}

pub(super) fn searchable_select_button(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, response.enabled(), title)
    });
    ui.painter().rect(
        rect,
        2.,
        theme::DARK,
        Stroke::new(
            1_f32,
            if response.hovered() {
                theme::ACCENT
            } else {
                theme::LIGHT
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter()
        .with_clip_rect(rect.with_max_x(rect.right() - 18.))
        .text(
            rect.left_center() + egui::vec2(5., 0.),
            Align2::LEFT_CENTER,
            title,
            theme::TextRole::FormValue.font(),
            ui.visuals().text_color(),
        );
    let arrow = rect.right_center() - egui::vec2(8., 0.);
    ui.painter().add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-3., -1.5),
            arrow + egui::vec2(0., 1.5),
            arrow + egui::vec2(3., -1.5),
        ],
        Stroke::new(1_f32, ui.visuals().text_color()),
    ));
    response
}

pub(in crate::app) fn searchable_select_popup(
    response: &egui::Response,
    id: egui::Id,
    selected: usize,
    options: &[(&str, Color32)],
) -> Option<(usize, bool)> {
    select_popup_contents(response, id, selected, options, true)
}

fn select_popup_contents(
    response: &egui::Response,
    id: egui::Id,
    selected: usize,
    options: &[(&str, Color32)],
    searchable: bool,
) -> Option<(usize, bool)> {
    let last_option = options.len().checked_sub(1)?;
    let ctx = &response.ctx;
    if !response.enabled() {
        egui::Popup::close_id(ctx, id);
        return None;
    }
    let open = egui::Popup::is_id_open(ctx, id);
    let cursor_id = id.with("highlighted");
    let search_id = id.with("search");
    if !open {
        ctx.data_mut(|data| {
            data.remove::<usize>(cursor_id);
            data.remove::<String>(search_id);
        });
    }
    let mut choice = None;
    let popup = egui::Popup::menu(response)
        .id(id)
        .open_memory(
            ((!open && response.clicked()) || response.clicked_by(egui::PointerButton::Primary))
                .then_some(egui::SetOpenCommand::Toggle),
        )
        .width(response.rect.width())
        .frame(egui::Frame::new().fill(theme::LIGHT));
    let popup = if searchable {
        popup
            .anchor(response.rect.left_top())
            .frame(context_menu_frame(&ctx.style()).fill(theme::LIGHT))
    } else {
        popup
    };
    popup.show(|ui| {
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

        let navigation = select_navigation(ui, !searchable);
        let mut search = String::new();
        if searchable {
            search = ui
                .data(|data| data.get_temp::<String>(search_id))
                .unwrap_or_default();
            let edit = ui.add_sized(
                [width, 34.],
                egui::TextEdit::singleline(&mut search)
                    .id(search_id)
                    .hint_text(locale::tr(ui, "Search"))
                    .font(theme::TextRole::FormValue.font())
                    .margin(egui::Margin::symmetric(9, 0))
                    .vertical_align(egui::Align::Center),
            );
            if !open {
                edit.request_focus();
            }
            ui.data_mut(|data| data.insert_temp(search_id, search.clone()));
        }
        let search = search.trim().to_lowercase();
        let visible = options
            .iter()
            .enumerate()
            .filter_map(|(index, (label, _))| {
                let label = label.to_lowercase();
                let mut label_chars = label.chars();
                search
                    .chars()
                    .all(|needle| label_chars.by_ref().any(|ch| ch == needle))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        let Some(last) = visible.len().checked_sub(1) else {
            return;
        };
        let previous = ui
            .data(|data| data.get_temp::<usize>(cursor_id))
            .unwrap_or(selected)
            .min(last_option);
        let index = visible
            .iter()
            .position(|&index| index == previous)
            .unwrap_or(0);
        let (index, confirm) = select_highlight(&navigation, index, last);
        let mut cursor = visible[index];
        if confirm {
            choice = Some((cursor, true));
            ui.close();
        }
        egui::ScrollArea::vertical()
            .max_height(250.)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                for index in visible {
                    let (label, color) = options[index];
                    let (rect, option) = ui.allocate_exact_size(
                        egui::vec2(width, if searchable { 27. } else { 32. }),
                        Sense::click(),
                    );
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
                        choice = Some((index, false));
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

pub(in crate::app) fn copy_iid_button(ui: &mut egui::Ui, iid: &str, help: &str, size: Vec2) {
    let icon = egui::Image::new(egui::include_image!("../../assets/icons/copy.svg"))
        .fit_to_exact_size(Vec2::splat(20.));
    let label = locale::tr(ui, help);
    let response = add_sized_button(ui, size, egui::Button::new(icon).fill(theme::LIGHT))
        .action_tooltip(label);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    if response.clicked() {
        ui.ctx().copy_text(iid.to_owned());
    }
}

pub(in crate::app) fn underlined_link(label: egui::Label) -> impl egui::Widget {
    move |ui: &mut egui::Ui| {
        let (pos, galley, response) = label.sense(Sense::click()).layout_in_ui(ui);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Link, ui.is_enabled(), galley.text())
        });
        if ui.is_rect_visible(response.rect) {
            let underline = Stroke::new(1_f32, theme::ACCENT);
            if ui.style().interaction.selectable_labels {
                egui::text_selection::LabelSelectionState::label_text_selection(
                    ui,
                    &response,
                    pos,
                    galley,
                    theme::ACCENT,
                    underline,
                );
            } else {
                ui.painter().add(
                    egui::epaint::TextShape::new(pos, galley, theme::ACCENT)
                        .with_underline(underline),
                );
            }
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

pub(in crate::app) fn hyperlink(ui: &mut egui::Ui, label: &str, url: &str) -> egui::Response {
    let label = egui::Label::new(locale::tr(ui, label)).wrap();
    let response = ui.add(underlined_link(label));
    if response.clicked_with_open_in_background() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    } else if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::same_tab(url));
    }
    if ui.style().url_in_tooltip {
        response.detail_tooltip(url)
    } else {
        response
    }
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
        .detail_tooltip(url);
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
pub(in crate::app) const CHECKBOX_WIDTH: f32 = 24.;

pub(in crate::app) fn checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    let response = literal_checkbox(ui, value, locale::tr(ui, label));
    if let Some(help) = help_text(label, label) {
        help_tooltip(response, help, false)
    } else {
        response
    }
}

/// Use for project data such as enum identifiers, which may also be translation keys.
pub(in crate::app) fn literal_checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: &str,
) -> egui::Response {
    let size = Vec2::splat(CHECKBOX_WIDTH);
    let icon = egui::Id::new("check-box");
    let layout = egui::AtomLayout::new((egui::Atom::custom(icon, size), label))
        .sense(Sense::click())
        .min_size(Vec2::splat(ui.spacing().interact_size.y))
        .fallback_text_color(ui.visuals().strong_text_color())
        .show(ui);
    let rect = layout.rect(icon);
    let mut response = layout.response;
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    if let Some(rect) = rect {
        let blue = Color32::from_rgb(0, 117, 255);
        let (fill, edge) = if *value {
            (blue, blue)
        } else {
            (Color32::WHITE, Color32::GRAY)
        };
        ui.painter()
            .rect(rect, 2., fill, Stroke::new(1_f32, edge), StrokeKind::Inside);
        if *value {
            paint_checkmark(ui.painter(), rect, Color32::WHITE);
        }
        if !response.enabled() {
            disabled_hatch(ui, rect);
        } else if response.hovered() || response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                2.,
                Stroke::new(1_f32, theme::ACCENT),
                StrokeKind::Outside,
            );
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(in crate::app) fn paint_checkmark(painter: &egui::Painter, rect: Rect, ink: Color32) {
    let point = |x, y| rect.min + egui::vec2(x, y) * (rect.width() / CHECKBOX_WIDTH);
    painter.add(egui::Shape::line(
        vec![point(4., 11.5), point(9.75, 17.25), point(20., 4.75)],
        Stroke::new(rect.width() * 0.15, ink),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_form_labels_center_on_the_control_at_different_widths() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let label = "Display the selected value in the editor";
        for width in [310., 90.] {
            let mut control = Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    form_row_wrapped_sized(
                        ui,
                        label,
                        "editorDisplayMode",
                        egui::vec2(width, 32.),
                        |ui| {
                            control = form_combo("display")
                                .selected_text("Replace entity tile")
                                .show_ui(ui, |_| {})
                                .response
                                .rect;
                        },
                    );
                });
            });
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(t) if t.galley.text().replace('\n', " ") == label => Some(t),
                    _ => None,
                })
                .unwrap();
            if width < 100. {
                assert!(text.galley.rows.len() >= 3);
            } else {
                assert_eq!(text.galley.rows.len(), 1);
            }
            let bounds = text.galley.rect.translate(text.pos.to_vec2());
            assert!(
                (bounds.center().y - control.center().y).abs() <= 1.,
                "{bounds:?} vs {control:?}"
            );
        }
    }

    #[test]
    fn checkboxes_translate_tool_labels_and_keep_project_values_literal() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        let mut literal_value = false;
        let mut render = |input, focus: bool| {
            ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    checkbox(ui, &mut false, "Array");
                    let response = literal_checkbox(ui, &mut literal_value, "Array");
                    if focus {
                        response.request_focus();
                    }
                });
            })
        };
        let output = render(egui::RawInput::default(), true);
        for expected in ["数组", "Array"] {
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == expected)
            }), "missing checkbox label {expected}");
        }
        let _ = render(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Space,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            false,
        );
        assert!(literal_value, "Space must toggle the focused checkbox");
    }

    #[test]
    fn truncated_project_field_name_has_a_shared_tooltip_with_literal_text() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        ctx.style_mut(|style| {
            style.interaction.tooltip_delay = 0.;
            style.animation_time = 0.;
        });
        let name = r"**Example** field\nwith literal markers";
        let pointer = egui::pos2(45., 20.);
        let mut output = egui::FullOutput::default();
        for frame in 0..5 {
            output = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::PointerMoved(pointer)],
                    time: Some(f64::from(frame) * 0.25),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        field_instance_label(ui, name, 72., None, false, false);
                    });
                },
            );
        }
        fn contains_tooltip_background(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Rect(rect) => rect.fill == Color32::from_rgb(60, 97, 72),
                egui::Shape::Vec(shapes) => shapes.iter().any(contains_tooltip_background),
                _ => false,
            }
        }
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == name)
        }));
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| contains_tooltip_background(&shape.shape))
        );
    }

    #[test]
    fn centered_help_card_keeps_wrapped_text_centered_as_the_card_grows() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let message = "Project settings may affect compatibility with older game APIs.";
        for (width, wraps) in [(720., false), (215., true)] {
            let mut card = Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    card = ui
                        .scope(|ui| centered_help_card(ui, width, 56., message))
                        .response
                        .rect;
                });
            });
            let paragraph = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == message => Some(text),
                    _ => None,
                })
                .unwrap();
            let rect = Rect::from_min_size(paragraph.pos, paragraph.galley.size());
            assert_eq!(paragraph.galley.rows.len() > 1, wraps);
            assert_eq!(card.height() > 56., wraps);
            assert!((rect.center().y - card.center().y).abs() < 0.5);
            assert!((rect.top() - card.top()) >= 11.);
            assert!((card.bottom() - rect.bottom()) >= 11.);
        }
    }

    #[test]
    fn bundled_form_help_has_simplified_chinese_translations() {
        let ctx = egui::Context::default();
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        let messages = HELP
            .as_object()
            .unwrap()
            .values()
            .chain(HELP["__info__"].as_object().unwrap().values())
            .filter_map(Value::as_str)
            .filter(|message| !message.is_empty());
        let mut missing = messages
            .filter(|message| locale::tr_ctx(&ctx, message) == *message)
            .collect::<Vec<_>>();
        missing.sort_unstable();
        missing.dedup();
        assert!(missing.is_empty(), "untranslated form help: {missing:#?}");
    }

    #[test]
    fn form_select_replays_navigation_keys_in_input_order() {
        let ctx = egui::Context::default();
        let draw = |events| {
            let mut selected = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        selected = form_select(
                            ui,
                            "test-picker",
                            29.,
                            &[("First", None), ("Second", None), ("Third", None)],
                            Some(2),
                            "Third",
                            FormSelectAppearance::Normal,
                        );
                    });
                },
            );
            selected
        };
        draw(vec![]);
        let pos = egui::pos2(40., 20.);
        for pressed in [true, false] {
            draw(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        let events = [Key::Home, Key::ArrowDown, Key::Enter]
            .into_iter()
            .map(|key| egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .collect();
        assert_eq!(draw(events), Some(1));
    }

    #[test]
    fn form_help_follows_preview_anchor_and_uses_localized_export_descriptions() {
        fn popup_rect(shape: &egui::Shape) -> Option<Rect> {
            match shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(60, 97, 72) => {
                    Some(rect.rect)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(popup_rect),
                _ => None,
            }
        }
        for (key, label, left, top, above, right_aligned) in [
            ("png", "Export as PNG", 40., 280., true, false),
            (
                "simplifiedExport",
                "Super simple export ❤",
                40.,
                280.,
                false,
                false,
            ),
            ("jsonStyle", "JSON indent style", 525., 285., true, true),
            ("jsonStyle", "JSON indent style", 40., 8., false, false),
            ("tiled", "Export Tiled TMX", 40., 280., true, false),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            if key == "tiled" {
                locale::set(&ctx, locale::Language::SimplifiedChinese);
            }
            ctx.style_mut(|style| style.interaction.tooltip_delay = 0.);
            let cell = Rect::from_min_size(egui::pos2(left, top), egui::vec2(250., 30.));
            let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 700.));
            let badge = Rect::from_center_size(
                egui::pos2(cell.right() - 12., cell.center().y),
                egui::vec2(24., 16.),
            );
            let mut output = egui::FullOutput::default();
            for index in 0..5 {
                output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events: if index == 0 {
                            vec![egui::Event::PointerMoved(badge.center())]
                        } else {
                            vec![]
                        },
                        time: Some(f64::from(index) * 0.25),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.scope_builder(egui::UiBuilder::new().max_rect(cell), |ui| {
                                property_label_with_width(ui, label, key, cell.width());
                            });
                        });
                    },
                );
            }
            let popup = output
                .shapes
                .iter()
                .find_map(|shape| popup_rect(&shape.shape))
                .unwrap_or_else(|| panic!("{key} hover did not show its help"));
            let gap = if above {
                badge.top() - popup.bottom()
            } else {
                popup.top() - badge.bottom()
            };
            let expected_gap = if key == "simplifiedExport" { 10. } else { 8. };
            assert!((gap - expected_gap).abs() <= 1., "{key} gap was {gap}");
            let offset = if right_aligned {
                popup.right() - badge.right()
            } else {
                popup.left() - badge.left()
            };
            assert!(offset.abs() <= 1., "{key} horizontal offset was {offset}");
            if key == "png" {
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::Text(text) if text.galley.job.text.contains("flattened")
                )));
            }
            if key == "tiled" {
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::Text(text) if text.galley.job.text.contains("子文件夹中生成 Tiled TMX 文件")
                )));
            }
        }
    }

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
        let visible = shape
            .galley
            .rect
            .translate(shape.pos.to_vec2())
            .intersect(clip);
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
            badge.left() >= label.pos.x + label.galley.rect.right(),
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
