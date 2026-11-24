//! Shared editor for the naming patterns in Project settings.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Variable(&'static str, &'static str, &'static str);

pub(super) const LEVEL_DEFAULT: &str = "Level_%idx";
pub(super) const LEVEL: &[Variable] = &[
    Variable("world", "WorldId", ""),
    Variable("idx1", "LevelIndex(1)", "Level index (starting at 1)"),
    Variable("idx", "LevelIndex(0)", "Level index (starting at 0)"),
    Variable("x", "LevelX", "X coordinate of the level"),
    Variable("y", "LevelY", "Y coordinate of the level"),
    Variable("gx", "GridX", "X grid coordinate of the level"),
    Variable("gy", "GridY", "Y grid coordinate of the level"),
    Variable("depth", "WorldDepth", "Level depth in the world"),
];
pub(super) const PNG: &[Variable] = &[
    Variable("world", "WorldName", ""),
    Variable("level_name", "LevelName", ""),
    Variable("level_idx", "LevelIdx", ""),
    Variable("layer_name", "LayerName", ""),
    Variable("layer_idx", "LayerIdx", ""),
];

enum Block {
    Literal(String),
    Var(Variable),
}

fn parse_blocks(mut input: &str, variables: &[Variable]) -> Vec<Block> {
    let mut output = Vec::new();
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix('%')
            && let Some(variable) = variables.iter().find(|v| rest.starts_with(v.0))
        {
            if !matches!(output.last(), Some(Block::Literal(_))) {
                output.push(Block::Literal(String::new()));
            }
            output.push(Block::Var(*variable));
            input = &rest[variable.0.len()..];
        } else {
            // An unknown percent expression stays literal until the user edits the pattern.
            let end = input.as_bytes()[1..]
                .iter()
                .position(|b| *b == b'%')
                .map_or(input.len(), |idx| idx + 1);
            output.push(Block::Literal(input[..end].into()));
            input = &input[end..];
        }
    }
    if !matches!(output.last(), Some(Block::Literal(_))) {
        output.push(Block::Literal(String::new()));
    }
    output
}

fn encoded_len(block: &Block) -> usize {
    match block {
        Block::Literal(value) => value.len(),
        Block::Var(var) => var.0.len() + 1,
    }
}

fn map_selection(old: &[Block], new: &[Block], index: usize) -> Option<usize> {
    let offset = old[..index].iter().map(encoded_len).sum::<usize>();
    let mut start = 0;
    new.iter().position(|block| {
        let end = start + encoded_len(block);
        let matches = match (&old[index], block) {
            (Block::Var(a), Block::Var(b)) => offset == start && a.0 == b.0,
            (Block::Literal(_), Block::Literal(_)) => (start..=end).contains(&offset),
            _ => false,
        };
        start = end;
        matches
    })
}

fn encode(blocks: &[Block]) -> String {
    let mut output = String::new();
    for block in blocks {
        match block {
            Block::Literal(value) => output.extend(value.chars().map(|c| {
                if " /\\.%$:?\"<>|*".contains(c) {
                    '_'
                } else {
                    c
                }
            })),
            Block::Var(var) => {
                output.push('%');
                output.push_str(var.0);
            }
        }
    }
    output
}

#[derive(Clone)]
struct Draft {
    value: String,
    focus: bool,
    cursor_at_start: bool,
}

#[derive(Clone, Default)]
struct State {
    input: String,
    selected: Option<usize>,
    draft: Option<Draft>,
    focus_variable: bool,
}

