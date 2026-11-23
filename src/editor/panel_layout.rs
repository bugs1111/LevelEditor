use super::*;
use std::collections::BTreeMap;

#[path = "external_enum_panel.rs"]
mod external_enum_panel;
#[path = "layer_settings.rs"]
mod layer_settings;
#[path = "rule_list.rs"]
mod rule_list;

#[path = "project_panel.rs"]
mod project_panel;
#[path = "world_level_panel.rs"]
mod world_level_panel;

const LIST_WIDTH: f32 = 250.;
const TILESET_LIST_WIDTH: f32 = 300.;
const FIELD_LIST_WIDTH: f32 = 200.;
const FIELD_LIST_GAP: f32 = 8.;
const FIELD_LABEL_WIDTH: f32 = 130.;
const SOURCE_ORANGE: Color32 = Color32::from_rgb(235, 78, 0);
const GROUP_BAR: Color32 = Color32::from_rgb(115, 127, 157);
fn active_id() -> egui::Id {
    egui::Id::new("source-panel-layout-active")
}
fn panel_kind(panel: Panel) -> Option<&'static str> {
    match panel {
        Panel::Entities => Some("entities"),
        Panel::Layers | Panel::Rules => Some("layers"),
        Panel::Enums => Some("enums"),
        Panel::Tilesets => Some("tilesets"),
        _ => None,
    }
}
fn title(panel: Panel) -> &'static str {
    match panel {
        Panel::Entities => "PROJECT ENTITIES",
        Panel::Layers => "PROJECT LAYERS",
        Panel::Enums => "PROJECT ENUMS",
        Panel::Tilesets => "PROJECT TILESETS",
        Panel::Rules => "AUTO-LAYER RULES",
        Panel::Fields => "LEVEL CUSTOM FIELDS",
        Panel::Project => "PROJECT SETTINGS",
        Panel::World => "WORLD",
        Panel::Level => "CURRENT LEVEL",
        Panel::Export => "PROJECT EXPORT",
        Panel::Help => "LDTK HELP",
        _ => "",
    }
}
fn image(panel: Panel) -> egui::ImageSource<'static> {
    match panel {
        Panel::Entities => egui::include_image!("../../assets/icons/entity.png"),
        Panel::Layers => egui::include_image!("../../assets/icons/layer.png"),
        Panel::Rules => egui::include_image!("../../assets/icons/rule.svg"),
        Panel::Enums => egui::include_image!("../../assets/icons/enum.png"),
        Panel::Tilesets => egui::include_image!("../../assets/icons/tile.png"),
        Panel::World => egui::include_image!("../../assets/icons/world.svg"),
        Panel::Level | Panel::Fields => egui::include_image!("../../assets/icons/level.svg"),
        Panel::Help => egui::include_image!("../../assets/icons/help.svg"),
        _ => egui::include_image!("../../assets/icons/projectSettings.png"),
    }
}
fn source_panel_header(
    ui: &mut egui::Ui,
    panel: Panel,
    width: f32,
    header_height: f32,
    heading: &str,
    backup_locked: bool,
) -> bool {
    let rules = panel == Panel::Rules;
    let (rect, header) = ui.allocate_exact_size(egui::vec2(width, header_height), Sense::hover());
    let corners = egui::CornerRadius {
        ne: 4,
        ..egui::CornerRadius::ZERO
    };
    let painter = ui.painter();
    painter.rect_filled(rect, corners, theme::LIGHT);
    painter
        .with_clip_rect(Rect::from_min_max(
            rect.min,
            egui::pos2(rect.right(), rect.top() + 1.),
        ))
        .rect_filled(rect, corners, Color32::from_rgb(115, 127, 157));
    let icon_size = if rules { 22. } else { 40. };
    let icon = egui::Image::new(image(panel)).fit_to_exact_size(Vec2::splat(icon_size));
    let icon = if panel == Panel::Fields {
        icon.tint(theme::ACCENT)
    } else {
        icon
    };
    // The header owns the layout height. Its controls do not advance the body cursor.
    ui.place(
        Rect::from_min_size(
            rect.min + egui::vec2(20., (header_height - icon_size) / 2.),
            Vec2::splat(icon_size),
        ),
        icon,
    );
    ui.painter().text(
        rect.min + egui::vec2(if rules { 54. } else { 72. }, header_height / 2.),
        Align2::LEFT_CENTER,
        heading,
        FontId::new(20., egui::FontFamily::Name("panel-header".into())),
        Color32::WHITE,
    );
    let close_icon_size = 22.667;
    let close_width = close_icon_size + if rules { 8. } else { 14. };
    let close_rect = Rect::from_min_size(
        rect.right_top() + egui::vec2(-21. - close_width, 8.),
        egui::vec2(close_width, if rules { close_width } else { 38. }),
    );
    let sense = if backup_locked { Sense::hover() } else { Sense::click() };
    let close = ui.interact(close_rect, header.id.with("close"), sense);
    if !backup_locked {
        close.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    close.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            close.enabled() && !backup_locked,
            locale::tr(ui, "Close"),
        )
    });
    let hovered = !backup_locked && close.hovered();
    ui.painter().rect_filled(
        close_rect.translate(egui::vec2(0., 1.)),
        3.,
        Color32::from_black_alpha(51),
    );
    ui.painter().rect(
        close_rect,
        3.,
        if hovered { Color32::WHITE } else { theme::LIGHT },
        Stroke::new(1., if hovered { Color32::WHITE } else { theme::MEDIUM }),
        StrokeKind::Inside,
    );
    let color = if backup_locked {
        Color32::from_white_alpha(51)
    } else if hovered {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    egui::Image::new(egui::include_image!("../../assets/icons/close.png"))
        .tint(color)
        .paint_at(
            ui,
            Rect::from_center_size(close_rect.center(), Vec2::splat(close_icon_size)),
        );
    if backup_locked {
        source_controls::disabled_hatch(ui, close_rect);
    }
    !backup_locked && close.clicked()
}

fn orange(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(RichText::new(locale::tr(ui, label)).color(Color32::WHITE))
            .fill(SOURCE_ORANGE)
            .corner_radius(2.),
    )
}
fn source_menu(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(5., 6.);
        ui.spacing_mut().interact_size.x = 18.;
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.visuals_mut().widgets.inactive.bg_stroke =
            Stroke::new(1_f32, Color32::from_black_alpha(45));
        let response = ui.menu_button(" ", body).response;
        for y in [-4., 0., 4.] {
            ui.painter()
                .circle_filled(response.rect.center() + egui::vec2(0., y), 1., theme::TEXT);
        }
    });
}

fn source_definition_menu(
    ui: &mut egui::Ui,
    size: Option<Vec2>,
    row: &egui::Response,
    mut body: impl FnMut(&mut egui::Ui),
) {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(5., 6.);
        ui.spacing_mut().interact_size.x = 18.;
        let fill = if size.is_some() {
            ui.spacing_mut().button_padding.y = 0.;
            Color32::from_black_alpha(25)
        } else {
            Color32::TRANSPARENT
        };
        ui.visuals_mut().widgets.inactive.weak_bg_fill = fill;
        ui.visuals_mut().widgets.inactive.bg_fill = fill;
        ui.visuals_mut().widgets.inactive.bg_stroke =
            Stroke::new(1., Color32::from_black_alpha(45));
        let mut button = egui::Button::new(" ");
        if let Some(size) = size {
            button = button.min_size(size);
        }
        let response = ui.add(button);
        let menu = egui::Popup::menu(&response)
            .align(egui::RectAlign::BOTTOM_START)
            .frame(source_controls::context_menu_frame(ui.style()))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(&mut body);
        let context = egui::Popup::context_menu(row)
            .frame(source_controls::context_menu_frame(ui.style()))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(10.)
            .show(body);
        let open = menu.is_some() || context.is_some();
        let icon = if open {
            egui::include_image!("../../assets/icons/contextMenuOpen.svg")
        } else {
            egui::include_image!("../../assets/icons/contextMenu.svg")
        };
        egui::Image::new(icon).tint(theme::TEXT).paint_at(
            ui,
            Rect::from_center_size(response.rect.center(), Vec2::splat(13.)),
        );
    });
}
fn tint(color: Color32, toward: Color32, ratio: f32) -> Color32 {
    Color32::from_rgb(
        (color.r() as f32 * (1. - ratio) + toward.r() as f32 * ratio) as u8,
        (color.g() as f32 * (1. - ratio) + toward.g() as f32 * ratio) as u8,
        (color.b() as f32 * (1. - ratio) + toward.b() as f32 * ratio) as u8,
    )
}
fn source_field_number(raw: &str, kind: &str) -> Value {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map_or(Value::Null, |value| {
            if kind == "Int" {
                json!(value.trunc() as i32)
            } else {
                json!(value)
            }
        })
}
fn source_file_types(raw: &str) -> Value {
    let tokens = raw
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | '.'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Value::Null;
    }
    let mut values = Vec::<String>::new();
    for token in tokens {
        let extension = token
            .strip_prefix('.')
            .unwrap_or(token)
            .to_ascii_lowercase();
        if !extension.is_empty()
            && !extension.contains("..")
            && extension.bytes().any(|c| c.is_ascii_alphanumeric())
        {
            let value = format!(".{extension}");
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    json!(values)
}

fn row(ui: &mut egui::Ui, label: &str, body: impl FnOnce(&mut egui::Ui)) {
    row_width(ui, label, 124., body);
}
fn row_width(ui: &mut egui::Ui, label: &str, label_width: f32, body: impl FnOnce(&mut egui::Ui)) {
    row_width_with_label(
        ui,
        label_width,
        |ui| source_controls::property_label_with_width(ui, label, label, label_width),
        body,
    );
}
fn row_width_with_label(
    ui: &mut egui::Ui,
    label_width: f32,
    label: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 30.),
            egui::Layout::right_to_left(egui::Align::Center),
            label,
        );
        ui.add_space(10.);
        ui.allocate_ui_with_layout(
            egui::vec2((ui.available_width() - 2.).max(40.), 30.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                body(ui);
            },
        );
    });
}
fn source_checkbox(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    source_controls::checkbox(ui, value, label)
}

