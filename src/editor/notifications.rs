use super::*;

#[derive(PartialEq, Eq)]
pub(super) enum CanvasNotice {
    ResizeRemovals(usize),
    EntityLimit { maximum: i64, identifier: String },
}

impl EditorApp {
    pub(super) fn report_resize_removals(&mut self, removed: usize) {
        if removed > 0 {
            self.report_canvas_notice(CanvasNotice::ResizeRemovals(removed));
        }
    }

    pub(super) fn report_entity_limit(&mut self, maximum: i64, identifier: String) {
        self.report_canvas_notice(CanvasNotice::EntityLimit { maximum, identifier });
    }

    fn report_canvas_notice(&mut self, notice: CanvasNotice) {
        if !self.canvas_notice.as_ref().is_some_and(|(last, since)| {
            *last == notice && since.elapsed().as_secs_f32() <= 0.7
        }) {
            self.canvas_notice = Some((notice, Instant::now()));
        }
    }

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
                .order(egui::Order::Tooltip)
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
        if let Some((notice, since)) = &self.canvas_notice {
            let (message, fill, edge) = match notice {
                CanvasNotice::ResizeRemovals(removed) => (
                    locale::format_ctx(
                        ctx,
                        "{count} entity(ies) deleted during resizing!",
                        &[("count", &removed.to_string())],
                    ),
                    [69, 48, 6],
                    [203, 141, 19],
                ),
                CanvasNotice::EntityLimit { maximum, identifier } => (
                    locale::format_ctx(
                        ctx,
                        "You cannot have more than {count} {entity}.",
                        &[("count", &maximum.to_string()), ("entity", identifier)],
                    ),
                    [87, 0, 0],
                    [255, 0, 0],
                ),
            };
            let duration = 3. + 0.04 * message.chars().count() as f32;
            let age = since.elapsed().as_secs_f32();
            let remaining = duration - age;
            if remaining <= 0. {
                self.canvas_notice = None;
            } else {
                // The original warning flashes after 250 ms and returns to its normal color.
                let brightness = [(0.25, 7.), (0.4, 3.), (0.85, 1.5), (1.75, 1.)]
                    .windows(2)
                    .find_map(|points| {
                        let [(start, from), (end, to)] = [points[0], points[1]];
                        (start..end).contains(&age).then(|| {
                            let progress = (age - start) / (end - start);
                            to + (from - to) * panels::source_controls::css_ease_remaining(progress)
                        })
                    })
                    .unwrap_or(1.);
                let shade = |channels: [u8; 3]| {
                    let [r, g, b] = channels.map(|v| (f32::from(v) * brightness).round() as u8);
                    Color32::from_rgb(r, g, b)
                };
                let border = shade(edge);
                let screen = ctx.content_rect();
                egui::Area::new(egui::Id::new("canvas-notice"))
                    .anchor(Align2::RIGHT_BOTTOM, egui::vec2(0., -screen.height() * 0.1 - 38.))
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .fade_in(false)
                    .show(ctx, |ui| {
                        let notice = egui::Frame::new()
                            .fill(shade(fill))
                            .stroke(Stroke::new(1., border))
                            .corner_radius(egui::CornerRadius {
                                nw: 3,
                                sw: 3,
                                ..Default::default()
                            })
                            .shadow(egui::epaint::Shadow {
                                offset: [0, 4],
                                blur: 6,
                                spread: 0,
                                color: Color32::from_black_alpha(180),
                            })
                            .inner_margin(egui::Margin {
                                left: 20,
                                right: 47,
                                top: 6,
                                bottom: 6,
                            })
                            .show(ui, |ui| {
                                ui.set_min_width(81.);
                                ui.set_max_width((screen.width() - 70.).max(81.));
                                ui.label(RichText::new(message).size(14.67).color(Color32::WHITE));
                            });
                        let rect = notice.response.rect;
                        ui.painter().rect_filled(
                            Rect::from_min_max(egui::pos2(rect.right() - 8., rect.top()), rect.max),
                            0.,
                            border,
                        );
                    });
                ctx.request_repaint_after(std::time::Duration::from_secs_f32(if age < 1.75 {
                    0.016
                } else {
                    remaining
                }));
            }
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
            && !(matches!(self.canvas_notice, Some((CanvasNotice::ResizeRemovals(_), _)))
                && self.status == "Level resized")
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
