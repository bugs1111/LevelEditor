use super::*;

pub(super) enum ApplicationWrite {
    Save(PathBuf),
    SaveAs(PathBuf),
    Create {
        path: PathBuf,
        browse_from: Option<PathBuf>,
    },
}

impl ApplicationWrite {
    fn path(&self) -> &Path {
        match self {
            Self::Save(path) | Self::SaveAs(path) | Self::Create { path, .. } => path,
        }
    }
}

fn in_application_directory(path: &Path) -> bool {
    let Some(directory) = path.parent().and_then(|parent| parent.canonicalize().ok()) else {
        return false;
    };
    let executable = std::env::current_exe().ok();
    let examples = resource_path_in("examples/original", executable.as_deref());
    executable
        .as_deref()
        .and_then(Path::parent)
        .into_iter()
        .chain(std::iter::once(examples.as_path()))
        .any(|root| {
            root.canonicalize()
                .is_ok_and(|root| directory.starts_with(root))
        })
}

pub(super) fn recommends_backups(project: &Project) -> bool {
    let Some(path) = &project.path else {
        return false;
    };
    !in_application_directory(path)
        && project.json["backupOnSave"] != true
        && !project.is_backup()
        && project.level_keys().len() >= 8
}

pub(super) fn suggest_backups(project: &Project) -> bool {
    recommends_backups(project)
        && !project.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.iter().any(|f| f == "IgnoreBackupSuggest"))
}

pub(super) fn backup_original_key(backup: &Path) -> egui::Id {
    egui::Id::new(("backup-original", backup))
}

const RESTORE_PREVIEW: &str = "restore-backup-preview";

impl EditorApp {
    pub(super) fn backup_original(&self, ctx: &egui::Context) -> Option<PathBuf> {
        let backup = self.project.as_ref()?.path.as_deref()?;
        ctx.data(|data| data.get_temp(backup_original_key(backup)))
    }

    pub(super) fn locate_backup_original(&mut self, ctx: &egui::Context) {
        let Some(backup) = self.project.as_ref().and_then(|p| p.path.clone()) else {
            return;
        };
        let mut dialog = FileDialog::new().add_filter("LDtk project", &["ldtk", "json"]);
        if let Some(parent) = backup.parent() {
            dialog = dialog.set_directory(parent);
        }
        if let Some(original) = dialog.pick_file() {
            self.request(ctx, Action::OpenBackup { backup, original });
        }
    }

    pub(super) fn confirm_preview_restore(&mut self, ctx: &egui::Context) {
        if let Some(backup) = self.project.as_ref().and_then(|p| p.path.clone())
            && let Some(original) = self.backup_original(ctx)
        {
            self.show_panel(None);
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new(RESTORE_PREVIEW), (backup, original))
            });
        }
    }

    pub(super) fn backup_restore_open(ctx: &egui::Context) -> bool {
        ctx.data(|data| {
            data.get_temp::<(PathBuf, PathBuf)>(egui::Id::new(RESTORE_PREVIEW))
                .is_some()
        })
    }

    pub(super) fn preview_backup_restore_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(RESTORE_PREVIEW);
        let Some((backup, original)) = ctx.data(|data| data.get_temp::<(PathBuf, PathBuf)>(id))
        else {
            return;
        };
        if self.project.as_ref().and_then(|p| p.path.as_ref()) != Some(&backup) {
            ctx.data_mut(|data| data.remove::<(PathBuf, PathBuf)>(id));
            return;
        }
        let modal = egui::Modal::new(id)
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 40.).clamp(0., 358.));
                ui.set_min_height(120.);
                ui.spacing_mut().item_spacing = egui::vec2(2., 14.);
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(14.667));
                ui.label(locale::tr(ui, "WARNING: restoring this backup will REPLACE the original project file with this version."));
                ui.label(locale::tr(ui, "Are you sure?"));
                ui.horizontal(|ui| {
                    let size = egui::vec2((ui.available_width() - 2.) * 0.5, 35.);
                    let confirm = button(ui, "CONFIRM", size, false, 13.333).clicked();
                    let cancel = button(ui, "CANCEL", size, true, 13.333).clicked();
                    (confirm, cancel)
                }).inner
            });
        let (confirm, cancel) = modal.inner;
        let cancel = cancel || modal.should_close();
        let confirm =
            confirm || ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        if confirm || cancel {
            ctx.data_mut(|data| data.remove::<(PathBuf, PathBuf)>(id));
        }
        if confirm && !cancel {
            if original.is_file() {
                self.request(ctx, Action::RestoreBackup { backup, original });
            } else {
                self.status = "Sorry, but I can't restore this backup: I can't locate the original project file.".into();
                self.status_error = true;
            }
        }
    }
}

pub(super) fn dialog_frame(padding: i8) -> egui::Frame {
    egui::Frame::new()
        .fill(theme::MEDIUM)
        .corner_radius(3)
        .inner_margin(padding)
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 32,
            spread: 0,
            color: Color32::from_black_alpha(153),
        })
}

pub(super) fn form_warning_dialog(id: egui::Id) -> egui::Modal {
    egui::Modal::new(id)
        .area(egui::Modal::default_area(id).anchor(Align2::LEFT_TOP, [0., 16.]))
        .frame(
            dialog_frame(20)
                .corner_radius(0.)
                .fill(Color32::from_rgb(121, 17, 5))
                .stroke(Stroke::new(1_f32, Color32::from_rgb(227, 77, 0))),
        )
        .backdrop_color(Color32::from_black_alpha(140))
}

pub(super) fn button(
    ui: &mut egui::Ui,
    label: &str,
    size: Vec2,
    gray: bool,
    font_size: f32,
) -> egui::Response {
    let label = locale::tr(ui, label);
    super::panels::source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(RichText::new(label).size(font_size).color(Color32::WHITE))
            .fill(if gray {
                theme::LIGHT
            } else {
                Color32::from_rgb(227, 77, 0)
            })
            .corner_radius(2),
    )
}

