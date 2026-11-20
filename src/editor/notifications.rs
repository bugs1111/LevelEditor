use super::*;

impl EditorApp {
    pub(super) fn notify_world_view(&mut self) {
        self.quick_notification = Some((
            "World view".into(),
            Some(egui::include_image!("../../assets/icons/world.svg")),
            Instant::now(),
        ));
    }

    pub(super) fn canvas_notifications(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.pending_auto_layer_rebuild {
            let viewport = ctx.content_rect();
            egui::Area::new(egui::Id::new("rebuild-auto-layers"))
                .fixed_pos(egui::pos2(viewport.left() + viewport.width() * 0.4 + 2., viewport.top() + 4.))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let layers_open = self.panel == Some(Panel::Layers);
                    let icon = egui::Image::new(egui::include_image!("../../assets/icons/autoLayer.svg"))
                        .fit_to_exact_size(Vec2::splat(18.));
                    let label = locale::tr(ui, "REBUILD ALL AUTO-LAYERS");
                    let button = panels::source_controls::add_sized_button(ui, egui::vec2(208., 30.),
                        egui::Button::image_and_text(icon, RichText::new(label).size(12.))
                            .fill(if layers_open {
                                Color32::from_rgb(136, 47, 0)
                            } else {
                                Color32::from_rgb(227, 77, 0)
                            }));
                    if layers_open {
                        if button.clicked() {
                            self.show_panel(None);
                        }
                    } else if button.on_hover_text(locale::tr(ui, "All project auto-layers need to be updated to adapt to your latest changes.")).clicked() {
                        self.start_auto_layer_rebuild(ctx);
                    }
                });
        }
        if let Some((name, icon, since)) = &self.quick_notification {
            let elapsed = since.elapsed().as_secs_f32();
            if elapsed >= 1.2 {
                self.quick_notification = None;
            } else if self.project.is_some() {
                let opacity = 0.5 + 0.5 * (std::f32::consts::PI * elapsed / 1.2).cos();
                egui::Area::new(egui::Id::new("quick-notification"))
                    .fixed_pos(egui::pos2(rect.left() + 125., rect.top() + 20.))
                    .order(egui::Order::Foreground)
                    .interactable(false)
                    .fade_in(false)
                    .show(ctx, |ui| {
                        ui.set_opacity(opacity);
                        egui::Frame::new()
                            .fill(Color32::from_black_alpha(204))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(16, 4))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = 8.;
                                ui.horizontal(|ui| {
                                    if let Some(icon) = icon {
                                        ui.add(egui::Image::new(icon.clone()).fit_to_exact_size(Vec2::splat(50.)));
                                    }
                                    ui.label(
                                        RichText::new(name)
                                            .font(FontId::new(
                                                40.,
                                                egui::FontFamily::Name("panel-title".into()),
                                            ))
                                            .color(theme::ACCENT),
                                    );
                                });
                            });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        if self
            .last_chance
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed().as_secs_f32() > 20.0)
        {
            self.last_chance = None;
        }
        if let Some((label, _)) = &self.last_chance {
            let mut restore = false;
            egui::Area::new(egui::Id::new("last-chance"))
                .anchor(Align2::RIGHT_TOP, egui::vec2(0., 40.))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let right = if rect.width() >= 426. { 80 } else { 20 };
                    let text_width = (rect.width() - f32::from(right) - 96.).clamp(100., 250.);
                    let notice = egui::Frame::new()
                        .fill(Color32::from_rgb(121, 17, 5))
                        .inner_margin(egui::Margin { left: 25, right, top: 20, bottom: 20 })
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 8], blur: 16, spread: 0, color: Color32::from_black_alpha(77)
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(10., 6.);
                            ui.horizontal(|ui| {
                                restore = project_dialogs::button(
                                    ui, "UNDO", egui::vec2(62., 69.), false, 13.333
                                ).clicked();
                                ui.vertical(|ui| {
                                    ui.set_width(text_width);
                                    ui.label(RichText::new(locale::tr(ui, label)).size(14.5).strong().color(theme::ACCENT));
                                    ui.label(RichText::new(locale::tr(ui, "This change cannot be undone, so this is your last chance. Please make sure you really wanted to do that :)"))
                                        .size(12.).italics().color(Color32::from_white_alpha(179)));
                                });
                            });
                        });
                    ui.painter().rect_filled(
                        Rect::from_min_size(notice.response.rect.min, egui::vec2(4., notice.response.rect.height())),
                        0.,
                        Color32::from_rgb(227, 77, 0),
                    );
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            if restore {
                self.restore_last_chance();
            }
        }
        if let Some(project) = &self.project
            && let Some(desc) = project.json["tutorialDesc"]
                .as_str()
                .filter(|s| !s.is_empty())
        {
            egui::Area::new(egui::Id::new("tutorial-description"))
                .fixed_pos(rect.left_top())
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(92, 168, 105))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(rect.width() - 24.0);
                            ui.set_min_height(64.0);
                            ui.horizontal(|ui| {
                                let width = (rect.width() - 64.0).min(700.0);
                                ui.add_space(((rect.width() - width) / 2.0 - 52.0).max(0.0));
                                ui.add(
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/help.svg"
                                    ))
                                    .fit_to_exact_size(Vec2::splat(40.0)),
                                );
                                ui.add_space(12.0);
                                ui.vertical(|ui| {
                                    ui.set_max_width(width - 64.0);
                                    for paragraph in desc.split("\n\n") {
                                        ui.label(
                                            RichText::new(paragraph)
                                                .size(14.67)
                                                .color(Color32::WHITE),
                                        );
                                        ui.add_space(8.0);
                                    }
                                });
                            });
                        });
                });
        }
        if self.project.is_some()
            && let Some(pointer) = ctx.pointer_hover_pos()
            && rect.contains(pointer)
        {
            let position =
                self.viewport
                    .to_world(pointer.into(), rect.min.into(), rect.size().into());
            let origin = self
                .selected
                .and_then(|key| {
                    self.project.as_ref().and_then(|p| {
                        canvas::placements(p, key)
                            .into_iter()
                            .find(|(k, _)| *k == key)
                            .map(|(_, b)| [b[0], b[1]])
                    })
                })
                .unwrap_or([0.0, 0.0]);
            let grid = self
                .selected
                .and_then(|k| self.project.as_ref()?.layer(k, self.layer))
                .map(|l| number(l, "__gridSize", 16.0))
                .unwrap_or(16.0);
            let local = if self.world {
                [position[0] - origin[0], position[1] - origin[1]]
            } else {
                position
            };
            let world = if self.world {
                position
            } else {
                [local[0] + origin[0], local[1] + origin[1]]
            };
            egui::Area::new(egui::Id::new("mouse-coordinates"))
                .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-4.0, -4.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        for (label, value, color) in [
                            (
                                "Grid",
                                [
                                    (local[0] / grid).floor() as i32,
                                    (local[1] / grid).floor() as i32,
                                ],
                                Color32::from_rgba_unmultiplied(60, 133, 110, 210),
                            ),
                            (
                                "Level",
                                [local[0].round() as i32, local[1].round() as i32],
                                Color32::from_rgba_unmultiplied(40, 105, 77, 210),
                            ),
                            (
                                "World",
                                [world[0].round() as i32, world[1].round() as i32],
                                Color32::from_rgba_unmultiplied(38, 42, 111, 210),
                            ),
                        ] {
                            egui::Frame::new()
                                .fill(color)
                                .corner_radius(9)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    let label = locale::tr(ui, label);
                                    ui.label(
                                        RichText::new(format!(
                                            "{label} = {},{}",
                                            value[0], value[1]
                                        ))
                                        .color(Color32::WHITE)
                                        .strong()
                                        .size(12.0),
                                    );
                                });
                        }
                    });
                });
        }
        let id = egui::Id::new("last-notification");
        let mut toast = ctx
            .data_mut(|d| d.get_temp::<(String, Instant)>(id))
            .unwrap_or_else(|| (self.status.clone(), Instant::now()));
        if toast.0 != self.status {
            toast = (self.status.clone(), Instant::now());
            ctx.data_mut(|d| d.insert_temp(id, toast.clone()));
        }
        if !self.is_busy()
            && !self.status.is_empty()
            && (self.status_error || toast.1.elapsed().as_secs_f32() < 4.0)
            && !self.status.starts_with("Project opened.")
            && !self.status.starts_with("Open a project")
        {
            egui::Area::new(egui::Id::new("notification"))
                .fixed_pos(rect.left_bottom() + egui::vec2(44.0, -48.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(if self.status_error {
                            Color32::from_rgb(120, 45, 30)
                        } else {
                            theme::MEDIUM
                        })
                        .corner_radius(3)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_max_width((rect.width() - 90.0).min(700.0));
                            ui.label(locale::tr(ui, &self.status));
                        });
                });
        }
    }
}
