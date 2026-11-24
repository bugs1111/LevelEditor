use super::super::source_controls::edit_text;
use super::*;
use crate::app::panels::IntGridRemoval;

const NEW_EMBEDDED_ICONS: i64 = -1;

#[derive(Clone, Copy)]
struct ValueDrag {
    layer: i64,
    value: i64,
}
#[derive(Clone, Copy)]
struct GroupDrag {
    layer: i64,
    index: usize,
}

fn value_group(value: &Value) -> i64 {
    value["groupUid"].as_i64().unwrap_or(0)
}

fn move_value(values: &mut Vec<Value>, value: i64, group: i64, slot: usize) -> bool {
    let Some(from) = values.iter().position(|v| v["value"] == value) else {
        return false;
    };
    let old_group = value_group(&values[from]);
    let members: Vec<_> = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| (value_group(v) == group).then_some(i))
        .collect();
    let old_slot = members.iter().position(|&i| i == from);
    let slot = slot - usize::from(old_slot.is_some_and(|i| i < slot));
    if old_slot == Some(slot) {
        return false;
    }
    if let Some(&last) = members.last() {
        // LDtk moves a value dropped lower in its own group to the end of that group.
        let to = if slot >= members.len() || old_slot.is_some_and(|i| slot > i) {
            last + 1
        } else {
            members[slot]
        };
        let item = values.remove(from);
        let new_index = to - usize::from(from < to);
        values.insert(new_index, item);
        values[new_index]["groupUid"] = json!(group);
    } else if old_group != group {
        values[from]["groupUid"] = json!(group);
    }
    old_group != group
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    tip: &str,
    size: Vec2,
    fill: Color32,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(4., 4.);
        source_controls::add_sized_button(
            ui,
            size,
            egui::Button::image(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.)))
                .fill(fill)
                .corner_radius(2.),
        )
        .on_hover_text(locale::tr(ui, tip))
    })
    .inner
}

fn drag_handle(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24., 30.), Sense::drag());
    let color = if response.hovered() || response.dragged() {
        Color32::WHITE
    } else {
        Color32::from_white_alpha(92)
    };
    for dy in [-1.5, 1.5] {
        let p = rect.center() + egui::vec2(0., dy);
        ui.painter()
            .hline(p.x - 6.0..=p.x + 6., p.y, Stroke::new(1_f32, color));
    }
    response.on_hover_and_drag_cursor(egui::CursorIcon::Grab)
}

fn group_actions(
    ui: &mut egui::Ui,
    colored: bool,
    remove: &mut bool,
    color: &mut bool,
    reset: &mut bool,
) {
    ui.set_min_width(160.);
    if ui
        .button(
            RichText::new(locale::tr(ui, "DELETE GROUP"))
                .size(13.333)
                .color(theme::ACCENT),
        )
        .clicked()
    {
        *remove = true;
        ui.close();
    }
    if ui
        .button(
            RichText::new(locale::tr(ui, "SET GROUP COLOR"))
                .size(13.333)
                .color(theme::ACCENT),
        )
        .clicked()
    {
        *color = true;
        ui.close();
    }
    if colored
        && ui
            .button(
                RichText::new(locale::tr(ui, "REMOVE GROUP COLOR"))
                    .size(13.333)
                    .color(theme::ACCENT),
            )
            .clicked()
    {
        *reset = true;
        ui.close();
    }
}

fn drop_slot(response: &egui::Response, slot: usize) -> usize {
    slot + usize::from(
        response
            .ctx
            .input(|input| input.pointer.latest_pos())
            .is_some_and(|p| p.y > response.rect.center().y),
    )
}

fn value_drop(
    ui: &egui::Ui,
    response: &egui::Response,
    layer: i64,
    group: i64,
    slot: usize,
) -> Option<(i64, i64, usize)> {
    let drag = response.dnd_hover_payload::<ValueDrag>()?;
    if drag.layer != layer {
        return None;
    }
    let index = drop_slot(response, slot);
    let y = if index == slot {
        response.rect.top()
    } else {
        response.rect.bottom()
    };
    ui.painter().hline(
        response.rect.x_range(),
        y,
        Stroke::new(2_f32, theme::ACCENT),
    );
    response.dnd_release_payload::<ValueDrag>()?;
    Some((drag.value, group, index))
}

