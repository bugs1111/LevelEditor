use super::*;

const MISSING_BACKGROUND: Color32 = Color32::from_rgb(0x79, 0x11, 0x05);

#[derive(Clone, Copy, PartialEq)]
enum FileStatus {
    Current,
    Modified,
    Missing,
}

fn file_status(
    ui: &egui::Ui,
    project: &Project,
    path: &std::path::Path,
    checksum: &str,
) -> FileStatus {
    let id = ui.make_persistent_id(("enum-source-file-status", path));
    if let Some((checked, known_checksum, status)) = ui
        .ctx()
        .data(|data| data.get_temp::<(Instant, String, FileStatus)>(id))
        && checked.elapsed() < std::time::Duration::from_secs(1)
        && known_checksum == checksum
    {
        return status;
    }
    // The panel renders every frame. Only hash a source at the status refresh interval.
    let status = match project.external_enum_source_changed(path) {
        Ok(false) => FileStatus::Current,
        Ok(true) => FileStatus::Modified,
        Err(_) => FileStatus::Missing,
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, (Instant::now(), checksum.to_owned(), status)));
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(1));
    status
}

fn source_icon(
    ui: &mut egui::Ui,
    image: egui::ImageSource<'static>,
    caption: &str,
    fill: Color32,
) -> bool {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(3., 3.);
        source_controls::add_sized_button(
            ui,
            [23., 24.],
            egui::Button::image(egui::Image::new(image).fit_to_exact_size(Vec2::splat(15.)))
                .fill(fill)
                .corner_radius(2.),
        )
    })
    .inner
    .on_hover_text(caption)
    .clicked()
}

