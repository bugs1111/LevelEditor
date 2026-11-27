use super::*;
use crate::app::chrome::ActionTooltip as _;
use std::time::Duration;

pub(super) const SAVED_PROJECT_TITLE: &str = "Saved project";

pub(super) fn layer_notification(
    layer: &Value,
) -> (String, Option<egui::ImageSource<'static>>, Instant) {
    let icon = match layer["__type"].as_str() {
        Some("Entities") => egui::include_image!("../../assets/icons/entity.png"),
        Some("IntGrid") => egui::include_image!("../../assets/icons/intGrid.png"),
        Some("AutoLayer") => egui::include_image!("../../assets/icons/autoLayer.svg"),
        _ => egui::include_image!("../../assets/icons/tile.png"),
    };
    (
        text(layer, "__identifier", "").to_owned(),
        Some(icon),
        Instant::now(),
    )
}

#[derive(PartialEq, Eq)]
pub(super) enum CanvasNotice {
    ResizeRemovals(usize),
    OutOfBounds(String),
    EntityLimit { maximum: i64, identifier: String },
    SavedProject(String),
    SavedTileSelection,
    EnteredFullscreen,
    SavingFailed,
    CannotSaveBackup,
    CannotEditBackup,
    NothingToSelect,
    SymmetricRule,
    LinkOpened,
    LinkCopied,
    ClipboardCopied(String),
    ValueReset,
    ProjectFileSettingChanged,
    LocateMissingPath,
    LocatingFile,
}

pub(super) struct CanvasNoticeEntry {
    pub(super) notice: CanvasNotice,
    since: Instant,
    compacted: Option<Instant>,
}

impl CanvasNotice {
    fn message(&self, ctx: &egui::Context) -> String {
        match self {
            Self::ResizeRemovals(removed) => locale::format_ctx(
                ctx,
                "{count} entity(ies) deleted during resizing!",
                &[("count", &removed.to_string())],
            ),
            Self::OutOfBounds(names) => locale::format_ctx(
                ctx,
                "Out-of-bounds entity removed: {names}",
                &[("names", names)],
            ),
            Self::EntityLimit {
                maximum,
                identifier,
            } => locale::format_ctx(
                ctx,
                "You cannot have more than {count} {entity}.",
                &[("count", &maximum.to_string()), ("entity", identifier)],
            ),
            Self::SavedProject(_) => locale::tr_ctx(ctx, SAVED_PROJECT_TITLE).into(),
            Self::SavedTileSelection => locale::tr_ctx(ctx, "Saved selection").into(),
            Self::EnteredFullscreen => locale::tr_ctx(ctx, "Press F11 to leave fullscreen").into(),
            Self::SavingFailed => locale::tr_ctx(ctx, "Saving failed!").into(),
            Self::CannotSaveBackup => locale::tr_ctx(ctx, "Cannot save over a backup file.").into(),
            Self::CannotEditBackup => {
                locale::tr_ctx(ctx, "Backup files should not be edited directly").into()
            }
            Self::NothingToSelect => locale::tr_ctx(ctx, "Nothing to select").into(),
            Self::SymmetricRule => {
                locale::tr_ctx(ctx, "This option will have no effect on a symetric rule.").into()
            }
            Self::LinkOpened => locale::tr_ctx(ctx, "Opening url...").into(),
            Self::LinkCopied => locale::tr_ctx(ctx, "Copied to clipboard.").into(),
            Self::ClipboardCopied(name) => {
                locale::format_ctx(ctx, "Copied \"{name}\" to clipboard.", &[("name", name)])
            }
            Self::ValueReset => locale::tr_ctx(ctx, "Value reset.").into(),
            Self::ProjectFileSettingChanged => {
                locale::tr_ctx(ctx, "Project file setting changed").into()
            }
            Self::LocateMissingPath => {
                locale::tr_ctx(ctx, "Sorry, but this file couldn't be found.").into()
            }
            Self::LocatingFile => locale::tr_ctx(ctx, "Locating file...").into(),
        }
    }

    fn subtitle(&self, ctx: &egui::Context) -> Option<&str> {
        match self {
            Self::SavedProject(name) => Some(name),
            Self::ProjectFileSettingChanged => Some(locale::tr_ctx(
                ctx,
                "You should save the project at least once for this setting to apply its effects.",
            )),
            _ => None,
        }
    }

