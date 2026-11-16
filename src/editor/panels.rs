#[path = "definition_clipboard.rs"]
mod definition_clipboard;
#[path = "entity_transfer.rs"]
mod entity_transfer;
#[path = "enum_sync_dialog.rs"]
mod enum_sync_dialog;
#[path = "panel_layout.rs"]
mod panel_layout;
#[path = "rule_batch.rs"]
mod rule_batch;
#[path = "rule_wizard.rs"]
mod rule_wizard;
#[path = "source_colors.rs"]
pub(in crate::app) mod source_colors;
#[path = "source_controls.rs"]
pub(in crate::app) mod source_controls;
use super::ui_fields::{
    Property as P, Widget as W, apply_patch, changed, field_value_editor, form, scalar_editor, tags,
};
use super::*;
use definition_clipboard::ClipTarget;
use serde_json::json;

impl EditorApp {
    pub(super) fn properties(&mut self, ui: &mut egui::Ui, panel: Panel) {
        let ctx = ui.ctx().clone();
        self.prepare_color_context(&ctx);
        let key = ui.make_persistent_id((
            std::mem::discriminant(&panel),
            self.definition_uid,
            self.rule_group_uid,
            self.rule_uid,
            self.field_uid,
        ));
        self.with_local_atlas_state(&ctx, key, |app| app.properties_inner(ui, panel));
    }
    fn properties_inner(&mut self, ui: &mut egui::Ui, panel: Panel) {
        match panel {
            Panel::Project => self.project_properties(ui),
            Panel::World => self.world_properties(ui),
            Panel::Level => self.level_properties(ui),
            Panel::Layers => self.layer_properties(ui),
            Panel::Entities => self.entity_def_properties(ui),
            Panel::Entity => self.entity_properties(ui),
            Panel::Fields => self.field_properties(ui),
            Panel::Enums => self.enum_properties(ui),
            Panel::Tilesets => self.tileset_properties(ui),
            Panel::Rules => self.rule_properties(ui),
            Panel::Settings => self.settings_properties(ui),
            Panel::Export => self.export_properties(ui),
            Panel::Help => self.help_contents(ui),
        }
    }

