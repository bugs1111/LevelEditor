use super::*;
use serde_json::json;

fn point_coordinates(value: &Value) -> Option<String> {
    Some(format!(
        "{},{}",
        value["cx"].as_i64()?,
        value["cy"].as_i64()?
    ))
}

pub(super) fn point_array_is_compact(definition: &Value) -> bool {
    matches!(
        text(definition, "editorDisplayMode", ""),
        "Points" | "PointPath" | "PointStar"
    )
}

pub(super) fn array_field_frame(empty: bool) -> egui::Frame {
    let padding = if empty { 2 } else { 8 };
    egui::Frame::new().inner_margin(egui::Margin {
        left: padding + 5,
        right: 1,
        top: if empty { 4 } else { padding },
        bottom: if empty { 3 } else { padding },
    })
}

pub(super) fn array_field_border(ui: &egui::Ui, rect: Rect) {
    ui.painter().vline(
        rect.left() + 4.,
        (rect.top() + 4.)..=(rect.bottom() - 4.),
        Stroke::new(1_f32, Color32::from_white_alpha(180)),
    );
}

pub(super) fn array_field_add(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 1,
            ..egui::Margin::ZERO
        })
        .show(ui, |ui| {
            let size = 22.667;
            ui.spacing_mut().button_padding.y = 0.;
            ui.spacing_mut().interact_size.y = size;
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = Color32::from_rgb(49, 55, 66);
            for style in [&mut widgets.hovered, &mut widgets.active] {
                style.weak_bg_fill = theme::LIGHT;
                style.expansion = 0.;
            }
            panels::source_controls::add_sized_button(
                ui,
                [ui.available_width(), size],
                egui::Button::image(
                    egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                        .fit_to_exact_size(Vec2::splat(size)),
                ),
            )
        })
        .inner
        .on_hover_text(locale::tr(ui, tooltip))
}

pub(super) fn array_field_remove(ui: &mut egui::Ui) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(1., 0.);
        ui.spacing_mut().interact_size = egui::vec2(22., 28.);
        let widgets = &mut ui.visuals_mut().widgets;
        for style in [&mut widgets.hovered, &mut widgets.active] {
            style.weak_bg_fill = theme::ACCENT;
            style.bg_stroke = Stroke::NONE;
            style.expansion = 0.;
            style.corner_radius = 3.into();
        }
        ui.add_sized(
            [22., 28.],
            egui::Button::new(
                egui::Image::new(egui::include_image!("../../assets/icons/delete.svg"))
                    .fit_to_exact_size(Vec2::splat(20.)),
            )
            .frame(true)
            .frame_when_inactive(false),
        )
    })
    .inner
    .on_hover_text(locale::tr(ui, "Remove array value"))
}

fn reference_alert_button(
    ui: &mut egui::Ui,
    size: Vec2,
    button: egui::Button<'_>,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = size.y;
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.weak_bg_fill = Color32::from_rgb(121, 17, 5);
        for style in [&mut widgets.hovered, &mut widgets.active] {
            style.weak_bg_fill = Color32::from_rgb(157, 18, 2);
            style.expansion = 0.;
        }
        panels::source_controls::add_sized_button(ui, size, button)
    })
    .inner
}

#[derive(Clone, Default)]
struct ArrayFieldDrag {
    from: usize,
    to: usize,
    grab: Vec2,
    pass: u64,
    rows: Vec<ArrayRowSlide>,
}

#[derive(Clone, Copy, Default)]
struct ArrayRowSlide {
    from_top: f32,
    top: f32,
    initial_offset: f32,
    started: f64,
    duration: f64,
}

impl ArrayRowSlide {
    fn place(self, top: f32, now: f64) -> Self {
        if top == self.top {
            return self;
        }
        let current_offset = self.offset(now);
        let initial_offset = self.top - top + current_offset;
        let duration = if current_offset != 0. && self.from_top == top {
            // SortableJS shortens a return to the previous slot by the distance left.
            0.1 * f64::from((initial_offset / (self.top - top)).abs())
        } else {
            0.1
        };
        Self {
            from_top: self.top,
            top,
            initial_offset,
            started: now,
            duration,
        }
    }

    fn offset(self, now: f64) -> f32 {
        if self.duration == 0. || now >= self.started + self.duration {
            return 0.;
        }
        if now <= self.started {
            return self.initial_offset;
        }
        let progress = ((now - self.started) / self.duration) as f32;
        self.initial_offset * panels::source_controls::css_ease_remaining(progress)
    }
}

pub(super) fn sortable_array_field_rows(
    ui: &mut egui::Ui,
    len: usize,
    mut draw: impl FnMut(&mut egui::Ui, usize),
) -> Option<ldtk_core::FieldArrayEdit> {
    let id = ui.id().with("array-sort");
    let pass = ui.ctx().cumulative_pass_nr();
    let mut drag = ui
        .data_mut(|data| data.remove_temp::<ArrayFieldDrag>(id))
        .filter(|drag| drag.pass + 1 >= pass && drag.from < len && drag.to < len);
    let escaped = drag.is_some()
        && egui::DragAndDrop::payload::<(egui::Id, usize)>(ui.ctx())
            .is_none_or(|payload| payload.0 != id);
    let (pointer, press, down, now) = ui.input(|input| {
        let pointer = &input.pointer;
        (
            pointer.interact_pos(),
            pointer.press_origin(),
            pointer.primary_down(),
            input.time,
        )
    });
    let preview = down && !escaped;
    let list_top = ui.next_widget_position().y;
    let mut rows = vec![ArrayRowSlide::default(); if preview { len } else { 0 }];
    let mut animating = false;
    let mut order = (0..len).collect::<Vec<_>>();
    if let Some(drag) = &drag {
        order.remove(drag.from);
        order.insert(drag.to, drag.from);
    }
    let mut hovered = None;
    for (slot, index) in order.into_iter().enumerate() {
        ui.push_id(index, |ui| {
            let active = drag
                .as_ref()
                .filter(|drag| preview && drag.from == index)
                .map(|drag| drag.grab);
            let first_paint =
                (preview && drag.is_some()).then(|| ui.painter().add(egui::Shape::Noop));
            let row = ui.horizontal(|ui| {
                if active.is_some() {
                    ui.multiply_opacity(0.7);
                }
                // The source reserves 28 pixels before the field editor.
                ui.spacing_mut().item_spacing.x = 2.;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(26., 33.), Sense::hover());
                egui::Image::new(egui::include_image!("../../assets/icons/dragAlpha.svg"))
                    .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.)));
                let handle = ui
                    .interact(
                        rect,
                        ui.make_persistent_id("array-handle"),
                        Sense::click_and_drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab);
                handle.dnd_set_drag_payload((id, index));
                draw(ui, index);
                handle
            });
            let rect = row.response.rect;
            if pointer.is_some_and(|pos| rect.intersect(ui.clip_rect()).contains(pos)) {
                hovered = Some(slot);
            }
            if row.inner.drag_started() {
                drag = Some(ArrayFieldDrag {
                    from: index,
                    to: slot,
                    grab: press.unwrap_or(row.inner.rect.center()) - rect.min,
                    pass,
                    rows: Vec::new(),
                });
                ui.ctx().request_repaint();
            }
            if let (Some(grab), Some(background), Some(pointer)) = (active, first_paint, pointer) {
                paint_array_field_drag(ui, id, rect, background, pointer - grab - rect.min);
            }
            if let Some(row) = rows.get_mut(index) {
                let top = rect.top() - list_top;
                *row = drag
                    .as_ref()
                    .and_then(|drag| drag.rows.get(index))
                    .map_or_else(
                        || ArrayRowSlide {
                            top,
                            ..Default::default()
                        },
                        |before| before.place(top, now),
                    );
                let offset = row.offset(now);
                if offset != 0.
                    && let Some(first) = first_paint
                {
                    slide_array_row_shapes(ui, first, offset);
                    animating = true;
                }
            }
        });
    }
    let mut drag = drag?;
    let old_destination = drag.to;
    if !escaped && let Some(slot) = hovered {
        drag.to = slot;
    }
    // SortableJS keeps the last previewed destination when ending outside or with Escape.
    if escaped || !down {
        return (drag.from != drag.to).then_some(ldtk_core::FieldArrayEdit::Move {
            from: drag.from,
            to: drag.to,
        });
    }
    if drag.to != old_destination || animating {
        ui.ctx().request_repaint();
    }
    drag.pass = pass;
    drag.rows = rows;
    ui.data_mut(|data| data.insert_temp(id, drag));
    None
}