fn variable_menu(
    response: &egui::Response,
    variables: &[Variable],
    used: &[&str],
    open: Option<egui::SetOpenCommand>,
) -> Option<Variable> {
    let height = (response.ctx.content_rect().bottom() - 16. - response.rect.bottom()).max(32.);
    egui::Popup::menu(response)
        .open_memory(open)
        .align_alternatives(&[])
        .frame(source_controls::context_menu_frame(&response.ctx.style()))
        .show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let rows = variables
                .iter()
                .copied()
                .filter(|var| !used.contains(&var.0))
                .map(|var| {
                    let title = ui.painter().layout_no_wrap(
                        var.1.to_uppercase(),
                        FontId::proportional(13.33),
                        Color32::PLACEHOLDER,
                    );
                    let description = (!var.2.is_empty()).then(|| {
                        ui.painter().layout_no_wrap(
                            locale::tr(ui, var.2).into(),
                            FontId::proportional(13.33),
                            Color32::PLACEHOLDER,
                        )
                    });
                    (var, title, description)
                })
                .collect::<Vec<_>>();
            let width = 40.
                + rows
                    .iter()
                    .map(|(_, title, description)| {
                        description
                            .as_ref()
                            .map_or(title.size().x, |text| text.size().x.max(title.size().x))
                    })
                    .fold(0., f32::max);
            egui::ScrollArea::vertical()
                .max_height(height - 2.)
                .show(ui, |ui| {
                    for (var, title, description) in rows {
                        let height = if description.is_some() { 48. } else { 32. };
                        let (rect, row) =
                            ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
                        let (color, subtitle) = if row.hovered() {
                            ui.painter().rect_filled(rect, 0., theme::ACCENT);
                            (Color32::BLACK, Color32::BLACK)
                        } else {
                            (theme::ACCENT, Color32::from_rgb(122, 134, 166))
                        };
                        row.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, var.1)
                        });
                        ui.painter()
                            .galley(rect.min + egui::vec2(20., 7.), title, color);
                        if let Some(description) = description {
                            ui.painter().galley(
                                rect.min + egui::vec2(20., 24.),
                                description,
                                subtitle,
                            );
                        }
                        ui.painter().hline(
                            rect.x_range(),
                            rect.bottom() - 0.5,
                            Stroke::new(1_f32, theme::MEDIUM),
                        );
                        if row.clicked() {
                            ui.close();
                            return Some(var);
                        }
                    }
                    None
                })
                .inner
        })
        .and_then(|popup| popup.inner)
}