    fn project_properties(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            return;
        }
        self.backup_project_controls(ui);
        if self.project.as_ref().is_some_and(Project::is_backup) {
            return;
        }
        ui.horizontal(|ui| {
            if source_controls::button(ui, "Save project").clicked() {
                self.save(false);
            }
            if source_controls::button(ui, "Save as…").clicked() {
                self.save(true);
            }
            if source_controls::button(ui, "Export…").clicked() {
                self.show_panel(Some(Panel::Export));
            }
        });
        ui.heading("Extra files");
        let patch = form(
            ui,
            &self.project.as_ref().unwrap().json,
            &[
                P("simplifiedExport", "Super simple export", W::Bool),
                P("externalLevels", "Save levels to separate files", W::Bool),
                P(
                    "imageExportMode",
                    "Export PNG",
                    W::Choice(&[
                        "None",
                        "OneImagePerLayer",
                        "OneImagePerLevel",
                        "LayersAndLevels",
                    ]),
                ),
                P("exportLevelBg", "Include level backgrounds", W::Bool),
                P("pngFilePattern", "PNG file names", W::Text),
                P("exportTiled", "Export Tiled TMX", W::Bool),
                P("backupOnSave", "Backup on save", W::Bool),
                P("backupLimit", "Max backup files", W::Int(1, 999)),
                P("backupRelPath", "Backup folder", W::Path),
                P("minifyJson", "Minify JSON", W::Bool),
                P(
                    "identifierStyle",
                    "Identifier convention",
                    W::Choice(&["Capitalize", "Uppercase", "Lowercase", "Free"]),
                ),
            ],
        );
        if changed(&patch) {
            let operation = |p: &mut Project| {
                if let Some(enabled) = patch["externalLevels"].as_bool() {
                    p.set_external_levels(enabled)?;
                }
                let mut patch = patch.clone();
                if let Some(style) = patch
                    .get("identifierStyle")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                {
                    p.apply_identifier_style(&style)?;
                    patch.as_object_mut().unwrap().remove("identifierStyle");
                }
                relative_path_patch(p, &mut patch, "backupRelPath")?;
                apply_patch(&mut p.json, &patch);
                Ok(())
            };
            if patch.get("identifierStyle").is_some() {
                self.edit_recoverable("Identifier style changed", operation);
            } else {
                self.edit("Project settings changed", operation);
            }
        }
        ui.heading("Workspace settings");
        let patch = form(
            ui,
            &self.project.as_ref().unwrap().json,
            &[
                P("defaultGridSize", "Default layer grid", W::Int(1, 1024)),
                P(
                    "defaultEntityWidth",
                    "Default entity width",
                    W::Int(1, 65536),
                ),
                P(
                    "defaultEntityHeight",
                    "Default entity height",
                    W::Int(1, 65536),
                ),
                P("levelNamePattern", "New level identifiers", W::Text),
                P("bgColor", "Workspace background", W::Color),
                P("defaultLevelBgColor", "Default level background", W::Color),
                P("defaultPivotX", "Default pivot X", W::Float(0.0, 1.0)),
                P("defaultPivotY", "Default pivot Y", W::Float(0.0, 1.0)),
                P("tutorialDesc", "Tutorial description", W::Multiline),
            ],
        );
        if changed(&patch) {
            self.edit("Workspace settings changed", |p| {
                apply_patch(&mut p.json, &patch);
                p.refresh_derived_data()
            });
        }
        ui.collapsing("Advanced project settings",|ui|{
            for flag in ["MultiWorlds","ExportPreCsvIntGridFormat","ExportOldTableOfContentData","DiscardPreCsvIntGrid","UseMultilinesType","ExportTileIds","IgnoreBackupSuggest","PrependIndexToLevelFileNames"]{
                let mut enabled=self.project.as_ref().unwrap().json["flags"].as_array().is_some_and(|a|a.contains(&json!(flag)));
                if ui.checkbox(&mut enabled,flag).changed(){if flag=="MultiWorlds" {if enabled&&self.edit("Multiple worlds enabled",|p|p.enable_multiworlds()).is_some(){if let Some(mut key)=self.selected{key.world=Some(0);self.navigate_level(key,true);}else{self.navigate_world(Some(0));}}else if !enabled{self.status="Move or remove extra worlds before converting back to a single world.".into();self.status_error=true;}return;}self.edit("Project option changed",|p|{let flags=p.json["flags"].as_array_mut().ok_or_else(||ldtk_core::Error::Invalid("flags must be an array".into()))?;flags.retain(|v|v!=flag);if enabled{flags.push(json!(flag));}Ok(())});}
            }
        });
        ui.heading("Custom commands");
        self.command_properties(ui);
    }

    fn backup_project_controls(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let iid = text(&project.json, "iid", "").to_owned();
        let read_only = project.is_backup();
        let original_key = egui::Id::new(("backup-original", iid.clone()));
        let browser_key = egui::Id::new(("backup-browser", iid.clone()));
        let rename_key = egui::Id::new(("project-rename", iid));
        if read_only {
            ui.heading("Backup preview");
            ui.label("This backup is read-only. Restore it to replace the original project, or save a new copy.");
            let original = ui
                .ctx()
                .data_mut(|data| data.get_temp::<PathBuf>(original_key));
            ui.horizontal(|ui| {
                if source_controls::button(ui, "Locate original project…").clicked()
                    && let Some(original) = rfd::FileDialog::new()
                        .add_filter("LDtk project", &["ldtk", "json"])
                        .pick_file()
                {
                    ui.ctx()
                        .data_mut(|data| data.insert_temp(original_key, original.clone()));
                    if let Some(backup) = self.project.as_ref().and_then(|p| p.path.clone()) {
                        self.request(ui.ctx(), Action::OpenBackup { backup, original });
                    }
                }
                if source_controls::button(ui, "Save copy as…").clicked() {
                    self.save(true);
                }
            });
            if let Some(original) = original {
                ui.label(format!("Original: {}", original.display()));
                if source_controls::button(ui, "Restore this backup…").clicked()
                    && let Some(backup) = self.project.as_ref().and_then(|p| p.path.clone())
                {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            browser_key,
                            BackupBrowser {
                                original,
                                backups: vec![],
                                restore: Some(backup),
                            },
                        )
                    });
                }
            }
        } else {
            ui.horizontal(|ui| {
                if source_controls::button(ui, "Browse backups…").clicked() {
                    match self.project.as_ref().unwrap().list_backups() {
                        Ok(backups) => {
                            if let Some(original) =
                                self.project.as_ref().and_then(|p| p.path.clone())
                            {
                                ui.ctx().data_mut(|data| {
                                    data.insert_temp(
                                        browser_key,
                                        BackupBrowser {
                                            original,
                                            backups,
                                            restore: None,
                                        },
                                    )
                                });
                            }
                        }
                        Err(error) => {
                            self.status = error.to_string();
                            self.status_error = true;
                        }
                    }
                }
                if source_controls::button(ui, "Rename project…").clicked()
                    && let Some(path) = self.project.as_ref().and_then(|p| p.path.as_ref())
                {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            rename_key,
                            path.file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        )
                    });
                }
            });
        }
        if let Some(mut name) = ui
            .ctx()
            .data_mut(|data| data.get_temp::<String>(rename_key))
        {
            let mut close = false;
            egui::Modal::new(rename_key).show(ui.ctx(), |ui| {
                ui.heading("Rename project");
                ui.label("New file name; the project extension is kept.");
                ui.text_edit_singleline(&mut name);
                ui.horizontal(|ui| {
                    if source_controls::button(ui, "Rename").clicked() {
                        self.request(ui.ctx(), Action::RenameProject(name.clone()));
                        close = true;
                    }
                    if source_controls::button(ui, "Cancel").clicked() {
                        close = true;
                    }
                });
            });
            ui.ctx().data_mut(|data| {
                if close {
                    data.remove::<String>(rename_key);
                } else {
                    data.insert_temp(rename_key, name);
                }
            });
        }
        if let Some(mut browser) = ui
            .ctx()
            .data_mut(|data| data.get_temp::<BackupBrowser>(browser_key))
        {
            let mut close = false;
            egui::Modal::new(browser_key).show(ui.ctx(), |ui| {
                ui.heading("Project backups");
                ui.label(browser.original.display().to_string());
                if let Some(backup)=browser.restore.clone(){
                    ui.colored_label(theme::ACCENT,"Restoring replaces the current project with this backup. The current version will be backed up first.");
                    ui.label(backup.display().to_string());
                    ui.horizontal(|ui|{
                        if source_controls::button(ui,"Restore backup").clicked(){self.request(ui.ctx(),Action::RestoreBackup{backup:backup.clone(),original:browser.original.clone()});close=true;}
                        if source_controls::button(ui,"Back").clicked(){browser.restore=None;}
                    });
                }else{
                    if browser.backups.is_empty(){ui.label("No backups found.");}
                    egui::ScrollArea::vertical().max_height(360.0).show(ui,|ui|{
                        for backup in &browser.backups{ui.horizontal(|ui|{
                            ui.label(format!("{}{}",backup.timestamp,if backup.crash{" (crash recovery)"}else{""}));
                            if source_controls::button(ui,"Open").clicked(){ui.ctx().data_mut(|data|data.insert_temp(original_key,browser.original.clone()));self.request(ui.ctx(),Action::OpenBackup{backup:backup.path.clone(),original:browser.original.clone()});close=true;}
                            if source_controls::button(ui,"Restore…").clicked(){browser.restore=Some(backup.path.clone());}
                        });}
                    });
                }
                if source_controls::button(ui,"Close").clicked(){close=true;}
            });
            ui.ctx().data_mut(|data| {
                if close {
                    data.remove::<BackupBrowser>(browser_key);
                } else {
                    data.insert_temp(browser_key, browser);
                }
            });
        }
    }

    fn world_properties(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let current_world = self.active_world;
        let worlds = project.json["worlds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .enumerate()
                    .map(|(i, w)| (i, text(w, "identifier", "World").to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        ui.horizontal(|ui| {
            let label = current_world
                .and_then(|i| worlds.iter().find(|(id, _)| *id == i))
                .map(|(_, s)| s.as_str())
                .unwrap_or("World");
            egui::ComboBox::from_id_salt("world-properties")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    if worlds.is_empty()
                        && ui
                            .selectable_label(current_world.is_none(), "World")
                            .clicked()
                    {
                        self.navigate_world(None);
                    }
                    for (index, name) in &worlds {
                        if ui
                            .selectable_label(current_world == Some(*index), name)
                            .clicked()
                        {
                            self.navigate_world(Some(*index));
                        }
                    }
                });
            if source_controls::button(ui, "+ World").clicked() {
                self.create_form_world();
            }
        });
        if let Some(index) = current_world
            && source_controls::button(ui, "Duplicate world").clicked()
        {
            let base = worlds
                .iter()
                .find(|(i, _)| *i == index)
                .map(|(_, name)| format!("{name}_copy"))
                .unwrap_or_else(|| "World_copy".into());
            let name = unique_name(&base, worlds.iter().map(|(_, n)| n.as_str()));
            if let Some(world) = self.edit("World duplicated", |p| p.duplicate_world(index, &name))
            {
                self.navigate_world(Some(world));
            }
        }
        ui.horizontal(|ui| {
            if source_controls::add_enabled_button(
                ui,
                current_world.is_some()
                    || worlds.is_empty()
                        && self.project.as_ref().unwrap().json["flags"]
                            .as_array()
                            .is_none_or(|f| !f.contains(&json!("MultiWorlds"))),
                egui::Button::new("Create level"),
            )
            .clicked()
            {
                self.start_level_spot_picker();
            }
            if source_controls::button(ui, "Duplicate level").clicked()
                && let Some(key) = self.selected
            {
                self.queue_world_form_action(
                    ui.ctx(),
                    world_form_dialog::WorldFormAction::DuplicateLevel(key),
                );
            }
            if source_controls::button(ui, "Delete level").clicked()
                && let Some(key) = self.selected
            {
                self.queue_world_form_action(
                    ui.ctx(),
                    world_form_dialog::WorldFormAction::DeleteLevel(key),
                );
            }
        });
        let metadata = self
            .project
            .as_ref()
            .map(|p| {
                let world = current_world
                    .and_then(|i| p.json["worlds"].get(i))
                    .unwrap_or(&p.json);
                let mut v = json!({});
                for key in [
                    "identifier",
                    "worldLayout",
                    "worldGridWidth",
                    "worldGridHeight",
                    "defaultLevelWidth",
                    "defaultLevelHeight",
                ] {
                    v[key] = world[key].clone();
                }
                v
            })
            .unwrap();
        let default_grid = self.project.as_ref().unwrap().json["defaultGridSize"]
            .as_i64()
            .unwrap_or(16);
        let patch = form(
            ui,
            &metadata,
            &[
                P("identifier", "World identifier", W::Text),
                P(
                    "worldLayout",
                    "World layout",
                    W::Choice(&["Free", "GridVania", "LinearHorizontal", "LinearVertical"]),
                ),
                P("worldGridWidth", "GridVania width", W::Int(1, 65536)),
                P("worldGridHeight", "GridVania height", W::Int(1, 65536)),
                P(
                    "defaultLevelWidth",
                    "New level width",
                    W::Int(default_grid, 9999),
                ),
                P(
                    "defaultLevelHeight",
                    "New level height",
                    W::Int(default_grid, 9999),
                ),
            ],
        );
        if changed(&patch) {
            self.queue_world_form_action(
                ui.ctx(),
                world_form_dialog::WorldFormAction::Settings {
                    world: current_world,
                    patch,
                },
            );
        }
        if let Some(world) = current_world
            && source_controls::button(ui, "Delete world and its levels").clicked()
        {
            self.queue_world_form_action(
                ui.ctx(),
                world_form_dialog::WorldFormAction::DeleteWorld(world),
            );
        }
    }

    fn level_properties(&mut self, ui: &mut egui::Ui) {
        let Some(key) = self.selected else {
            ui.label("Create a level in the World panel.");
            return;
        };
        let Some(level) = self.project.as_ref().and_then(|p| p.level(key)).cloned() else {
            return;
        };
        ui.label(format!("IID: {}", text(&level, "iid", "")));
        let mut patch = form(
            ui,
            &level,
            &[
                P("identifier", "Level identifier", W::Text),
                P("useAutoIdentifier", "Automatic identifier", W::Bool),
                P("worldDepth", "World depth", W::Int(-9999, 9999)),
                P("worldX", "World X", W::Int(-1_000_000, 1_000_000)),
                P("worldY", "World Y", W::Int(-1_000_000, 1_000_000)),
                P("pxWid", "Width", W::Int(1, 65536)),
                P("pxHei", "Height", W::Int(1, 65536)),
                P("bgColor", "Background", W::NullableColor),
                P("bgRelPath", "Background image", W::Path),
                P(
                    "bgPos",
                    "Image positioning",
                    W::Choice(&["Unscaled", "Contain", "Cover", "CoverDirty", "Repeat"]),
                ),
            ],
        );
        if let Some(path) = level["bgRelPath"].as_str() {
            let path = canvas::asset_path(self.project.as_ref().unwrap(), path);
            if let Some(texture) = self.assets.texture(ui.ctx(), &path)
                && let Some(pivot) = super::ui_fields::pivot_editor(
                    ui,
                    "background-pivot",
                    [
                        number(&level, "bgPivotX", 0.5),
                        number(&level, "bgPivotY", 0.5),
                    ],
                    texture.size_vec2().into(),
                )
            {
                patch["bgPivotX"] = json!(pivot[0]);
                patch["bgPivotY"] = json!(pivot[1]);
            }
        }
        if changed(&patch) {
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                snap_level_form_patch(p, key, &mut patch)?;
                relative_path_patch(p, &mut patch, "bgRelPath")?;
                normalize_background_patch(&level, &mut patch);
                if patch.get("pxWid").is_some() || patch.get("pxHei").is_some() {
                    p.resize_level(
                        key,
                        patch["pxWid"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxWid", 256.0) as i64)
                            as i32,
                        patch["pxHei"]
                            .as_i64()
                            .unwrap_or(number(&level, "pxHei", 256.0) as i64)
                            as i32,
                        0,
                        0,
                    )?;
                }
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| ldtk_core::Error::Invalid("level missing".into()))?;
                apply_patch(l, &patch);
                if patch.get("__bgColor").is_some() {
                    l["bgColor"] = patch["__bgColor"].clone();
                }
                p.refresh_derived_data()?;
                p.rebuild_auto_layers(key)
            };
            if patch.get("pxWid").is_some() || patch.get("pxHei").is_some() {
                self.edit_level_recoverable("Level resized", key, operation);
            } else {
                self.edit("Level properties changed", operation);
            }
        }
        let destinations = self.project.as_ref().unwrap().json["worlds"]
            .as_array()
            .map(|worlds| {
                worlds
                    .iter()
                    .enumerate()
                    .map(|(i, w)| (Some(i), text(w, "identifier", "World").to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !destinations.is_empty() {
            let mut destination = key.world;
            egui::ComboBox::from_id_salt("level-world")
                .selected_text(
                    destinations
                        .iter()
                        .find(|(i, _)| *i == destination)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("Root world"),
                )
                .show_ui(ui, |ui| {
                    if self.project.as_ref().unwrap().json["flags"]
                        .as_array()
                        .is_none_or(|flags| !flags.contains(&json!("MultiWorlds")))
                    {
                        ui.selectable_value(&mut destination, None, "Root world");
                    }
                    for (index, name) in &destinations {
                        ui.selectable_value(&mut destination, *index, name);
                    }
                });
            if destination != key.world {
                let index = self
                    .project
                    .as_ref()
                    .unwrap()
                    .level_keys()
                    .iter()
                    .filter(|k| k.world == destination)
                    .count();
                if let Some(moved) = self.edit("Level moved to world", |p| {
                    p.move_level_to_world(key, destination, index)
                }) {
                    self.navigate_level(moved, true);
                }
            }
        }
        ui.heading("Custom fields");
        self.instance_fields(ui, key, None, &level);
        if source_controls::button(ui, "Edit level field definitions").clicked() {
            self.field_owner = None;
            self.field_uid = None;
            self.show_panel(Some(Panel::Fields));
        }
    }

    fn layer_properties(&mut self, ui: &mut egui::Ui) {
        if !self.source_panel_active(ui) {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_name);
                egui::ComboBox::from_id_salt("new-layer-kind")
                    .selected_text(&self.new_kind)
                    .show_ui(ui, |ui| {
                        for kind in ["IntGrid", "Tiles", "AutoLayer", "Entities"] {
                            ui.selectable_value(&mut self.new_kind, kind.to_owned(), kind);
                        }
                    });
                ui.add(egui::DragValue::new(&mut self.new_grid).range(1..=1024));
                if source_controls::button(ui, "Add layer").clicked() {
                    let count = self
                        .project
                        .as_ref()
                        .and_then(|p| p.json["defs"]["layers"].as_array())
                        .map_or(0, Vec::len);
                    let name = if self.new_name.is_empty() {
                        format!("Layer_{count}")
                    } else {
                        self.new_name.clone()
                    };
                    let kind = self.new_kind.clone();
                    let grid = self.new_grid;
                    if let Some(uid) =
                        self.edit("Layer created", |p| p.add_layer(&name, &kind, grid))
                    {
                        self.definition_uid = Some(uid);
                        self.new_name.clear();
                    }
                }
            });
        }
        let Some(def) = self.definition_picker(ui, "layers") else {
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        ui.horizontal(|ui| {
            if source_controls::button(ui, "Move up").clicked() {
                let index = self.definition_index("layers", uid);
                self.edit("Layer reordered", |p| {
                    p.reorder_layer(uid, index.saturating_sub(1))
                });
            }
            if source_controls::button(ui, "Move down").clicked() {
                let index = self.definition_index("layers", uid);
                let n = self.project.as_ref().unwrap().json["defs"]["layers"]
                    .as_array()
                    .unwrap()
                    .len();
                self.edit("Layer reordered", |p| {
                    p.reorder_layer(uid, (index + 1).min(n - 1))
                });
            }
            if source_controls::button(ui, "Delete layer").clicked() {
                self.edit_recoverable("Layer deleted", |p| p.remove_definition("layers", uid));
                self.definition_uid = None;
            }
        });
        let patch = form(
            ui,
            &def,
            &[
                P("identifier", "Layer identifier", W::Text),
                P("doc", "Description", W::Multiline),
                P("gridSize", "Grid size", W::Int(1, 1024)),
                P("displayOpacity", "Display opacity", W::Float(0.0, 1.0)),
                P("inactiveOpacity", "Inactive opacity", W::Float(0.0, 1.0)),
                P("pxOffsetX", "Horizontal offset", W::Int(-65536, 65536)),
                P("pxOffsetY", "Vertical offset", W::Int(-65536, 65536)),
                P(
                    "parallaxFactorX",
                    "Horizontal parallax",
                    W::Float(-10.0, 10.0),
                ),
                P(
                    "parallaxFactorY",
                    "Vertical parallax",
                    W::Float(-10.0, 10.0),
                ),
                P("parallaxScaling", "Scale with parallax", W::Bool),
                P("guideGridWid", "Guide grid width", W::Int(0, 65536)),
                P("guideGridHei", "Guide grid height", W::Int(0, 65536)),
                P("tilePivotX", "Tile pivot X", W::Float(0.0, 1.0)),
                P("tilePivotY", "Tile pivot Y", W::Float(0.0, 1.0)),
                P("hideInList", "Hide in layer list", W::Bool),
                P("hideFieldsWhenInactive", "Hide inactive fields", W::Bool),
                P("canSelectWhenInactive", "Select when inactive", W::Bool),
                P("renderInWorldView", "Render in world view", W::Bool),
            ],
        );
        if changed(&patch) {
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                if let Some(grid) = patch["gridSize"].as_i64() {
                    p.change_layer_grid_size(uid, grid as i32)?;
                    patch.as_object_mut().unwrap().remove("gridSize");
                }
                if changed(&patch) {
                    p.update_layer_properties(uid, &patch)?;
                }
                for key in p.level_keys() {
                    p.rebuild_auto_layers(key)?;
                }
                Ok(())
            };
            if patch.get("gridSize").is_some() {
                self.edit_recoverable("Layer grid changed", operation);
            } else {
                self.edit("Layer properties changed", operation);
            }
        }
        let kind = text(&def, "type", "");
        if kind == "Tiles" || kind == "AutoLayer" || kind == "IntGrid" {
            let options = remap_options(ui, ("layer", uid), true, ldtk_core::TilesetRemap::TileIds);
            let mut selected = def["tilesetDefUid"].as_i64().unwrap_or(-1);
            let previous = selected;
            self.tileset_choice(ui, "Tileset", &mut selected);
            if selected != previous
                && let Some(discarded) = self.edit_recoverable("Tileset assigned", |p| {
                    p.assign_tileset_with_policy(
                        uid,
                        (selected >= 0).then_some(selected),
                        options.mode,
                        options.clear_overrides,
                        options.allow_loss,
                    )
                })
                && discarded > 0
            {
                self.status = format!("Tileset assigned; removed {discarded} references");
            }
        }
        if let Some(key) = self.selected
            && let Some((index, instance)) = self
                .project
                .as_ref()
                .and_then(|p| p.level(key))
                .and_then(|l| l["layerInstances"].as_array())
                .and_then(|a| {
                    a.iter()
                        .enumerate()
                        .find(|(_, li)| li["layerDefUid"] == uid)
                })
                .map(|(i, l)| (i, l.clone()))
        {
            ui.collapsing("Current level instance", |ui| {
                let patch = form(
                    ui,
                    &instance,
                    &[
                        P("visible", "Visible", W::Bool),
                        P("seed", "Random seed", W::Int(0, 9_999_999)),
                        P("pxOffsetX", "Horizontal offset", W::Int(-65536, 65536)),
                        P("pxOffsetY", "Vertical offset", W::Int(-65536, 65536)),
                    ],
                );
                if changed(&patch) {
                    self.edit_level("Layer instance changed", key, |p| {
                        let li = p.layer_mut(key, index).ok_or_else(|| {
                            ldtk_core::Error::Invalid("layer instance missing".into())
                        })?;
                        apply_patch(li, &patch);
                        p.refresh_derived_data()?;
                        p.rebuild_auto_layers(key)
                    });
                }
                if kind == "Tiles" || kind == "AutoLayer" || kind == "IntGrid" {
                    let options = remap_options(
                        ui,
                        ("instance", uid),
                        false,
                        ldtk_core::TilesetRemap::TileIds,
                    );
                    let mut override_uid = instance["overrideTilesetUid"].as_i64().unwrap_or(-1);
                    let old = override_uid;
                    self.tileset_choice(ui, "Tileset override (this level)", &mut override_uid);
                    if old != override_uid {
                        self.edit_level("Layer tileset override changed", key, |p| {
                            p.set_layer_tileset_override(
                                key,
                                index,
                                (override_uid >= 0).then_some(override_uid),
                                options.mode,
                                options.allow_loss,
                            )
                        });
                    }
                }
            });
        }
        if kind == "Entities" {
            if ui
                .button("Move existing entities to another layer…")
                .clicked()
            {
                self.begin_entity_transfer(ui.ctx(), uid);
            }
            for field in ["requiredTags", "excludedTags", "uiFilterTags"] {
                if let Some(value) = tags(ui, field, &def[field]) {
                    let patch = json!({field:value});
                    self.edit("Layer tags changed", |p| {
                        p.update_layer_properties(uid, &patch)
                    });
                }
            }
        }
        if kind == "IntGrid" {
            ui.heading("IntGrid values");
            let mut values = def["intGridValues"].as_array().cloned().unwrap_or_default();
            let mut groups = def["intGridValuesGroups"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut edited = false;
            let mut delete = None;
            let mut reorder = None;
            let mut icon_atlas = ui
                .ctx()
                .data_mut(|data| data.get_temp::<i64>(egui::Id::new(("intgrid-icon-atlas", uid))))
                .unwrap_or_else(|| def["tilesetDefUid"].as_i64().unwrap_or(-1));
            self.tileset_choice(ui, "Palette icon tileset", &mut icon_atlas);
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new(("intgrid-icon-atlas", uid)), icon_atlas)
            });
            if icon_atlas >= 0 {
                self.atlas_rect_picker(&ui.ctx().clone(), ui, icon_atlas);
            }
            let value_count = values.len();
            for (index, value) in values.iter_mut().enumerate() {
                ui.push_id(("intgrid-value", uid, index), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(value["value"].to_string());
                        if let Some(id) = ui_fields::buffered_text(
                            ui,
                            "identifier",
                            text(value, "identifier", ""),
                        ) {
                            value["identifier"] = if id.is_empty() {
                                Value::Null
                            } else {
                                json!(id)
                            };
                            edited = true;
                        }
                        edited |= color_field(ui, "", value, "color");
                        if source_controls::add_enabled_button(
                            ui,
                            index > 0,
                            egui::Button::new("↑"),
                        )
                        .clicked()
                        {
                            reorder = Some((index, index - 1));
                        }
                        if source_controls::add_enabled_button(
                            ui,
                            index + 1 < value_count,
                            egui::Button::new("↓"),
                        )
                        .clicked()
                        {
                            reorder = Some((index, index + 1));
                        }
                        if source_controls::small_button(ui, "Delete").clicked() {
                            delete = Some(index);
                        }
                    });
                    let mut group = value["groupUid"].as_i64().unwrap_or(0);
                    let old = group;
                    egui::ComboBox::from_id_salt(ui.id().with("value-group"))
                        .selected_text(
                            groups
                                .iter()
                                .find(|g| g["uid"] == group)
                                .map(|g| text(g, "identifier", "Group"))
                                .unwrap_or("Ungrouped"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut group, 0, "Ungrouped");
                            for g in &groups {
                                ui.selectable_value(
                                    &mut group,
                                    g["uid"].as_i64().unwrap_or(0),
                                    text(g, "identifier", "Group"),
                                );
                            }
                        });
                    if group != old {
                        value["groupUid"] = json!(group);
                        edited = true;
                    }
                    ui.horizontal(|ui| {
                        if source_controls::button(ui, "Use icon tile").clicked()
                            && let Some(tile) = self.selected_tile_rect_for(icon_atlas)
                        {
                            value["tile"] = tile;
                            edited = true;
                        }
                        if source_controls::button(ui, "Clear icon").clicked() {
                            value["tile"] = Value::Null;
                            edited = true;
                        }
                    });
                });
            }
            if let Some(index) = delete {
                values.remove(index);
                edited = true;
            } else if let Some((from, to)) = reorder {
                values.swap(from, to);
                edited = true;
            }
            if source_controls::button(ui, "Add IntGrid value").clicked() {
                let next = values
                    .iter()
                    .filter_map(|v| v["value"].as_i64())
                    .max()
                    .unwrap_or(0)
                    + 1;
                if next < 1000 {
                    let color = self.suggested_color(&values);
                    values.push(json!({"value":next,"identifier":format!("Value_{next}"),"color":format!("#{color:06X}"),"tile":null,"groupUid":0}));
                    edited = true;
                }
            }
            ui.collapsing("Value groups", |ui| {
                let mut remove = None;
                let mut reorder = None;
                let count = groups.len();
                for (index, group) in groups.iter_mut().enumerate() {
                    ui.push_id(("intgrid-group", uid, index), |ui| {
                        ui.horizontal(|ui| {
                            if let Some(name) =
                                ui_fields::buffered_text(ui, "name", text(group, "identifier", ""))
                            {
                                group["identifier"] = json!(name);
                                edited = true;
                            }
                            let mut color = group["color"]
                                .as_str()
                                .map(theme::color)
                                .map(|c| [c.r(), c.g(), c.b()]);
                            if source_colors::nullable_color_editor(ui, &mut color).changed() {
                                group["color"] = color
                                    .map(|c| {
                                        json!(format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
                                    })
                                    .unwrap_or(Value::Null);
                                edited = true;
                            }
                            if source_controls::add_enabled_button(
                                ui,
                                index > 0,
                                egui::Button::new("↑"),
                            )
                            .clicked()
                            {
                                reorder = Some((index, index - 1));
                            }
                            if source_controls::add_enabled_button(
                                ui,
                                index + 1 < count,
                                egui::Button::new("↓"),
                            )
                            .clicked()
                            {
                                reorder = Some((index, index + 1));
                            }
                            if source_controls::small_button(ui, "Delete group").clicked() {
                                remove = Some(index);
                            }
                        });
                    });
                }
                if let Some(index) = remove {
                    groups.remove(index);
                    edited = true;
                } else if let Some((from, to)) = reorder {
                    groups.swap(from, to);
                    edited = true;
                }
                if source_controls::button(ui, "Add value group").clicked() {
                    let id = groups
                        .iter()
                        .filter_map(|g| g["uid"].as_i64())
                        .max()
                        .unwrap_or(0)
                        + 1;
                    groups.push(
                        json!({"uid":id,"identifier":format!("Group_{id}"),"color":"#FFCC00"}),
                    );
                    edited = true;
                }
            });
            if edited {
                let operation = |p: &mut Project| {
                    p.update_layer_properties(
                        uid,
                        &json!({"intGridValues":values,"intGridValuesGroups":groups}),
                    )
                };
                if values.len() < def["intGridValues"].as_array().map_or(0, Vec::len) {
                    self.edit_recoverable("IntGrid value removed", operation);
                } else {
                    self.edit("IntGrid palette changed", operation);
                }
            }
        }
        if kind == "IntGrid" || kind == "AutoLayer" {
            if source_controls::button(ui, "Edit auto-layer rules").clicked() {
                self.definition_uid = Some(uid);
                self.rule_uid = None;
                self.rule_group_uid = None;
                self.show_panel(Some(Panel::Rules));
            }
            if kind == "AutoLayer" {
                let sources = self.project.as_ref().unwrap().json["defs"]["layers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|d| d["type"] == "IntGrid")
                    .cloned()
                    .collect::<Vec<_>>();
                let mut selected = def["autoSourceLayerDefUid"].as_i64().unwrap_or(-1);
                let old = selected;
                egui::ComboBox::from_id_salt("source-grid")
                    .selected_text(
                        sources
                            .iter()
                            .find(|s| s["uid"] == selected)
                            .map(|s| text(s, "identifier", "Source"))
                            .unwrap_or("Choose source IntGrid"),
                    )
                    .show_ui(ui, |ui| {
                        for source in sources {
                            ui.selectable_value(
                                &mut selected,
                                source["uid"].as_i64().unwrap(),
                                text(&source, "identifier", "Source"),
                            );
                        }
                    });
                if old != selected {
                    self.edit("Auto-layer source changed", |p| {
                        p.update_layer_properties(uid, &json!({"autoSourceLayerDefUid":selected}))?;
                        for key in p.level_keys() {
                            p.rebuild_auto_layers(key)?;
                        }
                        Ok(())
                    });
                }
            }
        }
    }

    fn entity_def_properties(&mut self, ui: &mut egui::Ui) {
        if !self.source_panel_active(ui) {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_name);
                if source_controls::button(ui, "Add entity").clicked() {
                    let name = if self.new_name.is_empty() {
                        format!(
                            "Entity_{}",
                            self.project.as_ref().unwrap().json["defs"]["entities"]
                                .as_array()
                                .map_or(0, Vec::len)
                        )
                    } else {
                        self.new_name.clone()
                    };
                    let used = self.project.as_ref().unwrap().json["defs"]["entities"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let color = self.suggested_color(&used);
                    if let Some(uid) = self.edit("Entity definition created", |p| {
                        let uid = p.add_entity_def(&name, 16, 16)?;
                        p.update_entity_definition(uid, &json!({"color":format!("#{color:06X}")}))?;
                        Ok(uid)
                    }) {
                        self.definition_uid = Some(uid);
                        self.entity_uid = Some(uid);
                        self.new_name.clear();
                    }
                }
            });
        }
        let Some(def) = self.definition_picker(ui, "entities") else {
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        let patch = form(
            ui,
            &def,
            &[
                P("identifier", "Entity identifier", W::Text),
                P("doc", "Description", W::Multiline),
                P("width", "Default width", W::Int(1, 65536)),
                P("height", "Default height", W::Int(1, 65536)),
                P("resizableX", "Resizable horizontally", W::Bool),
                P("resizableY", "Resizable vertically", W::Bool),
                P("minWidth", "Minimum width", W::Int(1, 65536)),
                P("maxWidth", "Maximum width", W::Int(1, 65536)),
                P("minHeight", "Minimum height", W::Int(1, 65536)),
                P("maxHeight", "Maximum height", W::Int(1, 65536)),
                P("pivotX", "Pivot X", W::Float(0.0, 1.0)),
                P("pivotY", "Pivot Y", W::Float(0.0, 1.0)),
                P("color", "Color", W::Color),
                P(
                    "renderMode",
                    "Display as",
                    W::Choice(&["Rectangle", "Ellipse", "Cross", "Tile"]),
                ),
                P("hollow", "Hollow", W::Bool),
                P("fillOpacity", "Fill opacity", W::Float(0.0, 1.0)),
                P("lineOpacity", "Outline opacity", W::Float(0.0, 1.0)),
                P("tileOpacity", "Tile opacity", W::Float(0.0, 1.0)),
                P(
                    "tileRenderMode",
                    "Tile render mode",
                    W::Choice(&[
                        "FitInside",
                        "Stretch",
                        "Cover",
                        "Repeat",
                        "FullSizeCropped",
                        "FullSizeUncropped",
                        "NineSlice",
                    ]),
                ),
                P("showName", "Show identifier", W::Bool),
                P("keepAspectRatio", "Keep aspect ratio", W::Bool),
                P("exportToToc", "Export to table of contents", W::Bool),
                P("maxCount", "Max instance count", W::Int(0, 1_000_000)),
                P(
                    "limitScope",
                    "Count limit scope",
                    W::Choice(&["PerLayer", "PerLevel", "PerWorld"]),
                ),
                P(
                    "limitBehavior",
                    "When count is reached",
                    W::Choice(&["PreventAdding", "DiscardOldOnes", "MoveLastOne"]),
                ),
            ],
        );
        if changed(&patch) {
            self.edit("Entity definition changed", |p| {
                p.update_entity_definition(uid, &patch)
            });
        }
        if let Some(value) = tags(ui, "Tags", &def["tags"]) {
            self.edit("Entity tags changed", |p| {
                p.update_entity_definition(uid, &json!({"tags":value}))
            });
        }
        let mut atlas = ui
            .ctx()
            .data_mut(|data| data.get_temp::<i64>(egui::Id::new(("entity-atlas", uid))))
            .or_else(|| def["tilesetId"].as_i64())
            .or_else(|| def["tileRect"]["tilesetUid"].as_i64())
            .unwrap_or_else(|| {
                self.project.as_ref().unwrap().json["defs"]["tilesets"][0]["uid"]
                    .as_i64()
                    .unwrap_or(-1)
            });
        self.tileset_choice(ui, "Entity icon tileset", &mut atlas);
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new(("entity-atlas", uid)), atlas));
        if atlas >= 0 {
            self.atlas_rect_picker(&ui.ctx().clone(), ui, atlas);
        }
        ui.horizontal(|ui| {
            if source_controls::button(ui, "Edit custom fields").clicked() {
                self.field_owner = Some(uid);
                self.field_uid = None;
                self.show_panel(Some(Panel::Fields));
            }
            if source_controls::button(ui, "Use selected tile").clicked()
                && let Some(rect) = self.selected_tile_rect_for(atlas)
            {
                self.edit("Entity tile changed", |p| {
                    p.update_entity_definition(
                        uid,
                        &json!({"tilesetId":atlas,"tileRect":rect,"renderMode":"Tile"}),
                    )
                });
            }
            if source_controls::button(ui, "Delete entity definition").clicked() {
                self.edit_recoverable("Entity definition deleted", |p| {
                    p.remove_definition("entities", uid)
                });
                self.definition_uid = None;
            }
        });
    }

    fn entity_properties(&mut self, ui: &mut egui::Ui) {
        self.entity_inspector_contents(ui);
    }

    pub(super) fn instance_fields(
        &mut self,
        ui: &mut egui::Ui,
        key: LevelKey,
        entity: Option<&str>,
        owner: &Value,
    ) {
        let fields = owner["fieldInstances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2., 10.);
            for field in fields {
                let Some(uid) = field["defUid"].as_i64() else {
                    continue;
                };
                let Some(definition) = self.find_field_definition(uid) else {
                    continue;
                };
                ui.push_id((key.world, key.level, entity, uid), |ui| {
                    let array = text(&field, "__type", "").starts_with("Array<");
                    let using_default = if array {
                        field["__value"].as_array().is_none_or(Vec::is_empty)
                    } else {
                        ui_fields::field_uses_default(&field, 0)
                    };
                    let background = ui.painter().add(egui::Shape::Noop);
                    let width = ui.available_width();
                    let mut value_rect = Rect::NOTHING;
                    let row =
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            let label = ui
                                .allocate_ui_with_layout(
                                    egui::vec2(120., 26.),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_width(120.);
                                        let name = text(&field, "__identifier", "Field");
                                        let label = if array {
                                            let count =
                                                field["__value"].as_array().map_or(0, Vec::len);
                                            if let Some(max) = definition["arrayMaxLength"].as_u64()
                                            {
                                                format!("{name} {count}/{max}")
                                            } else {
                                                format!("{name} {count}")
                                            }
                                        } else {
                                            name.to_owned()
                                        };
                                        ui.add_sized(
                                            [112., 26.],
                                            egui::Label::new(RichText::new(label).color(
                                                if using_default {
                                                    Color32::from_rgb(115, 127, 157)
                                                } else {
                                                    Color32::WHITE
                                                },
                                            ))
                                            .halign(egui::Align::RIGHT)
                                            .truncate()
                                            .sense(Sense::click()),
                                        )
                                        .on_hover_text(text(&definition, "doc", name))
                                    },
                                )
                                .inner;
                            label.context_menu(|ui| {
                                if ui.button("Edit field definition").clicked() {
                                    self.field_uid = Some(uid);
                                    self.field_owner = owner["defUid"].as_i64();
                                    if let Some(owner) = self.field_owner {
                                        self.definition_uid = Some(owner);
                                        self.show_panel(Some(Panel::Entities));
                                    } else {
                                        self.show_panel(Some(Panel::Fields));
                                    }
                                    ui.close();
                                }
                            });
                            let value = ui.allocate_ui_with_layout(
                                egui::vec2((width - 122.).max(30.), 26.),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width((width - 122.).max(30.));
                                    egui::Frame::new().inner_margin(2).show(ui, |ui| {
                                        let kind = text(&field, "__type", "");
                                        if kind == "Point"
                                            || kind == "EntityRef"
                                            || kind == "Array<Point>"
                                            || kind == "Array<EntityRef>"
                                        {
                                            let _ = self.field_pick_controls(
                                                ui,
                                                key,
                                                entity,
                                                &field,
                                                &definition,
                                            );
                                        } else if let Some(value) = field_value_editor(
                                            ui,
                                            self.project.as_ref().unwrap(),
                                            &field,
                                            &definition,
                                        ) {
                                            if entity.is_some() {
                                                self.edit_level("Field changed", key, |p| {
                                                    value.apply(p, key, entity, uid)
                                                });
                                            } else if self
                                                .edit("Field changed", |p| {
                                                    value.apply(p, key, entity, uid)
                                                })
                                                .is_some()
                                            {
                                                self.history.clear_level_timelines();
                                            }
                                        }
                                        if let Ok(errors) = self
                                            .project
                                            .as_ref()
                                            .unwrap()
                                            .field_value_errors(key, entity, uid)
                                        {
                                            for error in errors {
                                                ui.colored_label(Color32::LIGHT_RED, error);
                                            }
                                        }
                                    });
                                },
                            );
                            value_rect = value.response.rect;
                        });
                    let rect = row.response.rect;
                    let mut shapes = Vec::new();
                    if !using_default {
                        shapes.push(egui::Shape::rect_filled(
                            Rect::from_min_size(rect.min, egui::vec2(120., rect.height())),
                            egui::CornerRadius {
                                nw: 4,
                                sw: 4,
                                ne: 0,
                                se: 0,
                            },
                            Color32::from_rgb(115, 127, 157),
                        ));
                        shapes.push(egui::Shape::rect_filled(
                            value_rect,
                            egui::CornerRadius {
                                nw: 0,
                                sw: 0,
                                ne: 4,
                                se: 4,
                            },
                            theme::MEDIUM,
                        ));
                    }
                    ui.painter().set(background, egui::Shape::Vec(shapes));
                });
            }
        });
    }

    fn field_properties(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let owner = self.field_owner;
        let fields = owner
            .and_then(|uid| {
                project.json["defs"]["entities"]
                    .as_array()?
                    .iter()
                    .find(|d| d["uid"] == uid)
            })
            .map(|e| e["fieldDefs"].clone())
            .unwrap_or_else(|| project.json["defs"]["levelFields"].clone());
        let fields = fields.as_array().cloned().unwrap_or_default();
        if !self.source_panel_active(ui) {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_name);
                if source_controls::button(ui, "Add field").clicked() {
                    let name = if self.new_name.is_empty() {
                        format!("Field_{}", fields.len())
                    } else {
                        self.new_name.clone()
                    };
                    if let Some(uid) = self.edit("Field definition created", |p| {
                        p.add_field_definition(owner, &name, "Int", false)
                    }) {
                        self.field_uid = Some(uid);
                        self.new_name.clear();
                    }
                }
            });
        }
        if !self.source_panel_active(ui) {
            ui.horizontal_wrapped(|ui| {
                for field in &fields {
                    let uid = field["uid"].as_i64().unwrap_or(-1);
                    if ui
                        .selectable_label(
                            self.field_uid == Some(uid),
                            text(field, "identifier", "Field"),
                        )
                        .clicked()
                    {
                        self.field_uid = Some(uid);
                    }
                }
            });
        }
        if !fields.iter().any(|f| f["uid"].as_i64() == self.field_uid) {
            self.field_uid = fields.first().and_then(|f| f["uid"].as_i64());
        }
        if !self.source_panel_active(ui)
            && let Some(index) = fields
                .iter()
                .position(|f| f["uid"].as_i64() == self.field_uid)
        {
            let uid = fields[index]["uid"].as_i64().unwrap();
            ui.horizontal(|ui| {
                if source_controls::add_enabled_button(ui, index > 0, egui::Button::new("Field up"))
                    .clicked()
                {
                    self.edit("Field reordered", |p| {
                        p.reorder_field_definition(uid, index - 1)
                    });
                }
                if source_controls::add_enabled_button(
                    ui,
                    index + 1 < fields.len(),
                    egui::Button::new("Field down"),
                )
                .clicked()
                {
                    self.edit("Field reordered", |p| {
                        p.reorder_field_definition(uid, index + 1)
                    });
                }
            });
        }
        let Some(def) = fields
            .into_iter()
            .find(|d| d["uid"].as_i64() == self.field_uid)
        else {
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        if self.source_panel_active(ui) {
            self.source_field_properties(ui, &def);
            return;
        }
        let old_kind = text(&def, "__type", "Int");
        let mut kind = old_kind
            .strip_prefix("Array<")
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(old_kind)
            .to_owned();
        let mut array = def["isArray"].as_bool().unwrap_or(false);
        let old = (kind.clone(), array);
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("field-type")
                .selected_text(&kind)
                .show_ui(ui, |ui| {
                    for item in [
                        "Int",
                        "Float",
                        "Bool",
                        "String",
                        "Multilines",
                        "FilePath",
                        "Color",
                        "Point",
                        "Tile",
                        "EntityRef",
                    ] {
                        ui.selectable_value(&mut kind, item.into(), item);
                    }
                    let p = self.project.as_ref().unwrap();
                    for group in ["enums", "externalEnums"] {
                        if let Some(defs) = p.json["defs"][group].as_array() {
                            for d in defs {
                                let name = format!(
                                    "{}.{}",
                                    if group == "enums" {
                                        "LocalEnum"
                                    } else {
                                        "ExternEnum"
                                    },
                                    text(d, "identifier", "Enum")
                                );
                                ui.selectable_value(&mut kind, name.clone(), name);
                            }
                        }
                    }
                });
            ui.checkbox(&mut array, "Array");
        });
        if old != (kind.clone(), array) {
            self.edit_recoverable("Field type converted", |p| {
                p.convert_field_type(uid, &kind, array, true)
            });
        }
        let patch = form(
            ui,
            &def,
            &[
                P("identifier", "Field identifier", W::Text),
                P("doc", "Description", W::Multiline),
                P("canBeNull", "Allow null", W::Bool),
                P(
                    "min",
                    "Minimum",
                    W::Float(-1_000_000_000.0, 1_000_000_000.0),
                ),
                P(
                    "max",
                    "Maximum",
                    W::Float(-1_000_000_000.0, 1_000_000_000.0),
                ),
                P(
                    "arrayMinLength",
                    "Array minimum length",
                    W::Int(0, 1_000_000),
                ),
                P(
                    "arrayMaxLength",
                    "Array maximum length",
                    W::Int(0, 1_000_000),
                ),
                P("regex", "String filter", W::Text),
                P("acceptFileTypes", "Accepted file types", W::Text),
                P(
                    "editorDisplayMode",
                    "Editor display",
                    W::Choice(&[
                        "Hidden",
                        "ValueOnly",
                        "NameAndValue",
                        "EntityTile",
                        "LevelTile",
                        "Points",
                        "PointStar",
                        "PointPath",
                        "PointPathLoop",
                        "RadiusPx",
                        "RadiusGrid",
                        "ArrayCountNoLabel",
                        "ArrayCountWithLabel",
                        "RefLinkBetweenPivots",
                        "RefLinkBetweenCenters",
                    ]),
                ),
                P(
                    "editorDisplayPos",
                    "Display position",
                    W::Choice(&["Above", "Center", "Beneath"]),
                ),
                P("editorDisplayScale", "Display scale", W::Float(0.1, 10.0)),
                P(
                    "editorLinkStyle",
                    "Link style",
                    W::Choice(&[
                        "StraightArrow",
                        "CurvedArrow",
                        "ZigZag",
                        "DashedLine",
                        "ArrowsLine",
                        "StraightDottedLine",
                    ]),
                ),
                P("editorTextPrefix", "Text prefix", W::Text),
                P("editorTextSuffix", "Text suffix", W::Text),
                P("editorAlwaysShow", "Always display", W::Bool),
                P("editorShowInWorld", "Show in world", W::Bool),
                P("editorCutLongValues", "Show excerpt", W::Bool),
                P("useForSmartColor", "Override entity color", W::Bool),
                P("symmetricalRef", "Symmetrical references", W::Bool),
                P("autoChainRef", "Chain references", W::Bool),
                P(
                    "allowOutOfLevelRef",
                    "Allow cross-level references",
                    W::Bool,
                ),
                P(
                    "allowedRefs",
                    "Allowed reference entities",
                    W::Choice(&["Any", "OnlySame", "OnlyTags", "OnlySpecificEntity"]),
                ),
                P("exportToToc", "Export field to table of contents", W::Bool),
                P("searchable", "Include in search", W::Bool),
            ],
        );
        if changed(&patch) {
            self.edit("Field definition changed", |p| {
                p.update_field_definition(uid, &patch)
            });
        }
        if let Some(value) = tags(ui, "Allowed reference tags", &def["allowedRefTags"]) {
            self.edit("Reference tags changed", |p| {
                p.update_field_definition(uid, &json!({"allowedRefTags":value}))
            });
        }
        ui.label("Default value");
        if let Ok(instance) = self.project.as_ref().unwrap().make_field_instance(&def)
            && let Some(value) = scalar_editor(
                ui,
                self.project.as_ref().unwrap(),
                &kind,
                &instance["__value"],
                &def,
            )
        {
            self.edit("Field default changed", |p| p.set_field_default(uid, value));
        }
        if source_controls::button(ui, "Delete field").clicked() {
            self.edit_recoverable("Field deleted", |p| p.remove_field_definition(uid));
            self.field_uid = None;
        }
    }

    fn enum_properties(&mut self, ui: &mut egui::Ui) {
        if !self.source_panel_active(ui) {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_name);
                if source_controls::button(ui, "Add enum").clicked() {
                    let base = if self.new_name.is_empty() {
                        "NewEnum"
                    } else {
                        &self.new_name
                    };
                    let name = unique_name(
                        base,
                        self.project.as_ref().unwrap().json["defs"]["enums"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|e| text(e, "identifier", "")),
                    );
                    if let Some(uid) =
                        self.edit("Enum created", |p| p.add_enum_definition(&name, &[]))
                    {
                        self.definition_uid = Some(uid);
                        self.new_name.clear();
                    }
                }
                self.import_enum_menu(ui);
            });
        }
        if let Some(def) = self.definition_picker(ui, "enums") {
            let uid = def["uid"].as_i64().unwrap();
            let patch = form(ui, &def, &[P("identifier", "Enum identifier", W::Text)]);
            if changed(&patch) {
                self.edit("Enum renamed", |p| p.update_enum_definition(uid, &patch));
            }
            let mut atlas = def["iconTilesetUid"].as_i64().unwrap_or(-1);
            let old_atlas = atlas;
            self.tileset_choice(ui, "Enum icon tileset", &mut atlas);
            if atlas != old_atlas {
                let has_icons = def["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|v| !v["tileRect"].is_null());
                let operation = |p: &mut Project| {
                    let mut values = def["values"].as_array().cloned().unwrap_or_default();
                    for v in &mut values {
                        v["tileRect"] = Value::Null;
                    }
                    p.update_enum_definition(
                        uid,
                        &json!({"iconTilesetUid":(atlas>=0).then_some(atlas),"values":values}),
                    )
                };
                if has_icons {
                    self.edit_recoverable("Enum icons changed", operation);
                } else {
                    self.edit("Enum icon tileset changed", operation);
                }
            }
            if atlas >= 0 {
                self.atlas_rect_picker(&ui.ctx().clone(), ui, atlas);
            }
            let mut values = def["values"].as_array().cloned().unwrap_or_default();
            let mut edited = false;
            let mut remove = None;
            for (index, value) in values.iter_mut().enumerate() {
                let old = text(value, "id", "").to_owned();
                ui.push_id((uid, index), |ui| {
                    ui.horizontal(|ui| {
                        if let Some(id) =
                            ui_fields::buffered_text(ui, ("enum-value-name", uid, index), &old)
                            && self
                                .edit("Enum value renamed", |p| {
                                    p.rename_enum_value(uid, &old, &id)
                                })
                                .is_some()
                        {
                            value["id"] = json!(id);
                        }
                        let mut color = enum_color(value);
                        if source_colors::color_editor(ui, &mut color).changed() {
                            value["color"] = json!(
                                ((color[0] as u32) << 16)
                                    | ((color[1] as u32) << 8)
                                    | color[2] as u32
                            );
                            edited = true;
                        }
                        if source_controls::small_button(ui, "Use tile").clicked()
                            && let Some(tile) = self.selected_tile_rect_for(atlas)
                        {
                            value["tileRect"] = tile;
                            edited = true;
                        }
                        if source_controls::small_button(ui, "Clear tile").clicked() {
                            value["tileRect"] = Value::Null;
                            edited = true;
                        }
                        if source_controls::small_button(ui, "×").clicked() {
                            remove = Some(text(value, "id", "").to_owned());
                        }
                    });
                });
            }
            if let Some(value) = remove
                && self
                    .edit_recoverable("Enum value removed", |p| p.remove_enum_value(uid, &value))
                    .is_some()
            {
                values.retain(|v| v["id"] != value);
            }
            if source_controls::button(ui, "Add enum value").clicked() {
                let name = unique_name("Value", values.iter().map(|v| text(v, "id", "")));
                let color = self.suggested_color(&values);
                values.push(json!({"id":name,"color":color,"tileRect":null}));
                edited = true;
            }
            if edited {
                self.edit("Enum values changed", |p| {
                    p.update_enum_definition(uid, &json!({"values":values}))
                });
            }
            if source_controls::button(ui, "Delete enum").clicked() {
                self.edit_recoverable("Enum deleted", |p| p.remove_definition("enums", uid));
                self.definition_uid = None;
            }
        }
        ui.collapsing("External enums", |ui| {
            let defs = self.project.as_ref().unwrap().json["defs"]["externalEnums"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut sources = std::collections::BTreeSet::new();
            for d in &defs {
                let source = text(d, "externalRelPath", "");
                if sources.insert(source.to_owned()) {
                    ui.horizontal(|ui| {
                        ui.label(source);
                        if source_controls::small_button(ui, "Sync…").clicked() {
                            let base = self
                                .project
                                .as_ref()
                                .unwrap()
                                .path
                                .as_ref()
                                .and_then(|p| p.parent())
                                .unwrap_or(std::path::Path::new("."));
                            self.begin_enum_sync(ui.ctx(), base.join(source));
                        }
                    });
                }
                ui.label(format!(
                    "{}: {}",
                    text(d, "identifier", "Enum"),
                    d["values"]
                        .as_array()
                        .map(|a| a
                            .iter()
                            .map(|v| text(v, "id", ""))
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()
                ));
            }
        });
    }

    fn tileset_properties(&mut self, ui: &mut egui::Ui) {
        if !self.source_panel_active(ui) {
            ui.horizontal(|ui| {
                ui.label("Grid");
                ui.add(egui::DragValue::new(&mut self.new_grid).range(1..=1024));
                if source_controls::button(ui, "Import tileset image…").clicked()
                    && let Some(path) = self.pick_project_file(
                        "PickImage",
                        &["png", "jpg", "jpeg", "gif", "ase", "aseprite"],
                    )
                {
                    match ldtk_core::exports::load_image_rgba(&path) {
                        Ok(image) => {
                            let grid = self.new_grid;
                            if let Some(uid) = self.edit("Tileset imported", |p| {
                                p.add_tileset(
                                    &path,
                                    grid,
                                    image.width() as i32,
                                    image.height() as i32,
                                )
                            }) {
                                self.definition_uid = Some(uid);
                            }
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
            });
        }
        let Some(def) = self.definition_picker(ui, "tilesets") else {
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        let options = remap_options(
            ui,
            ("tileset", uid),
            false,
            ldtk_core::TilesetRemap::AtlasCoordinates,
        );
        let patch = form(
            ui,
            &def,
            &[
                P("identifier", "Tileset identifier", W::Text),
                P("relPath", "Image", W::Path),
                P("tileGridSize", "Tile size", W::Int(1, 1024)),
                P("padding", "Atlas padding", W::Int(0, 1024)),
                P("spacing", "Tile spacing", W::Int(0, 1024)),
                P(
                    "tagsSourceEnumUid",
                    "Tag source enum",
                    W::Int(-1, i32::MAX as i64),
                ),
            ],
        );
        if changed(&patch) {
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                relative_path_patch(p, &mut patch, "relPath")?;
                if let Some(rel) = patch["relPath"].as_str() {
                    let image =
                        ldtk_core::exports::load_image_rgba(&p.make_absolute_file_path(rel)?)?;
                    patch["pxWid"] = json!(image.width());
                    patch["pxHei"] = json!(image.height());
                }
                p.update_tileset_definition_with_policy(
                    uid,
                    &patch,
                    options.mode,
                    options.allow_loss,
                )
            };
            let recoverable = patch.get("tileGridSize").is_some()
                || patch.get("tagsSourceEnumUid").is_some()
                    && def["enumTags"].as_array().is_some_and(|a| !a.is_empty());
            let discarded = if recoverable {
                self.edit_recoverable("Tileset grid or tags changed", operation)
            } else {
                self.edit("Tileset changed", operation)
            };
            if let Some(discarded) = discarded
                && discarded > 0
            {
                self.status = format!("Tileset changed; removed {discarded} references");
            }
            self.assets = Assets::default();
        }
        if let Some(value) = tags(ui, "Tags", &def["tags"]) {
            self.edit("Tileset tags changed", |p| {
                p.update_tileset_definition(uid, &json!({"tags":value}))
            });
        }
        self.atlas_picker(&ui.ctx().clone(), ui, uid);
        ui.collapsing("Saved tile selections", |ui| {
            let mut selections = def["savedSelections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut edited = false;
            let mut remove = None;
            for (index, selection) in selections.iter_mut().enumerate() {
                ui.push_id(("saved-selection", uid, index), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Selection {} ({} tiles)",
                            index + 1,
                            selection["ids"].as_array().map_or(0, Vec::len)
                        ));
                        let mut mode = text(selection, "mode", "Stamp").to_owned();
                        let old = mode.clone();
                        egui::ComboBox::from_id_salt(ui.id().with("mode"))
                            .selected_text(&mode)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut mode, "Stamp".into(), "Stamp");
                                ui.selectable_value(&mut mode, "Random".into(), "Random");
                            });
                        if mode != old {
                            selection["mode"] = json!(mode);
                            edited = true;
                        }
                        if source_controls::small_button(ui, "Use").clicked()
                            && let Some(ids) = selection["ids"].as_array()
                        {
                            let ids = ids
                                .iter()
                                .filter_map(Value::as_i64)
                                .filter_map(|id| i32::try_from(id).ok())
                                .collect();
                            if self.set_atlas_selection(uid, ids) {
                                self.random_tiles = mode == "Random";
                            }
                        }
                        if source_controls::small_button(ui, "Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                });
            }
            if let Some(index) = remove {
                selections.remove(index);
                edited = true;
            }
            if source_controls::button(ui, "Save selected tiles").clicked() {
                let ids = self.selected_tile_ids_for(uid);
                if !ids.is_empty() {
                    selections.push(
                        json!({"ids":ids,"mode":if self.random_tiles{"Random"}else{"Stamp"}}),
                    );
                    edited = true;
                }
            }
            if edited {
                self.edit("Saved tile selections changed", |p| {
                    p.update_tileset_definition(uid, &json!({"savedSelections":selections}))
                });
            }
        });
        ui.collapsing("Selected tile data", |ui| {
            ui.label(format!("Tile #{}", self.tile_id));
            let existing = def["customData"]
                .as_array()
                .and_then(|a| a.iter().find(|v| v["tileId"] == self.tile_id));
            let mut data = existing
                .and_then(|v| v["data"].as_str())
                .unwrap_or("")
                .to_owned();
            if let Some(next) = text_editor::value_editor(
                ui,
                ("tileset-custom-data", uid, self.tile_id),
                &data,
                "Tile custom data",
                "LangJson",
                false,
            ) {
                data = next;
                let tile = self.tile_id;
                let mut entries = def["customData"].as_array().cloned().unwrap_or_default();
                entries.retain(|v| v["tileId"] != tile);
                if !data.is_empty() {
                    entries.push(json!({"tileId":tile,"data":data}));
                }
                self.edit("Tile data changed", |p| {
                    p.update_tileset_definition(uid, &json!({"customData":entries}))
                });
            }
            let enum_uid = def["tagsSourceEnumUid"].as_i64();
            if let Some(enum_def) = enum_uid
                .and_then(|id| {
                    self.project.as_ref().unwrap().json["defs"]["enums"]
                        .as_array()?
                        .iter()
                        .chain(
                            self.project.as_ref().unwrap().json["defs"]["externalEnums"]
                                .as_array()
                                .into_iter()
                                .flatten(),
                        )
                        .find(|d| d["uid"] == id)
                })
                .cloned()
                && let Some(values) = enum_def["values"].as_array()
            {
                for value in values {
                    let id = text(value, "id", "");
                    let mut enabled = def["enumTags"].as_array().is_some_and(|tags| {
                        tags.iter().any(|tag| {
                            tag["enumValueId"] == id
                                && tag["tileIds"]
                                    .as_array()
                                    .is_some_and(|ids| ids.contains(&json!(self.tile_id)))
                        })
                    });
                    if ui.checkbox(&mut enabled, id).changed() {
                        let mut entries = def["enumTags"].as_array().cloned().unwrap_or_default();
                        let pos = entries
                            .iter()
                            .position(|v| v["enumValueId"] == id)
                            .unwrap_or_else(|| {
                                entries.push(json!({"enumValueId":id,"tileIds":[]}));
                                entries.len() - 1
                            });
                        let ids = entries[pos]["tileIds"].as_array_mut().unwrap();
                        ids.retain(|v| *v != self.tile_id);
                        if enabled {
                            ids.push(json!(self.tile_id));
                        }
                        self.edit("Tile tag changed", |p| {
                            p.update_tileset_definition(uid, &json!({"enumTags":entries}))
                        });
                    }
                }
            }
        });
        if source_controls::button(ui, "Delete tileset").clicked() {
            self.edit_recoverable("Tileset deleted", |p| p.remove_definition("tilesets", uid));
            self.definition_uid = None;
        }
    }

    fn rule_properties(&mut self, ui: &mut egui::Ui) {
        let Some(def) = self.definition_picker(ui, "layers") else {
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        if def["type"] != "IntGrid" && def["type"] != "AutoLayer" {
            ui.label("Choose an IntGrid or AutoLayer.");
            return;
        }
        let mut atlas = def["tilesetDefUid"].as_i64().unwrap_or(-1);
        let old_atlas = atlas;
        self.tileset_choice(ui, "Rule output tileset", &mut atlas);
        if atlas != old_atlas {
            self.edit_recoverable("Auto-layer tileset changed", |p| {
                p.assign_tileset_with_policy(
                    uid,
                    (atlas >= 0).then_some(atlas),
                    ldtk_core::TilesetRemap::TileIds,
                    false,
                    false,
                )?;
                for key in p.level_keys() {
                    p.rebuild_auto_layers(key)?;
                }
                Ok(())
            });
        }
        let fields = self.project.as_ref().unwrap().json["defs"]["levelFields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let biome_fields = fields
            .iter()
            .filter(|f| text(f, "__type", "").contains("Enum."))
            .collect::<Vec<_>>();
        let mut biome = def["biomeFieldUid"].as_i64().unwrap_or(-1);
        let old_biome = biome;
        egui::ComboBox::from_id_salt(("rule-biome-field", uid))
            .selected_text(
                biome_fields
                    .iter()
                    .find(|f| f["uid"] == biome)
                    .map(|f| text(f, "identifier", "Biome"))
                    .unwrap_or("No biome field"),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut biome, -1, "No biome field");
                for field in &biome_fields {
                    ui.selectable_value(
                        &mut biome,
                        field["uid"].as_i64().unwrap_or(-1),
                        text(field, "identifier", "Biome"),
                    );
                }
            });
        if biome != old_biome {
            self.edit_recoverable("Auto-layer biome changed", |p| {
                p.update_layer_properties(
                    uid,
                    &json!({"biomeFieldUid":if biome<0{Value::Null}else{json!(biome)}}),
                )?;
                for key in p.level_keys() {
                    p.rebuild_auto_layers(key)?;
                }
                Ok(())
            });
        }
        ui.horizontal(|ui| {
            if source_controls::button(ui, "New rule group").clicked()
                && let Some(group) = self.edit("Rule group created", |p| {
                    p.add_auto_rule_group(uid, "New group")
                })
            {
                self.rule_group_uid = Some(group);
            }
            if source_controls::button(ui, "Rebuild all").clicked() {
                self.edit("Auto-layers rebuilt", |p| {
                    for key in p.level_keys() {
                        p.rebuild_auto_layers(key)?;
                    }
                    Ok(())
                });
            }
        });
        ui.horizontal(|ui| {
            if source_controls::button(ui, "Rules wizard…").clicked() {
                self.begin_rule_wizard(ui.ctx(), uid, None);
            }
            if self.rule_group_uid.is_some()
                && source_controls::button(ui, "Edit group with wizard…").clicked()
            {
                self.begin_rule_wizard(ui.ctx(), uid, self.rule_group_uid);
            }
        });
        let groups = def["autoRuleGroups"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (index, group) in groups.iter().enumerate() {
                let id = group["uid"].as_i64().unwrap_or(-1);
                let response = ui.add(
                    egui::Button::new(format!("≡ {}", text(group, "name", "Group")))
                        .selected(self.rule_group_uid == Some(id))
                        .sense(Sense::click_and_drag()),
                );
                response.dnd_set_drag_payload((uid, id));
                if response.clicked() {
                    self.rule_group_uid = Some(id);
                    self.rule_uid = None;
                    self.clipboard_focus(
                        ui.ctx(),
                        ClipTarget::Rule {
                            layer: uid,
                            group: id,
                            rule: None,
                        },
                    );
                }
                if let Some(payload) = response.dnd_release_payload::<(i64, i64)>()
                    && payload.0 == uid
                    && payload.1 != id
                {
                    self.edit("Rule group reordered", |p| {
                        p.reorder_auto_rule_group(uid, payload.1, index)
                    });
                }
                response.context_menu(|ui| {
                    self.clipboard_menu(
                        ui,
                        ClipTarget::Rule {
                            layer: uid,
                            group: id,
                            rule: None,
                        },
                    )
                });
            }
        });
        if !groups
            .iter()
            .any(|g| g["uid"].as_i64() == self.rule_group_uid)
        {
            self.rule_group_uid = groups.first().and_then(|g| g["uid"].as_i64());
        }
        let Some(group) = groups
            .iter()
            .find(|g| g["uid"].as_i64() == self.rule_group_uid)
        else {
            return;
        };
        let gid = group["uid"].as_i64().unwrap();
        let patch = form(
            ui,
            group,
            &[
                P("name", "Group name", W::Text),
                P("active", "Active", W::Bool),
                P("isOptional", "Optional group", W::Bool),
                P("color", "Group color", W::NullableColor),
            ],
        );
        if changed(&patch) {
            self.edit("Rule group changed", |p| {
                p.update_auto_rule_group(uid, gid, &patch)
            });
        }
        if let Some(field) = biome_fields.iter().find(|f| f["uid"] == biome) {
            let public = text(field, "__type", "")
                .trim_start_matches("Array<")
                .trim_end_matches('>');
            let enum_name = public.split_once('.').map(|(_, s)| s).unwrap_or("");
            let values = self.project.as_ref().unwrap().json["defs"]["enums"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(
                    self.project.as_ref().unwrap().json["defs"]["externalEnums"]
                        .as_array()
                        .into_iter()
                        .flatten(),
                )
                .find(|e| e["identifier"] == enum_name)
                .and_then(|e| e["values"].as_array())
                .cloned()
                .unwrap_or_default();
            ui.label("Required biome values");
            let mut required = group["requiredBiomeValues"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut changed_values = false;
            ui.horizontal_wrapped(|ui| {
                for value in values {
                    let id = text(&value, "id", "");
                    let mut enabled = required.contains(&json!(id));
                    if ui.checkbox(&mut enabled, id).changed() {
                        required.retain(|v| v != id);
                        if enabled {
                            required.push(json!(id));
                        }
                        changed_values = true;
                    }
                }
            });
            let mut mode = group["biomeRequirementMode"].as_i64().unwrap_or(0);
            let old_mode = mode;
            ui.horizontal(|ui| {
                ui.radio_value(&mut mode, 0, "Match any");
                ui.radio_value(&mut mode, 1, "Match all");
            });
            if changed_values || mode != old_mode {
                self.edit("Rule biome requirements changed", |p| {
                    p.update_auto_rule_group(
                        uid,
                        gid,
                        &json!({"requiredBiomeValues":required,"biomeRequirementMode":mode}),
                    )
                });
            }
        }
        ui.horizontal(|ui| {
            let index = groups.iter().position(|g| g["uid"] == gid).unwrap_or(0);
            if source_controls::add_enabled_button(ui, index > 0, egui::Button::new("Group up"))
                .clicked()
            {
                self.edit("Rule group reordered", |p| {
                    p.reorder_auto_rule_group(uid, gid, index - 1)
                });
            }
            if source_controls::add_enabled_button(
                ui,
                index + 1 < groups.len(),
                egui::Button::new("Group down"),
            )
            .clicked()
            {
                self.edit("Rule group reordered", |p| {
                    p.reorder_auto_rule_group(uid, gid, index + 1)
                });
            }
            if group["usesWizard"] != true
                && source_controls::button(ui, "Add rule").clicked()
                && let Some(rule) = self.edit("Auto rule created", |p| p.add_auto_rule(uid, gid, 3))
            {
                self.rule_uid = Some(rule);
            }
            if source_controls::button(ui, "Duplicate and remap…").clicked() {
                self.begin_group_remap(ui.ctx(), uid, gid);
            }
            if source_controls::button(ui, "Delete group").clicked() {
                self.edit_recoverable("Rule group deleted", |p| p.remove_auto_rule_group(uid, gid));
                self.rule_group_uid = None;
                self.rule_uid = None;
            }
        });
        ui.horizontal(|ui| {
            if source_controls::button(ui, "Use selected tile as group icon").clicked()
                && let Some(rect) = self.selected_tile_rect_for(atlas)
            {
                self.edit("Rule group icon changed", |p| {
                    p.update_auto_rule_group(uid, gid, &json!({"icon":rect}))
                });
            }
            if !group["icon"].is_null() && source_controls::button(ui, "Clear group icon").clicked()
            {
                self.edit("Rule group icon cleared", |p| {
                    p.update_auto_rule_group(uid, gid, &json!({"icon":null}))
                });
            }
        });
        if group["usesWizard"] == true {
            ui.label("These rules are managed by the Assistant. Use the wizard to change their layout or enable advanced mode to edit individual rules.");
            if source_controls::button(ui, "Edit using the Assistant").clicked() {
                self.begin_rule_wizard(ui.ctx(), uid, Some(gid));
            }
            if source_controls::button(ui, "Enable advanced mode").clicked() {
                self.edit_recoverable("Enabled advanced mode on rule group", |p| {
                    p.update_auto_rule_group(uid, gid, &json!({"usesWizard":false}))
                });
            }
            return;
        }
        let rules = group["rules"].as_array().cloned().unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            for (index, rule) in rules.iter().enumerate() {
                let rid = rule["uid"].as_i64().unwrap_or(-1);
                let response = ui.add(
                    egui::Button::new(format!("≡ Rule {}", index + 1))
                        .selected(self.rule_uid == Some(rid))
                        .sense(Sense::click_and_drag()),
                );
                response.dnd_set_drag_payload((uid, gid, rid));
                if response.clicked() {
                    self.rule_uid = Some(rid);
                    self.clipboard_focus(
                        ui.ctx(),
                        ClipTarget::Rule {
                            layer: uid,
                            group: gid,
                            rule: Some(rid),
                        },
                    );
                }
                if let Some(payload) = response.dnd_release_payload::<(i64, i64, i64)>()
                    && payload.0 == uid
                    && payload.2 != rid
                {
                    self.edit("Rule reordered", |p| {
                        p.move_auto_rule_to_group(uid, payload.1, gid, payload.2, index)
                    });
                }
                response.context_menu(|ui| {
                    self.clipboard_menu(
                        ui,
                        ClipTarget::Rule {
                            layer: uid,
                            group: gid,
                            rule: Some(rid),
                        },
                    )
                });
            }
        });
        if !rules.iter().any(|r| r["uid"].as_i64() == self.rule_uid) {
            self.rule_uid = rules.first().and_then(|r| r["uid"].as_i64());
        }
        let Some(rule) = rules.iter().find(|r| r["uid"].as_i64() == self.rule_uid) else {
            return;
        };
        let rid = rule["uid"].as_i64().unwrap();
        let size = number(rule, "size", 3.0) as usize;
        let mut next_size = size;
        ui.horizontal(|ui| {
            ui.label("Pattern size");
            egui::ComboBox::from_id_salt(("rule-size", rid))
                .selected_text(format!("{size} × {size}"))
                .show_ui(ui, |ui| {
                    for size in [1, 3, 5, 7, 9] {
                        ui.selectable_value(&mut next_size, size, format!("{size} × {size}"));
                    }
                });
        });
        if next_size != size {
            let pattern = resize_rule_pattern(rule, next_size);
            self.edit("Rule pattern resized", |p| {
                p.update_auto_rule(uid, gid, rid, &json!({"size":next_size,"pattern":pattern}))
            });
        }
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|id| self.project.as_ref().unwrap().layer_def(id))
            .unwrap_or(&def);
        let values = source["intGridValues"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let value_key = egui::Id::new(("rule-pattern-value", uid));
        let mut value = ui
            .ctx()
            .data_mut(|data| data.get_temp::<i64>(value_key))
            .unwrap_or(self.int_value.max(1) as i64);
        egui::ComboBox::from_id_salt(("rule-input-value", uid))
            .selected_text(if value == 1000001 {
                "Any non-empty".into()
            } else {
                values
                    .iter()
                    .find(|v| v["value"] == value)
                    .map(|v| text(v, "identifier", "Value").to_owned())
                    .unwrap_or_else(|| value.to_string())
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut value, 1000001, "Any non-empty");
                for v in &values {
                    ui.selectable_value(
                        &mut value,
                        v["value"].as_i64().unwrap_or(1),
                        format!("{}: {}", v["value"], text(v, "identifier", "Value")),
                    );
                }
                for group in source["intGridValuesGroups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let id = group["uid"].as_i64().unwrap_or(0);
                    ui.selectable_value(
                        &mut value,
                        (id + 1) * 1000,
                        format!("Group: {}", text(group, "identifier", "Group")),
                    );
                }
            });
        ui.ctx().data_mut(|data| data.insert_temp(value_key, value));
        let mut pattern = rule["pattern"]
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![json!(0); size * size]);
        let mut pattern_changed = false;
        ui.label("Pattern: left click includes the selected value, right click excludes it, middle click clears.");
        egui::Grid::new(("rule-pattern", rid))
            .spacing([2.0, 2.0])
            .show(ui, |ui| {
                for y in 0..size {
                    for x in 0..size {
                        let index = x + y * size;
                        let v = pattern.get(index).and_then(Value::as_i64).unwrap_or(0);
                        let response = source_controls::add_sized_button(
                            ui,
                            [34.0, 34.0],
                            egui::Button::new(if v == 0 {
                                "·".to_owned()
                            } else if v.abs() == 1000001 {
                                if v > 0 { "Any".into() } else { "Empty".into() }
                            } else {
                                v.to_string()
                            })
                            .fill(if v == 0 {
                                theme::DARK
                            } else {
                                theme::MEDIUM
                            }),
                        );
                        let next = if response.clicked() {
                            Some(value)
                        } else if response.secondary_clicked() {
                            Some(-value)
                        } else if response.clicked_by(egui::PointerButton::Middle) {
                            Some(0)
                        } else {
                            None
                        };
                        if let Some(next) = next
                            && index < pattern.len()
                        {
                            pattern[index] = json!(next);
                            pattern_changed = true;
                        }
                    }
                    ui.end_row();
                }
            });
        if pattern_changed {
            self.edit("Rule pattern changed", |p| {
                p.update_auto_rule(uid, gid, rid, &json!({"pattern":pattern}))
            });
        }
        let mut outside = rule["outOfBoundsValue"].as_i64();
        let previous = outside;
        source_controls::property_label(ui, "Out-of-bounds policy", "outOfBoundsValue");
        egui::ComboBox::from_id_salt(("single-rule-outside", rid))
            .selected_text(match outside {
                None => "Do not apply outside layer bounds (default)".to_owned(),
                Some(0) => "Empty cells".into(),
                Some(v) => values
                    .iter()
                    .find(|value| value["value"] == v)
                    .map(|value| format!("{v} - {}", text(value, "identifier", "")))
                    .unwrap_or_else(|| v.to_string()),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut outside,
                    None,
                    "Do not apply outside layer bounds (default)",
                );
                ui.selectable_value(&mut outside, Some(0), "Empty cells");
                for value in &values {
                    if let Some(v) = value["value"].as_i64() {
                        ui.selectable_value(
                            &mut outside,
                            Some(v),
                            format!("{v} - {}", text(value, "identifier", "")),
                        );
                    }
                }
            });
        if outside != previous {
            self.edit("Rule out-of-bounds value changed", |p| {
                p.update_auto_rule(uid, gid, rid, &json!({"outOfBoundsValue":outside}))
            });
        }
        let patch = form(
            ui,
            rule,
            &[
                P("active", "Active", W::Bool),
                P("chance", "Chance", W::Float(0.0, 1.0)),
                P("breakOnMatch", "Break on match", W::Bool),
                P("flipX", "Horizontal symmetry", W::Bool),
                P("flipY", "Vertical symmetry", W::Bool),
                P("alpha", "Tile opacity", W::Float(0.0, 1.0)),
                P("tileMode", "Tile mode", W::Choice(&["Single", "Stamp"])),
                P(
                    "checker",
                    "Checker mode",
                    W::Choice(&["None", "Horizontal", "Vertical"]),
                ),
                P("xModulo", "X modulo", W::Int(1, 1024)),
                P("yModulo", "Y modulo", W::Int(1, 1024)),
                P("xOffset", "X offset", W::Int(-65536, 65536)),
                P("yOffset", "Y offset", W::Int(-65536, 65536)),
                P("tileXOffset", "Tile X offset", W::Int(-65536, 65536)),
                P("tileYOffset", "Tile Y offset", W::Int(-65536, 65536)),
                P("tileRandomXMin", "Random X minimum", W::Int(-65536, 65536)),
                P("tileRandomXMax", "Random X maximum", W::Int(-65536, 65536)),
                P("tileRandomYMin", "Random Y minimum", W::Int(-65536, 65536)),
                P("tileRandomYMax", "Random Y maximum", W::Int(-65536, 65536)),
                P("pivotX", "Stamp pivot X", W::Float(0.0, 1.0)),
                P("pivotY", "Stamp pivot Y", W::Float(0.0, 1.0)),
                P("perlinActive", "Perlin noise", W::Bool),
                P(
                    "perlinSeed",
                    "Perlin seed",
                    W::Int(i32::MIN as i64, i32::MAX as i64),
                ),
                P("perlinScale", "Perlin scale", W::Float(0.001, 10.0)),
                P("perlinOctaves", "Perlin octaves", W::Int(1, 10)),
            ],
        );
        if changed(&patch) {
            self.edit("Auto rule changed", |p| {
                p.update_auto_rule(uid, gid, rid, &patch)
            });
        }
        if atlas >= 0 {
            self.atlas_picker(&ui.ctx().clone(), ui, atlas);
        }
        ui.horizontal(|ui| {
            let ids = self.selected_tile_ids_for(atlas);
            if source_controls::add_enabled_button(
                ui,
                !ids.is_empty(),
                egui::Button::new("Use selected tiles"),
            )
            .clicked()
            {
                let outputs = if rule["tileMode"] == "Stamp" {
                    json!([ids])
                } else {
                    json!(ids.iter().map(|id| json!([id])).collect::<Vec<_>>())
                };
                self.edit("Rule output changed", |p| {
                    p.update_auto_rule(uid, gid, rid, &json!({"tileRectsIds":outputs}))
                });
            }
            let index = rules.iter().position(|r| r["uid"] == rid).unwrap_or(0);
            if source_controls::add_enabled_button(ui, index > 0, egui::Button::new("Rule up"))
                .clicked()
            {
                self.edit("Auto rule reordered", |p| {
                    p.reorder_auto_rule(uid, gid, rid, index - 1)
                });
            }
            if source_controls::add_enabled_button(
                ui,
                index + 1 < rules.len(),
                egui::Button::new("Rule down"),
            )
            .clicked()
            {
                self.edit("Auto rule reordered", |p| {
                    p.reorder_auto_rule(uid, gid, rid, index + 1)
                });
            }
            if source_controls::button(ui, "Delete rule").clicked() {
                self.edit("Auto rule deleted", |p| p.remove_auto_rule(uid, gid, rid));
                self.rule_uid = None;
            }
        });
        if let Some(key) = self.selected
            && let Some(index) = self
                .project
                .as_ref()
                .unwrap()
                .level(key)
                .and_then(|l| l["layerInstances"].as_array())
                .and_then(|a| a.iter().position(|l| l["layerDefUid"] == uid))
            && group["isOptional"] == true
        {
            let mut active =
                self.project.as_ref().unwrap().layer(key, index).unwrap()["optionalRules"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(gid)));
            if ui
                .checkbox(&mut active, "Enable group in current level")
                .changed()
            {
                self.edit_level("Optional rule group toggled", key, |p| {
                    let a = p.layer_mut(key, index).unwrap()["optionalRules"]
                        .as_array_mut()
                        .unwrap();
                    a.retain(|v| *v != gid);
                    if active {
                        a.push(json!(gid));
                    }
                    p.rebuild_auto_layers(key)
                });
            }
        }
    }

    pub(super) fn show_settings_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("app-settings-dialog")).show(ctx, |ui| {
            ui.heading("Application settings");
            self.settings_properties(ui);
            if source_controls::button(ui, "Check for updates").clicked() {
                self.open_updater(ui.ctx());
            }
            close = source_controls::button(ui, "Close").clicked();
        });
        if close || modal.should_close() {
            self.show_panel(None);
        }
    }
    fn settings_properties(&mut self, ui: &mut egui::Ui) {
        let patch = form(
            ui,
            &self.settings.value,
            &[
                P("openLastProject", "Reopen last project", W::Bool),
                P("startFullScreen", "Start fullscreen", W::Bool),
                P("useBestGPU", "Use best GPU", W::Bool),
                P(
                    "navigationKeys",
                    "Navigation keys",
                    W::Choice(&["Wasd", "Zqsd", "Arrows"]),
                ),
                P("appUiScale", "Application UI scale", W::Float(0.5, 5.0)),
                P("editorUiScale", "Canvas UI scale", W::Float(0.5, 5.0)),
                P("mouseWheelSpeed", "Mouse wheel speed", W::Float(0.1, 5.0)),
                P(
                    "autoWorldModeSwitch",
                    "Auto world switching",
                    W::Choice(&["Never", "ZoomOutOnly", "ZoomInAndOut"]),
                ),
                P(
                    "fieldsRender",
                    "Fields render",
                    W::Choice(&["FR_Outline", "FR_Table"]),
                ),
                P(
                    "singleLayerModeIntensity",
                    "Single layer fade",
                    W::Float(0.0, 1.0),
                ),
                P(
                    "nearbyTilesRenderingDist",
                    "Nearby tiles distance",
                    W::Float(0.0, 10.0),
                ),
                P("colorBlind", "Prefer color-blind colors", W::Bool),
                P("blurMask", "Fade window when not focused", W::Bool),
            ],
        );
        if changed(&patch) {
            apply_patch(&mut self.settings.value, &patch);
            ui.ctx()
                .set_zoom_factor(self.settings.float("appUiScale", 1.0));
            if let Some(fullscreen) = patch["startFullScreen"].as_bool() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
            }
            if let Err(e) = self.settings.save() {
                self.status = e.to_string();
            }
        }
        if let Some(path) = app_log::path(self.settings.path.as_deref()) {
            ui.separator();
            ui.label(path.display().to_string());
            ui.horizontal(|ui| {
                text_editor::log_editor(ui, &path);
                if source_controls::button(ui, "Locate logs").clicked()
                    && let Err(error) = home_files::locate(&path, true)
                {
                    self.status = error.to_string();
                    self.status_error = true;
                }
            });
        }
    }

    fn export_properties(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            return;
        }
        ui.label("Export current project");
        for (label, format) in [
            ("Super simple export", "simple"),
            ("Layer and level PNGs", "png"),
            ("IntGrid CSV files", "csv"),
            ("Tiled TMX", "tmx"),
            ("GameMaker Studio 2 rooms", "gms"),
        ] {
            if source_controls::add_enabled_button(ui, !self.is_busy(), egui::Button::new(label))
                .clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                self.export_async(format, dir);
            }
        }
        ui.small("GameMaker export follows the original experimental exporter: entity instances and generated auto-layers are not exported.");
        if source_controls::button(ui, "Import Ogmo project…").clicked() {
            self.request(ui.ctx(), Action::BrowseOgmo);
        }
        ui.separator();
        self.command_properties(ui);
    }

    fn command_properties(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let iid = text(&project.json, "iid", "").to_owned();
        let mut trusted = self.settings.trusted(&iid);
        if ui
            .checkbox(&mut trusted, "Allow this project's custom commands")
            .changed()
        {
            self.settings.set_trusted(&iid, trusted);
        }
        let mut commands = self.project.as_ref().unwrap().json["customCommands"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut modified = false;
        let mut delete = None;
        for (index, command) in commands.iter_mut().enumerate() {
            ui.push_id(index, |ui| {
                let patch = form(
                    ui,
                    command,
                    &[
                        P(
                            "when",
                            "When",
                            W::Choice(&["Manual", "AfterLoad", "BeforeSave", "AfterSave"]),
                        ),
                        P("command", "Command", W::Text),
                    ],
                );
                if changed(&patch) {
                    apply_patch(command, &patch);
                    modified = true;
                }
                ui.horizontal(|ui| {
                    if source_controls::add_enabled_button(
                        ui,
                        trusted,
                        egui::Button::new("Run command"),
                    )
                    .clicked()
                    {
                        let p = self.project.as_ref().unwrap();
                        let command = text(command, "command", "").to_owned();
                        let cwd = p.path.as_ref().and_then(|p| p.parent()).map(PathBuf::from);
                        self.run_command(command, cwd);
                    }
                    if source_controls::small_button(ui, "Delete").clicked() {
                        delete = Some(index);
                    }
                });
            });
        }
        if let Some(index) = delete {
            commands.remove(index);
            modified = true;
        }
        if source_controls::button(ui, "Add command").clicked() {
            commands.push(json!({"when":"Manual","command":""}));
            modified = true;
        }
        if modified {
            let operation = |p: &mut Project| {
                p.json["customCommands"] = json!(commands);
                Ok(())
            };
            if delete.is_some() {
                self.edit_recoverable("Project command removed", operation);
            } else {
                self.edit("Custom commands changed", operation);
            }
        }
    }

    fn definition_picker(&mut self, ui: &mut egui::Ui, kind: &str) -> Option<Value> {
        let defs = self.project.as_ref()?.json["defs"][kind].as_array()?;
        if !defs
            .iter()
            .any(|d| d["uid"].as_i64() == self.definition_uid)
        {
            self.definition_uid = defs.first().and_then(|d| d["uid"].as_i64());
        }
        let options = defs
            .iter()
            .map(|d| {
                (
                    d["uid"].as_i64().unwrap_or(-1),
                    text(d, "identifier", "Definition").to_owned(),
                )
            })
            .collect::<Vec<_>>();
        if !self.source_panel_active(ui) {
            ui.horizontal_wrapped(|ui| {
                for (uid, name) in options {
                    if ui
                        .selectable_label(self.definition_uid == Some(uid), name)
                        .clicked()
                    {
                        self.definition_uid = Some(uid);
                    }
                }
            });
        }
        self.project.as_ref()?.json["defs"][kind]
            .as_array()?
            .iter()
            .find(|d| d["uid"].as_i64() == self.definition_uid)
            .cloned()
    }
    fn definition_index(&self, kind: &str, uid: i64) -> usize {
        self.project
            .as_ref()
            .and_then(|p| p.json["defs"][kind].as_array())
            .and_then(|ds| ds.iter().position(|d| d["uid"] == uid))
            .unwrap_or(0)
    }
    fn find_field_definition(&self, uid: i64) -> Option<Value> {
        let p = self.project.as_ref()?;
        p.json["defs"]["levelFields"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                p.json["defs"]["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|e| e["fieldDefs"].as_array().into_iter().flatten()),
            )
            .find(|d| d["uid"] == uid)
            .cloned()
    }
    fn tileset_choice(&self, ui: &mut egui::Ui, label: &str, value: &mut i64) {
        let Some(p) = &self.project else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(ui.id().with(label))
                .selected_text(
                    p.tileset(*value)
                        .map(|d| text(d, "identifier", "Tileset"))
                        .unwrap_or("Choose tileset"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, -1, "None / use default");
                    if let Some(defs) = p.json["defs"]["tilesets"].as_array() {
                        for d in defs {
                            if let Some(uid) = d["uid"].as_i64() {
                                ui.selectable_value(value, uid, text(d, "identifier", "Tileset"));
                            }
                        }
                    }
                });
        });
    }
    pub(in crate::app) fn selected_tile_ids_for(&self, uid: i64) -> Vec<i32> {
        let Some(d) = self.project.as_ref().and_then(|p| p.tileset(uid)) else {
            return vec![];
        };
        let columns = number(d, "__cWid", 1.0) as i32;
        let rows = number(d, "__cHei", 1.0) as i32;
        if columns <= 0 || rows <= 0 {
            return vec![];
        }
        if let Some((atlas, ids)) = &self.atlas_selection
            && *atlas == uid
        {
            return ids
                .iter()
                .copied()
                .filter(|id| *id >= 0 && *id < columns * rows)
                .collect();
        }
        let r = self.tile_rect;
        (r[1].max(0)..(r[1] + r[3]).min(rows))
            .flat_map(|y| (r[0].max(0)..(r[0] + r[2]).min(columns)).map(move |x| x + y * columns))
            .collect()
    }
    fn selected_tile_rect_for(&self, uid: i64) -> Option<Value> {
        if self
            .atlas_selection
            .as_ref()
            .is_some_and(|(atlas, ids)| *atlas == uid && ids.is_empty())
        {
            return None;
        }
        let d = self.project.as_ref()?.tileset(uid)?;
        let g = number(d, "tileGridSize", 16.0) as i32;
        let spacing = number(d, "spacing", 0.0) as i32;
        let padding = number(d, "padding", 0.0) as i32;
        let cols = number(d, "__cWid", 1.0) as i32;
        let rows = number(d, "__cHei", 1.0) as i32;
        let r = self.tile_rect;
        let x = r[0].max(0);
        let y = r[1].max(0);
        let w = r[2].min(cols - x);
        let h = r[3].min(rows - y);
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(
            json!({"tilesetUid":uid,"x":padding+x*(g+spacing),"y":padding+y*(g+spacing),"w":w*g+(w-1)*spacing,"h":h*g+(h-1)*spacing}),
        )
    }
}