fn slide_array_row_shapes(ui: &egui::Ui, first: egui::layers::ShapeIdx, offset: f32) {
    let offset = egui::vec2(0., offset);
    let clip = ui.clip_rect();
    ui.ctx().graphics_mut(|graphics| {
        let layer = graphics.entry(ui.layer_id());
        for index in first.0..layer.next_idx().0 {
            layer.mutate_shape(egui::layers::ShapeIdx(index), |entry| {
                entry.clip_rect = entry.clip_rect.translate(offset).intersect(clip);
                entry.shape.translate(offset);
            });
        }
    });
}

fn paint_array_field_drag(
    ui: &egui::Ui,
    id: egui::Id,
    rect: Rect,
    background: egui::layers::ShapeIdx,
    offset: Vec2,
) {
    let viewport = ui.ctx().content_rect();
    ui.ctx().graphics_mut(|graphics| {
        let Some(layer) = graphics.get(ui.layer_id()) else {
            return;
        };
        let content = layer
            .all_entries()
            .skip(background.0 + 1)
            .cloned()
            .collect::<Vec<_>>();
        let ghost = graphics.entry(egui::LayerId::new(egui::Order::Tooltip, id));
        ghost.add(
            viewport,
            egui::Shape::rect_filled(rect.translate(offset), 0., theme::MEDIUM),
        );
        for mut entry in content {
            let clip = entry
                .clip_rect
                .intersect(rect)
                .translate(offset)
                .intersect(viewport);
            if clip.is_positive() {
                entry.shape.translate(offset);
                ghost.add(clip, entry.shape);
            }
        }
    });
    let mut fill = vec![egui::Shape::rect_filled(
        rect,
        0.,
        Color32::from_rgba_unmultiplied(213, 105, 50, 178),
    )];
    let stripes = egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT);
    if let Ok(egui::load::TexturePoll::Ready { texture }) =
        stripes.load_for_size(ui.ctx(), rect.size())
    {
        fill.push(egui::Shape::image(
            texture.id,
            rect,
            Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.),
            Color32::from_white_alpha(178),
        ));
    }
    ui.painter().set(background, egui::Shape::Vec(fill));
    let edge = rect.shrink(0.5);
    ui.painter().extend(egui::Shape::dashed_line(
        &[
            edge.left_top(),
            edge.right_top(),
            edge.right_bottom(),
            edge.left_bottom(),
            edge.left_top(),
        ],
        Stroke::new(1_f32, theme::ACCENT),
        2.,
        1.,
    ));
}

pub(super) fn picker_outline(
    painter: &egui::Painter,
    rect: Rect,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    painter.extend(egui::Shape::dashed_line(
        &[
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ],
        stroke,
        dash,
        gap,
    ));
}

pub(super) fn picker_banner(
    ctx: &egui::Context,
    id: &'static str,
    message: &str,
    wrap_width: f32,
    error: bool,
) -> bool {
    let screen = ctx.content_rect();
    let error_fill = Color32::from_rgb(227, 77, 0);
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(egui::pos2(
            screen.center().x - 200.,
            screen.top() + screen.height() * 0.85,
        ))
        .default_size(egui::vec2(434., 82.))
        .order(egui::Order::Foreground)
        .constrain(false)
        .show(ctx, |ui| {
            let frame = egui::Frame::new()
                .fill(if error { error_fill } else { theme::DARK })
                .inner_margin(17)
                .corner_radius(4)
                .shadow(egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: 32,
                    spread: 0,
                    color: if error { error_fill } else { Color32::BLACK },
                });
            let shown = frame.show(ui, |ui| {
                let color = if error { Color32::WHITE } else { theme::ACCENT };
                let font = if error {
                    FontId::proportional(14.667)
                } else {
                    FontId::new(14.667, egui::FontFamily::Name("body-bold".into()))
                };
                let mut job =
                    egui::text::LayoutJob::simple(message.into(), font, color, wrap_width);
                if !error {
                    job.halign = egui::Align::Center;
                }
                let galley = ui.painter().layout_job(job);
                let height = 48_f32.max(galley.size().y + 8.);
                let (row, _) = ui.allocate_exact_size(egui::vec2(400., height), Sense::hover());
                let center_y = row.center().y;
                egui::Image::new(egui::include_image!("../../assets/icons/pick.svg")).paint_at(
                    ui,
                    Rect::from_min_size(egui::pos2(row.left(), center_y - 24.), Vec2::splat(48.)),
                );
                let text_x = row.left() + if error { 80. } else { 182. };
                ui.painter().galley(
                    egui::pos2(text_x, center_y - galley.size().y * 0.5 - 4.),
                    galley,
                    color,
                );
                ui.place(
                    Rect::from_min_size(
                        egui::pos2(row.right() - 100., center_y - 17.5),
                        egui::vec2(100., 35.),
                    ),
                    egui::Button::new(
                        RichText::new(locale::tr(ui, "CANCEL"))
                            .size(13.333)
                            .color(Color32::WHITE),
                    )
                    .fill(theme::LIGHT)
                    .corner_radius(0.),
                )
                .clicked()
            });
            picker_outline(
                ui.painter(),
                shown.response.rect.shrink(0.5),
                Stroke::new(1_f32, theme::ACCENT),
                2.,
                2.,
            );
            shown.inner
        })
        .inner
}