pub(super) fn dialog_title(ui: &mut egui::Ui, title: &str, width: f32, fill: Color32) {
    let (header, _) = ui.allocate_exact_size(egui::vec2(width, 36.), Sense::hover());
    ui.painter().rect_filled(header, 0., fill);
    ui.painter().text(
        header.left_center() + egui::vec2(20., 0.),
        Align2::LEFT_CENTER,
        locale::tr(ui, title),
        FontId::new(20., egui::FontFamily::Name("panel-title".into())),
        Color32::WHITE,
    );
}

fn save_warning(ui: &mut egui::Ui, width: f32, message: String) {
    let painter = ui.painter();
    let galley = painter.layout(
        message,
        FontId::proportional(14.667),
        Color32::PLACEHOLDER,
        width - 18.,
    );
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, galley.size().y + 18.), Sense::hover());
    // The source's 66% black fill over the save dialog's dark red.
    paint_warning(
        ui.painter(),
        rect,
        galley,
        Color32::from_rgb(41, 6, 2),
        Color32::WHITE,
    );
}

pub(super) fn paint_warning(
    painter: &egui::Painter,
    rect: Rect,
    galley: std::sync::Arc<egui::Galley>,
    fill: Color32,
    text_color: Color32,
) {
    let dark_red = Color32::from_rgb(139, 0, 0);
    painter.add(
        egui::Shadow {
            offset: [0, 0],
            blur: 4,
            spread: 0,
            color: dark_red,
        }
        .as_shape(rect, 0.),
    );
    painter.rect_filled(rect, 0., fill);

    // Sample the 32 px CSS inset blur from each edge.
    let mut inset = egui::Mesh::default();
    let stops = [
        (0., 0.5),
        (4., 0.4),
        (8., 0.31),
        (16., 0.16),
        (24., 0.067),
        (32., 0.023),
        (48., 0.),
    ];
    let horizontal = egui::vec2(rect.width(), 0.);
    let vertical = egui::vec2(0., rect.height());
    for (edge, inward, span) in [
        (rect.left_top(), Vec2::Y, horizontal),
        (rect.left_bottom(), -Vec2::Y, horizontal),
        (rect.left_top(), Vec2::X, vertical),
        (rect.right_top(), -Vec2::X, vertical),
    ] {
        for pair in stops.windows(2) {
            let [(first, first_alpha), (last, last_alpha)] = [pair[0], pair[1]];
            let start = edge + inward * first;
            let end = edge + inward * last;
            let a = inset.vertices.len() as u32;
            let near = dark_red.gamma_multiply(first_alpha);
            let far = dark_red.gamma_multiply(last_alpha);
            inset.colored_vertex(start, near);
            inset.colored_vertex(start + span, near);
            inset.colored_vertex(end, far);
            inset.colored_vertex(end + span, far);
            inset.add_triangle(a, a + 1, a + 2);
            inset.add_triangle(a + 1, a + 3, a + 2);
        }
    }
    painter
        .with_clip_rect(rect.shrink(1.))
        .add(egui::Shape::mesh(inset));
    let edge = rect.shrink(0.5);
    for side in [
        [edge.left_top(), edge.right_top()],
        [edge.left_bottom(), edge.right_bottom()],
        [edge.left_top(), edge.left_bottom()],
        [edge.right_top(), edge.right_bottom()],
    ] {
        painter.extend(egui::Shape::dashed_line(
            &side,
            Stroke::new(1_f32, Color32::RED),
            2.,
            3.,
        ));
    }
    let text = rect.min + Vec2::splat(9.);
    for offset in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
        painter.galley(
            text + offset * 1.5,
            galley.clone(),
            Color32::RED.gamma_multiply(0.16),
        );
    }
    painter.galley(text, galley, text_color);
}

impl EditorApp {
    pub(super) fn check_application_write(&mut self, ctx: &egui::Context, write: ApplicationWrite) {
        if in_application_directory(write.path()) {
            self.application_write = Some(write);
            ctx.request_repaint();
        } else {
            self.perform_project_write(ctx, write);
        }
    }

    fn perform_project_write(&mut self, ctx: &egui::Context, write: ApplicationWrite) {
        match write {
            ApplicationWrite::Save(path)
                if self
                    .project
                    .as_ref()
                    .is_some_and(|project| !project.is_backup())
                    && resources::is_missing_file(&path) =>
            {
                self.dirty = true;
                self.save_failure = Some(services::SaveFailure::MissingTarget(path));
                ctx.request_repaint();
            }
            ApplicationWrite::Save(path) | ApplicationWrite::SaveAs(path) => {
                self.save_document(ctx, path);
            }
            ApplicationWrite::Create { path, .. } => self.create_project_async(ctx, path),
        }
    }

