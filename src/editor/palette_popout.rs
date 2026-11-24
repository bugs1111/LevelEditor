//! TilePalette's unfitted hover expansion and source directional auto-closing.
use super::*;
use serde_json::json;

fn tileset_selector_id() -> egui::Id {
    egui::Id::new("tile-palette-tileset-picker")
}

fn tileset_warning_id() -> egui::Id {
    egui::Id::new("palette-tileset-confirm")
}

pub(super) fn tileset_selector_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, tileset_selector_id())
        || ctx.data(|data| data.get_temp::<i64>(tileset_warning_id()).is_some())
}

#[derive(Clone, Default)]
struct PopOut {
    open: bool,
    last: Option<egui::Pos2>,
    leaving: f32,
    dragging: bool,
    suspend_until: f64,
    need_closing: bool,
    was_hovered: bool,
    last_frame: u64,
    mask_capture: bool,
}
impl PopOut {
    fn move_pointer(&mut self, point: egui::Pos2, bounds: Rect, sidebar: f32, time: f64) {
        if let Some(last) = self.last {
            if self.dragging || bounds.contains(point) {
                self.leaving = 0.;
            } else if point.y < bounds.top() && point.x < sidebar {
                self.open = false;
            } else {
                let angle = |p: egui::Pos2| (p.y - point.y).atan2(p.x - point.x);
                let center = angle(bounds.center());
                let delta = |angle: f32| {
                    let d = center - angle;
                    d.sin().atan2(d.cos())
                };
                let corners = [
                    bounds.left_top(),
                    bounds.right_top(),
                    bounds.right_bottom(),
                    bounds.left_bottom(),
                ]
                .map(|p| delta(angle(p)));
                let min = corners.into_iter().fold(f32::INFINITY, f32::min) - 0.35;
                let max = corners.into_iter().fold(f32::NEG_INFINITY, f32::max) + 0.35;
                let motion = point - last;
                let direction = delta(motion.y.atan2(motion.x));
                self.leaving = (self.leaving
                    + motion.length()
                        * if direction >= min && direction <= max {
                            -3.
                        } else {
                            1.
                        })
                .max(0.);
                if self.leaving >= 30. {
                    if time < self.suspend_until {
                        self.need_closing = true;
                    } else {
                        self.open = false;
                    }
                } else {
                    self.need_closing = false;
                }
            }
        }
        self.last = Some(point);
    }
    fn update(&mut self, events: &[egui::Event], bounds: Rect, sidebar: f32, time: f64) {
        for event in events {
            match event {
                egui::Event::PointerMoved(point) => {
                    self.move_pointer(*point, bounds, sidebar, time)
                }
                egui::Event::PointerButton {
                    pos, pressed: true, ..
                } if bounds.contains(*pos) => self.dragging = true,
                egui::Event::PointerButton { pressed: false, .. } => {
                    self.dragging = false;
                    self.leaving = 0.;
                    self.suspend_until = time + 0.2;
                }
                _ => {}
            }
        }
        if self.need_closing && time >= self.suspend_until {
            self.open = false;
        }
    }
}
fn active_id() -> egui::Id {
    egui::Id::new("active-tile-palette-popout")
}
fn view_id() -> egui::Id {
    egui::Id::new("canvas-palette-fit")
}

#[derive(Clone, Copy)]
pub(super) struct CanvasAtlasView {
    pub fit: bool,
    pub sidebar_size: Vec2,
}