pub(super) struct FieldPick {
    key: LevelKey,
    owner: Option<String>,
    uid: i64,
    kind: String,
    index: Option<usize>,
    array: bool,
    panel: Option<Panel>,
    layer: usize,
    empty_world_preview: bool,
    show_point_cancel: bool,
    point_held: bool,
    point_waits_for_move: bool,
    point_origin: Option<([i32; 2], Color32)>,
    point_target: Option<[i32; 2]>,
}

impl FieldPick {
    fn point_cell(&self, project: &Project, view: &Viewport, position: [f32; 2]) -> [i32; 2] {
        let grid = self.point_grid(project);
        canvas::pointer_to_layer(project, self.key, self.layer, view, position)
            .map(|axis| (axis / grid).floor() as i32)
    }

    fn point_grid(&self, project: &Project) -> f32 {
        project
            .layer(self.key, self.layer)
            .map(|l| number(l, "__gridSize", 16.))
            .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.))
            .max(1.)
    }

    fn previous_path_point(&self, project: &Project) -> Option<[i32; 2]> {
        if !self.array
            || !matches!(
                self.definition(project)?["editorDisplayMode"].as_str(),
                Some("PointPath" | "PointPathLoop")
            )
        {
            return None;
        }
        let field = canvas::values(&self.owner(project)?["fieldInstances"])
            .iter()
            .find(|field| field["defUid"] == self.uid)?;
        let points = field["__value"].as_array()?;
        let point = points.get(self.index.unwrap_or(points.len()).checked_sub(1)?)?;
        Some([point["cx"].as_i64()? as i32, point["cy"].as_i64()? as i32])
    }

    fn point_link_origin(&self, project: &Project) -> Option<([i32; 2], Color32)> {
        let owner = self.owner(project)?;
        let (origin, color) = if self.owner.is_some() {
            let def = canvas::values(&project.json["defs"]["entities"])
                .iter()
                .find(|def| def["uid"] == owner["defUid"])?;
            let grid = self.point_grid(project);
            let origin = std::array::from_fn(|axis| {
                let pivot = number(def, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
                ((owner["px"][axis].as_f64().unwrap_or(0.) as f32 - f32::from(pivot == 1.)) / grid)
                    as i32
            });
            (
                origin,
                canvas::entity_smart_color(project, owner, def, true),
            )
        } else {
            (
                [0; 2],
                theme::color(text(
                    owner,
                    "bgColor",
                    text(&project.json, "defaultLevelBgColor", "#696A79"),
                )),
            )
        };
        Some((self.previous_path_point(project).unwrap_or(origin), color))
    }

    fn owner<'a>(&self, project: &'a Project) -> Option<&'a Value> {
        if let Some(iid) = &self.owner {
            project.layer(self.key, self.layer)?["entityInstances"]
                .as_array()?
                .iter()
                .find(|e| e["iid"] == *iid)
        } else {
            project.level(self.key)
        }
    }
    fn definition<'a>(&self, project: &'a Project) -> Option<&'a Value> {
        project.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|d| d["fieldDefs"].as_array().into_iter().flatten())
            .chain(
                project.json["defs"]["levelFields"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
            .find(|d| d["uid"] == self.uid)
    }
    fn accepts(&self, project: &Project, key: LevelKey, target: &Value) -> bool {
        let Some(definition) = self.definition(project) else {
            return false;
        };
        let Some(owner) = self.owner(project) else {
            return false;
        };
        if self.owner.as_deref() == target["iid"].as_str()
            || definition["allowOutOfLevelRef"] == false && key != self.key
        {
            return false;
        }
        let existing = owner["fieldInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["defUid"] == self.uid);
        if existing.is_some_and(|field| {
            let values = if self.array {
                field["__value"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            } else {
                std::slice::from_ref(&field["__value"])
            };
            values.iter().any(|v| v["entityIid"] == target["iid"])
        }) {
            return false;
        }
        match definition["allowedRefs"].as_str().unwrap_or("Any") {
            "OnlySame" => self.owner.is_none() || owner["defUid"] == target["defUid"],
            "OnlySpecificEntity" => target["defUid"] == definition["allowedRefsEntityUid"],
            "OnlyTags" => project.json["defs"]["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["uid"] == target["defUid"])
                .is_some_and(|d| {
                    definition["allowedRefTags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|tag| d["tags"].as_array().is_some_and(|tags| tags.contains(tag)))
                }),
            _ => true,
        }
    }

    fn reference_instruction(&self, ui: &egui::Ui, project: &Project) -> String {
        let definition = self.definition(project);
        let entities = canvas::values(&project.json["defs"]["entities"]);
        let entity_name = |uid: Option<i64>| {
            entities
                .iter()
                .find(|def| uid.is_some() && def["uid"].as_i64() == uid)
                .and_then(|def| def["identifier"].as_str())
        };
        let target = match definition.and_then(|def| def["allowedRefs"].as_str()) {
            Some("OnlySame") => {
                let name = entity_name(self.owner(project).and_then(|e| e["defUid"].as_i64()));
                name.map_or_else(
                    || locale::tr(ui, "any entity").into(),
                    |name| locale::format(ui, "another {entity}", &[("entity", name)]),
                )
            }
            Some("OnlySpecificEntity") => {
                match entity_name(definition.and_then(|def| def["allowedRefsEntityUid"].as_i64())) {
                    Some(name) => locale::format(ui, "any {entity} entity", &[("entity", name)]),
                    None => locale::tr(ui, "UNKNOWN ENTITY").into(),
                }
            }
            Some("OnlyTags") => locale::tr(ui, "any entity with tag TODO").into(),
            _ => locale::tr(ui, "any entity").into(),
        };
        let location = locale::tr(
            ui,
            if definition.is_some_and(|d| d["allowOutOfLevelRef"] == false) {
                "in this level"
            } else {
                "in any level"
            },
        );
        locale::format(
            ui,
            "Pick {target} {location}",
            &[("target", &target), ("location", location)],
        )
    }
}

fn reference_level_bounds(project: &Project, key: LevelKey) -> [f32; 4] {
    canvas::placements(project, key)
        .into_iter()
        .find_map(|(level, bounds)| (level == key).then_some(bounds))
        .unwrap_or_else(|| canvas::bounds(project, key, false))
}

impl EditorApp {
    pub(super) fn draw_field_pick_targets(&mut self, ui: &egui::Ui, rect: Rect) {
        let Some(pick) = self.reference_pick.as_ref().or(self.point_pick.as_ref()) else {
            return;
        };
        let Some(project) = &self.project else { return };
        let painter = ui.painter().with_clip_rect(rect);
        if pick.kind == "Point" {
            if let (Some((origin, color)), Some(target)) = (pick.point_origin, pick.point_target) {
                let points = [origin, target].map(|cell| {
                    let local = cell.map(|coord| (coord as f32 + 0.5) * pick.point_grid(project));
                    let world = canvas::layer_to_pointer_world(
                        project,
                        pick.key,
                        pick.layer,
                        &self.viewport,
                        local,
                    );
                    self.viewport
                        .to_screen(world, rect.min.into(), rect.size().into())
                        .into()
                });
                painter.line_segment(points, Stroke::new(self.viewport.zoom, color));
            }
            return;
        }
        picker_outline(
            &painter,
            rect.shrink(14.),
            Stroke::new(4_f32, theme::ACCENT),
            8.,
            4.,
        );
        let alpha = ui.input(|i| (i.time as f32 * 4.2).cos().abs());
        let mut count = 0;
        if !self.world
            && let Some(key) = self.selected
            && let Some(layers) = project
                .level(key)
                .and_then(|l| l["layerInstances"].as_array())
        {
            // Source hint circles include valid targets on hidden layers.
            for layer in layers {
                if layer["__type"] != "Entities" {
                    continue;
                }
                let offset = [
                    number(layer, "__pxTotalOffsetX", 0.),
                    number(layer, "__pxTotalOffsetY", 0.),
                ];
                for entity in layer["entityInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|e| pick.accepts(project, key, e))
                {
                    count += 1;
                    let width = number(entity, "width", 16.);
                    let height = number(entity, "height", 16.);
                    let position = std::array::from_fn(|axis| {
                        let size = [width, height][axis];
                        let px = entity["px"][axis].as_f64().unwrap_or(0.) as f32;
                        let pivot = entity["__pivot"][axis].as_f64().unwrap_or(0.) as f32;
                        (px + (0.5 - pivot) * size).round() + offset[axis]
                    });
                    let center = self
                        .viewport
                        .to_screen(position, rect.min.into(), rect.size().into())
                        .into();
                    for (padding, opacity) in [(8., 1.), (12., 0.33)] {
                        painter.circle_stroke(
                            center,
                            (width.max(height) * 0.5 + padding) * self.viewport.zoom,
                            Stroke::new(
                                self.viewport.zoom,
                                Color32::from_rgba_unmultiplied(
                                    255,
                                    204,
                                    0,
                                    (255. * alpha * opacity) as u8,
                                ),
                            ),
                        );
                    }
                }
            }
        }
        let same_level_only = pick
            .definition(project)
            .is_some_and(|d| d["allowOutOfLevelRef"] == false);
        let error = if self.world && pick.empty_world_preview {
            Some("No valid target in this level.")
        } else if same_level_only && (self.world || self.selected != Some(pick.key)) {
            Some("You can only pick a reference in the same level for this value.")
        } else if !self.world && count == 0 {
            Some("No valid target in this level.")
        } else {
            None
        };
        let message = error.map_or_else(
            || pick.reference_instruction(ui, project),
            |message| locale::tr(ui, message).to_owned(),
        );
        if picker_banner(
            ui.ctx(),
            "entity-reference-picker",
            &message,
            204.,
            error.is_some(),
        ) {
            self.cancel_reference_pick();
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    pub(super) fn picking_field(&self) -> bool {
        self.picking_point() || self.picking_entity_ref()
    }

    pub(super) fn picking_entity_ref(&self) -> bool {
        self.reference_pick.is_some()
    }

    pub(super) fn update_reference_world_preview(&mut self, key: LevelKey) {
        if self.world
            && let Some(project) = &self.project
            && let Some(level) = project.level(key)
            && let Some(pick) = self.reference_pick.as_mut()
        {
            let has_target = canvas::values(&level["layerInstances"])
                .iter()
                .flat_map(|layer| canvas::values(&layer["entityInstances"]))
                .any(|entity| pick.accepts(project, key, entity));
            pick.empty_world_preview |= !has_target;
        }
    }

    pub(super) fn reset_reference_world_preview(&mut self) {
        if let Some(pick) = &mut self.reference_pick {
            pick.empty_world_preview = false;
        }
    }

    pub(super) fn picking_point(&self) -> bool {
        self.point_pick.is_some()
    }

    pub(super) fn picking_point_in_level_panel(&self) -> bool {
        !self.world
            && self.panel == Some(Panel::Level)
            && self
                .point_pick
                .as_ref()
                .is_some_and(|pick| pick.owner.is_none() && pick.panel == self.panel)
    }

    pub(super) fn point_selection_has_priority(&self, world: [f32; 2], alt: bool) -> bool {
        if alt || self.selection_contains_world(world) {
            return true;
        }
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return false;
        };
        let Some(layer) = project
            .layer(key, self.layer)
            .filter(|layer| layer["__type"] == "Entities")
        else {
            return false;
        };
        let local = canvas::pointer_to_layer(project, key, self.layer, &self.viewport, world);
        canvas::values(&layer["entityInstances"])
            .iter()
            .any(|entity| selection_click::entity_is_over(project, entity, local))
    }

    pub(super) fn update_point_picker_context(&mut self, ctx: &egui::Context) {
        let Some(pick) = self.point_pick.as_mut() else {
            return;
        };
        pick.point_target = None;
        if ctx.input(|input| input.pointer.delta() != Vec2::ZERO) {
            pick.point_waits_for_move = false;
        }
        if self.selected != Some(pick.key)
            || self.layer != pick.layer
            || self.world && self.panel == Some(Panel::Level) && pick.owner.is_none()
        {
            self.point_pick = None;
        } else if self.world
            || ctx.input(|i| {
                !i.pointer.primary_down() && !i.pointer.primary_released()
                    || i.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::PointerGone | egui::Event::WindowFocused(false)
                        )
                    })
            })
        {
            pick.point_held = false;
        }
    }

    fn reference_card(
        &mut self,
        ui: &mut egui::Ui,
        source: LevelKey,
        value: &Value,
        width: f32,
        show_preview: bool,
    ) -> Option<egui::Response> {
        let project = self.project.as_ref()?;
        let (target, level, entity) = ui_fields::reference_target(project, value)?;
        let name = text(entity, "__identifier", "Entity");
        let level_name = text(level, "identifier", "");
        let location = if target.world != source.world
            && let Some(world) = target.world
            && let Some(name) = project.json["worlds"][world]["identifier"].as_str()
        {
            locale::format(
                ui,
                "{level} in {world}",
                &[("level", level_name), ("world", name)],
            )
        } else {
            level_name.to_owned()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 33.), Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        let highlighted = response.hovered() || response.has_focus();
        ui.painter().rect_stroke(
            rect,
            4.,
            Stroke::new(
                1_f32,
                if highlighted {
                    theme::ACCENT
                } else {
                    theme::LIGHT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let painter = ui.painter().with_clip_rect(rect.shrink(1.));
        egui::Image::new(egui::include_image!("../../assets/icons/right.svg")).paint_at(
            ui,
            Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(22.)),
        );
        let mut text_x = rect.left() + 26.;
        if show_preview && let Some((def, tile)) = ui_fields::reference_preview(project, entity) {
            let ctx = ui.ctx();
            let icon = Rect::from_min_size(egui::pos2(text_x, rect.top() + 9.5), Vec2::splat(18.));
            let painted = tile.as_ref().is_some_and(|tile| {
                library::paint_tile(ctx, &painter, icon, project, tile, &mut self.assets)
            });
            if !painted {
                library::paint_entity_definition_icon(
                    ctx,
                    &painter,
                    icon,
                    project,
                    def,
                    &mut self.assets,
                );
            }
            text_x += 22.;
        }
        painter.text(
            egui::pos2(text_x, rect.top() + 3.),
            egui::Align2::LEFT_TOP,
            name,
            FontId::proportional(12.),
            if highlighted {
                Color32::WHITE
            } else {
                theme::ACCENT
            },
        );
        painter.text(
            egui::pos2(text_x, rect.top() + 17.),
            egui::Align2::LEFT_TOP,
            location,
            FontId::proportional(10.667),
            if highlighted {
                Color32::WHITE
            } else {
                Color32::from_rgb(115, 127, 157)
            },
        );
        Some(response.on_hover_cursor(egui::CursorIcon::PointingHand))
    }

    pub(super) fn field_pick_controls(
        &mut self,
        ui: &mut egui::Ui,
        key: LevelKey,
        owner: Option<&str>,
        field: &Value,
        definition: &Value,
    ) -> Option<Rect> {
        let kind = text(field, "__type", "");
        let inner = kind
            .strip_prefix("Array<")
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(kind);
        if !matches!(inner, "Point" | "EntityRef") {
            return None;
        }
        let array = kind.starts_with("Array<");
        let mut pick_rect = None;
        let mut requested = None;
        let mut follow = None;
        let mut edit = None;
        let values = if array {
            field["__value"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
        } else {
            std::slice::from_ref(&field["__value"])
        };
        let can_add = array
            && (values.len() as u64) < definition["arrayMaxLength"].as_u64().unwrap_or(u64::MAX);
        if array && inner == "Point" {
            let group = array_field_frame(values.is_empty()).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                if point_array_is_compact(definition) && !values.is_empty() {
                    let summary = values
                        .iter()
                        .map(|value| {
                            point_coordinates(value)
                                .map(|point| format!("<{point}>"))
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let (row, _) = ui
                        .allocate_exact_size(egui::vec2(ui.available_width(), 37.), Sense::hover());
                    let color = Color32::from_rgb(115, 127, 157);
                    let text =
                        ui.painter()
                            .layout(summary, FontId::proportional(12.), color, row.width());
                    ui.painter()
                        .with_clip_rect(row)
                        .galley(row.min, text, color);
                } else {
                    for (index, value) in values.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                let required = value.is_null() && definition["canBeNull"] != true;
                                ui.set_height(if required { 39. } else { 28. });
                                if required {
                                    let show_cancel =
                                        self.point_pick.as_ref().is_some_and(|pick| {
                                            pick.show_point_cancel
                                                && pick.key == key
                                                && pick.owner.as_deref() == owner
                                                && definition["uid"] == pick.uid
                                                && pick.index == Some(index)
                                        });
                                    let label = locale::tr(
                                        ui,
                                        if show_cancel {
                                            "Cancel"
                                        } else {
                                            "Point required!"
                                        },
                                    )
                                    .to_uppercase();
                                    ui.add_space(1.);
                                    let response = ui
                                        .scope(|ui| {
                                            let error = Color32::from_rgb(227, 77, 0);
                                            let widgets = &mut ui.visuals_mut().widgets;
                                            widgets.inactive.weak_bg_fill = error;
                                            for style in [&mut widgets.hovered, &mut widgets.active]
                                            {
                                                style.weak_bg_fill = Color32::from_rgb(255, 93, 74);
                                                style.expansion = 0.;
                                            }
                                            ui.spacing_mut().button_padding = egui::vec2(1., 4.);
                                            let glow = ui.painter().add(egui::Shape::Noop);
                                            let response =
                                                panels::source_controls::add_sized_button(
                                                    ui,
                                                    [77., 38.],
                                                    egui::Button::new(
                                                        RichText::new(label)
                                                            .size(13.333)
                                                            .color(Color32::WHITE),
                                                    )
                                                    .wrap(),
                                                );
                                            ui.painter().set(
                                                glow,
                                                egui::epaint::Shadow {
                                                    offset: [0, 0],
                                                    blur: 32,
                                                    spread: 0,
                                                    color: error,
                                                }
                                                .as_shape(response.rect, 0.),
                                            );
                                            ui.painter().rect_stroke(
                                                response.rect,
                                                0.,
                                                Stroke::new(1_f32, theme::ACCENT),
                                                StrokeKind::Outside,
                                            );
                                            response
                                        })
                                        .inner;
                                    pick_rect = Some(response.rect);
                                    if response.clicked() {
                                        if self.picking_point() {
                                            self.cancel_point_pick();
                                        } else {
                                            requested = Some(Some(index));
                                        }
                                    }
                                } else {
                                    egui::Frame::new()
                                        .fill(theme::MEDIUM)
                                        .corner_radius(6.)
                                        .inner_margin(4)
                                        .show(ui, |ui| {
                                            if let Some(label) = point_coordinates(value) {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(label)
                                                            .monospace()
                                                            .size(13.333),
                                                    )
                                                    .selectable(false),
                                                );
                                            }
                                        });
                                }
                                if array_field_remove(ui).clicked() {
                                    edit = Some(ui_fields::FieldEdit::Array(
                                        ldtk_core::FieldArrayEdit::Remove { index },
                                    ));
                                }
                            });
                        });
                    }
                }
                if can_add {
                    if !values.is_empty() {
                        ui.add_space(2.);
                    }
                    let response = array_field_add(ui, "Add from canvas");
                    pick_rect = Some(response.rect);
                    if response.clicked() {
                        requested = Some(None);
                    }
                }
            });
            if !values.is_empty() {
                array_field_border(ui, group.response.rect);
            }
        } else if inner == "Point" {
            let value = &field["__value"];
            let required = value.is_null() && definition["canBeNull"] != true;
            let show_cancel = self.point_pick.as_ref().is_some_and(|pick| {
                pick.show_point_cancel
                    && pick.key == key
                    && pick.owner.as_deref() == owner
                    && definition["uid"] == pick.uid
            });
            let clear = definition["canBeNull"] == true && !value.is_null();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                ui.spacing_mut().button_padding.y = 2.;
                let height = if required { 24. } else { 23. };
                ui.spacing_mut().interact_size.y = height;
                let width = ui.available_width();
                let row = Rect::from_min_size(ui.next_widget_position(), egui::vec2(width, height));
                if !required
                    && ui_fields::field_uses_default(field, 0)
                    && !ui.rect_contains_pointer(row)
                {
                    ui.set_opacity(0.35);
                }
                let clear_width = (width * 0.23).max(18.);
                let pick_width = if clear {
                    width - clear_width - 2.
                } else {
                    width
                };
                let label = if show_cancel {
                    locale::tr(ui, "Cancel").to_uppercase()
                } else if required {
                    locale::tr(ui, "Point required!").to_uppercase()
                } else {
                    point_coordinates(value)
                        .unwrap_or_else(|| locale::tr(ui, "<No point>").to_uppercase())
                };
                let (fill, hover) = if required {
                    (
                        Color32::from_rgb(227, 77, 0),
                        Color32::from_rgb(255, 93, 74),
                    )
                } else {
                    (Color32::from_rgb(49, 55, 66), theme::LIGHT)
                };
                let widgets = &mut ui.visuals_mut().widgets;
                widgets.inactive.weak_bg_fill = fill;
                widgets.hovered.weak_bg_fill = hover;
                widgets.active.weak_bg_fill = hover;
                let response = panels::source_controls::add_sized_button(
                    ui,
                    [pick_width.max(16.), height],
                    egui::Button::new(RichText::new(label).size(13.333).color(Color32::WHITE))
                        .truncate(),
                );
                pick_rect = Some(response.rect);
                if response.clicked() {
                    if self.picking_point() {
                        self.cancel_point_pick();
                    } else {
                        requested = Some(None);
                    }
                }
                if clear {
                    let widgets = &mut ui.visuals_mut().widgets;
                    for style in [&mut widgets.hovered, &mut widgets.active] {
                        style.weak_bg_fill = theme::ACCENT;
                        style.bg_stroke = Stroke::NONE;
                    }
                    let response = ui
                        .add_sized(
                            [clear_width, height],
                            egui::Button::new(
                                RichText::new("X").size(13.333).color(Color32::WHITE),
                            )
                            .frame_when_inactive(false),
                        )
                        .on_hover_text(locale::tr(ui, "Clear field value"));
                    if response.clicked() {
                        edit = Some(ui_fields::FieldEdit::Value(Value::Null));
                    }
                }
            });
        } else {
            let content = |ui: &mut egui::Ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let mut draw = |ui: &mut egui::Ui, index: usize| {
                    let value = &values[index];
                    ui.spacing_mut().item_spacing.x = 1.;
                    ui.spacing_mut().button_padding = egui::vec2(1., 2.);
                    ui.spacing_mut().interact_size = egui::vec2(22., 28.);
                    let iid = value["entityIid"].as_str();
                    if array || iid.is_some() {
                        ui.set_min_height(33.);
                    }
                    let can_clear = definition["canBeNull"] == true && iid.is_some();
                    let action_count =
                        u8::from(iid.is_some()) + u8::from(can_clear) + u8::from(array);
                    let reserved = 23. * f32::from(action_count);
                    let width = (ui.available_width() - reserved).max(24.);
                    let request = Some(array.then_some(index));
                    if let Some(iid) = iid
                        && let Some(response) =
                            self.reference_card(ui, key, value, width, array && owner.is_some())
                    {
                        self.hover_entity_reference(&response, iid);
                        if response.clicked() {
                            follow = Some(iid.to_owned());
                        }
                    } else {
                        let label = locale::tr(ui, "Missing reference").to_uppercase();
                        let font_size = if array { 12. } else { 13.333 };
                        let width = width - 2.;
                        let width = if array {
                            let label_width = ui
                                .painter()
                                .layout_no_wrap(
                                    label.clone(),
                                    FontId::proportional(font_size),
                                    Color32::WHITE,
                                )
                                .size()
                                .x;
                            width.min(label_width + 8.)
                        } else {
                            width
                        };
                        ui.add_space(1.);
                        let response = reference_alert_button(
                            ui,
                            egui::vec2(width, if array { 22. } else { 23. }),
                            egui::Button::new(
                                RichText::new(label).size(font_size).color(Color32::WHITE),
                            )
                            .truncate(),
                        );
                        pick_rect = Some(response.rect);
                        if response.clicked() {
                            requested = request;
                        }
                    }
                    if iid.is_some() {
                        let response = panels::source_controls::add_sized_button(
                            ui,
                            [22., 28.],
                            egui::Button::image(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/icons/edit.svg"
                                ))
                                .fit_to_exact_size(Vec2::splat(16.)),
                            )
                            .fill(Color32::from_rgb(227, 77, 0)),
                        )
                        .on_hover_text(locale::tr(ui, "Pick from canvas"));
                        pick_rect = Some(response.rect);
                        if response.clicked() {
                            requested = request;
                        }
                    }
                    if can_clear {
                        let response = reference_alert_button(
                            ui,
                            egui::vec2(22., 28.),
                            egui::Button::image(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/icons/clear.svg"
                                ))
                                .fit_to_exact_size(Vec2::splat(16.)),
                            ),
                        )
                        .on_hover_text(locale::tr(ui, "Clear field value"));
                        if response.clicked() {
                            edit = Some(if array {
                                ui_fields::FieldEdit::Array(ldtk_core::FieldArrayEdit::Reset {
                                    index,
                                })
                            } else {
                                ui_fields::FieldEdit::Value(Value::Null)
                            });
                        }
                    }
                    if array && array_field_remove(ui).clicked() {
                        edit = Some(ui_fields::FieldEdit::Array(
                            ldtk_core::FieldArrayEdit::Remove { index },
                        ));
                    }
                };
                if array {
                    if let Some(moved) = sortable_array_field_rows(ui, values.len(), draw) {
                        edit = Some(ui_fields::FieldEdit::Array(moved));
                    }
                } else if !values.is_empty() {
                    ui.push_id(0, |ui| ui.horizontal(|ui| draw(ui, 0)));
                }
                if can_add {
                    if !values.is_empty() {
                        ui.add_space(2.);
                    }
                    let response = array_field_add(ui, "Add from canvas");
                    pick_rect = Some(response.rect);
                    if response.clicked() {
                        let index = values.len();
                        edit = Some(ui_fields::FieldEdit::Array(
                            ldtk_core::FieldArrayEdit::Insert { index },
                        ));
                        requested = Some(Some(index));
                    }
                }
            };
            if array {
                let group = array_field_frame(values.is_empty()).show(ui, content);
                if !values.is_empty() {
                    array_field_border(ui, group.response.rect);
                }
            } else {
                ui.scope(content);
            }
        }
        if let Some(value) = edit {
            let cancel_reference = match &value {
                ui_fields::FieldEdit::Array(ldtk_core::FieldArrayEdit::Remove { .. }) => true,
                ui_fields::FieldEdit::Value(Value::Null)
                | ui_fields::FieldEdit::Array(
                    ldtk_core::FieldArrayEdit::Reset { .. }
                    | ldtk_core::FieldArrayEdit::Insert { .. },
                ) => inner == "EntityRef",
                _ => false,
            };
            let uid = definition["uid"].as_i64().unwrap_or(-1);
            let status = if inner == "EntityRef" {
                ""
            } else {
                "Field changed"
            };
            let applied = if owner.is_some() {
                self.edit_level(status, key, |project| value.apply(project, key, owner, uid))
                    .is_some()
            } else {
                let applied = self
                    .edit(status, |project| value.apply(project, key, owner, uid))
                    .is_some();
                if applied {
                    self.history.clear_level_timelines();
                }
                applied
            };
            if !applied {
                requested = None;
            } else {
                if cancel_reference {
                    self.cancel_reference_pick();
                }
                self.cancel_point_pick();
            }
        }
        if let Some(iid) = follow {
            self.follow_entity(&iid);
        }
        if let Some(index) = requested {
            self.finish_gesture();
            self.finish_stroke();
            let mut pick = FieldPick {
                key,
                owner: owner.map(str::to_owned),
                uid: definition["uid"].as_i64().unwrap_or(-1),
                kind: inner.into(),
                index,
                array,
                panel: self.panel,
                layer: self.layer,
                empty_world_preview: false,
                show_point_cancel: inner == "Point",
                point_held: false,
                point_waits_for_move: false,
                point_origin: None,
                point_target: None,
            };
            if inner == "Point"
                && let Some(project) = &self.project
            {
                pick.point_origin = pick.point_link_origin(project);
            }
            if inner == "Point" {
                self.point_pick = Some(pick);
            } else {
                if let Some(point) = &mut self.point_pick {
                    point.point_held = false;
                    point.point_target = None;
                }
                self.reference_pick = Some(pick);
            }
            let keep_inspector = owner.is_some() && self.panel == Some(Panel::Entity);
            if !keep_inspector && !self.picking_point_in_level_panel() {
                self.panel = None;
                if inner == "Point" {
                    self.status =
                        "Click the canvas to pick a field value · Right click / Escape: finish"
                            .into();
                }
            }
            self.stamp = None;
        }
        pick_rect
    }

    pub(super) fn cancel_field_pick(&mut self) {
        if self.picking_entity_ref() {
            self.cancel_reference_pick();
        } else {
            self.cancel_point_pick();
        }
    }

    pub(super) fn cancel_point_pick(&mut self) {
        if let Some(pick) = self.point_pick.take() {
            self.panel = pick.panel;
        }
    }

    pub(super) fn cancel_reference_pick(&mut self) {
        let Some(pick) = self.reference_pick.take() else {
            return;
        };
        let Some(project) = self
            .project
            .as_ref()
            .filter(|project| project.level(pick.key).is_some())
        else {
            return;
        };
        if self.selected != Some(pick.key) {
            let changed_world = self.active_world != pick.key.world;
            let source = reference_level_bounds(project, pick.key);
            let old = self
                .selected
                .map(|key| reference_level_bounds(project, key))
                .unwrap_or(source);
            let owner_position =
                pick.owner(project)
                    .filter(|_| pick.owner.is_some())
                    .map(|entity| {
                        [
                            entity["px"][0].as_f64().unwrap_or(0.) as f32,
                            entity["px"][1].as_f64().unwrap_or(0.) as f32,
                        ]
                    });
            if changed_world {
                self.navigate_world(pick.key.world);
            } else if !self.world {
                let offset = [old[0] - source[0], old[1] - source[1]];
                self.viewport.center[0] += offset[0];
                self.viewport.center[1] += offset[1];
                self.camera_motion.translate_center(offset);
            }
            self.navigate_level(pick.key, false);
            if !self.world {
                self.brush_preview_waits_for_pointer = true;
            }
            if changed_world {
                self.animate_camera_fit();
            }
            if let Some(mut position) = owner_position {
                if self.world {
                    position[0] += source[0];
                    position[1] += source[1];
                }
                self.camera_motion.scroll_to(position);
            }
        }
        self.switch_layer(pick.layer);
        if self.world {
            self.panel = Some(Panel::World);
        } else if pick.owner.is_none() {
            self.panel = pick.panel;
        }
    }

    fn point_pick_release(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> Option<egui::Pos2> {
        let pick = self.point_pick.as_ref()?;
        let key = pick.key;
        let project = self.project.as_ref()?;
        let density = ctx.pixels_per_point();
        let mut held = pick.point_held;
        let in_level_panel = self.picking_point_in_level_panel();
        let (release, moved, cancel) = ctx.input(|i| {
            let mut release = None;
            let mut moved = false;
            for event in &i.events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        ..
                    } => {
                        if *pressed {
                            held = rect.contains(*pos) && response.contains_pointer();
                        } else {
                            if held
                                && rect.contains(*pos)
                                && (!in_level_panel || response.contains_pointer())
                            {
                                release = Some(*pos);
                            }
                            held = false;
                        }
                    }
                    egui::Event::PointerMoved(pos) if held && rect.contains(*pos) => {
                        let (world, _) =
                            self.canvas_pointer_pixels(project, key, rect, *pos, density);
                        moved |= self.selected_entity_resize_handle(world).is_none()
                            && !self.point_selection_has_priority(world, i.modifiers.alt);
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed: true,
                        ..
                    } if rect.contains(*pos) && response.contains_pointer() => {
                        return (None, moved, true);
                    }
                    _ => {}
                }
            }
            (release, moved, false)
        });
        self.point_pick.as_mut().unwrap().point_held = held;
        if moved {
            self.async_preview(key, ctx.input(|i| i.time));
        }
        if cancel {
            self.cancel_point_pick();
        }
        release
    }

    fn reference_target_at<'a>(
        &self,
        project: &'a Project,
        pick: &FieldPick,
        key: LevelKey,
        position: [f32; 2],
    ) -> Option<(&'a Value, &'a Value)> {
        let level = project.level(key)?;
        for (index, layer) in canvas::values(&level["layerInstances"]).iter().enumerate() {
            if !self.layer_visible(key, index) || layer["__type"] != "Entities" {
                continue;
            }
            let local = if self.world {
                [
                    position[0] - number(layer, "__pxTotalOffsetX", 0.),
                    position[1] - number(layer, "__pxTotalOffsetY", 0.),
                ]
            } else {
                canvas::pointer_to_layer(project, key, index, &self.viewport, position)
            };
            if let Some(entity) = canvas::values(&layer["entityInstances"])
                .iter()
                .find(|entity| {
                    pick.accepts(project, key, entity)
                        && selection_click::entity_is_over(project, entity, local)
                })
            {
                return Some((layer, entity));
            }
        }
        None
    }

    pub(super) fn field_pick_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        if !self.picking_field() {
            return false;
        }
        let point = !self.picking_entity_ref();
        if point && self.world {
            return false;
        }
        if point
            && let (Some(pointer), Some(project), Some(key)) =
                (response.hover_pos(), &self.project, self.selected)
        {
            let (world, _) =
                self.canvas_pointer_pixels(project, key, rect, pointer, ctx.pixels_per_point());
            let in_level_panel = self.picking_point_in_level_panel();
            if !in_level_panel
                && let Some((_, _, handle)) = self.selected_entity_resize_handle(world)
            {
                ctx.set_cursor_icon(level_resize::cursor(handle));
            } else if (in_level_panel
                || !self.point_selection_has_priority(world, ctx.input(|i| i.modifiers.alt)))
                && self.layer_visible(key, self.layer)
                && let Some(pick) = &mut self.point_pick
                && !pick.point_waits_for_move
            {
                pick.point_target = Some(pick.point_cell(project, &self.viewport, world));
                ctx.set_cursor_icon(egui::CursorIcon::Default);
            }
        }
        let pointer = if point {
            self.point_pick_release(ctx, rect, response)
        } else {
            response.hover_pos()
        };
        let Some(pointer) = pointer else {
            return true;
        };
        let pick = self
            .reference_pick
            .as_ref()
            .or(self.point_pick.as_ref())
            .unwrap();
        let Some(project) = &self.project else {
            return true;
        };
        let value = if pick.kind == "Point" {
            let grid = pick.point_grid(project);
            let layer = project.layer(pick.key, pick.layer);
            let (position, _) = self.canvas_pointer_pixels(
                project,
                pick.key,
                rect,
                pointer,
                ctx.pixels_per_point(),
            );
            let cell = pick.point_cell(project, &self.viewport, position);
            let width = layer
                .map(|l| number(l, "__cWid", 0.0))
                .unwrap_or_else(|| number(project.level(pick.key).unwrap(), "pxWid", 0.0) / grid)
                as i32;
            let height = layer
                .map(|l| number(l, "__cHei", 0.0))
                .unwrap_or_else(|| number(project.level(pick.key).unwrap(), "pxHei", 0.0) / grid)
                as i32;
            if cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height {
                return true;
            }
            json!({"cx":cell[0],"cy":cell[1]})
        } else {
            let Some(key) = self.selected else {
                return !self.world;
            };
            let (position, _) =
                self.canvas_pointer_pixels(project, key, rect, pointer, ctx.pixels_per_point());
            let level_bounds = if self.world {
                reference_level_bounds(project, key)
            } else {
                canvas::bounds(project, key, false)
            };
            let local = [position[0] - level_bounds[0], position[1] - level_bounds[1]];
            let Some((layer, entity)) = self.reference_target_at(project, pick, key, local) else {
                // Presses on a different preview go to the world tool. The picker
                // can resolve its entities only after that level becomes current.
                let width = level_bounds[2] - level_bounds[0];
                let height = level_bounds[3] - level_bounds[1];
                return !self.world
                    || ((0.0..width).contains(&local[0].round())
                        && (0.0..height).contains(&local[1].round()));
            };
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            if !ctx.input(|i| i.pointer.primary_pressed()) {
                return true;
            }
            let Some(level) = project.level(key) else {
                return true;
            };
            json!({
                "entityIid": entity["iid"],
                "layerIid": layer["iid"],
                "levelIid": level["iid"],
                "worldIid": key.world.and_then(|world| project.json["worlds"][world]["iid"].as_str())
                    .or_else(|| project.json["dummyWorldIid"].as_str())
            })
        };
        let point_cell = (pick.kind == "Point").then(|| {
            [
                value["cx"].as_i64().unwrap_or(0) as i32,
                value["cy"].as_i64().unwrap_or(0) as i32,
            ]
        });
        let key = pick.key;
        let layer = pick.layer;
        let owner = pick.owner.clone();
        let uid = pick.uid;
        let mut close = !point || !pick.array;
        let edit = if pick.array {
            let array = pick
                .owner(project)
                .and_then(|o| o["fieldInstances"].as_array())
                .and_then(|a| a.iter().find(|f| f["defUid"] == uid))
                .and_then(|f| f["__value"].as_array())
                .map(Vec::as_slice)
                .unwrap_or_default();
            if point && array.contains(&value) {
                return true;
            }
            ui_fields::FieldEdit::Array(if let Some(index) = pick.index {
                close |= index + 1 < array.len();
                ldtk_core::FieldArrayEdit::Set { index, value }
            } else {
                ldtk_core::FieldArrayEdit::InsertValue {
                    index: array.len(),
                    value,
                }
            })
        } else {
            ui_fields::FieldEdit::Value(value)
        };
        let pending_point = point
            && self
                .async_paint
                .as_ref()
                .is_some_and(|preview| preview.is_active_for(key, ctx.input(|i| i.time)));
        let snapshot = if pending_point && owner.is_some() {
            self.capture_entity_render(key, layer)
        } else {
            None
        };
        let preview = if pending_point {
            self.async_paint.take()
        } else {
            None
        };
        let status = if point { "Field picked" } else { "" };
        let result = if !point && owner.is_some() && self.selected != Some(key) {
            // The source records the slot insertion separately from a cross-level pick.
            self.edit_level_without_undo(status, key, |p| edit.apply(p, key, owner.as_deref(), uid))
        } else if owner.is_some() {
            self.edit_level_marked(status, key, |p, history| {
                if let Some(entity) = p
                    .layer(key, layer)
                    .and_then(|l| l["entityInstances"].as_array())
                    .and_then(|es| es.iter().find(|e| e["iid"].as_str() == owner.as_deref()))
                {
                    history.mark_entity_change(p, key, layer, entity);
                }
                edit.apply(p, key, owner.as_deref(), uid)
            })
        } else {
            let result = self.edit(status, |p| edit.apply(p, key, None, uid));
            if result.is_some() {
                self.history.clear_level_timelines();
            }
            result
        };
        if let Some(preview) = preview {
            self.async_paint = Some(preview);
            if result.is_some() {
                self.retain_entity_render(snapshot);
            }
        }
        if result.is_some() && !point {
            self.chain_entity = None;
            if let Some(pick) = &mut self.point_pick
                && pick.key == key
                && pick.owner == owner
            {
                // The reference edit refreshes the form while the Point tool stays active.
                pick.show_point_cancel = false;
            }
        }
        if result.is_some()
            && let Some(cell) = point_cell
        {
            self.bleep_layer_cells(key, layer, [cell[0], cell[1], 1, 1], theme::ACCENT);
        }
        if result.is_some() && close {
            self.cancel_field_pick();
        } else if result.is_some()
            && point
            && let Some(pick) = &mut self.point_pick
        {
            pick.index = None;
            pick.point_target = None;
            pick.point_waits_for_move = true;
            if let Some(project) = &self.project
                && pick.previous_path_point(project).is_some()
            {
                pick.point_origin = pick.point_link_origin(project);
            }
        }

        true
    }
}

#[cfg(test)]
#[path = "field_tools_tests.rs"]
mod tests;
