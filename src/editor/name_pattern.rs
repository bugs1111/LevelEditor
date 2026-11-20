//! Shared editor for the naming patterns in Project settings.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Variable(&'static str, &'static str, &'static str);

pub(super) const LEVEL_DEFAULT: &str = "Level_%idx";
pub(super) const LEVEL: &[Variable] = &[
    Variable("world", "WorldId", "World identifier"),
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

fn blocks(mut input: &str, variables: &[Variable]) -> Vec<Block> {
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
}

#[derive(Clone, Default)]
struct State {
    input: String,
    selected: Option<usize>,
    draft: Option<Draft>,
    focus_variable: bool,
}

fn choose_variable(ui: &mut egui::Ui, variables: &[Variable], used: &[&str]) -> Option<Variable> {
    for &var in variables {
        if used.contains(&var.0) {
            continue;
        }
        if ui.button(var.1).on_hover_text(var.2).clicked() {
            ui.close();
            return Some(var);
        }
    }
    None
}

pub(super) fn show(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    input: &str,
    default: &str,
    variables: &[Variable],
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let mut state = ui
        .data(|d| d.get_temp::<State>(id))
        .filter(|state| state.input == input)
        .unwrap_or_else(|| State {
            input: input.into(),
            ..Default::default()
        });
    let mut blocks = blocks(input, variables);
    let mut select = None;
    let mut edit = None;
    let mut remove = None;
    let mut replace = None;
    let mut moved = None;
    let mut add = None;
    let mut reset = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        let add_width = 32.;
        let reset_label = egui::WidgetText::from("[reset]");
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
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1., theme::LIGHT))
            .inner_margin(egui::Margin::same(4))
            .show(ui, |ui| {
                ui.set_width((width - 10.).max(14.));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(1., 2.);
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
                                            && i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                                        {
                                            Some(index - 1)
                                        } else if edge == Some(draft.value.chars().count())
                                            && index + 1 < blocks.len()
                                            && i.consume_key(egui::Modifiers::NONE, Key::ArrowRight)
                                        {
                                            Some(index + 1)
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
                                    select = next;
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
                                    RichText::new(var.1).monospace().size(12.).strong(),
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
                                .min_size(egui::vec2(if empty { 10. } else { 0. }, 22.))
                                .fill(if empty { theme::DARK } else { background })
                                .stroke(Stroke::new(1., stroke))
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
                                egui::Popup::menu(&response)
                                    .open_memory(open.then_some(egui::SetOpenCommand::Bool(true)))
                                    .show(|ui| {
                                        replace = choose_variable(ui, variables, &used)
                                            .map(|var| (index, var))
                                    });
                                if response.has_focus() {
                                    ui.input_mut(|i| {
                                        if i.consume_key(egui::Modifiers::NONE, Key::Backspace)
                                            || i.consume_key(egui::Modifiers::NONE, Key::Delete)
                                        {
                                            remove = Some(index);
                                        } else if i
                                            .consume_key(egui::Modifiers::NONE, Key::ArrowLeft)
                                        {
                                            select = index.checked_sub(1);
                                        } else if i
                                            .consume_key(egui::Modifiers::NONE, Key::ArrowRight)
                                            && index + 1 < blocks.len()
                                        {
                                            select = Some(index + 1);
                                        }
                                    });
                                }
                            }
                            if response.clicked() {
                                if matches!(block, Block::Var(_)) {
                                    response.request_focus();
                                }
                                select = Some(index);
                            }
                        });
                    }
                });
            });
        let button =
            source_controls::add_sized_button(ui, [add_width, 31.], egui::Button::new("+"))
                .on_hover_text("Add a variable");
        egui::Popup::menu(&button).show(|ui| add = choose_variable(ui, variables, &used));
        reset = ui.link(reset_label).clicked();
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
    } else if let Some(index) = remove {
        blocks.remove(index);
        state.selected = None;
        changed = true;
    } else if let Some((index, var)) = replace {
        blocks[index] = Block::Var(var);
        changed = true;
    } else if let Some(var) = add {
        blocks.push(Block::Var(var));
        state.selected = None;
        changed = true;
    }
    if let Some(index) = select
        && let Some(block) = blocks.get(index)
    {
        state.selected = Some(index);
        state.focus_variable = matches!(block, Block::Var(_));
        state.draft = match block {
            Block::Literal(value) => Some(Draft {
                value: value.clone(),
                focus: true,
            }),
            Block::Var(_) => None,
        };
    }
    let next = if reset {
        default.into()
    } else if changed {
        encode(&blocks)
    } else {
        input.into()
    };
    ui.data_mut(|d| {
        if reset || next != input {
            d.remove::<State>(id);
        } else {
            d.insert_temp(id, state);
        }
    });
    (next != input).then_some(next)
}