    pub(super) fn show_application_directory_warning(&mut self, ctx: &egui::Context) {
        let (message, alternative, here) = match &self.application_write {
            Some(ApplicationWrite::Save(_) | ApplicationWrite::SaveAs(_)) => (
                " you are trying to save a file in the application directory!",
                "Save somewhere else",
                "Save anyway (and lose my file during next update)",
            ),
            Some(ApplicationWrite::Create { .. }) => (
                " you are trying to create a project in the application directory!",
                "Create somewhere else",
                "Ignore that (you will lose your project during next update)",
            ),
            None => return,
        };
        let modal = egui::Modal::new(egui::Id::new("application-directory-write"))
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 56.).clamp(140., 360.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(0., 2.);
                let normal = FontId::proportional(14.667);
                let bold = FontId::new(14.667, egui::FontFamily::Name("body-bold".into()));
                let mut warning = egui::text::LayoutJob::default();
                warning.append(
                    locale::tr(ui, "WARNING:"),
                    0.,
                    egui::TextFormat::simple(bold.clone(), Color32::WHITE),
                );
                warning.append(
                    locale::tr(ui, message),
                    0.,
                    egui::TextFormat::simple(normal, Color32::WHITE),
                );
                ui.add_space(14.);
                ui.label(warning);
                ui.add_space(11.);
                ui.label(
                    RichText::new(locale::tr(
                        ui,
                        "Any file saved here will be LOST during next app update.",
                    ))
                    .font(bold)
                    .color(Color32::WHITE),
                );
                ui.add_space(13.);
                let mut clicked = None;
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                for (index, label) in [alternative, here, "Cancel"].into_iter().enumerate() {
                    if button(ui, label, egui::vec2(width, 35.), index != 0, 13.333).clicked() {
                        clicked = Some(index);
                    }
                }
                clicked
            });
        let Some(choice) = modal.inner.or_else(|| modal.should_close().then_some(2)) else {
            return;
        };
        let Some(write) = self.application_write.take() else {
            return;
        };
        ctx.request_repaint();
        match (choice, write) {
            (0, ApplicationWrite::Save(path) | ApplicationWrite::SaveAs(path)) => {
                if let Some(destination) = Self::pick_save_path(Some(&path)) {
                    self.perform_project_write(ctx, ApplicationWrite::SaveAs(destination));
                }
            }
            (0, ApplicationWrite::Create { browse_from, .. }) => {
                self.new_project_dialog(ctx, browse_from);
            }
            (1, write) => self.perform_project_write(ctx, write),
            _ => {}
        }
    }

    pub(super) fn show_save_failure(&mut self, ctx: &egui::Context) {
        #[derive(Clone, Copy)]
        enum Choice {
            Retry,
            SaveAs,
            OpenFolder,
            Cancel,
        }

        let (message, detail) = match &self.save_failure {
            Some(services::SaveFailure::Write { path, reason }) => match reason {
                services::SaveWriteFailure::DirectoryConflict => (
                    locale::format_ctx(
                        ctx,
                        "I need to create a folder named \"{name}\", but there is a file with the exact same name there.",
                        &[(
                            "name",
                            &path.file_stem().unwrap_or_default().to_string_lossy(),
                        )],
                    ),
                    "The project was NOT saved properly!",
                ),
                services::SaveWriteFailure::Io(message) => {
                    (message.clone(), "The project was NOT saved properly!")
                }
                services::SaveWriteFailure::Interrupted => (
                    locale::tr_ctx(ctx, services::SAVE_INTERRUPTED).into(),
                    "The destination file may have changed.",
                ),
            },
            Some(services::SaveFailure::MissingTarget(_)) => {
                self.show_missing_save_target(ctx);
                return;
            }
            None => return,
        };
        const ERROR_HEADER: Color32 = Color32::from_rgb(227, 77, 0);
        const ERROR_BODY: Color32 = Color32::from_rgb(121, 17, 5);
        let modal = egui::Modal::new(egui::Id::new("project-save-failure"))
            .frame(dialog_frame(0).fill(ERROR_BODY))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 16.).clamp(180., 500.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 2.);
                dialog_title(ui, "ERROR DURING PROJECT SAVING", width, ERROR_HEADER);
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 20,
                        right: 20,
                        top: 15,
                        bottom: 23,
                    })
                    .show(ui, |ui| {
                        let width = width - 40.;
                        ui.set_width(width);
                        let body =
                            |text: &str| RichText::new(text).size(14.667).color(Color32::WHITE);
                        save_warning(ui, width, message);
                        ui.add_space(13.);
                        ui.label(body(locale::tr(ui, detail)));
                        ui.add_space(16.);
                        let size = egui::vec2(width, 35.);
                        let mut choice = None;
                        if button(ui, "RETRY", size, false, 13.333).clicked() {
                            choice = Some(Choice::Retry);
                        }
                        if button(ui, "SAVE AS...", size, true, 13.333).clicked() {
                            choice = Some(Choice::SaveAs);
                        }
                        ui.horizontal(|ui| {
                            let open_width = 178_f32.min(width * 0.55);
                            if button(
                                ui,
                                "OPEN PROJECT FOLDER",
                                egui::vec2(open_width, 35.),
                                true,
                                13.333,
                            )
                            .clicked()
                            {
                                choice = Some(Choice::OpenFolder);
                            }
                            if button(
                                ui,
                                "CANCEL",
                                egui::vec2(width - open_width - 2., 35.),
                                true,
                                13.333,
                            )
                            .clicked()
                            {
                                choice = Some(Choice::Cancel);
                            }
                        });
                        choice
                    })
                    .inner
            });
        let Some(choice) = modal
            .inner
            .or_else(|| modal.should_close().then_some(Choice::Cancel))
        else {
            return;
        };
        let Some(services::SaveFailure::Write { path, .. }) = self.save_failure.take() else {
            return;
        };
        ctx.request_repaint();
        match choice {
            Choice::Retry if !path.exists() => {
                self.save_failure = Some(services::SaveFailure::MissingTarget(path));
            }
            Choice::Retry => self.save_to(ctx, path),
            Choice::SaveAs => {
                if let Some(path) = Self::pick_save_path(Some(&path)) {
                    self.save_to(ctx, path);
                }
            }
            Choice::OpenFolder => {
                let parent = path.parent().unwrap_or(Path::new("."));
                if let Err(error) = home_files::locate(parent, false) {
                    self.status = error.to_string();
                }
            }
            Choice::Cancel => {}
        }
    }

    fn show_missing_save_target(&mut self, ctx: &egui::Context) {
        let Some(services::SaveFailure::MissingTarget(path)) = &self.save_failure else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::new("missing-save-target"))
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 56.).clamp(160., 360.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(
                    RichText::new(locale::format(
                        ui,
                        "The project file is no longer in {path}. Save to this path anyway?",
                        &[("path", &path.display().to_string())],
                    ))
                    .size(14.667)
                    .color(Color32::WHITE),
                );
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    let size = egui::vec2((width - 2.) / 2., 35.);
                    let confirm = button(ui, "CONFIRM", size, false, 13.333).clicked();
                    let cancel = button(ui, "CANCEL", size, true, 13.333).clicked();
                    (confirm, cancel)
                })
                .inner
            });
        let (confirm, cancel) = modal.inner;
        if (confirm || cancel || modal.should_close())
            && let Some(services::SaveFailure::MissingTarget(path)) = self.save_failure.take()
        {
            ctx.request_repaint();
            if confirm && let Some(destination) = Self::pick_save_path(Some(&path)) {
                self.save_to(ctx, destination);
            }
        }
    }

    pub(super) fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("unsaved-changes"))
            .frame(dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let message = locale::tr(ui, "Do you want to save before leaving?");
                let width = ui.fonts_mut(|fonts| {
                    fonts
                        .layout_no_wrap(
                            message.into(),
                            FontId::proportional(14.667),
                            Color32::WHITE,
                        )
                        .size()
                        .x
                });
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(RichText::new(message).size(14.667).color(Color32::WHITE));
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    for (index, label) in ["YES", "NO", "CANCEL"].into_iter().enumerate() {
                        if button(
                            ui,
                            label,
                            egui::vec2((width - 4.) / 3., 35.),
                            index == 2,
                            14.667,
                        )
                        .clicked()
                        {
                            choice = Some(index);
                        }
                    }
                });
            });
        match choice {
            Some(0) if self.save(false) => {
                if let Some(action) = self.pending.take() {
                    self.execute(ctx, action);
                }
            }
            Some(1) => {
                if let Some(action) = self.pending.take() {
                    self.execute(ctx, action);
                }
            }
            Some(2) => self.pending = None,
            _ if modal.should_close() => self.pending = None,
            _ => {}
        }
    }

    pub(super) fn show_backup_suggestion(&mut self, ctx: &egui::Context) {
        let mut enable = None;
        egui::Modal::new(egui::Id::new("suggest-backups"))
            .frame(dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(400.);
                ui.spacing_mut().item_spacing = egui::vec2(0., 0.);
                dialog_title(ui, "ENABLE BACKUPS", 400., theme::LIGHT);
                egui::Frame::new().inner_margin(20).show(ui, |ui| {
                    ui.set_width(360.);
                    ui.label(RichText::new(locale::tr(ui, "As your project is growing bigger, it is STRONGLY advised to enable BACKUPS, to secure your work."))
                        .size(14.667).color(Color32::WHITE));
                    ui.add_space(16.);
                    if button(ui, "Enable backups when saving", egui::vec2(360., 67.333), false, 17.333).clicked() {
                        enable = Some(true);
                    }
                    ui.add_space(2.);
                    if button(ui, "No, and I understand the risk.", egui::vec2(360., 35.), true, 14.667).clicked() {
                        enable = Some(false);
                    }
                });
            });
        if let Some(enable) = enable {
            self.edit("Backup settings changed", |project| {
                if enable {
                    project.json["backupOnSave"] = serde_json::json!(true);
                } else {
                    project.json["flags"]
                        .as_array_mut()
                        .unwrap()
                        .push(serde_json::json!("IgnoreBackupSuggest"));
                }
                Ok(())
            });
            self.suggest_backups = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        app.repaint = Some(ctx.clone());
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            focused: true,
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
        })
    }

    fn label(output: &egui::FullOutput, value: &str) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == value => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing visible label: {value}"))
    }

    fn click(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    fn wait_for_save(app: &mut EditorApp, ctx: &egui::Context) -> egui::FullOutput {
        if let Some(pending) = app.saving.take() {
            let result = pending
                .recv_timeout(Duration::from_secs(15))
                .expect("save worker should send its result");
            // Deliver the worker's result through the normal frame update.
            let (sender, ready) = std::sync::mpsc::channel();
            assert!(sender.send(result).is_ok());
            app.saving = Some(ready);
        }
        let output = frame(app, ctx, vec![]);
        assert!(app.saving.is_none());
        if app.save_failure.is_some() {
            frame(app, ctx, vec![])
        } else {
            output
        }
    }

    #[test]
    fn saving_in_application_directory_waits_for_the_users_choice() {
        let executable = std::env::current_exe().unwrap();
        let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
        let path = directory.path().join("Bundled.ldtk");
        let mut project = Project::new();
        project.save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.project.as_mut().unwrap().json["defaultLevelBgColor"] = "#123456".into();
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);

        app.save(false);
        frame(&mut app, &ctx, vec![]);
        let warning = frame(&mut app, &ctx, vec![]);
        label(&warning, "Save somewhere else");
        assert!(app.saving.is_none());
        click(&mut app, &ctx, label(&warning, "Cancel").center());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(app.dirty && !app.is_busy());

        app.save(false);
        frame(&mut app, &ctx, vec![]);
        let warning = frame(&mut app, &ctx, vec![]);
        let save = label(
            &warning,
            "Save anyway (and lose my file during next update)",
        );
        click(&mut app, &ctx, save.center());
        wait_for_save(&mut app, &ctx);
        assert!(!app.dirty);
        assert_eq!(
            Project::load(&path).unwrap().json["defaultLevelBgColor"],
            "#123456"
        );
    }

    #[test]
    fn moving_the_project_folder_preserves_unsaved_levels_for_save_as() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("Original");
        let moved = dir.path().join("Moved");
        std::fs::create_dir(&original).unwrap();
        let path = original.join("Map.ldtk");
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let key = project.level_keys()[0];
        let relative = project.json["levels"][key.level]["externalRelPath"]
            .as_str()
            .unwrap()
            .to_owned();
        let before = std::fs::read(original.join(&relative)).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.project
            .as_mut()
            .unwrap()
            .paint_int_grid(key, 0, 1, 1, 1)
            .unwrap();
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);

        std::fs::rename(&original, &moved).unwrap();
        app.changed_resources(&ctx, vec![path.clone(), original.join(&relative)]);
        assert!(app.external_changes.is_none());
        app.save(false);
        frame(&mut app, &ctx, vec![]);
        let warning = frame(&mut app, &ctx, vec![]);
        label(&warning, "CONFIRM");
        click(&mut app, &ctx, label(&warning, "CANCEL").center());
        assert!(app.dirty && !app.is_busy());
        assert!(!original.exists());

        let new_path = dir.path().join("Rescued.ldtk");
        app.save_to(&ctx, new_path.clone());
        wait_for_save(&mut app, &ctx);
        let saved = Project::load(&new_path).unwrap();
        assert_eq!(saved.level(key), app.project.as_ref().unwrap().level(key));
        let layer = saved.layer(key, 0).unwrap();
        let width = layer["__cWid"].as_u64().unwrap() as usize;
        assert_eq!(layer["intGridCsv"][1 + width], 1);
        assert_eq!(std::fs::read(moved.join(relative)).unwrap(), before);
        assert!(!original.exists() && !app.dirty);
    }

    #[test]
    fn save_failure_reason_has_a_dashed_red_border() {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.save_failure = Some(services::SaveFailure::Write {
            path: PathBuf::from("Map.ldtk"),
            reason: services::SaveWriteFailure::DirectoryConflict,
        });
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let retry = label(&output, "RETRY");
        let segments = output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::LineSegment { points, stroke }
                if stroke.color.r() > 0 && stroke.color.g() == 0 && stroke.color.b() == 0
                    && (points[0].y - points[1].y).abs() < 0.01
                    && points[0].y < retry.top()
                    && (points[0].x - points[1].x).abs() <= 2.1)
            })
            .count();
        assert!(
            segments > 100,
            "save failure warning should paint both dashed horizontal borders"
        );
    }

    #[test]
    fn save_failure_keeps_the_inspector_visible_and_blocks_its_controls() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        let actor = project.add_entity_def("Actor", 16, 16).unwrap();
        let key = project.level_keys()[0];
        project.add_entity(key, 0, actor, 80, 88).unwrap();
        let iid = project.json["iid"].as_str().unwrap().to_owned();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Entity);
        app.entity_index = Some(0);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec![]);
        }
        let before = frame(&mut app, &ctx, vec![]);
        let extra = label(&before, "EXTRA ENTITY INFOS");
        let before_ui = app.settings.ui_state_int("extraEntityInfos", Some(&iid), 0);

        app.save_failure = Some(services::SaveFailure::Write {
            path: PathBuf::from("Map.ldtk"),
            reason: services::SaveWriteFailure::DirectoryConflict,
        });
        frame(&mut app, &ctx, vec![]);
        let under_modal = frame(&mut app, &ctx, vec![]);
        assert_eq!(label(&under_modal, "EXTRA ENTITY INFOS"), extra);
        label(&under_modal, "ERROR DURING PROJECT SAVING");

        click(&mut app, &ctx, extra.center());
        let after = frame(&mut app, &ctx, vec![]);
        assert!(app.save_failure.is_none());
        assert_eq!(app.panel, Some(Panel::Entity));
        assert_eq!(app.entity_index, Some(0));
        assert_eq!(
            app.settings.ui_state_int("extraEntityInfos", Some(&iid), 0),
            before_ui
        );
        assert_eq!(label(&after, "EXTRA ENTITY INFOS"), extra);
    }

    #[test]
    fn save_failure_preserves_layer_selection_and_blocks_layer_clicks() {
        let mut project = Project::new();
        project.add_layer("Lower", "Entities", 16).unwrap();
        project.add_layer("Upper", "Entities", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        assert_eq!(app.layer, 0);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let before = frame(&mut app, &ctx, vec![]);
        let upper = label(&before, "Upper");
        let lower = label(&before, "Lower");
        let row_fill = |output: &egui::FullOutput| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.width() == 300.
                        && rect.rect.height() == 31.
                        && rect.rect.contains(upper.center()) =>
                {
                    Some(rect.fill)
                }
                _ => None,
            })
        };
        assert_eq!(row_fill(&before), Some(theme::ACCENT));

        app.save_failure = Some(services::SaveFailure::Write {
            path: PathBuf::from("Map.ldtk"),
            reason: services::SaveWriteFailure::DirectoryConflict,
        });
        frame(&mut app, &ctx, vec![]);
        let under_modal = frame(&mut app, &ctx, vec![]);
        assert_eq!(row_fill(&under_modal), Some(theme::ACCENT));
        assert_eq!(label(&under_modal, "Lower"), lower);

        click(&mut app, &ctx, lower.center());
        frame(&mut app, &ctx, vec![]);
        assert!(app.save_failure.is_none());
        assert_eq!(app.layer, 0);
        click(&mut app, &ctx, lower.center());
        assert_eq!(app.layer, 1);

        let key = app.selected.unwrap();
        let upper_eye = egui::pos2(285., upper.center().y);
        click(&mut app, &ctx, upper_eye);
        assert_eq!(app.layer, 0);
        assert!(!app.layer_visible(key, 0));
        click(&mut app, &ctx, upper_eye);
        assert!(app.layer_visible(key, 0));
    }

    #[test]
    fn layer_visibility_drag_applies_one_value_and_keeps_the_selected_layer() {
        let mut project = Project::new();
        for name in ["Bottom", "Lower", "Upper", "Top"] {
            project.add_layer(name, "Entities", 16).unwrap();
        }
        let mut app = EditorApp::empty();
        app.set_project(project);
        let key = app.selected.unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let eye = |name| egui::pos2(285., label(&output, name).center().y);
        let (top, upper, lower, bottom) = (eye("Top"), eye("Upper"), eye("Lower"), eye("Bottom"));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        let visibility = |app: &EditorApp| [0, 1, 2, 3].map(|index| app.layer_visible(key, index));

        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(upper), button(upper, true)],
        );
        assert_eq!(visibility(&app), [true, false, true, true]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(lower),
                egui::Event::PointerMoved(bottom),
                button(bottom, false),
            ],
        );
        assert_eq!(visibility(&app), [true, false, false, false]);
        assert_eq!(app.layer, 0);

        app.undo_action(false);
        assert_eq!(visibility(&app), [true, false, false, true]);
        app.undo_action(false);
        assert_eq!(visibility(&app), [true, false, true, true]);
        app.undo_action(true);
        app.undo_action(true);
        assert_eq!(visibility(&app), [true, false, false, false]);

        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(upper), button(upper, true)],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(bottom), button(bottom, false)],
        );
        assert_eq!(visibility(&app), [true, true, false, true]);
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(lower)]);
        assert_eq!(visibility(&app), [true, true, false, true]);
        assert_eq!(app.layer, 0);

        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(upper), button(upper, true)],
        );
        frame(&mut app, &ctx, vec![egui::Event::WindowFocused(false)]);
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::WindowFocused(true),
                egui::Event::PointerMoved(top),
                button(top, false),
            ],
        );
        assert_eq!(visibility(&app), [true, false, false, true]);
        assert_eq!(app.layer, 0);
    }

    #[test]
    fn layer_context_menu_toggles_visibility_without_selecting_the_layer() {
        let mut project = Project::new();
        project.add_layer("Lower", "Entities", 16).unwrap();
        project.add_layer("Upper", "Entities", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let key = app.selected.unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let lower = label(&output, "Lower").center();
        let eye = egui::pos2(285., lower.y);
        let open_menu = |app: &mut EditorApp, pos: egui::Pos2| {
            frame(app, &ctx, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    app,
                    &ctx,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Secondary,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
            frame(app, &ctx, vec![])
        };
        let menu = open_menu(&mut app, eye);
        let toggle = label(&menu, "TOGGLE VISIBILITY");
        assert!(!app.layer_visible(key, 1));
        click(&mut app, &ctx, toggle.center());
        assert!(app.layer_visible(key, 1));
        assert_eq!(app.layer, 0);
        app.undo_action(false);
        assert!(!app.layer_visible(key, 1));
        app.undo_action(false);
        assert!(app.layer_visible(key, 1));

        let menu = open_menu(&mut app, lower);
        label(&menu, "TOGGLE VISIBILITY");
        assert!(app.layer_visible(key, 1));
        click(&mut app, &ctx, eye);
        assert!(app.layer_visible(key, 1));
        assert_eq!(app.layer, 0);

        let menu = open_menu(&mut app, lower);
        click(&mut app, &ctx, label(&menu, "EDIT LAYER SETTINGS").center());
        assert_eq!(app.layer, 1);
        assert_eq!(app.panel, Some(Panel::Layers));
    }

    #[test]
    fn layer_filter_keeps_a_matching_layer_and_selects_a_fallback_when_needed() {
        let mut project = Project::new();
        for (name, tag) in [
            ("HiddenLayer", ""),
            ("OtherLayer", "Other"),
            ("BackLayer", "Map"),
            ("FrontLayer", "Map"),
        ] {
            let uid = project.add_layer(name, "Entities", 16).unwrap();
            let tags = if tag.is_empty() {
                json!([])
            } else {
                json!([tag])
            };
            project
                .update_definition(
                    "layers",
                    uid,
                    &json!({
                        "uiFilterTags": tags,
                        "hideInList": tag.is_empty(),
                    }),
                )
                .unwrap();
        }
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.switch_layer(1);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "Map").center());
        assert_eq!(app.layer, 1, "retain the active layer when it matches");

        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "Other").center());
        assert_eq!(app.layer, 2);
        app.set_project(app.project.as_ref().unwrap().clone());
        assert_eq!(app.layer, 2, "restore the project's saved filter on open");
        app.switch_layer(3);
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "All").center());
        assert_eq!(app.layer, 0, "pick a listed layer when showing all");
    }

    #[test]
    fn layer_filter_dropdown_supports_keyboard_navigation_and_confirmation() {
        let mut project = Project::new();
        for (name, tag) in [
            ("DarkLayer", "Background"),
            ("OtherLayer", "Other"),
            ("CustomLayer", "Sprites"),
            ("FrontLayer", "Map"),
        ] {
            let uid = project.add_layer(name, "Entities", 16).unwrap();
            project
                .update_definition("layers", uid, &json!({"uiFilterTags": [tag]}))
                .unwrap();
        }
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let all = label(&output, "- Show all layers -").center();
        click(&mut app, &ctx, all);
        for key in [
            Key::Home,
            Key::ArrowDown,
            Key::ArrowDown,
            Key::ArrowDown,
            Key::Enter,
        ] {
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
            }
        }
        assert_eq!(app.layer, 2);
        let output = frame(&mut app, &ctx, vec![]);
        label(&output, "Other");
        assert!(!egui::Popup::is_any_open(&ctx));
    }

    #[test]
    fn empty_project_shows_the_layer_list_and_add_layer_guide() {
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        let empty = label(&output, "No layer");
        let guide = label(&output, "Need at least 1 layer");
        assert!(empty.center().y < guide.center().y);
        assert!(guide.left() < 12.);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.fill == Color32::from_rgb(39, 63, 47)
                    && rect.rect.top() == 140. && rect.rect.height() == 27.15625)
        }));
    }

    #[test]
    fn disconnected_save_worker_opens_recovery_and_retries_unsaved_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Map.ldtk");
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.project.as_mut().unwrap().level_mut(key).unwrap()["identifier"] = "Edited".into();
        app.dirty = true;
        app.settings.value["language"] = "zh-CN".into();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let (sender, job) = std::sync::mpsc::channel();
        app.saving = Some(job);
        app.save_attempt = Some(path.clone());
        app.saving_progress = Some(Arc::new(Mutex::new(save_progress::SaveProgress::default())));
        frame(&mut app, &ctx, vec![]);
        assert!(app.saving.is_some());
        assert!(app.save_failure.is_none());
        drop(sender);

        frame(&mut app, &ctx, vec![]);
        let failed = frame(&mut app, &ctx, vec![]);
        assert!(app.saving.is_none() && app.saving_progress.is_none());
        assert!(app.save_failure.is_some() && app.dirty);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        label(&failed, "保存已中断，无法确认是否完成。");
        label(&failed, "目标文件可能已发生变化。");
        let retry = label(&failed, "重试");
        click(&mut app, &ctx, retry.center());
        wait_for_save(&mut app, &ctx);
        assert!(!app.dirty && !app.status_error);
        assert_eq!(
            Project::load(&path).unwrap().level(key).unwrap()["identifier"],
            "Edited"
        );
    }

    #[test]
    fn embedded_project_save_rejects_a_conflicting_folder_then_retries_the_edit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Map.ldtk");
        let blocker = path.with_extension("");
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&blocker, "Existing file").unwrap();
        project.level_mut(key).unwrap()["identifier"] = "Edited".into();

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        app.save(false);
        let failed = wait_for_save(&mut app, &ctx);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(app.dirty && app.status_error);
        label(&failed, "Saving failed!");
        let retry = label(&failed, "RETRY");
        assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "Existing file");

        std::fs::remove_file(blocker).unwrap();
        click(&mut app, &ctx, retry.center());
        wait_for_save(&mut app, &ctx);
        assert_eq!(
            Project::load(&path).unwrap().level(key).unwrap()["identifier"],
            "Edited"
        );
        assert!(!app.dirty && !app.status_error);
    }

    #[test]
    fn failed_save_offers_recovery_and_retries_an_existing_destination() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("Original.ldtk");
        let destination = directory.path().join("Copy.ldtk");
        let blocker = destination.with_extension("");
        std::fs::write(&blocker, "Existing file").unwrap();
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        project.save(&original).unwrap();
        let original_root = std::fs::read(&original).unwrap();
        std::fs::write(&destination, &original_root).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.save_async_at(destination.clone());
        let failed = wait_for_save(&mut app, &ctx);
        label(&failed, "ERROR DURING PROJECT SAVING");
        label(&failed, "SAVE AS...");
        label(&failed, "OPEN PROJECT FOLDER");
        label(&failed, "CANCEL");
        let retry = label(&failed, "RETRY");
        assert!(app.dirty && app.status_error);
        assert_eq!(
            app.project.as_ref().unwrap().path.as_deref(),
            Some(original.as_path())
        );
        assert_eq!(std::fs::read(&destination).unwrap(), original_root);
        assert_eq!(std::fs::read_to_string(&blocker).unwrap(), "Existing file");

        std::fs::remove_file(blocker).unwrap();
        click(&mut app, &ctx, retry.center());
        wait_for_save(&mut app, &ctx);
        let saved = Project::load(&destination).unwrap();
        assert_eq!(saved.level_keys().len(), 1);
        assert_eq!(std::fs::read(&original).unwrap(), original_root);
        assert_eq!(
            app.project.as_ref().unwrap().path.as_ref(),
            Some(&destination)
        );
        assert!(!app.dirty && !app.status_error);
    }

    #[test]
    fn retrying_a_missing_save_target_asks_before_reopening_save_as() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Copy.ldtk");
        let blocker = path.with_extension("");
        std::fs::write(&blocker, "Existing file").unwrap();
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.dirty = true;
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.save_async_at(path.clone());
        let retry = label(&wait_for_save(&mut app, &ctx), "RETRY");
        std::fs::remove_file(blocker).unwrap();
        click(&mut app, &ctx, retry.center());
        assert!(matches!(
            &app.save_failure,
            Some(services::SaveFailure::MissingTarget(target)) if target == &path
        ));
        frame(&mut app, &ctx, vec![]);
        let confirm = frame(&mut app, &ctx, vec![]);
        label(&confirm, "CONFIRM");
        let cancel = label(&confirm, "CANCEL");
        assert!(app.saving.is_none());
        click(&mut app, &ctx, cancel.center());
        assert!(!app.is_busy() && app.dirty && !path.exists());
    }

    #[test]
    fn extensionless_save_with_external_levels_keeps_destination_unused() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("Copy");
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        assert!(project.save(&destination).is_err());
        assert!(
            !destination.exists(),
            "A failed save created a directory at the project path"
        );
    }

    #[test]
    fn leaving_dirty_project_uses_source_question_and_three_horizontal_choices() {
        for action in [Action::Close, Action::Quit] {
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.dirty = true;
            app.pending = Some(action);
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            let question = label(&output, "Do you want to save before leaving?");
            let choices = ["YES", "NO", "CANCEL"].map(|name| label(&output, name));
            assert!(
                choices[0].center().x < choices[1].center().x
                    && choices[1].center().x < choices[2].center().x
            );
            assert!(
                choices
                    .iter()
                    .all(|rect| (rect.center().y - choices[0].center().y).abs() < 1.
                        && rect.top() > question.bottom())
            );
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text()=="Unsaved changes")));
            click(&mut app, &ctx, choices[2].center());
            assert!(app.project.is_some() && app.dirty && app.pending.is_none());
            assert!(!app.allow_close);
        }
    }

    fn large_project() -> Project {
        let mut project = Project::new();
        for index in 1..8 {
            project
                .add_level(None, &format!("Level_{index}"), 256, 256)
                .unwrap();
        }
        project.path = Some(std::env::temp_dir().join("level-editor-backup-prompt.ldtk"));
        project.json["backupOnSave"] = json!(false);
        project
    }

    #[test]
    fn source_backup_choices_enable_backups_or_remember_the_decline() {
        for (choice, enabled) in [
            ("Enable backups when saving", true),
            ("No, and I understand the risk.", false),
        ] {
            let mut app = EditorApp::empty();
            app.set_project(large_project());
            let original = app.project.as_ref().unwrap().json.clone();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            frame(&mut app, &ctx, vec![]);
            let output = frame(&mut app, &ctx, vec![]);
            label(&output, "ENABLE BACKUPS");
            let enable = label(&output, "Enable backups when saving");
            let decline = label(&output, "No, and I understand the risk.");
            assert!(enable.bottom() < decline.top());
            assert_eq!(
                app.project.as_ref().unwrap().json,
                original,
                "opening must not change the project"
            );
            click(&mut app, &ctx, label(&output, choice).center());
            let project = app.project.as_ref().unwrap();
            assert_eq!(project.json["backupOnSave"], json!(enabled));
            assert_eq!(
                project.json["flags"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f == "IgnoreBackupSuggest"),
                !enabled
            );
            assert!(app.dirty);
            let project = project.clone();
            app.set_project(project);
            let output = frame(&mut app, &ctx, vec![]);
            assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text()=="ENABLE BACKUPS")));
        }
    }

    #[test]
    fn backup_suggestion_respects_the_source_threshold_and_suppressions() {
        let mut project = large_project();
        assert!(suggest_backups(&project));
        let mut small = project.clone();
        small.json["levels"].as_array_mut().unwrap().pop();
        assert!(!suggest_backups(&small));
        project.json["backupOnSave"] = json!(true);
        assert!(!suggest_backups(&project));
        project.json["backupOnSave"] = json!(false);
        project.json["flags"] = json!(["IgnoreBackupSuggest"]);
        assert!(!suggest_backups(&project));
        project.json["flags"] = json!([]);
        project.path = Some(resource_path("examples/original/Entities.ldtk"));
        assert!(!suggest_backups(&project));
        project.path = None;
        assert!(!suggest_backups(&project));
    }

    #[test]
    fn search_layer_change_shows_the_standard_layer_notification() {
        let mut project = Project::new();
        project.add_layer("Actors", "Entities", 16).unwrap();
        project.add_layer("Walls", "IntGrid", 16).unwrap();
        let key = project.level_keys()[0];
        let mut app = EditorApp::empty();
        app.quick_notification_test_age = Some(Duration::ZERO);
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);

        let target = 1 - app.layer;
        let layer = app.project.as_ref().unwrap().layer(key, target).unwrap();
        let name = layer["__identifier"].as_str().unwrap().to_owned();
        let iid = layer["iid"].as_str().unwrap().to_owned();
        app.search = iid;
        app.command_palette = true;
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        assert_eq!(app.layer, target);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t)
            if t.galley.text()==name && (t.galley.job.sections[0].format.font_id.size-40.).abs()<0.1
        )));
        assert!(!app.dirty);
    }

    #[test]
    fn entering_world_view_shows_the_notification_without_editing() {
        let mut app = EditorApp::empty();
        app.quick_notification_test_age = Some(Duration::ZERO);
        app.set_project(Project::new());
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);

        // Warm the world panel and notification fonts before checking the timed overlay.
        app.show_panel(Some(Panel::World));
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
        app.show_panel(None);
        frame(&mut app, &ctx, vec![]);

        app.show_panel(Some(Panel::World));
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(app.world);
        let rect = label(&output, "World view");
        assert!(rect.left() > 425. && rect.top() < 80.);
        assert!(!app.dirty);
    }

    #[test]
    fn function_keys_show_the_changed_layer_name_and_type_without_editing() {
        let mut project = Project::new();
        for (name, kind) in [
            ("Actors", "Entities"),
            ("Walls", "IntGrid"),
            ("Decoration", "Tiles"),
            ("Generated", "AutoLayer"),
        ] {
            project.add_layer(name, kind, 16).unwrap();
        }
        for index in 4..10 {
            project
                .add_layer(&format!("Layer_{index}"), "IntGrid", 16)
                .unwrap();
        }
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.quick_notification_test_age = Some(Duration::ZERO);
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        frame(&mut app, &ctx, vec![]);
        for key in [
            Key::F2,
            Key::F3,
            Key::F4,
            Key::F5,
            Key::F6,
            Key::F7,
            Key::F8,
            Key::F9,
            Key::F10,
            Key::F1,
        ] {
            frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            let output = frame(&mut app, &ctx, vec![]);
            let name = text(
                app.project
                    .as_ref()
                    .unwrap()
                    .layer(app.selected.unwrap(), app.layer)
                    .unwrap(),
                "__identifier",
                "",
            );
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(t)
                if t.galley.text()==name && (t.galley.job.sections[0].format.font_id.size-40.).abs()<0.1 && t.pos.y<80.)), "missing top layer notification for {name}");
            assert_eq!(app.project.as_ref().unwrap().json, original);
            assert!(!app.dirty);
        }
        app.quick_notification_test_age = Some(Duration::from_millis(1199));
        frame(&mut app, &ctx, vec![]);
        assert!(app.quick_notification.is_some());
        app.quick_notification_test_age = Some(Duration::from_millis(1200));
        frame(&mut app, &ctx, vec![]);
        assert!(app.quick_notification.is_none());
        app.switch_layer(app.layer);
        assert!(
            app.quick_notification.is_none(),
            "selecting the current layer must not repeat the notification"
        );
    }
}