fn source_text(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let (mut original, mut draft) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .unwrap_or_else(|| (current.into(), current.into()));
    if original != current {
        original = current.into();
        draft = current.into();
    }
    let response = ui.add_sized([width, 33.], egui::TextEdit::singleline(&mut draft).id(id));
    let commit = (response.lost_focus()
        || response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
        && draft != current;
    ui.data_mut(|d| d.insert_temp(id, (original, draft.clone())));
    commit.then_some(draft)
}
fn section(
    ui: &mut egui::Ui,
    title: impl Into<egui::WidgetText>,
    mut open: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut hovered = false;
    let frame = egui::Frame::new()
        .stroke(Stroke::new(1_f32, theme::LIGHT))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.), Sense::click());
            hovered = response.hovered();
            // The source custom collapse cursor uses zoom-out as its native fallback.
            let response = response.on_hover_cursor(if open {
                egui::CursorIcon::ZoomOut
            } else {
                egui::CursorIcon::Cell
            });
            ui.painter().rect_filled(
                rect,
                0.,
                if hovered {
                    theme::ACCENT
                } else if open {
                    GROUP_BAR
                } else {
                    theme::LIGHT
                },
            );
            let icon = if open {
                egui::include_image!("../../assets/icons/expanded.svg")
            } else {
                egui::include_image!("../../assets/icons/collapsed.svg")
            };
            egui::Image::new(icon).paint_at(
                ui,
                Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(24.)),
            );
            let ink = if open || hovered { Color32::BLACK } else { theme::ACCENT };
            let title = locale::widget(ui, title.into()).color(ink).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                (rect.width() - 24.).max(0.),
                FontId::new(17.333, egui::FontFamily::Name("panel-title".into())),
            );
            ui.painter().galley(
                rect.left_center() + egui::vec2(24., -title.size().y / 2.),
                title,
                ink,
            );
            if response.clicked() {
                open = !open;
            }
            if open {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(0, 4))
                    .show(ui, body);
            }
        });
    if hovered {
        ui.painter().rect_stroke(
            frame.response.rect,
            0.,
            Stroke::new(1_f32, theme::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    open
}
impl EditorApp {
    pub(super) fn source_panel_active(&self, ui: &egui::Ui) -> bool {
        ui.ctx()
            .data_mut(|d| d.get_temp::<bool>(active_id()))
            .unwrap_or(false)
    }
    #[cfg(test)]
    pub(in crate::app) fn source_panel(&mut self, ctx: &egui::Context, panel: Panel) -> bool {
        self.source_panel_with_canvas_input(ctx, panel, false)
    }
    pub(in crate::app) fn source_panel_with_canvas_input(
        &mut self,
        ctx: &egui::Context,
        panel: Panel,
        level_point_picker_was_open: bool,
    ) -> bool {
        self.prepare_color_context(ctx);
        if !matches!(
            panel,
            Panel::Entities
                | Panel::Layers
                | Panel::Enums
                | Panel::Tilesets
                | Panel::Rules
                | Panel::Fields
                | Panel::Project
                | Panel::World
                | Panel::Level
                | Panel::Export
                | Panel::Help
        ) {
            return false;
        }
        if let Some(kind) = panel_kind(panel)
            && let Some(project) = self.project.as_ref()
        {
            let defs = project.json["defs"][kind]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let allowed = |def: &&Value| {
                panel != Panel::Rules || def["type"] == "IntGrid" || def["type"] == "AutoLayer"
            };
            if panel == Panel::Enums {
                let internal = defs
                    .iter()
                    .any(|d| d["uid"].as_i64() == self.definition_uid);
                let external = project.json["defs"]["externalEnums"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|d| d["uid"].as_i64() == self.definition_uid));
                if !internal && !external {
                    self.definition_uid = defs.first().and_then(|d| d["uid"].as_i64());
                }
            } else if !defs
                .iter()
                .filter(allowed)
                .any(|d| d["uid"].as_i64() == self.definition_uid)
            {
                let preferred = if panel == Panel::Entities {
                    self.entity_uid
                } else {
                    self.selected
                        .and_then(|key| project.layer(key, self.layer))
                        .and_then(|li| li["layerDefUid"].as_i64())
                };
                self.definition_uid = preferred
                    .filter(|uid| defs.iter().filter(allowed).any(|d| d["uid"] == *uid))
                    .or_else(|| defs.iter().find(allowed).and_then(|d| d["uid"].as_i64()));
            }
        }
        let screen = ctx.content_rect();
        let fixed_width = matches!(
            panel,
            Panel::Project
                | Panel::World
                | Panel::Level
                | Panel::Layers
                | Panel::Entities
                | Panel::Enums
                | Panel::Tilesets
                | Panel::Fields
        );
        let source_width = source_panel_width(panel);
        let width = if fixed_width {
            source_width
        } else {
            source_width.min(screen.width())
        };
        let scroll_panel = matches!(panel, Panel::Project | Panel::Level | Panel::Layers | Panel::Tilesets);
        let wrapper_width = width + if scroll_panel { 10. } else { 0. };
        let scroll_x = fixed_width && wrapper_width > screen.width();
        let outer_scroll = scroll_panel || scroll_x;
        let panel_width = wrapper_width.min(screen.width());
        let rules = panel == Panel::Rules;
        let backup_locked = matches!(
            panel,
            Panel::Project | Panel::World | Panel::Level | Panel::Layers | Panel::Entities
                | Panel::Fields | Panel::Enums | Panel::Tilesets | Panel::Rules
        ) && self.project.as_ref().is_some_and(Project::is_backup);
        let heading = if rules {
            self.project
                .as_ref()
                .and_then(|p| p.layer_def(self.definition_uid?))
                .map(|def| {
                    let name = text(def, "identifier", "").to_uppercase();
                    locale::format_ctx(ctx, "RULES: {name}", &[("name", &name)])
                })
                .unwrap_or_else(|| locale::tr_ctx(ctx, "RULES").to_owned())
        } else {
            locale::tr_ctx(ctx, title(panel)).to_owned()
        };
        let header_height = if rules { 46. } else { 53. };
        let single_column = matches!(panel, Panel::Project | Panel::World | Panel::Level);
        let top = if panel == Panel::World && self.zen {
            0.
        } else {
            34.
        };
        let height = (screen.height() - top).max(100.);
        let body_top = if rules {
            17
        } else if panel == Panel::Level {
            24
        } else {
            16
        };
        let bottom_margin = if rules {
            8
        } else if scroll_panel || matches!(panel, Panel::Entities | Panel::Enums | Panel::Fields) {
            20
        } else {
            0
        };
        let mut close = false;
        let panel_id = egui::Id::new("source-definition-panel");
        let dimmer = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, panel_id));
        let shade = (!rules && panel != Panel::World).then(|| dimmer.add(egui::Shape::Noop));
        let area = egui::Area::new(panel_id)
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(0., top))
            .constrain(false)
            .show(ctx, |ui| {
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(14.));
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Button, FontId::proportional(14.));
                if !outer_scroll {
                    ui.set_width(width);
                }
                ui.set_max_width(panel_width);
                ui.set_min_height(height);
                ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                egui::Frame::new()
                    .fill(if panel == Panel::World && !backup_locked {
                        theme::DARK
                    } else {
                        theme::MEDIUM
                    })
                    .corner_radius(4.)
                    .shadow(if scroll_panel || rules || panel == Panel::Help {
                        egui::epaint::Shadow {
                            offset: [8, 0],
                            blur: 0,
                            spread: 0,
                            color: if scroll_panel {
                                Color32::from_black_alpha(128)
                            } else {
                                theme::DARK
                            },
                        }
                    } else {
                        egui::epaint::Shadow::NONE
                    })
                    .show(ui, |ui| {
                        if panel == Panel::Help {
                            ui.set_height(height);
                            ui.spacing_mut().scroll = source_controls::scroll_style();
                            let content_visuals = source_controls::scroll_visuals(ui);
                            egui::ScrollArea::both()
                                .id_salt("source-help-panel")
                                .max_width(width)
                                .max_height(height)
                                .auto_shrink([false, false])
                                .scroll_source(egui::scroll_area::ScrollSource {
                                    scroll_bar: true,
                                    drag: false,
                                    mouse_wheel: true,
                                })
                                .show(ui, |ui| {
                                    *ui.visuals_mut() = content_visuals;
                                    let width = source_panel_width(Panel::Help) - 11.;
                                    ui.set_width(width);
                                    close |= source_panel_header(
                                        ui,
                                        panel,
                                        width,
                                        header_height,
                                        &heading,
                                        false,
                                    );
                                    egui::Frame::new()
                                        .inner_margin(egui::Margin {
                                            left: 20,
                                            right: 20,
                                            top: 32,
                                            bottom: 20,
                                        })
                                        .show(ui, |ui| {
                                            ui.set_width(width - 40.);
                                            self.help_contents(ui);
                                        });
                                });
                            return;
                        }
                        let mut contents = |ui: &mut egui::Ui, height: f32| {
                            let body_height = (height
                                - header_height
                                - f32::from(body_top)
                                - f32::from(bottom_margin))
                            .max(40.);
                            close |= source_panel_header(
                                ui, panel, width, header_height, &heading, backup_locked,
                            );
                            if backup_locked {
                                let cursor = ui.cursor().min;
                                source_controls::disabled_hatch(
                                    ui,
                                    Rect::from_min_max(
                                        cursor,
                                        egui::pos2(cursor.x + width, screen.bottom()),
                                    ),
                                );
                                ui.visuals_mut().disabled_alpha = 0.2;
                                ui.disable();
                            }
                            let world_panel = panel == Panel::World;
                            egui::Frame::new()
                                .inner_margin(if world_panel {
                                    egui::Margin::ZERO
                                } else {
                                    egui::Margin {
                                        left: if rules { 8 } else { 20 },
                                        right: if rules { 8 } else { 20 },
                                        top: body_top,
                                        bottom: bottom_margin,
                                    }
                                })
                                .show(ui, |ui| {
                                    if rules {
                                        ui.set_width((width - 16.).max(0.));
                                        self.rule_list(ui, body_height);
                                        return;
                                    }
                                    if single_column {
                                        let inner_width = if world_panel { width } else { width - 40. };
                                        ui.set_width(inner_width);
                                        ui.set_max_width(inner_width);
                                        ui.ctx()
                                            .data_mut(|data| data.insert_temp(active_id(), true));
                                        if world_panel {
                                            ui.spacing_mut().scroll = source_controls::scroll_style();
                                            ui.visuals_mut().clip_rect_margin = 0.;
                                            let scroll_height = height - header_height;
                                            // Keep animated scrollbar width inside the fixed panel.
                                            let (_, rect) = ui
                                                .allocate_space(egui::vec2(inner_width, scroll_height));
                                            let mut viewport =
                                                ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                            viewport.set_clip_rect(rect.intersect(viewport.clip_rect()));
                                            egui::ScrollArea::vertical()
                                                .id_salt(("source-panel-body", title(panel)))
                                                .max_height(scroll_height)
                                                .auto_shrink([false, false])
                                                .show(&mut viewport, |ui| {
                                                    egui::Frame::new().inner_margin(16).show(ui, |ui| {
                                                        let content_width = ui.available_width();
                                                        ui.set_width(content_width);
                                                        ui.set_max_width(content_width);
                                                        self.source_world_form(ui);
                                                    });
                                                });
                                        } else if panel == Panel::Level {
                                            self.source_level_form(ui);
                                        } else {
                                            self.source_project_form(ui);
                                        }
                                        ui.ctx()
                                            .data_mut(|data| data.insert_temp(active_id(), false));
                                        return;
                                    }
                                    let (list_width, gap) = match panel {
                                        Panel::Entities | Panel::Enums | Panel::Layers => {
                                            (LIST_WIDTH, 16.)
                                        }
                                        Panel::Tilesets => (TILESET_LIST_WIDTH, 16.),
                                        Panel::Fields => (FIELD_LIST_WIDTH, FIELD_LIST_GAP),
                                        _ => (LIST_WIDTH.min(width * 0.28), 16.),
                                    };
                                    let right = (width - 40. - list_width - gap).max(80.);
                                    let sidebar = |app: &mut Self, ui: &mut egui::Ui, height| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(list_width, height),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_width(list_width);
                                                ui.set_min_height(height);
                                                app.source_sidebar(ui, panel, height);
                                            },
                                        );
                                    };
                                    let form = |app: &mut Self, ui: &mut egui::Ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(right, body_height),
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_width(right);
                                                ui.spacing_mut().item_spacing = egui::vec2(6., 8.);
                                                ui.ctx().data_mut(|d| d.insert_temp(active_id(), true));
                                                if scroll_panel {
                                                    app.properties(ui, panel);
                                                } else if panel == Panel::Enums {
                                                    app.source_enum_form(ui);
                                                } else if panel == Panel::Entities {
                                                    ui.set_width((right - 10.).max(60.));
                                                    app.source_entity_form(ui, body_height);
                                                } else {
                                                    egui::ScrollArea::vertical()
                                                        .id_salt(("source-panel-body", title(panel)))
                                                        .max_height(body_height)
                                                        .auto_shrink([false, false])
                                                        .show(ui, |ui| {
                                                            ui.set_width((right - 10.).max(60.));
                                                            app.properties(ui, panel);
                                                        });
                                                }
                                                ui.ctx().data_mut(|d| d.insert_temp(active_id(), false));
                                            },
                                        ).response.rect.height()
                                    };
                                    if scroll_panel {
                                        ui.set_width(width - 40.);
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                            let form_height = form(self, ui);
                                            ui.add_space(gap);
                                            sidebar(self, ui, form_height.max(body_height));
                                        });
                                    } else {
                                        ui.horizontal_top(|ui| {
                                            sidebar(self, ui, body_height);
                                            ui.add_space(gap);
                                            form(self, ui);
                                        });
                                    }
                                });
                        };
                        if outer_scroll {
                            ui.spacing_mut().scroll = source_controls::scroll_style();
                            let content_visuals = source_controls::scroll_visuals(ui);

                            let frame_id = ui.make_persistent_id(("definition-panel-frame", title(panel)));
                            let frame = ui.ctx().cumulative_frame_nr();
                            let reopened = ui.data_mut(|data| {
                                let previous = data.get_temp::<u64>(frame_id);
                                data.insert_temp(frame_id, frame);
                                previous.is_none_or(|previous| frame.saturating_sub(previous) > 1)
                            });
                            let mut scroll = egui::ScrollArea::new([scroll_x, scroll_panel])
                                .id_salt(("definition-panel-scroll", title(panel)))
                                .max_width(panel_width)
                                .max_height(height)
                                .auto_shrink([!scroll_x, false])
                                .scroll_source(egui::scroll_area::ScrollSource {
                                    scroll_bar: true,
                                    drag: false,
                                    mouse_wheel: true,
                                });
                            if reopened {
                                scroll = scroll.scroll_offset(Vec2::ZERO);
                            }
                            scroll.show_viewport(ui, |ui, viewport| {
                                *ui.visuals_mut() = content_visuals;
                                ui.set_width(width);
                                contents(ui, viewport.height());
                            });
                        } else {
                            contents(ui, height);
                        }
                    });
            });
        let right = if outer_scroll {
            area.response.rect.right().min(screen.right())
        } else {
            width
        };
        if backup_locked {
            let body = Rect::from_min_max(
                egui::pos2(0., top + header_height),
                egui::pos2(right, screen.bottom()),
            );
            if ctx.pointer_hover_pos().is_some_and(|pos| body.contains(pos)) {
                ctx.set_cursor_icon(egui::CursorIcon::NotAllowed);
            }
            let notice = locale::tr_ctx(ctx, "This panel is disabled for backup files.");
            let galley = dimmer.layout(
                notice.to_owned(),
                FontId::proportional(14.667),
                Color32::WHITE,
                (right - 48.).max(0.),
            );
            let center = egui::pos2(width.min(right) * 0.5, top + height * 0.5);
            let rect = Rect::from_center_size(center, galley.size() + Vec2::splat(32.));
            dimmer.add(
                egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(102),
                }
                .as_shape(rect, 4.),
            );
            dimmer.rect_filled(rect, 4., Color32::from_rgb(101, 148, 255));
            dimmer.galley(rect.min + Vec2::splat(16.), galley, Color32::WHITE);
        }
        if let Some(shade) = shade {
            let canvas_left = (screen.left() + chrome::SIDEBAR_WIDTH).min(screen.right());
            let mask = Rect::from_min_max(egui::pos2(canvas_left, screen.top()), screen.max);
            dimmer.set(
                shade,
                egui::Shape::rect_filled(mask, 0., Color32::from_black_alpha(89)),
            );
            let mask_press = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::PointerButton { pos, pressed: true, .. }
                        if mask.contains(*pos) && (pos.y < top || pos.x >= right) =>
                    {
                        Some(*pos)
                    }
                    _ => None,
                })
            });
            close |= !(panel == Panel::Level && level_point_picker_was_open)
                && mask_press.is_some_and(|pos| {
                    ctx.layer_id_at(pos)
                        .is_none_or(|layer| layer.order == egui::Order::Background)
                });
        }
        self.rule_wizard(ctx);
        if rules {
            self.rule_detail_dialog(ctx);
        }
        self.entity_transfer_dialog(ctx);
        self.group_remap_dialog(ctx);
        self.group_policy_dialog(ctx);
        self.source_field_type_picker(ctx);
        self.source_layer_type_picker(ctx);
        if close {
            self.dismiss_panel();
            ctx.request_repaint();
        }
        true
    }
    fn source_sidebar(&mut self, ui: &mut egui::Ui, panel: Panel, height: f32) {
        let width = match panel {
            Panel::Tilesets => TILESET_LIST_WIDTH,
            Panel::Fields => FIELD_LIST_WIDTH,
            _ => LIST_WIDTH,
        };
        let width = width.min(ui.available_width());
        ui.set_width(width);
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
        if panel == Panel::Rules {
            self.rule_list(ui, height);
            return;
        }
        if let Some(kind) = panel_kind(panel) {
            let tilesets = panel == Panel::Tilesets;
            let compact = matches!(panel, Panel::Layers | Panel::Tilesets);
            if compact || panel == Panel::Enums {
                ui.add_space(if compact { 3. } else { 2. });
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                let create = if compact || panel == Panel::Enums {
                    let size = if compact {
                        [ui.available_width(), 41.]
                    } else {
                        [ui.available_width() - 72., 43.]
                    };
                    source_controls::add_sized_button(
                        ui,
                        size,
                        egui::Button::image(
                            egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                                .fit_to_exact_size(Vec2::splat(20.)),
                        )
                        .fill(SOURCE_ORANGE)
                        .corner_radius(2.),
                    )
                } else {
                    let width = if panel == Panel::Entities {
                        165.
                    } else {
                        ui.available_width()
                    };
                    orange(ui, "⊞", egui::vec2(width, 41.))
                };
                if create
                    .on_hover_text(locale::tr(ui, "Create definition"))
                    .clicked()
                {
                    if panel == Panel::Layers {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::new("source-layer-type-picker"), true)
                        });
                    } else {
                        self.source_create_definition(kind, None);
                    }
                }
                if panel == Panel::Entities {
                    ui.scope(|ui| {
                        ui.spacing_mut().interact_size.y = 41.;
                        ui.spacing_mut().button_padding = egui::vec2(6., 10.);
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = SOURCE_ORANGE;
                        ui.visuals_mut().widgets.inactive.bg_fill = SOURCE_ORANGE;
                        ui.menu_button(
                            RichText::new(locale::tr(ui, "PRESETS")).color(Color32::WHITE),
                            |ui| {
                                if source_controls::button(ui, "Rectangle region").clicked() {
                                    self.source_create_definition("entities", Some(false));
                                    ui.close();
                                }
                                if source_controls::button(ui, "Circle region").clicked() {
                                    self.source_create_definition("entities", Some(true));
                                    ui.close();
                                }
                            },
                        );
                    });
                } else if panel == Panel::Enums {
                    ui.scope(|ui| {
                        ui.spacing_mut().interact_size = egui::vec2(70., 43.);
                        ui.spacing_mut().button_padding = egui::vec2(8., 6.);
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = SOURCE_ORANGE;
                        ui.visuals_mut().widgets.inactive.bg_fill = SOURCE_ORANGE;
                        self.import_enum_menu(ui, locale::tr(ui, "IMPORT"));
                    });
                }
            });
            let filter_id = ui.make_persistent_id(("source-definition-search", kind));
            let mut filter = ui
                .ctx()
                .data_mut(|d| d.get_temp::<String>(filter_id))
                .unwrap_or_default();
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1_f32, theme::LIGHT))
                .corner_radius(20.)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(egui::include_image!("../../assets/icons/search.svg"))
                                .fit_to_exact_size(Vec2::splat(16.)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut filter)
                                .frame(false)
                                .hint_text(locale::tr(ui, "Search…"))
                                .desired_width(ui.available_width()),
                        );
                    });
                });
            ui.ctx()
                .data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let definitions = self
                .project
                .as_ref()
                .and_then(|p| p.json["defs"][kind].as_array())
                .cloned()
                .unwrap_or_default();
            let no_tilesets = tilesets && definitions.is_empty();
            let mut groups = BTreeMap::<String, Vec<Value>>::new();
            for def in definitions {
                if !text(&def, "identifier", "")
                    .to_lowercase()
                    .contains(&filter.to_lowercase())
                {
                    continue;
                }
                if panel == Panel::Rules && def["type"] != "IntGrid" && def["type"] != "AutoLayer" {
                    continue;
                }
                let tags = def["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if tags.is_empty() {
                    groups
                        .entry(if tilesets {
                            String::new()
                        } else if matches!(panel, Panel::Entities | Panel::Enums) {
                            "Other".into()
                        } else {
                            kind.to_owned()
                        })
                        .or_default()
                        .push(def);
                } else {
                    for tag in tags {
                        groups.entry(tag).or_default().push(def.clone());
                    }
                }
            }
            let mut groups = groups.into_iter().collect::<Vec<_>>();
            if tilesets {
                groups.sort_by_cached_key(|(name, _)| name.to_lowercase());
            }
            let frame = if compact || panel == Panel::Enums {
                egui::Frame::new()
                    .fill(theme::DARK)
                    .stroke(Stroke::new(1., theme::LIGHT))
                    .inner_margin(4)
            } else {
                egui::Frame::NONE
            };
            let list_height = (height
                - match panel {
                    Panel::Layers | Panel::Tilesets => 100.,
                    Panel::Enums => 98.,
                    _ => 88.,
                })
            .max(40.);
            frame.show(ui, |ui| {
                if compact {
                    ui.set_width((width - 10.).max(1.));
                }
                if no_tilesets {
                    let rect = Rect::from_min_size(
                        ui.next_widget_position(),
                        egui::vec2(ui.available_width(), list_height),
                    )
                    .expand(4.);
                    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                        .paint_at(ui, rect);
                }
                egui::ScrollArea::vertical()
                    .id_salt(("source-definition-list", kind))
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if compact {
                            ui.set_min_height(list_height);
                        }
                        if no_tilesets {
                            ui.add_space(26.);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new(locale::tr(ui, "No tileset"))
                                        .font(FontId::new(17., egui::FontFamily::Name("panel-title".into())))
                                        .color(theme::LIGHT)
                                        .italics(),
                                );
                            });
                        }
                        let multiple_groups = groups.len() > 1;
                        for (name, definitions) in groups {
                            ui.set_width(width.min(ui.available_width()));
                            let show_header = if tilesets {
                                multiple_groups && filter.is_empty()
                            } else if panel == Panel::Enums {
                                multiple_groups
                            } else {
                                kind != "layers" || name != kind
                            };
                            self.source_definition_group(
                                ui,
                                kind,
                                &name,
                                &definitions,
                                show_header,
                            );
                        }
                        if panel == Panel::Enums {
                            let external = self
                                .project
                                .as_ref()
                                .and_then(|p| p.json["defs"]["externalEnums"].as_array())
                                .cloned()
                                .unwrap_or_default();
                            let mut sources = BTreeMap::<String, Vec<Value>>::new();
                            for def in external {
                                if text(&def, "identifier", "")
                                    .to_lowercase()
                                    .contains(&filter.to_lowercase())
                                {
                                    sources
                                        .entry(
                                            text(&def, "externalRelPath", "External enums")
                                                .to_owned(),
                                        )
                                        .or_default()
                                        .push(def);
                                }
                            }
                            for (path, defs) in sources {
                                self.source_external_enum_group(ui, &path, &defs);
                            }
                        }
                        self.definition_list_paste_menu(ui, ClipTarget::Definition {
                            kind: kind.to_owned(),
                            uid: None,
                        });
                    })
            });
        } else if panel == Panel::Fields {
            self.source_field_list(ui, self.field_owner, height);
        } else {
            for (label, target) in [
                ("Project settings", Panel::Project),
                ("Worlds", Panel::World),
                ("Levels", Panel::Level),
                ("Layers", Panel::Layers),
                ("Entities", Panel::Entities),
                ("Enums", Panel::Enums),
                ("Tilesets", Panel::Tilesets),
                ("Auto-layer rules", Panel::Rules),
                ("Export", Panel::Export),
            ] {
                if source_controls::add_sized_button(
                    ui,
                    [ui.available_width(), 36.],
                    egui::Button::new(locale::tr(ui, label)).selected(panel == target),
                )
                .clicked()
                {
                    self.show_panel(Some(target));
                }
            }
            if panel == Panel::Level || panel == Panel::World {
                let levels = self
                    .project
                    .as_ref()
                    .map(|p| {
                        p.level_keys()
                            .into_iter()
                            .filter_map(|key| {
                                p.level(key)
                                    .map(|l| (key, text(l, "identifier", "Level").to_owned()))
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                ui.separator();
                for (key, name) in levels {
                    if ui
                        .selectable_label(self.selected == Some(key), name)
                        .clicked()
                    {
                        self.navigate_level(key, true);
                    }
                }
            }
        }
    }
    fn source_definition_group(
        &mut self,
        ui: &mut egui::Ui,
        kind: &str,
        name: &str,
        definitions: &[Value],
        show_header: bool,
    ) {
        ui.set_width(ui.available_width());
        let enums = kind == "enums";
        let layers = kind == "layers";
        let tilesets = kind == "tilesets";
        let compact = layers || tilesets;
        let grouped_tilesets = tilesets && show_header;
        let row_height = if tilesets { 27. } else { 28. };
        let menu_width = if tilesets { 18. } else { 26. };
        let shared_frame = enums || compact;
        egui::Frame::new()
            .fill(if shared_frame {
                Color32::TRANSPARENT
            } else {
                theme::DARK
            })
            .stroke(if shared_frame {
                Stroke::NONE
            } else {
                Stroke::new(1., theme::LIGHT)
            })
            .inner_margin(if shared_frame { 0 } else { 4 })
            .show(ui, |ui| {
                let source_kind = match kind {
                    "entities" => "entity",
                    "tilesets" => "tileset",
                    "enums" => "enum",
                    _ => kind,
                };
                let key = format!(
                    "{}_{}_tag_{}",
                    self.project
                        .as_ref()
                        .map(|p| text(&p.json, "iid", ""))
                        .unwrap_or(""),
                    source_kind,
                    if name == "Other" || tilesets && name.is_empty() { "null" } else { name }
                );
                let mut open = !show_header || self.settings.ui_state_int(&key, None, 1) != 0;
                if show_header {
                    if grouped_tilesets {
                        ui.add_space(2.);
                    }
                    let (rect, response) = ui
                        .allocate_exact_size(
                            egui::vec2(ui.available_width(), if grouped_tilesets { 18. } else { 20. }),
                            Sense::click(),
                        );
                    ui.painter().rect_filled(rect, 0., GROUP_BAR);
                    let points = if open {
                        [
                            egui::vec2(7., -3.),
                            egui::vec2(12., 2.),
                            egui::vec2(17., -3.),
                        ]
                    } else {
                        [egui::vec2(9., -5.), egui::vec2(14., 0.), egui::vec2(9., 5.)]
                    };
                    for line in points.windows(2) {
                        ui.painter().line_segment(
                            [rect.left_center() + line[0], rect.left_center() + line[1]],
                            Stroke::new(2_f32, Color32::WHITE),
                        );
                    }
                    let heading = if tilesets && name.is_empty() {
                        locale::tr(ui, "Untagged")
                    } else if name == "Other"
                        && definitions.iter().all(|def| {
                            def["tags"]
                                .as_array()
                                .is_none_or(|tags| tags.iter().all(|tag| tag != name))
                        }) {
                        locale::tr(ui, if enums { "Untagged" } else { name })
                    } else {
                        name
                    };
                    ui.painter().text(
                        rect.left_center() + egui::vec2(24., 0.),
                        Align2::LEFT_CENTER,
                        heading.to_uppercase(),
                        FontId::new(14., egui::FontFamily::Name("panel-title".into())),
                        theme::ACCENT,
                    );
                    if response.clicked() {
                        open = !open;
                        self.settings.set_ui_state_int(&key, None, i64::from(open));
                    }
                }
                if open {
                    let margin = if grouped_tilesets {
                        egui::Margin {
                            left: 13,
                            right: 13,
                            top: 9,
                            bottom: 12,
                        }
                    } else {
                        egui::Margin::same(if layers { 0 } else { 4 })
                    };
                    egui::Frame::new()
                        .inner_margin(margin)
                        .show(ui, |ui| {
                            if compact {
                                ui.spacing_mut().item_spacing = egui::vec2(8., 1.);
                            }
                            let drag_group = tilesets.then(|| name.to_owned());
                            for def in definitions {
                                let uid = def["uid"].as_i64().unwrap_or(-1);
                                let selected = self.definition_uid == Some(uid);
                                let color_key = if kind == "layers" { "uiColor" } else { "color" };
                                let color = theme::color(text(def, color_key, "#687487"));
                                let layer_color = def["uiColor"].as_str().map(theme::color);
                                let fill = if layers {
                                    match (selected, layer_color) {
                                        (true, Some(color)) => color,
                                        (false, Some(color)) => tint(theme::DARK, color, 0.5),
                                        (true, None) => theme::ACCENT,
                                        (false, None) => theme::MEDIUM,
                                    }
                                } else if enums || tilesets {
                                    if selected { theme::ACCENT } else { theme::MEDIUM }
                                } else if selected {
                                    tint(color, Color32::WHITE, 0.65)
                                } else {
                                    tint(theme::MEDIUM, color, 0.22)
                                };
                                let item = egui::Frame::new()
                                    .fill(fill)
                                    .inner_margin(egui::Margin::symmetric(4, if compact { 3 } else { 0 }))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let grip = egui::Label::new(
                                                RichText::new("≡").color(theme::TEXT),
                                            )
                                            .sense(Sense::drag());
                                            let handle = if compact {
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2(16., row_height),
                                                    Sense::drag(),
                                                );
                                                egui::Image::new(egui::include_image!("../../assets/icons/dragAlpha.svg"))
                                                    .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.)));
                                                response
                                            } else if enums {
                                                ui.add_sized([19., 33.], grip)
                                            } else {
                                                ui.add(grip)
                                            };
                                            handle.dnd_set_drag_payload((kind.to_owned(), uid, drag_group.clone()));
                                            let label = text(def, "identifier", "Definition");
                                            let response = if compact {
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2((ui.available_width() - menu_width - 8.).max(32.), row_height),
                                                    Sense::click(),
                                                );
                                                let label_offset = if layers {
                                                    self.source_definition_thumbnail(
                                                        ui, def, kind,
                                                        Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(24.)),
                                                        color,
                                                    );
                                                    32.
                                                } else if !def["embedAtlas"].is_null() {
                                                    let icon = egui::Image::new(egui::include_image!("../../assets/icons/embed.svg"));
                                                    let icon = if selected { icon.tint(Color32::BLACK) } else { icon };
                                                    icon.paint_at(ui, Rect::from_center_size(
                                                        rect.left_center() + egui::vec2(8., 0.), Vec2::splat(16.),
                                                    ));
                                                    20.
                                                } else {
                                                    0.
                                                };
                                                let ink = if selected {
                                                    Color32::BLACK
                                                } else if let Some(color) = layer_color {
                                                    tint(color, Color32::WHITE, 0.3)
                                                } else {
                                                    theme::TEXT
                                                };
                                                let name = egui::WidgetText::from(RichText::new(label).color(ink));
                                                let galley = name.into_galley(
                                                    ui,
                                                    Some(egui::TextWrapMode::Truncate),
                                                    (rect.width() - label_offset).max(0.),
                                                    FontId::proportional(14.),
                                                );
                                                ui.painter().galley(
                                                    rect.left_center() + egui::vec2(label_offset, -galley.size().y / 2.),
                                                    galley,
                                                    ink,
                                                );
                                                response.widget_info(|| egui::WidgetInfo::selected(
                                                    egui::WidgetType::SelectableLabel, true, selected, label,
                                                ));
                                                response
                                            } else if enums {
                                                let (rect, response) = ui.allocate_exact_size(
                                                    egui::vec2((ui.available_width() - 21.).max(20.), 33.),
                                                    Sense::click(),
                                                );
                                                ui.painter_at(rect).text(
                                                    rect.left_center(),
                                                    Align2::LEFT_CENTER,
                                                    label,
                                                    FontId::proportional(14.),
                                                    if selected { Color32::BLACK } else { Color32::WHITE },
                                                );
                                                response
                                            } else {
                                                let (rect, _) = ui.allocate_exact_size(
                                                    Vec2::splat(32.),
                                                    Sense::hover(),
                                                );
                                                self.source_definition_thumbnail(
                                                    ui, def, kind, rect, color,
                                                );
                                                let available = (ui.available_width() - 42.).max(20.);
                                                source_controls::add_sized_button(
                                                    ui,
                                                    [available, 32.],
                                                    egui::Button::new(
                                                        RichText::new(label).color(if selected {
                                                            Color32::BLACK
                                                        } else {
                                                            tint(color, Color32::WHITE, 0.4)
                                                        }),
                                                    )
                                                    .truncate()
                                                    .frame(false),
                                                )
                                            };
                                            if response.clicked() {
                                                self.clipboard_focus(
                                                    ui.ctx(),
                                                    ClipTarget::Definition {
                                                        kind: kind.to_owned(),
                                                        uid: Some(uid),
                                                    },
                                                );
                                                if kind == "layers" {
                                                    self.select_layer_definition(Some(uid));
                                                } else {
                                                    self.definition_uid = Some(uid);
                                                }
                                                if kind == "entities" {
                                                    self.entity_uid = Some(uid);
                                                    self.field_owner = Some(uid);
                                                    self.field_uid = None;
                                                }
                                            }
                                            source_definition_menu(
                                                ui,
                                                compact.then_some(egui::vec2(menu_width, row_height)),
                                                &response,
                                                |ui| self.clipboard_menu(ui, ClipTarget::Definition {
                                                    kind: kind.to_owned(),
                                                    uid: Some(uid),
                                                }),
                                            );
                                        });
                                    });
                                if compact {
                                    let rect = item.response.rect;
                                    if let Some(color) = layer_color {
                                        ui.painter().rect_filled(
                                            Rect::from_min_max(egui::pos2(rect.right() - 4., rect.top()), rect.max),
                                            0.,
                                            if selected { Color32::WHITE } else { color },
                                        );
                                    }
                                    if !selected && item.response.contains_pointer() {
                                        ui.painter().rect_stroke(
                                            rect, 0., Stroke::new(2., Color32::from_white_alpha(166)),
                                            StrokeKind::Inside,
                                        );
                                    }
                                }
                                if let Some(payload) =
                                    item.response.dnd_hover_payload::<(String, i64, Option<String>)>()
                                    && payload.0 == kind
                                    && payload.1 != uid
                                    && payload.2 == drag_group
                                {
                                    ui.painter().hline(
                                        item.response.rect.x_range(),
                                        item.response.rect.top(),
                                        Stroke::new(2_f32, theme::ACCENT),
                                    );
                                }
                                if let Some(payload) =
                                    item.response.dnd_release_payload::<(String, i64, Option<String>)>()
                                    && payload.0 == kind
                                    && payload.1 != uid
                                    && payload.2 == drag_group
                                {
                                    self.source_reorder_definition(
                                        ui.ctx(),
                                        kind,
                                        payload.1,
                                        self.definition_index(kind, uid),
                                    );
                                }
                                if !compact {
                                    ui.add_space(3.);
                                }
                            }
                        });
                }
            });
        if !tilesets {
            ui.add_space(8.);
        } else if show_header {
            ui.add_space(3.);
        }
    }
    fn source_reorder_definition(&mut self, ctx: &egui::Context, kind: &str, uid: i64, index: usize) {
        let reordered = self.edit("Definition reordered", |p| {
            p.reorder_definition(kind, uid, index)
        });
        if reordered.is_some() && matches!(kind, "layers" | "tilesets") {
            if kind == "layers" {
                self.select_layer_definition(Some(uid));
            } else {
                self.definition_uid = Some(uid);
            }
            self.clipboard_focus(ctx, ClipTarget::Definition { kind: kind.to_owned(), uid: Some(uid) });
        }
    }
    pub(super) fn source_definition_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
        kind: &str,
        rect: Rect,
        color: Color32,
    ) {
        if kind == "layers" {
            egui::Image::new(layer_settings::layer_type_icon(text(def, "type", "")))
                .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(24.)));
            return;
        }
        let tile = if number(&def["uiTileRect"], "w", 0.) > 0. {
            &def["uiTileRect"]
        } else {
            &def["tileRect"]
        };
        let atlas = tile["tilesetUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.tileset(uid))
            .cloned();
        if let Some(atlas) = atlas {
            let texture = if atlas["embedAtlas"].is_null() {
                atlas["relPath"].as_str().and_then(|rel| {
                    self.assets.texture(
                        ui.ctx(),
                        &canvas::asset_path(self.project.as_ref().unwrap(), rel),
                    )
                })
            } else {
                self.assets.embedded(ui.ctx())
            };
            if let Some(texture) = texture {
                let size = texture.size_vec2();
                let uv = Rect::from_min_max(
                    egui::pos2(
                        number(tile, "x", 0.) / size.x,
                        number(tile, "y", 0.) / size.y,
                    ),
                    egui::pos2(
                        (number(tile, "x", 0.) + number(tile, "w", 16.)) / size.x,
                        (number(tile, "y", 0.) + number(tile, "h", 16.)) / size.y,
                    ),
                );
                ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
                ui.painter().rect_stroke(
                    rect,
                    0.,
                    Stroke::new(1_f32, color.gamma_multiply(0.6)),
                    StrokeKind::Inside,
                );
                return;
            }
        }
        ui.painter().rect_filled(
            rect.shrink(2.),
            0.,
            color.gamma_multiply(if kind == "entities" { 0.5 } else { 0.8 }),
        );
        ui.painter().rect_stroke(
            rect.shrink(2.),
            0.,
            Stroke::new(2_f32, color),
            StrokeKind::Inside,
        );
    }
    fn source_create_definition(&mut self, kind: &str, preset: Option<bool>) {
        if kind == "tilesets" {
            if let Some(path) = self.pick_project_file(
                "PickImage",
                &["png", "jpg", "jpeg", "gif", "ase", "aseprite"],
            ) {
                match ldtk_core::exports::load_image_rgba(&path) {
                    Ok(image) => {
                        let grid = self.new_grid;
                        if let Some(uid) = self.edit("Tileset imported", |p| {
                            p.add_tileset(&path, grid, image.width() as i32, image.height() as i32)
                        }) {
                            self.definition_uid = Some(uid);
                        }
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        self.status_error = true;
                    }
                }
            }
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let base = match (kind, preset) {
            ("entities", Some(true)) => "CircleRegion",
            ("entities", Some(false)) => "RectRegion",
            ("entities", None) => "Entity",
            ("layers", _) => "Layer",
            _ => "Enum",
        };
        let name = unique_name(
            base,
            project.json["defs"][kind]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| text(d, "identifier", "")),
        );
        let used = project.json["defs"][if kind == "entities" {
            "entities"
        } else {
            "enums"
        }]
        .as_array()
        .cloned()
        .unwrap_or_default();
        let color = self.suggested_color(&used);
        let result=match kind{"entities"=>self.edit("Entity created",|p|{let uid=p.add_entity_def(&name,number(&p.json,"defaultEntityWidth",16.)as i32,number(&p.json,"defaultEntityHeight",16.)as i32)?;p.update_entity_definition(uid,&json!({"color":format!("#{color:06X}")}))?;if let Some(circle)=preset{p.update_entity_definition(uid,&json!({"hollow":true,"resizableX":true,"resizableY":true,"keepAspectRatio":circle,"pivotX":if circle{0.5}else{0.},"pivotY":if circle{0.5}else{0.},"renderMode":if circle{"Ellipse"}else{"Rectangle"},"tags":["region"]}))?;}Ok(uid)}),"layers"=>{let kind=self.new_kind.clone();let grid=self.new_grid;self.edit("Layer created",|p|p.add_layer(&name,&kind,grid))},_=>self.edit("Enum created",|p|p.add_enum_definition(&name,&[]))};
        if let Some(uid) = result {
            if kind == "layers" {
                self.select_layer_definition(Some(uid));
            } else {
                self.definition_uid = Some(uid);
            }
            if kind == "entities" {
                self.entity_uid = Some(uid);
                self.field_owner = Some(uid);
                self.field_uid = None;
            }
        }
    }
    fn source_entity_form(&mut self, ui: &mut egui::Ui, height: f32) {
        let ctx = ui.ctx().clone();
        let key = ui.make_persistent_id(("entity-options-atlas", self.definition_uid));
        self.with_local_atlas_state(&ctx, key, |app| {
            ui.spacing_mut().scroll = source_controls::scroll_style();
            ui.visuals_mut().clip_rect_margin = 0.;
            let bottom = ui.cursor().top() + height;
            let settings_max = (height - 156.).max(height * 0.5);
            let settings = egui::ScrollArea::vertical()
                .id_salt("entity-settings")
                .max_height(settings_max)
                .auto_shrink([false, true])
                .show(ui, |ui| app.source_entity_form_inner(ui));
            let Some(uid) = settings.inner else {
                return;
            };
            ui.add_space(
                if app.settings.ui_state_int("extraEntityDetails", None, 1) == 0 {
                    10.
                } else {
                    22.
                },
            );
            let field_height = (bottom - ui.cursor().top()).max(0.);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                ui.allocate_ui_with_layout(
                    egui::vec2(FIELD_LIST_WIDTH, field_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(FIELD_LIST_WIDTH);
                        ui.set_max_width(FIELD_LIST_WIDTH);
                        app.source_field_list(ui, Some(uid), field_height);
                    },
                );
                ui.add_space(FIELD_LIST_GAP);
                let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), field_height));
                let mut field = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                field.set_clip_rect(field.clip_rect().intersect(rect));
                egui::ScrollArea::vertical()
                    .id_salt(("entity-field-properties", app.field_uid))
                    .max_height(field_height)
                    .auto_shrink([false, false])
                    .show(&mut field, |ui| {
                        ui.spacing_mut().text_edit_width = 170.;
                        app.field_properties(ui);
                    });
            });
        });
    }
    fn source_entity_form_inner(&mut self, ui: &mut egui::Ui) -> Option<i64> {
        let Some(def) = self.definition_picker(ui, "entities") else {
            ui.label(locale::tr(ui, "Create an entity using the + button."));
            return None;
        };
        let uid = def["uid"].as_i64().unwrap();
        let advanced_id = ui.make_persistent_id("entity-pivot-advanced");
        if self.field_owner != Some(uid) {
            self.field_owner = Some(uid);
            self.field_uid = None;
            ui.data_mut(|data| data.remove::<bool>(advanced_id));
        }
        let mut patch = json!({});
        row_width(ui, "Entity identifier", 108., |ui| {
            if let Some(name) = source_text(
                ui,
                ("entity-name", uid),
                text(&def, "identifier", ""),
                (ui.available_width() - 150.).max(80.),
            ) {
                patch["identifier"] = json!(name);
            }
            let mut show = def["showName"].as_bool().unwrap_or(true);
            if source_checkbox(ui, &mut show, "Show identifier").changed() {
                patch["showName"] = json!(show);
            }
        });
        ui.add_space(5.);
        let atlas = def["tilesetId"]
            .as_i64()
            .or_else(|| def["tileRect"]["tilesetUid"].as_i64())
            .unwrap_or(-1);
        let settings_open = self.settings.ui_state_int("extraEntityDetails", None, 1) != 0;
        let mut tag_change = None;
        let mut editor_atlas_choice = None;
        let mut ui_atlas_choice = None;
        let settings_next = section(ui, "ENTITY SETTINGS", settings_open, |ui| {
            ui.spacing_mut().item_spacing.y = 12.;
            row(ui, "Misc options", |ui| {
                for (key, label) in [
                    ("exportToToc", "Add to JSON Table-of-content"),
                    ("allowOutOfBounds", "Can be out of level bounds"),
                ] {
                    let mut value = def[key].as_bool().unwrap_or(false);
                    if source_checkbox(ui, &mut value, label).changed() {
                        patch[key] = json!(value);
                    }
                }
            });
            row(ui, "Documentation", |ui| {
                let mut doc = text(&def, "doc", "")
                    .replace("\\\\", "\\")
                    .replace("\\n", "\n");
                if ui
                    .add_sized(
                        [ui.available_width(), 32.],
                        egui::TextEdit::multiline(&mut doc).desired_rows(1),
                    )
                    .changed()
                {
                    patch["doc"] = json!(doc.replace('\\', "\\\\").replace('\n', "\\n"));
                }
            });
            row(ui, "Tags", |ui| {
                tag_change = tag_editor::show(
                    ui,
                    self.project.as_ref().unwrap(),
                    tag_editor::Field::Definition("entities"),
                    uid,
                    &def["tags"],
                );
            });
            row_width(ui, "Size", 136., |ui| {
                use source_controls::{NumberStyle, number_input_sized};
                ui.spacing_mut().item_spacing.x = 3.;
                for (key, separator) in [("width", ""), ("height", "x")] {
                    if !separator.is_empty() {
                        ui.weak(separator);
                    }
                    let current = def[key].as_i64().unwrap_or(16).to_string();
                    if let Some(draft) = number_input_sized(
                        ui,
                        key,
                        &current,
                        NumberStyle::Plain { placeholder: None },
                        60.,
                    ) {
                        patch[key] =
                            json!(world_level_panel::parse_form_int(&draft).clamp(1, 2048));
                    }
                }
                if def["resizableX"] != true && def["resizableY"] != true {
                    return;
                }
                ui.add_space(12.);
                for (group, dimensions) in [
                    (
                        "Min:",
                        [
                            ("minWidth", "maxWidth", "resizableX"),
                            ("minHeight", "maxHeight", "resizableY"),
                        ],
                    ),
                    (
                        "Max:",
                        [
                            ("maxWidth", "minWidth", "resizableX"),
                            ("maxHeight", "minHeight", "resizableY"),
                        ],
                    ),
                ] {
                    if group == "Max:" {
                        ui.add_space(12.);
                    }
                    ui.weak(locale::tr(ui, group));
                    for (index, (key, other, axis)) in dimensions.into_iter().enumerate() {
                        if index == 1 {
                            ui.weak("x");
                        }
                        let enabled = def[axis] == true;
                        let current = def[key]
                            .as_i64()
                            .map(|size| size.to_string())
                            .unwrap_or_default();
                        let edit = ui.add_enabled_ui(enabled, |ui| {
                            number_input_sized(
                                ui,
                                key,
                                &current,
                                NumberStyle::Plain {
                                    placeholder: enabled.then_some("None"),
                                },
                                60.,
                            )
                        });
                        if let Some(draft) = edit.inner {
                            let bound = patch.get(other).unwrap_or(&def[other]).as_i64();
                            let maximum = i64::from(i16::MAX);
                            let (min, max) = if key.starts_with("min") {
                                (0, bound.unwrap_or(maximum))
                            } else {
                                (bound.unwrap_or(0), maximum)
                            };
                            let value = world_level_panel::parse_form_int(&draft).max(min).min(max);
                            patch[key] = if value > 0 { json!(value) } else { Value::Null };
                        }
                    }
                }
            });
            row_width(ui, "", 136., |ui| {
                ui.weak(locale::tr(ui, "Resizable:"));
                for (axis, label, dimension, min, max) in [
                    ("resizableX", "Width", "width", "minWidth", "maxWidth"),
                    ("resizableY", "Height", "height", "minHeight", "maxHeight"),
                ] {
                    let mut enabled = def[axis].as_bool().unwrap_or(false);
                    if source_checkbox(ui, &mut enabled, label).changed() {
                        patch[axis] = json!(enabled);
                        if enabled {
                            patch[min] = patch.get(dimension).unwrap_or(&def[dimension]).clone();
                        } else {
                            patch[min] = Value::Null;
                            patch[max] = Value::Null;
                        }
                    }
                    ui.add_space(12.);
                }
                let can_lock = ["resizableX", "resizableY"]
                    .iter()
                    .all(|&axis| patch.get(axis).unwrap_or(&def[axis]) == true);
                let mut locked = def["keepAspectRatio"].as_bool().unwrap_or(false);
                if ui
                    .add_enabled_ui(can_lock, |ui| {
                        source_checkbox(ui, &mut locked, "Keep aspect ratio")
                    })
                    .inner
                    .changed()
                {
                    patch["keepAspectRatio"] = json!(locked);
                }
            });
            row_width(ui, "Editor visual", 136., |ui| {
                const SHAPES: [&str; 3] = ["Rectangle", "Ellipse", "Cross"];
                let mut rgb = enum_color(&def);
                if source_colors::color_editor_sized(ui, &mut rgb, None, egui::vec2(62., 30.))
                    .changed()
                {
                    patch["color"] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                }
                let tilesets = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let render_mode = text(&def, "renderMode", "Rectangle");
                let is_tile = render_mode == "Tile";
                let choices = source_controls::grouped_tileset_choices(ui, tilesets);
                let mut options = SHAPES.map(|shape| (locale::tr(ui, shape), None)).to_vec();
                options[0].1 = Some(locale::tr(ui, "Shapes"));
                options.extend(choices.iter().map(|(_, option)| *option));

                let current = if is_tile {
                    choices
                        .iter()
                        .position(|(choice, _)| {
                            *choice == source_controls::TilesetChoice::Existing(atlas)
                        })
                        .map(|index| index + SHAPES.len())
                } else {
                    SHAPES.iter().position(|shape| *shape == render_mode).or(Some(0))
                };
                let title =
                    current.map_or_else(|| locale::tr(ui, "Choose tileset"), |i| options[i].0);
                let reserved = if is_tile { 244. } else { 0. };
                let width = (ui.available_width() - reserved - 12.).max(40.);
                let choice = ui
                    .allocate_ui_with_layout(
                        egui::vec2(width, 29.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_max_width(width);
                            source_controls::form_select(
                                ui,
                                ("source-entity-atlas", uid),
                                29.,
                                &options,
                                current,
                                title,
                                source_controls::FormSelectAppearance::Normal,
                            )
                        },
                    )
                    .inner;
                if let Some(index) = choice.filter(|index| Some(*index) != current) {
                    let mode = if let Some(shape) = SHAPES.get(index) {
                        patch["tilesetId"] = Value::Null;
                        *shape
                    } else {
                        editor_atlas_choice = Some(choices[index - SHAPES.len()].0);
                        "Tile"
                    };
                    patch["renderMode"] = json!(mode);
                    patch["tileRect"] = Value::Null;
                    let next_is_tile = mode == "Tile";
                    if next_is_tile != is_tile {
                        patch["tileOpacity"] = json!(1);
                        patch["fillOpacity"] = json!(if next_is_tile { 0.08 } else { 1. });
                        patch["lineOpacity"] = json!(if next_is_tile { 0. } else { 1. });
                    }
                }
                if !is_tile {
                    return;
                }
                if atlas >= 0 {
                    ui.push_id(("entity-editor-tile", uid), |ui| {
                        if let Some(rect) = self.value_tile_thumbnail(
                            ui,
                            Some(atlas),
                            &def["tileRect"],
                            true,
                            false,
                        ) {
                            patch["tileRect"] = rect;
                        }
                    });
                }
                let modes = [
                    ("Cover", "Cover bounds"),
                    ("FitInside", "Fit inside bounds"),
                    ("Repeat", "Repeat"),
                    ("Stretch", "Dirty stretch to bounds"),
                    ("FullSizeCropped", "Full size (cropped in bounds)"),
                    ("FullSizeUncropped", "Full size (not cropped)"),
                    ("NineSlice", "9-slices scaling"),
                ];
                let current = text(&def, "tileRenderMode", "FitInside");
                let selected = modes.iter().position(|(mode, _)| *mode == current);
                let options = modes.map(|(_, label)| (locale::tr(ui, label), Color32::WHITE));
                let title = selected.map_or(current, |index| options[index].0);
                let width = ui.available_width().min(202.);
                let button = source_controls::searchable_select_button(ui, width, title);
                if let Some((index, _)) = source_controls::searchable_select_popup(
                    &button,
                    button.id.with(("entity-tile-render-mode", uid)),
                    selected.unwrap_or(0),
                    &options,
                ) && Some(index) != selected
                {
                    patch["tileRenderMode"] = json!(modes[index].0);
                }
            });

            if patch["tileRenderMode"]
                .as_str()
                .unwrap_or_else(|| text(&def, "tileRenderMode", "FitInside"))
                == "NineSlice"
            {
                let old = &def["nineSliceBorders"];
                let mut borders = if old.as_array().is_some_and(|values| values.len() == 4) {
                    std::array::from_fn(|i| old[i].as_i64().unwrap_or(2))
                } else {
                    [2; 4]
                };
                row_width(ui, "", 136., |ui| {
                    let help = ui.add(
                        egui::Image::new(egui::include_image!("../../assets/icons/help.svg"))
                            .fit_to_exact_size(Vec2::splat(32.))
                            .tint(source_controls::HELP_GREEN),
                    );
                    source_controls::help_tooltip(
                        help.on_hover_cursor(egui::CursorIcon::Help),
                        "9-slice scaling splits the texture into nine parts. Corners keep their size, edges repeat, and the center stretches. Border sizes are in pixels.",
                        false,
                    );
                    ui.label(locale::tr(ui, "9-slices borders:"));
                    for (index, key, label) in [
                        (0, "nineSliceUp", "top="),
                        (2, "nineSliceDown", "down="),
                        (3, "nineSliceLeft", "left="),
                        (1, "nineSliceRight", "right="),
                    ] {
                        ui.label(locale::tr(ui, label));
                        if let Some(value) = source_controls::number_input_sized(
                            ui,
                            key,
                            &borders[index].to_string(),
                            source_controls::NumberStyle::Plain { placeholder: None },
                            40.,
                        ) {
                            let value = if value.trim().is_empty() {
                                if index == 0 { 1 } else { borders[0] }
                            } else {
                                world_level_panel::parse_form_int(&value)
                                    .clamp(1, i64::from(i16::MAX))
                            };
                            if index == 0 && borders.iter().all(|border| *border == borders[0]) {
                                borders.fill(value);
                            } else {
                                borders[index] = value;
                            }
                        }
                    }
                });
                let next = json!(borders);
                if *old != next && !self.project.as_ref().unwrap().is_backup() {
                    patch["nineSliceBorders"] = next;
                }
            }
            row_width(ui, "", 136., |ui| {
                let is_tile = patch["renderMode"]
                    .as_str()
                    .unwrap_or_else(|| text(&def, "renderMode", "Rectangle"))
                    == "Tile";
                let mut hollow = def["hollow"].as_bool().unwrap_or(false);
                ui.spacing_mut().item_spacing.x = 4.;
                for (key, label, enabled) in [
                    ("tileOpacity", "Tile=", is_tile),
                    ("fillOpacity", "Fill=", !hollow),
                    ("lineOpacity", "Line=", true),
                ] {
                    ui.weak(locale::tr(ui, label));
                    let value = patch[key].as_f64().or_else(|| def[key].as_f64()).unwrap_or(1.);
                    let percent = (value * 100. + 0.5).floor().to_string();
                    let edit = ui.add_enabled_ui(enabled, |ui| {
                        source_controls::number_input_sized(
                            ui,
                            key,
                            &percent,
                            source_controls::NumberStyle::PercentSlider { min: 0., max: 100. },
                            60.,
                        )
                    });
                    if let Some(draft) = edit.inner {
                        let percent = draft
                            .trim()
                            .trim_end_matches('%')
                            .trim_end()
                            .parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite())
                            .unwrap_or(0.)
                            .clamp(0., 100.);
                        patch[key] = json!((percent + 0.5).floor() / 100.);
                    }
                    ui.weak("%");
                }
                if source_checkbox(ui, &mut hollow, "Hollow").changed() {
                    patch["hollow"] = json!(hollow);
                }
            });
            row_width(ui, "Alt UI icon", 136., |ui| {
                let tile = &def["uiTileRect"];
                let atlas = tile["tilesetUid"].as_i64();
                let tilesets = self.project.as_ref().unwrap().json["defs"]["tilesets"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let reserved = if atlas.is_some() { 38. } else { 0. };
                let width = (ui.available_width() - reserved).clamp(40., 267.);
                ui_atlas_choice = source_controls::tileset_picker(
                    ui,
                    width,
                    tilesets,
                    atlas,
                    "-- Use default editor visual --",
                    false,
                );
                if atlas.is_some() {
                    let tile = if number(tile, "w", 0.) > 0. {
                        tile
                    } else {
                        &Value::Null
                    };
                    ui.push_id(("entity-ui-tile", uid), |ui| {
                        if let Some(rect) = self.value_tile_thumbnail(ui, atlas, tile, true, false) {
                            patch["uiTileRect"] = rect;
                        }
                    });
                }
            });
            row_width(ui, "Pivot point", 136., |ui| {
                use source_controls::{NumberStyle, number_input_sized};
                let pivot = ["pivotX", "pivotY"].map(|key| def[key].as_f64().unwrap_or(0.));
                let advanced = pivot.iter().any(|ratio| ![0., 0.5, 1.].contains(ratio))
                    || ui
                        .data(|data| data.get_temp::<bool>(advanced_id))
                        .unwrap_or(false);
                ui.add_space(4.);
                if let Some([x, y]) = source_controls::pivot_grid(ui, pivot)
                    && pivot != [x, y]
                {
                    patch["pivotX"] = json!(x);
                    patch["pivotY"] = json!(y);
                }
                ui.add_space(2.);
                if !advanced {
                    if ui
                        .link(
                            RichText::new(locale::tr(ui, "Show advanced options"))
                                .color(theme::ACCENT)
                                .underline(),
                        )
                        .clicked()
                    {
                        ui.data_mut(|data| data.insert_temp(advanced_id, true));
                    }
                    return;
                }
                egui::Frame::new()
                    .inner_margin(4.)
                    .stroke(Stroke::new(1., theme::DARK))
                    .corner_radius(3.)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        for (label, inputs, pixels) in [
                            ("Float:", ["customFloatX", "customFloatY"], false),
                            ("Pixels:", ["customPixelX", "customPixelY"], true),
                        ] {
                            if pixels {
                                ui.separator();
                            }
                            ui.weak(locale::tr(ui, label));
                            for (axis, (key, dimension)) in
                                [("pivotX", "width"), ("pivotY", "height")]
                                    .into_iter()
                                    .enumerate()
                            {
                                if axis == 1 {
                                    ui.weak(",");
                                }
                                let size = patch
                                    .get(dimension)
                                    .unwrap_or(&def[dimension])
                                    .as_f64()
                                    .unwrap_or(16.)
                                    .max(1.);
                                let ratio = patch[key].as_f64().unwrap_or(pivot[axis]);
                                let displayed = if pixels {
                                    (ratio * size).floor()
                                } else {
                                    (ratio * 10_000.).trunc() / 10_000.
                                };
                                let style = if pixels {
                                    NumberStyle::Plain { placeholder: None }
                                } else {
                                    NumberStyle::Float
                                };
                                if let Some(draft) = number_input_sized(
                                    ui,
                                    inputs[axis],
                                    &displayed.to_string(),
                                    style,
                                    40.,
                                ) {
                                    let value = if pixels {
                                        world_level_panel::parse_form_int(&draft).clamp(-2048, 2048)
                                            as f64
                                            / size
                                    } else {
                                        draft
                                            .trim()
                                            .parse::<f64>()
                                            .ok()
                                            .filter(|value| value.is_finite())
                                            .unwrap_or(0.)
                                            .clamp(-50., 50.)
                                    };
                                    patch[key] = json!(value);
                                }
                            }
                        }
                    });
            });
            row_width(ui, "Max count", 136., |ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let mut count = def["maxCount"].as_i64().unwrap_or(0);
                let display = if count == 0 { String::new() } else { count.to_string() };
                if let Some(value) = source_controls::number_input_sized(
                    ui,
                    "maxCount",
                    &display,
                    source_controls::NumberStyle::Plain { placeholder: Some("Unlimited") },
                    90.,
                ) {
                    count = world_level_panel::parse_form_int(&value).clamp(0, 1024);
                    patch["maxCount"] = json!(count);
                }
                let mut limit_choice = |ui: &mut egui::Ui,
                                        key: &str,
                                        default: &str,
                                        choices: [(&str, &str); 3]| {
                    let current = text(&def, key, default);
                    let selected = choices
                        .iter()
                        .position(|(value, _)| *value == current)
                        .unwrap_or(choices.len());
                    let options = choices.map(|(_, label)| (locale::tr(ui, label), Color32::WHITE));
                    ui.add_enabled_ui(count > 0, |ui| {
                        let title = options.get(selected).map_or(current, |option| option.0);
                        let button = source_controls::searchable_select_button(ui, 110., title);
                        if let Some((index, _)) = source_controls::searchable_select_popup(
                            &button,
                            button.id.with((key, uid)),
                            selected,
                            &options,
                        ) && choices[index].0 != current
                        {
                            patch[key] = json!(choices[index].0);
                        }
                    });
                };
                limit_choice(
                    ui,
                    "limitScope",
                    "PerLevel",
                    [
                        ("PerLayer", "per layer"),
                        ("PerLevel", "per level"),
                        ("PerWorld", "in the world"),
                    ],
                );
                ui.add_space(4.);
                ui.weak(locale::tr(ui, ", and"));
                ui.add_space(4.);
                limit_choice(
                    ui,
                    "limitBehavior",
                    "MoveLastOne",
                    [
                        ("DiscardOldOnes", "discard older ones"),
                        ("PreventAdding", "prevent adding more"),
                        ("MoveLastOne", "move the last one instead of adding"),
                    ],
                );
                ui.add_space(4.);
                let end = locale::tr(ui, "when max is reached.");
                ui.add(egui::Label::new(RichText::new(end).weak()).truncate())
                    .on_hover_text(end);
            });
        });
        if settings_next != settings_open {
            self.settings
                .set_ui_state_int("extraEntityDetails", None, i64::from(settings_next));
        }
        if (changed(&patch) || ui_atlas_choice.is_some())
            && self
                .edit("Entity definition changed", |p| {
                    if let Some(choice) = editor_atlas_choice {
                        patch["tilesetId"] = match choice {
                            source_controls::TilesetChoice::Unassigned => Value::Null,
                            source_controls::TilesetChoice::Existing(uid) => json!(uid),
                            source_controls::TilesetChoice::EmbeddedIcons => {
                                json!(p.ensure_embedded_icons()?)
                            }
                        };
                    }
                    if let Some(choice) = ui_atlas_choice {
                        let atlas = match choice {
                            source_controls::TilesetChoice::Unassigned => None,
                            source_controls::TilesetChoice::Existing(uid) => Some(uid),
                            source_controls::TilesetChoice::EmbeddedIcons => {
                                Some(p.ensure_embedded_icons()?)
                            }
                        };
                        patch["uiTileRect"] = atlas.map_or(
                            Value::Null,
                            |uid| json!({"tilesetUid":uid,"x":0,"y":0,"w":0,"h":0}),
                        );
                    }
                    p.update_entity_definition(uid, &patch)
                })
                .is_some()
        {
            ui.data_mut(|data| data.remove::<bool>(advanced_id));
        }
        if let Some(change) = tag_change {
            self.edit("Entity tags changed", |p| match change {
                tag_editor::Change::Set(value) => {
                    p.update_entity_definition(uid, &json!({"tags":value}))
                }
                tag_editor::Change::Rename {
                    old,
                    new,
                    everywhere,
                } => p.rename_definition_tag("entities", uid, &old, &new, everywhere),
            });
        }
        Some(uid)
    }
    fn source_field_list(&mut self, ui: &mut egui::Ui, owner: Option<i64>, height: f32) {
        let fields = self
            .project
            .as_ref()
            .map(|p| {
                owner
                    .and_then(|uid| {
                        p.json["defs"]["entities"]
                            .as_array()?
                            .iter()
                            .find(|e| e["uid"] == uid)
                    })
                    .map(|e| e["fieldDefs"].clone())
                    .unwrap_or_else(|| p.json["defs"]["levelFields"].clone())
            })
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        if !fields.iter().any(|f| f["uid"].as_i64() == self.field_uid) {
            self.field_uid = fields.first().and_then(|f| f["uid"].as_i64());
        }
        ui.spacing_mut().item_spacing.y = FIELD_LIST_GAP;
        let button_row_height = 40.;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            ui.spacing_mut().button_padding.x = 4.;
            for (array, label) in [(false, "+ SINGLE VALUE"), (true, "+ ARRAY")] {
                if orange(
                    ui,
                    label,
                    egui::vec2(
                        if array {
                            70.
                        } else {
                            (ui.available_width() - 74.).max(90.)
                        },
                        button_row_height,
                    ),
                )
                .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("source-field-type-picker"),
                            (owner, array, None::<i64>, None::<String>),
                        )
                    });
                }
            }
        });
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(4)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("source-field-list", owner))
                    .max_height((height - button_row_height - FIELD_LIST_GAP - 10.).max(40.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        for (index, field) in fields.iter().enumerate() {
                            let uid = field["uid"].as_i64().unwrap_or(-1);
                            let selected = self.field_uid == Some(uid);
                            let kind = text(field, "__type", "Int")
                                .trim_start_matches("Array<")
                                .trim_end_matches('>');
                            let (badge, color) = match kind {
                                "Int" | "Float" => ("123", "#50b3cb"),
                                "Bool" => ("", "#cd88dd"),
                                "String" | "Multilines" => ("Abc", "#bd5f32"),
                                "Color" => ("#", "#99d367"),
                                "Tile" => ("Tile", "#99d367"),
                                "Point" => ("XY", "#7779c9"),
                                "FilePath" => ("File", "#7779c9"),
                                "EntityRef" => ("Ref", "#7779c9"),
                                _ => ("Enu", "#ff4b4b"),
                            };
                            let item = egui::Frame::new()
                                .fill(if selected {
                                    theme::ACCENT
                                } else {
                                    theme::MEDIUM
                                })
                                .inner_margin(egui::Margin::symmetric(5, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let badge_width = 34.;
                                        let menu_size = egui::vec2(18., 30.);
                                        ui.add(
                                            egui::Label::new(RichText::new("≡").color(
                                                if selected {
                                                    Color32::BLACK
                                                } else {
                                                    theme::TEXT
                                                },
                                            ))
                                            .sense(Sense::drag()),
                                        )
                                        .dnd_set_drag_payload((owner, uid));
                                        let name_width = ui.available_width()
                                            - badge_width
                                            - menu_size.x
                                            - 2. * ui.spacing().item_spacing.x;
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(name_width.max(25.), 30.),
                                            Sense::click(),
                                        );
                                        let name = text(field, "identifier", "Field");
                                        let ink = if selected {
                                            Color32::BLACK
                                        } else {
                                            Color32::WHITE
                                        };
                                        let inset = 8.;
                                        let galley = egui::WidgetText::from(RichText::new(name).color(ink)).into_galley(
                                            ui,
                                            Some(egui::TextWrapMode::Truncate),
                                            (rect.width() - inset).max(0.),
                                            FontId::proportional(14.),
                                        );
                                        ui.painter_at(rect).galley(
                                            rect.left_center()
                                                + egui::vec2(inset, -galley.size().y / 2.),
                                            galley,
                                            ink,
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::WidgetType::SelectableLabel,
                                                true,
                                                selected,
                                                name,
                                            )
                                        });
                                        if response.clicked() {
                                            self.clipboard_focus(
                                                ui.ctx(),
                                                ClipTarget::Field {
                                                    owner,
                                                    uid: Some(uid),
                                                },
                                            );
                                            self.field_uid = Some(uid);
                                        }
                                        egui::Frame::new()
                                            .fill(theme::color(color).gamma_multiply(0.7))
                                            .corner_radius(3.)
                                            .inner_margin(4)
                                            .show(ui, |ui| {
                                                ui.set_width(badge_width - 8.);
                                                if kind == "Bool" {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        egui::vec2(20., 20.),
                                                        Sense::hover(),
                                                    );
                                                    ui.painter().add(egui::epaint::Shape::line(
                                                        vec![
                                                            rect.min + egui::vec2(3., 10.),
                                                            rect.min + egui::vec2(8., 15.),
                                                            rect.min + egui::vec2(17., 5.),
                                                        ],
                                                        Stroke::new(3_f32, Color32::WHITE),
                                                    ));
                                                } else {
                                                    ui.label(
                                                        RichText::new(badge).color(Color32::WHITE),
                                                    );
                                                }
                                            });
                                        source_definition_menu(ui, Some(menu_size), &response, |ui| {
                                            self.clipboard_menu(ui, ClipTarget::Field {
                                                owner,
                                                uid: Some(uid),
                                            });
                                        });
                                    });
                                });
                            if let Some(payload) =
                                item.response.dnd_hover_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                ui.painter().hline(
                                    item.response.rect.x_range(),
                                    item.response.rect.top(),
                                    Stroke::new(2_f32, theme::ACCENT),
                                );
                            }
                            if let Some(payload) =
                                item.response.dnd_release_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                self.edit("Field reordered", |p| {
                                    p.reorder_field_definition(payload.1, index)
                                });
                            }
                            ui.add_space(2.);
                        }
                        self.definition_list_paste_menu(ui, ClipTarget::Field { owner, uid: None });
                    });
            });
    }
}