pub(super) struct Change {
    pub(super) value: String,
    pub(super) reset: bool,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    input: &str,
    default: &str,
    variables: &[Variable],
) -> Option<Change> {
    let id = ui.make_persistent_id(key);
    let mut state = ui
        .data(|d| d.get_temp::<State>(id))
        .filter(|state| state.input == input)
        .unwrap_or_else(|| State {
            input: input.into(),
            ..Default::default()
        });
    let mut blocks = parse_blocks(input, variables);
    let mut select = None;
    let mut edit = None;
    let mut remove = None;
    let mut replace = None;
    let mut moved = None;
    let mut add = None;
    let mut reset = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        let add_width = 44.;
        let reset_label =
            egui::WidgetText::from(RichText::new(locale::tr(ui, "[reset]")).size(12.));
        let reset_width = reset_label
            .clone()
            .into_galley(ui, None, f32::INFINITY, egui::TextStyle::Body)
            .size()
            .x;
        let actions_width = add_width + reset_width + 2. * ui.spacing().item_spacing.x;
        let width = (ui.available_width() - actions_width).max(24.);
        let used = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Var(var) => Some(var.0),
                Block::Literal(_) => None,
            })
            .collect::<Vec<_>>();
        let has_choices = variables.iter().any(|var| !used.contains(&var.0));
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width((width - 10.).max(14.));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(1., 2.);
                    ui.spacing_mut().button_padding = egui::vec2(2., 2.);
                    for (index, block) in blocks.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            let selected = state.selected == Some(index);
                            if matches!(block, Block::Literal(_))
                                && selected
                                && let Some(draft) = state.draft.as_mut()
                            {
                                let input_id = id.with(("literal", index));
                                ui_commit::prepare_editor(ui.ctx(), input_id);
                                // A cursor arrow at a literal edge moves into the neighboring token.
                                let edge = egui::TextEdit::load_state(ui.ctx(), input_id)
                                    .and_then(|s| s.cursor.char_range())
                                    .filter(|r| r.is_empty())
                                    .map(|r| r.primary.index);
                                let next = if ui.ctx().memory(|m| m.has_focus(input_id)) {
                                    ui.input_mut(|i| {
                                        if edge == Some(0)
                                            && index > 0
                                            && (i
                                                .consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                                                || i.consume_key(
                                                    egui::Modifiers::NONE,
                                                    Key::Backspace,
                                                ))
                                        {
                                            Some((index - 1, false))
                                        } else if edge == Some(draft.value.chars().count())
                                            && index + 1 < blocks.len()
                                            && (i.consume_key(
                                                egui::Modifiers::NONE,
                                                Key::ArrowRight,
                                            ) || i
                                                .consume_key(egui::Modifiers::NONE, Key::Delete))
                                        {
                                            Some((index + 1, false))
                                        } else {
                                            None
                                        }
                                    })
                                } else {
                                    None
                                };
                                let width = (draft.value.chars().count() as f32 * 8. + 22.)
                                    .clamp(32., ui.available_width().max(32.));
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut draft.value)
                                        .id(input_id)
                                        .desired_width(width)
                                        .text_color(Color32::from_rgb(235, 78, 0)),
                                );
                                if draft.focus {
                                    response.request_focus();
                                    if draft.cursor_at_start
                                        && let Some(mut text_state) =
                                            egui::TextEdit::load_state(ui.ctx(), input_id)
                                    {
                                        text_state.cursor.set_char_range(Some(
                                            egui::text::CCursorRange::one(
                                                egui::text::CCursor::new(0),
                                            ),
                                        ));
                                        text_state.store(ui.ctx(), input_id);
                                    }
                                    draft.focus = false;
                                }
                                ui_commit::track_editor(ui.ctx(), &response);
                                if response.lost_focus()
                                    || next.is_some()
                                    || response.has_focus()
                                        && ui.input(|i| {
                                            i.key_pressed(Key::Enter) || i.key_pressed(Key::Escape)
                                        })
                                {
                                    response.surrender_focus();
                                    edit = Some((index, draft.value.clone()));
                                    if next.is_some() {
                                        select = next;
                                    }
                                }
                                return;
                            }

                            let (label, background, empty) = match block {
                                Block::Literal(value) => (
                                    RichText::new(value).size(12.),
                                    theme::MEDIUM,
                                    value.is_empty(),
                                ),
                                Block::Var(var) => (
                                    RichText::new(var.1)
                                        .monospace()
                                        .size(12.)
                                        .strong()
                                        .color(Color32::WHITE),
                                    if selected {
                                        Color32::from_rgb(103, 42, 15)
                                    } else {
                                        theme::LIGHT
                                    },
                                    false,
                                ),
                            };
                            let stroke = if selected {
                                Color32::from_rgb(235, 78, 0)
                            } else if empty {
                                theme::DARK
                            } else {
                                theme::LIGHT
                            };
                            let button = egui::Button::new(label)
                                .min_size(egui::vec2(if empty { 10. } else { 0. }, 20.))
                                .fill(if empty { theme::DARK } else { background })
                                .stroke(Stroke::new(1_f32, stroke))
                                .sense(if empty {
                                    Sense::click()
                                } else {
                                    Sense::click_and_drag()
                                });
                            let response = ui.add(button);
                            if !empty {
                                response.dnd_set_drag_payload((id, index));
                            }
                            if let Some(payload) =
                                response.dnd_release_payload::<(egui::Id, usize)>()
                                && payload.0 == id
                                && payload.1 != index
                            {
                                moved = Some((payload.1, index));
                            }
                            if let Block::Var(_) = block {
                                if selected && state.focus_variable {
                                    response.request_focus();
                                    state.focus_variable = false;
                                }
                                let open =
                                    response.clicked() && selected || response.secondary_clicked();
                                if has_choices
                                    && let Some(variable) = variable_menu(
                                        &response,
                                        variables,
                                        &used,
                                        open.then_some(egui::SetOpenCommand::Bool(true)),
                                    )
                                {
                                    replace = Some((index, variable));
                                }
                                if response.has_focus() {
                                    ui.input_mut(|i| {
                                        let backspace =
                                            i.consume_key(egui::Modifiers::NONE, Key::Backspace);
                                        if backspace
                                            || i.consume_key(egui::Modifiers::NONE, Key::Delete)
                                        {
                                            remove = Some((index, backspace));
                                        } else if i
                                            .consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                                        {
                                            select =
                                                index.checked_sub(1).map(|index| (index, false));
                                        } else if i
                                            .consume_key(egui::Modifiers::NONE, Key::ArrowRight)
                                            && index + 1 < blocks.len()
                                        {
                                            select = Some((index + 1, true));
                                        }
                                    });
                                }
                            }
                            if response.clicked() {
                                if matches!(block, Block::Var(_)) {
                                    response.request_focus();
                                }
                                select = Some((index, false));
                            }
                        });
                    }
                });
            });
        let plus = egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
            .fit_to_exact_size(Vec2::splat(17.));
        let button =
            source_controls::add_sized_button(ui, [add_width, 31.], egui::Button::image(plus))
                .on_hover_text(locale::tr(ui, "Add a variable"));
        if has_choices {
            add = variable_menu(
                &button,
                variables,
                &used,
                button.clicked().then_some(egui::SetOpenCommand::Toggle),
            );
        }
        reset = ui
            .scope(|ui| {
                ui.visuals_mut().hyperlink_color = theme::ACCENT;
                ui.link(reset_label).clicked()
            })
            .inner;
    });

    let mut changed = false;
    if let Some((index, value)) = edit {
        if let Block::Literal(old) = &mut blocks[index]
            && *old != value
        {
            *old = value;
            changed = true;
        }
        state.selected = None;
        state.draft = None;
    }
    if let Some((from, to)) = moved {
        let block = blocks.remove(from);
        blocks.insert(to, block);
        state.selected = None;
        state.draft = None;
        changed = true;
    } else if let Some((index, backspace)) = remove {
        blocks.remove(index);
        state.selected = None;
        select = index.checked_sub(1).map(|index| (index, backspace));
        changed = true;
    } else if let Some((index, var)) = replace {
        blocks[index] = Block::Var(var);
        select = Some((index, false));
        changed = true;
    } else if let Some(var) = add {
        blocks.push(Block::Var(var));
        select = Some((blocks.len() - 1, false));
        changed = true;
    }
    let next = if reset {
        default.into()
    } else if changed {
        encode(&blocks)
    } else {
        input.into()
    };
    if !reset
        && let Some((index, cursor_at_start)) = select.filter(|(index, _)| *index < blocks.len())
    {
        let normalized = (next != input).then(|| parse_blocks(&next, variables));
        let selected = normalized
            .as_ref()
            .map_or(Some(index), |new| map_selection(&blocks, new, index));
        if let Some(index) = selected {
            let block = &normalized.as_deref().unwrap_or(&blocks)[index];
            state.selected = Some(index);
            state.focus_variable = matches!(block, Block::Var(_));
            state.draft = match block {
                Block::Literal(value) => Some(Draft {
                    value: value.clone(),
                    focus: true,
                    cursor_at_start,
                }),
                Block::Var(_) => None,
            };
        }
    }
    ui.data_mut(|d| {
        if reset {
            d.remove::<State>(id);
        } else {
            state.input.clone_from(&next);
            d.insert_temp(id, state);
        }
    });
    (next != input).then_some(Change { value: next, reset })
}
