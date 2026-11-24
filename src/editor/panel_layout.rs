use super::*;

#[path = "definition_sidebar.rs"]
mod definition_sidebar;
#[path = "entity_definition_form.rs"]
mod entity_definition_form;
#[path = "field_definition_form.rs"]
mod field_definition_form;

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
    let sense = if backup_locked {
        Sense::hover()
    } else {
        Sense::click()
    };
    let close = ui.interact(close_rect, header.id.with("close"), sense);
    if !backup_locked {
        close
            .clone()
            .on_hover_cursor(egui::CursorIcon::PointingHand);
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
        if hovered {
            Color32::WHITE
        } else {
            theme::LIGHT
        },
        Stroke::new(
            1_f32,
            if hovered {
                Color32::WHITE
            } else {
                theme::MEDIUM
            },
        ),
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
            Stroke::new(1_f32, Color32::from_black_alpha(45));
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
            let ink = if open || hovered {
                Color32::BLACK
            } else {
                theme::ACCENT
            };
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
                .map(Vec::as_slice)
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
        let scroll_panel = matches!(
            panel,
            Panel::Project | Panel::Level | Panel::Layers | Panel::Tilesets
        );
        let wrapper_width = width + if scroll_panel { 10. } else { 0. };
        let scroll_x = fixed_width && wrapper_width > screen.width();
        let outer_scroll = scroll_panel || scroll_x;
        let panel_width = wrapper_width.min(screen.width());
        let rules = panel == Panel::Rules;
        let backup_locked = matches!(
            panel,
            Panel::Project
                | Panel::World
                | Panel::Level
                | Panel::Layers
                | Panel::Entities
                | Panel::Fields
                | Panel::Enums
                | Panel::Tilesets
                | Panel::Rules
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
                                ui,
                                panel,
                                width,
                                header_height,
                                &heading,
                                backup_locked,
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
                                        let inner_width =
                                            if world_panel { width } else { width - 40. };
                                        ui.set_width(inner_width);
                                        ui.set_max_width(inner_width);
                                        ui.ctx()
                                            .data_mut(|data| data.insert_temp(active_id(), true));
                                        if world_panel {
                                            ui.spacing_mut().scroll =
                                                source_controls::scroll_style();
                                            ui.visuals_mut().clip_rect_margin = 0.;
                                            let scroll_height = height - header_height;
                                            // Keep animated scrollbar width inside the fixed panel.
                                            let (_, rect) = ui.allocate_space(egui::vec2(
                                                inner_width,
                                                scroll_height,
                                            ));
                                            let mut viewport =
                                                ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                            viewport.set_clip_rect(
                                                rect.intersect(viewport.clip_rect()),
                                            );
                                            egui::ScrollArea::vertical()
                                                .id_salt(("source-panel-body", title(panel)))
                                                .max_height(scroll_height)
                                                .auto_shrink([false, false])
                                                .show(&mut viewport, |ui| {
                                                    egui::Frame::new().inner_margin(16).show(
                                                        ui,
                                                        |ui| {
                                                            let content_width =
                                                                ui.available_width();
                                                            ui.set_width(content_width);
                                                            ui.set_max_width(content_width);
                                                            self.source_world_form(ui);
                                                        },
                                                    );
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
                                                app.source_sidebar(ui, panel, list_width, height);
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
                                                ui.ctx()
                                                    .data_mut(|d| d.insert_temp(active_id(), true));
                                                if scroll_panel {
                                                    app.properties(ui, panel);
                                                } else if panel == Panel::Enums {
                                                    app.source_enum_form(ui);
                                                } else if panel == Panel::Entities {
                                                    ui.set_width((right - 10.).max(60.));
                                                    app.source_entity_form(ui, body_height);
                                                } else {
                                                    egui::ScrollArea::vertical()
                                                        .id_salt((
                                                            "source-panel-body",
                                                            title(panel),
                                                        ))
                                                        .max_height(body_height)
                                                        .auto_shrink([false, false])
                                                        .show(ui, |ui| {
                                                            ui.set_width((right - 10.).max(60.));
                                                            app.properties(ui, panel);
                                                        });
                                                }
                                                ui.ctx().data_mut(|d| {
                                                    d.insert_temp(active_id(), false)
                                                });
                                            },
                                        )
                                        .response
                                        .rect
                                        .height()
                                    };
                                    if scroll_panel {
                                        ui.set_width(width - 40.);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::TOP),
                                            |ui| {
                                                let form_height = form(self, ui);
                                                ui.add_space(gap);
                                                sidebar(self, ui, form_height.max(body_height));
                                            },
                                        );
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

                            let frame_id =
                                ui.make_persistent_id(("definition-panel-frame", title(panel)));
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
            if ctx
                .pointer_hover_pos()
                .is_some_and(|pos| body.contains(pos))
            {
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
                    egui::Event::PointerButton {
                        pos, pressed: true, ..
                    } if mask.contains(*pos) && (pos.y < top || pos.x >= right) => Some(*pos),
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
}

#[cfg(test)]
#[path = "panel_layout_tests.rs"]
mod tests;