fn field_display_select<'a>(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    value: &mut &'a str,
    options: impl Iterator<Item = (&'a str, &'a str)> + Clone,
) {
    let label = options
        .clone()
        .find(|(key, _)| key == value)
        .or_else(|| options.clone().next())
        .map_or("", |(_, label)| label);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 32.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_max_width(width);
            egui::ComboBox::from_id_salt(id)
                .selected_text(locale::tr(ui, label))
                .width(width)
                .truncate()
                .show_ui(ui, |ui| {
                    for (key, label) in options {
                        ui.selectable_value(value, key, locale::tr(ui, label));
                    }
                });
        },
    );
}

impl EditorApp {
    pub(super) fn source_field_properties(&mut self, ui: &mut egui::Ui, def: &Value) {
        let previous_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 10.;
        source_controls::form_heading_line(ui, "General settings");
        let uid = def["uid"].as_i64().unwrap();
        let public = text(def, "__type", "Int");
        let kind = public.trim_start_matches("Array<").trim_end_matches('>');
        let array = def["isArray"] == true;
        source_controls::form_row_wrapped(ui, "Type", "Type", FIELD_LABEL_WIDTH, |ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.spacing_mut().button_padding.x = 4.;
            let type_width = (ui.available_width() - 73.).max(40.);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(type_width, 33.), Sense::hover());
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            painter.rect_stroke(rect, 0., Stroke::new(1., theme::LIGHT), StrokeKind::Inside);
            painter.text(
                rect.left_center() + egui::vec2(8., 0.),
                Align2::LEFT_CENTER,
                public,
                FontId::monospace(14.),
                theme::TEXT.gamma_multiply(0.6),
            );
            if orange(ui, "CONVERT", egui::vec2(73., 33.)).clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("source-field-type-picker"),
                        (self.field_owner, array, Some(uid), None::<String>),
                    )
                });
            }
        });
        let mut patch = form_with_label_width(
            ui,
            def,
            &[
                P("identifier", "Field identifier", W::Text),
                P("doc", "Documentation", W::Text),
            ],
            FIELD_LABEL_WIDTH,
        );
        if let Some(owner) = self.field_owner {
            let can_export = self.project.as_ref().is_some_and(|project| {
                project.json["defs"]["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|def| def["uid"] == owner && def["exportToToc"] == true)
            });
            source_controls::form_row_wrapped(
                ui,
                "Export to ToC",
                "exportToToc",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let mut checked = def["exportToToc"] == true;
                    if ui
                        .add_enabled_ui(can_export, |ui| source_checkbox(ui, &mut checked, ""))
                        .inner
                        .changed()
                    {
                        patch["exportToToc"] = json!(checked);
                    }
                },
            );
        }
        if matches!(kind, "String" | "Multilines") || kind.contains("Enum.") {
            let searchable = form_with_label_width(
                ui,
                def,
                &[P("searchable", "Searchable", W::Bool)],
                FIELD_LABEL_WIDTH,
            );
            if let Some(value) = searchable.get("searchable") {
                patch["searchable"] = value.clone();
            }
        }
        source_controls::form_heading_line(ui, "Value specifications");
        if !matches!(kind, "Point" | "EntityRef" | "Tile" | "FilePath")
            && let Ok(value) = self.project.as_ref().unwrap().field_default_value(def)
        {
            let label = match kind {
                "Bool" => "Default state",
                "Color" => "Default color",
                _ => "Default value",
            };
            if let Some(value) =
                source_controls::form_row_wrapped(ui, label, label, FIELD_LABEL_WIDTH, |ui| {
                    if matches!(kind, "Int" | "Float") {
                        let current = def["defaultOverride"]["params"][0]
                            .as_number()
                            .map(ToString::to_string);
                        let hint = if value.is_null() {
                            locale::tr(ui, "(null)").to_owned()
                        } else {
                            value.to_string()
                        };
                        let width = ui.available_width();
                        source_controls::edit_text(
                            ui,
                            ("field-numeric-default", uid),
                            current.as_deref().unwrap_or(""),
                            |ui, draft, id| {
                                ui.add_sized(
                                    [width, 33.],
                                    egui::TextEdit::singleline(draft)
                                        .id(id)
                                        .vertical_align(egui::Align::Center)
                                        .hint_text(hint),
                                )
                            },
                        )
                        .map(|raw| source_field_number(&raw, kind))
                    } else {
                        ui.vertical(|ui| {
                            scalar_editor(
                                ui,
                                self.project.as_ref().unwrap(),
                                kind,
                                &value,
                                def,
                            )
                        })
                        .inner
                    }
                })
            {
                self.edit("Field default changed", |p| p.set_field_default(uid, value));
            }
        }
        if matches!(kind, "Int" | "Float") {
            source_controls::form_row_wrapped(ui, "Limits", "Limits", FIELD_LABEL_WIDTH, |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                let width = ((ui.available_width() - 24.) / 2.).max(40.);
                for key in ["min", "max"] {
                    if key == "max" {
                        ui.label(locale::tr(ui, "to"));
                    }
                    let current = def[key].as_f64().map(|value| value.to_string());
                    if let Some(raw) = ui_fields::buffered_text_sized(
                        ui,
                        key,
                        current.as_deref().unwrap_or(""),
                        egui::vec2(width, 33.),
                    ) {
                        patch[key] = source_field_number(&raw, kind);
                    }
                }
            });
        }
        let mut properties = Vec::new();
        if !matches!(kind, "Bool" | "Tile") {
            properties.push(P(
                "canBeNull",
                if array {
                    "Can contain nulls"
                } else if matches!(kind, "FilePath" | "Point" | "EntityRef") {
                    "Is optional"
                } else {
                    "Can be null"
                },
                W::Bool,
            ));
        }
        if kind == "FilePath" {
            properties.push(P("acceptFileTypes", "File extensions", W::Text));
        }
        if kind == "String" {
            properties.push(P("regex", "Regex check", W::Text));
        }
        if kind == "Multilines" {
            let previous = def["textLanguageMode"].as_str().map(str::to_owned);
            let mut language = previous.clone();
            source_controls::form_row_wrapped(
                ui,
                "Language",
                "textLanguageMode",
                FIELD_LABEL_WIDTH,
                |ui| {
                    egui::ComboBox::from_id_salt(("field-language", uid))
                        .selected_text(locale::tr(ui, language.as_deref().unwrap_or("Plain text")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut language, None, locale::tr(ui, "Plain text"));
                            for (mode, label) in [
                                ("LangJson", "JSON"),
                                ("LangXml", "XML/HTML"),
                                ("LangMarkdown", "Markdown"),
                                ("LangRuby", "Ruby"),
                                ("LangPython", "Python"),
                                ("LangHaxe", "Haxe"),
                                ("LangJS", "Javascript"),
                                ("LangLua", "Lua"),
                                ("LangC", "C/C++/C#"),
                                ("LangLog", "Log file"),
                            ] {
                                ui.selectable_value(
                                    &mut language,
                                    Some(mode.to_owned()),
                                    locale::tr(ui, label),
                                );
                            }
                        });
                },
            );
            if language != previous {
                patch["textLanguageMode"] = language.map(Value::String).unwrap_or(Value::Null);
            }
        }
        let mut specs = def.clone();
        if kind == "FilePath" {
            specs["acceptFileTypes"] = json!(
                def["acceptFileTypes"]
                    .as_array()
                    .map(|values| values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("  "))
                    .unwrap_or_default()
            );
        }
        if let Some(obj) =
            form_with_label_width(ui, &specs, &properties, FIELD_LABEL_WIDTH).as_object()
        {
            patch.as_object_mut().unwrap().extend(obj.clone());
        }
        if kind == "FilePath"
            && let Some(raw) = patch["acceptFileTypes"].as_str()
        {
            patch["acceptFileTypes"] = source_file_types(raw);
        }
        let mut tile_default_edit = None;
        if kind == "Tile" {
            let project = self.project.as_ref().unwrap();
            let atlas = def["tilesetUid"].as_i64();
            let bound = atlas.is_some_and(|atlas| project.tileset(atlas).is_some());
            let tile = if bound {
                project.field_default_value(def).unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            let optional_label = if array {
                "Can contain nulls"
            } else {
                "Is optional"
            };
            source_controls::form_row_wrapped(
                ui,
                optional_label,
                "canBeNull",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let mut optional = def["canBeNull"] == true;
                    if ui
                        .add_enabled_ui(tile.is_null(), |ui| source_checkbox(ui, &mut optional, ""))
                        .inner
                        .changed()
                    {
                        patch["canBeNull"] = json!(optional);
                    }
                },
            );
            source_controls::form_row_wrapped(
                ui,
                "Tileset",
                "tilesetUid",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let placeholder = locale::tr(ui, "-- Pick a tileset --");
                    let mut candidates = Vec::new();
                    if atlas.is_none() {
                        candidates.push((None, placeholder));
                    }
                    candidates.extend(
                        self.project.as_ref().unwrap().json["defs"]["tilesets"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|tileset| {
                                Some((
                                    Some(tileset["uid"].as_i64()?),
                                    tileset["identifier"].as_str()?,
                                ))
                            }),
                    );
                    let options: Vec<_> =
                        candidates.iter().map(|(_, label)| (*label, None)).collect();
                    let current = candidates.iter().position(|(id, _)| *id == atlas);
                    let title = current.map_or("", |index| options[index].0);
                    let appearance = if atlas.is_none() {
                        source_controls::FormSelectAppearance::Required
                    } else {
                        source_controls::FormSelectAppearance::Normal
                    };
                    let width = if bound {
                        ui.available_width() * 0.5
                    } else {
                        ui.available_width()
                    };
                    let selected = ui
                        .allocate_ui_with_layout(
                            egui::vec2(width, 29.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                source_controls::form_select(
                                    ui,
                                    ("field-tile-atlas", uid),
                                    29.,
                                    &options,
                                    current,
                                    title,
                                    appearance,
                                )
                            },
                        )
                        .inner;
                    if let Some(index) = selected.filter(|i| Some(*i) != current) {
                        patch["tilesetUid"] = json!(candidates[index].0);
                    }
                    if bound {
                        ui.weak(locale::tr(ui, "Default:"));
                        tile_default_edit = ui
                            .push_id(("field-default-tile", uid), |ui| {
                                self.value_tile_thumbnail(ui, atlas, &tile, true, false)
                            })
                            .inner;
                        if !tile.is_null() {
                            let image = egui::Image::new(egui::include_image!(
                                "../../assets/icons/clear.svg"
                            ))
                            .fit_to_exact_size(Vec2::splat(18.));
                            if source_controls::add_sized_button(
                                ui,
                                egui::vec2(42., 35.),
                                egui::Button::image(image).fill(theme::LIGHT),
                            )
                            .on_hover_text(locale::tr(ui, "Clear default value"))
                            .clicked()
                            {
                                tile_default_edit = Some(Value::Null);
                            }
                        }
                    }
                },
            );
        }
        if kind == "EntityRef" {
            let policy = text(def, "allowedRefs", "OnlySame");
            source_controls::form_row_wrapped(
                ui,
                "Reference options",
                "Reference options",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 16.;
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Button, FontId::proportional(13.333));
                        for (key, label) in [
                            ("symmetricalRef", "Symmetrical"),
                            ("allowOutOfLevelRef", "In any level"),
                            ("autoChainRef", "Auto-chain"),
                        ] {
                            if key == "autoChainRef" {
                                ui.end_row();
                            }
                            let mut checked = def[key] == true;
                            if ui
                                .add_enabled_ui(
                                    key != "symmetricalRef" || policy == "OnlySame",
                                    |ui| source_checkbox(ui, &mut checked, label),
                                )
                                .inner
                                .changed()
                            {
                                patch[key] = json!(checked);
                            }
                        }
                    });
                },
            );
            source_controls::form_row_wrapped(
                ui,
                "Allowed references",
                "allowedRefs",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.vertical(|ui| {
                        use source_controls::{searchable_select_button, searchable_select_popup};
                        let project = self.project.as_ref().unwrap();
                        let entities = project.json["defs"]["entities"].as_array().unwrap();
                        let owner_name = entities
                            .iter()
                            .find(|entity| entity["uid"].as_i64() == self.field_owner)
                            .and_then(|entity| entity["identifier"].as_str())
                            .unwrap_or("");
                        let same =
                            locale::format(ui, "Only another '{name}'s", &[("name", owner_name)]);
                        let policies = ["Any", "OnlySame", "OnlyTags", "OnlySpecificEntity"];
                        let options = [
                            locale::tr(ui, "Any entity"),
                            same.as_str(),
                            locale::tr(ui, "Any entity with one of the specified tags"),
                            locale::tr(ui, "Only a specific Entity"),
                        ]
                        .map(|label| (label, Color32::TRANSPARENT));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;
                            let current = policies
                                .iter()
                                .position(|value| *value == policy)
                                .unwrap_or(1);
                            let width = ui.available_width();
                            let width = if policy == "OnlySpecificEntity" {
                                (width - ui.spacing().item_spacing.x) / 2.
                            } else {
                                width
                            };
                            let button = searchable_select_button(ui, width, options[current].0);
                            if let Some((index, _)) = searchable_select_popup(
                                &button,
                                ui.make_persistent_id(("field-reference-policy", uid)),
                                current,
                                &options,
                            )
                            .filter(|(index, _)| *index != current)
                            {
                                let next = policies[index];
                                patch["allowedRefs"] = json!(next);
                                if matches!(next, "Any" | "OnlyTags") {
                                    patch["symmetricalRef"] = json!(false);
                                }
                                if next == "OnlySpecificEntity" {
                                    patch["allowedRefsEntityUid"] = json!(self.field_owner);
                                }
                            }
                            if policy == "OnlySpecificEntity" {
                                let options: Vec<_> = entities
                                    .iter()
                                    .map(|entity| {
                                        (text(entity, "identifier", "Entity"), Color32::TRANSPARENT)
                                    })
                                    .collect();
                                let current = entities
                                    .iter()
                                    .position(|entity| entity["uid"] == def["allowedRefsEntityUid"]);
                                let title = current.map_or_else(
                                    || locale::tr(ui, "Choose entity"),
                                    |index| options[index].0,
                                );
                                let button = searchable_select_button(ui, ui.available_width(), title);
                                if let Some((index, _)) = searchable_select_popup(
                                    &button,
                                    ui.make_persistent_id(("field-reference-type", uid)),
                                    current.unwrap_or(0),
                                    &options,
                                )
                                .filter(|(index, _)| Some(*index) != current)
                                {
                                    patch["allowedRefsEntityUid"] = entities[index]["uid"].clone();
                                }
                            }
                        });
                        if policy == "OnlyTags"
                            && let Some(tag_editor::Change::Set(tags)) = tag_editor::show(
                                ui,
                                project,
                                tag_editor::Field::ReferenceTargets,
                                uid,
                                &def["allowedRefTags"],
                            )
                        {
                            patch["allowedRefTags"] = tags;
                        }
                    });
                },
            );
        }
        if array {
            use source_controls::{NumberStyle, number_input_sized};
            source_controls::form_row_wrapped(
                ui,
                "Array length",
                "arrayMinLength",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    let width = ((ui.available_width() - 24.) / 2.).max(40.);
                    for (key, opposite, placeholder, minimum) in [
                        ("arrayMinLength", "arrayMaxLength", "min length", true),
                        ("arrayMaxLength", "arrayMinLength", "max length", false),
                    ] {
                        if !minimum {
                            ui.label(locale::tr(ui, "to"));
                        }
                        let current = def[key].as_i64().map(|n| n.to_string());
                        if let Some(raw) = number_input_sized(
                            ui,
                            key,
                            current.as_deref().unwrap_or(""),
                            NumberStyle::Plain { placeholder: Some(placeholder) },
                            width,
                        ) {
                            let length = raw.trim().parse::<i64>().unwrap_or(0).clamp(0, 99_999);
                            patch[key] = if length == 0 { Value::Null } else { json!(length) };
                            if length > 0
                                && def[opposite].as_i64().is_some_and(|bound| {
                                    if minimum { bound < length } else { bound > length }
                                })
                            {
                                patch[opposite] = json!(length);
                            }
                        }
                    }
                },
            );
        }
        source_controls::form_heading_line(ui, "Display");
        ui.push_id(uid, |ui| {
            self.source_field_display(ui, def, kind, array, &mut patch)
        });
        ui.spacing_mut().item_spacing.y = previous_spacing;
        if changed(&patch) || tile_default_edit.is_some() {
            self.edit("Field definition changed", |p| {
                if changed(&patch) {
                    p.update_field_definition(uid, &patch)?;
                }
                if let Some(tile) = tile_default_edit {
                    p.set_field_default(uid, tile)?;
                }
                Ok(())
            });
        }
    }
    fn source_field_display(
        &self,
        ui: &mut egui::Ui,
        def: &Value,
        kind: &str,
        array: bool,
        patch: &mut Value,
    ) {
        let entity = self.field_owner.is_some();
        let point = kind == "Point";
        let reference = kind == "EntityRef";
        let number = matches!(kind, "Int" | "Float");
        let enumeration = kind.contains("Enum.");
        let tile = kind == "Tile" || enumeration;
        let path = kind == "FilePath";
        let identifier = text(def, "identifier", "Field");
        let name_and_value = if array {
            locale::format(
                ui,
                r#"Show "{identifier}=[...values...]""#,
                &[("identifier", identifier)],
            )
        } else {
            locale::format(
                ui,
                r#"Show "{identifier}=...""#,
                &[("identifier", identifier)],
            )
        };
        let value_only = if array {
            "Show values only"
        } else {
            "Show value only"
        };
        let isolated_point = if array {
            "Show isolated points"
        } else {
            "Show isolated point"
        };
        let connected_point = if array {
            "Show star of points"
        } else {
            "Show connected point"
        };
        let entity_tile = if path {
            "Replace entity tile (image file only)"
        } else {
            "Replace entity tile"
        };
        let options = [
            ("Hidden", "Do not show", true),
            ("ValueOnly", value_only, !point),
            ("NameAndValue", &name_and_value, true),
            (
                "EntityTile",
                entity_tile,
                entity && (tile || path && !array),
            ),
            (
                "LevelTile",
                "Replace level render in world view",
                !entity && tile,
            ),
            ("Points", isolated_point, entity && point),
            ("PointStar", connected_point, entity && point),
            ("PointPath", "Show path of points", entity && point && array),
            (
                "PointPathLoop",
                "Show path of points (looping)",
                entity && point && array,
            ),
            (
                "RadiusPx",
                "As a radius (pixels)",
                entity && number && !array,
            ),
            (
                "RadiusGrid",
                "As a radius (grid-based)",
                entity && number && !array,
            ),
            ("ArrayCountWithLabel", "Show array length with label", array),
            ("ArrayCountNoLabel", "Show array length only", array),
            (
                "RefLinkBetweenPivots",
                "Reference link (using pivot coord)",
                reference,
            ),
            (
                "RefLinkBetweenCenters",
                "Reference link (using center coord)",
                reference,
            ),
        ];
        let current = text(def, "editorDisplayMode", "Hidden");
        let mut mode = current;
        let text_mode = matches!(
            current,
            "ValueOnly" | "NameAndValue" | "ArrayCountWithLabel" | "ArrayCountNoLabel"
        );
        let link = matches!(
            current,
            "PointStar"
                | "PointPath"
                | "PointPathLoop"
                | "RefLinkBetweenPivots"
                | "RefLinkBetweenCenters"
        );
        let color = !matches!(current, "Hidden" | "EntityTile" | "LevelTile");
        source_controls::form_row_wrapped(
            ui,
            "Display in editor",
            "editorDisplayMode",
            FIELD_LABEL_WIDTH,
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                ui.horizontal_wrapped(|ui| {
                    let position_width = if entity && text_mode { 114. } else { 0. };
                    let link_width = if link { 137. } else { 0. };
                    let world_width = if entity { 0. } else { 185. };
                    let scale_width = if text_mode { 62. } else { 0. };
                    let color_width = if color { 70. } else { 0. };
                    let accessory_width =
                        position_width + link_width + world_width + scale_width + color_width;
                    let mode_width = (ui.available_width() - accessory_width).max(64.);
                    field_display_select(
                        ui,
                        "field-display-mode",
                        mode_width,
                        &mut mode,
                        options
                            .into_iter()
                            .filter_map(|(key, label, allowed)| allowed.then_some((key, label))),
                    );
                    if entity && text_mode {
                        let original = text(def, "editorDisplayPos", "Above");
                        let mut position = original;
                        field_display_select(
                            ui,
                            "field-display-position",
                            position_width - 4.,
                            &mut position,
                            ["Above", "Center", "Beneath"]
                                .into_iter()
                                .map(|option| (option, option)),
                        );
                        if position != original {
                            patch["editorDisplayPos"] = json!(position);
                        }
                    }
                    if link {
                        let original = text(def, "editorLinkStyle", "ZigZag");
                        let mut style = original;
                        field_display_select(
                            ui,
                            "field-link-style",
                            link_width - 4.,
                            &mut style,
                            [
                                ("ZigZag", "Zig-zag"),
                                ("StraightArrow", "Straight arrow"),
                                ("CurvedArrow", "Curved arrow"),
                                ("ArrowsLine", "Line of arrows"),
                                ("DashedLine", "Dashed line"),
                            ]
                            .into_iter(),
                        );
                        if style != original {
                            patch["editorLinkStyle"] = json!(style);
                        }
                    }
                    if !entity {
                        let mut world_view = def["editorShowInWorld"].as_bool().unwrap_or(true);
                        let checkbox = ui.allocate_ui_with_layout(
                            egui::vec2(world_width - 4., 32.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| source_checkbox(ui, &mut world_view, "Show in world view"),
                        );
                        if checkbox.inner.changed() {
                            patch["editorShowInWorld"] = json!(world_view);
                        }
                    }
                    if text_mode {
                        let before =
                            (def["editorDisplayScale"].as_f64().unwrap_or(1.) * 100.).round();
                        let mut percent = before;
                        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
                        let response = ui.add_sized(
                            [scale_width - 4., 32.],
                            egui::DragValue::new(&mut percent)
                                .speed(1.)
                                .range(10. ..=3_276_700.)
                                .clamp_existing_to_range(false)
                                .max_decimals(0)
                                .update_while_editing(false)
                                .custom_parser(|raw| {
                                    Some(
                                        raw.trim()
                                            .trim_end_matches('%')
                                            .trim_end()
                                            .parse::<f64>()
                                            .ok()
                                            .filter(|value| value.is_finite())
                                            .unwrap_or(100.)
                                            .round(),
                                    )
                                }),
                        );
                        ui_commit::track_editor(ui.ctx(), &response);
                        response.on_hover_text(locale::tr(ui, "Display scale (%)"));
                        if percent != before {
                            patch["editorDisplayScale"] = json!(percent.round() / 100.);
                        }
                    }
                    if color {
                        let project = self.project.as_ref().unwrap();
                        let inherited = if let Some(owner) = self.field_owner {
                            let entity = &project.json["defs"]["entities"];
                            let entity = entity
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|def| def["uid"] == owner);
                            entity
                                .and_then(|def| def["color"].as_str())
                                .map_or(theme::TEXT, theme::color)
                        } else {
                            let background = self
                                .selected
                                .and_then(|key| project.level(key))
                                .map_or_else(
                                    || {
                                        theme::color(text(
                                            &project.json,
                                            "defaultLevelBgColor",
                                            "#696A79",
                                        ))
                                    },
                                    |level| canvas::level_smart_color(project, level),
                                );
                            background.lerp_to_gamma(Color32::WHITE, 0.45)
                        };
                        let mut value = def["editorDisplayColor"]
                            .as_str()
                            .map(theme::color)
                            .map(|c| [c.r(), c.g(), c.b()]);
                        if source_colors::inherited_color_editor(ui, &mut value, inherited)
                            .changed()
                        {
                            patch["editorDisplayColor"] = value.map_or(Value::Null, |[r, g, b]| {
                                json!(format!("#{r:02X}{g:02X}{b:02X}"))
                            });
                        }
                    }
                });
            },
        );
        if mode != current {
            patch["editorDisplayMode"] = json!(mode);
        }
        if number {
            source_controls::form_row_wrapped(
                ui,
                "Prefix/suffix",
                "editorTextPrefix",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    let width = ((ui.available_width() - 39.) / 2.).max(30.);
                    let affix_size = egui::vec2(width, 33.);
                    for (key, prefix) in [("editorTextPrefix", true), ("editorTextSuffix", false)] {
                        if !prefix {
                            ui.label("1234");
                        }
                        if let Some(value) =
                            ui_fields::buffered_text_sized(ui, key, text(def, key, ""), affix_size)
                        {
                            let value = if prefix {
                                value.trim_start()
                            } else {
                                value.trim_end()
                            };
                            patch[key] = if value.is_empty() {
                                Value::Null
                            } else {
                                json!(value)
                            };
                        }
                    }
                },
            );
        }
        let color_label = if entity {
            "Override entity color"
        } else {
            "Override level color"
        };
        for (key, label, visible, enabled) in [
            ("editorAlwaysShow", "Always show", true, mode != "Hidden"),
            (
                "editorCutLongValues",
                "Only show an excerpt in editor",
                matches!(kind, "String" | "Multilines"),
                true,
            ),
            (
                "useForSmartColor",
                color_label,
                kind == "Color" || enumeration,
                true,
            ),
        ] {
            if visible {
                source_controls::form_row_wrapped(ui, label, key, FIELD_LABEL_WIDTH, |ui| {
                    let mut checked = def[key].as_bool().unwrap_or(false);
                    if ui
                        .add_enabled_ui(enabled, |ui| source_checkbox(ui, &mut checked, ""))
                        .inner
                        .changed()
                    {
                        patch[key] = json!(checked);
                    }
                });
            }
        }
    }

    fn source_field_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-field-type-picker");
        let Some((owner, array, convert, mut chosen)) =
            ctx.data_mut(|d| d.get_temp::<(Option<i64>, bool, Option<i64>, Option<String>)>(id))
        else {
            return;
        };
        let mut open = true;
        let mut dismiss = false;
        let mut apply = None;
        let title = if convert.is_some() {
            "Convert field type"
        } else if array {
            "Create array field"
        } else {
            "Create single value field"
        };
        egui::Window::new(locale::tr_ctx(ctx, title))
            .id(id)
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for kind in [
                        "Int", "Float", "Bool", "String", "Multilines", "Color", "FilePath",
                        "Tile", "EntityRef", "Point",
                    ] {
                        if owner.is_none() && matches!(kind, "EntityRef" | "Point") {
                            continue;
                        }
                        let button = egui::Button::new(locale::tr(ui, kind));
                        if source_controls::add_sized_button(ui, [95., 44.], button).clicked() {
                            chosen = Some(kind.into());
                            if convert.is_none() {
                                apply = chosen.clone();
                            }
                        }
                    }
                });
                ui.separator();
                ui.label(locale::tr(ui, "Enums"));
                let project = self.project.as_ref().unwrap();
                for (group, prefix) in [("enums", "LocalEnum"), ("externalEnums", "ExternEnum")] {
                    for def in project.json["defs"][group].as_array().into_iter().flatten() {
                        let name = text(def, "identifier", "Enum");
                        if source_controls::button(ui, RichText::new(name)).clicked() {
                            chosen = Some(format!("{prefix}.{name}"));
                            if convert.is_none() {
                                apply = chosen.clone();
                            }
                        }
                    }
                }
                if project.json["defs"]["enums"].as_array().is_none_or(Vec::is_empty)
                    && project.json["defs"]["externalEnums"].as_array().is_none_or(Vec::is_empty)
                {
                    ui.label(locale::tr(ui, "Create an enum in the Enum panel first."));
                    if source_controls::button(ui, "Open enum panel").clicked() {
                        self.show_panel(Some(Panel::Enums));
                        dismiss = true;
                    }
                }
                if convert.is_some() && let Some(kind) = &chosen {
                    ui.separator();
                    ui.colored_label(
                        theme::ACCENT,
                        locale::tr(ui, "Converting changes existing values and defaults. Values incompatible with the chosen type will be cleared."),
                    );
                    let label = if kind.contains('.') { kind } else { locale::tr(ui, kind) };
                    ui.label(locale::format(ui, "Convert to {kind}", &[("kind", label)]));
                    if source_controls::button(ui, "Convert all values").clicked() {
                        apply = chosen.clone();
                    }
                }
            });
        if dismiss {
            open = false;
        }
        if let Some(kind) = apply {
            if let Some(uid) = convert {
                if self
                    .edit_recoverable("Field type converted", |p| {
                        p.convert_field_type(uid, &kind, array, true)
                    })
                    .is_some()
                {
                    open = false;
                }
            } else {
                let fields = self.project.as_ref().unwrap();
                let fields = owner
                    .and_then(|uid| {
                        fields.json["defs"]["entities"]
                            .as_array()
                            .and_then(|a| a.iter().find(|d| d["uid"] == uid))
                    })
                    .map(|d| &d["fieldDefs"])
                    .unwrap_or(&fields.json["defs"]["levelFields"]);
                let base = kind.split('.').next_back().unwrap_or(&kind);
                let name = unique_name(
                    base,
                    fields
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|d| text(d, "identifier", "")),
                );
                if let Some(uid) = self.edit("Field created", |p| {
                    p.add_field_definition(owner, &name, &kind, array)
                }) {
                    self.field_uid = Some(uid);
                    open = false;
                }
            }
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, (owner, array, convert, chosen)));
        } else {
            ctx.data_mut(|d| d.remove::<(Option<i64>, bool, Option<i64>, Option<String>)>(id));
        }
    }
}

