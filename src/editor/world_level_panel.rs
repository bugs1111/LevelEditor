//! Source WorldPanel and LevelInstancePanel forms.
use super::source_controls::edit_text;
use super::*;
use crate::app::chrome::ActionTooltip as _;

const WORLD_LABEL_WIDTH: f32 = 110.;
const LEVEL_LABEL_WIDTH: f32 = 102.;
const FORM_ROW_GAP: f32 = 10.;
const SMALL_INPUT_WIDTH: f32 = 60.;
const LEVEL_ACTION_SIZE: f32 = 128. / 3.;
const LEVEL_ACTION_ORANGE: Color32 = Color32::from_rgb(227, 77, 0);
const LEVEL_ACTION_RED: Color32 = Color32::from_rgb(121, 17, 5);

const WORLD_LAYOUTS: [(&str, &str); 4] = [
    ("Free", "2D free map - Freely positioned in space"),
    (
        "GridVania",
        "GridVania - Levels are positioned inside a large world-scale grid",
    ),
    ("LinearHorizontal", "Horizontal - One level after the other"),
    ("LinearVertical", "Vertical - One level after the other"),
];
const BACKGROUND_POSITIONS: [(&str, &str); 5] = [
    ("Unscaled", "Not scaled"),
    ("Contain", "Fit inside (keep aspect ratio)"),
    ("Cover", "Cover level (keep aspect ratio)"),
    ("CoverDirty", "Cover (dirty scaling)"),
    ("Repeat", "Repeat"),
];

fn icon_button(
    ui: &mut egui::Ui,
    image: impl Into<egui::Image<'static>>,
    help: &str,
    fill: Color32,
    size: Vec2,
) -> egui::Response {
    let button =
        egui::Button::image(image.into().fit_to_exact_size(egui::vec2(16., 16.))).fill(fill);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(4., 2.);
        source_controls::add_sized_button(ui, size, button).action_tooltip(locale::tr(ui, help))
    })
    .inner
}

fn depth_arrow(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    help: &str,
    enabled: bool,
) -> egui::Response {
    let (fill, tint) = if enabled {
        (LEVEL_ACTION_ORANGE, Color32::WHITE)
    } else {
        (theme::DARK, Color32::from_white_alpha(77))
    };
    ui.add_enabled_ui(enabled, |ui| {
        icon_button(
            ui,
            egui::Image::new(icon).tint(tint),
            help,
            fill,
            egui::vec2(45., 20.),
        )
    })
    .inner
}

fn background_position_controls(
    ui: &mut egui::Ui,
    level: &Value,
    image_size: Option<[f64; 2]>,
    patch: &mut Value,
) {
    let current = text(level, "bgPos", "Cover");
    let mut mode = current.to_owned();
    let pivot = [
        level["bgPivotX"].as_f64().unwrap_or(0.5),
        level["bgPivotY"].as_f64().unwrap_or(0.5),
    ];
    let standard = |pivot: [f64; 2]| pivot.iter().all(|v| [0., 0.5, 1.].contains(v));
    let id = ui.make_persistent_id(("background-pivot", text(level, "iid", "")));
    let mut show_advanced = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    let mut choice = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        ui.spacing_mut().interact_size.y = 42.;
        let width = (ui.available_width() - 154.).clamp(80., 160.);
        let label = BACKGROUND_POSITIONS
            .iter()
            .find(|(value, _)| *value == current)
            .map_or(current, |(_, label)| locale::tr(ui, label));
        ui.allocate_ui_with_layout(
            egui::vec2(width, 42.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                source_controls::form_combo("bgPos")
                    .width(width)
                    .truncate()
                    .selected_text(RichText::new(label).size(12.))
                    .show_ui(ui, |ui| {
                        for (value, label) in BACKGROUND_POSITIONS {
                            ui.selectable_value(&mut mode, value.into(), locale::tr(ui, label));
                        }
                    });
            },
        );
        if image_size.is_some() {
            choice = source_controls::pivot_grid_sized(ui, pivot, 14.)
                .filter(|selection| *selection != pivot);
            if !show_advanced && standard(pivot) {
                ui.add_space(4.);
                let help = locale::tr(ui, "Show advanced options");
                let label = egui::Label::new(RichText::new(help).size(12.)).truncate();
                show_advanced = ui
                    .add(source_controls::underlined_link(label))
                    .action_tooltip(help)
                    .clicked();
            }
        }
    });

    if choice.is_none()
        && (show_advanced || !standard(pivot))
        && let Some(image_size) = image_size
    {
        let level_size = [
            level["pxWid"].as_f64().unwrap_or(256.),
            level["pxHei"].as_f64().unwrap_or(256.),
        ];
        let (_, crop) = ldtk_core::exports::background_image_layout(level_size, image_size, &mode);
        choice = background_pivot_inputs(ui, id, pivot, crop.map(f64::trunc));
    }
    if let Some([x, y]) = choice {
        patch["bgPivotX"] = json!(x);
        patch["bgPivotY"] = json!(y);
    }
    if mode != current {
        patch["bgPos"] = json!(mode);
    }
    // The source rebuilds the controls after applying a preset or changing modes.
    ui.data_mut(|data| {
        data.insert_temp(id, show_advanced && choice.is_none() && mode == current);
    });
}

fn background_pivot_inputs(
    ui: &mut egui::Ui,
    id: egui::Id,
    pivot: [f64; 2],
    tile_size: [f64; 2],
) -> Option<[f64; 2]> {
    let mut next = pivot;
    egui::Frame::new()
        .stroke(Stroke::new(1_f32, theme::DARK))
        .inner_margin(4)
        .show(ui, |ui| {
            let layout = if ui.available_width() >= 280. {
                egui::Layout::left_to_right(egui::Align::Min)
            } else {
                egui::Layout::top_down(egui::Align::Min)
            };
            ui.with_layout(layout, |ui| {
                for (pixels, caption) in [(false, "Float:"), (true, "Pixels:")] {
                    if pixels {
                        ui.separator();
                    }
                    ui.horizontal(|ui| {
                        ui.set_height(32.);
                        ui.spacing_mut().item_spacing.x = 4.;
                        ui.label(
                            RichText::new(locale::tr(ui, caption))
                                .size(12.)
                                .color(theme::TEXT),
                        );
                        for axis in 0..2 {
                            if axis == 1 {
                                ui.label(",");
                            }
                            let extent = tile_size[axis];
                            let current = if pixels {
                                (pivot[axis] * extent).floor()
                            } else {
                                pivot[axis]
                            };
                            let draft = ui
                                .add_enabled_ui(!pixels || extent > 0., |ui| {
                                    edit_text(
                                        ui,
                                        id.with((pixels, axis)),
                                        &current.to_string(),
                                        |ui, text, id| {
                                            ui.add_sized(
                                                [40., 32.],
                                                form_text_box(text)
                                                    .font(FontId::proportional(12.))
                                                    .id(id),
                                            )
                                        },
                                    )
                                })
                                .inner;
                            if let Some(draft) = draft {
                                let value = if pixels {
                                    draft
                                        .trim()
                                        .parse::<i64>()
                                        .ok()
                                        .map(|v| v.clamp(-2048, 2048) as f64 / extent)
                                } else {
                                    draft
                                        .trim()
                                        .parse::<f64>()
                                        .ok()
                                        .filter(|v| v.is_finite())
                                        .map(|v| v.clamp(-50., 50.))
                                };
                                if let Some(value) = value {
                                    next[axis] = value;
                                }
                            }
                        }
                    });
                }
            });
        });
    (next != pivot).then_some(next)
}

