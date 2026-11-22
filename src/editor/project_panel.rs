//! Source EditProject form.
use super::*;

#[path = "name_pattern.rs"]
mod name_pattern;

const TILED_EXPORT_DISCLAIMER: &str = "source-tiled-export-disclaimer";

fn project_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let response = if matches!(key, "exportLevelBg" | "pngFilePattern") {
        source_controls::form_row_left_aligned(ui, label, key, 250., body)
    } else {
        source_controls::form_grid_row(ui, label, key, egui::vec2(250., 30.), body)
    };
    ui.add_space(10.);
    response
}

fn project_heading(ui: &mut egui::Ui, name: &str) {
    source_controls::form_heading(ui, name);
    ui.add_space(13.);
}

fn project_checkbox(ui: &mut egui::Ui, values: &Value, patch: &mut Value, key: &str) {
    ui.add_space(4.);
    let mut value = values[key].as_bool().unwrap_or(false);
    if source_checkbox(ui, &mut value, "").changed() {
        patch[key] = json!(value);
    }
}

fn project_locate_button(ui: &mut egui::Ui, is_file: bool) -> bool {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::splat(4.);
        ui.add_sized(
            [30., 30.],
            egui::Button::image(
                egui::Image::new(egui::include_image!("../../assets/icons/locate.svg"))
                    .fit_to_exact_size(Vec2::splat(22.)),
            )
            .fill(theme::LIGHT)
            .stroke(Stroke::NONE)
            .corner_radius(3.),
        )
    })
    .inner
    .on_hover_text(locale::tr(
        ui,
        if is_file {
            "Locate file"
        } else {
            "Locate folder"
        },
    ))
    .clicked()
}

fn project_number(
    ui: &mut egui::Ui,
    project: &Project,
    patch: &mut Value,
    key: &str,
    min: i64,
    max: i64,
) {
    ui.push_id(key, |ui| {
        if let Some(value) = scalar_editor(
            ui,
            project,
            "Int",
            &project.json[key],
            &json!({"min":min,"max":max}),
        ) {
            patch[key] = value;
        }
    });
}

fn project_text(ui: &mut egui::Ui, values: &Value, patch: &mut Value, key: &str) {
    if let Some(value) = ui_fields::buffered_text(ui, key, text(values, key, "")) {
        patch[key] = json!(value);
    }
}

fn project_choice(
    ui: &mut egui::Ui,
    values: &Value,
    patch: &mut Value,
    key: &str,
    choices: &[(&str, &str)],
) -> bool {
    let current = text(values, key, choices[0].0);
    let selected = choices
        .iter()
        .position(|(value, _)| *value == current)
        .unwrap_or(choices.len());
    let options = choices
        .iter()
        .map(|(_, label)| (locale::tr(ui, label), Color32::WHITE))
        .collect::<Vec<_>>();
    let width = if key == "when" {
        ui.available_width().min(164.)
    } else {
        ui.available_width()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.), Sense::click());
    let title = options.get(selected).map_or(current, |option| option.0);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, title));
    ui.painter().rect(
        rect,
        2.,
        theme::DARK,
        Stroke::new(
            1.,
            if response.hovered() {
                theme::ACCENT
            } else {
                theme::LIGHT
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter()
        .with_clip_rect(rect.with_max_x(rect.right() - 18.))
        .text(
            rect.left_center() + egui::vec2(5., 0.),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(13.33),
            ui.visuals().text_color(),
        );
    let arrow = rect.right_center() - egui::vec2(8., 0.);
    ui.painter().add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-3., -1.5),
            arrow + egui::vec2(0., 1.5),
            arrow + egui::vec2(3., -1.5),
        ],
        Stroke::new(1., ui.visuals().text_color()),
    ));
    if let Some((picked, by_enter)) = source_controls::searchable_select_popup(
        &response,
        response.id.with(key),
        selected,
        &options,
    ) && choices[picked].0 != current
    {
        patch[key] = json!(choices[picked].0);
        by_enter
    } else {
        false
    }
}