impl EditorApp {
    fn source_layer_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-layer-type-picker");
        let mut open = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        let mut chosen = None;
        egui::Window::new(locale::tr_ctx(ctx, "Create a layer"))
            .id(id)
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                for (kind, description) in [
                    (
                        "IntGrid",
                        "Integer grids for collisions, game data and automatic tiles.",
                    ),
                    (
                        "Entities",
                        "Instances of gameplay objects, such as the player or items.",
                    ),
                    ("Tiles", "Image tiles picked from a tileset."),
                    (
                        "AutoLayer",
                        "Tiles rendered automatically using rules and a source IntGrid layer.",
                    ),
                ] {
                    if source_controls::add_sized_button(
                        ui,
                        [140., 42.],
                        egui::Button::new(locale::tr(ui, kind)),
                    )
                    .clicked()
                    {
                        chosen = Some(kind);
                    }
                    ui.label(locale::tr(ui, description));
                    ui.add_space(8.);
                }
            });
        if let Some(kind) = chosen {
            self.new_kind = kind.into();
            self.source_create_definition("layers", None);
            open = false;
        }
        ctx.data_mut(|d| d.insert_temp(id, open));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_panels_keep_the_form_visible_under_the_read_only_notice() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project::new();
        let original = dir.path().join("Map.ldtk");
        project.save(&original).unwrap();
        let backup = project.backup().unwrap().unwrap().join("Map.ldtk");
        let mut app = EditorApp::empty();
        app.set_project(Project::load(backup).unwrap());
        let before = app.project.as_ref().unwrap().json.clone();
        for panel in [Panel::Project, Panel::Layers, Panel::Help] {
            app.show_panel(Some(panel));
            let mut out = egui::FullOutput::default();
            for pressed in [true, false] {
                let pos = egui::pos2(294., 240.);
                out = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.))),
                        events: vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE },
                        ],
                        ..Default::default()
                    },
                    |ctx| { app.source_panel(ctx, panel); },
                );
            }
            let label = |name| out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == name));
            assert_eq!(label("This panel is disabled for backup files."), panel != Panel::Help);
            if panel == Panel::Project {
                assert!(label("Backup on save"));
                assert!(label("RESTORE THIS BACKUP"));
            }
            assert_eq!(app.project.as_ref().unwrap().json, before);
        }
    }

    #[test]
    fn modal_panel_uses_the_source_canvas_mask_and_keeps_recovery_above_it() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let draw = |app: &mut EditorApp, panel| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    app.canvas_notifications(ctx, screen);
                    assert!(app.source_panel(ctx, panel));
                    ctx.move_to_top(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("source-definition-panel"),
                    ));
                },
            )
        };
        draw(&mut app, Panel::Level);
        app.last_chance = Some(("Level resized".into(), Instant::now()));
        app.report_resize_removals(2);
        draw(&mut app, Panel::Level);
        let output = draw(&mut app, Panel::Level);
        let mask = |out: &egui::FullOutput| {
            out.shapes.iter().enumerate().find_map(|(index, shape)| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.right_bottom() == screen.right_bottom()
                        && rect.fill.r() == 0
                        && (1..255).contains(&rect.fill.a()) =>
                {
                    Some((index, rect.rect, rect.fill))
                }
                _ => None,
            })
        };
        let (shade, rect, fill) = mask(&output).expect("modal panel canvas mask");
        assert_eq!(rect, Rect::from_min_max(egui::pos2(300., 0.), screen.max));
        assert_eq!(fill, Color32::from_black_alpha(89));
        for label in ["Level resized", "2 entity(ies) deleted during resizing!"] {
            let notice = output
                .shapes
                .iter()
                .position(|shape| text_center(&shape.shape, label).is_some())
                .expect(label);
            assert!(notice > shade, "{label} must paint above the mask");
        }
        assert!(mask(&draw(&mut app, Panel::World)).is_none());
        assert!(mask(&draw(&mut app, Panel::Rules)).is_none());
    }

    #[test]
    fn header_controls_do_not_change_the_panel_body_position() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                for (panel, height) in
                    [(Panel::Layers, 53.), (Panel::Rules, 46.), (Panel::Help, 53.)]
                {
                    for padding in [0., 6.] {
                        ui.spacing_mut().button_padding.y = padding;
                        let top = ui.next_widget_position().y;
                        assert!(!source_panel_header(ui, panel, 400., height, title(panel), false));
                        assert_eq!(ui.next_widget_position().y, top + height, "{panel:?}");
                    }
                }
            });
        });
    }

    #[test]
    fn tileset_sidebar_uses_compact_rows_and_orders_untagged_before_named_groups() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let draw = |app: &mut EditorApp| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::Tilesets)),
            )
        };
        let center = |out: &egui::FullOutput, label| {
            out.shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
        };
        draw(&mut app);
        let empty = draw(&mut app);
        assert!(center(&empty, "No tileset").is_some());
        let library_help = "Tilesets are large images in which you pick sub-parts to draw them in your levels. They can also be used as icons for some specific elements (like Enums).";
        assert!(center(&empty, library_help).is_some());
        let builtin = app.project.as_mut().unwrap().ensure_embedded_icons().unwrap();
        app.definition_uid = Some(builtin);
        let single = draw(&mut app);
        assert!(center(&single, library_help).is_none());
        assert!(center(&single, "TILESETS").is_none());
        let row = single.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(row) if row.fill == theme::ACCENT && row.rect.width() > 200. => {
                Some(row.rect)
            }
            _ => None,
        }).expect("selected tileset row");
        assert_eq!(row.x_range(), egui::Rangef::new(29., 311.));
        assert!((row.top() - 198.).abs() <= 1.);
        assert!((row.height() - 33.).abs() <= 1.);
        let project = app.project.as_mut().unwrap();
        for (image, tag) in [("Canopy.png", "Forest"), ("Road.png", "city")] {
            let uid = project.add_tileset(Path::new(image), 16, 32, 32).unwrap();
            project.update_tileset_definition(uid, &json!({"tags":[tag]})).unwrap();
        }
        let tagged = draw(&mut app);
        let headings = ["UNTAGGED", "CITY", "FOREST"].map(|name| {
            center(&tagged, name).unwrap_or_else(|| panic!("missing group {name}")).y
        });
        assert!(headings[0] < headings[1] && headings[1] < headings[2]);
    }

    #[test]
    fn project_and_level_settings_scroll_the_header_with_the_form() {
        for (panel, width) in [(Panel::Project, 840.), (Panel::Level, 490.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            let mut time = 0.;
            let mut frame = |open, events| {
                time += 0.05;
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 500.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        if open {
                            assert!(app.source_panel(ctx, panel));
                        }
                    },
                )
            };
            let header = |output: &egui::FullOutput| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == theme::LIGHT
                            && rect.rect.width() >= 400.
                            && rect.rect.height() == 53. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
            };
            frame(true, vec![]);
            let initial = header(&frame(true, vec![])).expect("settings panel header");
            assert_eq!(initial.top(), 34.);
            frame(
                true,
                vec![
                    egui::Event::PointerMoved(egui::pos2(345., 330.)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -390.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..5 {
                frame(true, vec![]);
            }
            let scrolled = frame(true, vec![]);
            assert!(
                header(&scrolled).is_none_or(|rect| rect.top() < 20.),
                "{panel:?}: title stayed pinned"
            );
            frame(false, vec![]);
            let reopened = frame(true, vec![]);
            let reopened = header(&reopened).expect("reopened panel header");
            assert_eq!(reopened.top(), 34.);
            assert_eq!(reopened.width(), width, "{panel:?}");
        }
    }

    #[test]
    fn definition_panels_keep_their_full_width_and_scroll_in_narrow_windows() {
        for (panel, width) in [
            (Panel::Layers, 1000.),
            (Panel::Entities, 1000.),
            (Panel::Enums, 840.),
            (Panel::Tilesets, 1000.),
            (Panel::Fields, 790.),
            (Panel::World, 400.),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.);
            let mut app = EditorApp::empty();
            let mut project = Project::new();
            project.add_layer("Ground", "IntGrid", 16).unwrap();
            project.add_entity_def("Marker", 16, 16).unwrap();
            project.add_enum_definition("Kind", &[]).unwrap();
            app.set_project(project);
            let mut time = 0.;
            let mut draw = |window_width, events| {
                time += 0.05;
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(window_width, 600.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| assert!(app.source_panel(ctx, panel)),
                )
            };
            let header = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.fill == theme::LIGHT
                                && rect.rect.top() == 34.
                                && rect.rect.height() == 53.
                                && rect.rect.width() > 300. =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .expect("definition panel header")
            };
            let narrow_width = match panel {
                Panel::Fields => 720.,
                Panel::World => 340.,
                _ => 800.,
            };
            for window_width in [1440., narrow_width] {
                draw(window_width, vec![]);
                draw(window_width, vec![]);
                let initial = header(&draw(window_width, vec![]));
                assert_eq!(initial.x_range(), egui::Rangef::new(0., width), "{panel:?}");
            }
            draw(
                narrow_width,
                vec![
                    egui::Event::PointerMoved(egui::pos2(narrow_width - 60., 295.)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(-500., 0.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..6 {
                draw(narrow_width, vec![]);
            }
            let scrolled = header(&draw(narrow_width, vec![]));
            assert!(
                scrolled.left() < -30. && scrolled.right() <= narrow_width + 1.,
                "{panel:?}: {scrolled:?}"
            );
        }
    }

    #[test]
    fn source_layer_form_keeps_the_outer_scrollbar_out_of_the_column_grid() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        app.set_project(project);
        let mut time = 0.;
        let mut draw = |visible, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 620.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    if visible {
                        assert!(app.source_panel(ctx, Panel::Layers));
                    }
                },
            )
        };
        draw(true, vec![]);
        let initial = draw(true, vec![]);
        let form = initial.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == GROUP_BAR && rect.rect.top() == 104. => {
                Some(rect.rect)
            }
            _ => None,
        });
        assert_eq!(form.unwrap().x_range(), egui::Rangef::new(287., 979.));
        let title_visible = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                text_center(&shape.shape, "PROJECT LAYERS")
                    .is_some_and(|center| center.y >= 34. && shape.clip_rect.contains(center))
            })
        };
        assert!(title_visible(&initial));
        draw(true, vec![egui::Event::PointerMoved(egui::pos2(750., 305.))]);
        draw(true, vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0., -360.),
            modifiers: egui::Modifiers::NONE,
        }]);
        for _ in 0..5 {
            draw(true, vec![]);
        }
        assert!(!title_visible(&draw(true, vec![])));
        draw(false, vec![]);
        draw(true, vec![]);
        assert!(title_visible(&draw(true, vec![])));
    }

    #[test]
    fn source_layer_sidebar_keeps_full_width_compact_rows_and_selects_by_icon() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut project = Project::new();
        let lower = project.add_layer("Road", "Tiles", 16).unwrap();
        let upper = project.add_layer("Landscape_Collision", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let active_layer = |app: &EditorApp| {
            app.project
                .as_ref()
                .unwrap()
                .layer(app.selected.unwrap(), app.layer)
                .unwrap()["layerDefUid"]
                .as_i64()
                .unwrap()
        };
        assert_eq!(active_layer(&app), upper);
        app.definition_uid = Some(lower);
        app.show_panel(Some(Panel::Layers));
        assert_eq!(app.definition_uid, Some(upper));
        let mut time = 0.;
        let mut draw = |app: &mut EditorApp, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.))),
                    events,
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::Layers)),
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let selected = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == theme::ACCENT && rect.rect.width() > 100. => {
                Some(rect.rect)
            }
            _ => None,
        });
        assert_eq!(
            selected,
            Some(Rect::from_min_size(egui::pos2(25., 194.), egui::vec2(240., 34.)))
        );
        let list = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == theme::DARK && rect.rect.height() > 500. => {
                Some(rect.rect)
            }
            _ => None,
        });
        let list = list.expect("full-height layer list");
        assert_eq!(list.x_range(), egui::Rangef::new(20., 270.));
        assert!((list.top() - 189.).abs() < 0.5, "{list:?}");
        assert!(list.bottom() >= 875.5, "{list:?}");

        let pos = egui::pos2(65., 246.);
        for pressed in [true, false] {
            draw(&mut app, vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(app.definition_uid, Some(lower));
        assert_eq!(active_layer(&app), lower);
        app.source_reorder_definition(&ctx, "layers", upper, 1);
        assert_eq!(app.definition_uid, Some(upper));
        assert_eq!(app.definition_index("layers", upper), 1);
        assert_eq!(active_layer(&app), upper);
        app.source_create_definition("layers", None);
        assert_eq!(Some(active_layer(&app)), app.definition_uid);
        assert_ne!(active_layer(&app), upper);
    }

    fn draw_section(
        ctx: &egui::Context,
        open: &mut bool,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640., 480.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(360.);
                    *open = section(ui, "WORLD SETTINGS: \"World\"", *open, |ui| {
                        ui.label("Section content");
                    });
                });
            },
        )
    }
    #[test]
    fn source_collapser_hover_highlights_the_header_and_group_border_and_changes_cursor() {
        for (mut open, cursor) in [
            (true, egui::CursorIcon::ZoomOut),
            (false, egui::CursorIcon::Cell),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let output = draw_section(&ctx, &mut open, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, "WORLD SETTINGS: \"World\""))
                .unwrap();
            draw_section(&ctx, &mut open, vec![egui::Event::PointerMoved(pos)]);
            let output = draw_section(&ctx, &mut open, vec![]);
            assert_eq!(output.platform_output.cursor_icon, cursor);
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && rect.fill==Color32::from_rgb(255,204,0))),"hovered title must be yellow");
            assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && rect.stroke.color==Color32::from_rgb(255,204,0) && (!open || rect.rect.height()>28.))),"hovered expanded group must have a yellow content border");
            let output = draw_section(
                &ctx,
                &mut open,
                vec![egui::Event::PointerMoved(egui::pos2(620., 450.))],
            );
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::Default
            );
            assert!(!output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.rect.contains(pos) && (rect.fill==Color32::from_rgb(255,204,0) || rect.stroke.color==Color32::from_rgb(255,204,0)))));
        }
    }

    #[test]
    fn source_collapser_click_switches_the_hover_cursor_and_content_visibility() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut open = true;
        let output = draw_section(&ctx, &mut open, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "WORLD SETTINGS: \"World\""))
            .unwrap();
        for expected_open in [false, true] {
            draw_section(&ctx, &mut open, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                draw_section(
                    &ctx,
                    &mut open,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
            let output = draw_section(&ctx, &mut open, vec![]);
            assert_eq!(open, expected_open);
            assert_eq!(
                output.platform_output.cursor_icon,
                if open {
                    egui::CursorIcon::ZoomOut
                } else {
                    egui::CursorIcon::Cell
                }
            );
            assert_eq!(
                output
                    .shapes
                    .iter()
                    .any(|shape| text_center(&shape.shape, "Section content").is_some()),
                open
            );
        }
    }
    #[test]
    fn world_level_operations_follow_the_source_button_strip() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let mut draw = |time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 800.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::World)),
            )
        };
        draw(0.);
        draw(1.);
        let output = draw(2.);
        let rects = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => Some(rect),
            _ => None,
        });

        let button_rects = rects
            .clone()
            .filter(|shape| {
                let [r, g, b, _] = shape.fill.to_array();
                r > 100
                    && u16::from(r) > u16::from(g) * 2
                    && b < 50
                    && shape.rect.top() < 111.
                    && shape.rect.height() >= 40.
            })
            .map(|shape| (shape.rect.min, shape.rect.size()))
            .collect::<Vec<_>>();
        let expected = [
            (egui::pos2(17., 106.), egui::vec2(266.69, 42.66)),
            (egui::pos2(285.69, 106.), egui::vec2(42.66, 42.66)),
            (egui::pos2(330.34, 106.), egui::vec2(42.66, 42.66)),
        ];
        assert_eq!(button_rects.len(), expected.len());
        for (&(position, size), (source_position, source_size)) in button_rects.iter().zip(expected) {
            assert!((position - source_position).abs().max_elem() <= 1.);
            assert!((size - source_size).abs().max_elem() <= 1.);
        }
        assert!(
            rects.clone().any(|shape| {
                shape.fill == GROUP_BAR
                    && (shape.rect.min - egui::pos2(17., 166.)).abs().max_elem() <= 1.
            }),
            "World settings starts after the source 16 point gutter"
        );
    }

    #[test]
    fn multi_world_panel_uses_the_source_action_rows() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        project.add_world("Away").unwrap();
        app.set_project(project);
        let mut draw = |time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::World)),
            )
        };
        draw(0.);
        draw(1.);
        let output = draw(2.);
        let rects: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) => Some((rect.fill, rect.rect)),
                _ => None,
            })
            .collect();
        for (color, x, y, width, height) in [
            (theme::LIGHT, 16., 105., 269., 45.),
            (Color32::from_rgb(227, 77, 0), 286., 106., 43., 43.),
            (Color32::from_rgb(121, 17, 5), 330., 106., 43., 43.),
            (Color32::from_rgb(227, 77, 0), 17., 159., 266., 43.),
            (GROUP_BAR, 17., 218., 356., 28.),
        ] {
            assert!(
                rects.iter().any(|(fill, rect)| {
                    *fill == color
                        && (rect.min - egui::pos2(x, y)).abs().max_elem() <= 1.
                        && (rect.size() - egui::vec2(width, height)).abs().max_elem() <= 1.
                }),
                "missing action or heading at ({x}, {y}) with size ({width}, {height}); same-color controls: {:?}",
                rects.iter().filter(|(fill, _)| *fill == color).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn world_selector_escape_keeps_the_panel_and_keyboard_selects_the_next_world() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let mut project = Project::new();
        let away = project.add_world("Away").unwrap();
        let target = project.add_level(Some(away), "Other", 256, 256).unwrap();
        app.set_project(project);
        app.repaint = Some(ctx.clone());
        app.show_panel(Some(Panel::World));
        let draw = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    if app.panel == Some(Panel::World) {
                        app.source_panel(ctx, Panel::World);
                    }
                },
            )
        };
        let click = |app: &mut EditorApp, pos| {
            draw(app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                draw(
                    app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            draw(app, vec![])
        };
        let open = |app: &mut EditorApp| {
            let output = click(app, egui::pos2(220., 125.));
            assert!(egui::Popup::is_any_open(&ctx));
            output
        };
        let press = |app: &mut EditorApp, key| {
            for pressed in [true, false] {
                draw(
                    app,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        };
        draw(&mut app, vec![]);
        let original = app.selected;
        open(&mut app);
        for _ in 0..12 {
            draw(&mut app, vec![]);
        }
        let opened = draw(&mut app, vec![]);
        let active_row = opened
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(row) if row.fill == source_controls::FORM_OPTION_HIGHLIGHT => {
                    Some(row.rect)
                }
                _ => None,
            })
            .expect("highlighted world option");
        assert!((active_row.height() - 17.).abs() <= 1.);
        assert!((active_row.width() - 267.).abs() <= 2., "{active_row:?}");
        press(&mut app, Key::Escape);
        assert_eq!(app.panel, Some(Panel::World));
        assert!(app.world);
        assert_eq!(app.selected, original);
        assert!(!egui::Popup::is_any_open(&ctx));

        open(&mut app);
        press(&mut app, Key::ArrowDown);
        assert_eq!(app.selected, original);
        press(&mut app, Key::Enter);
        assert_eq!(app.selected, Some(target));
        assert_eq!(app.active_world, Some(away));
        assert!(!app.world);
        assert_eq!(app.panel, None);
        assert!(!egui::Popup::is_any_open(&ctx));
        app.show_panel(Some(Panel::World));
        open(&mut app);
        click(&mut app, active_row.center());
        assert_eq!(app.selected, original);
        assert_eq!(app.panel, None);
        assert!(!app.world);
    }

    #[test]
    fn world_and_level_forms_anchor_controls_and_reserve_the_world_scrollbar() {
        for (panel, xs) in [
            (Panel::World, &[143., 135.][..]),
            (Panel::Level, &[139.][..]),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            let mut draw = |time| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440., 800.),
                        )),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| assert!(app.source_panel(ctx, panel)),
                )
            };
            draw(0.);
            draw(1.);
            let output = draw(2.);
            let rects = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape) => Some(shape),
                _ => None,
            });
            for x in xs {
                assert!(
                    rects.clone().any(|shape| {
                        shape.rect.left() == *x
                            && shape.rect.width() == 60.
                            && shape.rect.height() == 33.
                    }),
                    "missing source-sized numeric box at x={x}"
                );
            }
            if panel == Panel::World {
                let headers = rects
                    .filter(|shape| shape.fill == GROUP_BAR && shape.rect.height() == 28.)
                    .map(|shape| (shape.rect.left(), shape.rect.right()))
                    .collect::<Vec<_>>();
                assert_eq!(
                    headers,
                    [(17., 373.); 3],
                    "World forms reserve the 10 point scrollbar"
                );
            } else {
                assert!(
                    rects.clone().any(|shape| {
                        shape.fill == GROUP_BAR && shape.rect.min == egui::pos2(21., 112.)
                    }),
                    "Current level keeps the source gutter below its header"
                );
            }
        }
    }

    fn draw_fields(
        app: &mut EditorApp,
        ctx: &egui::Context,
        owner: i64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_width(200.);
                    app.source_field_list(ui, Some(owner), 400.);
                });
                app.source_field_type_picker(ctx);
            },
        )
    }
    fn text_center(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_center(s, label)),
            _ => None,
        }
    }
    fn click_field(app: &mut EditorApp, ctx: &egui::Context, owner: i64, pos: egui::Pos2) {
        for pressed in [true, false] {
            draw_fields(
                app,
                ctx,
                owner,
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
    fn draw_first_field_definition(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let project = app.project.as_ref().unwrap();
        let def = app
            .field_owner
            .and_then(|owner| {
                project.json["defs"]["entities"]
                    .as_array()?
                    .iter()
                    .find(|entity| entity["uid"] == owner)
            })
            .map_or(&project.json["defs"]["levelFields"], |owner| &owner["fieldDefs"])[0]
            .clone();
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    app.source_field_properties(ui, &def);
                });
            },
        )
    }

    #[test]
    fn array_length_inputs_share_a_row_and_move_the_opposite_bound() {
        let mut project = Project::new();
        let uid = project
            .add_field_definition(None, "Entries", "Int", true)
            .unwrap();
        project
            .update_field_definition(uid, &json!({"arrayMaxLength":4}))
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = draw_first_field_definition(&mut app, &ctx, vec![]);
        let position = |label| {
            output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap_or_else(|| panic!("missing {label}"))
        };
        let min = position("min length");
        let max = position("4");
        assert!((min.y - max.y).abs() <= 1.);
        assert!(min.x < max.x);
        for pressed in [true, false] {
            draw_first_field_definition(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(min),
                    egui::Event::PointerButton {
                        pos: min,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        draw_first_field_definition(&mut app, &ctx, vec![egui::Event::Text("7".into())]);
        draw_first_field_definition(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
        assert_eq!(def["arrayMinLength"], 7);
        assert_eq!(def["arrayMaxLength"], 7);
    }

    #[test]
    fn bool_array_definition_renders_and_edits_the_saved_default_state() {
        let mut project = Project::new();
        let uid = project
            .add_field_definition(None, "Enabled", "Bool", true)
            .unwrap();
        project.set_field_default(uid, json!(true)).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = draw_first_field_definition(&mut app, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(0, 117, 255) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .expect("checked array default");
        for pressed in [true, false] {
            draw_first_field_definition(
                &mut app,
                &ctx,
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
        let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
        assert_eq!(
            def["defaultOverride"],
            json!({"id":"V_Bool","params":[false]})
        );
    }

    #[test]
    fn numeric_field_can_save_zero_as_an_explicit_default() {
        let mut project = Project::new();
        project
            .add_field_definition(None, "Count", "Int", false)
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = draw_first_field_definition(&mut app, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "0"))
            .expect("zero default");
        for pressed in [true, false] {
            draw_first_field_definition(
                &mut app,
                &ctx,
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
        draw_first_field_definition(&mut app, &ctx, vec![egui::Event::Text("0".into())]);
        draw_first_field_definition(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        let def = &app.project.as_ref().unwrap().json["defs"]["levelFields"][0];
        assert_eq!(def["defaultOverride"], json!({"id":"V_Int","params":[0]}));
    }

    #[test]
    fn reference_policy_resets_symmetry_and_initializes_a_specific_target() {
        let mut project = Project::new();
        let owner = project.add_entity_def("Door", 16, 16).unwrap();
        let uid = project
            .add_field_definition(Some(owner), "Pair", "EntityRef", false)
            .unwrap();
        project
            .update_field_definition(uid, &json!({"symmetricalRef":true}))
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.field_owner = Some(owner);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let click = |app: &mut EditorApp, label| {
            let output = draw_first_field_definition(app, &ctx, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap_or_else(|| panic!("missing {label}"));
            for pressed in [true, false] {
                draw_first_field_definition(
                    app,
                    &ctx,
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
        };
        click(&mut app, "Only another 'Door's");
        draw_first_field_definition(
            &mut app,
            &ctx,
            [Key::ArrowDown, Key::ArrowDown, Key::Enter]
                .into_iter()
                .map(|key| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                })
                .collect(),
        );
        let def = |app: &EditorApp| {
            app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0].clone()
        };
        assert_eq!(def(&app)["allowedRefs"], "OnlySpecificEntity");
        assert_eq!(def(&app)["allowedRefsEntityUid"], owner);
        assert_eq!(def(&app)["symmetricalRef"], true);
        click(&mut app, "Only a specific Entity");
        click(&mut app, "Any entity");
        assert_eq!(def(&app)["allowedRefs"], "Any");
        assert_eq!(def(&app)["symmetricalRef"], false);
    }

    #[test]
    fn custom_field_world_view_checkbox_is_visible_only_for_level_fields() {
        let mut project = Project::new();
        let entity = project.add_entity_def("Actor", 16, 16).unwrap();
        for owner in [None, Some(entity)] {
            project
                .add_field_definition(owner, "Caption", "String", false)
                .unwrap();
        }
        let mut app = EditorApp::empty();
        app.set_project(project);
        for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
            app.field_owner = owner;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut draw = |time| {
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440., 1200.),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        assert!(app.source_panel(ctx, panel));
                    },
                )
            };
            draw(0.);
            let output = draw(1.);
            let has_label = |label| {
                output
                    .shapes
                    .iter()
                    .any(|shape| text_center(&shape.shape, label).is_some())
            };
            assert!(has_label("Always show"), "field form must be visible");
            assert_eq!(has_label("Show in world view"), owner.is_none());
        }
    }

    #[test]
    fn field_display_controls_follow_the_owner_and_mode_in_one_row() {
        let mut project = Project::new();
        let entity = project.add_entity_def("Actor", 16, 16).unwrap();
        let health = project
            .add_field_definition(None, "Health", "Int", false)
            .unwrap();
        project.set_field_default(health, json!(5)).unwrap();
        project
            .update_field_definition(
                health,
                &json!({"editorDisplayMode":"NameAndValue","editorDisplayScale":1.25,
                    "editorShowInWorld":true,"editorDisplayPos":"Beneath"}),
            )
            .unwrap();
        let path = project
            .add_field_definition(Some(entity), "Path", "Point", true)
            .unwrap();
        project
            .update_field_definition(
                path,
                &json!({"editorDisplayMode":"PointPath","editorLinkStyle":"CurvedArrow",
                    "editorDisplayScale":1.25,"editorDisplayPos":"Beneath"}),
            )
            .unwrap();
        for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.field_owner = owner;
            app.settings.set_ui_state_int("extraEntityDetails", None, 0);
            let draw = |app: &mut EditorApp, time| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440., 900.),
                        )),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| assert!(app.source_panel(ctx, panel)),
                )
            };
            draw(&mut app, 0.);
            let output = draw(&mut app, 1.);
            let at = |label| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| text_center(&shape.shape, label))
            };
            assert!(at("Beneath").is_none(), "unavailable text position");
            if owner.is_none() {
                let world = at("Show in world view").unwrap();
                assert!(world.y < 500., "Display row is too low: {}", world.y);
                let scale = at("125").expect("scale displayed as a percentage");
                assert!((world.y - scale.y).abs() <= 2.);
                assert!(at("1234").is_some(), "paired prefix and suffix preview");
            } else {
                let mode = at("Show path of points").unwrap();
                let style = at("Curved arrow").unwrap();
                assert!((mode.y - style.y).abs() <= 2.);
                assert!(at("125").is_none(), "graphical modes have no text scale");
                assert!(at("Show in world view").is_none());
            }
        }
    }

    #[test]
    fn entity_field_display_color_stays_on_the_same_row_as_scale() {
        let mut project = Project::new();
        let owner = project.add_entity_def("Actor", 16, 16).unwrap();
        let uid = project
            .add_field_definition(Some(owner), "Costs", "Int", true)
            .unwrap();
        project
            .update_field_definition(
                uid,
                &json!({
                    "editorDisplayMode":"NameAndValue",
                    "editorDisplayScale":1.25,
                    "editorDisplayColor":"#0135AB",
                }),
            )
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.settings.set_ui_state_int("extraEntityDetails", None, 0);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let draw = |app: &mut EditorApp, time| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::Entities)),
            )
        };
        draw(&mut app, 0.);
        let output = draw(&mut app, 1.);
        let scale = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "125"))
            .expect("display scale");
        let color = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(1, 53, 171) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .expect("display color");
        assert!(
            (scale.y - color.y).abs() <= 2.,
            "scale {scale:?}, color {color:?}"
        );
    }

    #[test]
    fn level_point_picker_keeps_the_form_open_through_cancel_and_canvas_commit() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let level = project.level_keys()[0];
        let uid = project
            .add_field_definition(None, "Target", "Point", false)
            .unwrap();
        let original = json!({"cx":10,"cy":8});
        project
            .set_field_editor_value(level, None, uid, original.clone())
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.show_panel(Some(Panel::Level));
        app.fit = false;
        app.viewport.center = [128., 128.];
        app.viewport.zoom = 1.;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let draw = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    let level_point_picker_was_open = app.picking_point_in_level_panel();
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (rect, response) =
                                ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                            app.canvas_input(ctx, rect, &response);
                        });
                    app.dialogs(ctx, level_point_picker_was_open);
                },
            )
        };
        let click = |app: &mut EditorApp, pos| {
            draw(app, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                draw(
                    app,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
        };
        let control = |output: &egui::FullOutput, label| {
            output.shapes
                .iter()
                .find_map(|shape| text_center(&shape.shape, label))
                .unwrap()
        };
        let value = |app: &EditorApp| {
            app.project.as_ref().unwrap().level(level).unwrap()["fieldInstances"][0]["__value"]
                .clone()
        };
        draw(&mut app, vec![]);
        let pick = control(&draw(&mut app, vec![]), "10,8");
        click(&mut app, pick);
        assert!(app.picking_point());
        assert_eq!(app.panel, Some(Panel::Level));
        let cancel = control(&draw(&mut app, vec![]), "CANCEL");
        click(&mut app, cancel);
        assert!(!app.picking_point());
        assert_eq!(value(&app), original);

        click(&mut app, pick);
        let point = app
            .viewport
            .to_screen([104., 120.], screen.min.into(), screen.size().into());
        click(&mut app, point.into());
        assert_eq!(value(&app), json!({"cx":6,"cy":7}));
        assert!(!app.picking_point());
        assert_eq!(app.panel, Some(Panel::Level));
        let updated = control(&draw(&mut app, vec![]), "6,7");
        click(&mut app, updated);
        click(&mut app, egui::pos2(450., 78.));
        assert!(!app.picking_point());
        assert!(app.panel.is_none());
    }

    #[test]
    fn level_field_panel_returns_to_current_level_when_dismissed() {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let draw = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    if let Some(panel) = app.panel {
                        app.source_panel(ctx, panel);
                    }
                },
            )
        };
        app.show_panel(Some(Panel::Fields));
        draw(&mut app, Vec::new());
        let close = egui::pos2(750., 78.);
        draw(&mut app, vec![egui::Event::PointerMoved(close)]);
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(close),
                    egui::Event::PointerButton {
                        pos: close,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(app.panel, Some(Panel::Level));

        app.show_panel(Some(Panel::Fields));
        draw(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(app.panel, Some(Panel::Level));
    }

    #[test]
    fn entity_and_level_field_definitions_share_one_control_column() {
        fn text_x(shape: &egui::Shape, label: &str) -> Option<f32> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => Some(text.pos.x),
                egui::Shape::Vec(shapes) => shapes
                    .iter()
                    .filter_map(|shape| text_x(shape, label))
                    .max_by(f32::total_cmp),
                _ => None,
            }
        }

        let mut project = Project::new();
        let entity = project.add_entity_def("Actor", 16, 16).unwrap();
        for owner in [None, Some(entity)] {
            let field = project
                .add_field_definition(owner, "Caption", "String", false)
                .unwrap();
            project
                .update_field_definition(
                    field,
                    &json!({"doc":"Example documentation","canBeNull":false}),
                )
                .unwrap();
            project
                .set_field_default(field, json!("Original caption"))
                .unwrap();
        }
        for (owner, panel) in [(None, Panel::Fields), (Some(entity), Panel::Entities)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.field_owner = owner;
            app.settings.set_ui_state_int("extraEntityDetails", None, 0);
            let mut draw = |time| {
                ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440., 900.),
                        )),
                        ..Default::default()
                    },
                    |ctx| assert!(app.source_panel(ctx, panel)),
                )
            };
            draw(0.);
            draw(1.);
            let output = draw(2.);
            let column = |label| {
                output
                    .shapes
                    .iter()
                    .filter_map(|item| text_x(&item.shape, label))
                    .max_by(f32::total_cmp)
                    .unwrap_or_else(|| panic!("{label} is missing in {}", title(panel)))
            };
            if owner.is_none() {
                assert!(
                    output.shapes.iter().any(|item| {
                        matches!(&item.shape, egui::Shape::Rect(shape) if
                            shape.fill == theme::LIGHT
                            && shape.rect == Rect::from_min_size(egui::pos2(0., 34.), egui::vec2(790., 53.))
                        )
                    }),
                    "level field definitions use a 750-point body with 20-point side padding"
                );
                let form_x = column("GENERAL SETTINGS");
                assert!(
                    (form_x - 228.).abs() <= 1.,
                    "field properties start at x={form_x}, expected x=228"
                );
            }
            let identifier = column("Caption");
            for control in ["Example documentation", "Original caption"] {
                let x = column(control);
                assert!(
                    (x - identifier).abs() <= 1.,
                    "{control} starts at {x}, but the field identifier starts at {identifier} in {}",
                    title(panel)
                );
            }
        }
    }

    #[test]
    fn file_extension_editor_preserves_source_array_schema_and_normalization() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Asset", "FilePath", false)
            .unwrap();
        let types = source_file_types(" .PNG, jpg; PNG ..bad ...bad __ * .tar.gz ");
        assert_eq!(types, json!([".png", ".jpg", "..bad", ".tar.gz"]));
        p.update_field_definition(uid, &json!({"acceptFileTypes":types}))
            .unwrap();
        assert_eq!(
            p.json["defs"]["levelFields"][0]["acceptFileTypes"],
            json!([".png", ".jpg", "..bad", ".tar.gz"])
        );
        assert_eq!(source_file_types("???"), Value::Null);
        assert_eq!(source_file_types("__ ..."), json!([]));
    }
    #[test]
    fn native_add_field_chooses_type_before_mutating_and_can_undo() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        let owner = p.add_entity_def("Actor", 16, 16).unwrap();
        app.set_project(p);
        let before = app.project.as_ref().unwrap().json.clone();
        let output = draw_fields(&mut app, &ctx, owner, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text_center(&s.shape, "+ SINGLE VALUE"))
            .unwrap();
        click_field(&mut app, &ctx, owner, pos);
        assert_eq!(
            app.project.as_ref().unwrap().json,
            before,
            "opening type picker must not create an Int field"
        );
        draw_fields(&mut app, &ctx, owner, vec![]);
        let output = draw_fields(&mut app, &ctx, owner, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| text_center(&s.shape, "Float"))
            .expect("source type chooser");
        click_field(&mut app, &ctx, owner, pos);
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["entities"][0]["fieldDefs"][0]["__type"],
            "Float"
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }
    #[test]
    fn sparse_saved_selection_ids_do_not_expand_to_the_bounding_rectangle() {
        let mut app = EditorApp::empty();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] =
            json!([{"uid":10,"tileGridSize":16,"pxWid":64,"pxHei":64,"__cWid":4,"__cHei":4}]);
        app.set_project(p);
        app.tile_rect = [0, 0, 3, 3];
        app.atlas_selection = Some((10, vec![0, 2, 8, 10]));
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 2, 8, 10]);
        app.atlas_selection = None;
        assert_eq!(
            app.selected_tile_ids_for(10),
            vec![0, 1, 2, 4, 5, 6, 8, 9, 10]
        );
    }
    #[test]
    fn nested_field_form_labels_remain_fully_readable_inside_their_third_column() {
        fn texts<'a>(
            shape: &'a egui::Shape,
            clip: Rect,
            items: &mut Vec<(Rect, &'a egui::epaint::TextShape)>,
        ) {
            match shape {
                egui::Shape::Text(text) => items.push((clip, text)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, clip, items)
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::load(resource_path("examples/original/Entities.ldtk")).unwrap();
        project.migrate_to_current().unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.settings.set_ui_state_int("extraEntityDetails", None, 0);
        let mut time = 0.;
        let mut draw = |app: &mut EditorApp| {
            time += 1.;
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Entities);
                },
            )
        };
        draw(&mut app);
        draw(&mut app);
        let output = draw(&mut app);
        let mut items = Vec::new();
        for shape in &output.shapes {
            texts(&shape.shape, shape.clip_rect, &mut items);
        }
        let general = items
            .iter()
            .find(|(_, text)| text.galley.job.text == "GENERAL SETTINGS")
            .unwrap()
            .1;
        let field_text = |label| {
            items
                .iter()
                .find(|(_, text)| {
                    text.pos.y > general.pos.y
                        && text
                            .galley
                            .job
                            .text
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            == label
                })
                .unwrap_or_else(|| panic!("missing third-column label {label}"))
        };
        for label in ["Field identifier", "Documentation", "Limits", "Can be null"] {
            let (clip, text) = field_text(label);
            let bounds = Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                bounds.left() >= general.pos.x - 0.1,
                "{label} escapes left into the field list: {bounds:?}, third column {}",
                general.pos.x
            );
            assert!(
                clip.expand(0.1).contains_rect(bounds),
                "{label} is clipped instead of readable: text {bounds:?}, clip {clip:?}"
            );
            assert!(text.galley.rows.len() <= 2, "{label} wraps inside words");
            assert!(
                bounds.right() <= general.pos.x + FIELD_LABEL_WIDTH - 32. + 0.1,
                "{label} overlaps its label-cell padding/info pill: {bounds:?}"
            );
            assert!(
                (text.galley.job.sections[0].format.font_id.size - 12.667).abs() < 0.001,
                "source dt labels use 9.5pt"
            );
        }
        let (_, default) = field_text("Default value");
        let (_, limits) = field_text("Limits");
        let (_, nullable) = field_text("Can be null");
        let (_, range_separator) = field_text("to");
        assert!(default.pos.y < limits.pos.y && limits.pos.y < nullable.pos.y);
        assert!((limits.pos.y - range_separator.pos.y).abs() <= 3.);
        let body = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == theme::MEDIUM
                        && rect.rect.left() == 0.
                        && rect.rect.height() > 500. =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            body.right() <= 1000.,
            "third column body must stay under the original 1000-point header: {body:?}"
        );
    }
    #[test]
    fn definition_list_background_requests_clipboard_on_secondary_click() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.panel = Some(Panel::Layers);
        let pos = egui::pos2(100., 380.);
        let mut render = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.source_sidebar(ui, Panel::Layers, 550.);
                    });
                },
            )
        };
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = render(vec![egui::Event::PointerMoved(pos)]);
        let _ = render(vec![button(true)]);
        let output = render(vec![button(false)]);
        assert!(
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::RequestPaste)
        );
    }

    #[test]
    fn source_entity_panel_has_original_header_geometry_and_integrated_fields() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/original/Test_file_for_API_showing_all_features.ldtk");
        let mut project = Project::load(path).unwrap();
        project.migrate_to_current().unwrap();
        app.set_project(project);
        let original = app.project.as_ref().unwrap().json.clone();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.source_panel(ctx, Panel::Entities);
            },
        );
        let output = ctx.run(
            egui::RawInput {
                time: Some(1.0),
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                assert!(app.source_panel(ctx, Panel::Entities));
            },
        );
        let header = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Rect(shape)
                    if shape.fill == theme::LIGHT
                        && shape.rect.width() > 900.
                        && shape.rect.height() == 53. =>
                {
                    Some(shape.rect)
                }
                _ => None,
            })
            .next()
            .expect("original full-width panel header");
        assert_eq!(
            header,
            Rect::from_min_size(egui::pos2(0., 34.), egui::vec2(1000., 53.))
        );
        let top_border = Rect::from_min_max(header.min, egui::pos2(header.right(), 35.));
        assert!(output.shapes.iter().any(|shape| {
            shape.clip_rect == top_border
                && matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.fill == Color32::from_rgb(115, 127, 157)
                        && rect.rect == header
                        && rect.corner_radius.nw == 0)
        }));
        let body = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Rect(r)
                    if r.fill == theme::MEDIUM && r.rect.left() == 0. && r.rect.height() > 500. =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .next()
            .expect("source panel body");
        assert!(
            body.right() <= 1000.,
            "body expanded beyond header: {body:?}"
        );
        let visible_position = |output: &egui::FullOutput, label| {
            output.shapes.iter().find_map(|shape| {
                text_center(&shape.shape, label).filter(|pos| shape.clip_rect.contains(*pos))
            })
        };
        let max_count = visible_position(&output, "Max count").expect("entity settings");
        let add_field = visible_position(&output, "+ ARRAY").expect("field toolbar");
        let heading = visible_position(&output, "GENERAL SETTINGS").expect("field properties");
        let _ = ctx.run(
            egui::RawInput {
                time: Some(1.1),
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                events: vec![
                    egui::Event::PointerMoved(heading + egui::vec2(40., 24.)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -900.),
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ctx| {
                app.source_panel(ctx, Panel::Entities);
            },
        );
        let output = ctx.run(
            egui::RawInput {
                time: Some(1.2),
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                ..Default::default()
            },
            |ctx| {
                app.source_panel(ctx, Panel::Entities);
            },
        );
        assert_eq!(visible_position(&output, "Max count"), Some(max_count));
        assert_eq!(visible_position(&output, "+ ARRAY"), Some(add_field));
        assert_ne!(visible_position(&output, "GENERAL SETTINGS"), Some(heading));
        assert_eq!(app.field_owner, app.definition_uid);
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
}