pub(super) fn canvas_view(ctx: &egui::Context) -> Option<CanvasAtlasView> {
    ctx.data(|d| d.get_temp::<CanvasAtlasView>(view_id()))
}
fn focus_request_id(uid: i64) -> egui::Id {
    egui::Id::new(("tile-palette-focus-request", uid))
}
pub(super) fn request_focus(ctx: &egui::Context, uid: i64, immediate: bool) {
    ctx.data_mut(|data| data.insert_temp(focus_request_id(uid), immediate));
}
pub(super) fn take_focus(ctx: &egui::Context, uid: i64) -> Option<bool> {
    ctx.data_mut(|data| data.remove_temp(focus_request_id(uid)))
}
pub(super) fn consume_escape(ctx: &egui::Context) -> bool {
    let Some((id, frame)) = ctx.data(|d| d.get_temp::<(egui::Id, u64)>(active_id())) else {
        return false;
    };
    if frame + 1 < ctx.cumulative_frame_nr()
        || !ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        return false;
    }
    ctx.data_mut(|d| {
        if let Some(mut state) = d.get_temp::<PopOut>(id) {
            state.open = false;
            state.was_hovered = true;
            d.insert_temp(id, state);
        }
        d.remove::<(egui::Id, u64)>(active_id());
    });
    true
}
impl EditorApp {
    fn tile_palette_header(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        uid: i64,
        fit: &mut bool,
    ) {
        let mut target = uid;
        let (choices, default_uid, incompatible) = self
            .project
            .as_ref()
            .map(|p| {
                let layer = self.selected.and_then(|key| p.layer(key, self.layer));
                let default_uid = layer
                    .and_then(|l| l["layerDefUid"].as_i64())
                    .and_then(|id| p.layer_def(id))
                    .and_then(|d| d["tilesetDefUid"].as_i64());
                let occupied =
                    layer.is_some_and(|l| l["gridTiles"].as_array().is_some_and(|a| !a.is_empty()));
                let width = p
                    .tileset(uid)
                    .map(|d| d["pxWid"].clone())
                    .unwrap_or(Value::Null);
                let choices = p.json["defs"]["tilesets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        (
                            d["uid"].as_i64().unwrap_or(-1),
                            text(d, "identifier", "").to_owned(),
                            occupied && d["pxWid"] != width,
                        )
                    })
                    .collect::<Vec<_>>();
                (choices, default_uid, occupied)
            })
            .unwrap_or_default();
        let (header, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 29.), Sense::hover());
        let button_width = 30.65625;
        let buttons_left = header.right() - 3. - 3. * button_width - 4.;
        let selector =
            Rect::from_min_max(header.min, egui::pos2(buttons_left - 9., header.bottom()));
        let choices_text = choices
            .iter()
            .map(|(id, name, bad)| {
                let default = if Some(*id) == default_uid {
                    locale::tr(ui, "(default)")
                } else {
                    ""
                };
                let warning = if *bad {
                    locale::tr(ui, "(INCOMPATIBLE SIZE!)")
                } else {
                    ""
                };
                [name, default, warning]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        if let Some(current) = choices.iter().position(|(id, _, _)| *id == uid) {
            let response = ui
                .interact(selector, ui.id().with("palette-tileset"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let value = &choices_text[current];
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, response.enabled(), value)
            });
            let id = tileset_selector_id();
            let painter = ui.painter();
            painter.rect_filled(
                selector.translate(egui::vec2(0., 2.)),
                3.,
                Color32::from_black_alpha(77),
            );
            let border = if response.hovered() || egui::Popup::is_id_open(ctx, id) {
                Color32::WHITE
            } else {
                Color32::from_rgb(115, 127, 157)
            };
            painter.rect(
                selector,
                3.,
                theme::LIGHT,
                Stroke::new(1_f32, border),
                StrokeKind::Inside,
            );
            let font = FontId::proportional(13.333);
            let caption = Rect::from_min_max(
                selector.min + egui::vec2(8., 0.),
                selector.max - egui::vec2(18., 0.),
            );
            let full = painter.layout_no_wrap(value.clone(), font.clone(), Color32::WHITE);
            let galley = if full.size().x <= caption.width() {
                full
            } else {
                painter.layout_no_wrap(choices[current].1.clone(), font, Color32::WHITE)
            };
            painter.with_clip_rect(caption).galley(
                caption.left_center() - egui::vec2(0., galley.size().y / 2.),
                galley,
                Color32::WHITE,
            );
            let arrow = selector.right_center() - egui::vec2(9., 0.);
            painter.add(egui::Shape::line(
                vec![
                    arrow + egui::vec2(-4., -2.),
                    arrow + egui::vec2(0., 2.),
                    arrow + egui::vec2(4., -2.),
                ],
                Stroke::new(1.5_f32, Color32::WHITE),
            ));
            let options = choices_text
                .iter()
                .zip(&choices)
                .map(|(text, (_, _, bad))| {
                    (
                        text.as_str(),
                        if *bad { theme::DARK } else { Color32::WHITE },
                    )
                })
                .collect::<Vec<_>>();
            if let Some(index) =
                panels::source_controls::select_popup(&response, id, current, &options)
            {
                target = choices[index].0;
            }
        }

        let button = |slot, image, toggle, tip, keys| {
            let rect = Rect::from_min_size(
                egui::pos2(
                    buttons_left + slot as f32 * (button_width + 2.),
                    header.top() + 1.,
                ),
                egui::vec2(button_width, 27.),
            );
            let response = ui
                .interact(rect, ui.id().with(tip), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    response.enabled(),
                    locale::tr(ui, tip),
                )
            });
            let painter = ui.painter();
            if let Some(on) = toggle {
                let green = Color32::from_rgb(207, 221, 64);
                let outline = if on { green } else { theme::LIGHT };
                painter.rect(
                    rect,
                    2.,
                    theme::DARK,
                    Stroke::new(1_f32, outline),
                    StrokeKind::Outside,
                );
                if on {
                    for (inset, amount) in [0.44, 0.36, 0.28, 0.21, 0.15, 0.10, 0.06, 0.03]
                        .into_iter()
                        .enumerate()
                    {
                        painter.rect_stroke(
                            rect.shrink(inset as f32),
                            2.,
                            Stroke::new(1_f32, green.gamma_multiply(amount)),
                            StrokeKind::Inside,
                        );
                    }
                }
            } else {
                painter.add(
                    egui::epaint::Shadow {
                        offset: [0, 2],
                        blur: 2,
                        spread: 0,
                        color: Color32::from_black_alpha(77),
                    }
                    .as_shape(rect, 0.),
                );
                let fill = if response.hovered() {
                    Color32::from_rgb(115, 127, 157)
                } else {
                    theme::LIGHT
                };
                painter.rect_filled(rect, 0., fill);
                for (inset, alpha) in [14, 3].into_iter().enumerate() {
                    painter.rect_stroke(
                        rect.shrink(inset as f32),
                        0.,
                        Stroke::new(1_f32, Color32::from_white_alpha(alpha)),
                        StrokeKind::Inside,
                    );
                }
                painter.rect_filled(
                    Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.), rect.max),
                    0.,
                    Color32::from_black_alpha(77),
                );
            }
            egui::Image::new(image)
                .tint(Color32::WHITE.gamma_multiply(if toggle == Some(false) { 0.5 } else { 1. }))
                .paint_at(
                    ui,
                    Rect::from_center_size(rect.center(), Vec2::splat(22.65625)),
                );
            chrome::source_tooltip(&response, tip, keys, false);
            response.clicked()
        };
        if button(
            0,
            egui::include_image!("../../assets/icons/random.svg"),
            Some(self.random_tiles),
            "Enable random mode for current selection of tiles",
            &["R"][..],
        ) {
            self.random_tiles = !self.random_tiles;
        }
        if button(
            1,
            egui::include_image!("../../assets/icons/save.svg"),
            None,
            "Memorize current selection of tiles",
            &["S", "SHIFT"][..],
        ) {
            self.tile_selection_memory(ctx, false);
        }
        if button(
            2,
            egui::include_image!("../../assets/icons/fit.svg"),
            Some(*fit),
            "Fit tileset view in the interface panel",
            &[][..],
        ) {
            *fit = !*fit;
        }
        let pending_id = tileset_warning_id();
        if target != uid {
            if incompatible && choices.iter().any(|c| c.0 == target && c.2) {
                ctx.data_mut(|d| d.insert_temp(pending_id, target));
            } else {
                self.apply_palette_tileset(target, false);
            }
        }
        if let Some(target) = ctx.data(|d| d.get_temp::<i64>(pending_id)) {
            let modal = egui::Modal::new(pending_id).show(ctx, |ui| {
                ui.set_max_width(400.);
                ui.label(locale::tr(
                    ui,
                    "Warning: using this tileset in this layer will mess any existing tiles here.",
                ));
                let enter =
                    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
                ui.horizontal(|ui| {
                    let confirm = panels::source_controls::button(ui, "Confirm").clicked() || enter;
                    let cancel = panels::source_controls::button(ui, "Cancel").clicked();
                    (confirm, cancel)
                })
                .inner
            });
            let (confirm, cancel) = modal.inner;
            if confirm {
                self.apply_palette_tileset(target, true);
            }
            if confirm || cancel || modal.should_close() {
                ctx.data_mut(|d| d.remove::<i64>(pending_id));
                ctx.request_repaint();
            }
        }
    }
    fn apply_palette_tileset(&mut self, uid: i64, recoverable: bool) {
        let Some(key) = self.selected else {
            return;
        };
        let layer = self.layer;
        let operation = |p: &mut Project| {
            p.set_layer_tileset_override(
                key,
                layer,
                Some(uid),
                ldtk_core::TilesetRemap::TileIds,
                true,
            )
            .map(|_| ())
        };
        if recoverable {
            self.edit_level_recoverable("Changed layer tileset", key, operation);
        } else {
            self.edit_level("Layer tileset changed", key, operation);
        }
    }
    pub(super) fn tile_palette(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, uid: i64) {
        let Some(project) = &self.project else {
            return;
        };
        let iid = project.json["iid"].as_str().unwrap_or("").to_owned();
        let fit_name = format!("{iid}_tileset_{uid}_fit");
        let mut fit = self.settings.value["uiStates"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["id"] == fit_name)
            .and_then(|s| s["val"].as_bool())
            .unwrap_or(false);
        let previous_fit = fit;
        self.tile_palette_header(ctx, ui, uid, &mut fit);
        if previous_fit != fit {
            if !self.settings.value["uiStates"].is_array() {
                self.settings.value["uiStates"] = json!([]);
            }
            let states = self.settings.value["uiStates"].as_array_mut().unwrap();
            states.retain(|s| s["id"] != fit_name);
            if fit {
                states.push(json!({"id":fit_name,"val":true}));
            }
            let _ = self.settings.save();
        }
        let height = ui
            .available_height()
            .min(ui.clip_rect().bottom() - ui.cursor().top())
            .max(64.);
        let (placeholder, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
        let id = egui::Id::new(("tile-palette-popout", &iid, uid));
        let area_id = id.with("content");
        let mut state = ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap_or_default();
        let frame = ctx.cumulative_frame_nr();
        if state.last_frame + 1 < frame {
            state = PopOut::default();
        }
        state.last_frame = frame;
        let hover = ctx.pointer_hover_pos().is_some_and(|p| {
            placeholder.contains(p)
                && ctx
                    .layer_id_at(p)
                    .is_none_or(|layer| layer == ui.layer_id() || layer.id == area_id)
        });
        if fit {
            state.open = false;
        } else if !state.open
            && hover
            && !state.was_hovered
            && !self.stroke
            && self.gesture.is_none()
            && !self.is_busy()
        {
            state.open = true;
            state.last = None;
            state.leaving = 0.;
            state.need_closing = false;
            state.dragging = false;
        }
        state.was_hovered = hover;
        let width = if state.open {
            (ctx.content_rect().width() * 0.45).max(500.)
        } else {
            placeholder.width()
        };
        let bounds = Rect::from_min_size(placeholder.min, egui::vec2(width, height));
        let render_mask = state.open || state.mask_capture;
        if state.open {
            let (events, time) = ctx.input(|i| (i.events.clone(), i.time));
            state.update(&events, bounds, placeholder.right(), time);
            if state.need_closing && state.open {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    (state.suspend_until - time).max(0.),
                ));
            }
        }
        if render_mask {
            // Modal.hx's transparent mask consumes the complete outside mouse gesture.
            // Keep it through release: canvas brushes also accept held buttons.
            if ctx.input(|i|i.events.iter().any(|e|matches!(e,egui::Event::PointerButton{pos,pressed:true,..} if !bounds.contains(*pos)))) {
                state.open=false;state.mask_capture=true;
            }
            let screen = ctx.content_rect();
            egui::Area::new(id.with("mask"))
                .order(egui::Order::Foreground)
                .fixed_pos(screen.min)
                .constrain(false)
                .sense(Sense::click_and_drag())
                .show(ctx, |ui| {
                    ui.allocate_rect(screen, Sense::click_and_drag());
                });
            if !ctx.input(|i| i.pointer.any_down()) {
                state.mask_capture = false;
            }
            if state.open {
                ctx.move_to_top(egui::LayerId::new(egui::Order::Foreground, area_id));
            }
        }
        // Keep one Area identity in both states, preserving the picker's drag/zoom/scroll IDs.
        let width = if state.open {
            width
        } else {
            placeholder.width()
        };
        let bounds = Rect::from_min_size(placeholder.min, egui::vec2(width, height));
        let clip = if state.open {
            bounds.intersect(ctx.content_rect())
        } else {
            bounds.intersect(ui.clip_rect())
        };
        egui::Area::new(area_id)
            .order(if state.open {
                egui::Order::Foreground
            } else {
                egui::Order::Middle
            })
            .fixed_pos(bounds.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_min_size(bounds.size());
                ui.set_max_size(bounds.size());
                if state.open {
                    ui.painter().with_clip_rect(ctx.content_rect()).add(
                        egui::epaint::Shadow {
                            offset: [0, 0],
                            blur: 32,
                            spread: 0,
                            color: Color32::BLACK,
                        }
                        .as_shape(bounds, 0),
                    );
                }
                ui.set_clip_rect(clip);
                ui.painter().rect_filled(bounds, 0., theme::DARK);
                if state.open {
                    ui.painter().rect_stroke(
                        bounds,
                        0.,
                        Stroke::new(1_f32, Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
                ctx.data_mut(|d| {
                    d.insert_temp(
                        view_id(),
                        CanvasAtlasView {
                            fit,
                            sidebar_size: placeholder.size(),
                        },
                    )
                });
                self.atlas_picker(ctx, ui, uid);
                ctx.data_mut(|d| d.remove::<CanvasAtlasView>(view_id()));
            });
        ctx.data_mut(|d| {
            if state.open {
                d.insert_temp(active_id(), (id, frame));
            } else if d
                .get_temp::<(egui::Id, u64)>(active_id())
                .is_some_and(|(active, _)| active == id)
            {
                d.remove::<(egui::Id, u64)>(active_id());
            }
            d.insert_temp(id, state);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bounds() -> Rect {
        Rect::from_min_max(egui::pos2(0., 100.), egui::pos2(500., 500.))
    }
    fn state() -> PopOut {
        PopOut {
            open: true,
            last: Some(egui::pos2(490., 300.)),
            ..Default::default()
        }
    }
    #[test]
    fn single_row_palette_header_toggles_random_saves_sparse_selection_and_fits() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut p = Project::new();
        let layer = p.add_layer("Art", "Tiles", 16).unwrap();
        let uid = p
            .add_tileset(std::path::Path::new("TopDown_by_deepnight.png"), 16, 64, 32)
            .unwrap();
        p.assign_tileset(layer, uid).unwrap();
        let key = p.level_keys()[0];
        p.paint_tile(key, 0, 0, 0, 0, [0, 0], 0, false).unwrap();
        let other = p
            .add_tileset(std::path::Path::new("Alternate.png"), 16, 128, 32)
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.set_atlas_selection(uid, vec![0, 2]);
        let mut fit = false;
        let bounds = std::cell::Cell::new(Rect::NOTHING);
        let run = |app: &mut EditorApp, fit: &mut bool, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let start = ui.cursor().min;
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(Rect::from_min_size(start, egui::vec2(300., 100.))),
                        );
                        app.tile_palette_header(ctx, &mut child, uid, fit);
                        bounds.set(child.min_rect());
                    });
                },
            )
        };
        let initial = run(&mut app, &mut fit, vec![]);
        assert!(
            initial.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text().starts_with("TopDown_by_deepnight"))
            }),
            "the closed selector must leave room for the complete atlas name"
        );
        assert!(
            bounds.get().height() <= 30.,
            "one28pt row: {:?}",
            bounds.get()
        );
        assert!(
            bounds.get().width() <= 300.,
            "long tileset name must not widen sidebar: {:?}",
            bounds.get()
        );
        let y = bounds.get().top() + 14.;
        //194pt selector,2pt gap, then three32pt icon buttons with2pt gaps.
        for (x, action) in [(220., 0), (254., 1), (288., 2)] {
            for pressed in [true, false] {
                run(
                    &mut app,
                    &mut fit,
                    vec![
                        egui::Event::PointerMoved(egui::pos2(x, y)),
                        egui::Event::PointerButton {
                            pos: egui::pos2(x, y),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            match action {
                0 => assert!(app.random_tiles),
                1 => assert_eq!(
                    app.project.as_ref().unwrap().tileset(uid).unwrap()["savedSelections"][0],
                    json!({"ids":[0,2],"mode":"Random"})
                ),
                _ => assert!(fit),
            }
        }
        assert_eq!(app.selected_tile_ids_for(uid), vec![0, 2]);
        let selector = egui::pos2(100., y);
        let open = |app: &mut EditorApp, fit: &mut bool| {
            for pressed in [true, false] {
                run(
                    app,
                    fit,
                    vec![
                        egui::Event::PointerMoved(selector),
                        egui::Event::PointerButton {
                            pos: selector,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
        };
        let press = |app: &mut EditorApp, fit: &mut bool, key| {
            for pressed in [true, false] {
                run(
                    app,
                    fit,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
            }
        };
        open(&mut app, &mut fit);
        press(&mut app, &mut fit, Key::End);
        press(&mut app, &mut fit, Key::Enter);
        let warning = tileset_warning_id();
        assert_eq!(ctx.data(|data| data.get_temp::<i64>(warning)), Some(other));
        press(&mut app, &mut fit, Key::Escape);
        assert_eq!(ctx.data(|data| data.get_temp::<i64>(warning)), None);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["__tilesetDefUid"],
            uid
        );
        open(&mut app, &mut fit);
        press(&mut app, &mut fit, Key::End);
        press(&mut app, &mut fit, Key::Enter);
        assert_eq!(ctx.data(|data| data.get_temp::<i64>(warning)), Some(other));
        press(&mut app, &mut fit, Key::Enter);
        assert_eq!(ctx.data(|data| data.get_temp::<i64>(warning)), None);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 0).unwrap()["__tilesetDefUid"],
            other
        );
    }
    #[test]
    fn outward_travel_closes_at_thirty_pixels_but_return_travel_reduces_debt() {
        let mut s = state();
        s.move_pointer(egui::pos2(510., 300.), bounds(), 250., 0.);
        assert!(s.open);
        assert_eq!(s.leaving, 20.);
        s.move_pointer(egui::pos2(505., 300.), bounds(), 250., 0.);
        assert_eq!(s.leaving, 5.);
        s.move_pointer(egui::pos2(529., 300.), bounds(), 250., 0.);
        assert!(s.open);
        s.move_pointer(egui::pos2(530., 300.), bounds(), 250., 0.);
        assert!(!s.open);
    }
    #[test]
    fn drag_holds_open_and_release_defers_close_for_two_tenths() {
        let mut s = state();
        s.dragging = true;
        s.move_pointer(egui::pos2(700., 300.), bounds(), 250., 0.);
        assert!(s.open);
        assert_eq!(s.leaving, 0.);
        s.update(
            &[egui::Event::PointerButton {
                pos: egui::pos2(700., 300.),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            bounds(),
            250.,
            1.,
        );
        s.move_pointer(egui::pos2(740., 300.), bounds(), 250., 1.1);
        assert!(s.open);
        assert!(s.need_closing);
        s.update(&[], bounds(), 250., 1.21);
        assert!(!s.open);
    }
    #[test]
    fn moving_above_sidebar_closes_immediately_and_approaching_popup_keeps_open() {
        let mut s = state();
        s.move_pointer(egui::pos2(100., 99.), bounds(), 250., 0.);
        assert!(!s.open);
        let mut s = state();
        s.last = Some(egui::pos2(700., 300.));
        s.move_pointer(egui::pos2(680., 300.), bounds(), 250., 0.);
        assert!(s.open);
        assert_eq!(s.leaving, 0.);
    }
    fn editor() -> (EditorApp, egui::Context, egui::Id, egui::TextureId) {
        let ctx = egui::Context::default();
        let mut app = EditorApp::empty();
        let image = ldtk_core::exports::embedded_atlas().unwrap();
        let mut p = Project::new();
        p.json["defs"]["tilesets"] = json!([{"uid":10,"identifier":"Icons","embedAtlas":"LdtkIcons","relPath":null,"tileGridSize":16,"pxWid":image.width(),"pxHei":image.height(),"__cWid":image.width()/16,"__cHei":image.height()/16}]);
        let id = egui::Id::new((
            "tile-palette-popout",
            p.json["iid"].as_str().unwrap(),
            10_i64,
        ));
        app.set_project(p);
        let texture = app.assets.embedded(&ctx).unwrap().id();
        (app, ctx, id, texture)
    }
    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                app.shortcuts(ctx);
                egui::SidePanel::left("test-sidebar")
                    .exact_width(250.)
                    .show(ctx, |ui| {
                        ui.add_space(100.);
                        app.tile_palette(ctx, ui, 10);
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                    app.canvas_input(ctx, rect, &response);
                });
            },
        )
    }
    fn atlas(output: &egui::FullOutput, texture: egui::TextureId) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::epaint::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn keyboard_navigation_scrolls_to_saved_and_regular_tiles_and_snaps_on_wrap() {
        let (mut app, ctx, area_id, _) = editor();
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut project = app.project.take().unwrap();
        let layer = project.add_layer("Ground", "Tiles", 16).unwrap();
        project.assign_tileset(layer, 10).unwrap();
        project.update_tileset_definition(
            10,
            &json!({"savedSelections":[{"ids":[0,1],"mode":"Stamp"},{"ids":[2,3],"mode":"Random"}]}),
        ).unwrap();
        app.set_project(project);
        let texture = app.assets.embedded(&ctx).unwrap().id();
        app.world = false;
        app.settings.value["navigationKeys"] = json!("Wasd");
        assert!(app.set_atlas_selection(10, vec![0, 1]));
        frame(&mut app, &ctx, vec![]);
        let initial = atlas(&frame(&mut app, &ctx, vec![]), texture);
        let anchor = ctx
            .read_response(area_id.with("content"))
            .unwrap()
            .rect
            .center()
            .x;
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let right = key(Key::D, true);
        let recalling = atlas(&frame(&mut app, &ctx, vec![right.clone()]), texture);
        assert_eq!(app.selected_tile_ids_for(10), [2, 3]);
        assert!(app.random_tiles);
        let recalled = anchor - 144.;
        assert!((recalled..initial.left()).contains(&recalling.left()));
        frame(&mut app, &ctx, vec![key(Key::D, false)]);
        for _ in 0..20 {
            frame(&mut app, &ctx, vec![]);
        }
        let settled = atlas(&frame(&mut app, &ctx, vec![]), texture);
        assert!((settled.left() - recalled).abs() <= 1.);

        let moving = atlas(&frame(&mut app, &ctx, vec![right]), texture);
        assert_eq!(app.selected_tile_ids_for(10), [4]);
        assert!((anchor - 216. ..settled.left()).contains(&moving.left()));

        assert!(app.set_atlas_selection(10, vec![0, 1]));
        let wrapped = atlas(&frame(&mut app, &ctx, vec![key(Key::A, true)]), texture);
        assert_eq!(app.selected_tile_ids_for(10), [30]);
        assert!((wrapped.left() + 30.5 * 48. - anchor).abs() <= 1.);
    }

    #[test]
    fn atlas_shows_saved_group_on_hover_and_merges_only_stamp_borders() {
        let (mut app, ctx, id, texture) = editor();
        ctx.style_mut(|style| style.animation_time = 0.);
        app.project
            .as_mut()
            .unwrap()
            .update_tileset_definition(
                10,
                &json!({"savedSelections":[{"ids":[2,3],"mode":"Random"}]}),
            )
            .unwrap();
        assert!(app.set_atlas_selection(10, vec![0, 1]));
        frame(&mut app, &ctx, vec![]);
        let initial = frame(&mut app, &ctx, vec![]);
        let origin = atlas(&initial, texture).min;
        let area = ctx.read_response(id.with("content")).unwrap().rect;
        let brush_center = origin + egui::vec2(48., 24.);
        assert!(
            (brush_center.x - area.center().x).abs() <= 1.,
            "initial brush: {brush_center:?}, viewport: {area:?}"
        );
        assert!((brush_center.y - (area.top() + area.height() * 0.25)).abs() <= 1.);
        let selected_edge = |output: &egui::FullOutput, x: f32, color| {
            output.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, stroke } => {
                    stroke.color == color && points.iter().all(|p| (p.x - x).abs() <= 2.)
                }
                _ => false,
            })
        };
        let stamp = Color32::from_rgb(243, 228, 164);
        assert!(selected_edge(&initial, origin.x, stamp));
        assert!(selected_edge(&initial, origin.x + 96., stamp));
        assert!(!selected_edge(&initial, origin.x + 48., stamp));
        let shared_edge = origin.x + 48.;
        let outline_joins = initial.shapes.iter().filter(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, stroke } => {
                stroke.color == stamp
                    && points[0].y == points[1].y
                    && points.iter().any(|p| (p.x - shared_edge).abs() <= 0.01)
            }
            _ => false,
        });
        assert_eq!(
            outline_joins.count(),
            4,
            "continuous outline across the stamp"
        );

        let point = origin + egui::vec2(120., 24.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(point)]);
        let hover = frame(&mut app, &ctx, vec![]);
        assert!(
            hover.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Circle(dot) => {
                    dot.fill == Color32::from_rgba_unmultiplied(144, 238, 144, 128)
                        && dot.center.x >= origin.x + 144.
                }
                _ => false,
            }),
            "the other tile in the saved group is previewed"
        );
        assert_eq!(app.selected_tile_ids_for(10), vec![0, 1]);
        assert!(!app.random_tiles);

        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        let random = frame(&mut app, &ctx, vec![]);
        assert_eq!(app.selected_tile_ids_for(10), vec![2, 3]);
        assert!(app.random_tiles);
        assert!(selected_edge(
            &random,
            origin.x + 144.,
            Color32::from_rgb(144, 238, 144)
        ));
    }

    #[test]
    fn canvas_palette_wheel_zooms_without_adding_a_toolbar_row() {
        let (mut app, ctx, _id, texture) = editor();
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(100., 200.))],
        );
        let before = frame(&mut app, &ctx, vec![]);
        assert!(!before.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text=="Zoom" || t.galley.job.text=="Fit tileset view")));
        let original = atlas(&before, texture);
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., 100.),
                modifiers: Default::default(),
            }],
        );
        let zoomed = atlas(&output, texture);
        assert!(
            (zoomed.width() / original.width() - 1.1).abs() < 0.001,
            "source wheel scaling: {original:?} -> {zoomed:?}"
        );
        assert_eq!(app.flips, 0);
    }
    #[test]
    fn actual_hover_expands_and_drag_selects_without_losing_picker_state_then_escape_closes() {
        let (mut a, ctx, id, texture) = editor();
        frame(&mut a, &ctx, vec![]);
        let output = frame(&mut a, &ctx, vec![]);
        let initial = atlas(&output, texture);
        let point = initial.min + egui::vec2(24., 24.);
        frame(&mut a, &ctx, vec![egui::Event::PointerMoved(point)]);
        assert!(ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
        let output = frame(&mut a, &ctx, vec![]);
        let image = atlas(&output, texture);
        assert_eq!(initial.size(), image.size(), "opening must preserve zoom");
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        let start = image.min + egui::vec2(24., 24.);
        frame(
            &mut a,
            &ctx,
            vec![egui::Event::PointerMoved(start), press(start, true)],
        );
        frame(
            &mut a,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(700., start.y))],
        );
        assert!(ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
        let end = image.min + egui::vec2(72., 24.);
        frame(
            &mut a,
            &ctx,
            vec![egui::Event::PointerMoved(end), press(end, false)],
        );
        assert_eq!(a.selected_tile_ids_for(10), vec![0, 1]);
        frame(
            &mut a,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
        frame(&mut a, &ctx, vec![]);
        assert!(!ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
    }
    #[test]
    fn fitted_or_running_tool_never_auto_expands_and_fit_uses_original_boolean_state() {
        for fitted in [true, false] {
            let (mut a, ctx, id, texture) = editor();
            if fitted {
                let iid = a.project.as_ref().unwrap().json["iid"].as_str().unwrap();
                a.settings.value["uiStates"] =
                    json!([{"id":format!("{iid}_tileset_10_fit"),"val":true}]);
            } else {
                a.stroke = true;
            }
            frame(&mut a, &ctx, vec![]);
            let output = frame(&mut a, &ctx, vec![]);
            let image = atlas(&output, texture);
            frame(
                &mut a,
                &ctx,
                vec![egui::Event::PointerMoved(image.min + egui::vec2(10., 10.))],
            );
            assert!(!ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
            if fitted {
                assert!(image.width() <= 250.);
            }
        }
    }
    #[test]
    fn outside_mask_click_closes_without_starting_or_continuing_a_canvas_stroke() {
        let (mut app, ctx, id, texture) = editor();
        app.project
            .as_mut()
            .unwrap()
            .add_layer("Ground", "IntGrid", 16)
            .unwrap();
        app.normalize_palette();
        app.world = false;
        app.fit = false;
        app.viewport.center = [128., 128.];
        app.viewport.zoom = 1.;
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let image = atlas(&output, texture);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(image.min + egui::vec2(10., 10.))],
        );
        frame(&mut app, &ctx, vec![]);
        let area = ctx.read_response(id.with("content")).unwrap().rect;
        let edge = egui::pos2(area.right() - 2., 400.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(edge)]);
        let outside = edge + egui::vec2(8., 0.);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(outside)]);
        assert!(ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut app, &ctx, vec![button(outside, true)]);
        assert!(!ctx.data(|d| d.get_temp::<PopOut>(id)).unwrap().open);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(outside + egui::vec2(20., 0.))],
        );
        frame(&mut app, &ctx, vec![button(outside, false)]);
        assert!(!app.stroke);
        assert!(!app.dirty);
        let csv = &app
            .project
            .as_ref()
            .unwrap()
            .layer(
                LevelKey {
                    world: None,
                    level: 0,
                },
                0,
            )
            .unwrap()["intGridCsv"];
        assert!(csv.as_array().unwrap().iter().all(|v| v == 0));
        // A later independent click reaches the canvas after the swallowed gesture ends.
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![button(outside, true)]);
        frame(&mut app, &ctx, vec![button(outside, false)]);
        assert!(app.dirty);
    }
}