fn level_action_button(
    ui: &mut egui::Ui,
    width: f32,
    image: egui::ImageSource<'static>,
    label: Option<&str>,
    fill: Color32,
) -> egui::Response {
    let image = egui::Image::new(image).fit_to_exact_size(Vec2::splat(22.667));
    let button = match label {
        Some(label) => egui::Button::image_and_text(
            image,
            RichText::new(locale::tr(ui, label)).color(Color32::WHITE),
        ),
        None => egui::Button::image(image),
    };
    source_controls::add_sized_button(
        ui,
        [width, LEVEL_ACTION_SIZE],
        button.fill(fill).stroke(Stroke::NONE),
    )
}

fn world_selector_id() -> egui::Id {
    egui::Id::new("world-form-selector")
}

fn world_selector(
    ui: &mut egui::Ui,
    worlds: &[(usize, String)],
    current_world: Option<usize>,
    name: &str,
) -> Option<usize> {
    let height = LEVEL_ACTION_SIZE + 2.;
    let width = (ui.available_width() - 2. * LEVEL_ACTION_SIZE - 3.).max(40.);
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_max_width(width);
            let salt = "world-properties";
            let id = ui.make_persistent_id(egui::Id::new(salt));
            ui.data_mut(|data| data.insert_temp(world_selector_id(), id));
            let options: Vec<_> = worlds
                .iter()
                .map(|(_, name)| (name.as_str(), None))
                .collect();
            let current = worlds
                .iter()
                .position(|(index, _)| Some(*index) == current_world);
            source_controls::form_select(
                ui,
                salt,
                height,
                &options,
                current,
                name,
                source_controls::FormSelectAppearance::Normal,
            )
            .map(|index| worlds[index].0)
            .or(current_world)
        },
    )
    .inner
}

fn form_text_box(value: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(value)
        .font(FontId::proportional(13.333))
        .margin(egui::Margin::symmetric(9, 0))
        .vertical_align(egui::Align::Center)
}

pub(super) fn form_text_input(
    ui: &mut egui::Ui,
    key: &'static str,
    current: &str,
    width: f32,
) -> Option<String> {
    edit_text(ui, key, current, |ui, draft, id| {
        add_form_text_box(ui, draft, id, key, width)
    })
}

fn add_form_text_box(
    ui: &mut egui::Ui,
    draft: &mut String,
    id: egui::Id,
    key: &str,
    width: f32,
) -> egui::Response {
    let (height, align) = if key == "worldDepth" {
        (40., egui::Align::Center)
    } else {
        (33., egui::Align::Min)
    };
    ui.add_sized(
        [width.max(20.), height],
        form_text_box(draft)
            .horizontal_align(align)
            .text_color(Color32::WHITE)
            .id(id),
    )
}

pub(super) fn parse_form_int(draft: &str) -> i64 {
    let text = draft.trim_start();
    let negative = text.starts_with('-');
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (radix, digits) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .map_or((10, digits), |digits| (16, digits));
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    if end == 0 {
        return 0;
    }
    let value = i64::from_str_radix(&digits[..end], radix).unwrap_or(i64::MAX);
    if negative { -value } else { value }
}

fn int_input(
    ui: &mut egui::Ui,
    key: &'static str,
    current: &Value,
    minimum: i64,
    maximum: i64,
    patch: &mut Value,
) {
    let current = current.as_i64().unwrap_or(0).to_string();
    let mut immediate = None;
    let typed = edit_text(ui, key, &current, |ui, draft, id| {
        if source_controls::step_number(ui, id, draft) {
            immediate = Some(draft.clone());
        }
        let response = add_form_text_box(ui, draft, id, key, SMALL_INPUT_WIDTH);
        if response.gained_focus() {
            source_controls::select_text_input(ui.ctx(), id, draft);
        }
        response
    });
    if immediate.is_some() {
        let id = ui.make_persistent_id(key);
        ui.data_mut(|data| data.remove::<(String, String)>(id));
    }
    if let Some(draft) = immediate.or(typed) {
        patch[key] = json!(parse_form_int(&draft).clamp(minimum, maximum));
    }
}

fn property_row(
    ui: &mut egui::Ui,
    label: &str,
    label_width: f32,
    body: impl FnOnce(&mut egui::Ui),
) {
    source_controls::form_grid_row(ui, label, label, egui::vec2(label_width, 33.), |ui| {
        ui.spacing_mut().item_spacing.x = 3.;
        body(ui);
    });
}

fn dimension_separator(ui: &mut egui::Ui, label: &str) {
    ui.add_sized(
        [8., 16.],
        egui::Label::new(label).halign(egui::Align::Center),
    );
}

fn iid_row(ui: &mut egui::Ui, iid: &str, width: f32, key: &str, help: &str) {
    source_controls::form_grid_row(ui, "IID", key, egui::vec2(width, 33.), |ui| {
        ui.spacing_mut().item_spacing.x = 1.;
        let mut value = iid.to_owned();
        ui.add_sized(
            [
                (ui.available_width() - SMALL_INPUT_WIDTH - 2.).max(20.),
                33.,
            ],
            form_text_box(&mut value)
                .interactive(false)
                .background_color(Color32::TRANSPARENT)
                .text_color(GROUP_BAR),
        );
        if icon_button(
            ui,
            egui::include_image!("../../assets/icons/copy.svg"),
            help,
            theme::LIGHT,
            egui::vec2(SMALL_INPUT_WIDTH, 32.),
        )
        .clicked()
        {
            ui.ctx().copy_text(iid.to_owned());
        }
    });
}

fn lock_level_inputs(ui: &mut egui::Ui) {
    ui.visuals_mut().disabled_alpha = 0.3;
    ui.disable();
}

fn group(
    ui: &mut egui::Ui,
    key: &'static str,
    title: impl Into<egui::WidgetText>,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let id = ui.make_persistent_id(key);
    let open = ui.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let open = section(ui, title, open, |ui| {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = FORM_ROW_GAP;
                body(ui);
            });
    });
    ui.data_mut(|d| d.insert_temp(id, open));
    open
}

fn properties_title(ui: &egui::Ui, template: &str, name: &str) -> egui::text::LayoutJob {
    let title = locale::tr(ui, template);
    let (before, after) = title.split_once("{name}").unwrap_or((title, ""));
    let normal = egui::TextFormat::simple(
        FontId::new(17.333, egui::FontFamily::Name("panel-title".into())),
        Color32::PLACEHOLDER,
    );
    let highlight = egui::TextFormat::simple(
        FontId::new(17.333, egui::FontFamily::Name("heading".into())),
        Color32::WHITE,
    );
    let mut job = egui::text::LayoutJob::default();
    job.append(before, 0., normal.clone());
    job.append(name, 0., highlight);
    job.append(after, 0., normal);
    job
}

fn depth_moves(depth: i64, other_depths: impl Iterator<Item = i64>) -> (bool, bool) {
    // The original depth bounds always include zero, even without a level on it.
    let mut further = depth < 0;
    let mut closer = depth > 0;
    for other in other_depths {
        further |= other >= depth;
        closer |= other <= depth;
    }
    (further, closer)
}

impl EditorApp {
    pub(in crate::app) fn world_selector_open(ctx: &egui::Context) -> bool {
        ctx.data(|data| data.get_temp::<egui::Id>(world_selector_id()))
            .is_some_and(|id| egui::ComboBox::is_open(ctx, id))
    }