fn snap_level_form_patch(p: &Project, key: LevelKey, patch: &mut Value) -> ldtk_core::Result<()> {
    let world = if let Some(index) = key.world {
        p.json["worlds"]
            .get(index)
            .ok_or_else(|| ldtk_core::Error::Invalid("world missing".into()))?
    } else {
        &p.json
    };
    if world["worldLayout"] == "GridVania" {
        for (property, grid, positive) in [
            ("worldX", "worldGridWidth", false),
            ("worldY", "worldGridHeight", false),
            ("pxWid", "worldGridWidth", true),
            ("pxHei", "worldGridHeight", true),
        ] {
            if let Some(n) = patch[property].as_i64() {
                let grid = world[grid]
                    .as_i64()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| ldtk_core::Error::Invalid("invalid world grid".into()))?;
                let snapped = (n as f64 / grid as f64 + 0.5).floor() as i64;
                patch[property] = json!(if positive {
                    snapped.max(1) * grid
                } else {
                    snapped * grid
                });
            }
        }
    }
    Ok(())
}
pub(in crate::app) fn unique_name<'a>(
    base: &str,
    existing: impl Iterator<Item = &'a str>,
) -> String {
    let names = existing.collect::<std::collections::BTreeSet<_>>();
    if !names.contains(base) {
        return base.to_owned();
    }
    let cut = base.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, mut index) = if cut < base.len() {
        (&base[..cut], base[cut..].parse::<u64>().unwrap_or(1))
    } else {
        (base, 2)
    };
    loop {
        let name = format!("{prefix}{index}");
        if !names.contains(name.as_str()) {
            return name;
        }
        index = index.saturating_add(1);
    }
}
fn enum_color(value: &Value) -> [u8; 3] {
    let color = value["color"].as_u64().unwrap_or_else(|| {
        u64::from_str_radix(
            value["color"]
                .as_str()
                .unwrap_or("0")
                .trim_start_matches('#'),
            16,
        )
        .unwrap_or(0)
    });
    [(color >> 16) as u8, (color >> 8) as u8, color as u8]
}

