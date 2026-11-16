use super::*;

pub(super) fn normalize_platform_pointer(ctx: &egui::Context, input: &mut egui::RawInput) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let id = egui::Id::new("mac-control-secondary-click");
    let mut active = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    for event in &mut input.events {
        match event {
            egui::Event::PointerButton {
                button,
                pressed,
                modifiers,
                ..
            } if *button == egui::PointerButton::Primary => {
                if *pressed {
                    active = modifiers.ctrl && !modifiers.mac_cmd;
                }
                if active {
                    *button = egui::PointerButton::Secondary;
                }
                if !*pressed {
                    active = false;
                }
            }
            egui::Event::PointerGone | egui::Event::WindowFocused(false) => active = false,
            _ => {}
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, active));
}

impl EditorApp {
    pub(super) fn focus_overlay(&mut self, ctx: &egui::Context) {
        if !self.clicktrap {
            return;
        }
        if let Some(frames) = &mut self.focus_frames {
            *frames = frames.saturating_sub(1);
            if *frames == 0 {
                self.clicktrap = false;
                self.focus_frames = None;
                ctx.request_repaint();
                return;
            }
            ctx.request_repaint();
        }
        let rect = ctx.content_rect();
        egui::Area::new(egui::Id::new("source-focus-clicktrap"))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                if self.settings.bool("blurMask") {
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        Color32::from_rgba_unmultiplied(30, 34, 41, 102),
                    );
                    if let Some(texture) = self.assets.builtin(
                        ctx,
                        "focus-stripes",
                        include_bytes!("../../assets/img/stripesLight.png"),
                    ) {
                        let size = texture.size_vec2();
                        let mut mesh = egui::Mesh::with_texture(texture.id());
                        let mut y = rect.top();
                        while y < rect.bottom() {
                            let mut x = rect.left();
                            while x < rect.right() {
                                mesh.add_rect_with_uv(
                                    Rect::from_min_size(egui::pos2(x, y), size),
                                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                x += size.x;
                            }
                            y += size.y;
                        }
                        ui.painter()
                            .with_clip_rect(rect)
                            .add(egui::Shape::mesh(mesh));
                    }
                }
                if response.clicked() || ctx.input(|i| i.pointer.any_released()) {
                    self.clicktrap = false;
                    ctx.request_repaint();
                }
            });
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn control_click_maps_both_halves_even_if_control_is_released_first() {
        let ctx = egui::Context::default();
        let event = |pressed, ctrl| egui::Event::PointerButton {
            pos: egui::pos2(10., 10.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers {
                ctrl,
                ..Default::default()
            },
        };
        let mut press = egui::RawInput {
            events: vec![event(true, true)],
            ..Default::default()
        };
        normalize_platform_pointer(&ctx, &mut press);
        assert!(matches!(
            press.events[0],
            egui::Event::PointerButton {
                button: egui::PointerButton::Secondary,
                ..
            }
        ));
        let mut release = egui::RawInput {
            events: vec![event(false, false)],
            ..Default::default()
        };
        normalize_platform_pointer(&ctx, &mut release);
        assert!(matches!(
            release.events[0],
            egui::Event::PointerButton {
                button: egui::PointerButton::Secondary,
                ..
            }
        ));
        let mut plain = egui::RawInput {
            events: vec![event(true, false), event(false, false)],
            ..Default::default()
        };
        normalize_platform_pointer(&ctx, &mut plain);
        assert!(plain.events.iter().all(|e| matches!(
            e,
            egui::Event::PointerButton {
                button: egui::PointerButton::Primary,
                ..
            }
        )));
    }
}
