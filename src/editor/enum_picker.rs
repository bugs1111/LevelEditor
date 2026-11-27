use super::*;
use crate::app::panels::source_controls;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Null,
    Default,
    Value(usize),
}

#[derive(Clone)]
struct Picker {
    id: egui::Id,
    frame: u64,
    query: String,
    focus: Option<Choice>,
    columns: usize,
    list_height: Option<f32>,
}

#[derive(Clone, Default)]
struct LayoutChange {
    project: Option<String>,
    key: String,
    columns: usize,
}

fn picker_id() -> egui::Id {
    egui::Id::new("enum-picker")
}

pub(in crate::app) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<Picker>(picker_id()))
        .is_some_and(|picker| ctx.cumulative_frame_nr() <= picker.frame + 1)
}

pub(super) fn prepare(app: &mut EditorApp, ctx: &egui::Context) {
    if let Some(change) = ctx.data_mut(|data| data.remove_temp::<LayoutChange>(picker_id())) {
        if change.columns == 1 {
            app.settings
                .delete_ui_state(&change.key, change.project.as_deref());
        } else {
            app.settings.set_ui_state_int(
                &change.key,
                change.project.as_deref(),
                change.columns as i64,
            );
        }
    }
    ctx.data_mut(|data| {
        data.insert_temp(
            picker_id().with("layouts"),
            app.settings.value["uiStates"].clone(),
        );
    });
}

pub(super) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    enumeration: &Value,
    field: &Value,
    current: &Value,
    using_default: bool,
) -> Option<Value> {
    let values = enumeration["values"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let stored_id = current.as_str().unwrap_or_default();
    let uid = enumeration["uid"].as_i64().unwrap_or(-1);
    let current_value = project.enum_value(uid, stored_id);
    let current_id = current_value.map_or(stored_id, |value| text(value, "id", stored_id));
    let null_label = locale::tr(
        ui,
        if field["canBeNull"] == true {
            "-- null --"
        } else {
            "[ Value required ]"
        },
    );
    let has_default = !field["defaultOverride"].is_null();
    let default_selected = using_default && has_default;
    let label = if default_selected {
        locale::format(ui, "{value} (default)", &[("value", current_id)])
    } else if current_id.is_empty() {
        null_label.to_owned()
    } else {
        current_id.to_owned()
    };
    let width = ui.available_width().max(20.);
    let icon = current_value.and_then(|value| enum_icon(project, enumeration, value));
    let button = egui::Button::opt_image_and_text(icon, Some(label.into()))
        .right_text("▾")
        .min_size(egui::vec2(width, 27.))
        .truncate();
    let button = ui.add(button);
    let picker = begin(&button)?;
    let show_null = field["canBeNull"] == true || current_value.is_none();
    let default_value = has_default
        .then(|| {
            let mut scalar = field.clone();
            scalar["isArray"] = json!(false);
            let instance = project.make_field_instance(&scalar).ok()?;
            project.enum_value(uid, instance["__value"].as_str()?)
        })
        .flatten();
    let default_label = default_value
        .map(|value| locale::format(ui, "{value} (default)", &[("value", text(value, "id", ""))]));
    let selected = if default_selected && default_value.is_some() {
        Some(Choice::Default)
    } else if current_value.is_none() {
        Some(Choice::Null)
    } else {
        values
            .iter()
            .position(|value| value["id"].as_str() == Some(current_id))
            .map(Choice::Value)
    };
    let has_icons = enumeration["iconTilesetUid"].is_number()
        && values.iter().any(|value| value["tileRect"].is_object());
    let labels: Vec<_> = values.iter().map(|value| text(value, "id", "")).collect();
    let picked = show_popup(
        ui,
        &button,
        picker,
        Options {
            labels: &labels,
            null_label: show_null.then_some(null_label),
            default_label: default_label.as_deref(),
            selected,
            grid: has_icons.then(|| (format!("fieldInstance_{uid}"), project.json["iid"].as_str())),
        },
        |choice| {
            let value = match choice {
                Choice::Null => None,
                Choice::Default => default_value,
                Choice::Value(index) => Some(&values[index]),
            };
            let icon = value.and_then(|value| enum_icon(project, enumeration, value));
            let tint = value
                .filter(|_| choice != Choice::Default)
                .and_then(|value| value["color"].as_u64())
                .map(|rgb| {
                    Color32::from_rgba_unmultiplied(
                        (rgb >> 16) as u8,
                        (rgb >> 8) as u8,
                        rgb as u8,
                        77,
                    )
                });
            (icon, tint)
        },
    );
    match picked {
        Some(Choice::Default) => Some(Value::Null),
        Some(Choice::Null) if !current.is_null() => Some(Value::Null),
        Some(Choice::Value(index)) if Some(Choice::Value(index)) != selected => {
            Some(values[index]["id"].clone())
        }
        _ => None,
    }
}

pub(in crate::app) fn show_text<'a>(
    ui: &mut egui::Ui,
    choices: &'a [(&'a str, &str)],
    current: &str,
) -> Option<&'a str> {
    let selected = choices.iter().position(|(value, _)| *value == current);
    let labels: Vec<_> = choices
        .iter()
        .map(|(_, label)| locale::tr(ui, label))
        .collect();
    let label = selected.map_or(current, |index| labels[index]);
    let width = ui.available_width();
    let (rect, button) = ui.allocate_exact_size(egui::vec2(width, 30.), Sense::click());
    let button = button.on_hover_cursor(egui::CursorIcon::PointingHand);
    button.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let color = if button.hovered() {
        theme::ACCENT
    } else {
        theme::TEXT
    };
    let border = if button.hovered() {
        theme::ACCENT
    } else {
        theme::LIGHT
    };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect(
        rect,
        3.,
        theme::DARK,
        Stroke::new(1_f32, border),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_center() + egui::vec2(5., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.667),
        color,
    );
    let arrow = Rect::from_min_size(
        egui::pos2(rect.right() - 17.5, rect.top() + 1.),
        egui::vec2(16.5, 28.),
    );
    painter.rect_filled(arrow, 0., theme::DARK.gamma_multiply(0.67));
    egui::Image::new(egui::include_image!("../../assets/icons/expanded.svg")).paint_at(
        ui,
        Rect::from_center_size(arrow.center(), Vec2::splat(16.5)),
    );
    let picker = begin(&button)?;
    let picked = show_popup(
        ui,
        &button,
        picker,
        Options {
            labels: &labels,
            null_label: None,
            default_label: None,
            selected: selected.map(Choice::Value),
            grid: None,
        },
        |_| (None, None),
    );
    match picked {
        Some(Choice::Value(index)) if Some(index) != selected => Some(choices[index].0),
        _ => None,
    }
}

