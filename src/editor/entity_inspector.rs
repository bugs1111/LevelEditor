//! Source EntityInstanceEditor: a right-edge inspector, independent of modal panels.
use super::tools::Gesture;
use super::*;

fn state_id() -> egui::Id {
    egui::Id::new("entity-instance-inspector")
}
#[derive(Clone)]
struct State {
    width: f32,
    rect: Rect,
    opacity: f32,
    entity_iid: String,
}
impl Default for State {
    fn default() -> Self {
        Self {
            width: 350.,
            rect: Rect::NOTHING,
            opacity: 1.,
            entity_iid: String::new(),
        }
    }
}
pub(super) fn remember_canvas(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|data| data.insert_temp(state_id().with("canvas"), rect));
}
fn coordinate(
    ui: &mut egui::Ui,
    key: &str,
    value: i32,
    unit: f64,
    bounds: [i32; 2],
    enabled: bool,
) -> Option<i32> {
    ui.push_id(key, |ui| {
        let draft_id = ui.id().with("coordinate-draft");
        let mut display = ui
            .data(|data| data.get_temp::<(i32, f64, f64)>(draft_id))
            .filter(|(original, original_unit, _)| *original == value && *original_unit == unit)
            .map_or(f64::from(value) / unit, |(_, _, draft)| draft);
        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
        let response = ui.add_enabled(
            enabled,
            egui::DragValue::new(&mut display)
                .range(f64::from(bounds[0]) / unit..=f64::from(bounds[1]) / unit)
                .speed(0.088 * if unit > 1. { 0.05 } else { 1. })
                .max_decimals(if unit > 1. { 4 } else { 0 })
                .min_decimals(0)
                .custom_formatter(|value, _| value.to_string())
                .update_while_editing(false),
        );
        ui_commit::track_editor(ui.ctx(), &response);
        let commit = response.drag_stopped() || (response.changed() && !response.dragged());
        ui.data_mut(|data| {
            if commit {
                data.remove::<(i32, f64, f64)>(draft_id);
            } else {
                data.insert_temp(draft_id, (value, unit, display));
            }
        });
        let next = (display * unit) as i32;
        (commit && next != value).then_some(next)
    })
    .inner
}
fn incoming_references(project: &Project, iid: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    for key in project.level_keys() {
        let Some(level) = project.level(key) else {
            continue;
        };
        for layer in level["layerInstances"].as_array().into_iter().flatten() {
            for source in layer["entityInstances"].as_array().into_iter().flatten() {
                let points_to =
                    |value: &Value| value["entityIid"] == iid || value.as_str() == Some(iid);
                let refers = source["fieldInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|field| match field["__type"].as_str() {
                        Some("EntityRef") => points_to(&field["__value"]),
                        Some("Array<EntityRef>") => field["__value"]
                            .as_array()
                            .is_some_and(|values| values.iter().any(points_to)),
                        _ => false,
                    });
                if refers {
                    refs.push((
                        text(source, "iid", "").to_owned(),
                        format!(
                            "{} / {}",
                            text(level, "identifier", "Level"),
                            text(source, "__identifier", "Entity")
                        ),
                    ));
                }
            }
        }
    }
    refs
}
impl EditorApp {
    pub(super) fn show_entity_inspector(&mut self, ctx: &egui::Context) {
        if self
            .selected
            .and_then(|key| {
                self.project.as_ref()?.layer(key, self.layer)?["entityInstances"]
                    .get(self.entity_index?)
            })
            .is_none()
        {
            self.show_panel(None);
            return;
        }
        let mut state = ctx
            .data(|data| data.get_temp::<State>(state_id()))
            .unwrap_or_default();
        let iid = self
            .project
            .as_ref()
            .unwrap()
            .layer(self.selected.unwrap(), self.layer)
            .unwrap()["entityInstances"][self.entity_index.unwrap()]["iid"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let changed_entity = state.entity_iid != iid;
        state.entity_iid = iid;
        let occupied = matches!(
            self.gesture,
            Some(
                Gesture::EntityMove { .. }
                    | Gesture::EntityResize { .. }
                    | Gesture::SelectionMove { .. }
                    | Gesture::Rectangle { select: true, .. }
            )
        );
        state.opacity = ctx.animate_value_with_time(
            state_id().with("opacity"),
            if occupied { 0.3 } else { 1. },
            0.3,
        );
        let max_height = ctx.content_rect().height() * 0.9;
        let shown = egui::Area::new(state_id())
            .order(egui::Order::Middle)
            .anchor(egui::Align2::RIGHT_CENTER, egui::Vec2::ZERO)
            .movable(false)
            .constrain(false)
            .default_width(state.width)
            .default_height(max_height)
            .show(ctx, |ui| {
                ui.set_opacity(state.opacity);
                ui.set_max_height(max_height);
                let frame = egui::Frame::new()
                    .fill(theme::DARK)
                    .inner_margin(10)
                    .shadow(egui::Shadow {
                        offset: [-4, 0],
                        blur: 16,
                        spread: 0,
                        color: Color32::from_black_alpha(153),
                    })
                    .show(ui, |ui| {
                        ui.set_width(state.width - 20.);
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        let mut scroll = egui::ScrollArea::vertical()
                            .id_salt("entity-properties-scroll")
                            .max_height((max_height - 20.).max(1.))
                            .auto_shrink([false, true]);
                        if changed_entity {
                            scroll = scroll.vertical_scroll_offset(0.);
                        }
                        scroll.show(ui, |ui| self.properties(ui, Panel::Entity));
                    });
                let bar = Rect::from_min_max(
                    frame.response.rect.min,
                    egui::pos2(
                        frame.response.rect.left() + 4.,
                        frame.response.rect.bottom(),
                    ),
                );
                let response = ui
                    .interact(bar, state_id().with("resize"), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                ui.painter().rect_filled(bar, 0., theme::ACCENT);
                if response.dragged()
                    && let Some(pos) = ctx.pointer_interact_pos()
                {
                    state.width = (ctx.content_rect().right() - pos.x).clamp(350., 820.);
                    ctx.request_repaint();
                }
                frame.response.rect
            });
        state.rect = shown.inner;
        if (state.rect.center().y - ctx.content_rect().center().y).abs() > 0.5 {
            ctx.request_repaint();
        }
        self.draw_entity_inspector_link(ctx, state.rect);
        ctx.data_mut(|data| data.insert_temp(state_id(), state));
    }
    fn draw_entity_inspector_link(&self, ctx: &egui::Context, panel: Rect) {
        let Some(canvas) = ctx.data(|data| data.get_temp::<Rect>(state_id().with("canvas"))) else {
            return;
        };
        let Some(p) = &self.project else {
            return;
        };
        let Some(key) = self.selected else {
            return;
        };
        let Some(entity) = self
            .entity_index
            .and_then(|index| p.layer(key, self.layer)?["entityInstances"].get(index))
        else {
            return;
        };
        let Some(def) = p.json["defs"]["entities"]
            .as_array()
            .and_then(|defs| defs.iter().find(|d| d["uid"] == entity["defUid"]))
        else {
            return;
        };
        // Original connector uses raw entity center, without layer offset/parallax.
        let center = std::array::from_fn(|axis| {
            let pos = entity["px"][axis].as_f64().unwrap_or(0.) as f32;
            let size = number(entity, if axis == 0 { "width" } else { "height" }, 16.);
            let pivot = number(def, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
            (pos + (0.5 - pivot) * size + 0.5).floor()
        });
        let start = egui::Pos2::from(self.viewport.to_screen(
            center,
            canvas.min.into(),
            canvas.size().into(),
        ));
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            state_id().with("link"),
        ))
        .with_clip_rect(canvas)
        .line_segment(
            [start, panel.left_center()],
            Stroke::new(
                4_f32,
                theme::color(text(def, "color", "#ffcc00")).gamma_multiply(0.33),
            ),
        );
    }
    pub(super) fn entity_inspector_contents(&mut self, ui: &mut egui::Ui) {
        let (Some(key), Some(index), Some(project)) =
            (self.selected, self.entity_index, self.project.as_ref())
        else {
            return;
        };
        let Some(layer) = project.layer(key, self.layer) else {
            return;
        };
        let Some(entity) = layer["entityInstances"].get(index).cloned() else {
            return;
        };
        let Some(def) = project.json["defs"]["entities"]
            .as_array()
            .and_then(|defs| defs.iter().find(|d| d["uid"] == entity["defUid"]))
            .cloned()
        else {
            return;
        };
        let iid = text(&entity, "iid", "");
        let grid = number(layer, "__gridSize", 16.).max(1.);
        let dimensions = [
            number(project.level(key).unwrap(), "pxWid", 256.) as i32,
            number(project.level(key).unwrap(), "pxHei", 256.) as i32,
        ];
        let readonly = project.is_backup();
        let project_iid = text(&project.json, "iid", "").to_owned();
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 36. - ui.spacing().item_spacing.x).max(1.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 20.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    ui.add(
                        egui::Label::new(
                            RichText::new(text(&def, "identifier", "Entity"))
                                .strong()
                                .color(theme::ACCENT),
                        )
                        .truncate(),
                    );
                },
            );
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("Edit")
                            .strong()
                            .size(10.667)
                            .color(Color32::BLACK),
                    )
                    .fill(theme::ACCENT)
                    .corner_radius(2)
                    .small()
                    .wrap_mode(egui::TextWrapMode::Extend),
                )
                .clicked()
            {
                self.definition_uid = def["uid"].as_i64();
                self.show_panel(Some(Panel::Entities));
            }
        });
        ui.separator();
        let mut extra_open = self
            .settings
            .ui_state_int("extraEntityInfos", Some(&project_iid), 0)
            != 0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.), Sense::click());
        let hover = response.hovered();
        ui.painter()
            .rect_filled(rect, 3, if hover { theme::ACCENT } else { theme::LIGHT });
        egui::Image::new(if extra_open {
            egui::include_image!("../../assets/icons/expanded.svg")
        } else {
            egui::include_image!("../../assets/icons/collapsed.svg")
        })
        .paint_at(
            ui,
            Rect::from_center_size(rect.left_center() + egui::vec2(12., 0.), Vec2::splat(16.)),
        );
        ui.painter().text(
            rect.left_center() + egui::vec2(24., 0.),
            egui::Align2::LEFT_CENTER,
            "EXTRA ENTITY INFOS",
            FontId::new(17.333, egui::FontFamily::Name("panel-title".into())),
            if hover || extra_open {
                Color32::BLACK
            } else {
                theme::ACCENT
            },
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            extra_open = !extra_open;
            self.settings.set_ui_state_int(
                "extraEntityInfos",
                Some(&project_iid),
                i64::from(extra_open),
            );
        }
        if extra_open {
            egui::Frame::new()
                .fill(theme::MEDIUM)
                .stroke(Stroke::new(1_f32, theme::LIGHT))
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.label("IID");
                    ui.horizontal(|ui| {
                        let mut id = iid.to_owned();
                        ui.add(
                            egui::TextEdit::singleline(&mut id)
                                .desired_width((ui.available_width() - 75.).max(30.))
                                .interactive(false),
                        );
                        if ui.small_button("Copy IID").clicked() {
                            ui.ctx().copy_text(iid.to_owned());
                        }
                    });
                    ui.label("Coords");
                    let unit_id = state_id().with("grid-units");
                    let mut cells = ui
                        .ctx()
                        .data(|data| data.get_temp::<bool>(unit_id))
                        .unwrap_or(false);
                    let unit = if cells { f64::from(grid) } else { 1. };
                    let mut position = [
                        entity["px"][0].as_i64().unwrap_or(0) as i32,
                        entity["px"][1].as_i64().unwrap_or(0) as i32,
                    ];
                    let mut size = [
                        number(&entity, "width", 16.) as i32,
                        number(&entity, "height", 16.) as i32,
                    ];
                    let mut moved = false;
                    let mut resized = false;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.;
                        for axis in 0..2 {
                            if let Some(v) = coordinate(
                                ui,
                                if axis == 0 { "x" } else { "y" },
                                position[axis],
                                unit,
                                [0, dimensions[axis]],
                                !readonly,
                            ) {
                                position[axis] = v;
                                moved = true;
                            }
                            ui.label(if axis == 0 { "," } else { ";" });
                        }
                        for (axis, dimension) in size.iter_mut().enumerate() {
                            let (enabled, min, max) = if axis == 0 {
                                ("resizableX", "minWidth", "maxWidth")
                            } else {
                                ("resizableY", "minHeight", "maxHeight")
                            };
                            let bounds = [
                                number(&def, min, 1.) as i32,
                                number(&def, max, 32767.) as i32,
                            ];
                            if let Some(v) = coordinate(
                                ui,
                                if axis == 0 { "w" } else { "h" },
                                *dimension,
                                unit,
                                bounds,
                                !readonly && def[enabled] == true,
                            ) {
                                *dimension = v;
                                resized = true;
                            }
                            if axis == 0 {
                                ui.label("×");
                            }
                        }
                        if ui
                            .small_button(if cells { "cells" } else { "px" })
                            .clicked()
                        {
                            cells = !cells;
                            ui.ctx().data_mut(|data| data.insert_temp(unit_id, cells));
                        }
                    });
                    let layer = self.layer;
                    if moved {
                        self.edit_level("Entity moved", key, |p| {
                            p.move_entity(key, layer, index, position[0], position[1])
                        });
                    }
                    if resized {
                        self.edit_level("Entity resized", key, |p| {
                            p.resize_entity(key, layer, index, size[0], size[1])
                        });
                    }
                    let refs = incoming_references(self.project.as_ref().unwrap(), iid);
                    if !refs.is_empty() {
                        ui.label("References to this entity");
                        for (iid, label) in &refs {
                            if ui.small_button(label).clicked() {
                                self.follow_entity(iid);
                            }
                        }
                    }
                });
        }
        if let Some(doc) = def["doc"].as_str().filter(|doc| !doc.is_empty()) {
            egui::Frame::new()
                .fill(Color32::from_rgb(39, 63, 47))
                .stroke(Stroke::new(1_f32, Color32::from_rgb(94, 171, 107)))
                .inner_margin(8)
                .corner_radius(3)
                .show(ui, |ui| {
                    ui.colored_label(Color32::from_rgb(121, 221, 138), doc);
                });
            ui.add_space(8.);
        }
        ui.add_enabled_ui(!readonly, |ui| {
            self.instance_fields(ui, key, Some(iid), &entity)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const KEY: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    struct Fixture {
        app: EditorApp,
        ctx: egui::Context,
        time: f64,
        field: i64,
    }
    impl Fixture {
        fn new() -> Self {
            let mut p = Project::new();
            p.add_layer("Actors", "Entities", 16).unwrap();
            let uid = p.add_entity_def("Actor", 32, 48).unwrap();
            p.update_entity_definition(uid,&json!({"resizableX":true,"resizableY":false,"doc":"An actor with editable properties.","color":"#204060"})).unwrap();
            let field = p
                .add_field_definition(Some(uid), "Name", "String", false)
                .unwrap();
            p.add_entity(KEY, 0, uid, 64, 80).unwrap();
            let iid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
                .as_str()
                .unwrap()
                .to_owned();
            p.set_field_value(KEY, Some(&iid), field, json!("seed"))
                .unwrap();
            let mut app = EditorApp::empty();
            app.set_project(p);
            app.entity_index = Some(0);
            app.panel = Some(Panel::Entity);
            app.fit = false;
            Self {
                app,
                ctx: {
                    let ctx = egui::Context::default();
                    theme::configure(&ctx);
                    ctx
                },
                time: 0.,
                field,
            }
        }
        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            self.time += 1. / 60.;
            self.app.repaint = Some(self.ctx.clone());
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            self.app.ui_commit_raw_input(&self.ctx, &mut input);
            self.ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    remember_canvas(ctx, ui.max_rect());
                });
                self.app.dialogs(ctx);
                self.app.flush_ui_action(ctx);
            })
        }
        fn warm(&mut self) -> egui::FullOutput {
            self.frame(vec![]);
            self.frame(vec![]);
            self.frame(vec![])
        }
        fn state(&self) -> State {
            self.ctx
                .data(|d| d.get_temp::<State>(state_id()))
                .expect("inspector state")
        }
        fn click_pos(&mut self, pos: egui::Pos2) {
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![button(pos, true)]);
            self.frame(vec![button(pos, false)]);
            self.frame(vec![]);
        }
        fn click(&mut self, text: &str) {
            let output = self.warm();
            self.click_pos(label(&output, text));
        }
    }
    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn label(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, label))
            .unwrap_or_else(|| panic!("missing label {label}"))
    }
    #[test]
    fn inspector_header_keeps_edit_on_one_line_at_minimum_width() {
        fn rows(shape: &egui::Shape) -> Option<usize> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == "Edit" => {
                    Some(text.galley.rows.len())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(rows),
                _ => None,
            }
        }
        let mut f = Fixture::new();
        let output = f.warm();
        let button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.fill == theme::ACCENT
                        && rect.rect.width() > 15.
                        && rect.rect.height() < 30. =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .expect("orange header edit button");
        assert!(
            (button.right() - (f.state().rect.right() - 10.)).abs() < 1.,
            "{button:?}"
        );
        assert_eq!(
            output.shapes.iter().find_map(|shape| rows(&shape.shape)),
            Some(1)
        );
    }
    #[test]
    fn inspector_title_aligns_left_and_extra_info_state_survives_a_new_context() {
        let mut f = Fixture::new();
        let output = f.warm();
        let title = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Actor" => Some(text.pos.x),
                _ => None,
            })
            .unwrap();
        assert!((title - f.state().rect.left() - 10.).abs() < 1.);
        f.click("EXTRA ENTITY INFOS");
        let iid = text(&f.app.project.as_ref().unwrap().json, "iid", "");
        assert_eq!(
            f.app
                .settings
                .ui_state_int("extraEntityInfos", Some(iid), 0),
            1
        );
        f.ctx = egui::Context::default();
        theme::configure(&f.ctx);
        let output = f.warm();
        label(&output, "Copy IID");
    }
    #[test]
    fn inspector_is_right_anchored_centered_and_limited_to_ninety_percent_height() {
        let mut f = Fixture::new();
        f.warm();
        let state = f.state();
        assert!((state.rect.right() - 1000.).abs() < 1.);
        assert!((state.rect.width() - 350.).abs() < 1.);
        assert!(
            (state.rect.center().y - 400.).abs() < 1.,
            "{:?}",
            state.rect
        );
        assert!(state.rect.height() <= 720.);
    }
    #[test]
    fn actual_extra_infos_toggle_exposes_iid_copy_and_cell_units() {
        let mut f = Fixture::new();
        f.click("EXTRA ENTITY INFOS");
        let output = f.warm();
        label(&output, "px");
        f.click("px");
        let output = f.warm();
        label(&output, "cells");
        label(&output, "4");
        label(&output, "5");
        let pos = label(&output, "Copy IID");
        f.frame(vec![egui::Event::PointerMoved(pos), button(pos, true)]);
        let output = f.frame(vec![button(pos, false)]);
        let iid =
            f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
                .as_str()
                .unwrap();
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(text) if text==iid))
        );
    }
    #[test]
    fn actual_resize_bar_changes_width_clamps_to_source_bounds_and_remains_centered() {
        let mut f = Fixture::new();
        f.warm();
        let start = f.state().rect.left_center() + egui::vec2(2., 0.);
        f.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        f.frame(vec![egui::Event::PointerMoved(egui::pos2(400., 400.))]);
        f.frame(vec![button(egui::pos2(400., 400.), false)]);
        f.warm();
        assert!((f.state().width - 600.).abs() < 1.);
        let start = f.state().rect.left_center() + egui::vec2(2., 0.);
        f.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        f.frame(vec![egui::Event::PointerMoved(egui::pos2(50., 400.))]);
        f.frame(vec![button(egui::pos2(50., 400.), false)]);
        f.warm();
        assert_eq!(f.state().width, 820.);
        assert!((f.state().rect.center().y - 400.).abs() < 1.);
    }
    #[test]
    fn inspector_fades_during_entity_and_selection_gestures_and_recovers() {
        let mut f = Fixture::new();
        f.warm();
        f.app.gesture = Some(Gesture::EntityMove {
            key: KEY,
            layer: 0,
            index: 0,
            start: [64., 80.],
            end: [96., 80.],
            origin: [64, 80],
            moved: true,
        });
        f.frame(vec![]);
        f.time += 0.4;
        f.frame(vec![]);
        assert!((f.state().opacity - 0.3).abs() < 0.01);
        f.app.gesture = None;
        f.frame(vec![]);
        f.time += 0.4;
        f.frame(vec![]);
        assert!((f.state().opacity - 1.).abs() < 0.01);
    }
    #[test]
    fn actual_header_edit_commits_custom_field_draft_before_switching_definition_panel() {
        let mut f = Fixture::new();
        let output = f.warm();
        label(&output, "An actor with editable properties.");
        f.click("seed");
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        f.frame(vec![key(
            egui::Key::A,
            egui::Modifiers {
                command: true,
                ctrl: true,
                ..Default::default()
            },
        )]);
        f.frame(vec![egui::Event::Text("updated".into())]);
        assert!(!f.app.dirty, "typing remains a draft until blur");
        f.click("Edit");
        f.warm();
        assert!(f.app.panel == Some(Panel::Entities));
        let entity = &f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0];
        let field = entity["fieldInstances"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["defUid"] == f.field)
            .unwrap();
        assert_eq!(field["__value"], "updated");
        assert!(f.app.dirty);
    }
    #[test]
    fn incoming_reference_button_follows_source_across_levels() {
        let mut f = Fixture::new();
        let p = f.app.project.as_mut().unwrap();
        let uid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["defUid"]
            .as_i64()
            .unwrap();
        let target = p.layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let field = p
            .add_field_definition(Some(uid), "Next", "EntityRef", false)
            .unwrap();
        let next = p.add_level(None, "Other", 256, 256).unwrap();
        p.add_entity(next, 0, uid, 112, 128).unwrap();
        let source = p.layer(next, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_value(next, Some(&source), field, json!({"entityIid":target}))
            .unwrap();
        f.click("EXTRA ENTITY INFOS");
        f.click("Other / Actor");
        assert_eq!(f.app.selected, Some(next));
        assert_eq!(f.app.entity_index, Some(0));
        assert!(f.app.panel == Some(Panel::Entity));
        assert!(!f.app.world);
    }
    #[test]
    fn long_custom_fields_scroll_without_growing_beyond_source_height_limit() {
        let mut f = Fixture::new();
        let p = f.app.project.as_mut().unwrap();
        let uid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["defUid"]
            .as_i64()
            .unwrap();
        for i in 0..20 {
            p.add_field_definition(Some(uid), &format!("Field_{i}"), "String", false)
                .unwrap();
        }
        f.warm();
        let state = f.state();
        assert!(state.rect.height() <= 720.1);
        assert!(state.rect.height() > 700.);
        assert!((state.rect.width() - 350.).abs() < 1.);
    }
    #[test]
    fn cell_coordinate_text_commits_pixels_and_keeps_disabled_dimension_unchanged() {
        let mut f = Fixture::new();
        f.click("EXTRA ENTITY INFOS");
        f.click("px");
        f.click("4");
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        f.frame(vec![key(
            egui::Key::A,
            egui::Modifiers {
                command: true,
                ctrl: true,
                ..Default::default()
            },
        )]);
        f.frame(vec![egui::Event::Text("4.75".into())]);
        f.frame(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        let e = &f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0];
        assert_eq!(e["px"][0], 76);
        assert_eq!(e["height"], 48);
        assert!(f.app.dirty);
    }
    #[test]
    fn canvas_drag_release_over_faded_inspector_still_finishes_entity_move() {
        let mut f = Fixture::new();
        f.warm();
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 800.));
        let frame = |f: &mut Fixture, events: Vec<egui::Event>| {
            f.time += 1. / 60.;
            let _ = f.ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    time: Some(f.time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let (canvas, response) =
                                ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                            remember_canvas(ctx, canvas);
                            f.app.canvas_controls(ctx, canvas, &response);
                        });
                    f.app.dialogs(ctx);
                },
            );
        };
        let start = egui::Pos2::from(f.app.viewport.to_screen(
            [64., 80.],
            rect.min.into(),
            rect.size().into(),
        ));
        frame(&mut f, vec![]);
        frame(&mut f, vec![egui::Event::PointerMoved(start)]);
        frame(&mut f, vec![button(start, true)]);
        assert!(matches!(f.app.gesture, Some(Gesture::EntityMove { .. })));
        let end = egui::pos2(700., 400.);
        frame(&mut f, vec![egui::Event::PointerMoved(end)]);
        frame(&mut f, vec![button(end, false)]);
        assert!(f.app.gesture.is_none());
        assert!(f.app.dirty);
        assert_ne!(
            f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["px"],
            json!([64, 80])
        );
    }
    #[test]
    fn coordinate_slider_keeps_document_unchanged_until_release() {
        let mut f = Fixture::new();
        f.click("EXTRA ENTITY INFOS");
        let output = f.warm();
        let start = label(&output, "64");
        f.frame(vec![egui::Event::PointerMoved(start)]);
        f.frame(vec![button(start, true)]);
        let end = start + egui::vec2(50., 0.);
        f.frame(vec![egui::Event::PointerMoved(end)]);
        assert_eq!(
            f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["px"][0],
            64
        );
        f.frame(vec![button(end, false)]);
        assert_eq!(
            f.app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["px"][0],
            68
        );
    }
}