fn resize_rule_pattern(rule: &Value, size: usize) -> Vec<Value> {
    let old = number(rule, "size", 1.0) as i32;
    let offset = (size as i32 - old) / 2;
    let mut pattern = vec![json!(0); size * size];
    for y in 0..old {
        for x in 0..old {
            let nx = x + offset;
            let ny = y + offset;
            if nx >= 0 && ny >= 0 && (nx as usize) < size && (ny as usize) < size {
                pattern[nx as usize + ny as usize * size] = rule["pattern"]
                    .get((x + y * old) as usize)
                    .cloned()
                    .unwrap_or(json!(0));
            }
        }
    }
    pattern
}

#[derive(Clone)]
struct BackupBrowser {
    original: PathBuf,
    backups: Vec<ldtk_core::BackupInfo>,
    restore: Option<PathBuf>,
}

#[derive(Clone, Copy)]
struct RemapOptions {
    mode: ldtk_core::TilesetRemap,
    allow_loss: bool,
    clear_overrides: bool,
}
fn remap_options(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    overrides: bool,
    default: ldtk_core::TilesetRemap,
) -> RemapOptions {
    let id = ui.make_persistent_id(("tileset-remapping", key));
    let mut options = ui
        .data_mut(|data| data.get_temp::<RemapOptions>(id))
        .unwrap_or(RemapOptions {
            mode: default,
            allow_loss: false,
            clear_overrides: false,
        });
    ui.collapsing("Existing tile references", |ui| {
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::TileIds,
            "Keep tile numbers",
        );
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::AtlasCoordinates,
            "Keep image positions",
        );
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::Clear,
            "Clear affected tiles",
        );
        ui.checkbox(
            &mut options.allow_loss,
            "Allow removal of references that no longer fit",
        );
        if overrides {
            ui.checkbox(
                &mut options.clear_overrides,
                "Reset tileset overrides in all levels",
            );
        }
    });
    ui.data_mut(|data| data.insert_temp(id, options));
    options
}