struct Options<'a> {
    labels: &'a [&'a str],
    null_label: Option<&'a str>,
    default_label: Option<&'a str>,
    selected: Option<Choice>,
    grid: Option<(String, Option<&'a str>)>,
}

impl Options<'_> {
    fn label(&self, choice: Choice) -> &str {
        match choice {
            Choice::Null => self.null_label.unwrap_or_default(),
            Choice::Default => self.default_label.unwrap_or_default(),
            Choice::Value(index) => self.labels[index],
        }
    }
}

const PICKER_BORDER: Color32 = Color32::from_rgb(115, 127, 157);

fn begin(button: &egui::Response) -> Option<Picker> {
    let ctx = &button.ctx;
    let id = button.id.with("enum-picker");
    let frame = ctx.cumulative_frame_nr();
    let stored = ctx.data(|data| data.get_temp::<Picker>(picker_id()));
    if let Some(mut picker) = stored.filter(|picker| picker.id == id && frame <= picker.frame + 1) {
        picker.frame = frame;
        Some(picker)
    } else {
        button.clicked().then(|| Picker {
            id,
            frame,
            query: String::new(),
            focus: None,
            columns: 1,
            list_height: None,
        })
    }
}

fn show_popup(
    ui: &egui::Ui,
    button: &egui::Response,
    mut picker: Picker,
    options: Options<'_>,
    visuals: impl Fn(Choice) -> (Option<egui::Image<'static>>, Option<Color32>),
) -> Option<Choice> {
    let ctx = ui.ctx();
    let id = picker.id;
    let opening = button.clicked();
    let has_icons = options.grid.is_some();
    if opening {
        picker.focus = options.selected;
        if let Some((key, project_iid)) = options.grid.as_ref() {
            let layout = ctx.data(|data| data.get_temp::<Value>(picker_id().with("layouts")));
            if let Some(layout) = layout {
                let settings = super::super::settings::Settings {
                    value: json!({"uiStates":layout}),
                    path: None,
                };
                picker.columns = settings.ui_state_int(key, *project_iid, 1).clamp(1, 10) as usize;
            }
        }
    }
    let screen = ctx.content_rect().shrink(8.);
    let query = picker.query.trim().to_lowercase();
    let mut width = button.rect.width() - 4.;
    if !has_icons {
        for label in options
            .labels
            .iter()
            .copied()
            .filter(|label| matches_query(label, &query))
        {
            let text_width = ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(
                        label.to_owned(),
                        FontId::proportional(14.667),
                        Color32::WHITE,
                    )
                    .size()
                    .x
            });
            width = width.max(text_width + 10.);
        }
    }
    let width = width
        .max(picker.columns as f32 * 64.)
        .min(screen.width() - 4.);
    let max_list_height = (ctx.content_rect().height() * 0.8).min(screen.height() - 36.);
    let list_height = picker.list_height.unwrap_or_else(|| {
        let row_count = options.labels.len().div_ceil(picker.columns) as f32;
        let row_height = if picker.columns > 1 {
            72.
        } else if has_icons {
            42.
        } else {
            27.
        };
        row_count * row_height
            + if options.null_label.is_some() {
                27.
            } else {
                0.
            }
            + if options.default_label.is_some() {
                row_height
            } else {
                0.
            }
    });
    let popup_height = list_height.min(max_list_height) + 38.;
    let position = egui::pos2(
        button.rect.left().clamp(
            screen.left(),
            (screen.right() - width - 4.).max(screen.left()),
        ),
        button.rect.top().clamp(
            screen.top(),
            (screen.bottom() - popup_height).max(screen.top()),
        ),
    );
    let search_id = id.with("search");
    let area = egui::Area::new(id)
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .sense(Sense::hover())
        .fixed_pos(position)
        .constrain(false)
        .default_width(width)
        .fade_in(false);
    let response = egui::Modal::new(id)
        .area(area)
        .backdrop_color(Color32::TRANSPARENT)
        .frame(
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1_f32, PICKER_BORDER))
                .inner_margin(1)
                .corner_radius(3),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::splat(1.);
            let mut recenter = opening;
            let layout_popup = id.with("layout-popup");
            let layout_open = egui::Popup::is_id_open(ctx, layout_popup);
            let keyboard =
                !layout_open && ui.memory(|memory| memory.top_modal_layer() == Some(ui.layer_id()));
            let movement = if keyboard {
                move_key(ui, picker.columns)
            } else {
                None
            };
            let enter = keyboard
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
            // Leave Escape for the modal even while the search input has focus.
            let escape = keyboard
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
            ui.horizontal(|ui| {
                let input = ui
                    .scope(|ui| {
                        let widgets = &mut ui.visuals_mut().widgets;
                        for style in [
                            &mut widgets.inactive,
                            &mut widgets.hovered,
                            &mut widgets.active,
                        ] {
                            style.corner_radius = 0.into();
                            style.expansion = 0.;
                            style.bg_stroke = Stroke::new(1_f32, theme::LIGHT);
                        }
                        let width = (width - if has_icons { 33. } else { 0. }).max(1.);
                        ui.add(
                            egui::TextEdit::singleline(&mut picker.query)
                                .id(search_id)
                                .return_key(None)
                                .hint_text(
                                    egui::RichText::new(locale::tr(ui, "Search"))
                                        .color(Color32::from_gray(117)),
                                )
                                .font(theme::TextRole::FormValue.font())
                                .text_color(Color32::WHITE)
                                .background_color(theme::DARK)
                                .margin(egui::vec2(8., 8.))
                                .min_size(egui::vec2(width, 33.))
                                .desired_width(width),
                        )
                    })
                    .inner;
                recenter |= input.changed();
                if keyboard && !escape && !enter && !input.has_focus() {
                    input.request_focus();
                }
                if let Some((key, project_iid)) = options.grid.as_ref() {
                    let icon = if picker.columns == 1 {
                        egui::include_image!("../../assets/icons/listView.svg")
                    } else {
                        egui::include_image!("../../assets/icons/gridView.svg")
                    };
                    let button = ui.add_sized(
                        [32., 28.],
                        egui::Button::image(
                            egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.)),
                        )
                        .frame(false),
                    );
                    if let Some(columns) =
                        source_controls::select_picker_columns_menu(&button, layout_popup)
                    {
                        picker.columns = columns;
                        recenter = true;
                        input.request_focus();
                        ctx.data_mut(|data| {
                            data.insert_temp(
                                picker_id(),
                                LayoutChange {
                                    project: project_iid.map(str::to_owned),
                                    key: key.clone(),
                                    columns,
                                },
                            )
                        });
                    }
                }
            });

            let query = picker.query.trim().to_lowercase();
            let mut choices = Vec::with_capacity(options.labels.len() + 2);
            if options
                .null_label
                .is_some_and(|label| matches_query(label, &query))
            {
                choices.push(Choice::Null);
            }
            if options
                .default_label
                .is_some_and(|label| matches_query(label, &query))
            {
                choices.push(Choice::Default);
            }
            choices.extend(
                options
                    .labels
                    .iter()
                    .enumerate()
                    .filter_map(|(index, label)| {
                        matches_query(label, &query).then_some(Choice::Value(index))
                    }),
            );
            let columns = if has_icons { picker.columns } else { 1 };
            let rows = rows(&choices, columns, has_icons);
            let height = rows.last().map_or(0., Row::bottom);
            if picker.list_height != Some(height) {
                picker.list_height = Some(height);
                ctx.request_repaint();
            }
            let focus = picker
                .focus
                .and_then(|focus| choices.iter().position(|choice| *choice == focus));
            if !choices.is_empty() {
                let mut index = focus.unwrap_or(0);
                if let Some((delta, same_row)) = movement {
                    let next = index.saturating_add_signed(delta).min(choices.len() - 1);
                    if !same_row || row_at(&rows, index) == row_at(&rows, next) {
                        index = next;
                    }
                    recenter = true;
                }
                picker.focus = Some(choices[index]);
            } else {
                picker.focus = None;
            }

            let mut picked = if enter { picker.focus } else { None };
            if height > 0. {
                let viewport_height = height.min(max_list_height);
                ui.spacing_mut().item_spacing.y = 0.;
                ui.spacing_mut().scroll = egui::style::ScrollStyle {
                    bar_width: 10.,
                    bar_inner_margin: 0.,
                    bar_outer_margin: 0.,
                    ..egui::style::ScrollStyle::solid()
                };
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("options")
                    .max_height(viewport_height)
                    .auto_shrink([false, true]);
                if recenter
                    && let Some(index) = choices
                        .iter()
                        .position(|choice| Some(*choice) == picker.focus)
                {
                    let row = &rows[row_at(&rows, index)];
                    scroll = scroll.vertical_scroll_offset(
                        (row.top - viewport_height * 0.5).clamp(0., height - viewport_height),
                    );
                }
                let pointer_moved = ui.input(|input| input.pointer.is_moving());
                scroll.show_viewport(ui, |ui, viewport| {
                    let first = rows.partition_point(|row| row.bottom() < viewport.min.y);
                    let end = rows.partition_point(|row| row.top <= viewport.max.y);
                    ui.add_space(rows.get(first).map_or(height, |row| row.top));
                    for row in &rows[first..end] {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), row.height),
                            Sense::hover(),
                        );
                        let span =
                            row.choices.len() == 1 && choices[row.choices.start] == Choice::Default;
                        let cell_width = if span {
                            rect.width()
                        } else {
                            rect.width() / columns as f32
                        };
                        for (col, index) in row.choices.clone().enumerate() {
                            let choice = choices[index];
                            let rect = Rect::from_min_size(
                                rect.min + egui::vec2(col as f32 * cell_width, 0.),
                                egui::vec2(cell_width, rect.height()),
                            );
                            let response = ui
                                .interact(rect, id.with(index), Sense::click())
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if pointer_moved && response.hovered() {
                                picker.focus = Some(choice);
                            }
                            if response.clicked() {
                                picked = Some(choice);
                            }
                            let label = options.label(choice);
                            let (icon, tint) = visuals(choice);
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    label,
                                )
                            });
                            paint_choice(
                                ui,
                                rect,
                                label,
                                icon,
                                tint,
                                choice,
                                has_icons,
                                columns > 1,
                                Some(choice) == picker.focus,
                                Some(choice) == options.selected,
                            );
                        }
                    }
                    ui.add_space(height - rows.get(end).map_or(height, |row| row.top));
                });
            }
            (picked, escape)
        });

    let (picked, escape) = response.inner;
    if picked.is_some() || escape || response.should_close() {
        ctx.memory_mut(|memory| memory.surrender_focus(search_id));
        ctx.data_mut(|data| data.remove::<Picker>(picker_id()));
        ctx.request_repaint();
    } else {
        ctx.data_mut(|data| data.insert_temp(picker_id(), picker));
    }
    picked
}