impl EditorApp {
    pub(super) fn source_external_enum_group(
        &mut self,
        ui: &mut egui::Ui,
        source: &str,
        definitions: &[Value],
    ) {
        let project = self.project.as_ref().unwrap();
        let Ok(path) = project.make_absolute_file_path(source) else {
            return;
        };
        let status = file_status(
            ui,
            project,
            &path,
            text(&definitions[0], "externalFileChecksum", ""),
        );
        let key = format!(
            "{}_externalEnums_tag_{source}",
            text(&project.json, "iid", "")
        );
        let mut open = self.settings.ui_state_int(&key, None, 1) != 0;
        let name = source.rsplit(['/', '\\']).next().unwrap_or(source);
        let missing = status == FileStatus::Missing;
        ui.push_id(source, |ui| {
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1., theme::LIGHT))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .fill(if missing {
                            MISSING_BACKGROUND
                        } else {
                            GROUP_BAR
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.;
                                let (arrow_rect, toggle) =
                                    ui.allocate_exact_size(egui::vec2(23., 24.), Sense::click());
                                let offsets = if open {
                                    [
                                        egui::vec2(-5., -2.),
                                        egui::vec2(0., 3.),
                                        egui::vec2(5., -2.),
                                    ]
                                } else {
                                    [
                                        egui::vec2(-3., -5.),
                                        egui::vec2(2., 0.),
                                        egui::vec2(-3., 5.),
                                    ]
                                };
                                ui.painter().add(egui::Shape::line(
                                    offsets.map(|offset| arrow_rect.center() + offset).to_vec(),
                                    Stroke::new(2., Color32::WHITE),
                                ));
                                let row_width = (ui.available_width()
                                    - if missing { 48. } else { 72. })
                                .max(12.);
                                let (name_rect, name_response) = ui.allocate_exact_size(
                                    egui::vec2(row_width, 24.),
                                    Sense::click(),
                                );
                                ui.painter_at(name_rect).text(
                                    name_rect.left_center() + egui::vec2(1., 0.),
                                    Align2::LEFT_CENTER,
                                    name,
                                    FontId::new(
                                        13.333,
                                        egui::FontFamily::Name("panel-title".into()),
                                    ),
                                    Color32::WHITE,
                                );
                                let toggle = toggle.clicked() || name_response.clicked();
                                if source_icon(
                                    ui,
                                    egui::include_image!("../../assets/icons/refresh.svg"),
                                    "Reload and synchronize Enums",
                                    theme::MEDIUM,
                                ) {
                                    if missing {
                                        self.retry_resource(ui.ctx(), &path);
                                    } else {
                                        self.begin_enum_sync(ui.ctx(), path.clone());
                                    }
                                }
                                if !missing
                                    && source_icon(
                                        ui,
                                        egui::include_image!("../../assets/icons/locate.svg"),
                                        "Locate file",
                                        theme::MEDIUM,
                                    )
                                {
                                    self.locate_project_path(path.clone(), true);
                                }
                                let remove = source_icon(
                                    ui,
                                    egui::include_image!("../../assets/icons/delete.svg"),
                                    "Remove this external Enum source",
                                    Color32::from_rgb(113, 20, 5),
                                );
                                if toggle || remove {
                                    open = !open;
                                    self.settings.set_ui_state_int(&key, None, i64::from(open));
                                }
                                if remove {
                                    self.pending_enum_removal = Some(source.to_owned());
                                }
                            });
                            let warning = match status {
                                FileStatus::Current => None,
                                FileStatus::Modified => Some("FILE WAS MODIFIED, PLEASE USE SYNC."),
                                FileStatus::Missing => Some("FILE NOT FOUND!"),
                            };
                            if let Some(warning) = warning {
                                egui::Frame::new()
                                    .outer_margin(egui::Margin {
                                        left: 24,
                                        right: 1,
                                        top: 0,
                                        bottom: 1,
                                    })
                                    .fill(SOURCE_ORANGE)
                                    .stroke(Stroke::new(1., Color32::WHITE.gamma_multiply(0.60)))
                                    .corner_radius(3.)
                                    .inner_margin(8)
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(
                                            RichText::new(warning).size(12.).color(Color32::WHITE),
                                        );
                                    });
                            }
                        });
                    if open {
                        egui::Frame::new().inner_margin(12).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 1.;
                            for def in definitions {
                                let uid = def["uid"].as_i64().unwrap();
                                let selected = self.definition_uid == Some(uid);
                                egui::Frame::new()
                                    .fill(if selected {
                                        theme::ACCENT
                                    } else if missing {
                                        MISSING_BACKGROUND
                                    } else {
                                        theme::MEDIUM
                                    })
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 1.;
                                            let (rect, row) = ui.allocate_exact_size(
                                                egui::vec2(
                                                    (ui.available_width() - 21.).max(20.),
                                                    33.,
                                                ),
                                                Sense::click(),
                                            );
                                            ui.painter_at(rect).text(
                                                rect.left_center() + egui::vec2(4., 0.),
                                                Align2::LEFT_CENTER,
                                                text(def, "identifier", "Enum"),
                                                FontId::proportional(14.),
                                                if selected {
                                                    Color32::BLACK
                                                } else {
                                                    Color32::WHITE
                                                },
                                            );
                                            if row.clicked() {
                                                self.definition_uid = Some(uid);
                                                self.clipboard_focus(
                                                    ui.ctx(),
                                                    ClipTarget::Definition {
                                                        kind: "externalEnums".into(),
                                                        uid: Some(uid),
                                                    },
                                                );
                                            }
                                            let remove = |app: &mut Self, ui: &mut egui::Ui| {
                                                if source_controls::button(
                                                    ui,
                                                    "Remove extern source",
                                                )
                                                .clicked()
                                                {
                                                    app.pending_enum_removal =
                                                        Some(source.to_owned());
                                                    ui.close();
                                                }
                                            };
                                            row.context_menu(|ui| remove(self, ui));
                                            source_menu(ui, |ui| remove(self, ui));
                                        });
                                    });
                            }
                        });
                    }
                });
        });
    }

    pub(super) fn source_enum_form(&mut self, ui: &mut egui::Ui) {
        let Some(uid) = self.definition_uid else {
            let green = Color32::from_rgb(101, 178, 117);
            egui::Frame::new()
                .stroke(Stroke::new(1., green))
                .inner_margin(10)
                .corner_radius(4.)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(egui::include_image!("../../assets/icons/help.svg"))
                            .tint(green).fit_to_exact_size(Vec2::splat(40.)));
                        ui.add(egui::Label::new(RichText::new("Enumerations (Enums) are list of values. Examples: your enemy types, items, states etc.").size(16.).color(green)).wrap());
                    });
                });
            return;
        };
        let project = self.project.as_ref().unwrap();
        let Some(def) = project.json["defs"]["externalEnums"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|def| def["uid"] == uid)
            .cloned()
        else {
            self.properties(ui, Panel::Enums);
            return;
        };
        let source = text(&def, "externalRelPath", "");
        let path = project.make_absolute_file_path(source).ok();
        source_controls::form_row(ui, "Source file", "Source file", 110., |ui| {
            let width = (ui.available_width() - 28.).max(20.);
            egui::Frame::new()
                .stroke(Stroke::new(1., theme::LIGHT))
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.set_width((width - 10.).max(10.));
                    ui.label(
                        RichText::new(source)
                            .strong()
                            .size(14.)
                            .color(Color32::WHITE),
                    );
                });
            if source_icon(
                ui,
                egui::include_image!("../../assets/icons/locate.svg"),
                "Locate file",
                theme::MEDIUM,
            ) && let Some(path) = path
            {
                self.locate_project_path(path, true);
            }
        });
        ui.add_space(6.);
        source_controls::form_row_wrapped(ui, "Enum identifier", "identifier", 110., |ui| {
            let mut name = text(&def, "identifier", "");
            ui.add_sized(
                [ui.available_width(), 30.],
                egui::TextEdit::singleline(&mut name).text_color(GROUP_BAR),
            );
        });
        self.source_external_enum_values(ui, def);
    }
}