impl EditorApp {
    pub(super) fn int_grid_values(&mut self, ui: &mut egui::Ui, def: &Value) {
        let layer = def["uid"].as_i64().unwrap();
        let project = self.project.as_ref().unwrap();
        let state =
            ui.make_persistent_id(("intgrid-values", text(&project.json, "iid", ""), layer));
        let mut values = def["intGridValues"].as_array().cloned().unwrap_or_default();
        let mut groups = def["intGridValuesGroups"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let atlas_key = state.with("icon-tileset");
        let atlas = ui
            .data(|data| data.get_temp::<Option<i64>>(atlas_key))
            .unwrap_or_else(|| values.iter().find_map(|v| v["tile"]["tilesetUid"].as_i64()));
        let mut selected_atlas = atlas;
        layer_row(ui, "Values interface icons", |ui| {
            let tilesets = project.json["defs"]["tilesets"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            if let Some(choice) = tileset_picker(
                ui,
                ui.available_width(),
                tilesets,
                atlas,
                "-- No icon --",
                false,
            ) {
                selected_atlas = match choice {
                    TilesetChoice::Unassigned => None,
                    TilesetChoice::Existing(uid) => Some(uid),
                    TilesetChoice::EmbeddedIcons => Some(NEW_EMBEDDED_ICONS),
                };
            }
        });
        if selected_atlas != atlas {
            for value in &mut values {
                value["tile"] = Value::Null;
            }
        }
        let mut add_value = None;
        let mut add_group = false;
        let mut remove = None;
        let mut value_move = None;
        let mut group_move = None;
        layer_row(ui, "Grid values", |ui| {
            ui.vertical(|ui| {
                egui::Frame::new().fill(theme::DARK).inner_margin(10).corner_radius(3).show(ui, |ui| {
                    let width = ui.available_width();
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 4.;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let group_width = (width * 0.25).max(30.);
                        if icon_button(ui, egui::include_image!("../../assets/icons/add.svg"),
                            "Create an IntGrid value", egui::vec2(width - group_width - 2., 32.), theme::LIGHT).clicked() {
                            add_value = Some(0);
                        }
                        add_group = icon_button(ui, egui::include_image!("../../assets/icons/folder.svg"),
                            "Create a group of IntGrid values", egui::vec2(group_width, 32.), theme::LIGHT).clicked();
                    });
                    let has_groups = !groups.is_empty();
                    for (index, group) in std::iter::once((None, None))
                        .chain(groups.iter_mut().enumerate().map(|(i, g)| (Some(i), Some(g))))
                    {
                        let id = group.as_ref().and_then(|g| g["uid"].as_i64()).unwrap_or(0);
                        let row = ui.push_id(("intgrid-group", id), |ui| {
                            let fill = group.as_ref()
                                .and_then(|g| g["color"].as_str())
                                .map(theme::color)
                                .map_or(theme::MEDIUM, |color| theme::DARK.lerp_to_gamma(color, 0.7));
                            let frame = egui::Frame::new().fill(fill).corner_radius(2)
                                .inner_margin(egui::Margin { left: if has_groups { 32 } else { 8 }, right: 2, top: 8, bottom: 8 })
                                .show(ui, |ui| {
                                    let width = ui.available_width();
                                    ui.set_width(width);
                                    ui.spacing_mut().item_spacing.y = 4.;
                                    if has_groups {
                                        let header = ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.;
                                            ui.add(egui::Image::new(egui::include_image!("../../assets/icons/folderClose.svg"))
                                                .fit_to_exact_size(Vec2::splat(22.)));
                                            if let Some(g) = group {
                                                if self.int_grid_group_header(ui, g, id) {
                                                    remove = Some(IntGridRemoval::Group(id));
                                                }
                                            } else {
                                                ui.label(RichText::new(locale::tr(ui, "Ungrouped")).italics().color(theme::TEXT.gamma_multiply(0.62)));
                                            }
                                        });
                                        if let Some(index) = index {
                                            let handle = Rect::from_min_size(
                                                header.response.rect.left_top() - egui::vec2(26., 0.),
                                                egui::vec2(20., 32.),
                                            );
                                            let hover = ui.interact(handle, ui.id().with("group-drag"), Sense::drag());
                                            hover.dnd_set_drag_payload(GroupDrag { layer, index });
                                            let color = if hover.hovered() { Color32::WHITE } else { Color32::from_white_alpha(92) };
                                            for dy in [-1.5, 1.5] {
                                                ui.painter().hline(handle.center().x - 6.0..=handle.center().x + 6., handle.center().y + dy, Stroke::new(1_f32, color));
                                            }
                                        }
                                    }
                                    let indices: Vec<_> = values.iter().enumerate()
                                        .filter_map(|(i, v)| (value_group(v) == id).then_some(i))
                                        .collect();
                                    if indices.is_empty() {
                                        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.), Sense::hover());
                                        source_controls::disabled_hatch(ui, rect);
                                        value_move = value_drop(ui, &response, layer, id, 0).map(|(value, group, _)| (value, group, 0)).or(value_move);
                                    }
                                    for (slot, index) in indices.into_iter().enumerate() {
                                        let value = values[index]["value"].as_i64().unwrap_or(0);
                                        let row = ui.push_id(("intgrid-value", value), |ui| {
                                            ui.horizontal(|ui| self.int_grid_value_row(ui, &mut values[index], layer, value, selected_atlas, state))
                                        }).inner;
                                        let (name, delete) = row.inner;
                                        if let Some(anchor) = delete {
                                            remove = Some(IntGridRemoval::Value(value, anchor));
                                        }
                                        if let Some(name) = name {
                                            let cleaned = Project::cleanup_tag(&name);
                                            if name.trim().is_empty() {
                                                values[index]["identifier"] = Value::Null;
                                            } else if let Some(name) = cleaned.filter(|name| !values.iter().enumerate().any(|(i, v)| i != index && v["identifier"] == *name)) {
                                                values[index]["identifier"] = json!(name);
                                            } else {
                                                self.status = "Invalid or duplicate IntGrid value identifier".into();
                                                self.status_error = true;
                                            }
                                        }
                                        value_move = value_drop(ui, &row.response, layer, id, slot).or(value_move);
                                    }
                                    if icon_button(ui, egui::include_image!("../../assets/icons/add.svg"),
                                        "Create an IntGrid value in this group", egui::vec2(width, 32.), SOURCE_ORANGE).clicked() {
                                        add_value = Some(id);
                                    }
                                });
                            if index.is_some() {
                                let rect = frame.response.rect;
                                ui.painter().rect_filled(Rect::from_min_size(rect.min, egui::vec2(3., rect.height())), 0., Color32::from_rgb(117, 128, 158));
                            }
                            frame.response
                        }).inner;
                        if let Some(index) = index
                            && let Some(drag) = row.dnd_hover_payload::<GroupDrag>()
                            && drag.layer == layer
                        {
                            let slot = drop_slot(&row, index);
                            let y = if slot == index { row.rect.top() } else { row.rect.bottom() };
                            ui.painter().hline(row.rect.x_range(), y, Stroke::new(2_f32, theme::ACCENT));
                            if row.dnd_release_payload::<GroupDrag>().is_some() {
                                group_move = Some((drag.index, slot - usize::from(drag.index < slot)));
                            }
                        }
                    }
                });
            });
        });
        let regrouped = value_move
            .is_some_and(|(value, group, slot)| move_value(&mut values, value, group, slot));
        if let Some((from, to)) = group_move
            && from != to
        {
            let moved = groups.remove(from);
            groups.insert(to, moved);
        }
        let mut focus = None;
        if let Some(group) = add_value {
            let next = values
                .iter()
                .filter_map(|v| v["value"].as_i64())
                .max()
                .unwrap_or(0)
                + 1;
            if next <= i64::from(i32::MAX) {
                let color = self.suggested_color(&values);
                values.push(json!({"value":next,"identifier":null,"color":format!("#{color:06X}"),"tile":null,"groupUid":group}));
                focus = Some(next);
            }
        }
        if add_group {
            let uid = groups
                .iter()
                .filter_map(|g| g["uid"].as_i64())
                .max()
                .unwrap_or(0)
                + 1;
            groups.push(json!({"uid":uid,"identifier":null,"color":null}));
        }
        let mut confirmed_removal = false;
        if let Some(action) = remove
            && self.int_grid_removal_needs_confirmation(layer, action, &values)
        {
            self.int_grid_removal = Some((layer, action));
            remove = None;
        }
        if let Some((owner, action)) = self.int_grid_removal
            && owner == layer
            && let Some(confirm) = self.confirm_int_grid_removal(ui.ctx(), action)
        {
            self.int_grid_removal = None;
            if confirm {
                remove = Some(action);
                confirmed_removal = true;
            }
        }
        match remove {
            Some(IntGridRemoval::Value(id, _)) => values.retain(|v| v["value"] != id),
            Some(IntGridRemoval::Group(id)) => {
                groups.retain(|g| g["uid"] != id);
                for v in &mut values {
                    if value_group(v) == id {
                        v["groupUid"] = json!(0);
                    }
                }
            }
            None => {}
        }
        let recoverable = confirmed_removal && matches!(remove, Some(IntGridRemoval::Value(..)));
        let same =
            |key, items: &[Value]| def[key].as_array().map(Vec::as_slice).unwrap_or(&[]) == items;
        let contents_changed =
            !same("intGridValues", &values) || !same("intGridValuesGroups", &groups);
        let create_icons = selected_atlas == Some(NEW_EMBEDDED_ICONS);
        let applied = if contents_changed || create_icons {
            let patch = json!({"intGridValues":values,"intGridValuesGroups":groups});
            let current_level = self.selected;
            let edit = |p: &mut Project| {
                let atlas = if create_icons {
                    Some(p.ensure_embedded_icons()?)
                } else {
                    selected_atlas
                };
                if contents_changed {
                    p.update_layer_properties(layer, &patch)?;
                }
                // LDtk updates rules that read this IntGrid in the current level.
                if confirmed_removal && let Some(key) = current_level {
                    // Cell removal already rebuilt the IntGrid's own rules.
                    if matches!(remove, Some(IntGridRemoval::Group(_))) {
                        p.rebuild_auto_layer(key, layer)?;
                    }
                    let linked: Vec<_> = p.json["defs"]["layers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|def| {
                            def["type"] == "AutoLayer" && def["autoSourceLayerDefUid"] == layer
                        })
                        .filter_map(|def| def["uid"].as_i64())
                        .collect();
                    for uid in linked {
                        p.rebuild_auto_layer(key, uid)?;
                    }
                }
                Ok(atlas)
            };
            let result = if recoverable {
                self.edit_recoverable("IntGrid value removed", edit)
            } else {
                self.edit("IntGrid palette changed", edit)
            };
            if let Some(atlas) = result {
                selected_atlas = atlas;
                self.normalize_palette();
            }
            result.is_some()
        } else {
            true
        };
        if applied {
            self.pending_auto_layer_rebuild |= regrouped;
            if selected_atlas != atlas {
                ui.data_mut(|data| data.insert_temp(atlas_key, selected_atlas));
            }
            if let Some(value) = focus {
                ui.data_mut(|data| data.insert_temp(state.with("focus"), value));
            }
        }
    }

    fn int_grid_group_header(&mut self, ui: &mut egui::Ui, group: &mut Value, uid: i64) -> bool {
        let color_id = ui.make_persistent_id("group-color");
        let fallback = locale::format(ui, "Group {uid}", &[("uid", &uid.to_string())]);
        let current = group["identifier"].as_str().unwrap_or("");
        let name_state = ui.make_persistent_id("editing-group-name");
        let size = egui::vec2((ui.available_width() - 51.).max(30.), 32.);
        let mut remove = false;
        let mut set_color = false;
        let mut reset = false;
        let colored = group["color"].is_string();
        if let Some(focus) = ui.data(|data| data.get_temp::<bool>(name_state)) {
            let mut close = false;
            let draft = edit_text(ui, "group-name", current, |ui, draft, id| {
                if focus {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                let response = ui.add_sized(
                    size,
                    egui::TextEdit::singleline(draft)
                        .id(id)
                        .vertical_align(egui::Align::Center)
                        .hint_text(&fallback),
                );
                close = response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter));
                response
            });
            if let Some(name) = draft {
                group["identifier"] = json!(Project::cleanup_tag(&name));
            }
            ui.data_mut(|data| {
                if close {
                    data.remove::<bool>(name_state);
                } else {
                    data.insert_temp(name_state, false);
                }
            });
        } else {
            let name = if current.is_empty() {
                &fallback
            } else {
                current
            };
            let response = ui
                .allocate_ui_with_layout(
                    size,
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(size.x);
                        ui.add(
                            egui::Label::new(RichText::new(name).strong().color(Color32::WHITE))
                                .truncate()
                                .sense(Sense::click()),
                        )
                    },
                )
                .inner;
            if response.clicked() {
                ui.data_mut(|data| data.insert_temp(name_state, true));
            }
            response.context_menu(|ui| {
                group_actions(ui, colored, &mut remove, &mut set_color, &mut reset)
            });
        }
        let menu = source_controls::add_sized_button(
            ui,
            egui::vec2(47., 32.),
            egui::Button::new("⋮").fill(Color32::from_black_alpha(22)),
        )
        .on_hover_text(locale::tr(ui, "Group options"));
        egui::Popup::menu(&menu)
            .align(egui::RectAlign::BOTTOM_END)
            .show(|ui| group_actions(ui, colored, &mut remove, &mut set_color, &mut reset));
        egui::Popup::context_menu(&menu)
            .id(menu.id.with("context"))
            .show(|ui| group_actions(ui, colored, &mut remove, &mut set_color, &mut reset));
        if reset {
            group["color"] = Value::Null;
        }
        if set_color
            || ui
                .data(|data| data.get_temp::<bool>(color_id))
                .unwrap_or(false)
        {
            let color = group["color"]
                .as_str()
                .map(theme::color)
                .unwrap_or(Color32::BLACK);
            let mut rgb = [color.r(), color.g(), color.b()];
            let done =
                source_colors::standalone_color_dialog(ui.ctx(), color_id, &mut rgb, set_color);
            ui.data_mut(|data| data.insert_temp(color_id, !done));
            if done {
                group["color"] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
            }
        }
        remove
    }

    fn int_grid_value_row(
        &mut self,
        ui: &mut egui::Ui,
        value: &mut Value,
        layer: i64,
        id: i64,
        atlas: Option<i64>,
        state: egui::Id,
    ) -> (Option<String>, Option<Rect>) {
        ui.spacing_mut().item_spacing.x = 4.;
        drag_handle(ui).dnd_set_drag_payload(ValueDrag { layer, value: id });
        let color = theme::color(value["color"].as_str().unwrap_or("#000000"));
        let (badge, _) = ui.allocate_exact_size(egui::vec2(32., 30.), Sense::hover());
        ui.painter()
            .rect_filled(badge, 2., Color32::BLACK.lerp_to_gamma(color, 0.5));
        ui.painter().rect_stroke(
            badge,
            2.,
            Stroke::new(1_f32, color.lerp_to_gamma(Color32::WHITE, 0.2)),
            StrokeKind::Inside,
        );
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            id,
            FontId::proportional(19.),
            color.lerp_to_gamma(Color32::WHITE, 0.5),
        );
        let mut rgb = [color.r(), color.g(), color.b()];
        let swatch = source_colors::color_editor_sized(ui, &mut rgb, None, egui::vec2(64., 32.));
        ui.painter().rect_stroke(
            swatch.rect.shrink(1.),
            0.,
            Stroke::new(3_f32, Color32::from_gray(229)),
            StrokeKind::Inside,
        );
        if swatch.changed() {
            value["color"] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        }
        if atlas.is_some() {
            if let Some(icon) = self.value_tile_thumbnail(ui, atlas, &value["tile"], true, true) {
                value["tile"] = icon;
            }
        } else {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.), Sense::hover());
            source_controls::disabled_hatch(ui, rect);
        }
        let width = (ui.available_width() - 32.).max(20.);
        let name = edit_text(
            ui,
            "value-name",
            text(value, "identifier", ""),
            |ui, draft, editor_id| {
                let focus_id = state.with("focus");
                if ui.data(|data| data.get_temp::<i64>(focus_id)) == Some(id) {
                    ui.memory_mut(|memory| memory.request_focus(editor_id));
                    ui.data_mut(|data| data.remove::<i64>(focus_id));
                }
                let hint = locale::tr(ui, "Optional value identifier");
                ui.add_sized(
                    [width, 32.],
                    egui::TextEdit::singleline(draft)
                        .id(editor_id)
                        .background_color(Color32::BLACK.lerp_to_gamma(color, 0.3))
                        .horizontal_align(egui::Align::Center)
                        .vertical_align(egui::Align::Center)
                        .hint_text(RichText::new(hint).italics().color(theme::LIGHT)),
                )
            },
        );
        let delete = icon_button(
            ui,
            egui::include_image!("../../assets/icons/delete.svg"),
            "Delete IntGrid value",
            egui::vec2(28., 32.),
            Color32::from_rgb(121, 17, 5),
        );
        (name, delete.clicked().then_some(delete.rect))
    }

    fn int_grid_removal_needs_confirmation(
        &self,
        layer: i64,
        action: IntGridRemoval,
        values: &[Value],
    ) -> bool {
        match action {
            IntGridRemoval::Value(value, _) => self.project.as_ref().is_some_and(|p| {
                p.level_keys()
                    .into_iter()
                    .filter_map(|key| p.level(key))
                    .flat_map(|level| level["layerInstances"].as_array().into_iter().flatten())
                    .filter(|li| li["layerDefUid"] == layer)
                    .any(|li| {
                        li["intGridCsv"]
                            .as_array()
                            .is_some_and(|csv| csv.iter().any(|v| *v == value))
                    })
            }),
            IntGridRemoval::Group(group) => values.iter().any(|v| value_group(v) == group),
        }
    }

    fn confirm_int_grid_removal(
        &self,
        ctx: &egui::Context,
        action: IntGridRemoval,
    ) -> Option<bool> {
        let screen = ctx.content_rect();
        let width = (screen.width() - 40.).clamp(180., 358.);
        let question = match action {
            IntGridRemoval::Value(..) => {
                "This value is used in some levels: removing it will also remove the value from all these levels. Are you sure?"
            }
            IntGridRemoval::Group(_) => {
                "Deleting this group will move all its values back to \"UNGROUPED\". Confirm?"
            }
        };
        let id = egui::Id::new("intgrid-removal");
        let mut modal = egui::Modal::new(id)
            .frame(
                project_dialogs::dialog_frame(20)
                    .fill(Color32::from_rgb(121, 17, 5))
                    .corner_radius(0.)
                    .stroke(Stroke::new(1_f32, SOURCE_ORANGE)),
            )
            .backdrop_color(Color32::from_black_alpha(140));
        if let IntGridRemoval::Value(_, anchor) = action {
            let x = if anchor.left() >= screen.left() + screen.width() * 0.6 {
                anchor.right() - width
            } else {
                anchor.left()
            };
            let (y, pivot) = if anchor.top() >= screen.top() + screen.height() * 0.7 {
                (anchor.top(), Align2::LEFT_BOTTOM)
            } else {
                (anchor.bottom(), Align2::LEFT_TOP)
            };
            modal = modal.area(
                egui::Area::new(id)
                    .kind(egui::UiKind::Modal)
                    .sense(Sense::hover())
                    .order(egui::Order::Foreground)
                    .pivot(pivot)
                    .constrain_to(screen.shrink(16.))
                    .fixed_pos(egui::pos2(x, y)),
            );
        }
        let response = modal.show(ctx, |ui| {
            ui.set_width(width);
            ui.label(
                RichText::new(locale::tr(ui, question))
                    .size(14.5)
                    .color(Color32::WHITE),
            );
            ui.add_space(12.);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                let size = egui::vec2((width - 2.) * 0.5, 35.);
                let confirm = project_dialogs::button(ui, "CONFIRM", size, false, 13.333).clicked();
                let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                (confirm, cancel)
            })
            .inner
        });
        let (confirm, cancel) = response.inner;
        if confirm || ctx.input(|input| input.key_pressed(Key::Enter)) {
            Some(true)
        } else if cancel || response.should_close() {
            Some(false)
        } else {
            None
        }
    }
}