impl EditorApp {
    pub(super) fn source_project_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let read_only = project.is_backup();
        if read_only && ui.is_enabled() {
            ui.disable();
        }
        let iid = text(&project.json, "iid", "").to_owned();
        let path = project.path.clone();
        let identifier_confirmation = ui.make_persistent_id(("identifier-style", &iid, &path));
        ui.add_space(2.);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            let width = ((ui.available_width() - 77.) / 3.).max(30.);
            let action = |ui: &mut egui::Ui, enabled, image, label: &str| {
                ui.add_enabled_ui(enabled, |ui| {
                    source_controls::add_sized_button(
                        ui,
                        [width, 42.],
                        egui::Button::image_and_text(
                            egui::Image::new(image).fit_to_exact_size(Vec2::splat(16.)),
                            RichText::new(locale::tr(ui, label)).color(Color32::WHITE),
                        )
                        .truncate()
                        .fill(SOURCE_ORANGE),
                    )
                })
                .inner
                .on_hover_text(locale::tr(ui, label))
            };
            if read_only {
                source_controls::add_sized_button(
                    ui,
                    [(ui.available_width() - 44.).max(30.), 42.],
                    egui::Button::new(locale::tr(ui, "RESTORE THIS BACKUP")).fill(theme::DARK),
                );
            }
            for (label, image, save_as) in [
                (
                    "SAVE PROJECT",
                    egui::include_image!("../../assets/icons/save.svg"),
                    false,
                ),
                (
                    "SAVE AS...",
                    egui::include_image!("../../assets/icons/saveAs.svg"),
                    true,
                ),
            ].into_iter().filter(|_| !read_only) {
                if action(ui, !self.is_busy(), image, label).clicked() {
                    self.save(save_as);
                }
            }
            if !read_only && action(
                ui,
                path.is_some() && !self.is_busy(),
                egui::include_image!("../../assets/icons/rename.svg"),
                "RENAME PROJECT",
            )
            .clicked()
                && let Some(path) = &path
            {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new(("project-rename", iid.clone())),
                        path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    );
                });
            }
            if ui
                .add_enabled_ui(path.is_some(), |ui| {
                    source_controls::add_sized_button(
                        ui,
                        [42., 42.],
                        egui::Button::image(
                            egui::Image::new(egui::include_image!("../../assets/icons/locate.svg"))
                                .fit_to_exact_size(Vec2::splat(16.)),
                        ),
                    )
                })
                .inner
                .on_hover_text(locale::tr(ui, "Locate project file"))
                .clicked()
                && let Some(path) = &path
            {
                self.locate_project_path(path.clone(), true);
            }
            if read_only {
                return;
            }
            source_menu(ui, |ui| {
                if source_controls::button(ui, "Browse backups…").clicked() {
                    match self.project.as_ref().unwrap().list_backups() {
                        Ok(backups) => {
                            if let Some(original) = &path {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new(("backup-browser", iid.clone())),
                                        BackupBrowser {
                                            original: original.clone(),
                                            backups,
                                            restore: None,
                                        },
                                    );
                                });
                            }
                        }
                        Err(error) => {
                            self.status = error.to_string();
                            self.status_error = true;
                        }
                    }
                    ui.close();
                }
                if source_controls::button(ui, "Export…").clicked() {
                    self.show_panel(Some(Panel::Export));
                    ui.close();
                }
            });
        });
        ui.add_space(21.);
        project_heading(ui, "Extra files");
        let mut patch = json!({});
        let project = self.project.as_ref().unwrap();
        let values = &project.json;
        let output_dir = project.external_files_directory();
        let mut locate = None;
        project_row(ui, "Super simple export ❤", "simplifiedExport", |ui| {
            project_checkbox(ui, values, &mut patch, "simplifiedExport");
            if values["simplifiedExport"] == true
                && let Some((project_path, dir)) = path.as_ref().zip(output_dir.as_ref())
            {
                let reveal_file = !dir.exists();
                if project_locate_button(ui, reveal_file) {
                    locate = Some(if reveal_file {
                        (project_path.clone(), true)
                    } else {
                        (dir.join("simplified"), false)
                    });
                }
            }
            ui.hyperlink_to(
                RichText::new(locale::tr(ui, "Learn more about this format"))
                    .color(theme::ACCENT)
                    .underline(),
                "https://ldtk.io/go/simple",
            );
        });
        project_row(
            ui,
            "Save levels to separate files",
            "externalLevels",
            |ui| {
                project_checkbox(ui, values, &mut patch, "externalLevels");
                if values["externalLevels"] == true
                    && let Some(dir) = &output_dir
                    && project_locate_button(ui, false)
                {
                    locate = Some((dir.clone(), false));
                }
            },
        );
        if !values["simplifiedExport"].as_bool().unwrap_or(false) {
            let exports_png = text(values, "imageExportMode", "None") != "None";
            project_row(ui, "Export as PNG", "png", |ui| {
                if exports_png
                    && let Some(dir) = &output_dir
                    && project_locate_button(ui, false)
                {
                    locate = Some((dir.join("png"), false));
                }
                project_choice(
                    ui,
                    values,
                    &mut patch,
                    "imageExportMode",
                    &[
                        ("None", "Don't export any image"),
                        ("OneImagePerLayer", "One PNG per layer"),
                        (
                            "OneImagePerLevel",
                            "One PNG per level (layers are merged down)",
                        ),
                        ("LayersAndLevels", "One PNG per layer and one per level."),
                    ],
                );
            });
            if patch.get("imageExportMode").is_some() {
                patch["pngFilePattern"] = Value::Null;
            }
            if exports_png {
                project_row(ui, "Also export level backgrounds", "exportLevelBg", |ui| {
                    project_checkbox(ui, values, &mut patch, "exportLevelBg");
                });
                project_row(ui, "PNG file names", "pngFilePattern", |ui| {
                    let default = if values["imageExportMode"] == "OneImagePerLevel" {
                        "%level_name"
                    } else {
                        "%level_name__%layer_name"
                    };
                    if let Some(change) = name_pattern::show(
                        ui,
                        (&iid, "pngFilePattern"),
                        values["pngFilePattern"].as_str().unwrap_or(default),
                        default,
                        name_pattern::PNG,
                    ) && patch.get("imageExportMode").is_none()
                    {
                        patch["pngFilePattern"] = if change.value == default {
                            Value::Null
                        } else {
                            json!(change.value)
                        };
                    }
                });
            }
        }
        project_row(ui, "Export Tiled TMX", "tiled", |ui| {
            project_checkbox(ui, values, &mut patch, "exportTiled");
            if values["exportTiled"] == true
                && let Some(dir) = &output_dir
                && project_locate_button(ui, false)
            {
                locate = Some((dir.join("tiled"), false));
            }
            ui.hyperlink_to(
                RichText::new(locale::tr(ui, "Learn more about Tiled export limitations"))
                    .color(theme::ACCENT)
                    .underline(),
                "https://ldtk.io/go/tiled",
            );
        });

        project_heading(ui, "Project settings");
        let mut use_ldtk = path.as_ref().is_none_or(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ldtk"))
        });
        let extension_changed = project_row(ui, "Use .ldtk extension", "useAppExtension", |ui| {
            let enabled = path.is_some() && !self.is_busy();
            ui.add_space(4.);
            let response = ui
                .add_enabled_ui(enabled, |ui| source_checkbox(ui, &mut use_ldtk, ""))
                .inner;
            ui.label(locale::tr(ui, "(recommended)"));
            response.changed()
        });
        project_row(ui, "Backup on save", "backup", |ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            project_checkbox(ui, values, &mut patch, "backupOnSave");
            if values["backupOnSave"].as_bool().unwrap_or(false) {
                let custom = values["backupRelPath"].as_str();
                let directory = project.backup_directory();
                let caption = locale::tr(ui, "Max backup files:");
                let font = FontId::proportional(14.667);
                let muted = Color32::from_rgb(156, 157, 161);
                let caption_width = ui
                    .painter()
                    .layout_no_wrap(caption.to_owned(), font.clone(), muted)
                    .size()
                    .x;
                let locate_width = if directory.is_some() { 30. } else { 0. };
                let reset_width = if custom.is_some() { 26. } else { 0. };
                let reserved_width = caption_width + locate_width + reset_width + 32. + 6.;
                let width = ((ui.available_width() - reserved_width) * 0.5).max(44.);
                let directory_label = if custom.is_some() {
                    "[Custom dir]"
                } else {
                    "[Default dir]"
                };
                if source_controls::add_sized_button(
                    ui,
                    [width, 33.],
                    egui::Button::new(
                        RichText::new(locale::tr(ui, directory_label).to_uppercase())
                            .size(13.333)
                            .color(Color32::WHITE),
                    )
                    .fill(theme::LIGHT)
                    .truncate(),
                )
                .on_hover_text(custom.unwrap_or_else(|| locale::tr(ui, "Project backup folder")))
                .clicked()
                {
                    let mut dialog = FileDialog::new();
                    if let Some(dir) = directory
                        .clone()
                        .filter(|dir| dir.is_dir())
                        .or_else(|| {
                            path.as_ref()
                                .and_then(|path| path.parent().map(PathBuf::from))
                        })
                    {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(dir) = dialog.pick_folder() {
                        patch["backupRelPath"] = json!(dir.to_string_lossy());
                    }
                }
                if custom.is_some()
                    && ui
                        .scope(|ui| {
                            ui.spacing_mut().button_padding.x = 2.;
                            source_controls::add_sized_button(
                                ui,
                                [reset_width, 33.],
                                egui::Button::image(
                                    egui::Image::new(egui::include_image!("../../assets/icons/reset.svg"))
                                        .fit_to_exact_size(Vec2::splat(16.)),
                                )
                                .fill(theme::LIGHT),
                            )
                        })
                    .inner
                    .on_hover_text(locale::tr(ui, "Reset destination folder to default"))
                    .clicked()
                {
                    patch["backupRelPath"] = Value::Null;
                }
                if let Some(dir) = directory
                    && project_locate_button(ui, false)
                {
                    locate = Some((dir, false));
                }
                ui.add_space(3.);
                ui.label(RichText::new(caption).font(font).color(muted));
                ui.add_space(3.);
                let limit = values["backupLimit"].as_i64().unwrap_or(10).to_string();
                if let Some(draft) = source_controls::number_input_sized(
                    ui,
                    "backupLimit",
                    &limit,
                    source_controls::NumberStyle::Plain { placeholder: None },
                    width,
                ) && let Ok(value) = draft.trim().parse::<i64>()
                {
                    patch["backupLimit"] = json!(value.clamp(3, 50));
                }
            } else if project_dialogs::recommends_backups(project) {
                ui.add_space(3.);
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.), Sense::hover());
                let red = Color32::from_rgb(227, 77, 0);
                ui.painter().add(
                    egui::epaint::Shadow {
                        offset: [0, 0],
                        blur: 10,
                        spread: 0,
                        color: red.gamma_multiply(0.8),
                    }
                    .as_shape(rect, 14.),
                );
                ui.painter().circle(
                    rect.center(),
                    12.5,
                    Color32::from_rgb(121, 17, 5),
                    Stroke::new(3., red),
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "!",
                    FontId::new(20., egui::FontFamily::Name("heading".into())),
                    theme::ACCENT,
                );
                source_controls::help_tooltip(
                    response.on_hover_cursor(egui::CursorIcon::Help),
                    "It's recommended to enable backups for large or important projects.",
                    false,
                );
            }
        });
        if values.get("jsonStyle").is_some() {
            project_row(ui, "JSON indent style", "jsonStyle", |ui| {
                project_choice(
                    ui,
                    values,
                    &mut patch,
                    "jsonStyle",
                    &[
                        ("Minified", "MINIFIED (minimal file size, no indentation)"),
                        (
                            "Compact",
                            "COMPACT (mostly indented, but small arrays are inlined)",
                        ),
                        ("Full", "FULL (fully indented)"),
                    ],
                );
            });
        } else {
            project_row(ui, "Minify JSON", "minify", |ui| {
                project_checkbox(ui, values, &mut patch, "minifyJson");
            });
        }
        let identifier_confirmed = project_row(
            ui,
            "Identifers format convention",
            "identifierStyle",
            |ui| {
                project_choice(
                    ui,
                    values,
                    &mut patch,
                    "identifierStyle",
                    &[
                        (
                            "Capitalize",
                            "\"My_identifier_1\" -- First letter is always uppercase, the rest is up to you",
                        ),
                        ("Uppercase", "\"MY_IDENTIFIER_1\" -- Full uppercase"),
                        ("Lowercase", "\"my_identifier_1\" -- Full lowercase"),
                        (
                            "Free",
                            "\"my_IdEnTifIeR_1\" -- I wON't cHaNge yOuR leTteR caSe",
                        ),
                    ],
                )
            },
        );
        // The preview's Enter event also confirms the warning opened by the selector.
        let chosen_style = if identifier_confirmed {
            None
        } else {
            patch
                .as_object_mut()
                .and_then(|patch| patch.remove("identifierStyle"))
        };
        let opened_confirmation = chosen_style.is_some();
        if let Some(style) = chosen_style {
            ui.ctx()
                .data_mut(|data| data.insert_temp(identifier_confirmation, style));
        }
        let tiled_enabled = patch["exportTiled"] == true;
        let output_setting_changed = patch.get("externalLevels").is_some()
            || patch["simplifiedExport"] == true
            || patch["imageExportMode"]
                .as_str()
                .is_some_and(|mode| mode != "None");
        let applied = self.apply_source_project_patch(patch);
        let show_tiled_disclaimer = applied && tiled_enabled;
        if show_tiled_disclaimer {
            ui.ctx()
                .data_mut(|data| data.insert_temp(egui::Id::new(TILED_EXPORT_DISCLAIMER), true));
        } else if applied && output_setting_changed {
            self.recommend_saving_project();
        }
        if let Some((path, select)) = locate {
            self.locate_project_path(path, select);
        }

        project_heading(ui, "User custom commands");
        project_row(ui, "Custom commands", "Custom commands", |ui| {
            ui.vertical(|ui| {
                self.source_project_commands(ui);
                ui.add_space(5.);
            });
        });

        project_heading(ui, "Workspace settings");
        let project = self.project.as_ref().unwrap();
        let values = &project.json;
        let mut patch = json!({});
        project_row(ui, "Default layer grid", "defaultGridSize", |ui| {
            project_number(ui, project, &mut patch, "defaultGridSize", 1, 1024);
            ui.label("px");
        });
        project_row(ui, "Default entity size", "Default entity size", |ui| {
            project_number(ui, project, &mut patch, "defaultEntityWidth", 1, 65536);
            ui.label("×");
            project_number(ui, project, &mut patch, "defaultEntityHeight", 1, 65536);
            ui.label("px");
        });
        let reset_level_name = project_row(ui, "Default level identifiers", "levelName", |ui| {
            if let Some(change) = name_pattern::show(
                ui,
                (&iid, "levelNamePattern"),
                values["levelNamePattern"]
                    .as_str()
                    .unwrap_or(name_pattern::LEVEL_DEFAULT),
                name_pattern::LEVEL_DEFAULT,
                name_pattern::LEVEL,
            ) {
                patch["levelNamePattern"] = json!(change.value);
                change.reset
            } else {
                false
            }
        });
        for (key, label, help) in [
            ("bgColor", "Workspace background color", "bgColor"),
            (
                "defaultLevelBgColor",
                "Default level bg color",
                "defaultLevelbgColor",
            ),
        ] {
            project_row(ui, label, help, |ui| {
                ui.push_id(key, |ui| {
                    let color = theme::color(text(values, key, "#FFFFFF"));
                    let mut rgb = [color.r(), color.g(), color.b()];
                    if source_colors::color_editor_tag(
                        ui,
                        &mut rgb,
                        if key == "bgColor" { Some("bg") } else { None },
                    )
                    .changed()
                    {
                        patch[key] = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                    }
                });
            });
        }
        project_row(
            ui,
            "Default pivot point for new entities",
            "Default pivot point for new entities",
            |ui| {
                if let Some(pivot) = ui_fields::pivot_editor(
                    ui,
                    "project-default-pivot",
                    [
                        values["defaultPivotX"].as_f64().unwrap_or(0.) as f32,
                        values["defaultPivotY"].as_f64().unwrap_or(0.) as f32,
                    ],
                    [
                        values["defaultEntityWidth"].as_f64().unwrap_or(16.) as f32,
                        values["defaultEntityHeight"].as_f64().unwrap_or(16.) as f32,
                    ],
                ) {
                    patch["defaultPivotX"] = json!(pivot[0]);
                    patch["defaultPivotY"] = json!(pivot[1]);
                }
            },
        );
        if changed(&patch)
            && self
                .edit("Workspace settings changed", |project| {
                    apply_patch(&mut project.json, &patch);
                    project.refresh_derived_data()
                })
                .is_some()
            && reset_level_name
        {
            self.report_canvas_notice(notifications::CanvasNotice::ValueReset);
        }
        ui.add_space(16.);
        self.source_project_advanced(ui);
        if read_only {
            return;
        }
        self.backup_project_dialogs(ui);
        self.confirm_source_identifier_style(
            ui.ctx(),
            identifier_confirmation,
            opened_confirmation,
        );
        self.tiled_export_disclaimer(ui.ctx(), show_tiled_disclaimer);
        if extension_changed {
            self.change_project_extension(use_ldtk);
        }
    }

    pub(in crate::app) fn tiled_export_disclaimer_open(ctx: &egui::Context) -> bool {
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new(TILED_EXPORT_DISCLAIMER)))
            .unwrap_or(false)
    }

    fn tiled_export_disclaimer(&mut self, ctx: &egui::Context, just_opened: bool) {
        if !Self::tiled_export_disclaimer_open(ctx) {
            return;
        }
        let id = egui::Id::new(TILED_EXPORT_DISCLAIMER);
        let modal = egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 42.).clamp(0., 458.);
                ui.set_width(width);
                let icon_width = 91_f32.min(width * 0.25);
                let text_width = (width - icon_width - 16.).max(1.);
                let font = FontId::proportional(14.667);
                let message = ui.painter().layout(
                    locale::tr(ui, "Disclaimer: Tiled export is only meant to load your LDtk project in a game framework that only supports Tiled files. It is recommended to write your own LDtk JSON parser, as some LDtk features may not be supported.").into(),
                    font.clone(), Color32::WHITE, text_width,
                );
                let footer = ui.painter().layout(
                    locale::tr(ui, "It's not so complicated, I promise :)").into(),
                    font, Color32::WHITE, text_width,
                );
                let height = message.size().y + footer.size().y + 14.667;
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
                let icon = Rect::from_min_size(rect.min, egui::vec2(icon_width, height));
                ui.painter().rect_filled(icon, 8., Color32::from_black_alpha(230));
                egui::Image::new(egui::include_image!("../../assets/icons/projectSettings.png"))
                    .paint_at(ui, Rect::from_center_size(
                        icon.center_top() + egui::vec2(0., 46.), Vec2::splat(60.).min(icon.size()),
                    ));
                let text = rect.min + egui::vec2(icon_width + 16., 0.);
                let footer_pos = text + egui::vec2(0., message.size().y + 14.667);
                ui.painter().galley(text, message, Color32::WHITE);
                ui.painter().galley(footer_pos, footer, Color32::WHITE);
                ui.add_space(14.);
                project_dialogs::button(ui, "CLOSE", egui::vec2(width, 35.), false, 13.333).clicked()
            });
        if modal.inner
            || !just_opened
                && (modal.should_close()
                    || modal.is_top_modal
                        && ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        }))
        {
            ctx.data_mut(|data| data.remove::<bool>(id));
            self.recommend_saving_project();
        }
    }

    fn confirm_source_identifier_style(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        just_opened: bool,
    ) {
        let Some(style) = ctx.data(|data| data.get_temp::<Value>(id)) else {
            return;
        };
        let modal = project_dialogs::form_warning_dialog(id)
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 42.).clamp(0., 358.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 14.667);
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(14.667));
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Button, FontId::proportional(13.333));
                ui.visuals_mut().override_text_color = Some(Color32::WHITE);
                ui.label(locale::tr(ui, "WARNING!"));
                ui.label(locale::tr(ui, "Please make sure the game engine or importer you're using supports this kind of LDtk identifier!"));
                if style != "Free" {
                    ui.label(locale::tr(ui, "If you proceed, all identifiers in this project will be converted to the new format!"));
                }
                ui.label(locale::tr(ui, "Are you sure?"));
                ui.horizontal(|ui| {
                    let size = [(ui.available_width() - 2.) * 0.5, 35.];
                    let confirm = source_controls::add_sized_button(
                        ui, size, egui::Button::new(locale::tr(ui, "CONFIRM")).fill(SOURCE_ORANGE),
                    ).clicked();
                    let cancel = source_controls::add_sized_button(
                        ui, size, egui::Button::new(locale::tr(ui, "CANCEL")).fill(theme::LIGHT),
                    ).clicked();
                    (confirm, cancel)
                }).inner
            });
        let (confirm, cancel) = modal.inner;
        let cancel = cancel || modal.should_close();
        let confirm = confirm
            || (!just_opened
                && modal.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)));
        if confirm || cancel {
            ctx.data_mut(|data| data.remove::<Value>(id));
        }
        if confirm && !cancel {
            self.apply_source_project_patch(json!({"identifierStyle": style}));
        }
    }

    pub(super) fn locate_project_path(&mut self, path: PathBuf, select: bool) {
        let (target, select) = if select && !path.exists() {
            (path.parent().unwrap_or(&path), false)
        } else {
            (path.as_path(), select)
        };
        if !target.exists() {
            self.status = format!("Could not locate {}.", target.display());
            self.status_error = true;
            self.report_canvas_notice(notifications::CanvasNotice::LocateMissingPath);
            return;
        }
        match home_files::locate(target, select) {
            Ok(()) => {
                self.status = "Locating file...".into();
                self.status_error = false;
                self.report_canvas_notice(notifications::CanvasNotice::LocatingFile);
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }

    fn apply_source_project_patch(&mut self, patch: Value) -> bool {
        if !changed(&patch) {
            return false;
        }
        let operation = |project: &mut Project| {
            if let Some(enabled) = patch["externalLevels"].as_bool() {
                project.set_external_levels(enabled)?;
            }
            let mut patch = patch.clone();
            if let Some(style) = patch["identifierStyle"].as_str().map(str::to_owned) {
                project.apply_identifier_style(&style)?;
                patch.as_object_mut().unwrap().remove("identifierStyle");
            }
            relative_path_patch(project, &mut patch, "backupRelPath")?;
            apply_patch(&mut project.json, &patch);
            Ok(())
        };
        if patch.get("identifierStyle").is_some() {
            self.edit_recoverable("Identifier style changed", operation)
                .is_some()
        } else {
            self.edit("Project settings changed", operation).is_some()
        }
    }

    fn source_project_commands(&mut self, ui: &mut egui::Ui) {
        let project = self.project.as_ref().unwrap();
        let iid = text(&project.json, "iid", "").to_owned();
        let mut trust = self.settings.trust_state(&iid);
        if let Some(trusted) = trust {
            let clear = egui::Frame::new()
                .stroke(Stroke::new(
                    1_f32,
                    if trusted {
                        Color32::from_rgb(82, 162, 84)
                    } else {
                        Color32::from_rgb(177, 67, 67)
                    },
                ))
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.label(locale::tr(
                        ui,
                        if trusted {
                            "This project is allowed to run user commands on your computer."
                        } else {
                            "This project is NOT allowed to run user commands on your computer."
                        },
                    ));
                    ui.link(locale::tr(
                        ui,
                        if trusted {
                            "Remove authorization"
                        } else {
                            "Unblock it"
                        },
                    ))
                    .clicked()
                })
                .inner;
            if clear {
                self.settings.clear_trust(&iid);
                trust = None;
            }
        }
        let mut commands = self.project.as_ref().unwrap().json["customCommands"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut modified = false;
        let mut delete = None;
        for (index, command) in commands.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let mut patch = json!({});
                    project_choice(
                        ui,
                        command,
                        &mut patch,
                        "when",
                        &[
                            (
                                "Manual",
                                if cfg!(target_os = "macos") {
                                    "Run manually (CMD-R)"
                                } else {
                                    "Run manually (CTRL-R)"
                                },
                            ),
                            ("AfterLoad", "Run after loading"),
                            ("BeforeSave", "Run before saving"),
                            ("AfterSave", "Run after saving"),
                        ],
                    );
                    project_text(ui, command, &mut patch, "command");
                    if changed(&patch) {
                        apply_patch(command, &patch);
                        modified = true;
                    }
                    if source_controls::add_enabled_button(
                        ui,
                        trust != Some(false),
                        egui::Button::new(locale::tr(ui, "Run")),
                    )
                    .on_hover_text(locale::tr(ui, "Run command"))
                    .clicked()
                    {
                        let cwd = self
                            .project
                            .as_ref()
                            .unwrap()
                            .path
                            .as_ref()
                            .and_then(|p| p.parent())
                            .map(PathBuf::from);
                        self.run_command(text(command, "command", "").to_owned(), cwd);
                    }
                    if source_controls::small_button(ui, "×")
                        .on_hover_text(locale::tr(ui, "Delete command"))
                        .clicked()
                    {
                        delete = Some(index);
                    }
                });
            });
        }
        if let Some(index) = delete {
            commands.remove(index);
            modified = true;
        }
        let add_label = locale::tr(ui, "Add command");
        let add = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding.y = 4.;
                source_controls::add_sized_button(
                    ui,
                    [ui.available_width(), 30.],
                    egui::Button::image(
                        egui::Image::new(egui::include_image!("../../assets/icons/add.svg"))
                            .fit_to_exact_size(Vec2::splat(22.)),
                    )
                    .fill(SOURCE_ORANGE),
                )
            })
            .inner
            .on_hover_text(add_label);
        add.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, add_label));
        if add.clicked() {
            commands.push(json!({"when":"Manual","command":""}));
            modified = true;
        }
        if modified {
            let operation = |project: &mut Project| {
                project.json["customCommands"] = json!(commands);
                Ok(())
            };
            if delete.is_some() {
                self.edit_recoverable("Project command removed", operation);
            } else {
                self.edit("Custom commands changed", operation);
            }
        }
    }

    fn source_project_advanced(&mut self, ui: &mut egui::Ui) {
        let options = [
            (
                "MultiWorlds",
                "Multi-worlds support",
                "Stores levels in a 'worlds' array at the root of the project JSON. Experimental support for embedded levels.",
            ),
            (
                "PrependIndexToLevelFileNames",
                "Prefix level file names with their index in array",
                "Prefixes external level file names with their position in the array. Inserting a level then renames every subsequent file. This was the default before LDtk 1.0.0.",
            ),
            (
                "ExportPreCsvIntGridFormat",
                "Export legacy pre-CSV IntGrid layers data",
                "Includes the deprecated \"intGrid\" array in exported JSON. Use this if your game API requires LDtk 0.8.x or earlier.",
            ),
            (
                "UseMultilinesType",
                "Use \"Multilines\" instead of \"String\" for fields in JSON",
                "Uses \"Multilines\" as the \"__type\" of multiline field definitions and instances.",
            ),
            (
                "ExportOldTableOfContentData",
                "Export old entity table-of-content data",
                "Fills the legacy 'instances' array in each 'toc' entry alongside 'instancesData'.",
            ),
        ];
        let project = &self.project.as_ref().unwrap().json;
        let flags = project["flags"].as_array().cloned().unwrap_or_default();
        let has_extra_worlds = project["worlds"]
            .as_array()
            .is_some_and(|worlds| worlds.len() > 1);
        egui::CollapsingHeader::new(locale::tr(ui, "Advanced project settings"))
            .id_salt("Advanced project settings")
            .default_open(options.iter().any(|(flag, _, _)| flags.iter().any(|value| value == *flag)))
            .show(ui, |ui| {
                ui.label(locale::tr(ui, "Advanced options are not recommended for most users as they might break compatibility with older game APIs. Please make sure you know what you're doing before using any of them!"));
                for (flag, label, description) in options {
                    ui.add_space(6.);
                    let mut enabled = flags.contains(&json!(flag));
                    let response = ui
                        .add_enabled_ui(flag != "MultiWorlds" || !has_extra_worlds, |ui| {
                            source_checkbox(ui, &mut enabled, label)
                        })
                        .inner;
                    ui.horizontal(|ui| {
                        ui.add_space(32.);
                        ui.add(egui::Label::new(RichText::new(locale::tr(ui, description)).size(12.)).wrap());
                    });
                    if response.changed() {
                        if flag == "MultiWorlds" {
                            let updated = if enabled {
                                self.edit("Multiple worlds enabled", Project::enable_multiworlds)
                            } else {
                                self.edit("Multiple worlds disabled", Project::disable_multiworlds)
                            };
                            if updated == Some(true) {
                                self.active_world = enabled.then_some(0);
                                if let Some(key) = &mut self.selected {
                                    key.world = self.active_world;
                                }
                            }
                            continue;
                        }
                        self.edit("Project option changed", |project| {
                            let flags = project.json["flags"].as_array_mut().ok_or_else(|| ldtk_core::Error::Invalid("flags must be an array".into()))?;
                            flags.retain(|value| value != flag);
                            if enabled { flags.push(json!(flag)); }
                            Ok(())
                        });
                    }
                }
                let patch = form(ui, &self.project.as_ref().unwrap().json, &[
                    P("tutorialDesc", "Tutorial description", W::Multiline),
                ]);
                if changed(&patch) {
                    self.edit("Workspace settings changed", |project| {
                        apply_patch(&mut project.json, &patch);
                        project.refresh_derived_data()
                    });
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> (EditorApp, egui::Context) {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.repaint = Some(ctx.clone());
        (app, ctx)
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
                    egui::vec2(850., 1800.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                if app.panel == Some(Panel::Project) {
                    app.shortcuts(ctx);
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_max_width(800.);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    app.source_project_form(ui);
                });
            },
        )
    }

    fn label(output: &egui::FullOutput, expected: &str) -> Option<Rect> {
        fn find(shape: &egui::Shape, expected: &str) -> Option<Rect> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text.replace('\n', " ") == expected => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, expected)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|s| find(&s.shape, expected))
    }

    #[test]
    fn source_project_actions_share_the_first_row_and_sections_follow_the_template() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, vec![]);
        let save = label(&output, "SAVE PROJECT").unwrap();
        for action in ["SAVE AS...", "RENAME PROJECT"] {
            let other = label(&output, action).unwrap_or_else(|| panic!("missing {action}"));
            assert!((other.center().y - save.center().y).abs() < 1.);
        }
        let headings = [
            "EXTRA FILES",
            "PROJECT SETTINGS",
            "USER CUSTOM COMMANDS",
            "WORKSPACE SETTINGS",
            "Advanced project settings",
        ];
        let mut previous = save.bottom();
        for heading in headings {
            let rect = label(&output, heading).unwrap_or_else(|| panic!("missing {heading}"));
            assert!(rect.top() > previous, "{heading} is out of source order");
            previous = rect.bottom();
        }
        label(&output, "Default entity size").expect("source paired entity dimensions");
        let separate_files = label(&output, "Save levels to separate files").unwrap();
        let simple_export = label(&output, "Super simple export ❤").unwrap();
        let checkbox_step = separate_files.center().y - simple_export.center().y;
        assert_eq!(
            checkbox_step, 40.,
            "the source form uses a 30px row and a 10px gap"
        );
        let png = label(&output, "Export as PNG").unwrap();
        let row_step = png.center().y - separate_files.center().y;
        assert!((37. ..=42.).contains(&row_step), "row step is {row_step}px");
        let commands = label(&output, "Custom commands").unwrap();
        let add_command = egui::pos2(400., commands.center().y);
        fn action_rect(shape: &egui::Shape, center: egui::Pos2) -> Option<Rect> {
            match shape {
                egui::Shape::Rect(rect)
                    if rect.fill == SOURCE_ORANGE && rect.rect.contains(center) =>
                {
                    Some(rect.rect)
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| action_rect(shape, center))
                }
                _ => None,
            }
        }
        let add_rect = output
            .shapes
            .iter()
            .find_map(|shape| action_rect(&shape.shape, add_command))
            .expect("orange add command button");
        assert!(add_rect.width() > 500., "add command fills the value column");
    }

    #[test]
    fn project_form_aligns_labels_and_spaces_sections_like_the_preview() {
        let (mut app, ctx) = editor();
        let values = &mut app.project.as_mut().unwrap().json;
        values["backupOnSave"] = json!(false);
        values["jsonStyle"] = json!("Full");
        let output = frame(&mut app, &ctx, vec![]);
        let bounds = |name| label(&output, name).unwrap();
        let json = bounds("JSON indent style");
        let external = bounds("Save levels to separate files");
        assert!((json.right() - external.right()).abs() <= 1.);

        let section_top = |name| bounds(name).top();
        for (before, after, distance) in [
            ("EXTRA FILES", "PROJECT SETTINGS", 217.),
            ("PROJECT SETTINGS", "USER CUSTOM COMMANDS", 217.),
            ("USER CUSTOM COMMANDS", "WORKSPACE SETTINGS", 102.),
        ] {
            let actual = section_top(after) - section_top(before);
            assert!(
                (actual - distance).abs() <= 1.,
                "{before} to {after}: {actual}"
            );
        }
    }

    #[test]
    fn backup_row_warns_at_eight_levels_and_expands_its_controls_when_enabled() {
        let (mut app, ctx) = editor();
        let dir = tempfile::tempdir().unwrap();
        let project = app.project.as_mut().unwrap();
        project.save(dir.path().join("Backup.ldtk")).unwrap();
        project.json["backupOnSave"] = json!(false);
        project.json["flags"] = json!(["IgnoreBackupSuggest"]);
        for index in 1..7 {
            project
                .add_level(None, &format!("Level_{index}"), 256, 256)
                .unwrap();
        }
        assert!(label(&frame(&mut app, &ctx, vec![]), "!").is_none());
        app.project
            .as_mut()
            .unwrap()
            .add_level(None, "Eighth", 256, 256)
            .unwrap();
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "!").is_some());

        let backup = label(&output, "Backup on save").unwrap();
        click(&mut app, &ctx, egui::pos2(282., backup.center().y + 3.));
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "!").is_none());
        assert!(label(&output, "[DEFAULT DIR]").is_some());
        let count_label = label(&output, "Max backup files:").unwrap();
        let offset = count_label.left() - backup.right();
        assert!(
            offset > 250.,
            "backup directory and count field share the available width: {offset}"
        );
        assert!(count_label.right() < label(&output, "10").unwrap().left());
    }

    #[test]
    fn source_project_shows_only_the_active_export_and_backup_options() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["backupOnSave"] = json!(false);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "PNG file names").is_none());
        assert!(label(&output, "Also export level backgrounds").is_none());
        assert!(label(&output, "Max backup files:").is_none());

        let json = &mut app.project.as_mut().unwrap().json;
        json["imageExportMode"] = json!("LayersAndLevels");
        json["backupOnSave"] = json!(true);
        let output = frame(&mut app, &ctx, vec![]);
        let export_heading = label(&output, "EXTRA FILES").unwrap();
        let png_names = label(&output, "PNG file names").expect("PNG pattern when export is enabled");
        assert!((png_names.left() - export_heading.left()).abs() <= 1.);
        label(&output, "LevelName").expect("effective default PNG naming pattern");
        label(&output, "LayerName").expect("effective default PNG naming pattern");
        let backgrounds = label(&output, "Also export level backgrounds").expect("PNG backgrounds");
        assert!((backgrounds.left() - export_heading.left()).abs() <= 1.);
        label(&output, "Max backup files:").expect("backup limit when backups are enabled");

        app.project.as_mut().unwrap().json["simplifiedExport"] = json!(true);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "Export as PNG").is_none());
        assert!(label(&output, "PNG file names").is_none());
    }

    #[test]
    fn project_tiled_export_shows_the_disclaimer_before_the_save_reminder() {
        let (mut app, ctx) = editor();
        let initial = frame(&mut app, &ctx, vec![]);
        let tiled = label(&initial, "Export Tiled TMX").unwrap();
        click(&mut app, &ctx, egui::pos2(289., tiled.center().y));
        assert_eq!(app.project.as_ref().unwrap().json["exportTiled"], true);
        assert!(app.canvas_notices.is_empty());
        let dialog = frame(&mut app, &ctx, vec![]);
        assert!(label(&dialog, "It's not so complicated, I promise :)").is_some());
        let close = label(&dialog, "CLOSE").expect("Tiled disclaimer close button");
        click(&mut app, &ctx, close.center());
        assert_eq!(app.project.as_ref().unwrap().json["exportTiled"], true);
        assert_eq!(app.canvas_notices.len(), 1);
        let dialog = frame(&mut app, &ctx, vec![]);
        assert!(label(&dialog, "It's not so complicated, I promise :)").is_none());
    }

    #[test]
    fn project_png_mode_change_resets_the_pattern_and_prompts_to_save() {
        let (mut app, ctx) = editor();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.v1.ldtk");
        app.project.as_mut().unwrap().save(&path).unwrap();
        let doc = &mut app.project.as_mut().unwrap().json;
        doc["imageExportMode"] = json!("OneImagePerLayer");
        doc["pngFilePattern"] = json!("Custom_%level_name");
        let initial = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&initial, "One PNG per layer").unwrap().center(),
        );
        frame(&mut app, &ctx, vec![]);
        let menu = frame(&mut app, &ctx, vec![egui::Event::Text("merged".into())]);
        assert!(label(&menu, "One PNG per level (layers are merged down)").is_some());
        assert!(label(&menu, "Don't export any image").is_none());
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let doc = &app.project.as_ref().unwrap().json;
        assert_eq!(doc["imageExportMode"], "OneImagePerLevel");
        assert!(doc["pngFilePattern"].is_null());
        assert!(matches!(
            app.canvas_notices.last().map(|entry| &entry.notice),
            Some(notifications::CanvasNotice::ProjectFileSettingChanged)
        ));

        let output = frame(&mut app, &ctx, vec![]);
        let png = label(&output, "Export as PNG").unwrap();
        click(&mut app, &ctx, egui::pos2(293., png.center().y));
        assert!(app.status_error);
        assert!(
            app.status
                .contains(&*dir.path().join("Map.v1/png").to_string_lossy())
        );
        assert!(matches!(
            app.canvas_notices.last().map(|entry| &entry.notice),
            Some(notifications::CanvasNotice::LocateMissingPath)
        ));
        assert!(!dir.path().join("Map.v1").exists());
    }

    #[test]
    fn source_project_locate_separate_levels_reports_the_missing_output_folder() {
        let (mut app, ctx) = editor();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.v1.ldtk");
        app.project.as_mut().unwrap().save(&path).unwrap();
        let out = frame(&mut app, &ctx, vec![]);
        let row = label(&out, "Save levels to separate files").unwrap();
        let locate = egui::pos2(310., row.center().y);
        click(&mut app, &ctx, locate);
        assert!(!app.status_error);

        app.project.as_mut().unwrap().json["externalLevels"] = json!(true);
        frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, locate);
        assert!(app.status_error);
        assert!(
            app.status
                .contains(&*dir.path().join("Map.v1").to_string_lossy())
        );
        assert_eq!(app.project.as_ref().unwrap().json["externalLevels"], true);
        assert!(!dir.path().join("Map.v1").exists());
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
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    #[test]
    fn source_project_extension_switch_renames_the_file_without_saving_edits() {
        let (mut app, ctx) = editor();
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("World.ldtk");
        let new = dir.path().join("World.json");
        let mut project = Project::new();
        let key = project.add_level(None, "Room", 64, 64).unwrap();
        project.save(&old).unwrap();
        let disk_before = std::fs::read(&old).unwrap();
        app.set_project(project);
        app.navigate_level(key, false);
        app.edit("Change description", |p| {
            p.json["tutorialDesc"] = json!("Unsaved");
            Ok(())
        });
        let out = frame(&mut app, &ctx, vec![]);
        let row = label(&out, "Use .ldtk extension").unwrap();
        let pos = egui::pos2(278., row.center().y);
        click(&mut app, &ctx, pos);

        assert!(!app.status_error, "{}", app.status);
        assert!(!old.exists());
        assert_eq!(std::fs::read(&new).unwrap(), disk_before);
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.path.as_deref(), Some(new.as_path()));
        assert_eq!(project.json["tutorialDesc"], "Unsaved");
        assert!(app.dirty);
        assert_eq!(app.selected, Some(key));
        assert_eq!(app.settings.value["recentProjects"], json!([new]));
        assert!(
            app.file_snapshot
                .as_ref()
                .unwrap()
                .changed_files()
                .unwrap()
                .is_empty()
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert!(app.project.as_ref().unwrap().json["tutorialDesc"].is_null());
    }

    #[test]
    fn source_project_can_disable_multiworlds_and_keep_the_selected_level() {
        let (mut app, ctx) = editor();
        let mut project = Project::new();
        project.enable_multiworlds().unwrap();
        project.add_layer("First", "IntGrid", 16).unwrap();
        project.add_layer("Second", "IntGrid", 16).unwrap();
        let selected = project.add_level(Some(0), "Room", 64, 64).unwrap();
        let iid = project.level(selected).unwrap()["iid"].clone();
        app.set_project(project);
        app.navigate_level(selected, false);
        app.layer = 1;
        assert_eq!(
            app.edit_level("Paint cell", selected, |p| {
                p.paint_int_grid(selected, 1, 0, 0, 1)
            }),
            Some(true)
        );
        let out = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&out, "Multi-worlds support").unwrap().center(),
        );

        let key = LevelKey {
            world: None,
            level: selected.level,
        };
        assert_eq!(app.selected, Some(key));
        assert_eq!(app.active_world, None);
        assert_eq!(app.layer, 1);
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.level(key).unwrap()["iid"], iid);
        assert_eq!(project.json["worlds"], json!([]));
        assert_eq!(project.json["flags"], json!([]));
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(key, 1).unwrap()["intGridCsv"][0],
            0
        );
        assert_eq!(
            app.edit_level("Paint cell", key, |p| p.paint_int_grid(key, 1, 1, 0, 1)),
            Some(true)
        );
        let out = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&out, "Multi-worlds support").unwrap().center(),
        );
        assert_eq!(app.selected, Some(selected));
        assert_eq!(app.active_world, Some(0));
        assert_eq!(app.layer, 1);
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(selected, 1).unwrap()["intGridCsv"][1],
            0
        );
    }

    #[test]
    fn source_project_disables_the_multiworlds_checkbox_for_multiple_worlds() {
        let (mut app, ctx) = editor();
        let mut project = Project::new();
        project.add_world("ExtraWorld").unwrap();
        let before = project.json.clone();
        app.set_project(project);
        let out = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&out, "Multi-worlds support").unwrap().center(),
        );
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.status_error);
    }

    #[test]
    fn source_project_advanced_shows_user_options_and_keeps_internal_flags() {
        let (mut app, ctx) = editor();
        let internal = json!([
            "IgnoreBackupSuggest",
            "ExportTileIds",
            "DiscardPreCsvIntGrid"
        ]);
        app.project.as_mut().unwrap().json["flags"] = internal.clone();

        let out = frame(&mut app, &ctx, vec![]);
        assert!(label(&out, "Multi-worlds support").is_none());
        click(
            &mut app,
            &ctx,
            label(&out, "Advanced project settings").unwrap().center(),
        );
        let out = frame(&mut app, &ctx, vec![]);
        for name in internal.as_array().unwrap() {
            assert!(label(&out, name.as_str().unwrap()).is_none());
        }
        click(
            &mut app,
            &ctx,
            label(&out, "Export old entity table-of-content data")
                .unwrap()
                .center(),
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["flags"],
            json!([
                "IgnoreBackupSuggest",
                "ExportTileIds",
                "DiscardPreCsvIntGrid",
                "ExportOldTableOfContentData"
            ])
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json["flags"], internal);
    }

    #[test]
    fn preview_154_json_style_picker_is_undoable() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["jsonStyle"] = json!("Full");
        frame(&mut app, &ctx, vec![]);
        let out = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(&out, "FULL (fully indented)").unwrap().center(),
        );
        frame(&mut app, &ctx, vec![]);
        let filtered = frame(&mut app, &ctx, vec![egui::Event::Text("compact".into())]);
        assert!(label(&filtered, "COMPACT (mostly indented, but small arrays are inlined)").is_some());
        assert!(label(&filtered, "MINIFIED (minimal file size, no indentation)").is_none());
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.project.as_ref().unwrap().json["jsonStyle"], "Compact");
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json["jsonStyle"], "Full");
    }

    #[test]
    fn source_project_identifier_style_confirms_before_renaming() {
        let (mut app, ctx) = editor();
        app.panel = Some(Panel::Project);
        app.project
            .as_mut()
            .unwrap()
            .add_entity_def("My_entity", 16, 16)
            .unwrap();
        let before = app.project.as_ref().unwrap().json.clone();
        let choose_lowercase = |app: &mut EditorApp| {
            let out = frame(app, &ctx, vec![]);
            click(
                app,
                &ctx,
                label(&out, "\"My_identifier_1\" -- First letter is always uppercase, the rest is up to you")
                    .unwrap()
                    .center(),
            );
            let out = frame(app, &ctx, vec![]);
            click(
                app,
                &ctx,
                label(&out, "\"my_identifier_1\" -- Full lowercase")
                    .unwrap()
                    .center(),
            );
        };

        frame(&mut app, &ctx, vec![]);
        choose_lowercase(&mut app);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(app.panel == Some(Panel::Project));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(app.last_chance.is_none());

        choose_lowercase(&mut app);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.json["identifierStyle"], "Lowercase");
        assert_eq!(
            project.json["defs"]["entities"][0]["identifier"],
            "my_entity"
        );
        assert!(app.last_chance.is_some());
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_identifier_search_applies_on_enter_and_can_be_restored() {
        let (mut app, ctx) = editor();
        app.panel = Some(Panel::Project);
        app.project
            .as_mut()
            .unwrap()
            .add_entity_def("My_entity", 16, 16)
            .unwrap();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            label(
                &output,
                "\"My_identifier_1\" -- First letter is always uppercase, the rest is up to you",
            )
            .unwrap()
            .center(),
        );
        frame(&mut app, &ctx, vec![egui::Event::Text("lowercase".into())]);
        let output = frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.json["identifierStyle"], "Lowercase");
        assert_eq!(project.json["defs"]["entities"][0]["identifier"], "my_entity");
        assert!(label(&output, "WARNING!").is_none());
        assert!(app.last_chance.is_some());
        app.restore_last_chance();
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_checkbox_edits_are_undoable() {
        let (mut app, ctx) = editor();
        app.project
            .as_mut()
            .unwrap()
            .json
            .as_object_mut()
            .unwrap()
            .remove("jsonStyle");
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let label = label(&output, "Minify JSON").unwrap();
        fn checkbox(shape: &egui::Shape, label: Rect) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - 24.).abs() < 1.
                        && (rect.rect.height() - 24.).abs() < 1.
                        && rect.rect.y_range().contains(label.center().y)
                        && rect.rect.left() > label.right() =>
                {
                    Some(rect.rect.center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| checkbox(s, label)),
                _ => None,
            }
        }
        let pos = output
            .shapes
            .iter()
            .find_map(|s| checkbox(&s.shape, label))
            .expect("Minify checkbox in the source value column");
        click(&mut app, &ctx, pos);
        assert_eq!(app.project.as_ref().unwrap().json["minifyJson"], true);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_command_add_is_undoable() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let pos = egui::pos2(400., label(&output, "Custom commands").unwrap().center().y);
        click(&mut app, &ctx, pos);
        assert_eq!(
            app.project.as_ref().unwrap().json["customCommands"],
            json!([
                {"when":"Manual","command":""}
            ])
        );
        let output = frame(&mut app, &ctx, vec![]);
        assert!(
            label(&output, "Run").unwrap().right() < 808.,
            "command actions stay inside the property column"
        );
        assert!(
            label(&output, "×").unwrap().right() < 808.,
            "command removal stays inside the property column"
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_command_trigger_can_be_filtered_and_selected() {
        let (mut app, ctx) = editor();
        app.project.as_mut().unwrap().json["customCommands"] =
            json!([{"when":"Manual","command":""}]);
        let output = frame(&mut app, &ctx, vec![]);
        let manual = if cfg!(target_os = "macos") {
            "Run manually (CMD-R)"
        } else {
            "Run manually (CTRL-R)"
        };
        click(&mut app, &ctx, label(&output, manual).unwrap().center());
        frame(&mut app, &ctx, vec![]);
        let output = frame(&mut app, &ctx, vec![egui::Event::Text("loading".into())]);
        assert!(label(&output, "Run after saving").is_none());
        click(&mut app, &ctx, label(&output, "Run after loading").unwrap().center());
        assert_eq!(
            app.project.as_ref().unwrap().json["customCommands"][0]["when"],
            "AfterLoad"
        );
        assert!(!app.command_open);
    }

    #[test]
    fn source_project_command_trust_controls_clear_the_decision_and_keep_the_prompt() {
        let (mut app, ctx) = editor();
        let allowed = "This project is allowed to run user commands on your computer.";
        let blocked = "This project is NOT allowed to run user commands on your computer.";
        let project = app.project.as_mut().unwrap();
        let iid = text(&project.json, "iid", "").to_owned();
        project.json["customCommands"] = json!([{"when":"Manual","command":"echo sample"}]);
        let before = project.json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        for text in [allowed, blocked, "Remove authorization", "Unblock it"] {
            assert!(label(&output, text).is_none(), "unknown trust: {text}");
        }

        for (trusted, message, action) in [
            (false, blocked, "Unblock it"),
            (true, allowed, "Remove authorization"),
        ] {
            app.settings.set_trusted(&iid, trusted);
            let output = frame(&mut app, &ctx, vec![]);
            assert!(label(&output, message).is_some());
            assert!(label(&output, if trusted { blocked } else { allowed }).is_none());
            if !trusted {
                click(&mut app, &ctx, label(&output, "Run").unwrap().center());
                assert!(app.hook_flow.is_none());
            }
            click(&mut app, &ctx, label(&output, action).unwrap().center());
            assert_eq!(app.settings.trust_state(&iid), None);
            let output = frame(&mut app, &ctx, vec![]);
            assert!(label(&output, message).is_none());
            assert!(label(&output, action).is_none());
        }

        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "Run").unwrap().center());
        assert!(app.hook_flow.is_some());
        let draw_prompt =
            |app: &mut EditorApp| ctx.run(egui::RawInput::default(), |ctx| app.trust_dialog(ctx));
        draw_prompt(&mut app);
        let prompt = draw_prompt(&mut app);
        assert!(label(&prompt, "Allow project commands").is_some());
        assert!(label(&prompt, "Block project commands").is_some());
        assert_eq!(app.settings.trust_state(&iid), None);
        assert!(app.command_result.is_none());
        assert!(!app.command_open);
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_level_name_draft_commits_on_enter_and_can_undo() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, "Level_")
            .expect("literal block in the level naming pattern")
            .center();
        click(&mut app, &ctx, pos);
        frame(&mut app, &ctx, vec![]);
        let command = egui::Modifiers {
            command: true,
            mac_cmd: true,
            ..Default::default()
        };
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
                egui::Event::Text("Room_".into()),
            ],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json,
            before,
            "typing keeps a draft until committed"
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "Room_%idx"
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["identifier"],
            "Room_0"
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn level_name_pattern_keeps_keyboard_selection_after_committing_a_literal() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "Level_").unwrap().center());
        frame(&mut app, &ctx, vec![]);
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(
            &mut app,
            &ctx,
            vec![
                key(Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("Room_".into()),
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![key(Key::ArrowRight, egui::Modifiers::NONE)],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "Room_%idx"
        );
        frame(&mut app, &ctx, vec![]);
        frame(
            &mut app,
            &ctx,
            vec![key(Key::Delete, egui::Modifiers::NONE)],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "Room_"
        );
    }

    #[test]
    fn source_project_level_name_blocks_can_be_added_reordered_and_reset() {
        let (mut app, ctx) = editor();
        let output = frame(&mut app, &ctx, vec![]);
        let reset = label(&output, "[reset]").unwrap();
        let plus = egui::pos2(reset.left() - 18., reset.center().y);
        click(&mut app, &ctx, plus);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "LEVELINDEX(1)").is_some());
        assert!(label(&output, "Level index (starting at 1)").is_some());
        let world = label(&output, "WORLDID").expect("unused variable").center();
        click(&mut app, &ctx, world);
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "Level_%idx%world"
        );
        let output = frame(&mut app, &ctx, vec![]);
        let from = label(&output, "WorldId").unwrap().center();
        let to = label(&output, "Level_").unwrap().center();
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(from), pointer(from, true)],
        );
        frame(&mut app, &ctx, vec![egui::Event::PointerMoved(to)]);
        frame(&mut app, &ctx, vec![pointer(to, false)]);
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "%worldLevel_%idx"
        );
        let output = frame(&mut app, &ctx, vec![]);
        let world = label(&output, "WorldId").unwrap().center();
        click(&mut app, &ctx, world);
        click(&mut app, &ctx, world);
        let output = frame(&mut app, &ctx, vec![]);
        let coordinate = label(&output, "LEVELX")
            .expect("variable replacement menu")
            .center();
        click(&mut app, &ctx, coordinate);
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "%xLevel_%idx"
        );
        let output = frame(&mut app, &ctx, vec![]);
        click(&mut app, &ctx, label(&output, "[reset]").unwrap().center());
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "Level_%idx"
        );
        assert!(matches!(
            app.canvas_notices.last().map(|entry| &entry.notice),
            Some(notifications::CanvasNotice::ValueReset)
        ));
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().json["levelNamePattern"],
            "%xLevel_%idx"
        );
    }

    #[test]
    fn source_project_patch_normalizes_backup_paths_and_identifier_changes() {
        let (mut app, _) = editor();
        let dir = tempfile::tempdir().unwrap();
        let project = app.project.as_mut().unwrap();
        project.path = Some(dir.path().join("World.ldtk"));
        project.add_entity_def("My_entity", 16, 16).unwrap();
        let before = project.json.clone();
        app.apply_source_project_patch(json!({
            "backupRelPath": dir.path().join("saved").to_string_lossy(),
            "identifierStyle":"Lowercase",
        }));
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.json["backupRelPath"], "saved");
        assert_eq!(
            project.json["defs"]["entities"][0]["identifier"],
            "my_entity"
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_drawing_keeps_document_history_and_trust_unchanged() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let settings = app.settings.value.clone();
        for _ in 0..2 {
            frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert_eq!(app.settings.value, settings);
        assert!(!app.history.can_undo());
    }
}
