//! World canvas context actions and the source level-location picker.
use super::{panels::source_controls, *};
use serde_json::json;

#[derive(Default)]
pub(super) struct WorldContext {
    menu: Option<WorldMenu>,
    pub(super) picking: bool,
    pointer: Option<[f32; 2]>,
    no_room: bool,
}
impl WorldContext {
    pub(super) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }
}

#[derive(Clone, Copy)]
struct WorldMenu {
    screen: egui::Pos2,
    position: [f32; 2],
    level: Option<LevelKey>,
}
#[derive(Clone, Copy)]
enum MenuAction {
    New,
    Duplicate(LevelKey),
    Delete(LevelKey),
    World(usize),
    Move(LevelKey, usize),
}

struct MenuRow {
    action: MenuAction,
    label: String,
    detail: Option<String>,
    enabled: bool,
}

impl MenuRow {
    fn height(&self, separator: bool) -> f32 {
        (if self.detail.is_some() { 47. } else { 31. }) + if separator { 1. } else { 0. }
    }
}

#[derive(Clone, Copy, Debug)]
struct Spot {
    rect: [f32; 4],
    size: [i32; 2],
    index: Option<usize>,
    overlaps: bool,
}

fn container(p: &Project, world: Option<usize>) -> &Value {
    world
        .and_then(|i| p.json["worlds"].get(i))
        .unwrap_or(&p.json)
}
fn round(value: f32) -> f32 {
    value.round()
}
fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[0] + a[2] > b[0] && a[1] < b[3] && a[1] + a[3] > b[1]
}
fn normalize_linear(p: &mut Project, world: Option<usize>) {
    if text(container(p, world), "worldLayout", "Free").starts_with("Linear") {
        for key in p.level_keys_in_world(world) {
            if let Some(level) = p.level_mut(key) {
                level["worldX"] = json!(-1);
                level["worldY"] = json!(-1);
            }
        }
    }
}
fn level_name(p: &Project, base: &str) -> String {
    panels::unique_name(
        &p.cleanup_identifier(base),
        p.level_keys()
            .into_iter()
            .filter_map(|key| p.level(key)?["identifier"].as_str()),
    )
}
impl EditorApp {
    fn context_placements(&self) -> Vec<(LevelKey, [f32; 4])> {
        self.project
            .as_ref()
            .map(|p| {
                canvas::placements(
                    p,
                    LevelKey {
                        world: self.active_world,
                        level: 0,
                    },
                )
            })
            .unwrap_or_default()
    }
    fn context_pointer(&self, ctx: &egui::Context, rect: Rect, pointer: egui::Pos2) -> [f32; 2] {
        if let (Some(project), Some(key)) = (&self.project, self.selected) {
            self.canvas_pointer_pixels(project, key, rect, pointer, ctx.pixels_per_point())
                .1
        } else {
            // An empty world has no level to use as the coordinate origin.
            self.viewport.pointer_pixel(
                pointer.into(),
                rect.min.into(),
                rect.size().into(),
                [0.; 2],
                ctx.pixels_per_point(),
            )
        }
    }
    pub(super) fn world_context_menu(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        let busy = self.is_busy();
        let keep_menu_during_save = self.world_context.menu_open()
            && (self.saving.is_some() || self.holding_save_completion());
        let covered_by_search = self.command_palette && self.world_context.menu_open();
        if self.project.as_ref().is_none_or(Project::is_backup)
            || self.picking_entity_ref()
            || self
                .panel
                .is_some_and(|panel| !matches!(panel, Panel::Entity | Panel::World))
            || busy && !keep_menu_during_save
            || self.pending.is_some()
            || self.crash.is_some()
            || self.clicktrap
            || text_editor::is_open(ctx)
            || world_form_dialog::is_open(ctx)
            || help::changelog_is_open(ctx)
            || self.command_palette && !covered_by_search
            || self.command_open
            || !self.resource_missing.is_empty()
        {
            self.cancel_world_context();
            return false;
        }
        if self.world_context.picking {
            if !self.world {
                self.cancel_world_context();
                return false;
            }
            if self.fit {
                self.camera_motion.cancel();
                if let (Some(p), Some(key)) = (&self.project, self.selected) {
                    self.viewport.fit_world(
                        canvas::bounds(p, key, true),
                        rect.size().into(),
                        ctx.pixels_per_point(),
                    );
                }
                self.fit = false;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.cancel_world_context();
                return true;
            }
            if response.hovered()
                && let Some(pointer) = ctx.pointer_hover_pos()
            {
                let (pan, delta, primary, space) = ctx.input(|i| {
                    (
                        i.pointer.button_down(egui::PointerButton::Middle),
                        i.pointer.delta(),
                        i.pointer.primary_pressed(),
                        i.key_down(egui::Key::Space),
                    )
                });
                if pan || (space && ctx.input(|i| i.pointer.primary_down())) {
                    if delta.length_sq() > 0.0 {
                        self.camera_motion.cancel_pan();
                    }
                    self.viewport.center[0] -= delta.x / self.viewport.zoom;
                    self.viewport.center[1] -= delta.y / self.viewport.zoom;
                }
                self.wheel_zoom(ctx, rect, pointer);
                if !self.world_context.picking {
                    return true;
                }
                let position = self.context_pointer(ctx, rect, pointer);
                self.world_context.pointer = Some(position);
                ctx.set_cursor_icon(egui::CursorIcon::Cell);
                if primary && !space {
                    self.create_level_at(position, true);
                }
            }
            return true;
        }
        if !busy
            && !covered_by_search
            && response.hovered()
            && ctx.input(|i| {
                i.pointer.secondary_pressed()
                    && !tools::space_secondary_pressed(i)
                    && (i.modifiers.is_none()
                        || cfg!(target_os = "macos")
                            && i.modifiers.ctrl
                            && !i.modifiers.alt
                            && !i.modifiers.shift
                            && !i.modifiers.command)
            })
            && let Some(pointer) = ctx.pointer_hover_pos()
        {
            let position = self.context_pointer(ctx, rect, pointer);
            let hit = self.level_at_world(position);
            if self.world || hit.is_none() {
                if let Some(key) = hit {
                    self.navigate_level(key, false);
                }
                self.world_context.menu = Some(WorldMenu {
                    screen: pointer,
                    position,
                    level: hit,
                });
            }
        }
        if self.world_context.no_room {
            let mut pick = false;
            let mut cancel = false;
            project_dialogs::show_dialog(
                ctx,
                egui::Id::new("world-no-room"),
                "New level",
                440.,
                |ui| {
                    ui.label(locale::tr(
                        ui,
                        "No room for a level here! Do you want to pick another location?",
                    ));
                    ui.horizontal_wrapped(|ui| {
                        pick = project_dialogs::action(ui, "Pick another location", true).clicked();
                        cancel = project_dialogs::action(ui, "Cancel", false).clicked();
                    });
                },
            );
            if pick {
                self.start_level_spot_picker();
            } else if cancel {
                self.cancel_world_context();
            }
            return true;
        }
        let Some(menu) = self.world_context.menu else {
            return false;
        };
        let basic = |label, action| MenuRow {
            action,
            label: locale::tr_ctx(ctx, label).to_uppercase(),
            detail: None,
            enabled: true,
        };
        let mut rows = vec![basic("New level", MenuAction::New)];
        if let Some(key) = menu.level {
            rows.push(basic("Duplicate", MenuAction::Duplicate(key)));
            rows.push(basic("Delete", MenuAction::Delete(key)));
        }
        let world_rows_start = rows.len();
        let heading = self
            .project
            .as_ref()
            .and_then(|p| p.json["worlds"].as_array())
            .filter(|worlds| worlds.len() > 1)
            .map(|worlds| {
                rows.extend(worlds.iter().enumerate().map(|(index, world)| {
                    let name = text(world, "identifier", "World").to_uppercase();
                    let count = world["levels"].as_array().map_or(0, Vec::len);
                    MenuRow {
                        action: menu
                            .level
                            .map_or(MenuAction::World(index), |key| MenuAction::Move(key, index)),
                        label: if menu.level.is_some() {
                            format!("➔ {name}")
                        } else {
                            name
                        },
                        detail: Some(locale::format_ctx(
                            ctx,
                            "{count} level(s)",
                            &[("count", &count.to_string())],
                        )),
                        enabled: Some(index) != self.active_world,
                    }
                }));
                locale::tr_ctx(
                    ctx,
                    if menu.level.is_some() {
                        "Move this level to:"
                    } else {
                        "Go to world:"
                    },
                )
            });
        let title_font = FontId::new(14.667, egui::FontFamily::Name("body-bold".into()));
        let label_font = FontId::proportional(13.333);
        let detail_font = FontId::proportional(12.);
        let screen = ctx.content_rect();
        let width = ctx.fonts_mut(|fonts| {
            let mut measure = |label: &str, font: &FontId| {
                fonts
                    .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
                    .size()
                    .x
            };
            let mut width = heading.map_or(0., |title| measure(title, &title_font));
            for row in &rows {
                width = width.max(measure(&row.label, &label_font));
                if let Some(detail) = &row.detail {
                    width = width.max(measure(detail, &detail_font));
                }
            }
            (width + 40.).min(screen.width() - 32.)
        });
        let natural_height = rows
            .iter()
            .enumerate()
            .map(|(index, row)| row.height(index > 0 && index != world_rows_start))
            .sum::<f32>()
            + if heading.is_some() { 35. } else { 2. };
        let height = natural_height.min((screen.height() - 32.).max(2.));
        let x = if menu.screen.x >= screen.left() + screen.width() * 0.6 {
            menu.screen.x - width
        } else {
            menu.screen.x
        };
        let y = if menu.screen.y >= screen.top() + screen.height() * 0.7 {
            menu.screen.y - 10. - height
        } else {
            menu.screen.y + 10.
        };
        let position = egui::pos2(
            x.clamp(
                screen.left(),
                (screen.right() - width - 2.).max(screen.left()),
            ),
            y.clamp(
                screen.top() + 16.,
                (screen.bottom() - height - 16.).max(screen.top() + 16.),
            ),
        );
        let mut action = None;
        let mut open = true;
        // Source opens on button-down; the corresponding right-button release must not close it.
        let close = if covered_by_search
            || ctx.input(|i| {
                i.pointer.button_down(egui::PointerButton::Secondary)
                    || i.pointer.button_released(egui::PointerButton::Secondary)
            }) {
            egui::PopupCloseBehavior::IgnoreClicks
        } else {
            egui::PopupCloseBehavior::CloseOnClickOutside
        };
        egui::Popup::menu(response)
            .id(egui::Id::new("world-context-menu"))
            .at_position(position)
            .align(egui::RectAlign::BOTTOM_START)
            .align_alternatives(&[])
            .gap(0.)
            .width(width + 2.)
            .frame(
                source_controls::context_menu_frame(&ctx.style()).shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::BLACK,
                }),
            )
            .open_bool(&mut open)
            .close_behavior(close)
            .show(|ui| {
                ui.set_width(width);
                let mut draw_rows = |ui: &mut egui::Ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    for (index, row) in rows.iter().enumerate() {
                        if index == world_rows_start
                            && let Some(heading) = heading
                        {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(width, 33.), Sense::hover());
                            ui.painter().rect_filled(rect, 0., theme::MEDIUM);
                            ui.painter().text(
                                rect.left_center() + egui::vec2(20., 0.),
                                Align2::LEFT_CENTER,
                                heading,
                                title_font.clone(),
                                Color32::WHITE,
                            );
                        }
                        let separator = index > 0 && index != world_rows_start;
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(width, row.height(separator)),
                            if row.enabled && !covered_by_search {
                                Sense::click()
                            } else {
                                Sense::hover()
                            },
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                row.enabled,
                                &row.label,
                            )
                        });
                        let painter = ui.painter_at(rect);
                        let color = if !row.enabled {
                            source_controls::disabled_hatch(ui, rect);
                            theme::LIGHT
                        } else if response.hovered() && !covered_by_search {
                            painter.rect_filled(rect, 0., theme::ACCENT);
                            Color32::BLACK
                        } else {
                            theme::ACCENT
                        };
                        if separator {
                            painter.hline(
                                rect.x_range(),
                                rect.top() + 0.5,
                                Stroke::new(1_f32, theme::MEDIUM),
                            );
                        }
                        let origin = rect.left_top() + egui::vec2(20., 15.5 + f32::from(separator));
                        let paint_text = |text: &str, font: &FontId, origin, color| {
                            let mut label = RichText::new(text).font(font.clone()).color(color);
                            if !row.enabled {
                                label = label.italics();
                            }
                            let galley = egui::WidgetText::from(label).into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                width - 40.,
                                egui::TextStyle::Body,
                            );
                            painter.galley(
                                origin - egui::vec2(0., galley.size().y / 2.),
                                galley,
                                color,
                            );
                        };
                        paint_text(&row.label, &label_font, origin, color);
                        if let Some(detail) = &row.detail {
                            paint_text(
                                detail,
                                &detail_font,
                                origin + egui::vec2(0., 16.5),
                                Color32::from_rgb(115, 127, 157),
                            );
                        }
                        if response.clicked() && row.enabled && !busy {
                            action = Some(row.action);
                        }
                    }
                };
                if height < natural_height {
                    ui.set_height(height - 2.);
                    egui::ScrollArea::vertical()
                        .max_height(height - 2.)
                        .auto_shrink([false, true])
                        .show(ui, draw_rows);
                } else {
                    draw_rows(ui);
                }
            });
        if !open || action.is_some() {
            self.world_context.menu = None;
        }
        match action {
            Some(MenuAction::New) => {
                if !self.create_level_at(menu.position, false) {
                    self.world_context.no_room = true;
                }
            }
            Some(MenuAction::Duplicate(key)) => self.duplicate_context_level(key),
            Some(MenuAction::Delete(key)) => self.delete_context_level(key),
            Some(MenuAction::World(world)) => self.switch_world(Some(world), !self.world),
            Some(MenuAction::Move(key, world)) => self.move_context_level(key, world),
            None => {}
        }
        true
    }
    pub(super) fn world_context_overlay(&mut self, ui: &egui::Ui, rect: Rect) {
        if !self.world_context.picking {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        field_tools::picker_outline(
            &painter,
            rect.shrink(14.),
            Stroke::new(4_f32, theme::ACCENT),
            8.,
            4.,
        );
        if let Some(spot) = self
            .world_context
            .pointer
            .and_then(|p| self.level_insert_spot(p))
        {
            let [x, y, w, h] = spot.rect;
            let density = ui.ctx().pixels_per_point();
            let origin = self.viewport.level_render_canvas_origin(rect, density);
            let size = rect.size().into();
            let a = self.viewport.to_screen([x, y], origin, size);
            let b = self.viewport.to_screen([x + w, y + h], origin, size);
            let preview = Rect::from_min_max(a.into(), b.into());
            let color = if spot.overlaps {
                Color32::from_rgb(255, 68, 0)
            } else {
                Color32::from_rgb(141, 203, 251)
            };
            let scale = self.viewport.zoom * density;
            painter.rect_filled(preview, 0., color.gamma_multiply(0.3));
            painter.rect_stroke(
                preview,
                0.,
                Stroke::new(2. * scale, color.gamma_multiply(0.7)),
                egui::StrokeKind::Middle,
            );
            let radius = preview.size().min_elem() * 0.2;
            let center = preview.center();
            for axis in [egui::vec2(radius, 0.), egui::vec2(0., radius)] {
                painter.line_segment(
                    [center - axis, center + axis],
                    Stroke::new(10. * scale, color),
                );
            }
        }
        if field_tools::picker_banner(
            ui.ctx(),
            "level-spot-picker-instructions",
            locale::tr(ui, "Pick a spot for a new level"),
            174.,
            false,
        ) {
            self.cancel_world_context();
        }
    }
    pub(super) fn cancel_world_context(&mut self) {
        self.world_context = WorldContext::default();
    }
    pub(super) fn start_level_spot_picker(&mut self) {
        if self.project.as_ref().is_none_or(Project::is_backup) {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        if !self.world {
            self.world = true;
            self.fit = true;
        }
        self.world_context = WorldContext {
            picking: true,
            ..Default::default()
        };
    }
    fn level_insert_spot(&self, position: [f32; 2]) -> Option<Spot> {
        let p = self.project.as_ref()?;
        let world = container(p, self.active_world);
        let size = [
            number(world, "defaultLevelWidth", 256.) as i32,
            number(world, "defaultLevelHeight", 256.) as i32,
        ];
        let [w, h] = size.map(|v| v as f32);
        let position = position.map(round);
        let mut spot = Spot {
            rect: [position[0] - w * 0.5, position[1] - h * 0.5, w, h],
            size,
            index: None,
            overlaps: false,
        };
        let placements = self.context_placements();
        let layout = text(world, "worldLayout", "Free");
        if layout.starts_with("Linear") {
            let axis = usize::from(layout == "LinearVertical");
            let mut points: Vec<_> = placements
                .iter()
                .enumerate()
                .map(|(index, (_, b))| (index, b[axis]))
                .collect();
            points.push((
                placements.len(),
                placements.last().map_or(0., |(_, b)| b[axis + 2]),
            ));
            let (index, coord) = points.into_iter().min_by(|a, b| {
                (position[axis] - a.1)
                    .abs()
                    .total_cmp(&(position[axis] - b.1).abs())
            })?;
            spot.index = Some(index);
            if axis == 0 {
                spot.rect = [coord - w * 0.5, -h * 0.1, w, h + (h * 0.2).trunc()];
            } else {
                spot.rect = [-w * 0.1, coord - h * 0.5, w + (w * 0.2).trunc(), h];
            }
        } else {
            spot.overlaps = placements.iter().any(|(_, b)| {
                position[0] >= b[0]
                    && position[0] < b[2]
                    && position[1] >= b[1]
                    && position[1] < b[3]
            });
            if !spot.overlaps {
                for (_, b) in &placements {
                    if overlaps(spot.rect, *b) {
                        let center = [round((b[0] + b[2]) * 0.5), round((b[1] + b[3]) * 0.5)];
                        let bw = b[2] - b[0];
                        let bh = b[3] - b[1];
                        let slope = (spot.rect[1] + h * 0.5 - center[1])
                            / (spot.rect[0] + w * 0.5 - center[0]);
                        if slope * bw * 0.5 >= -bh * 0.5 && slope * bw * 0.5 <= bh * 0.5 {
                            spot.rect[0] = if spot.rect[0] < center[0] {
                                b[0] - w
                            } else {
                                b[2]
                            };
                        } else {
                            spot.rect[1] = if spot.rect[1] < center[1] {
                                b[1] - h
                            } else {
                                b[3]
                            };
                        }
                    }
                }
                spot.overlaps = placements.iter().any(|(_, b)| overlaps(spot.rect, *b));
            }
            if layout == "GridVania" || self.grid {
                let grid = if layout == "GridVania" {
                    [
                        number(world, "worldGridWidth", 256.),
                        number(world, "worldGridHeight", 256.),
                    ]
                } else {
                    [number(&p.json, "defaultGridSize", 16.); 2]
                };
                for (axis, grid) in grid.into_iter().enumerate() {
                    spot.rect[axis] = round(spot.rect[axis] / grid.max(1.)) * grid.max(1.);
                }
            }
        }
        Some(spot)
    }
    fn create_level_at(&mut self, position: [f32; 2], reject_overlap: bool) -> bool {
        let Some(spot) = self.level_insert_spot(position) else {
            return false;
        };
        if reject_overlap && spot.overlaps {
            return false;
        }
        let world = self.active_world;
        let depth = self.world_depth as i32;
        let previous_origin = self.context_level_origin();
        let key = self.edit("Create level", |p| {
            let name = level_name(p, "Level1");
            let mut key = p.add_level(world, &name, spot.size[0], spot.size[1])?;
            if let Some(index) = spot.index {
                key = p.move_level_to_world(key, world, index)?;
                normalize_linear(p, world);
            } else {
                p.move_level(
                    key,
                    round(spot.rect[0]) as i32,
                    round(spot.rect[1]) as i32,
                    depth,
                )?;
            }
            p.level_mut(key).unwrap()["useAutoIdentifier"] = json!(true);
            p.refresh_derived_data()?;
            Ok(key)
        });
        if let Some(key) = key {
            self.scroll_to_context_level(key, previous_origin);
            true
        } else {
            false
        }
    }
    fn context_level_origin(&self) -> [f32; 2] {
        if self.world {
            [0.; 2]
        } else {
            self.project
                .as_ref()
                .zip(self.selected)
                .map_or([0.; 2], |(project, key)| {
                    navigation::level_world_origin(project, key)
                })
        }
    }
    pub(super) fn scroll_to_context_level(&mut self, key: LevelKey, previous_origin: [f32; 2]) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(
            &ctx,
            ui_commit::DeferredUiAction::ScrollToLevel {
                key,
                previous_origin,
            },
        ) {
            return;
        }
        self.navigate_level(key, false);
        if let Some((_, b)) = self
            .context_placements()
            .into_iter()
            .find(|(k, _)| *k == key)
        {
            let origin = if self.world { [0.; 2] } else { [b[0], b[1]] };
            // The source keeps the camera in world coordinates when the level changes.
            for axis in 0..2 {
                self.viewport.center[axis] += previous_origin[axis] - origin[axis];
            }
            self.camera_motion.scroll_to(std::array::from_fn(|axis| {
                ((b[axis] + b[axis + 2]) * 0.5).round() - origin[axis]
            }));
        }
    }
    pub(super) fn duplicate_context_level(&mut self, key: LevelKey) {
        if let Some(copy) = self.edit("Duplicate level", |p| {
            let source = p.level(key).unwrap();
            let auto = source["useAutoIdentifier"].clone();
            let name = level_name(p, text(source, "identifier", "Level1"));
            let copy = p.duplicate_level(key, &name)?;
            if !text(container(p, key.world), "worldLayout", "Free").starts_with("Linear") {
                let level = p.level(copy).unwrap();
                let delta = number(&p.json, "defaultGridSize", 16.) as i32 * 4;
                p.move_level(
                    copy,
                    number(level, "worldX", 0.) as i32 + delta,
                    number(level, "worldY", 0.) as i32 + delta,
                    number(level, "worldDepth", 0.) as i32,
                )?;
            } else {
                normalize_linear(p, key.world);
            }
            p.level_mut(copy).unwrap()["useAutoIdentifier"] = auto;
            p.refresh_derived_data()?;
            Ok(copy)
        }) {
            self.navigate_level(copy, false);
        }
    }
    pub(super) fn delete_context_level(&mut self, key: LevelKey) {
        let placements = self.context_placements();
        if placements.len() <= 1 {
            self.status = "You can't delete the last level.".into();
            return;
        }
        let Some((_, b)) = placements.iter().find(|(k, _)| *k == key) else {
            return;
        };
        let closest = placements
            .iter()
            .filter(|(k, _)| *k != key)
            .min_by(|(_, a), (_, c)| {
                let distance = |a: &[f32; 4]| {
                    ((a[0] - b[2]).max(0.) + (b[0] - a[2]).max(0.))
                        .max((a[1] - b[3]).max(0.) + (b[1] - a[3]).max(0.))
                };
                distance(a).total_cmp(&distance(c))
            })
            .map(|(k, _)| *k)
            .unwrap();
        let next = LevelKey {
            level: closest.level - usize::from(closest.level > key.level),
            ..closest
        };
        let previous_origin = self.context_level_origin();
        if self
            .edit_recoverable("Delete level", |p| p.remove_level(key))
            .is_some()
        {
            self.scroll_to_context_level(next, previous_origin);
        }
    }
    fn move_context_level(&mut self, key: LevelKey, world: usize) {
        if let Some(target) = self.edit("Move level to world", |p| {
            let count = p.level_keys_in_world(Some(world)).len();
            let origin = canvas::placements(p, key)
                .into_iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, bounds)| [bounds[0] as i32, bounds[1] as i32]);
            let target = p.move_level_to_world(key, Some(world), count)?;
            if !text(container(p, Some(world)), "worldLayout", "Free").starts_with("Linear")
                && let Some(origin) = origin
            {
                // Linear JSON uses -1; World.moveToWorld keeps the source runtime position.
                let level = p.level_mut(target).unwrap();
                level["worldX"] = json!(origin[0]);
                level["worldY"] = json!(origin[1]);
            }
            normalize_linear(p, key.world);
            normalize_linear(p, Some(world));
            p.refresh_derived_data()?;
            Ok(target)
        }) {
            self.navigate_level(target, true);
            self.show_panel(None);
            self.fit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const K: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    fn editor() -> EditorApp {
        let mut a = EditorApp::empty();
        a.set_project(Project::new());
        a.world = true;
        a.fit = false;
        a
    }
    #[test]
    fn free_and_gridvania_spots_match_source_deinterlace_and_grid_rounding() {
        let mut a = editor();
        let spot = a.level_insert_spot([300., 128.]).unwrap();
        assert_eq!(spot.rect, [256., 0., 256., 256.]);
        assert!(!spot.overlaps);
        assert!(a.level_insert_spot([128., 128.]).unwrap().overlaps);
        let p = a.project.as_mut().unwrap();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(128);
        p.json["worldGridHeight"] = json!(64);
        p.json["defaultLevelWidth"] = json!(128);
        p.json["defaultLevelHeight"] = json!(64);
        assert_eq!(
            a.level_insert_spot([330., 300.]).unwrap().rect,
            [256., 256., 128., 64.]
        );
        assert_eq!(
            a.level_insert_spot([-128., -64.]).unwrap().rect,
            [-256., -128., 128., 64.]
        );
        assert!(a.create_level_at([-128., -64.], true));
        let created = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap();
        assert_eq!(created["worldX"], -256);
        assert_eq!(created["worldY"], -128);
    }
    #[test]
    fn linear_picker_inserts_at_nearest_boundary_and_reorganizes_both_axes() {
        for (layout, point, axis) in [
            ("LinearHorizontal", [290., 0.], 0),
            ("LinearVertical", [0., 290.], 1),
        ] {
            let mut a = editor();
            let p = a.project.as_mut().unwrap();
            p.add_level(None, "Second", 128, 128).unwrap();
            p.json["worldLayout"] = json!(layout);
            p.json["defaultLevelWidth"] = json!(96);
            p.json["defaultLevelHeight"] = json!(96);
            assert_eq!(a.level_insert_spot(point).unwrap().index, Some(1));
            assert!(a.create_level_at(point, true));
            let p = a.project.as_ref().unwrap();
            let key = a.selected.unwrap();
            assert_eq!(key.level, 1);
            assert_eq!(p.level(key).unwrap()["pxWid"], 96);
            assert_eq!(p.level(key).unwrap()["useAutoIdentifier"], true);
            let coord = if axis == 0 { "worldX" } else { "worldY" };
            assert_eq!(p.level(key).unwrap()[coord], -1);
            let placements = canvas::placements(p, key);
            assert_eq!(placements[1].1[axis], 304.);
            assert_eq!(placements[2].1[axis], 448.);
            assert!(a.world);
            assert!(a.history.can_undo());
        }
    }
    #[test]
    fn source_direct_new_allows_overlap_but_picker_rejects_it() {
        let mut a = editor();
        assert!(!a.create_level_at([128., 128.], true));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.create_level_at([128., 128.], false));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    #[test]
    fn context_level_changes_scroll_from_the_previous_world_camera_position() {
        let canvas = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        for world in [false, true] {
            let mut app = editor();
            app.world = world;
            app.grid = false;
            app.viewport.zoom = 1.5;
            let project = app.project.as_mut().unwrap();
            project.move_level(K, 512, -256, 0).unwrap();
            app.viewport.center = if world { [587., -166.] } else { [75., 90.] };

            assert!(app.create_level_at([900., 360.], true));
            let created = app.selected.unwrap();
            let start = if world { [587., -166.] } else { [-185., -398.] };
            let end = if world { [900., 360.] } else { [128., 128.] };
            assert_eq!(app.viewport.center, start);
            assert!(
                app.camera_motion
                    .tick(&mut app.viewport, canvas, [0.; 2], 1., 1, 1. / 60.)
            );
            let moved = std::array::from_fn(|i| start[i] + (end[i] - start[i]) * 0.15);
            assert_eq!(app.viewport.center, moved);
            assert_eq!(app.viewport.zoom, 1.5);

            app.delete_context_level(created);
            assert_eq!(app.selected, Some(K));
            let start = if world {
                moved
            } else {
                [moved[0] + 260., moved[1] + 488.]
            };
            let end = if world { [640., -128.] } else { [128., 128.] };
            assert_eq!(app.viewport.center, start);
            assert!(
                app.camera_motion
                    .tick(&mut app.viewport, canvas, [0.; 2], 1., 2, 1. / 60.)
            );
            assert_eq!(
                app.viewport.center,
                std::array::from_fn(|i| start[i] + (end[i] - start[i]) * 0.15)
            );
            assert_eq!(app.viewport.zoom, 1.5);
        }
    }

    #[test]
    fn duplicate_offset_and_delete_closest_last_chance_match_source() {
        let mut a = editor();
        a.duplicate_context_level(K);
        let p = a.project.as_ref().unwrap();
        assert_eq!(p.level_keys().len(), 2);
        assert_eq!(p.level(a.selected.unwrap()).unwrap()["worldX"], 64);
        a.delete_context_level(a.selected.unwrap());
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
        a.delete_context_level(K);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        a.restore_last_chance();
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    fn frame(
        a: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) -> egui::FullOutput {
        a.repaint = Some(ctx.clone());
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                events,
                modifiers,
                ..Default::default()
            },
            |ctx| {
                a.shortcuts(ctx);
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(800., 600.), Sense::click_and_drag());
                        a.canvas_input(ctx, rect, &response);
                        a.update_world_hover(ctx, rect, response.hovered());
                        a.world_context_overlay(ui, rect);
                    });
                a.palette_dialog(ctx);
            },
        )
    }
    fn button(
        pos: egui::Pos2,
        button: egui::PointerButton,
        pressed: bool,
        modifiers: egui::Modifiers,
    ) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        }
    }
    fn label_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, label))
            .unwrap_or_else(|| panic!("missing menu label {label}"))
    }
    fn right_click(a: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) -> egui::FullOutput {
        frame(
            a,
            ctx,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Secondary,
                true,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Secondary,
                false,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        )
    }
    fn click(a: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        frame(
            a,
            ctx,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Primary,
                true,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
        frame(
            a,
            ctx,
            vec![button(
                pos,
                egui::PointerButton::Primary,
                false,
                egui::Modifiers::NONE,
            )],
            egui::Modifiers::NONE,
        );
    }
    #[test]
    fn preview_154_fractional_world_edges_agree_for_context_click_and_marquee() {
        let mut app = editor();
        app.viewport.zoom = 1.5;
        app.viewport.center = [253.6, 128.];
        let project = app.project.as_mut().unwrap();
        let neighbor = project.add_level(None, "Neighbor", 128, 128).unwrap();
        project.move_level(neighbor, 320, 64, 0).unwrap();
        let ctx = egui::Context::default();
        let outside = egui::pos2(499., 300.);
        let inside = egui::pos2(500., 300.);

        // At 1.5x the rendered camera origin is x=20, before snapping it is x=19.6.
        let output = right_click(&mut app, &ctx, outside);
        label_center(&output, "NEW LEVEL");
        assert!(app.world_context.menu.unwrap().level.is_none());
        assert_eq!(app.selected, Some(K));
        app.cancel_world_context();

        let primary = egui::PointerButton::Primary;
        let none = egui::Modifiers::NONE;
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(outside)],
            none,
        );
        frame(
            &mut app,
            &ctx,
            vec![button(outside, primary, true, none)],
            none,
        );
        assert!(matches!(
            app.gesture,
            Some(tools::Gesture::WorldRectangle { .. })
        ));
        let end = egui::pos2(501., 303.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], none);
        assert!(matches!(
            app.gesture,
            Some(tools::Gesture::WorldRectangle {
                corners: Some([[319., 128.], [321., 130.]]),
                ..
            })
        ));
        frame(
            &mut app,
            &ctx,
            vec![button(end, primary, false, none)],
            none,
        );
        app.world_selection.clear();
        click(&mut app, &ctx, inside);
        assert_eq!(app.selected, Some(neighbor));
        assert!(app.world);

        // With x=20.4 before snapping, x=501 hits the level only through the rendered origin.
        app.navigate_level(K, false);
        app.viewport.center[0] = 253.06667;
        app.project
            .as_mut()
            .unwrap()
            .move_level(neighbor, 321, 64, 0)
            .unwrap();
        click(&mut app, &ctx, egui::pos2(501., 300.));
        assert_eq!(app.selected, Some(neighbor));
    }

    #[test]
    fn actual_world_right_click_menu_duplicates_and_deletes_with_last_chance() {
        let mut a = editor();
        a.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        let pos = egui::pos2(400., 300.);
        let output = right_click(&mut a, &ctx, pos);
        let duplicate = label_center(&output, "DUPLICATE");
        click(&mut a, &ctx, duplicate);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
        let output = right_click(&mut a, &ctx, pos);
        let delete = label_center(&output, "DELETE");
        click(&mut a, &ctx, delete);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
        let output = right_click(&mut a, &ctx, pos);
        let delete = label_center(&output, "DELETE");
        click(&mut a, &ctx, delete);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
        assert!(a.last_chance.is_some());
    }
    #[test]
    fn chinese_world_menu_keeps_level_actions_available() {
        let mut a = editor();
        a.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        let output = right_click(&mut a, &ctx, egui::pos2(400., 300.));
        label_center(&output, "新建关卡");
        label_center(&output, "删除");
        let duplicate = label_center(&output, "复制");
        click(&mut a, &ctx, duplicate);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    #[test]
    fn level_spot_picker_draws_source_preview_and_has_priority_over_world_hover() {
        let mut app = editor();
        app.grid = false;
        app.viewport.center = [128., 128.];
        app.viewport.zoom = 0.75;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.start_level_spot_picker();
        let _ = frame(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        for (pos, color) in [
            (egui::pos2(400., 300.), Color32::from_rgb(255, 68, 0)),
            (egui::pos2(740., 150.), Color32::from_rgb(141, 203, 251)),
        ] {
            let output = frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(pos)],
                egui::Modifiers::NONE,
            );
            assert!(app.world_hover.is_none());
            assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Cell);
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(r) if
                    r.fill == color.gamma_multiply(0.3))
            }));
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(r) if
                    r.stroke == Stroke::new(1.5_f32, color.gamma_multiply(0.7)))
            }));
            let cross = output.shapes.iter().filter(|shape| {
                matches!(&shape.shape, egui::Shape::LineSegment { points, stroke } if
                    *stroke == Stroke::new(7.5_f32, color) &&
                    ((points[0].distance(points[1])) - 76.8).abs() < 0.01)
            });
            assert_eq!(cross.count(), 2);
            let text = label_center(&output, "Pick a spot for a new level");
            assert!(text.y > 600. * 0.85);
            let instructions = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == "Pick a spot for a new level" =>
                    {
                        Some(&text.galley)
                    }
                    _ => None,
                })
                .expect("picker instruction banner");
            assert_eq!(instructions.job.wrap.max_width, 174.);
            assert_eq!(instructions.job.halign, egui::Align::Center);
            assert!(!instructions.elided);
            assert!(instructions.size().x <= 174.);
        }
    }

    #[test]
    fn actual_picker_rejects_occupied_click_then_creates_at_preview_in_all_layouts() {
        for layout in ["Free", "GridVania", "LinearHorizontal", "LinearVertical"] {
            let mut a = editor();
            a.project.as_mut().unwrap().json["worldLayout"] = json!(layout);
            a.viewport.center = [128., 128.];
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            a.start_level_spot_picker();
            if matches!(layout, "Free" | "GridVania") {
                click(&mut a, &ctx, egui::pos2(400., 300.));
                assert!(a.world_context.picking);
                assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 1);
            }
            let pos = egui::pos2(720., 560.);
            let world = a.context_pointer(
                &ctx,
                Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.)),
                pos,
            );
            let spot = a.level_insert_spot(world).unwrap();
            assert!(!spot.overlaps);
            click(&mut a, &ctx, pos);
            assert!(!a.world_context.picking, "{layout}");
            assert_eq!(
                a.project.as_ref().unwrap().level_keys().len(),
                2,
                "{layout}"
            );
            assert!(a.dirty);
            let level = a
                .project
                .as_ref()
                .unwrap()
                .level(a.selected.unwrap())
                .unwrap();
            assert_eq!(level["useAutoIdentifier"], true);
        }
    }
    #[test]
    fn context_menu_requires_unmodified_world_or_empty_space_and_backup_stays_readonly() {
        let mut a = editor();
        a.world = false;
        a.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        let inside = egui::pos2(400., 300.);
        right_click(&mut a, &ctx, inside);
        assert!(a.world_context.menu.is_none());
        let outside = egui::pos2(760., 560.);
        let output = right_click(&mut a, &ctx, outside);
        label_center(&output, "NEW LEVEL");
        a.cancel_world_context();
        let modifiers = egui::Modifiers {
            alt: true,
            ..Default::default()
        };
        frame(
            &mut a,
            &ctx,
            vec![button(
                outside,
                egui::PointerButton::Secondary,
                true,
                modifiers,
            )],
            modifiers,
        );
        assert!(a.world_context.menu.is_none());
        a.project.as_mut().unwrap().path = Some(std::path::PathBuf::from(
            "/private/tmp/backup_2026-11-16_12-00-00/world.ldtk",
        ));
        assert!(a.project.as_ref().unwrap().is_backup());
        a.world = true;
        right_click(&mut a, &ctx, inside);
        assert!(a.world_context.menu.is_none());
        a.start_level_spot_picker();
        assert!(!a.world_context.picking);
    }
    #[test]
    fn world_menu_moves_level_with_iid_intact_and_goes_to_empty_world() {
        let mut a = editor();
        let p = a.project.as_mut().unwrap();
        p.add_world("Other").unwrap();
        a.active_world = Some(0);
        a.selected = Some(LevelKey {
            world: Some(0),
            level: 0,
        });
        a.viewport.center = [128., 128.];
        let iid = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap()["iid"]
            .clone();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = right_click(&mut a, &ctx, egui::pos2(400., 300.));
        let target = label_center(&output, "➔ OTHER");
        click(&mut a, &ctx, target);
        assert_eq!(a.active_world, Some(1));
        assert!(!a.world);
        assert_eq!(
            a.project
                .as_ref()
                .unwrap()
                .level(a.selected.unwrap())
                .unwrap()["iid"],
            iid
        );
        a.fit = false;
        a.viewport.center = [128., 128.];
        let output = right_click(&mut a, &ctx, egui::pos2(760., 560.));
        let target = label_center(&output, "WORLD_0");
        click(&mut a, &ctx, target);
        assert_eq!(a.active_world, Some(0));
        assert!(a.selected.is_none());
        assert!(a.world);
        a.start_level_spot_picker();
        assert!(a.create_level_at([0., 0.], true));
        assert_eq!(a.selected.unwrap().world, Some(0));
    }
    #[test]
    fn world_context_menu_uses_source_rows_and_escape_keeps_the_world_view() {
        let mut app = editor();
        app.project.as_mut().unwrap().add_world("Away").unwrap();
        app.navigate_world(Some(0));
        app.viewport.center = [128., 128.];
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let anchor = egui::pos2(740., 550.);
        let output = right_click(&mut app, &ctx, anchor);
        let create = label_center(&output, "NEW LEVEL");
        let title = label_center(&output, "Go to world:");
        let destination = label_center(&output, "AWAY");
        assert!(create.y < title.y && title.y < destination.y && destination.y < anchor.y - 10.);
        label_center(&output, "0 level(s)");
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
        assert!(app.world_context.menu.is_none());
        assert!(app.world);
        assert_eq!(app.panel, Some(Panel::World));
    }

    #[test]
    fn save_shortcut_writes_the_project_while_the_world_context_menu_is_open() {
        for world in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("World.ldtk");
            let mut project = Project::new();
            project.json["backupOnSave"] = json!(false);
            project.save(&path).unwrap();
            let mut app = EditorApp::empty();
            app.set_project(project);
            app.world = world;
            app.fit = false;
            app.viewport.center = [128.; 2];
            app.project.as_mut().unwrap().json["bgColor"] = json!("#334455");
            app.dirty = true;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            right_click(&mut app, &ctx, egui::pos2(740., 550.));
            assert!(app.world_context.menu_open());

            let modifiers = egui::Modifiers::COMMAND;
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key: Key::S,
                    physical_key: Some(Key::S),
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
            );
            assert!(
                app.world_context.menu_open(),
                "saving keeps the menu visible"
            );
            app.saving
                .take()
                .expect("save with the context menu open")
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .unwrap();
            assert_eq!(Project::load(&path).unwrap().json["bgColor"], "#334455");
            assert_eq!(app.world, world);
        }
    }

    #[test]
    fn canvas_display_shortcuts_work_without_dismissing_the_context_menu() {
        let mut app = editor();
        app.world = false;
        app.viewport.center = [128.; 2];
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        right_click(&mut app, &ctx, egui::pos2(740., 550.));
        assert!(app.world_context.menu_open());

        let state = |app: &EditorApp| {
            [
                app.grid,
                app.render_auto,
                app.settings.bool("emptySpaceSelection"),
                app.tile_stacking,
                app.single_layer,
                app.details,
            ]
        };
        let before = state(&app);
        let details = if cfg!(target_os = "macos") {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::CTRL | egui::Modifiers::COMMAND
        };
        for (index, (key, modifiers)) in [
            (Key::G, egui::Modifiers::NONE),
            (Key::R, egui::Modifiers::SHIFT),
            (Key::E, egui::Modifiers::SHIFT),
            (Key::T, egui::Modifiers::SHIFT),
            (Key::A, egui::Modifiers::SHIFT),
            (Key::H, details),
        ]
        .into_iter()
        .enumerate()
        {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
            );
            assert_ne!(state(&app)[index], before[index], "{key:?}");
            assert!(app.world_context.menu_open());
        }
        assert!(!app.dirty);
    }

    #[test]
    fn editor_panel_shortcuts_replace_the_world_context_menu() {
        for (key, modifiers, panel) in [
            (Key::H, egui::Modifiers::NONE, Panel::Help),
            (Key::P, egui::Modifiers::NONE, Panel::Project),
            (Key::C, egui::Modifiers::NONE, Panel::Level),
            (Key::L, egui::Modifiers::NONE, Panel::Layers),
            (Key::E, egui::Modifiers::NONE, Panel::Entities),
            (Key::U, egui::Modifiers::NONE, Panel::Enums),
            (Key::T, egui::Modifiers::NONE, Panel::Tilesets),
            (Key::W, egui::Modifiers::SHIFT, Panel::World),
        ] {
            let mut app = editor();
            app.world = false;
            app.viewport.center = [128.; 2];
            let project = app.project.as_ref().unwrap().json.clone();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            right_click(&mut app, &ctx, egui::pos2(740., 550.));
            assert!(app.world_context.menu_open());
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
            );
            assert_eq!(app.panel, Some(panel), "{key:?}");
            assert!(!app.world_context.menu_open());
            assert_eq!(app.world, panel == Panel::World);
            assert_eq!(app.project.as_ref().unwrap().json, project);
        }
    }

    #[test]
    fn exit_shortcuts_dismiss_the_context_menu_before_leaving_or_prompting() {
        for key in [Key::W, Key::Q] {
            for (world, dirty) in [(false, false), (false, true), (true, false), (true, true)] {
                let mut app = editor();
                app.world = false;
                if world {
                    app.show_panel(Some(Panel::World));
                }
                app.fit = false;
                app.viewport.center = [128.; 2];
                app.dirty = dirty;
                let project = app.project.as_ref().unwrap().json.clone();
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                right_click(&mut app, &ctx, egui::pos2(740., 550.));
                assert!(app.world_context.menu_open());

                let modifiers = egui::Modifiers::COMMAND;
                let output = frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    modifiers,
                );
                assert!(!app.world_context.menu_open());
                assert!(!app.world && app.panel.is_none());
                let quit = key == Key::Q;
                let window_closed = output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::Close));
                assert_eq!(window_closed, quit && !dirty);
                assert_eq!(app.allow_close, quit && !dirty);
                if dirty {
                    if quit {
                        assert!(matches!(app.pending, Some(Action::Quit)));
                    } else {
                        assert!(matches!(app.pending, Some(Action::Close)));
                    }
                    assert_eq!(app.project.as_ref().unwrap().json, project);
                    assert!(app.dirty);
                } else {
                    assert_eq!(app.project.is_none(), !quit);
                    assert!(app.pending.is_none());
                }
            }
        }
    }

    #[test]
    fn search_opens_above_the_context_menu_and_escape_closes_one_at_a_time() {
        let mut app = editor();
        app.world = false;
        app.viewport.center = [128.; 2];
        app.project
            .as_mut()
            .unwrap()
            .add_entity_def("Actor", 16, 16)
            .unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let anchor = egui::pos2(740., 550.);
        right_click(&mut app, &ctx, anchor);
        assert!(app.world_context.menu_open());
        let keys = |key, modifiers| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                })
                .collect()
        };
        let command = egui::Modifiers::COMMAND;
        frame(&mut app, &ctx, keys(Key::P, command), command);
        assert!(app.command_palette);
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("Actor".to_owned())],
            egui::Modifiers::NONE,
        );
        assert_eq!(app.search, "Actor");
        label_center(&output, "Actor");
        let menu_row = label_center(&output, "NEW LEVEL");
        assert_eq!(
            ctx.layer_id_at(menu_row).unwrap().id,
            egui::Id::new("command-palette")
        );

        for menu_remains in [true, false] {
            frame(
                &mut app,
                &ctx,
                keys(Key::Escape, egui::Modifiers::NONE),
                egui::Modifiers::NONE,
            );
            assert!(!app.command_palette);
            assert_eq!(app.world_context.menu_open(), menu_remains);
        }
    }
    #[test]
    fn actual_empty_space_new_level_and_picker_cancel_preserve_source_mode() {
        let mut a = editor();
        a.world = false;
        a.viewport.center = [128., 128.];
        a.project
            .as_mut()
            .unwrap()
            .move_level(K, 512, 256, 0)
            .unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let point = egui::pos2(760., 560.);
        let position = a.context_pointer(
            &ctx,
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.)),
            point,
        );
        let expected = a.level_insert_spot(position).unwrap().rect;
        let output = right_click(&mut a, &ctx, point);
        click(&mut a, &ctx, label_center(&output, "NEW LEVEL"));
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
        assert!(!a.world);
        let level = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap();
        assert_eq!(level["worldX"], expected[0] as i32);
        assert_eq!(level["worldY"], expected[1] as i32);
        a.start_level_spot_picker();
        assert!(a.world);
        let output = frame(&mut a, &ctx, vec![], egui::Modifiers::NONE);
        // Area does one sizing pass before its interactive position stabilizes.
        let _ = output;
        let output = frame(&mut a, &ctx, vec![], egui::Modifiers::NONE);
        click(&mut a, &ctx, label_center(&output, "CANCEL"));
        assert!(!a.world_context.picking);
        assert!(a.world);
        assert_eq!(a.project.as_ref().unwrap().level_keys().len(), 2);
    }
    #[test]
    fn moving_from_linear_to_free_preserves_effective_world_position() {
        let mut a = editor();
        let p = a.project.as_mut().unwrap();
        p.add_level(None, "Second", 128, 128).unwrap();
        p.set_world_layout(None, "LinearHorizontal", 256, 256)
            .unwrap();
        p.add_world("Other").unwrap();
        a.active_world = Some(0);
        a.selected = Some(LevelKey {
            world: Some(0),
            level: 1,
        });
        a.move_context_level(a.selected.unwrap(), 1);
        let level = a
            .project
            .as_ref()
            .unwrap()
            .level(a.selected.unwrap())
            .unwrap();
        assert_eq!(level["worldX"], 304);
        assert_eq!(level["worldY"], 0);
    }
}