    fn duration(&self, ctx: &egui::Context, message: &str) -> f32 {
        let length = message.encode_utf16().count()
            + self
                .subtitle(ctx)
                .map_or(0, |name| name.encode_utf16().count());
        3. + 0.04 * length as f32
    }
}

impl EditorApp {
    pub(super) fn report_resize_removals(&mut self, removed: usize) {
        if removed > 0 {
            self.report_canvas_notice(CanvasNotice::ResizeRemovals(removed));
        }
    }

    pub(super) fn report_entity_limit(&mut self, maximum: i64, identifier: String) {
        self.report_canvas_notice(CanvasNotice::EntityLimit {
            maximum,
            identifier,
        });
    }

    pub(super) fn report_out_of_bounds(&mut self, names: &[String]) {
        if !names.is_empty() {
            self.report_canvas_notice(CanvasNotice::OutOfBounds(names.join(", ")));
        }
    }

    pub(super) fn report_saved_project(&mut self, name: String) {
        self.report_canvas_notice(CanvasNotice::SavedProject(name));
    }

    pub(super) fn report_saving_failed(&mut self) {
        self.report_canvas_notice(CanvasNotice::SavingFailed);
    }

    pub(super) fn report_saved_tile_selection(&mut self) {
        self.report_canvas_notice(CanvasNotice::SavedTileSelection);
    }

    pub(super) fn report_empty_selection(&mut self) {
        if let Some(ctx) = &self.repaint {
            app_log::record(
                ctx,
                self.settings.path.as_deref(),
                "Nothing to select",
                true,
            );
        }
        self.report_canvas_notice(CanvasNotice::NothingToSelect);
    }

    pub(super) fn notify_quick(&mut self, message: String) {
        self.quick_notification = Some((message, None, Instant::now()));
    }

    pub(super) fn recommend_saving_project(&mut self) {
        if !self.canvas_notices.iter().any(|entry| {
            entry.notice == CanvasNotice::ProjectFileSettingChanged
                && entry.since.elapsed().as_secs_f32() < 2.
        }) {
            self.report_canvas_notice(CanvasNotice::ProjectFileSettingChanged);
        }
    }

    pub(super) fn report_canvas_notice(&mut self, notice: CanvasNotice) {
        let now = Instant::now();
        if let Some(last) = self.canvas_notices.last_mut() {
            let repeated = last.notice == notice
                || matches!(
                    (&last.notice, &notice),
                    (CanvasNotice::SavedProject(_), CanvasNotice::SavedProject(_))
                );
            if repeated && now.duration_since(last.since).as_secs_f32() <= 0.7 {
                return;
            }
            last.compacted.get_or_insert(now);
        }
        self.canvas_notices.push(CanvasNoticeEntry {
            notice,
            since: now,
            compacted: None,
        });
    }