fn relative_path_patch(project: &Project, patch: &mut Value, key: &str) -> ldtk_core::Result<()> {
    if project.path.is_some()
        && let Some(path) = patch[key].as_str()
        && std::path::Path::new(path).is_absolute()
    {
        patch[key] = json!(project.make_relative_file_path(path)?);
    }
    Ok(())
}

fn normalize_background_patch(level: &Value, patch: &mut Value) {
    if let Some(path) = patch.get("bgRelPath") {
        if path.is_null() {
            patch["bgPos"] = Value::Null;
            patch["__bgPos"] = Value::Null;
        } else if level["bgPos"].is_null() && patch.get("bgPos").is_none() {
            patch["bgPos"] = json!("Cover");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_assignment_defaults_to_cover_and_removal_resets_position() {
        let mut patch = json!({"bgRelPath":"large.png"});
        normalize_background_patch(&json!({"bgPos":null}), &mut patch);
        assert_eq!(patch["bgPos"], "Cover");
        let mut patch = json!({"bgRelPath":"replacement.png"});
        normalize_background_patch(&json!({"bgPos":"Contain"}), &mut patch);
        assert!(patch.get("bgPos").is_none());
        let mut patch = json!({"bgRelPath":null});
        normalize_background_patch(&json!({"bgPos":"Cover","__bgPos":{}}), &mut patch);
        assert_eq!(patch["bgPos"], Value::Null);
        assert_eq!(patch["__bgPos"], Value::Null);
    }
    #[test]
    fn original_project_property_panels_draw_without_mutating_the_document() {
        let mut app = EditorApp::empty();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/original/Test_file_for_API_showing_all_features.ldtk");
        let mut project = Project::load(path).unwrap();
        project.migrate_to_current().unwrap();
        app.set_project(project);
        let before = app.project.as_ref().unwrap().json.clone();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        for panel in [
            Panel::Project,
            Panel::World,
            Panel::Level,
            Panel::Layers,
            Panel::Entities,
            Panel::Entity,
            Panel::Fields,
            Panel::Enums,
            Panel::Tilesets,
            Panel::Rules,
            Panel::Export,
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        app.properties(ui, panel);
                    });
                },
            );
            assert_eq!(
                app.project.as_ref().unwrap().json,
                before,
                "property rendering changed a document"
            );
        }
    }
    #[test]
    fn field_instance_label_context_menu_opens_the_exact_definition() {
        fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        let mut p = Project::new();
        let key = p.level_keys()[0];
        let uid = p
            .add_field_definition(None, "Title", "String", false)
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.panel = Some(Panel::Level);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let draw = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 400.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let owner = app.project.as_ref().unwrap().level(key).unwrap().clone();
                        app.instance_fields(ui, key, None, &owner);
                    });
                },
            )
        };
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let label = output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, "Title"))
            .unwrap();
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(label),
                    egui::Event::PointerButton {
                        pos: label,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let output = draw(&mut app, vec![]);
        let action = output
            .shapes
            .iter()
            .find_map(|shape| find(&shape.shape, "Edit field definition"))
            .expect("actual field label context menu");
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(action),
                    egui::Event::PointerButton {
                        pos: action,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(app.panel == Some(Panel::Fields));
        assert_eq!(app.field_owner, None);
        assert_eq!(app.field_uid, Some(uid));
    }
    #[test]
    fn field_instance_rows_keep_labels_inline_and_default_activation_preserves_other_wrappers() {
        fn point(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
            fn find(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
                    _ => None,
                }
            }
            output
                .shapes
                .iter()
                .find_map(|shape| find(&shape.shape, label))
                .unwrap_or_else(|| panic!("missing {label}"))
        }
        let mut p = Project::new();
        let key = p.level_keys()[0];
        let number = p.add_field_definition(None, "Count", "Int", false).unwrap();
        p.set_field_editor_value(key, None, number, json!(7))
            .unwrap();
        let title = p
            .add_field_definition(None, "Title", "String", false)
            .unwrap();
        p.set_field_default(title, json!("seed")).unwrap();
        let array = p
            .add_field_definition(None, "Words", "String", true)
            .unwrap();
        p.set_field_editor_value(key, None, array, json!([null, "keep"]))
            .unwrap();
        p.level_mut(key).unwrap()["fieldInstances"][2]["realEditorValues"][1]["extension"] =
            json!({"preserve":true});
        let untouched = p.level(key).unwrap()["fieldInstances"][2].clone();
        let mut app = EditorApp::empty();
        app.set_project(p);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.repaint = Some(ctx.clone());
        let frame = |app: &mut EditorApp, events: Vec<egui::Event>| {
            let modifiers = events
                .iter()
                .find_map(|event| match event {
                    egui::Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default();
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(360., 600.),
                )),
                events,
                modifiers,
                ..Default::default()
            };
            app.ui_commit_raw_input(&ctx, &mut input);
            ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| {
                        ui.set_width(330.);
                        let owner = app.project.as_ref().unwrap().level(key).unwrap().clone();
                        app.instance_fields(ui, key, None, &owner);
                    })
                    .inner
            })
        };
        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        assert!(
            (point(&output, "Count").y - point(&output, "7").y).abs() < 8.,
            "field label and value must share a row"
        );
        let pos = point(&output, "Default");
        let before = app.project.as_ref().unwrap().json.clone();
        for pressed in [true, false] {
            frame(
                &mut app,
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
        frame(&mut app, vec![]);
        assert_eq!(
            app.project.as_ref().unwrap().json,
            before,
            "activating a default must not materialize it"
        );
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            }],
        );
        frame(&mut app, vec![egui::Event::Text("edited".into())]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        frame(&mut app, vec![]);
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["fieldInstances"][1]["__value"],
            "edited"
        );
        assert_eq!(
            app.project.as_ref().unwrap().level(key).unwrap()["fieldInstances"][2],
            untouched
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("World.ldtk");
        app.project.as_mut().unwrap().save(&path).unwrap();
        let loaded = Project::load(path).unwrap();
        assert_eq!(
            loaded.level(key).unwrap()["fieldInstances"][1]["__value"],
            "edited"
        );
        assert_eq!(loaded.level(key).unwrap()["fieldInstances"][2], untouched);
    }
    #[test]
    fn creating_first_extra_world_through_native_button_preserves_original_world() {
        fn frame(
            app: &mut EditorApp,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> egui::FullOutput {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 900.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.world_properties(ui));
                },
            )
        }
        fn text_center(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| text_center(shape, label))
                }
                _ => None,
            }
        }
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let original = app.project.as_ref().unwrap().json.clone();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = frame(&mut app, &ctx, vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "+ World"))
            .expect("native add-world button is visible");
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.json["worlds"].as_array().unwrap().len(),
            2,
            "{}",
            app.status
        );
        assert_eq!(
            project.json["worlds"][0]["levels"][0]["iid"],
            original["levels"][0]["iid"]
        );
        assert_eq!(app.active_world, Some(1));
        assert_eq!(app.selected.unwrap().world, Some(1));
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
    #[test]
    fn gridvania_level_form_rounds_positions_and_sizes_like_source() {
        let mut p = Project::new();
        p.json["worldLayout"] = json!("GridVania");
        p.json["worldGridWidth"] = json!(256);
        p.json["worldGridHeight"] = json!(128);
        let mut patch = json!({"worldX":100,"worldY":-64,"pxWid":300,"pxHei":10});
        snap_level_form_patch(&p, p.level_keys()[0], &mut patch).unwrap();
        assert_eq!(
            patch,
            json!({"worldX":0,"worldY":0,"pxWid":256,"pxHei":128})
        );
        p.json["worldLayout"] = json!("Free");
        let mut patch = json!({"worldX":100,"pxWid":300});
        snap_level_form_patch(&p, p.level_keys()[0], &mut patch).unwrap();
        assert_eq!(patch, json!({"worldX":100,"pxWid":300}));
    }
    #[test]
    fn external_enum_preview_is_read_only_until_apply() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        std::fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let before = app.project.as_ref().unwrap().json.clone();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        app.begin_enum_sync(&ctx, path);
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.enum_sync_dialog(ctx));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.history.can_undo());
    }
}