    pub(in crate::app) fn move_selected_level_world_depth(&mut self, delta: i64) {
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let mut depths = project
            .level_keys_in_world(key.world)
            .filter_map(|other| {
                Some((
                    other,
                    project.level(other)?["worldDepth"].as_i64().unwrap_or(0),
                ))
            })
            .collect::<Vec<_>>();
        let Some(selected) = depths.iter().position(|(other, _)| *other == key) else {
            return;
        };
        let current = depths[selected].1;
        let (further, closer) = depth_moves(
            current,
            depths
                .iter()
                .filter_map(|&(other, depth)| (other != key).then_some(depth)),
        );
        if (delta > 0 && !further) || (delta < 0 && !closer) {
            self.select_world_depth(current);
            return;
        }
        let target = current + delta;
        depths[selected].1 = target;
        // Moving further keeps depth zero occupied by shifting every depth
        // down until the nearest positive one reaches zero.
        let shift = if delta > 0 && depths.iter().all(|&(_, depth)| depth != 0) {
            depths
                .iter()
                .map(|&(_, depth)| depth)
                .filter(|&depth| depth > 0)
                .min()
                .unwrap_or(0)
        } else {
            0
        };
        if self
            .edit("Level depth changed", |project| {
                for (other, depth) in depths {
                    if other == key || shift != 0 {
                        project.level_mut(other).unwrap()["worldDepth"] = json!(depth - shift);
                    }
                }
                project.refresh_derived_data()
            })
            .is_some()
        {
            self.select_world_depth(target - shift);
        }
    }