    fn paint_canvas_notices(&mut self, ctx: &egui::Context, now: Instant) {
        use panels::source_controls::css_ease_remaining;

        if self.canvas_notices.is_empty() {
            return;
        }
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("canvas-notices"),
        ));
        let mut bottom = screen.bottom() - screen.height() * 0.1 - 38.;
        let mut scheduled = f32::INFINITY;
        for entry in self.canvas_notices.iter().rev() {
            let message = entry.notice.message(ctx);
            let subtitle = entry.notice.subtitle(ctx);
            let age = now.duration_since(entry.since).as_secs_f32();
            let duration = entry.notice.duration(ctx, &message);
            if age >= duration + 0.1 {
                continue;
            }
            let compact_age = entry
                .compacted
                .map(|since| now.duration_since(since).as_secs_f32());
            let compact_remaining = match compact_age {
                Some(age) if age < 0.3 => css_ease_remaining(age / 0.3),
                Some(_) => 0.,
                None => 1.,
            };
            let zoom = if entry.compacted.is_some() { 0.9 } else { 1. };
            let opacity = 0.8 + 0.2 * compact_remaining;
            let left = 20. * zoom;
            let right = (20. + 20. * compact_remaining) * zoom;
            let vertical = (4. + 2. * compact_remaining) * zoom;
            let edge_width = (4. + 4. * compact_remaining).round();
            let horizontal = left + right + edge_width + 1.;
            let max_text_width = (screen.width() - horizontal - 10.).max(1.);
            let mut body_font = theme::TextRole::NoticeBody.font();
            body_font.size *= zoom;
            let title_font = if subtitle.is_some() {
                let mut font = theme::TextRole::NoticeTitle.font();
                font.size *= zoom;
                font
            } else {
                body_font.clone()
            };
            let galley = painter.layout(message, title_font, Color32::PLACEHOLDER, max_text_width);
            let subtitle_inset = 4. * zoom;
            let subtitle = subtitle.map(|text| {
                painter.layout(
                    text.to_owned(),
                    body_font,
                    Color32::PLACEHOLDER,
                    (max_text_width - subtitle_inset).max(1.),
                )
            });
            let text_width = subtitle.as_ref().map_or(galley.size().x, |line| {
                galley.size().x.max(line.size().x + subtitle_inset)
            });
            let subtitle_y = galley.size().y + 4. * zoom;
            let text_height = subtitle
                .as_ref()
                .map_or(galley.size().y, |line| subtitle_y + line.size().y);
            let width = (text_width + left + right).max(150.) + edge_width + 1.;
            let height = text_height + vertical * 2. + 2.;
            // Match the source's max-height arrival and jQuery slide-up on dismissal.
            let blink = !matches!(
                entry.notice,
                CanvasNotice::SavedProject(_)
                    | CanvasNotice::SavedTileSelection
                    | CanvasNotice::EnteredFullscreen
                    | CanvasNotice::LinkOpened
                    | CanvasNotice::LinkCopied
                    | CanvasNotice::ClipboardCopied(_)
                    | CanvasNotice::ValueReset
                    | CanvasNotice::LocatingFile
            );
            let arrival_time = if blink { 1. } else { 1.3 };
            let arrived = 2. + 200. * (1. - css_ease_remaining((age / arrival_time).min(1.)));
            let exit = ((age - duration) / 0.1).clamp(0., 1.);
            let visible = 0.5 + 0.5 * (std::f32::consts::PI * exit).cos();
            let top = bottom - height.min(arrived) * visible;
            let row = Rect::from_min_size(
                egui::pos2(screen.right() - width, top),
                egui::vec2(width, height),
            );
            let clip = Rect::from_min_max(
                egui::pos2(screen.left(), screen.top()),
                egui::pos2(
                    screen.right(),
                    if top + height > bottom + 0.01 {
                        bottom
                    } else {
                        bottom + 8.
                    },
                ),
            );
            bottom = top - 8. * visible;
            if row.bottom() >= screen.top() {
                let painter = painter.with_clip_rect(clip);
                let brightness = if !blink {
                    1.
                } else {
                    [(0.25, 7.), (0.4, 3.), (0.85, 1.5), (1.75, 1.)]
                        .windows(2)
                        .find_map(|points| {
                            let [(start, from), (end, to)] = [points[0], points[1]];
                            (start..end).contains(&age).then(|| {
                                to + (from - to) * css_ease_remaining((age - start) / (end - start))
                            })
                        })
                        .unwrap_or(1.)
                };
                let (fill, edge) = match entry.notice {
                    CanvasNotice::EntityLimit { .. }
                    | CanvasNotice::SavingFailed
                    | CanvasNotice::CannotSaveBackup
                    | CanvasNotice::CannotEditBackup
                    | CanvasNotice::NothingToSelect
                    | CanvasNotice::SymmetricRule
                    | CanvasNotice::LocateMissingPath => theme::notice::ERROR,
                    CanvasNotice::SavedProject(_)
                    | CanvasNotice::ValueReset
                    | CanvasNotice::EnteredFullscreen => theme::notice::SUCCESS,
                    CanvasNotice::SavedTileSelection
                    | CanvasNotice::LinkOpened
                    | CanvasNotice::LinkCopied
                    | CanvasNotice::LocatingFile => theme::notice::INFO,
                    CanvasNotice::ClipboardCopied(_) => theme::notice::CLIPBOARD,
                    _ => theme::notice::WARNING,
                };
                let shade = |color: Color32| {
                    let [r, g, b, _] = color
                        .to_array()
                        .map(|v| (f32::from(v) * brightness).round() as u8);
                    Color32::from_rgb(r, g, b).gamma_multiply(opacity)
                };
                let corners = egui::CornerRadius {
                    nw: 3,
                    sw: 3,
                    ..Default::default()
                };
                let shadow = egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 6,
                    spread: 0,
                    color: Color32::from_black_alpha(180).gamma_multiply(opacity),
                };
                painter.add(shadow.as_shape(row, corners));
                painter.rect_filled(row, corners, shade(fill));
                if entry.compacted.is_some()
                    && let Ok(egui::load::TexturePoll::Ready { texture }) =
                        egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
                            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                            .load_for_size(ctx, row.size())
                {
                    painter.image(
                        texture.id,
                        row,
                        Rect::from_min_size(egui::Pos2::ZERO, row.size() / 8.),
                        Color32::WHITE.gamma_multiply(opacity),
                    );
                }
                let border = shade(edge);
                painter.rect_stroke(
                    row,
                    corners,
                    Stroke::new(1_f32, border),
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(
                    Rect::from_min_max(egui::pos2(row.right() - edge_width, top), row.max),
                    0.,
                    border,
                );
                let pos = row.min + egui::vec2(left + 1., vertical + 1.);
                let text_opacity = opacity * if entry.compacted.is_some() { 0.8 } else { 1. };
                painter.galley(pos, galley, Color32::WHITE.gamma_multiply(text_opacity));
                if let Some(subtitle) = subtitle {
                    painter.galley(
                        pos + egui::vec2(subtitle_inset, subtitle_y),
                        subtitle,
                        Color32::WHITE.gamma_multiply(0.8 * text_opacity),
                    );
                }
            }
            scheduled = scheduled.min(
                if age < 1.75 || age >= duration || compact_age.is_some_and(|age| age < 0.3) {
                    0.016
                } else {
                    duration - age
                },
            );
        }
        self.canvas_notices.retain(|entry| {
            let duration = entry.notice.duration(ctx, &entry.notice.message(ctx));
            now.duration_since(entry.since).as_secs_f32() < duration + 0.1
        });
        if scheduled.is_finite() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(scheduled));
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
        let backup_bottom = self.backup_preview_banner(ctx, rect);
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
                    } else if button.action_tooltip(locale::tr(ui, "All project auto-layers need to be updated to adapt to your latest changes.")).clicked() {
                        self.start_auto_layer_rebuild(ctx);
                    }
                });
        }
        if let Some((name, icon, since)) = &self.quick_notification {
            let elapsed = since.elapsed();
            #[cfg(test)]
            let elapsed = self.quick_notification_test_age.unwrap_or(elapsed);
            let elapsed = elapsed.as_secs_f32();
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
                                        ui.add(
                                            egui::Image::new(icon.clone())
                                                .fit_to_exact_size(Vec2::splat(50.)),
                                        );
                                    }
                                    ui.label(
                                        RichText::new(name)
                                            .font(theme::TextRole::QuickNotice.font())
                                            .color(theme::ACCENT),
                                    );
                                });
                            });
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        if let Some((_, since)) = &self.last_chance {
            let remaining = Duration::from_secs(20).saturating_sub(since.elapsed());
            if remaining.is_zero() {
                self.last_chance = None;
            } else {
                ctx.request_repaint_after(remaining);
            }
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
                .fixed_pos(egui::pos2(rect.left(), backup_bottom.unwrap_or(rect.top())))
                .order(egui::Order::Middle)
                .fade_in(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(94, 171, 107))
                        .inner_margin(egui::Margin::symmetric(0, 8))
                        .show(ui, |ui| {
                            let max_width = (rect.width().min(700.) - 64.).max(0.);
                            let paragraphs = desc
                                .lines()
                                .filter(|line| !line.is_empty())
                                .map(|line| {
                                    ui.painter().layout(
                                        line.into(),
                                        FontId::proportional(14.667),
                                        Color32::WHITE,
                                        max_width,
                                    )
                                })
                                .collect::<Vec<_>>();
                            let text_width = paragraphs
                                .iter()
                                .map(|line| line.size().x)
                                .fold(0., f32::max);
                            let text_height =
                                paragraphs.iter().map(|line| line.size().y).sum::<f32>()
                                    + 6. * paragraphs.len().saturating_sub(1) as f32;
                            let height = text_height.max(64.);
                            let (row, _) = ui.allocate_exact_size(
                                egui::vec2(rect.width(), height),
                                Sense::hover(),
                            );
                            let left = row.center().x - (text_width + 64.) * 0.5;
                            egui::Image::new(egui::include_image!("../../assets/icons/help.svg"))
                                .paint_at(
                                    ui,
                                    Rect::from_center_size(
                                        egui::pos2(left + 24., row.center().y),
                                        Vec2::splat(48.),
                                    ),
                                );
                            let extra = (height - text_height) / paragraphs.len().max(1) as f32;
                            let mut y = row.top() + extra * 0.5;
                            for paragraph in paragraphs {
                                let advance = paragraph.size().y + 6. + extra;
                                ui.painter().galley(
                                    egui::pos2(left + 64., y),
                                    paragraph,
                                    Color32::WHITE,
                                );
                                y += advance;
                            }
                        });
                });
        }
        let now = Instant::now();
        self.paint_canvas_notices(ctx, now);
        let id = egui::Id::new("last-notification");
        let remaining = ctx.data_mut(|data| {
            let (message, since) =
                data.get_temp_mut_or_insert_with(id, || (self.status.clone(), now));
            if *message != self.status {
                message.clone_from(&self.status);
                *since = now;
            }
            Duration::from_secs(4).saturating_sub(now.saturating_duration_since(*since))
        });
        let resize_notice = self.status == "Level resized"
            && self
                .canvas_notices
                .iter()
                .any(|entry| matches!(entry.notice, CanvasNotice::ResizeRemovals(_)));
        if !self.is_busy()
            && !self.status.is_empty()
            && self.status != SAVED_PROJECT_TITLE
            && !self.status.starts_with("Save failed: ")
            && !resize_notice
            && (self.status_error || !remaining.is_zero())
            && !self.status.starts_with("Project opened.")
            && !self.status.starts_with("Open a project")
        {
            if !self.status_error {
                ctx.request_repaint_after(remaining);
            }
            egui::Area::new(egui::Id::new("notification"))
                .fixed_pos(rect.left_bottom() + egui::vec2(44.0, -48.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let (fill, edge) = if self.status_error {
                        theme::notice::ERROR
                    } else {
                        theme::notice::INFO
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(Stroke::new(1_f32, edge))
                        .corner_radius(3)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_max_width((rect.width() - 90.0).min(700.0));
                            ui.label(
                                RichText::new(locale::tr(ui, &self.status))
                                    .font(theme::TextRole::NoticeBody.font()),
                            );
                        });
                });
        }
    }

    fn backup_preview_banner(&mut self, ctx: &egui::Context, rect: Rect) -> Option<f32> {
        let project = self
            .project
            .as_ref()
            .filter(|project| project.is_backup())?;
        let date = project.backup_date_label();
        let linked = self.backup_original(ctx).is_some();
        let busy = self.is_busy();
        let banner = egui::Area::new(egui::Id::new("backup-preview-banner"))
            .fixed_pos(rect.left_top())
            .order(egui::Order::Middle)
            .fade_in(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(101, 148, 255))
                    .inner_margin(egui::Margin::symmetric(0, 8))
                    .show(ui, |ui| {
                        ui.set_width(rect.width());
                        ui.set_min_height(86.);
                        let width = rect.width().min(600.);
                        let buttons = (width * 0.44).min(250.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.;
                            ui.add_space((rect.width() - width) * 0.5);
                            ui.vertical(|ui| {
                                ui.set_width((width - buttons - 16.).max(0.));
                                ui.spacing_mut().item_spacing.y = 8.;
                                ui.add_space(4.);
                                let description = if linked {
                                    "This file is a BACKUP: you cannot edit or modify to it in any way. You may only restore it to replace the original project."
                                } else {
                                    "The images are not displayed properly because the location of the original project represented by this backup is unknown."
                                };
                                let font = FontId::new(12., egui::FontFamily::Name("body-bold".into()));
                                ui.label(RichText::new(locale::tr(ui, description)).font(font.clone()).color(Color32::WHITE));
                                if let Some(date) = &date {
                                    ui.label(RichText::new(date).font(font).color(Color32::WHITE));
                                }
                            });
                            ui.add_space(16.);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.;
                                ui.add_space(1.);
                                if !linked && backup_banner_button(
                                    ui, "LOCATE THE ORIGINAL PROJECT", buttons, theme::LIGHT, !busy,
                                ).clicked() {
                                    self.locate_backup_original(ctx);
                                }
                                if backup_banner_button(
                                    ui, "RESTORE THIS BACKUP", buttons, Color32::from_rgb(227, 77, 0), linked && !busy,
                                ).clicked() {
                                    self.confirm_preview_restore(ctx);
                                }
                            });
                        });
                    });
            });
        Some(banner.response.rect.bottom())
    }
}