fn matches_query(label: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut haystack = label.trim().chars().flat_map(char::to_lowercase);
    query.chars().all(|needle| haystack.any(|ch| ch == needle))
}

fn move_key(ui: &egui::Ui, columns: usize) -> Option<(isize, bool)> {
    let columns = columns as isize;
    ui.input_mut(|input| {
        [
            (Key::ArrowUp, -columns, false),
            (Key::ArrowDown, columns, false),
            (Key::PageUp, -3 * columns, false),
            (Key::PageDown, 3 * columns, false),
            (Key::ArrowLeft, -1, true),
            (Key::ArrowRight, 1, true),
        ]
        .into_iter()
        .find_map(|(key, delta, same_row)| {
            ((!same_row || columns > 1) && input.consume_key(egui::Modifiers::NONE, key))
                .then_some((delta, same_row))
        })
    })
}

struct Row {
    choices: Range<usize>,
    top: f32,
    height: f32,
}

impl Row {
    fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

fn row_at(rows: &[Row], choice: usize) -> usize {
    rows.partition_point(|row| row.choices.end <= choice)
}

fn rows(choices: &[Choice], columns: usize, has_icons: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut top = 0.;
    while start < choices.len() {
        let end = (start + columns).min(choices.len());
        let end = match choices[start..end]
            .iter()
            .position(|choice| *choice == Choice::Default)
        {
            Some(0) => start + 1,
            Some(offset) => start + offset,
            None => end,
        };
        let height = match &choices[start..end] {
            [Choice::Null] => 27.,
            [Choice::Default] if columns > 1 => 34.,
            _ if columns > 1 => 72.,
            _ if has_icons => 42.,
            _ => 27.,
        };
        rows.push(Row {
            choices: start..end,
            top,
            height,
        });
        top += height;
        start = end;
    }
    rows
}

#[allow(clippy::too_many_arguments)]
fn paint_choice(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    icon: Option<egui::Image<'static>>,
    tint: Option<Color32>,
    choice: Choice,
    has_icons: bool,
    grid: bool,
    focused: bool,
    selected: bool,
) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0., theme::LIGHT);
    if choice == Choice::Null {
        stripes(ui, rect);
    }
    if let Some(tint) = tint {
        painter.rect_filled(rect, 0., tint);
    }
    painter.hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(1_f32, PICKER_BORDER),
    );
    if grid {
        painter.vline(rect.left(), rect.y_range(), Stroke::new(1_f32, theme::DARK));
    }
    if focused {
        painter.rect_filled(rect, 0., theme::ACCENT);
        if !selected {
            painter.rect_stroke(
                rect.shrink(1.),
                0.,
                Stroke::new(1_f32, Color32::from_rgb(255, 255, 68)),
                egui::StrokeKind::Inside,
            );
        }
    }
    if selected {
        let border = rect.shrink(2.);
        painter.extend(egui::Shape::dashed_line(
            &[
                border.left_top(),
                border.right_top(),
                border.right_bottom(),
                border.left_bottom(),
                border.left_top(),
            ],
            Stroke::new(if has_icons { 2. } else { 1_f32 }, Color32::WHITE),
            4.,
            3.,
        ));
    }
    let is_default = choice == Choice::Default;
    let has_picture = has_icons && choice != Choice::Null;
    let vertical = grid && !is_default;
    let image_size = if vertical {
        48.
    } else if grid {
        24.
    } else {
        32.
    };
    let font_size = if vertical {
        11.
    } else if is_default {
        12.
    } else {
        14.667
    };
    let color = if focused {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    let mut label = egui::RichText::new(label).size(font_size).color(color);
    if is_default {
        label = label.italics();
    }
    let label = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let leading = if has_picture {
        image_size + if is_default { 4. } else { 8. }
    } else {
        0.
    };
    let row_left = if is_default {
        rect.center().x - (leading + label.size().x) / 2.
    } else {
        rect.left() + 5.
    };
    if has_picture {
        let center = if vertical {
            egui::pos2(rect.center().x, rect.top() + 5. + image_size / 2.)
        } else {
            egui::pos2(row_left + image_size / 2., rect.center().y)
        };
        let icon_rect = Rect::from_center_size(center, Vec2::splat(image_size)).intersect(rect);
        if let Some(icon) = icon {
            icon.paint_at(ui, icon_rect);
        } else {
            stripes(ui, icon_rect);
        }
    }
    let label_pos = if vertical {
        rect.center_bottom() - egui::vec2(label.size().x / 2., label.size().y + 5.)
    } else {
        egui::pos2(row_left + leading, rect.center().y - label.size().y / 2.)
    };
    painter.galley(label_pos, label, color);
}

fn stripes(ui: &egui::Ui, rect: Rect) {
    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
        .paint_at(ui, rect);
}