    pub(super) fn source_world_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else { return };
        let multi = project.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.contains(&json!("MultiWorlds")));
        let current_world = self.active_world;
        let world = current_world
            .and_then(|i| project.json["worlds"].get(i))
            .unwrap_or(&project.json)
            .clone();
        let name = text(&world, "identifier", "World").to_owned();
        let iid = if current_world.is_some() {
            text(&world, "iid", "")
        } else {
            text(&world, "dummyWorldIid", "")
        }
        .to_owned();
        let default_grid = number(&project.json, "defaultGridSize", 16.) as i64;
        if multi {
            let worlds = project.json["worlds"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, w)| (i, text(w, "identifier", "World").to_owned()))
                .collect::<Vec<_>>();
            ui.add_space(2.);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.;
                let selected = world_selector(ui, &worlds, current_world, &name);
                if selected != current_world && selected.is_some() {
                    self.switch_world(selected, false);
                }
                if level_action_button(
                    ui,
                    LEVEL_ACTION_SIZE,
                    egui::include_image!("../../assets/icons/add.svg"),
                    None,
                    LEVEL_ACTION_ORANGE,
                )
                .action_tooltip(locale::tr(ui, "Create a new world"))
                .clicked()
                {
                    self.create_form_world();
                }
                if level_action_button(
                    ui,
                    LEVEL_ACTION_SIZE,
                    egui::include_image!("../../assets/icons/delete.svg"),
                    None,
                    LEVEL_ACTION_RED,
                )
                .action_tooltip(locale::tr(ui, "Delete current world"))
                .clicked()
                    && let Some(world) = current_world
                {
                    self.queue_world_form_action(
                        ui.ctx(),
                        world_form_dialog::WorldFormAction::DeleteWorld(world),
                    );
                }
            });
            ui.add_space(8.);
        }
        // The source button margins add one point on each edge and two between buttons.
        ui.add_space(if multi { 1. } else { 3. });
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(1, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    let create_width =
                        (ui.available_width() - 2. * (LEVEL_ACTION_SIZE + 2.)).max(40.);
                    let enabled = !multi || current_world.is_some();
                    ui.add_enabled_ui(enabled, |ui| {
                        if level_action_button(
                            ui,
                            create_width,
                            egui::include_image!("../../assets/icons/add.svg"),
                            Some("CREATE A NEW LEVEL"),
                            LEVEL_ACTION_ORANGE,
                        )
                        .clicked()
                        {
                            self.start_level_spot_picker();
                        }
                    });
                    ui.add_enabled_ui(self.selected.is_some(), |ui| {
                        if level_action_button(
                            ui,
                            LEVEL_ACTION_SIZE,
                            egui::include_image!("../../assets/icons/copy.svg"),
                            None,
                            LEVEL_ACTION_ORANGE,
                        )
                        .action_tooltip(locale::tr(ui, "Duplicate current level"))
                        .clicked()
                            && let Some(key) = self.selected
                        {
                            self.queue_world_form_action(
                                ui.ctx(),
                                world_form_dialog::WorldFormAction::DuplicateLevel(key),
                            );
                        }
                        if level_action_button(
                            ui,
                            LEVEL_ACTION_SIZE,
                            egui::include_image!("../../assets/icons/delete.svg"),
                            None,
                            LEVEL_ACTION_RED,
                        )
                        .action_tooltip(locale::tr(ui, "Delete current level"))
                        .clicked()
                            && let Some(key) = self.selected
                        {
                            self.queue_world_form_action(
                                ui.ctx(),
                                world_form_dialog::WorldFormAction::DeleteLevel(key),
                            );
                        }
                    });
                });
            });
        ui.add_space(16.);
        let mut patch = json!({});
        let world_open = group(
            ui,
            "worldPanel_world",
            properties_title(ui, "WORLD SETTINGS: \"{name}\"", &name),
            |ui| {
                iid_row(ui, &iid, WORLD_LABEL_WIDTH, "worldIid", "Copy world IID");
                property_row(ui, "World identifier", WORLD_LABEL_WIDTH, |ui| {
                    ui.add_enabled_ui(multi, |ui| {
                        if let Some(next) =
                            form_text_input(ui, "worldId", &name, ui.available_width())
                        {
                            patch["identifier"] = json!(next);
                        }
                    });
                });
                source_controls::form_grid_row(
                    ui,
                    "World layout",
                    "World layout",
                    egui::vec2(WORLD_LABEL_WIDTH, 34.),
                    |ui| {
                        let current = text(&world, "worldLayout", "Free");
                        if let Some(layout) =
                            ui_fields::enum_picker::show_text(ui, &WORLD_LAYOUTS, current)
                        {
                            patch["worldLayout"] = json!(layout);
                        }
                    },
                );
                if world["worldLayout"] == "GridVania" {
                    property_row(ui, "Gridvania grid size", WORLD_LABEL_WIDTH, |ui| {
                        int_input(
                            ui,
                            "worldGridWidth",
                            &world["worldGridWidth"],
                            1,
                            i16::MAX.into(),
                            &mut patch,
                        );
                        dimension_separator(ui, "x");
                        int_input(
                            ui,
                            "worldGridHeight",
                            &world["worldGridHeight"],
                            1,
                            i16::MAX.into(),
                            &mut patch,
                        );
                        ui.label("px");
                    });
                }
                property_row(ui, "Default \"new level\" size", WORLD_LABEL_WIDTH, |ui| {
                    int_input(
                        ui,
                        "defaultLevelWidth",
                        &world["defaultLevelWidth"],
                        default_grid,
                        9999,
                        &mut patch,
                    );
                    dimension_separator(ui, "x");
                    int_input(
                        ui,
                        "defaultLevelHeight",
                        &world["defaultLevelHeight"],
                        default_grid,
                        9999,
                        &mut patch,
                    );
                    ui.label("px");
                });
                let mut bounds = ui.min_rect();
                bounds.max.y += 2.;
                ui.expand_to_include_rect(bounds);
            },
        );
        if changed(&patch) {
            self.queue_world_form_action(
                ui.ctx(),
                world_form_dialog::WorldFormAction::Settings {
                    world: current_world,
                    patch,
                },
            );
        }
        ui.add_space(if world_open { 8. } else { 2. });
        self.source_level_form(ui);
    }

    pub(super) fn source_level_form(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.selected else {
            return;
        };
        let Some(level) = self.project.as_ref().and_then(|p| p.level(key)) else {
            return;
        };
        let enabled = level["worldDepth"].as_i64().unwrap_or(0) == self.world_depth
            || self.level_form_depth_override == Some(key);
        let backdrop = (!enabled).then(|| ui.painter().add(egui::Shape::Noop));
        let form = ui.scope(|ui| self.source_level_form_contents(ui, enabled));
        if let Some(backdrop) = backdrop
            && let Ok(egui::load::TexturePoll::Ready { texture }) =
                egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .load_for_size(ui.ctx(), Vec2::splat(8.))
        {
            let rect = form.response.rect;
            let mut mesh = egui::Mesh::with_texture(texture.id);
            mesh.add_rect_with_uv(
                rect,
                Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.),
                Color32::WHITE,
            );
            ui.painter().set(backdrop, mesh);
        }
    }

    fn source_level_form_contents(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let Some(key) = self.selected else { return };
        let Some(mut level) = self.project.as_ref().and_then(|p| p.level(key)).cloned() else {
            return;
        };
        let project = self.project.as_ref().unwrap();
        let world = key
            .world
            .and_then(|index| project.json["worlds"].get(index))
            .unwrap_or(&project.json);
        if let Some((dragged, [x, y])) = self.linear_drag_preview()
            && dragged == key
        {
            level["worldX"] = json!(x);
            level["worldY"] = json!(y);
        } else if text(world, "worldLayout", "Free").starts_with("Linear")
            && let Some((_, bounds)) = canvas::placements(project, key)
                .into_iter()
                .find(|(candidate, _)| *candidate == key)
        {
            level["worldX"] = json!(bounds[0] as i64);
            level["worldY"] = json!(bounds[1] as i64);
        }
        let default_grid =
            number(&self.project.as_ref().unwrap().json, "defaultGridSize", 16.) as i64;
        let default_background = text(
            &self.project.as_ref().unwrap().json,
            "defaultLevelBgColor",
            "#696A79",
        )
        .to_owned();
        let name = text(&level, "identifier", "Level");
        let mut patch = json!({});
        let mut depth_move = 0;
        let level_open = group(
            ui,
            "levelPanel_level",
            properties_title(ui, "LEVEL PROPERTIES: \"{name}\"", name),
            |ui| {
                if !enabled {
                    lock_level_inputs(ui);
                }
                iid_row(
                    ui,
                    text(&level, "iid", ""),
                    LEVEL_LABEL_WIDTH,
                    "levelIid",
                    "Copy level IID",
                );
                property_row(ui, "Level identifier", LEVEL_LABEL_WIDTH, |ui| {
                    let mut auto = level["useAutoIdentifier"].as_bool().unwrap_or(false);
                    ui.spacing_mut().item_spacing.x = 16.;
                    let width = (ui.available_width() - 99.).max(20.);
                    ui.add_enabled_ui(!auto, |ui| {
                        if let Some(value) = form_text_input(ui, "identifier", name, width) {
                            patch["identifier"] = json!(value);
                        }
                    });
                    if source_checkbox(ui, &mut auto, "auto").changed() {
                        patch["useAutoIdentifier"] = json!(auto);
                    }
                });
                property_row(ui, "World depth", LEVEL_LABEL_WIDTH, |ui| {
                    ui.spacing_mut().item_spacing.x = 1.;
                    int_input(
                        ui,
                        "worldDepth",
                        &level["worldDepth"],
                        i16::MIN.into(),
                        i16::MAX.into(),
                        &mut patch,
                    );
                    let project = self.project.as_ref().unwrap();
                    let depths = project
                        .level_keys_in_world(key.world)
                        .filter(|other| *other != key)
                        .filter_map(|other| {
                            Some(project.level(other)?["worldDepth"].as_i64().unwrap_or(0))
                        });
                    let depth = level["worldDepth"].as_i64().unwrap_or(0);
                    let (further, closer) = depth_moves(depth, depths);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        // Keep the disabled fill and hatch opaque. Only the arrow icon fades.
                        ui.visuals_mut().disabled_alpha = 1.;
                        if depth_arrow(
                            ui,
                            egui::include_image!("../../assets/icons/up.svg"),
                            "Move level further",
                            further,
                        )
                        .clicked()
                        {
                            depth_move = 1;
                        }
                        if depth_arrow(
                            ui,
                            egui::include_image!("../../assets/icons/down.svg"),
                            "Move level closer",
                            closer,
                        )
                        .clicked()
                        {
                            depth_move = -1;
                        }
                    });
                });
                property_row(ui, "World coords", LEVEL_LABEL_WIDTH, |ui| {
                    int_input(
                        ui,
                        "worldX",
                        &level["worldX"],
                        i16::MIN.into(),
                        i16::MAX.into(),
                        &mut patch,
                    );
                    dimension_separator(ui, ",");
                    int_input(
                        ui,
                        "worldY",
                        &level["worldY"],
                        i16::MIN.into(),
                        i16::MAX.into(),
                        &mut patch,
                    );
                    ui.label("px");
                });
                property_row(ui, "Size", LEVEL_LABEL_WIDTH, |ui| {
                    int_input(ui, "pxWid", &level["pxWid"], default_grid, 4096, &mut patch);
                    dimension_separator(ui, "x");
                    int_input(ui, "pxHei", &level["pxHei"], default_grid, 4096, &mut patch);
                    ui.label("px");
                });
                source_controls::form_grid_row(
                    ui,
                    "Bg color",
                    "Bg color",
                    egui::vec2(LEVEL_LABEL_WIDTH, 32.),
                    |ui| {
                        let col = theme::color(text(&level, "bgColor", &default_background));
                        let mut rgb = [col.r(), col.g(), col.b()];
                        let is_default = level["bgColor"].is_null();
                        ui.spacing_mut().item_spacing.x = if is_default { 4. } else { 8. };
                        if source_colors::color_editor_sized(
                            ui,
                            &mut rgb,
                            Some("bg"),
                            egui::vec2(64., 32.),
                        )
                        .changed()
                        {
                            let color = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                            patch["bgColor"] = if color.eq_ignore_ascii_case(&default_background) {
                                Value::Null
                            } else {
                                json!(color)
                            };
                        }
                        if is_default {
                            ui.label(
                                RichText::new(locale::tr(ui, "(default)"))
                                    .color(Color32::from_gray(153)),
                            );
                        } else {
                            let label = egui::Label::new(
                                RichText::new(locale::tr(ui, "Use default")).size(13.333),
                            );
                            if ui.add(source_controls::underlined_link(label)).clicked() {
                                patch["bgColor"] = Value::Null;
                            }
                        }
                    },
                );
                source_controls::form_grid_row(
                    ui,
                    "Bg image",
                    "Bg image",
                    egui::vec2(LEVEL_LABEL_WIDTH, 24.),
                    |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            if let Some(path) = self.source_image_picker(
                                ui,
                                ("level-background", text(&level, "iid", "")),
                                level["bgRelPath"].as_str(),
                            ) {
                                patch["bgRelPath"] = json!(path);
                            }
                            if let Some(path) = level["bgRelPath"].as_str() {
                                let path = canvas::asset_path(self.project.as_ref().unwrap(), path);
                                let size = self
                                    .assets
                                    .texture(ui.ctx(), &path)
                                    .map(|texture| texture.size().map(|n| n as f64));
                                background_position_controls(ui, &level, size, &mut patch);
                            }
                        });
                    },
                );
                // Include the final row's margin without an extra spacer beneath it.
                ui.add_space(0.);
            },
        );
        if enabled && changed(&patch) {
            let resizing = patch.get("pxWid").is_some() || patch.get("pxHei").is_some();
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                snap_level_form_patch(p, key, &mut patch)?;
                relative_path_patch(p, &mut patch, "bgRelPath")?;
                normalize_background_patch(&level, &mut patch);
                if let Some(identifier) = patch["identifier"].as_str() {
                    let mut name = p.cleanup_identifier(identifier);
                    if name.is_empty() || name == "_" {
                        name = p.cleanup_identifier("Unnamed");
                    }
                    patch["identifier"] = json!(unique_name(
                        &name,
                        p.level_keys()
                            .into_iter()
                            .filter(|other| *other != key)
                            .filter_map(|other| p.level(other)?["identifier"].as_str())
                    ));
                }
                if let Some(path) = patch["bgRelPath"].as_str() {
                    ldtk_core::exports::load_image_rgba(&canvas::asset_path(p, path))?;
                }
                let removed = if resizing {
                    p.resize_level_counting_removed_entities(
                        key,
                        patch["pxWid"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxWid", 256.) as i64)
                            as i32,
                        patch["pxHei"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxHei", 256.) as i64)
                            as i32,
                        0,
                        0,
                    )?
                } else {
                    0
                };
                let target = p
                    .level_mut(key)
                    .ok_or_else(|| ldtk_core::Error::Invalid("level missing".into()))?;
                apply_patch(target, &patch);
                p.refresh_derived_data()?;
                p.rebuild_auto_layers(key)?;
                Ok(removed)
            };
            let applied = if resizing {
                self.edit_level_recoverable("Level resized", key, operation)
            } else {
                self.edit("Level properties changed", operation)
            };
            if let Some(removed) = applied {
                self.report_resize_removals(removed);
                if patch.get("worldDepth").is_some() {
                    // A typed depth refreshes the original form without rechecking its active state.
                    self.level_form_depth_override = Some(key);
                }
            }
        }
        if depth_move != 0 {
            self.move_selected_level_world_depth(depth_move);
            self.world_selection.clear();
        }
        ui.add_space(if level_open { 8. } else { 2. });
        group(ui, "levelPanel_fields", "CUSTOM FIELDS", |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            if level["fieldInstances"].as_array().is_none_or(Vec::is_empty) {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.), Sense::hover());
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                    .paint_at(ui, rect);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    locale::tr(ui, "No custom field."),
                    FontId::proportional(12.667),
                    Color32::from_rgb(115, 127, 157),
                );
            } else {
                ui.add_space(1.);
                let outer_shadow = ui.painter().add(egui::Shape::Noop);
                let fields = egui::Frame::new()
                    .fill(theme::MEDIUM)
                    .shadow(egui::Shadow {
                        offset: [0, 0],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(230),
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = FORM_ROW_GAP;
                        if !enabled {
                            lock_level_inputs(ui);
                        }
                        self.instance_fields(ui, key, None, &level);
                    });
                // The wider, lighter shadow reproduces the soft edge beyond the main blur.
                ui.painter().set(
                    outer_shadow,
                    egui::Shadow {
                        offset: [0, 0],
                        blur: 48,
                        spread: 0,
                        color: Color32::from_black_alpha(43),
                    }
                    .as_shape(fields.response.rect, 0.),
                );
            }
            if source_controls::add_sized_button(
                ui,
                [ui.available_width(), 36.],
                egui::Button::new(
                    RichText::new(locale::tr(ui, "EDIT CUSTOM FIELDS")).color(Color32::WHITE),
                )
                .fill(SOURCE_ORANGE),
            )
            .clicked()
            {
                self.field_owner = None;
                self.field_uid = None;
                self.show_panel(Some(Panel::Fields));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> (EditorApp, egui::Context) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.repaint = Some(ctx.clone());
        (app, ctx)
    }
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700., 1500.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut form = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(Rect::from_min_size(
                                ui.next_widget_position(),
                                egui::vec2(if panel == Panel::World { 368. } else { 450. }, 1450.),
                            ))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    if panel == Panel::World {
                        app.source_world_form(&mut form);
                    } else {
                        app.source_level_form(&mut form);
                    }
                });
                app.world_form_dialog(ctx);
            },
        )
    }
    fn find_label(output: &egui::FullOutput, expected: &str) -> Option<egui::Pos2> {
        fn find(shape: &egui::Shape, expected: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text.replace('\n', " ") == expected => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, expected)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|s| find(&s.shape, expected))
    }
    fn label(output: &egui::FullOutput, expected: &str) -> egui::Pos2 {
        find_label(output, expected)
            .unwrap_or_else(|| panic!("missing source form label {expected}"))
    }

    #[test]
    fn biome_field_in_both_panels_shows_only_the_project_documentation() {
        const DOC: &str = "Change this Biome value to enable/disable rule groups.";
        for panel in [Panel::World, Panel::Level] {
            let (mut app, ctx) = editor();
            let mut project = Project::new();
            let field = project
                .add_field_definition(None, "biome", "String", false)
                .unwrap();
            project
                .update_field_definition(field, &json!({"doc": DOC}))
                .unwrap();
            app.set_project(project);
            locale::set(&ctx, locale::Language::SimplifiedChinese);
            ctx.style_mut(|style| {
                style.interaction.tooltip_delay = 0.;
                style.interaction.show_tooltips_only_when_still = false;
            });

            let initial = frame(&mut app, &ctx, panel, vec![]);
            let name = label(&initial, "biome");
            let badge = initial
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "?" => {
                        let center = text.pos + text.galley.size() * 0.5;
                        ((center.y - name.y).abs() < 7. && center.x > name.x).then_some(center)
                    }
                    _ => None,
                })
                .expect("biome question badge");
            for (position, shows_help) in [(name, false), (badge, true)] {
                frame(
                    &mut app,
                    &ctx,
                    panel,
                    vec![egui::Event::PointerMoved(position)],
                );
                frame(&mut app, &ctx, panel, vec![]);
                let output = frame(&mut app, &ctx, panel, vec![]);
                let name_count = output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            &shape.shape, egui::Shape::Text(text) if text.galley.text() == "biome"
                        )
                    })
                    .count();
                assert_eq!(name_count, 1);
                assert_eq!(find_label(&output, DOC).is_some(), shows_help);
            }
        }
    }

    fn depth_arrow_centers(output: &egui::FullOutput) -> [egui::Pos2; 2] {
        let mut centers = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if shape.rect.size() == egui::vec2(45., 20.) && shape.blur_width == 0. =>
                {
                    Some(shape.rect.center())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        centers.sort_by(|a, b| a.y.total_cmp(&b.y));
        centers.dedup();
        centers.try_into().expect("two depth arrow buttons")
    }
    fn click_at(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        pos: egui::Pos2,
    ) -> egui::FullOutput {
        frame(app, ctx, panel, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                panel,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        frame(app, ctx, panel, vec![])
    }
    fn click(
        app: &mut EditorApp,
        ctx: &egui::Context,
        panel: Panel,
        expected: &str,
    ) -> egui::FullOutput {
        frame(app, ctx, panel, vec![]);
        let output = frame(app, ctx, panel, vec![]);
        click_at(app, ctx, panel, label(&output, expected))
    }
    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }
    fn replace_text(app: &mut EditorApp, ctx: &egui::Context, panel: Panel, value: &str) {
        frame(
            app,
            ctx,
            panel,
            vec![key(
                Key::A,
                egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            )],
        );
        let edit = if value.is_empty() {
            key(Key::Backspace, egui::Modifiers::NONE)
        } else {
            egui::Event::Text(value.into())
        };
        frame(app, ctx, panel, vec![edit]);
        frame(
            app,
            ctx,
            panel,
            vec![key(Key::Enter, egui::Modifiers::NONE)],
        );
    }
    #[test]
    fn source_world_and_level_inputs_center_text_with_source_padding() {
        let (mut app, ctx) = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.json["defaultLevelWidth"] = json!(337);
        let level = project.level_mut(key).unwrap();
        level["worldX"] = json!(752);
        level["worldDepth"] = json!(7);
        let level_iid = level["iid"].as_str().unwrap().to_owned();
        app.world_depth = 7;

        for (panel, values) in [
            (Panel::World, &["337", "752", &level_iid, "7"][..]),
            (Panel::Level, &["752", &level_iid, "7"][..]),
        ] {
            let output = frame(&mut app, &ctx, panel, vec![]);
            for value in values {
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == *value => Some(text),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing input text {value}"));
                let depth = *value == "7";
                let field = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape)
                            if (33.0..=40.0).contains(&shape.rect.height())
                                && shape.rect.contains(text.pos) =>
                        {
                            Some(shape.rect)
                        }
                        _ => None,
                    })
                    .min_by(|a, b| a.width().total_cmp(&b.width()))
                    .unwrap_or_else(|| panic!("missing frame for input {value}"));
                let center = text.pos + text.galley.rect.center().to_vec2();
                assert!(
                    (center.y - field.center().y).abs() <= 1.,
                    "{panel:?} {value} vertical alignment"
                );
                if depth {
                    assert_eq!(field.size(), egui::vec2(60., 40.));
                    assert!(
                        (center.x - field.center().x).abs() <= 1.,
                        "center depth value"
                    );
                } else {
                    assert_eq!(
                        text.pos.x - field.left(),
                        9.,
                        "{panel:?} {value} text inset"
                    );
                }
            }
        }
    }

    #[test]
    fn source_world_embeds_world_level_and_custom_field_groups() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&output, "CREATE A NEW LEVEL");
        label(&output, "WORLD SETTINGS: \"World\"");
        label(&output, "LEVEL PROPERTIES: \"Level_0\"");
        label(&output, "CUSTOM FIELDS");
        label(&output, "Default \"new level\" size");
        label(&output, "World coords");
    }

    #[test]
    fn world_and_level_forms_top_align_wrapped_labels_and_controls() {
        let (mut app, ctx) = editor();
        theme::use_reference_layout_fonts(&ctx);
        ctx.style_mut(|style| style.spacing.item_spacing.y = 0.);
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.json["defaultLevelWidth"] = json!(337);
        project.level_mut(key).unwrap()["worldX"] = json!(752);

        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        let text = |expected: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text().replace('\n', " ") == expected =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing form text {expected}"))
        };
        for (caption, input) in [
            ("Default \"new level\" size", "337"),
            ("World coords", "752"),
        ] {
            let caption = text(caption);
            assert_eq!(caption.galley.job.halign, egui::Align::RIGHT);
            for rows in caption.galley.rows.windows(2) {
                assert_eq!(rows[1].pos.y - rows[0].pos.y, 15.);
            }
            assert!((4. ..=8.).contains(&(text(input).pos.y - caption.pos.y)));
        }
        let collapsed = click(&mut app, &ctx, Panel::World, "WORLD SETTINGS: \"World\"");
        let world = label(&collapsed, "WORLD SETTINGS: \"World\"");
        let level = label(&collapsed, "LEVEL PROPERTIES: \"Level_0\"");
        assert_eq!(level.y - world.y, 32.);
    }

    #[test]
    fn source_world_language_switch_preserves_project_identifiers() {
        let (mut app, ctx) = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.json["identifier"] = json!("World");
        project.level_mut(key).unwrap()["identifier"] = json!("Layer");
        let before = project.json.clone();

        let english = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&english, "WORLD SETTINGS: \"World\"");
        label(&english, "LEVEL PROPERTIES: \"Layer\"");

        locale::set(&ctx, locale::Language::SimplifiedChinese);
        let chinese = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&chinese, "世界设置：「World」");
        label(&chinese, "关卡属性：「Layer」");
        label(&chinese, "创建新关卡");

        locale::set(&ctx, locale::Language::English);
        let english = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&english, "CREATE A NEW LEVEL");
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn only_populated_level_field_forms_have_the_source_backdrop_and_shadow() {
        let (mut app, ctx) = editor();
        let has_shadow = |output: egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Vec(shapes)
                    if matches!(shapes.as_slice(), [egui::Shape::Rect(shadow), egui::Shape::Rect(backdrop)]
                        if shadow.blur_width == 24. && shadow.fill == Color32::from_black_alpha(230)
                            && shadow.rect.width() > 300.
                            && shadow.rect == backdrop.rect && backdrop.fill == theme::MEDIUM))
            })
        };
        assert!(!has_shadow(frame(&mut app, &ctx, Panel::World, vec![])));
        app.project
            .as_mut()
            .unwrap()
            .add_field_definition(None, "Status", "String", false)
            .unwrap();
        assert!(has_shadow(frame(&mut app, &ctx, Panel::World, vec![])));
    }
    #[test]
    fn source_level_uses_paired_coords_size_and_default_background() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        for expected in [
            "World coords",
            "Size",
            "Bg color",
            "Bg image",
            "(default)",
            "EDIT CUSTOM FIELDS",
        ] {
            label(&output, expected);
        }
    }
    #[test]
    fn level_background_controls_share_a_compact_picker_and_source_reset_link() {
        for panel in [Panel::World, Panel::Level] {
            let (mut app, ctx) = editor();
            let key = app.selected.unwrap();
            let swatch = Color32::from_rgb(194, 74, 88);
            app.project.as_mut().unwrap().level_mut(key).unwrap()["bgColor"] = json!("#C24A58");
            let output = frame(&mut app, &ctx, panel, vec![]);
            let picker = label(&output, "[ No image ]");
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(shape)
                    if shape.rect.contains(picker) && shape.rect.height() == 24.
                        && shape.fill == theme::LIGHT.gamma_multiply(0.6))
                }),
                "{panel:?} empty picker is a compact grey action"
            );
            let swatch = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == swatch && rect.rect.size() == egui::vec2(64., 32.) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("source level color swatch");
            let link = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Use default" => Some(text),
                    _ => None,
                })
                .expect("source reset link");
            assert_eq!(link.pos.x, swatch.right() + 8.);
            assert_eq!(link.fallback_color, theme::ACCENT);
            assert_eq!(link.underline, Stroke::new(1_f32, theme::ACCENT));
            assert!(
                link.galley
                    .job
                    .sections
                    .iter()
                    .all(|part| part.format.underline == Stroke::NONE)
            );
            let output = click(&mut app, &ctx, panel, "Use default");
            assert!(app.project.as_ref().unwrap().level(key).unwrap()["bgColor"].is_null());
            label(&output, "(default)");
        }
    }

    #[test]
    fn level_custom_fields_follow_the_background_row_height() {
        let (mut app, ctx) = editor();
        let group_offsets = |app: &mut EditorApp, time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Level);
                },
            )
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == GROUP_BAR && rect.rect.height() == 28. => {
                    Some(rect.rect.top())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
        };
        group_offsets(&mut app, 0.);
        group_offsets(&mut app, 1.);
        let empty = group_offsets(&mut app, 2.);
        assert_eq!(empty.as_slice(), &[112., 466.]);

        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("Wide.png");
        image::RgbaImage::from_pixel(512, 128, image::Rgba([30, 34, 41, 255]))
            .save(&image)
            .unwrap();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.path = Some(dir.path().join("Wide.ldtk"));
        project.level_mut(key).unwrap()["bgRelPath"] = json!("Wide.png");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.assets.texture(&ctx, &image).is_none() {
            assert!(std::time::Instant::now() < deadline, "background must load");
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.assets.poll(&ctx);
        }
        assert_eq!(group_offsets(&mut app, 3.).as_slice(), &[112., 516.]);
    }

    #[test]
    fn background_pivot_pixels_use_the_cropped_tile_size() {
        let (mut app, ctx) = editor();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("Wide.png");
        image::RgbaImage::from_pixel(512, 128, image::Rgba([30, 34, 41, 255]))
            .save(&image)
            .unwrap();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.path = Some(dir.path().join("Wide.ldtk"));
        apply_patch(
            project.level_mut(key).unwrap(),
            &json!({"bgRelPath":"Wide.png","bgPos":"Cover","bgPivotX":0.5,"bgPivotY":0.0}),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.assets.texture(&ctx, &image).is_none() {
            assert!(std::time::Instant::now() < deadline, "background must load");
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.assets.poll(&ctx);
        }
        let output = click(&mut app, &ctx, Panel::Level, "Show advanced options");
        let x_pixels = label(&output, "64");
        click_at(&mut app, &ctx, Panel::Level, x_pixels);
        replace_text(&mut app, &ctx, Panel::Level, "32");

        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["bgPivotX"], 0.25);
        assert_eq!(level["bgPivotY"], 0.0);
    }

    #[test]
    fn manually_changing_depth_keeps_the_level_form_editable_until_the_active_depth_changes() {
        let (mut app, ctx) = editor();
        app.show_panel(Some(Panel::Level));
        let key = app.selected.unwrap();
        app.project.as_mut().unwrap().json["worldLayout"] = json!("Free");
        let edit_depth = |app: &mut EditorApp, value: &str| {
            let output = frame(app, &ctx, Panel::Level, vec![]);
            let [further, _] = depth_arrow_centers(&output);
            click_at(app, &ctx, Panel::Level, egui::pos2(156., further.y + 10.));
            replace_text(app, &ctx, Panel::Level, value);
        };
        let edit_x = |app: &mut EditorApp, value: &str| {
            let panel = app.panel.unwrap();
            let output = frame(app, &ctx, panel, vec![]);
            let pos = egui::pos2(156., label(&output, "World coords").y);
            click_at(app, &ctx, panel, pos);
            replace_text(app, &ctx, panel, value);
        };

        edit_depth(&mut app, "2");
        assert_eq!(app.world_depth, 0);
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldDepth"],
            2
        );
        edit_x(&mut app, "160");
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldX"],
            160
        );

        app.select_world_depth(1);
        edit_x(&mut app, "32");
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldX"],
            160
        );
        app.select_world_depth(2);
        edit_x(&mut app, "32");
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldX"],
            32
        );

        edit_depth(&mut app, "3");
        app.dismiss_panel();
        app.show_panel(Some(Panel::Level));
        edit_x(&mut app, "48");
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldX"],
            48
        );

        app.show_panel(Some(Panel::World));
        app.select_world_depth(2);
        edit_x(&mut app, "64");
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["worldX"],
            48
        );
    }

    #[test]
    fn off_depth_level_form_locks_values_and_keeps_group_actions() {
        let (mut app, ctx) = editor();
        app.panel = Some(Panel::Level);
        app.world_depth = 1;
        let original = app.project.as_ref().unwrap().json.clone();
        frame(&mut app, &ctx, Panel::Level, vec![]);
        let stripes = egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let texture = loop {
            if let egui::load::TexturePoll::Ready { texture } =
                stripes.load_for_size(&ctx, Vec2::splat(8.)).unwrap()
            {
                break texture;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "source stripe texture should be ready"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(shape)
                    if shape.fill == GROUP_BAR && shape.rect.height() == 28.)
                })
                .count(),
            2,
            "inactive depth retains both full-color section headers"
        );
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == texture.id && mesh.calc_bounds().height() > 300.)
            }),
            "source hatch spans the full inactive form"
        );
        click(&mut app, &ctx, Panel::Level, "auto");
        assert_eq!(app.project.as_ref().unwrap().json, original);

        let output = click(
            &mut app,
            &ctx,
            Panel::Level,
            "LEVEL PROPERTIES: \"Level_0\"",
        );
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().replace('\n', " ") == "World coords")
        }), "inactive properties may still collapse");
        click(&mut app, &ctx, Panel::Level, "EDIT CUSTOM FIELDS");
        assert_eq!(app.panel, Some(Panel::Fields));
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
    #[test]
    fn source_linear_level_coords_show_runtime_positions_without_editing_saved_sentinels() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["worldLayout"] = json!("LinearHorizontal");
        let key = app.selected.unwrap();
        let level = app.project.as_mut().unwrap().level_mut(key).unwrap();
        level["worldX"] = json!(-1);
        level["worldY"] = json!(-1);
        let original = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "-1")
        }));
        label(&output, "0");
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
    #[test]
    fn source_linear_depth_arrow_preserves_saved_position_sentinels() {
        let (mut app, ctx) = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_level(None, "Other", 256, 256).unwrap();
        project.json["worldLayout"] = json!("LinearHorizontal");
        let level = project.level_mut(key).unwrap();
        level["worldX"] = json!(-1);
        level["worldY"] = json!(-1);
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let [pos, _] = depth_arrow_centers(&output);
        click_at(&mut app, &ctx, Panel::Level, pos);
        let level = app.project.as_ref().unwrap().level(key).unwrap();
        assert_eq!(level["worldDepth"], 1, "the arrow must change depth");
        assert_eq!(
            (level["worldX"].clone(), level["worldY"].clone()),
            (json!(-1), json!(-1))
        );
    }

    #[test]
    fn lone_level_depth_arrows_can_move_toward_zero() {
        for initial in [-1, 1] {
            let (mut app, ctx) = editor();
            let key = app.selected.unwrap();
            let level = app.project.as_mut().unwrap().level_mut(key).unwrap();
            level["worldDepth"] = json!(initial);
            app.world_depth = initial;

            let output = frame(&mut app, &ctx, Panel::Level, vec![]);
            let [further, closer] = depth_arrow_centers(&output);
            let (away, toward) = if initial < 0 {
                (closer, further)
            } else {
                (further, closer)
            };
            click_at(&mut app, &ctx, Panel::Level, away);
            assert_eq!(app.world_depth, initial);
            assert!(!app.dirty);

            click_at(&mut app, &ctx, Panel::Level, toward);
            assert_eq!(app.world_depth, 0, "starting at depth {initial}");
            let level = app.project.as_ref().unwrap().level(key).unwrap();
            assert_eq!(level["worldDepth"], 0);
            assert!(app.dirty);
        }
    }

    #[test]
    fn source_world_duplicate_cancel_and_confirm_use_existing_transaction() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        let pos = egui::pos2(325., label(&output, "CREATE A NEW LEVEL").y);
        click_at(&mut app, &ctx, Panel::World, pos);
        assert!(world_form_dialog::is_open(&ctx));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        click(&mut app, &ctx, Panel::World, "CANCEL");
        assert_eq!(app.project.as_ref().unwrap().json, before);
        click_at(&mut app, &ctx, Panel::World, pos);
        click(&mut app, &ctx, Panel::World, "CONFIRM");
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level_keys().len(), 2);
        let level = project.level(app.selected.unwrap()).unwrap();
        assert_eq!(
            (level["worldX"].as_i64(), level["worldY"].as_i64()),
            (Some(64), Some(64))
        );
    }
    #[test]
    fn source_level_iid_copy_uses_clipboard_and_does_not_edit() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let iid = before["levels"][0]["iid"].as_str().unwrap();
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let pos = egui::pos2(441., label(&output, "IID").y);
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let output = frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command,egui::OutputCommand::CopyText(text) if text==iid))
        );
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
    #[test]
    fn source_world_layout_picker_filters_and_commits_from_keyboard() {
        let (mut app, ctx) = editor();
        let output = click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        label(&output, "Search");
        for (_, label_text) in WORLD_LAYOUTS {
            label(&output, label_text);
        }
        let output = frame(
            &mut app,
            &ctx,
            Panel::World,
            vec![egui::Event::Text("grdvna".into())],
        );
        label(&output, WORLD_LAYOUTS[1].1);
        assert!(find_label(&output, WORLD_LAYOUTS[2].1).is_none());
        frame(
            &mut app,
            &ctx,
            Panel::World,
            vec![key(Key::Escape, egui::Modifiers::NONE)],
        );
        assert_eq!(app.project.as_ref().unwrap().json["worldLayout"], "Free");

        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        frame(
            &mut app,
            &ctx,
            Panel::World,
            vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
        );
        frame(
            &mut app,
            &ctx,
            Panel::World,
            vec![key(Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["worldLayout"],
            "GridVania"
        );
    }
    #[test]
    fn source_world_layout_confirmation_uses_friendly_labels_and_conditional_grid() {
        let (mut app, ctx) = editor();
        let project = app.project.as_mut().unwrap();
        project.add_level(None, "Second", 256, 256).unwrap();
        project.add_level(None, "Third", 256, 256).unwrap();
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[1].1);
        for _ in 0..12 {
            frame(&mut app, &ctx, Panel::World, vec![]);
        }
        let warning = frame(&mut app, &ctx, Panel::World, vec![]);
        assert!(world_form_dialog::is_open(&ctx));
        fn red_frame(shape: &egui::Shape) -> Option<Rect> {
            match shape {
                egui::Shape::Rect(rect)
                    if rect.fill == LEVEL_ACTION_RED && rect.rect.width() > 300. =>
                {
                    Some(rect.rect)
                }
                egui::Shape::Vec(children) => children.iter().find_map(red_frame),
                _ => None,
            }
        }
        let warning_rect = warning
            .shapes
            .iter()
            .find_map(|shape| red_frame(&shape.shape))
            .expect("red layout warning backdrop");
        assert!(
            (warning_rect.min - egui::pos2(0., 16.)).abs().max_elem() <= 1.,
            "{warning_rect:?}"
        );
        assert!(
            (warning_rect.width() - 400.).abs() <= 1.,
            "{warning_rect:?}"
        );
        assert_eq!(app.project.as_ref().unwrap().json["worldLayout"], "Free");
        click(&mut app, &ctx, Panel::World, "CANCEL");
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[0].1);
        click(&mut app, &ctx, Panel::World, WORLD_LAYOUTS[1].1);
        click(&mut app, &ctx, Panel::World, "CONFIRM");
        assert_eq!(
            app.project.as_ref().unwrap().json["worldLayout"],
            "GridVania"
        );
        let output = frame(&mut app, &ctx, Panel::World, vec![]);
        label(&output, "Gridvania grid size");
        assert!(app.last_chance.is_some());
    }
    #[test]
    fn focused_numeric_inputs_commit_arrow_steps_in_level_and_world_forms() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["defaultLevelWidth"] = json!(320);
        let level = app.selected.unwrap();
        let view = frame(&mut app, &ctx, Panel::Level, vec![]);
        click_at(
            &mut app,
            &ctx,
            Panel::Level,
            egui::pos2(156., label(&view, "World coords").y),
        );
        for (arrow, expected) in [(Key::ArrowUp, 1), (Key::ArrowDown, 0)] {
            frame(
                &mut app,
                &ctx,
                Panel::Level,
                vec![key(arrow, egui::Modifiers::NONE)],
            );
            assert_eq!(
                app.project.as_ref().unwrap().level(level).unwrap()["worldX"],
                expected
            );
        }
        let view = frame(&mut app, &ctx, Panel::Level, vec![]);
        click_at(
            &mut app,
            &ctx,
            Panel::Level,
            egui::pos2(156., label(&view, "Size").y),
        );
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![key(Key::ArrowUp, egui::Modifiers::NONE)],
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(level).unwrap()["pxWid"],
            257
        );

        app.show_panel(Some(Panel::World));
        click(&mut app, &ctx, Panel::World, "320");
        frame(
            &mut app,
            &ctx,
            Panel::World,
            vec![key(Key::ArrowDown, egui::Modifiers::NONE)],
        );
        assert_eq!(app.project.as_ref().unwrap().json["defaultLevelWidth"], 319);
    }

    #[test]
    fn typed_world_coordinates_use_source_integer_parsing_and_bounds() {
        let (mut app, ctx) = editor();
        let level = app.selected.unwrap();
        for (typed, expected) in [
            ("40000", 32767),
            ("-40000", -32768),
            ("-37.8", -37),
            ("", 0),
        ] {
            let view = frame(&mut app, &ctx, Panel::Level, vec![]);
            click_at(
                &mut app,
                &ctx,
                Panel::Level,
                egui::pos2(156., label(&view, "World coords").y),
            );
            replace_text(&mut app, &ctx, Panel::Level, typed);
            assert_eq!(
                app.project.as_ref().unwrap().level(level).unwrap()["worldX"],
                expected
            );
        }
    }

    #[test]
    fn source_level_size_entry_clamps_minimum_and_preserves_recovery() {
        let (mut app, ctx) = editor();
        let key = app.selected.unwrap();
        let project = app.project.as_mut().unwrap();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let normal = project.add_entity_def("Normal", 16, 16).unwrap();
        let allowed = project.add_entity_def("Allowed", 16, 16).unwrap();
        project
            .update_entity_definition(allowed, &json!({"allowOutOfBounds":true}))
            .unwrap();
        for (uid, x) in [(normal, 15), (normal, 16), (normal, 32), (allowed, 48)] {
            assert!(project.add_entity(key, 0, uid, x, 24).unwrap());
        }
        let output = frame(&mut app, &ctx, Panel::Level, vec![]);
        let pos = egui::pos2(156., label(&output, "Size").y);
        click_at(&mut app, &ctx, Panel::Level, pos);
        replace_text(&mut app, &ctx, Panel::Level, "1");
        assert_eq!(app.project.as_ref().unwrap().json["levels"][0]["pxWid"], 16);
        assert!(app.last_chance.is_some());
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let notice = |app: &mut EditorApp| {
            let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 800.));
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| app.canvas_notifications(ctx, rect),
            )
        };
        notice(&mut app);
        label(&notice(&mut app), "2 entity(ies) deleted during resizing!");
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        label(&notice(&mut app), "调整关卡尺寸时删除了 2 个实体！");
        app.restore_last_chance();
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["pxWid"],
            256
        );
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
    #[test]
    fn source_identifier_draft_is_tracked_and_commits_before_panel_switch() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["levels"][0]["useAutoIdentifier"] = json!(false);
        click(&mut app, &ctx, Panel::Level, "Level_0");
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![key(
                Key::A,
                egui::Modifiers {
                    command: true,
                    ctrl: true,
                    ..Default::default()
                },
            )],
        );
        frame(
            &mut app,
            &ctx,
            Panel::Level,
            vec![egui::Event::Text("Draft level".into())],
        );
        assert!(ui_commit::has_pending_ui_edit(&ctx));
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "Level_0"
        );
        app.show_panel(Some(Panel::Help));
        frame(&mut app, &ctx, Panel::Level, vec![]);
        frame(&mut app, &ctx, Panel::Level, vec![]);
        app.flush_ui_action(&ctx);
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "Draft_level"
        );
        assert_eq!(app.panel, Some(Panel::Help));
    }
}