fn backup_banner_button(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    fill: Color32,
    enabled: bool,
) -> egui::Response {
    let label = locale::tr(ui, label);
    let muted = Color32::from_rgb(115, 127, 157);
    ui.add_enabled_ui(enabled, |ui| {
        if !enabled && ui.is_visible() {
            ui.set_opacity(1.);
        }
        let widgets = &mut ui.visuals_mut().widgets;
        widgets.inactive.weak_bg_fill = fill;
        widgets.hovered.weak_bg_fill = if fill == theme::LIGHT {
            muted
        } else {
            Color32::from_rgb(255, 93, 74)
        };
        widgets.active.weak_bg_fill = widgets.hovered.weak_bg_fill;
        let color = if enabled { Color32::WHITE } else { muted };
        let mut button =
            egui::Button::new(RichText::new(label).size(13.333).color(color)).corner_radius(0);
        if !enabled {
            button = button.fill(theme::DARK);
        }
        panels::source_controls::add_sized_button(ui, egui::vec2(width, 40.), button)
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_notice_stays_above_the_tutorial_and_shows_the_local_date() {
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let path = PathBuf::from("backup_2024-01-04_11-00-00/World.ldtk");
        project.path = Some(path.clone());
        project.json["tutorialDesc"] = serde_json::json!("First line\nSecond line");
        app.project = Some(project);
        ctx.data_mut(|data| {
            data.insert_temp(
                project_dialogs::backup_original_key(&path),
                PathBuf::from("World.ldtk"),
            );
        });

        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let canvas = Rect::from_min_max(egui::pos2(300., 0.), screen.max);
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    app.canvas_notifications(ctx, canvas);
                },
            );
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                app.canvas_notifications(ctx, canvas);
            },
        );
        let background = |color| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == color && rect.rect.width() == canvas.width() =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("notification background")
        };
        let backup = background(Color32::from_rgb(101, 148, 255));
        let tutorial = background(Color32::from_rgb(94, 171, 107));
        assert_eq!(backup.top(), canvas.top());
        assert_eq!(tutorial.top(), backup.bottom());
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.job.text.starts_with("Thu Jan 04 2024 11:00:00 GMT")))
        );
    }

    fn paint(app: &mut EditorApp, ctx: &egui::Context, now: Instant) -> egui::FullOutput {
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| app.paint_canvas_notices(ctx, now),
        )
    }

    #[test]
    fn simultaneous_canvas_notices_keep_the_earlier_message_above_the_latest() {
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.report_entity_limit(1, "Actor".into());
        app.report_out_of_bounds(&["Actor".into()]);
        let at = Instant::now() + std::time::Duration::from_secs(2);
        let output = paint(&mut app, &ctx, at);
        let row = |color| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == color && rect.rect.width() > 150. => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("notice background")
        };
        let red = row(Color32::from_rgb(87, 0, 0).gamma_multiply(0.8));
        let amber = row(Color32::from_rgb(69, 48, 6));
        assert_eq!(red.right(), amber.right());
        assert_eq!(amber.bottom(), 772.);
        assert!((amber.top() - red.bottom() - 8.).abs() < 0.01);
        assert!(red.height() < amber.height());
    }

    #[test]
    fn status_notification_schedules_its_expiration_without_input() {
        let mut app = EditorApp::empty();
        app.status = "Project settings changed".into();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| {
                app.canvas_notifications(ctx, screen)
            });
        }
        let output = ctx.run(Default::default(), |ctx| {
            app.canvas_notifications(ctx, screen)
        });
        let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert!(
            !delay.is_zero() && delay <= std::time::Duration::from_secs(4),
            "{delay:?}"
        );
    }

    #[test]
    fn canvas_notices_expire_independently_and_repeats_do_not_restart_the_timer() {
        let mut app = EditorApp::empty();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.report_out_of_bounds(&["A_very_long_entity_identifier".into()]);
        app.report_entity_limit(1, "A".into());
        let limit_since = app.canvas_notices[1].since;
        app.report_entity_limit(1, "A".into());
        assert_eq!(app.canvas_notices.len(), 2);
        assert_eq!(app.canvas_notices[1].since, limit_since);

        let at = limit_since + std::time::Duration::from_millis(4500);
        let output = paint(&mut app, &ctx, at);
        assert_eq!(app.canvas_notices.len(), 1);
        assert!(app.canvas_notices[0].compacted.is_some());
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text().ends_with("A_very_long_entity_identifier"))
        }));
        paint(&mut app, &ctx, at + std::time::Duration::from_secs(2));
        assert!(app.canvas_notices.is_empty());
    }
}
