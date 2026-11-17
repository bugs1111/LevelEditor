//! Source EditProject form.
use super::*;

fn project_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.;
        source_controls::property_label_with_width(ui, label, key, 250.);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 30.),
            egui::Layout::left_to_right(egui::Align::Center),
            body,
        )
        .inner
    })
    .inner
}

fn project_checkbox(ui: &mut egui::Ui, values: &Value, patch: &mut Value, key: &str) {
    let mut value = values[key].as_bool().unwrap_or(false);
    if source_checkbox(ui, &mut value, "").changed() {
        patch[key] = json!(value);
    }
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
) {
    let current = text(values, key, choices[0].0);
    let mut value = current.to_owned();
    egui::ComboBox::from_id_salt(key)
        .width(if key == "when" {
            140.
        } else {
            ui.available_width().min(490.)
        })
        .selected_text(
            choices
                .iter()
                .find(|(key, _)| *key == current)
                .map_or(current, |(_, label)| *label),
        )
        .show_ui(ui, |ui| {
            for (key, label) in choices {
                ui.selectable_value(&mut value, (*key).to_owned(), *label);
            }
        });
    if value != current {
        patch[key] = json!(value);
    }
}

impl EditorApp {
    pub(super) fn source_project_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if project.is_backup() {
            self.backup_project_controls(ui);
            return;
        }
        let iid = text(&project.json, "iid", "").to_owned();
        let path = project.path.clone();
        ui.horizontal(|ui| {
            for (label, image, save_as) in [
                (
                    "Save project",
                    egui::include_image!("../../assets/icons/save.svg"),
                    false,
                ),
                (
                    "Save as...",
                    egui::include_image!("../../assets/icons/saveAs.svg"),
                    true,
                ),
            ] {
                if source_controls::add_enabled_button(
                    ui,
                    !self.is_busy(),
                    egui::Button::image_and_text(
                        egui::Image::new(image).fit_to_exact_size(Vec2::splat(16.)),
                        label,
                    ),
                )
                .clicked()
                {
                    self.save(save_as);
                }
            }
            if source_controls::add_enabled_button(
                ui,
                path.is_some() && !self.is_busy(),
                egui::Button::image_and_text(
                    egui::Image::new(egui::include_image!("../../assets/icons/rename.svg"))
                        .fit_to_exact_size(Vec2::splat(16.)),
                    "Rename project",
                ),
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
            if source_controls::add_enabled_button(
                ui,
                path.is_some(),
                egui::Button::image(
                    egui::Image::new(egui::include_image!("../../assets/icons/locate.svg"))
                        .fit_to_exact_size(Vec2::splat(16.)),
                ),
            )
            .on_hover_text("Locate project file")
            .clicked()
                && let Some(path) = &path
                && let Err(error) = home_files::locate(path, true)
            {
                self.status = error.to_string();
                self.status_error = true;
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
        ui.add_space(16.);
        ui.heading("Extra files");
        let mut patch = json!({});
        let project = self.project.as_ref().unwrap();
        let values = &project.json;
        project_row(ui, "Super simple export ❤", "simplifiedExport", |ui| {
            project_checkbox(ui, values, &mut patch, "simplifiedExport");
            ui.hyperlink_to("Learn more about this format", "https://ldtk.io/go/simple");
        });
        project_row(
            ui,
            "Save levels to separate files",
            "externalLevels",
            |ui| {
                project_checkbox(ui, values, &mut patch, "externalLevels");
            },
        );
        if !values["simplifiedExport"].as_bool().unwrap_or(false) {
            project_row(ui, "Export as PNG", "png", |ui| {
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
            if text(values, "imageExportMode", "None") != "None" {
                project_row(ui, "Also export level backgrounds", "exportLevelBg", |ui| {
                    project_checkbox(ui, values, &mut patch, "exportLevelBg");
                });
                project_row(ui, "PNG file names", "pngFilePattern", |ui| {
                    let default = if values["imageExportMode"] == "OneImagePerLevel" {
                        "%level_name"
                    } else {
                        "%level_name__%layer_name"
                    };
                    if let Some(value) = ui_fields::buffered_text(
                        ui,
                        "pngFilePattern",
                        values["pngFilePattern"].as_str().unwrap_or(default),
                    ) {
                        patch["pngFilePattern"] = if value == default {
                            Value::Null
                        } else {
                            json!(value)
                        };
                    }
                    if source_controls::small_button(ui, "Reset").clicked() {
                        patch["pngFilePattern"] = Value::Null;
                    }
                });
            }
        }
        project_row(ui, "Export Tiled TMX", "tiled", |ui| {
            project_checkbox(ui, values, &mut patch, "exportTiled");
            ui.hyperlink_to(
                "Learn more about Tiled export limitations",
                "https://ldtk.io/go/tiled",
            );
        });

        ui.add_space(16.);
        ui.heading("Project settings");
        project_row(ui, "Backup on save", "backup", |ui| {
            project_checkbox(ui, values, &mut patch, "backupOnSave");
            if values["backupOnSave"].as_bool().unwrap_or(false) {
                let custom = values["backupRelPath"].as_str();
                if source_controls::button(
                    ui,
                    if custom.is_some() {
                        "[Custom dir]"
                    } else {
                        "[Default dir]"
                    },
                )
                .on_hover_text(custom.unwrap_or("Project backup folder"))
                .clicked()
                {
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(dir) = self.settings.ui_dir(
                        path.as_deref(),
                        "BackupDir",
                        path.as_ref().and_then(|p| p.parent()),
                    ) {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(dir) = dialog.pick_folder() {
                        self.settings
                            .store_ui_dir(path.as_deref(), "BackupDir", &dir);
                        patch["backupRelPath"] = json!(dir.to_string_lossy());
                    }
                }
                if custom.is_some() && source_controls::small_button(ui, "Reset").clicked() {
                    patch["backupRelPath"] = Value::Null;
                }
                ui.label("Max backup files:");
                project_number(ui, project, &mut patch, "backupLimit", 1, 999);
            }
        });
        project_row(ui, "Minify JSON", "minify", |ui| {
            project_checkbox(ui, values, &mut patch, "minifyJson");
        });
        project_row(
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
                );
            },
        );
        self.apply_source_project_patch(patch);

        ui.add_space(16.);
        ui.heading("User custom commands");
        project_row(ui, "Custom commands", "Custom commands", |ui| {
            ui.vertical(|ui| self.source_project_commands(ui));
        });

        ui.add_space(16.);
        ui.heading("Workspace settings");
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
        project_row(ui, "Default level identifiers", "levelName", |ui| {
            project_text(ui, values, &mut patch, "levelNamePattern");
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
        if changed(&patch) {
            self.edit("Workspace settings changed", |project| {
                apply_patch(&mut project.json, &patch);
                project.refresh_derived_data()
            });
        }
        ui.add_space(16.);
        self.source_project_advanced(ui);
        self.backup_project_dialogs(ui);
    }

    fn apply_source_project_patch(&mut self, patch: Value) {
        if !changed(&patch) {
            return;
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
            self.edit_recoverable("Identifier style changed", operation);
        } else {
            self.edit("Project settings changed", operation);
        }
    }

    fn source_project_commands(&mut self, ui: &mut egui::Ui) {
        let project = self.project.as_ref().unwrap();
        let iid = text(&project.json, "iid", "").to_owned();
        let trusted = self.settings.trusted(&iid);
        egui::Frame::new()
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
                ui.label(if trusted {
                    "This project is allowed to run user commands on your computer."
                } else {
                    "This project is NOT allowed to run user commands on your computer."
                });
                if ui
                    .link(if trusted {
                        "Remove authorization"
                    } else {
                        "Unblock it"
                    })
                    .clicked()
                {
                    self.settings.set_trusted(&iid, !trusted);
                }
            });
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
                            ("Manual", "Run manually"),
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
                    if source_controls::add_enabled_button(ui, trusted, egui::Button::new("Run"))
                        .on_hover_text("Run command")
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
                        .on_hover_text("Delete command")
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
        if source_controls::button(ui, "+")
            .on_hover_text("Add command")
            .clicked()
        {
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
        let flags = self.project.as_ref().unwrap().json["flags"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        egui::CollapsingHeader::new("Advanced project settings")
            .default_open(!flags.is_empty())
            .show(ui, |ui| {
                ui.label("Advanced options are not recommended for most users as they might break compatibility with older game APIs. Please make sure you know what you're doing before using any of them!");
                for (flag, label) in [
                    ("MultiWorlds", "Multi-worlds support"),
                    ("PrependIndexToLevelFileNames", "Prefix level file names with their index in array"),
                    ("ExportPreCsvIntGridFormat", "Export legacy pre-CSV IntGrid layers data"),
                    ("UseMultilinesType", "Use \"Multilines\" instead of \"String\" for fields in JSON"),
                    ("ExportOldTableOfContentData", "Export old entity table-of-content data"),
                    ("DiscardPreCsvIntGrid", "DiscardPreCsvIntGrid"),
                    ("ExportTileIds", "ExportTileIds"),
                    ("IgnoreBackupSuggest", "IgnoreBackupSuggest"),
                ] {
                    let mut enabled = flags.contains(&json!(flag));
                    if source_checkbox(ui, &mut enabled, label).changed() {
                        if flag == "MultiWorlds" {
                            if enabled && self.edit("Multiple worlds enabled", |p| p.enable_multiworlds()).is_some() {
                                if let Some(mut key) = self.selected {
                                    key.world = Some(0);
                                    self.navigate_level(key, true);
                                } else {
                                    self.navigate_world(Some(0));
                                }
                            } else if !enabled {
                                self.status = "Move or remove extra worlds before converting back to a single world.".into();
                                self.status_error = true;
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
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.set_max_width(800.);
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
        let save = label(&output, "Save project").unwrap();
        for action in ["Save as...", "Rename project"] {
            let other = label(&output, action).unwrap_or_else(|| panic!("missing {action}"));
            assert!((other.center().y - save.center().y).abs() < 1.);
        }
        let headings = [
            "Extra files",
            "Project settings",
            "User custom commands",
            "Workspace settings",
            "Advanced project settings",
        ];
        let mut previous = save.bottom();
        for heading in headings {
            let rect = label(&output, heading).unwrap_or_else(|| panic!("missing {heading}"));
            assert!(rect.top() > previous, "{heading} is out of source order");
            previous = rect.bottom();
        }
        label(&output, "Default entity size").expect("source paired entity dimensions");
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
        label(&output, "PNG file names").expect("PNG pattern when export is enabled");
        label(&output, "%level_name__%layer_name").expect("effective default PNG naming pattern");
        label(&output, "Also export level backgrounds").expect("PNG backgrounds");
        label(&output, "Max backup files:").expect("backup limit when backups are enabled");

        app.project.as_mut().unwrap().json["simplifiedExport"] = json!(true);
        let output = frame(&mut app, &ctx, vec![]);
        assert!(label(&output, "Export as PNG").is_none());
        assert!(label(&output, "PNG file names").is_none());
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
    fn source_project_checkbox_edits_are_undoable() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let label = label(&output, "Minify JSON").unwrap();
        fn checkbox(shape: &egui::Shape, label: Rect) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - 24.).abs() < 1.
                        && (rect.rect.height() - 24.).abs() < 1.
                        && (rect.rect.center().y - label.center().y).abs() < 1.
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
    fn source_project_command_add_and_trust_use_existing_services() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, "+").expect("Add command").center();
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
        let pos = label(&output, "Unblock it").unwrap().center();
        click(&mut app, &ctx, pos);
        let iid = text(&app.project.as_ref().unwrap().json, "iid", "");
        assert!(app.settings.trusted(iid));
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_project_level_name_draft_commits_on_enter_and_can_undo() {
        let (mut app, ctx) = editor();
        let before = app.project.as_ref().unwrap().json.clone();
        let output = frame(&mut app, &ctx, vec![]);
        let pos = label(&output, "Level_%idx")
            .expect("level naming pattern editor")
            .center();
        click(&mut app, &ctx, pos);
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
                egui::Event::Text("Room_%idx".into()),
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
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
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
