#[path = "application_settings.rs"]
mod application_settings;
#[path = "definition_clipboard.rs"]
mod definition_clipboard;
#[path = "entity_transfer.rs"]
mod entity_transfer;
#[path = "enum_sync_dialog.rs"]
mod enum_sync_dialog;
pub(super) use enum_sync_dialog::PendingSync;
#[path = "image_picker.rs"]
mod image_picker;
#[cfg(test)]
#[path = "layer_settings_tests.rs"]
mod layer_settings_tests;
#[path = "panel_layout.rs"]
mod panel_layout;
#[path = "rule_batch.rs"]
mod rule_batch;
#[path = "rule_detail.rs"]
mod rule_detail;
pub(super) use rule_detail::RuleDraft;
#[path = "rule_wizard.rs"]
mod rule_wizard;
#[path = "source_colors.rs"]
pub(in crate::app) mod source_colors;
#[path = "source_controls.rs"]
pub(in crate::app) mod source_controls;
#[path = "tag_editor.rs"]
mod tag_editor;
use super::ui_fields::{
    FieldEdit, Property as P, Widget as W, apply_patch, changed, field_value_editor, form,
    form_with_label_width, scalar_editor, tags,
};
use super::*;
use crate::app::chrome::ActionTooltip as _;
use definition_clipboard::ClipTarget;
use serde_json::json;
const TILESET_MIN_LABEL_WIDTH: f32 = 110.;
const EMBEDDED_TILESET_DESCRIPTION: &str = "This tileset is using an embedded image from LDtk and cannot be modified. The tileset was added to your project because an element (an Entity or an Enum) uses it.";
const EMBEDDED_TILESET_LICENSE: &str = "LICENSING: this image is only meant to be used inside LDtk and cannot be redistributed in any way. Please contact the author (FinalBossBlues) if you want to license this image for your own use in your game.";
const ENUM_VALUE_SIZE: f32 = 32.;
const ENUM_VALUE_SPACING: f32 = 3.;

#[derive(Clone)]
struct EnumValueRemoval {
    project_iid: Value,
    uid: i64,
    value: String,
    anchor: Rect,
}

fn enum_value_removal_id() -> egui::Id {
    egui::Id::new("enum-value-removal")
}

fn tileset_has_marked_tiles(tileset: &Value) -> bool {
    tileset["enumTags"].as_array().is_some_and(|tags| {
        tags.iter()
            .any(|tag| tag["tileIds"].as_array().is_some_and(|ids| !ids.is_empty()))
    })
}

fn embedded_tileset_notice(ui: &mut egui::Ui) {
    const WEBSITE: &str = "https://finalbossblues.itch.io/icons";
    const PATREON: &str = "https://www.patreon.com/finalbossblues";
    let spacing = ui.spacing().item_spacing.y;
    ui.add_space(6.);
    ui.spacing_mut().item_spacing.y = 0.;
    let card = egui::Frame::new().inner_margin(17).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 8.;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 32.),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                let support = source_controls::add_sized_button(
                    ui,
                    [99., 32.],
                    egui::Button::image_and_text(
                        egui::Image::new(egui::include_image!("../../assets/icons/love.svg"))
                            .fit_to_exact_size(egui::Vec2::splat(20.))
                            .tint(Color32::from_white_alpha(128)),
                        RichText::new("Patreon").color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(42, 114, 165))
                    .corner_radius(0.),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .detail_tooltip(PATREON);
                support.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Link, true, "Patreon")
                });
                if support.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(PATREON));
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 32.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.;
                        ui.label(RichText::new(locale::tr(ui, "Image by")).size(15.));
                        ui.label(
                            RichText::new("FinalBossBlues")
                                .font(FontId::new(15., egui::FontFamily::Name("body-bold".into()))),
                        );
                        source_controls::hyperlink(ui, "website", WEBSITE);
                    },
                );
            },
        );
        let (_, line) = ui.allocate_space(egui::vec2(ui.available_width(), 1.));
        ui.painter().rect_filled(line, 0., theme::LIGHT);
        ui.label(
            RichText::new(locale::tr(ui, EMBEDDED_TILESET_DESCRIPTION))
                .size(15.)
                .color(Color32::WHITE),
        );
        ui.label(
            RichText::new(locale::tr(ui, EMBEDDED_TILESET_LICENSE))
                .size(15.)
                .color(theme::ACCENT),
        );
    });
    ui.spacing_mut().item_spacing.y = spacing;
    let rect = card.response.rect.shrink(0.5);
    ui.painter().extend(egui::Shape::dashed_line(
        &[
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ],
        Stroke::new(1_f32, Color32::from_rgb(115, 127, 157)),
        3.,
        2.,
    ));
}

pub(super) fn enum_value_removal_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<EnumValueRemoval>(enum_value_removal_id())
            .is_some()
    })
}

#[derive(Clone, Copy)]
pub(super) enum IntGridRemoval {
    Value(i64, Rect),
    Group(i64),
}

fn collapsing(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        ui.make_persistent_id(title),
        false,
    );
    let open = panel_layout::section(ui, title, state.is_open(), |ui| {
        egui::Frame::new().inner_margin(8).show(ui, body);
    });
    state.set_open(open);
    state.store(ui.ctx());
}

fn tileset_tag_enum_picker(
    ui: &mut egui::Ui,
    project: &Project,
    current: Option<i64>,
) -> Option<Option<i64>> {
    let local = project.json["defs"]["enums"]
        .as_array()
        .into_iter()
        .flatten();
    let external = project.json["defs"]["externalEnums"]
        .as_array()
        .into_iter()
        .flatten();
    let all = local.clone().chain(external.clone());
    let selected = all
        .clone()
        .find(|e| e["uid"].as_i64() == current)
        .map(|e| text(e, "identifier", "-- None --"))
        .unwrap_or_else(|| locale::tr(ui, "-- None --"));
    let mut tag_names = local
        .clone()
        .flat_map(|e| e["tags"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    tag_names.sort_by_key(|tag| tag.to_lowercase());
    tag_names.dedup();
    let mut groups: Vec<(&str, Vec<(&Value, bool)>)> = Vec::new();
    let untagged = local
        .clone()
        .filter(|e| e["tags"].as_array().is_none_or(|tags| tags.is_empty()))
        .map(|e| (e, false))
        .collect::<Vec<_>>();
    if !untagged.is_empty() {
        groups.push((locale::tr(ui, "Untagged"), untagged));
    }
    for tag in tag_names {
        groups.push((
            tag,
            local
                .clone()
                .filter(|e| {
                    e["tags"]
                        .as_array()
                        .is_some_and(|tags| tags.iter().any(|v| v == tag))
                })
                .map(|e| (e, false))
                .collect(),
        ));
    }
    let mut sources = external
        .clone()
        .map(|e| text(e, "externalRelPath", "External enums"))
        .collect::<Vec<_>>();
    sources.sort_unstable();
    sources.dedup();
    for source in sources {
        groups.push((
            source,
            external
                .clone()
                .filter(|e| text(e, "externalRelPath", "External enums") == source)
                .map(|e| (e, true))
                .collect(),
        ));
    }
    let mut next = current;
    source_controls::form_combo("tileset-tag-source-enum")
        .width(ui.available_width().min(465.))
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next, None, locale::tr(ui, "-- None --"));
            for (name, entries) in &groups {
                if groups.len() > 1 {
                    ui.label(*name);
                }
                for (e, is_external) in entries {
                    if *is_external {
                        let label = locale::format(
                            ui,
                            "{name} (unsupported external enum)",
                            &[("name", text(e, "identifier", ""))],
                        );
                        ui.add_enabled(false, egui::Button::new(label));
                    } else if let Some(uid) = e["uid"].as_i64() {
                        ui.selectable_value(&mut next, Some(uid), text(e, "identifier", ""));
                    }
                }
            }
        });
    (next != current).then_some(next)
}

pub(super) fn source_panel_width(panel: Panel) -> f32 {
    match panel {
        Panel::Rules => 356.,
        Panel::World => 400.,
        Panel::Level => 490.,
        Panel::Fields => 790.,
        Panel::Project | Panel::Enums => 840.,
        Panel::Help => 907.,
        _ => 1000.,
    }
}

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
            Panel::Rules => self.rule_list(ui, ui.available_height()),
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
        ui.heading(locale::tr(ui, "Extra files"));
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
        ui.heading(locale::tr(ui, "Workspace settings"));
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
        collapsing(ui, "Advanced project settings", |ui| {
            for flag in [
                "MultiWorlds",
                "ExportPreCsvIntGridFormat",
                "ExportOldTableOfContentData",
                "DiscardPreCsvIntGrid",
                "UseMultilinesType",
                "ExportTileIds",
                "IgnoreBackupSuggest",
                "PrependIndexToLevelFileNames",
            ] {
                let mut enabled = self.project.as_ref().unwrap().json["flags"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(flag)));
                if ui.checkbox(&mut enabled, flag).changed() {
                    if flag == "MultiWorlds" {
                        if enabled
                            && self
                                .edit("Multiple worlds enabled", |p| p.enable_multiworlds())
                                .is_some()
                        {
                            if let Some(mut key) = self.selected {
                                key.world = Some(0);
                                self.navigate_level(key, true);
                            } else {
                                self.navigate_world(Some(0));
                            }
                        } else if !enabled {
                            self.status="Move or remove extra worlds before converting back to a single world.".into();
                            self.status_error = true;
                        }
                        return;
                    }
                    self.edit("Project option changed", |p| {
                        let flags = p.json["flags"].as_array_mut().ok_or_else(|| {
                            ldtk_core::Error::Invalid("flags must be an array".into())
                        })?;
                        flags.retain(|v| v != flag);
                        if enabled {
                            flags.push(json!(flag));
                        }
                        Ok(())
                    });
                }
            }
        });
        ui.heading(locale::tr(ui, "Custom commands"));
        self.source_project_commands(ui);
    }

    fn backup_project_controls(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let iid = text(&project.json, "iid", "").to_owned();
        let read_only = project.is_backup();
        let browser_key = egui::Id::new(("backup-browser", iid.clone()));
        let rename_key = egui::Id::new(("project-rename", iid));
        if read_only {
            ui.heading(locale::tr(ui, "Backup preview"));
            ui.label(locale::tr(ui, "This backup is read-only. Restore it to replace the original project, or save a new copy."));
            let original = self.backup_original(ui.ctx());
            ui.horizontal(|ui| {
                if source_controls::button(ui, "Locate original project…").clicked() {
                    self.locate_backup_original(ui.ctx());
                }
                if source_controls::button(ui, "Save copy as…").clicked() {
                    self.save(true);
                }
            });
            if let Some(original) = original {
                ui.label(locale::format(
                    ui,
                    "Original: {path}",
                    &[("path", &original.display().to_string())],
                ));
                if source_controls::button(ui, "Restore this backup…").clicked() {
                    self.confirm_preview_restore(ui.ctx());
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
        self.backup_project_dialogs(ui);
    }

    fn backup_project_dialogs(&mut self, ui: &mut egui::Ui) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let iid = text(&project.json, "iid", "").to_owned();
        let browser_key = egui::Id::new(("backup-browser", iid.clone()));
        let rename_key = egui::Id::new(("project-rename", iid));
        if let Some(mut name) = ui
            .ctx()
            .data_mut(|data| data.get_temp::<String>(rename_key))
        {
            let mut close = false;
            project_dialogs::show_dialog(ui.ctx(), rename_key, "Rename project", 440., |ui| {
                ui.label(locale::tr(
                    ui,
                    "New file name; the project extension is kept.",
                ));
                ui.add_sized(
                    [ui.available_width(), 30.],
                    egui::TextEdit::singleline(&mut name).vertical_align(egui::Align::Center),
                );
                ui.horizontal_wrapped(|ui| {
                    if project_dialogs::action(ui, "Rename", true).clicked() {
                        self.request(ui.ctx(), Action::RenameProject(name.clone()));
                        close = true;
                    }
                    if project_dialogs::action(ui, "Cancel", false).clicked() {
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
            project_dialogs::show_dialog(ui.ctx(), browser_key, "Project backups", 560., |ui| {
                ui.label(browser.original.display().to_string());
                if let Some(backup) = browser.restore.clone() {
                    ui.colored_label(theme::ACCENT,locale::tr(ui, "Restoring replaces the current project with this backup. The current version will be backed up first."));
                    ui.label(backup.display().to_string());
                    ui.horizontal_wrapped(|ui| {
                        if project_dialogs::action(ui, "Restore backup", true).clicked() {
                            self.request(
                                ui.ctx(),
                                Action::RestoreBackup {
                                    backup,
                                    original: browser.original.clone(),
                                },
                            );
                            close = true;
                        }
                        if project_dialogs::action(ui, "Back", false).clicked() {
                            browser.restore = None;
                        }
                    });
                } else if browser.backups.is_empty() {
                    ui.label(locale::tr(ui, "No backups found."));
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(360.)
                        .show(ui, |ui| {
                            for backup in &browser.backups {
                                ui.horizontal_wrapped(|ui| {
                                    let suffix = if backup.crash {
                                        locale::tr(ui, " (crash recovery)")
                                    } else {
                                        ""
                                    };
                                    ui.label(format!("{}{suffix}", backup.timestamp));
                                    if project_dialogs::action(ui, "Open", false).clicked() {
                                        self.request(
                                            ui.ctx(),
                                            Action::OpenBackup {
                                                backup: backup.path.clone(),
                                                original: browser.original.clone(),
                                            },
                                        );
                                        close = true;
                                    }
                                    if project_dialogs::action(ui, "Restore…", false).clicked() {
                                        browser.restore = Some(backup.path.clone());
                                    }
                                });
                            }
                        });
                }
                close |= project_dialogs::action(ui, "Close", false).clicked();
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
            source_controls::form_combo("world-properties")
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
                egui::Button::new(locale::tr(ui, "Create level")),
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
            ui.label(locale::tr(ui, "Create a level in the World panel."));
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
            let resizing = patch.get("pxWid").is_some() || patch.get("pxHei").is_some();
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                snap_level_form_patch(p, key, &mut patch)?;
                relative_path_patch(p, &mut patch, "bgRelPath")?;
                normalize_background_patch(&level, &mut patch);
                let removed = if resizing {
                    p.resize_level_counting_removed_entities(
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
                    )?
                } else {
                    0
                };
                let l = p
                    .level_mut(key)
                    .ok_or_else(|| ldtk_core::Error::Invalid("level missing".into()))?;
                apply_patch(l, &patch);
                if patch.get("__bgColor").is_some() {
                    l["bgColor"] = patch["__bgColor"].clone();
                }
                p.refresh_derived_data()?;
                p.rebuild_auto_layers(key)?;
                Ok(removed)
            };
            if resizing {
                if let Some(removed) = self.edit_level_recoverable("Level resized", key, operation)
                {
                    self.report_resize_removals(removed);
                }
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
            source_controls::form_combo("level-world")
                .selected_text(
                    destinations
                        .iter()
                        .find(|(i, _)| *i == destination)
                        .map(|(_, name)| name.as_str())
                        .unwrap_or_else(|| locale::tr(ui, "Root world")),
                )
                .show_ui(ui, |ui| {
                    if self.project.as_ref().unwrap().json["flags"]
                        .as_array()
                        .is_none_or(|flags| !flags.contains(&json!("MultiWorlds")))
                    {
                        ui.selectable_value(&mut destination, None, locale::tr(ui, "Root world"));
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
                    .level_keys_in_world(destination)
                    .len();
                if let Some(moved) = self.edit("Level moved to world", |p| {
                    p.move_level_to_world(key, destination, index)
                }) {
                    self.navigate_level(moved, true);
                }
            }
        }
        ui.heading(locale::tr(ui, "Custom fields"));
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
                source_controls::form_combo("new-layer-kind")
                    .selected_text(locale::tr(ui, &self.new_kind))
                    .show_ui(ui, |ui| {
                        for kind in ["IntGrid", "Tiles", "AutoLayer", "Entities"] {
                            ui.selectable_value(
                                &mut self.new_kind,
                                kind.to_owned(),
                                locale::tr(ui, kind),
                            );
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
        if self.source_panel_active(ui) {
            ui.spacing_mut().item_spacing.y = 2.;
        } else {
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
                    self.request_definition_removal(ui.ctx(), "layers", uid);
                }
            });
        }
        let patch = self.source_layer_settings(ui, &def);
        if changed(&patch) {
            let rebuild = patch
                .as_object()
                .unwrap()
                .keys()
                .any(|key| !matches!(key.as_str(), "uiColor" | "guideColor" | "guideOpacity"));
            let operation = |p: &mut Project| {
                let mut patch = patch.clone();
                if let Some(grid) = patch["gridSize"].as_i64() {
                    p.change_layer_grid_size(uid, grid as i32)?;
                    patch.as_object_mut().unwrap().remove("gridSize");
                }
                if changed(&patch) {
                    p.update_layer_properties(uid, &patch)?;
                }
                if rebuild {
                    for key in p.level_keys() {
                        p.rebuild_auto_layers(key)?;
                    }
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
        if kind == "IntGrid" || kind == "AutoLayer" {
            self.source_generated_layer_settings(ui, &def);
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
            collapsing(ui, "Current level instance", |ui| {
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
                self.request_definition_removal(ui.ctx(), "entities", uid);
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
        const LABEL_WIDTH: f32 = 120.;
        let fields = owner["fieldInstances"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let has_fields = !fields.is_empty();
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
                    let kind = text(&field, "__type", "");
                    let array = kind.starts_with("Array<");
                    let point_required = kind == "Point"
                        && field["__value"].is_null()
                        && definition["canBeNull"] != true;
                    let point_array_required = kind == "Array<Point>"
                        && definition["canBeNull"] != true
                        && !field_tools::point_array_is_compact(&definition)
                        && field["__value"]
                            .as_array()
                            .is_some_and(|values| values.iter().any(Value::is_null));
                    let count = field["__value"].as_array().map_or(0, Vec::len);
                    let min = definition["arrayMinLength"].as_u64();
                    let max = definition["arrayMaxLength"].as_u64();
                    let array_warning = (array
                        && (min.is_some_and(|n| (count as u64) < n)
                            || max.is_some_and(|n| (count as u64) > n)))
                    .then(|| {
                        let lower = min.map_or_else(|| "null".to_owned(), |n| n.to_string());
                        let bounds = match max {
                            Some(max) if min == Some(max) => lower,
                            Some(max) => format!("{lower}-{max}"),
                            None => format!("{lower}+"),
                        };
                        locale::format(
                            ui,
                            "Array should have {bounds} value(s)",
                            &[("bounds", &bounds)],
                        )
                    });
                    let value_error = point_required || array_warning.is_some();
                    let row_error = value_error || point_array_required;
                    let using_default = if array {
                        count == 0
                    } else {
                        ui_fields::field_uses_default(&field, 0)
                    };
                    let background = ui.painter().add(egui::Shape::Noop);
                    let width = ui.available_width();
                    let mut value_rect = Rect::NOTHING;
                    let row = ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let (label, empty_array) = ui
                            .allocate_ui_with_layout(
                                egui::vec2(LABEL_WIDTH, 26.),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(LABEL_WIDTH);
                                    ui.spacing_mut().item_spacing.y = 0.;
                                    let name = text(&field, "__identifier", "Field");
                                    let label = if array {
                                        if let Some(max) = definition["arrayMaxLength"]
                                            .as_u64()
                                            .filter(|max| *max > 0)
                                        {
                                            format!("{name} {count}/{max}")
                                        } else {
                                            format!("{name} {count}")
                                        }
                                    } else {
                                        name.to_owned()
                                    };
                                    let label = source_controls::field_instance_label(
                                        ui,
                                        &label,
                                        LABEL_WIDTH,
                                        definition["doc"].as_str(),
                                        using_default,
                                        row_error,
                                    );
                                    let empty_array = array
                                        && count > 0
                                        && ui
                                            .allocate_ui_with_layout(
                                                egui::vec2(LABEL_WIDTH - 8., 12.),
                                                egui::Layout::top_down(egui::Align::RIGHT),
                                                |ui| {
                                                    let label = egui::Label::new(
                                                        RichText::new(locale::tr(
                                                            ui,
                                                            "Empty array",
                                                        ))
                                                        .size(9.333),
                                                    );
                                                    ui.add(source_controls::underlined_link(label))
                                                        .clicked()
                                                },
                                            )
                                            .inner;
                                    (label, empty_array)
                                },
                            )
                            .inner;
                        egui::Popup::context_menu(&label)
                            .frame(source_controls::context_menu_frame(ui.style()))
                            .show(|ui| {
                                if source_controls::context_menu_item(ui, "Edit field definition")
                                    .clicked()
                                {
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
                        let value_width = (width - LABEL_WIDTH - 2.).max(30.);
                        let value = ui.allocate_ui_with_layout(
                            egui::vec2(value_width, 26.),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(value_width);
                                if (array || kind == "EntityRef")
                                    && using_default
                                    && !row_error
                                    && !ui.rect_contains_pointer(ui.response().rect)
                                {
                                    ui.multiply_opacity(0.35);
                                }
                                egui::Frame::new()
                                        .inner_margin(if using_default { 0 } else { 2 })
                                        .show(ui, |ui| {
                                            if let Some(warning) = &array_warning {
                                                ui.scope(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 0.;
                                                    ui.add_space(6.);
                                                    let width = ui.available_width();
                                                    let rect = egui::Frame::new()
                                                        .fill(Color32::from_rgb(75, 9, 4))
                                                        .inner_margin(8)
                                                        .show(ui, |ui| {
                                                            ui.set_width((width - 16.).max(0.));
                                                            ui.label(
                                                                RichText::new(warning)
                                                                    .color(Color32::WHITE),
                                                            );
                                                        })
                                                        .response
                                                        .rect;
                                                    ui.painter().extend(egui::Shape::dashed_line(
                                                        &[
                                                            rect.left_top(),
                                                            rect.right_top(),
                                                            rect.right_bottom(),
                                                            rect.left_bottom(),
                                                            rect.left_top(),
                                                        ],
                                                        Stroke::new(1_f32, Color32::RED),
                                                        2.,
                                                        2.,
                                                    ));
                                                    ui.add_space(11.);
                                                });
                                            }
                                            let value = if empty_array {
                                                Some(FieldEdit::Value(json!([])))
                                            } else if matches!(
                                                kind,
                                                "Point"
                                                    | "EntityRef"
                                                    | "Array<Point>"
                                                    | "Array<EntityRef>"
                                            ) {
                                                let _ = self.field_pick_controls(
                                                    ui,
                                                    key,
                                                    entity,
                                                    &field,
                                                    &definition,
                                                );
                                                None
                                            } else {
                                                field_value_editor(
                                                    ui,
                                                    self.project.as_ref().unwrap(),
                                                    &field,
                                                    &definition,
                                                )
                                            };
                                            if let Some(value) = value {
                                                let cancel_picker = empty_array
                                                    || matches!(
                                                &value,
                                                FieldEdit::Array(
                                                    ldtk_core::FieldArrayEdit::Insert { .. }
                                                        | ldtk_core::FieldArrayEdit::Remove { .. }
                                                )
                                            );
                                                let applied = if entity.is_some() {
                                                    self.edit_level("Field changed", key, |p| {
                                                        value.apply(p, key, entity, uid)
                                                    })
                                                } else {
                                                    let applied = self.edit("Field changed", |p| {
                                                        value.apply(p, key, entity, uid)
                                                    });
                                                    if applied.is_some() {
                                                        self.history.clear_level_timelines();
                                                    }
                                                    applied
                                                };
                                                if applied.is_some() {
                                                    if cancel_picker {
                                                        self.cancel_reference_pick();
                                                    }
                                                    self.cancel_point_pick();
                                                }
                                            }
                                            if !point_required
                                                && !matches!(kind, "FilePath" | "Array<FilePath>")
                                                && let Ok(errors) = self
                                                    .project
                                                    .as_ref()
                                                    .unwrap()
                                                    .field_value_errors(key, entity, uid)
                                            {
                                                for error in errors {
                                                    let inline_error = matches!(
                                                        error.as_str(),
                                                        "Array too short" | "Array too long"
                                                    ) || (kind
                                                        == "Array<Point>"
                                                        && error == "Value required");
                                                    if !inline_error {
                                                        ui.colored_label(Color32::LIGHT_RED, error);
                                                    }
                                                }
                                            }
                                        });
                            },
                        );
                        value_rect = value.response.rect;
                    });
                    let rect = row.response.rect;
                    let mut shapes = Vec::new();
                    if !using_default || row_error {
                        let error_fill = Color32::from_rgb(121, 17, 5);
                        shapes.push(egui::Shape::rect_filled(
                            Rect::from_min_size(rect.min, egui::vec2(LABEL_WIDTH, rect.height())),
                            egui::CornerRadius {
                                nw: 4,
                                sw: 4,
                                ne: 0,
                                se: 0,
                            },
                            if row_error {
                                error_fill
                            } else {
                                Color32::from_rgb(115, 127, 157)
                            },
                        ));
                        shapes.push(egui::Shape::rect_filled(
                            value_rect,
                            egui::CornerRadius {
                                nw: 0,
                                sw: 0,
                                ne: 4,
                                se: 4,
                            },
                            if value_error {
                                error_fill
                            } else {
                                theme::MEDIUM
                            },
                        ));
                        if row_error {
                            shapes.push(egui::Shape::rect_filled(
                                Rect::from_min_size(rect.min, egui::vec2(4., rect.height())),
                                0.,
                                Color32::from_rgb(227, 77, 0),
                            ));
                        }
                    }
                    ui.painter().set(background, egui::Shape::Vec(shapes));
                });
            }
            if has_fields {
                // Source field rows include their gap below the final row as well.
                ui.add_space(0.);
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
                if source_controls::add_enabled_button(
                    ui,
                    index > 0,
                    egui::Button::new(locale::tr(ui, "Field up")),
                )
                .clicked()
                {
                    self.edit("Field reordered", |p| {
                        p.reorder_field_definition(uid, index - 1)
                    });
                }
                if source_controls::add_enabled_button(
                    ui,
                    index + 1 < fields.len(),
                    egui::Button::new(locale::tr(ui, "Field down")),
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
            source_controls::form_combo("field-type")
                .selected_text(if kind.contains('.') {
                    &kind
                } else {
                    locale::tr(ui, &kind)
                })
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
                        ui.selectable_value(&mut kind, item.into(), locale::tr(ui, item));
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
            ui.checkbox(&mut array, locale::tr(ui, "Array"));
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
        ui.label(locale::tr(ui, "Default value"));
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
                let import = source_controls::button(ui, "Import / sync external…");
                self.import_enum_menu(ui, &import);
            });
        }
        if let Some(mut def) = self.definition_picker(ui, "enums") {
            let uid = def["uid"].as_i64().unwrap();
            let name = enum_property_row(ui, "Enum identifier", "identifier", |ui| {
                ui_fields::buffered_text_sized(
                    ui,
                    "identifier",
                    text(&def, "identifier", ""),
                    egui::vec2(ui.available_width(), 30.),
                )
            });
            if let Some(name) = name {
                self.edit("Enum renamed", |p| {
                    p.update_enum_definition(uid, &json!({"identifier":name}))
                });
            }
            let atlas = self.enum_icon_tileset(ui, &mut def, false, true);
            let tag_change = enum_property_row(ui, "Tags", "tags", |ui| {
                tag_editor::show(
                    ui,
                    self.project.as_ref().unwrap(),
                    tag_editor::Field::Definition("enums"),
                    uid,
                    &def["tags"],
                )
            });
            if let Some(change) = tag_change {
                self.edit("Enum tags changed", |p| match change {
                    tag_editor::Change::Set(value) => {
                        p.update_enum_definition(uid, &json!({"tags":value}))
                    }
                    tag_editor::Change::Rename {
                        old,
                        new,
                        everywhere,
                    } => p.rename_definition_tag("enums", uid, &old, &new, everywhere),
                });
            }
            enum_values_heading(ui);
            let add_value = source_controls::add_sized_button(
                ui,
                [ui.available_width(), 35.],
                egui::Button::new(locale::tr(ui, "Add a new value").to_uppercase())
                    .fill(theme::LIGHT),
            )
            .clicked();
            let focus_id = ui.make_persistent_id((
                "new-enum-value",
                self.project.as_ref().unwrap().json["iid"].as_str(),
                uid,
            ));
            let focus_value = ui.data_mut(|data| data.remove_temp::<String>(focus_id));
            let mut values = def["values"].as_array().cloned().unwrap_or_default();
            let mut edited = false;
            let mut remove = None;
            let mut reorder = None;
            enum_value_list(ui, uid, |ui| {
                for (index, value) in values.iter_mut().enumerate() {
                    let old = text(value, "id", "").to_owned();
                    let row = ui.push_id((uid, &old), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = ENUM_VALUE_SPACING;
                            let handle = ui
                                .add_sized([25., 34.], egui::Label::new("≡").sense(Sense::drag()))
                                .on_hover_cursor(egui::CursorIcon::Grab);
                            if let Some(tile) = self.value_tile_thumbnail(
                                ui,
                                atlas,
                                &value["tileRect"],
                                true,
                                false,
                            ) {
                                value["tileRect"] = tile;
                                edited = true;
                            }
                            edited |= enum_value_color(ui, value, true);
                            let name_width =
                                (ui.available_width() - ENUM_VALUE_SIZE - ENUM_VALUE_SPACING)
                                    .max(20.);
                            let focus = focus_value.as_deref() == Some(old.as_str());
                            if let Some(input) =
                                enum_value_name(ui, value, &old, name_width, true, focus)
                                && let Some(id) = self.edit("Enum value renamed", |p| {
                                    p.rename_enum_value_from_editor(uid, &old, &input)
                                })
                            {
                                value["id"] = json!(id);
                            }
                            let delete = ui
                                .scope(|ui| {
                                    ui.spacing_mut().button_padding = Vec2::splat(4.);
                                    let icon = egui::Image::new(egui::include_image!(
                                        "../../assets/icons/delete.svg"
                                    ))
                                    .fit_to_exact_size(Vec2::splat(20.));
                                    source_controls::add_sized_button(
                                        ui,
                                        Vec2::splat(ENUM_VALUE_SIZE),
                                        egui::Button::image(icon),
                                    )
                                })
                                .inner;
                            if delete.clicked() {
                                remove = Some((text(value, "id", "").to_owned(), delete.rect));
                            }
                            if handle.drag_started() {
                                handle
                                    .dnd_set_drag_payload((uid, text(value, "id", "").to_owned()));
                            }
                        })
                    });
                    let item = row.inner.response;
                    if focus_value.as_deref() == Some(old.as_str()) {
                        item.scroll_to_me(Some(egui::Align::BOTTOM));
                    }
                    if let Some(payload) = item.dnd_hover_payload::<(i64, String)>()
                        && payload.0 == uid
                        && value["id"] != payload.1
                    {
                        ui.painter().rect_stroke(
                            item.rect,
                            2.,
                            egui::Stroke::new(1_f32, theme::ACCENT),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if let Some(payload) = item.dnd_release_payload::<(i64, String)>()
                        && payload.0 == uid
                        && value["id"] != payload.1
                    {
                        reorder = Some((payload.1.clone(), index));
                    }
                }
            });
            if let Some((value, anchor)) = remove {
                let project = self.project.as_ref().unwrap();
                if project.is_enum_value_used(uid, &value) {
                    ui.ctx().data_mut(|data| {
                        data.insert_temp(
                            enum_value_removal_id(),
                            EnumValueRemoval {
                                project_iid: project.json["iid"].clone(),
                                uid,
                                value,
                                anchor,
                            },
                        );
                    });
                } else if self
                    .edit("Enum value removed", |p| p.remove_enum_value(uid, &value))
                    .is_some()
                {
                    values.retain(|v| v["id"] != value);
                }
            } else if let Some((id, to)) = reorder
                && let Some(from) = values.iter().position(|v| v["id"] == id)
            {
                let value = values.remove(from);
                values.insert(to, value);
                edited = true;
            }
            let new_name = if add_value {
                let project = self.project.as_ref().unwrap();
                let identifier = text(&def, "identifier", "");
                let name = (0..=values.len())
                    .map(|index| project.cleanup_identifier(&format!("{identifier}{index}")))
                    .find(|name| values.iter().all(|value| value["id"] != *name))
                    .unwrap();
                let color = self.suggested_color(&values);
                values.push(json!({"id":name,"color":color,"tileRect":null}));
                edited = true;
                Some(name)
            } else {
                None
            };
            // The new input can be at the bottom edge, where a status toast would cover it.
            let notice = if add_value { "" } else { "Enum values changed" };
            if edited
                && self
                    .edit(notice, |p| {
                        p.update_enum_definition(uid, &json!({"values":values}))
                    })
                    .is_some()
                && let Some(name) = new_name
            {
                ui.data_mut(|data| data.insert_temp(focus_id, name));
                ui.ctx().request_repaint();
            }
            if !self.source_panel_active(ui) && source_controls::button(ui, "Delete enum").clicked()
            {
                self.request_definition_removal(ui.ctx(), "enums", uid);
            }
        }
        if self.source_panel_active(ui) {
            return;
        }
        collapsing(ui, "External enums", |ui| {
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

    pub(super) fn enum_value_removal_dialog(&mut self, ctx: &egui::Context) {
        let id = enum_value_removal_id();
        let Some(pending) = ctx.data(|data| data.get_temp::<EnumValueRemoval>(id)) else {
            return;
        };
        if self.panel != Some(Panel::Enums)
            || self
                .project
                .as_ref()
                .is_none_or(|p| p.json["iid"] != pending.project_iid)
        {
            ctx.data_mut(|data| data.remove::<EnumValueRemoval>(id));
            return;
        }
        let screen = ctx.content_rect();
        let width = (screen.width() - 42.).clamp(1., 358.);
        let anchor = pending.anchor;
        let x = if anchor.left() - screen.left() >= screen.width() * 0.6 {
            anchor.right() - width
        } else {
            anchor.left()
        };
        let above = anchor.top() - screen.top() >= screen.height() * 0.7;
        let area = egui::Area::new(id)
            .kind(egui::UiKind::Modal)
            .sense(Sense::hover())
            .order(egui::Order::Foreground)
            .pivot(if above {
                Align2::LEFT_BOTTOM
            } else {
                Align2::LEFT_TOP
            })
            .fixed_pos(egui::pos2(
                x.clamp(
                    screen.left(),
                    (screen.right() - width - 42.).max(screen.left()),
                ),
                if above { anchor.top() } else { anchor.bottom() },
            ));
        let warning = "WARNING! This enum value is USED in one or more entity instances. These values will also be removed!";
        let modal = project_dialogs::form_warning_dialog(id)
            .area(area)
            .backdrop_color(Color32::from_black_alpha(204))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(
                    RichText::new(locale::tr(ui, warning))
                        .size(14.667)
                        .color(Color32::WHITE),
                );
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    let size = egui::vec2((width - 2.) * 0.5, 35.);
                    let confirm = project_dialogs::button(ui, "CONFIRM", size, false, 13.333);
                    let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333);
                    (confirm.clicked(), cancel.clicked())
                })
                .inner
            });
        let (confirm, cancel) = modal.inner;
        let closed = cancel || modal.should_close();
        let enter = modal.is_top_modal
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        if closed || confirm || enter {
            ctx.data_mut(|data| data.remove::<EnumValueRemoval>(id));
        }
        if !closed && (confirm || enter) {
            self.edit_recoverable("Enum value removed", |p| {
                p.remove_enum_value(pending.uid, &pending.value)
            });
        }
    }

    fn enum_icon_tileset(
        &mut self,
        ui: &mut egui::Ui,
        def: &mut Value,
        external: bool,
        editable: bool,
    ) -> Option<i64> {
        let previous = def["iconTilesetUid"].as_i64();
        let project = self.project.as_ref().unwrap();
        let tilesets = project.json["defs"]["tilesets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let choice = enum_property_row(ui, "Icon tileset", "icons", |ui| {
            let response = ui.add_enabled_ui(editable, |ui| {
                source_controls::tileset_picker(
                    ui,
                    ui.available_width(),
                    tilesets,
                    previous,
                    "-- Select a tileset --",
                    false,
                )
            });
            if !editable {
                source_controls::disabled_hatch(ui, response.response.rect);
            }
            response.inner
        });
        let Some(choice) = choice else {
            return previous;
        };
        let uid = def["uid"].as_i64().unwrap();
        let mut values = def["values"].as_array().cloned().unwrap_or_default();
        let has_icons =
            previous.is_some() && values.iter().any(|value| !value["tileRect"].is_null());
        for value in &mut values {
            value["tileRect"] = Value::Null;
        }
        let edit = |p: &mut Project| {
            let atlas = match choice {
                source_controls::TilesetChoice::Unassigned => None,
                source_controls::TilesetChoice::Existing(uid) => Some(uid),
                source_controls::TilesetChoice::EmbeddedIcons => Some(p.ensure_embedded_icons()?),
            };
            if external {
                p.update_external_enum_appearance(uid, atlas, &values)?;
            } else {
                p.update_enum_definition(uid, &json!({"iconTilesetUid":atlas,"values":values}))?;
            }
            Ok(atlas)
        };
        let updated = if has_icons {
            self.edit_recoverable("Enum icons changed", edit)
        } else {
            self.edit("Enum icon tileset changed", edit)
        };
        if let Some(atlas) = updated {
            def["iconTilesetUid"] = json!(atlas);
            def["values"] = json!(values);
            atlas
        } else {
            previous
        }
    }

    pub(super) fn source_external_enum_values(&mut self, ui: &mut egui::Ui, mut def: Value) {
        let editable = !std::path::Path::new(text(&def, "externalRelPath", ""))
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cdb"));
        let atlas = self.enum_icon_tileset(ui, &mut def, true, editable);
        enum_values_heading(ui);
        let uid = def["uid"].as_i64().unwrap();
        let mut values = def["values"].as_array().cloned().unwrap_or_default();
        let mut edited = false;
        enum_value_list(ui, uid, |ui| {
            for value in &mut values {
                let id = text(value, "id", "").to_owned();
                ui.push_id((uid, &id), |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = ENUM_VALUE_SPACING;
                        if let Some(tile) = self.value_tile_thumbnail(
                            ui,
                            atlas,
                            &value["tileRect"],
                            editable,
                            false,
                        ) {
                            value["tileRect"] = tile;
                            edited = true;
                        }
                        edited |= enum_value_color(ui, value, editable);
                        enum_value_name(ui, value, &id, ui.available_width(), false, false);
                    });
                });
            }
        });
        if edited {
            self.edit("External enum appearance changed", |p| {
                p.update_external_enum_appearance(uid, atlas, &values)
            });
        }
    }

    pub(super) fn value_tile_thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        atlas: Option<i64>,
        tile: &Value,
        editable: bool,
        clear_on_secondary: bool,
    ) -> Option<Value> {
        let project = self.project.as_ref()?;
        let tileset = atlas.and_then(|uid| project.tileset(uid));
        let enabled = editable && tileset.is_some();
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, preview) = ui.allocate_exact_size(Vec2::splat(ENUM_VALUE_SIZE), sense);
        let empty = tileset.is_none() || tile.is_null();
        let mut border = rect;
        let ctx = ui.ctx().clone();
        if let Some(tileset) = tileset.filter(|_| !empty) {
            let texture = if let Some(path) = tileset["relPath"].as_str() {
                self.assets
                    .texture(&ctx, &canvas::asset_path(project, path))
            } else if !tileset["embedAtlas"].is_null() {
                self.assets.embedded(&ctx)
            } else {
                None
            };
            if let Some(texture) = texture {
                let size = texture.size_vec2();
                let source = Rect::from_min_size(
                    egui::pos2(number(tile, "x", 0.), number(tile, "y", 0.)),
                    egui::vec2(number(tile, "w", 16.), number(tile, "h", 16.)).max(Vec2::ONE),
                );
                let preview_size = if editable { ENUM_VALUE_SIZE } else { 35. };
                let image_size = source.size() * (preview_size / source.size().max_elem());
                let image_rect = if editable {
                    Rect::from_center_size(rect.center(), image_size)
                } else {
                    let image_rect = Rect::from_min_size(rect.min + Vec2::splat(1.), image_size);
                    border = image_rect.expand(1.);
                    image_rect
                };
                ui.painter().image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(
                        (source.min.to_vec2() / size).to_pos2(),
                        (source.max.to_vec2() / size).to_pos2(),
                    ),
                    Color32::WHITE,
                );
            }
        }
        if empty {
            ui.painter().rect_filled(rect, 4., theme::DARK);
            source_controls::disabled_hatch(ui, rect);
        }
        ui.painter().rect_stroke(
            border,
            4.,
            Stroke::new(
                1_f32,
                if preview.hovered() && enabled {
                    theme::ACCENT
                } else {
                    theme::LIGHT
                },
            ),
            egui::StrokeKind::Inside,
        );
        let tileset = tileset.filter(|_| enabled)?;
        let uid = tileset["uid"].as_i64()?;
        let id = ui.make_persistent_id(("enum-icon-picker", text(&project.json, "iid", ""), uid));
        if clear_on_secondary && preview.secondary_clicked() {
            return (!tile.is_null()).then_some(Value::Null);
        }
        let opened = preview
            .action_tooltip(locale::tr(
                ui,
                if clear_on_secondary {
                    "Left-click to pick a tile or right-click to remove it."
                } else {
                    "Click to pick an icon."
                },
            ))
            .clicked();
        let open = opened || ctx.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return None;
        }
        let (picked, keep_open) = self.show_icon_atlas_picker(&ctx, id, uid, tile, opened);
        ctx.data_mut(|data| data.insert_temp(id, keep_open));
        picked
    }

    fn show_icon_atlas_picker(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        uid: i64,
        tile: &Value,
        opening: bool,
    ) -> (Option<Value>, bool) {
        let Some(tileset) = self.project.as_ref().and_then(|p| p.tileset(uid)) else {
            return (None, false);
        };
        let current = if tile.is_null() {
            None
        } else {
            let padding = number(tileset, "padding", 0.);
            let spacing = number(tileset, "spacing", 0.);
            let stride = (number(tileset, "tileGridSize", 16.) + spacing).max(1.);
            let cells = |origin, size| {
                let start = number(tile, origin, 0.) - padding;
                let end = start + number(tile, size, 16.).max(1.) - 1.;
                let first = (start / stride) as i32;
                let last = (end / stride) as i32;
                [first, last - first + 1]
            };
            let [x, w] = cells("x", "w");
            let [y, h] = cells("y", "h");
            Some([x, y, w, h])
        };
        self.with_local_atlas_state(ctx, id, |app| {
            if opening {
                app.atlas_selection = current.is_none().then_some((uid, vec![]));
                if let Some(rect) = current {
                    app.tile_rect = rect;
                }
            }
            let before = app.atlas_selection.clone();
            let viewport = ctx.content_rect();
            let modal = egui::Modal::new(id)
                .frame(
                    project_dialogs::dialog_frame(0)
                        .corner_radius(0.)
                        .fill(theme::DARK)
                        .stroke(Stroke::new(1_f32, theme::LIGHT)),
                )
                .area(
                    egui::Area::new(id)
                        .kind(egui::UiKind::Modal)
                        .sense(Sense::hover())
                        .order(egui::Order::Foreground)
                        .fixed_pos(viewport.left_top() + Vec2::splat(50.)),
                )
                .show(ctx, |ui| {
                    ui.set_width(viewport.width() * 0.9);
                    ui.set_height(viewport.height() * 0.9);
                    app.atlas_icon_picker(ctx, ui, uid, opening);
                });
            let picked = if before != app.atlas_selection {
                app.selected_tile_rect_for(uid)
            } else {
                None
            };
            let keep_open = picked.is_none() && !modal.should_close();
            (picked, keep_open)
        })
    }

    fn tileset_properties(&mut self, ui: &mut egui::Ui) {
        let in_definition_panel = self.source_panel_active(ui);
        if !in_definition_panel {
            ui.horizontal(|ui| {
                ui.label(locale::tr(ui, "Grid"));
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
            if in_definition_panel && self.project.is_some() {
                source_controls::help_card(ui, ui.available_width(), 62., |ui| {
                    source_controls::help_paragraph(
                        ui,
                        "Tilesets are large images in which you pick sub-parts to draw them in your levels. They can also be used as icons for some specific elements (like Enums).",
                    );
                });
            }
            return;
        };
        let uid = def["uid"].as_i64().unwrap();
        let embedded = !def["embedAtlas"].is_null();
        let editable_rows = [
            ("Source image", "relPath"),
            ("Identifier", "identifier"),
            ("Tiles layout", "tileGridSize"),
            ("Enum for tile marking", "tagsSourceEnumUid"),
        ];
        let label_width = std::iter::once(("Tags", "tags"))
            .chain(editable_rows.into_iter().filter(|_| !embedded))
            .map(|(label, key)| source_controls::property_label_intrinsic_width(ui, label, key))
            .fold(
                if embedded {
                    122.
                } else {
                    TILESET_MIN_LABEL_WIDTH
                },
                f32::max,
            );
        let mut patch = json!({});
        if !embedded {
            source_controls::form_row(ui, "Source image", "relPath", label_width, |ui| {
                if let Some(path) =
                    self.source_image_picker(ui, ("tileset", uid), def["relPath"].as_str())
                {
                    patch["relPath"] = json!(path);
                }
            });
            if let Some(identifier) =
                source_controls::form_row(ui, "Identifier", "identifier", label_width, |ui| {
                    ui_fields::buffered_text_sized(
                        ui,
                        "identifier",
                        text(&def, "identifier", ""),
                        egui::vec2(ui.available_width(), 33.),
                    )
                })
            {
                patch["identifier"] = json!(identifier);
            }
            source_controls::form_row(ui, "Tiles layout", "tileGridSize", label_width, |ui| {
                let max = if def["relPath"].is_string() {
                    def["pxWid"]
                        .as_i64()
                        .unwrap_or(100)
                        .min(def["pxHei"].as_i64().unwrap_or(100))
                        .max(2)
                } else {
                    100
                };
                let caption_gap = 3.;
                let number_padding = 4.;
                let controls = [
                    ("tileGridSize", 2, "px tiles, spaced by"),
                    ("spacing", 0, "px, with an atlas padding of"),
                    ("padding", 0, "px"),
                ]
                .map(|(key, min, suffix)| {
                    let caption = ui.painter().layout_no_wrap(
                        locale::tr(ui, suffix).to_owned(),
                        FontId::proportional(14.667),
                        Color32::from_white_alpha(128),
                    );
                    (key, min, def[key].as_i64().unwrap_or(min), caption)
                });
                let caption_width = controls
                    .iter()
                    .map(|(_, _, _, caption)| caption.size().x + 2. * caption_gap)
                    .sum::<f32>();
                let number_font = ui.style().drag_value_text_style.resolve(ui.style());
                let minimum_width = controls
                    .iter()
                    .map(|(_, _, value, _)| {
                        ui.painter()
                            .layout_no_wrap(value.to_string(), number_font.clone(), theme::TEXT)
                            .size()
                            .x
                            + 2. * number_padding
                    })
                    .fold(0., f32::max);
                let width = ((ui.available_width() - caption_width) / controls.len() as f32)
                    .floor()
                    .max(minimum_width);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = caption_gap;
                    ui.spacing_mut().button_padding.x = number_padding;
                    ui.spacing_mut().interact_size.x = width;
                    for (key, min, mut value, caption) in controls {
                        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
                        let response = ui.add_sized(
                            [width, 32.],
                            egui::DragValue::new(&mut value)
                                .range(min..=max)
                                .update_while_editing(false),
                        );
                        ui_commit::track_editor(ui.ctx(), &response);
                        if response.changed() {
                            patch[key] = json!(value);
                        }
                        ui.add(egui::Label::new(caption).extend());
                    }
                });
            });
        }
        let changed_tags = source_controls::form_row(ui, "Tags", "tags", label_width, |ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            tag_editor::show(
                ui,
                self.project.as_ref().unwrap(),
                tag_editor::Field::Definition("tilesets"),
                uid,
                &def["tags"],
            )
        });
        if let Some(change) = changed_tags {
            self.edit("Tileset tags changed", |p| match change {
                tag_editor::Change::Set(value) => {
                    p.update_tileset_definition(uid, &json!({"tags":value}))
                }
                tag_editor::Change::Rename {
                    old,
                    new,
                    everywhere,
                } => p.rename_definition_tag("tilesets", uid, &old, &new, everywhere),
            });
        }
        if embedded {
            embedded_tileset_notice(ui);
        }
        let current = def["tagsSourceEnumUid"].as_i64();
        let pending_id = egui::Id::new((
            "tileset-tag-source-confirm",
            text(&self.project.as_ref().unwrap().json, "iid", ""),
            uid,
        ));
        if !embedded {
            source_controls::form_row(
                ui,
                "Enum for tile marking",
                "tagsSourceEnumUid",
                label_width,
                |ui| {
                    if let Some(next) =
                        tileset_tag_enum_picker(ui, self.project.as_ref().unwrap(), current)
                    {
                        if current.is_some() && tileset_has_marked_tiles(&def) {
                            ui.ctx().data_mut(|data| data.insert_temp(pending_id, next));
                        } else {
                            patch["tagsSourceEnumUid"] = json!(next);
                        }
                    }
                },
            );
        }
        if let Some(next) = ui
            .ctx()
            .data(|data| data.get_temp::<Option<i64>>(pending_id))
        {
            let mut confirm = false;
            let mut cancel = false;
            let modal = project_dialogs::show_dialog(
                ui.ctx(),
                pending_id,
                "Enum for tile marking",
                440.,
                |ui| {
                    ui.label(locale::tr(ui, "Be careful: you have tags in this tileset. You will LOSE them by changing the source Enum!"));
                    ui.horizontal_wrapped(|ui| {
                        confirm = project_dialogs::action(ui, "Confirm", true).clicked();
                        cancel = project_dialogs::action(ui, "Cancel", false).clicked();
                    });
                },
            );
            cancel |= modal.should_close();
            if confirm || cancel {
                ui.ctx()
                    .data_mut(|data| data.remove::<Option<i64>>(pending_id));
            }
            if confirm && !cancel {
                patch["tagsSourceEnumUid"] = json!(next);
            }
        }
        if let Some(next) = patch.get("tagsSourceEnumUid") {
            let allowed = next
                .as_i64()
                .and_then(|uid| {
                    self.project.as_ref().unwrap().json["defs"]["enums"]
                        .as_array()?
                        .iter()
                        .find(|e| e["uid"] == uid)
                })
                .and_then(|e| e["values"].as_array());
            let tags = def["enumTags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|tag| {
                    allowed
                        .is_some_and(|values| values.iter().any(|v| v["id"] == tag["enumValueId"]))
                })
                .cloned()
                .collect::<Vec<_>>();
            patch["enumTags"] = json!(tags);
        }
        if in_definition_panel {
            self.definition_atlas_picker(&ui.ctx().clone(), ui, uid);
        } else {
            self.atlas_picker(&ui.ctx().clone(), ui, uid);
        }
        if embedded {
            return;
        }
        let options = remap_options(
            ui,
            ("tileset", uid),
            false,
            ldtk_core::TilesetRemap::AtlasCoordinates,
        );
        if changed(&patch) {
            self.apply_tileset_form_patch(ui.ctx(), &def, patch, options);
        }
        if in_definition_panel {
            return;
        }
        collapsing(ui, "Saved tile selections", |ui| {
            let mut selections = def["savedSelections"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut edited = false;
            let mut remove = None;
            for (index, selection) in selections.iter_mut().enumerate() {
                ui.push_id(("saved-selection", uid, index), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(locale::format(
                            ui,
                            "Selection {number} ({count} tiles)",
                            &[
                                ("number", &(index + 1).to_string()),
                                (
                                    "count",
                                    &selection["ids"].as_array().map_or(0, Vec::len).to_string(),
                                ),
                            ],
                        ));
                        let mut mode = text(selection, "mode", "Stamp").to_owned();
                        let old = mode.clone();
                        source_controls::form_combo(ui.id().with("mode"))
                            .selected_text(locale::tr(ui, &mode))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut mode,
                                    "Stamp".into(),
                                    locale::tr(ui, "Stamp"),
                                );
                                ui.selectable_value(
                                    &mut mode,
                                    "Random".into(),
                                    locale::tr(ui, "Random"),
                                );
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
        collapsing(ui, "Selected tile data", |ui| {
            ui.label(locale::format(
                ui,
                "Tile #{id}",
                &[("id", &self.tile_id.to_string())],
            ));
            if let Some(next) = text_editor::value_editor(
                ui,
                ("tileset-custom-data", uid, self.tile_id),
                library::tile_custom_data(&def, self.tile_id).unwrap_or_default(),
                "Tile custom data",
                "LangJson",
                false,
            ) {
                self.set_tileset_custom_data(uid, self.tile_id, &next);
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
                        self.mark_tileset_tiles(uid, id, &[self.tile_id], enabled);
                    }
                }
            }
        });
        if source_controls::button(ui, "Delete tileset").clicked() {
            self.edit_recoverable("Tileset deleted", |p| p.remove_definition("tilesets", uid));
            self.definition_uid = None;
        }
    }

    fn apply_tileset_form_patch(
        &mut self,
        ctx: &egui::Context,
        def: &Value,
        mut patch: Value,
        options: RemapOptions,
    ) {
        let uid = def["uid"].as_i64().unwrap();
        let image_changed = patch.get("relPath").is_some();
        let geometry_changed = ["tileGridSize", "padding", "spacing"]
            .iter()
            .any(|key| patch.get(key).is_some());
        let remove_image = image_changed && patch["relPath"].is_null();
        let prepare = |app: &mut Self, patch: &mut Value| -> ldtk_core::Result<Option<PathBuf>> {
            let project = app.project.as_ref().unwrap();
            relative_path_patch(project, patch, "relPath")?;
            if let Some(relative) = patch["relPath"].as_str() {
                let path = project.make_absolute_file_path(relative)?;
                let image = ldtk_core::exports::load_image_rgba(&path)?;
                let current = text(def, "identifier", "");
                let old_name = def["relPath"]
                    .as_str()
                    .and_then(|path| Path::new(path).file_stem())
                    .and_then(|stem| stem.to_str())
                    .map(|stem| project.cleanup_identifier(stem));
                if patch.get("identifier").is_none()
                    && (current.starts_with("Tileset")
                        || old_name.is_some_and(|name| !name.is_empty() && current.contains(&name)))
                    && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
                {
                    let name = project.cleanup_identifier(stem);
                    if !name.is_empty() && name != "_" {
                        let others = project.json["defs"]["tilesets"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|d| d["uid"] != uid)
                            .filter_map(|d| d["identifier"].as_str());
                        patch["identifier"] = json!(unique_name(&name, others));
                    }
                }
                patch["pxWid"] = json!(image.width());
                patch["pxHei"] = json!(image.height());
                let max = i64::from(image.width().min(image.height()));
                for key in ["tileGridSize", "spacing", "padding"] {
                    if let Some(value) = patch[key].as_i64().or_else(|| def[key].as_i64()) {
                        patch[key] = json!(value.min(max));
                    }
                }
                app.assets.store_file_image(ctx, &path, image)?;
            }
            if remove_image || !(image_changed || geometry_changed) {
                return Ok(None);
            }
            let Some(relative) = patch["relPath"]
                .as_str()
                .or_else(|| def["relPath"].as_str())
            else {
                return Ok(None);
            };
            let path = project.make_absolute_file_path(relative)?;
            let get = |key: &str| {
                patch[key]
                    .as_i64()
                    .or_else(|| def[key].as_i64())
                    .unwrap_or(0)
            };
            let (width, height) = (get("pxWid"), get("pxHei"));
            if width <= 0 || height <= 0 {
                return Ok(None);
            }
            let padding = resources::trimmed_atlas_padding(def, width, height, get("padding"));
            let pixels = resources::atlas_pixel_data(
                &mut app.assets,
                &path,
                [width as usize, height as usize],
                get("tileGridSize"),
                padding,
                get("spacing"),
            );
            let needs_reload = pixels.is_none();
            patch["cachedPixelData"] = pixels.unwrap_or(Value::Null);
            Ok(needs_reload.then_some(path))
        };
        let reload = match prepare(self, &mut patch) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
                return;
            }
        };
        let recoverable = image_changed
            || geometry_changed
            || patch.get("tagsSourceEnumUid").is_some() && tileset_has_marked_tiles(def);
        let operation = |p: &mut Project| {
            if remove_image {
                p.remove_tileset_image(uid)?;
                patch.as_object_mut().unwrap().remove("relPath");
                if !changed(&patch) {
                    return Ok(0);
                }
            }
            p.update_tileset_definition_with_policy(uid, &patch, options.mode, options.allow_loss)
        };
        let discarded = if recoverable {
            self.edit_recoverable("Tileset image, grid or tags changed", operation)
        } else {
            self.edit("Tileset changed", operation)
        };
        if let Some(count) = discarded {
            if count > 0 {
                self.status = format!("Tileset changed; removed {count} references");
            }
            if let Some(path) = reload {
                self.retry_resource(ctx, &path);
            }
        }
    }

    fn export_properties(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            return;
        }
        ui.label(locale::tr(ui, "Export current project"));
        for (label, format) in [
            ("Super simple export", "simple"),
            ("Layer and level PNGs", "png"),
            ("IntGrid CSV files", "csv"),
            ("Tiled TMX", "tmx"),
            ("Tiled JSON (.tmj)", "tmj"),
            ("GameMaker Studio 2 rooms", "gms"),
        ] {
            if source_controls::add_enabled_button(
                ui,
                !self.is_busy(),
                egui::Button::new(locale::tr(ui, label)),
            )
            .clicked()
                && let Some(dir) = FileDialog::new().pick_folder()
            {
                self.export_async(format, dir);
            }
        }
        ui.small(locale::tr(ui, "GameMaker export follows the original experimental exporter: entity instances and generated auto-layers are not exported."));
        if source_controls::button(ui, "Import Ogmo project…").clicked() {
            self.request(ui.ctx(), Action::BrowseOgmo);
        }
        ui.add_space(12.);
        source_controls::form_heading(ui, "User custom commands");
        ui.add_space(13.);
        self.source_project_commands(ui);
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
            ui.label(locale::tr(ui, label));
            source_controls::form_combo(ui.id().with(label))
                .selected_text(
                    p.tileset(*value)
                        .map(|d| text(d, "identifier", "Tileset"))
                        .unwrap_or_else(|| locale::tr(ui, "Choose tileset")),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, -1, locale::tr(ui, "None / use default"));
                    if let Some(defs) = p.json["defs"]["tilesets"].as_array() {
                        for d in defs {
                            if let Some(uid) = d["uid"].as_i64() {
                                ui.selectable_value(
                                    value,
                                    uid,
                                    text(d, "identifier", locale::tr(ui, "Tileset")),
                                );
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
                let snapped = (n as f64 / grid as f64).round() as i64;
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
fn enum_property_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    key: &str,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    source_controls::form_row_wrapped(ui, label, key, 110., content)
}

fn enum_values_heading(ui: &mut egui::Ui) {
    ui.add_space(8.);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.), Sense::hover());
    ui.painter().text(
        rect.left_top(),
        Align2::LEFT_TOP,
        locale::tr(ui, "All possible enum values").to_uppercase(),
        FontId::new(24., egui::FontFamily::Name("panel-title".into())),
        theme::ACCENT,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 1.,
        Stroke::new(1_f32, theme::ACCENT),
    );
}

fn enum_value_list(ui: &mut egui::Ui, uid: i64, contents: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 4.;
        ui.spacing_mut().scroll = egui::style::ScrollStyle {
            bar_width: 10.,
            bar_inner_margin: 0.,
            bar_outer_margin: 0.,
            ..egui::style::ScrollStyle::solid()
        };
        egui::ScrollArea::vertical()
            .id_salt(("enum-values", uid))
            .max_height(ui.available_height())
            .auto_shrink([false, true])
            .show(ui, contents);
    });
}

fn enum_value_color(ui: &mut egui::Ui, value: &mut Value, editable: bool) -> bool {
    let mut color = enum_color(value);
    let changed = ui
        .add_enabled_ui(editable, |ui| {
            source_colors::color_editor_sized(ui, &mut color, None, Vec2::splat(ENUM_VALUE_SIZE))
        })
        .inner
        .changed();
    if editable && changed {
        let [r, g, b] = color;
        value["color"] = json!(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b));
        return true;
    }
    false
}

fn enum_value_name(
    ui: &mut egui::Ui,
    value: &Value,
    mut name: &str,
    width: f32,
    editable: bool,
    focus: bool,
) -> Option<String> {
    let size = egui::vec2(width, ENUM_VALUE_SIZE);
    ui.scope(|ui| {
        if !value["color"].is_null() {
            let [r, g, b] = enum_color(value);
            let color = Color32::from_rgb(r, g, b);
            let lighten = |channel| (f32::from(channel) * 0.3 + 255. * 0.7).round() as u8;
            let style = ui.visuals_mut();
            style.override_text_color = Some(Color32::from_rgb(lighten(r), lighten(g), lighten(b)));
            style.extreme_bg_color = Color32::from_rgb(r / 2, g / 2, b / 2);
            style.text_edit_bg_color = Some(style.extreme_bg_color);
            style.widgets.inactive.bg_stroke.color = color;
        }
        if editable {
            let key = "enum-value-name";
            if focus {
                let id = ui.make_persistent_id(key);
                let mut state =
                    egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(name.chars().count()),
                    )));
                state.store(ui.ctx(), id);
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            ui_fields::buffered_text_sized(ui, key, name, size)
        } else {
            let background = ui.painter().add(egui::Shape::Noop);
            let response = ui.add_sized(
                size,
                egui::TextEdit::singleline(&mut name)
                    .desired_width(f32::INFINITY)
                    .min_size(size),
            );
            if !value["color"].is_null() {
                ui.painter().set(
                    background,
                    egui::Shape::rect_filled(response.rect, 2., ui.visuals().text_edit_bg_color()),
                );
            }
            None
        }
    })
    .inner
}

pub(super) fn enum_color(value: &Value) -> [u8; 3] {
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
    collapsing(ui, "Existing tile references", |ui| {
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::TileIds,
            locale::tr(ui, "Keep tile numbers"),
        );
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::AtlasCoordinates,
            locale::tr(ui, "Keep image positions"),
        );
        ui.radio_value(
            &mut options.mode,
            ldtk_core::TilesetRemap::Clear,
            locale::tr(ui, "Clear affected tiles"),
        );
        source_controls::checkbox(
            ui,
            &mut options.allow_loss,
            "Allow removal of references that no longer fit",
        );
        if overrides {
            source_controls::checkbox(
                ui,
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
    fn replacing_tileset_image_updates_pixels_and_name_in_the_same_undo() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Ground.png");
        let new = dir.path().join("Walls.png");
        image::RgbaImage::from_pixel(32, 16, image::Rgba([255, 0, 0, 255]))
            .save(&new)
            .unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&old, 16, 16, 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let def = app.project.as_ref().unwrap().tileset(uid).unwrap().clone();
        let ctx = egui::Context::default();
        let options = RemapOptions {
            mode: ldtk_core::TilesetRemap::AtlasCoordinates,
            allow_loss: false,
            clear_overrides: false,
        };
        let input = json!({"relPath":new});
        app.apply_tileset_form_patch(&ctx, &def, input, options);
        let updated = app.project.as_ref().unwrap().tileset(uid).unwrap();
        assert_eq!(updated["relPath"], "Walls.png");
        assert_eq!(updated["identifier"], "Walls");
        assert_eq!(updated["pxWid"], 32);
        assert_eq!(updated["cachedPixelData"]["opaqueTiles"], "11");
        assert!(app.resource_checks.is_empty());
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().tileset(uid).unwrap(), &def);
    }
    #[test]
    fn tileset_definition_atlas_fills_the_panel_and_zooms_without_scrolling_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Atlas.png");
        image::RgbaImage::from_pixel(320, 320, image::Rgba([255, 255, 255, 255]))
            .save(&path)
            .unwrap();
        let mut project = Project::new();
        let uid = project.add_tileset(&path, 16, 320, 320).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.definition_uid = Some(uid);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let texture = (0..100)
            .find_map(|_| {
                app.assets.poll(&ctx);
                let texture = app.assets.texture(&ctx, &path);
                if texture.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                texture
            })
            .expect("atlas loaded");
        let mut time = 0.;
        let mut draw = |events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::Tilesets)),
            )
        };
        let atlas = |out: &egui::FullOutput| {
            out.shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => {
                        Some((mesh.calc_bounds(), shape.clip_rect))
                    }
                    _ => None,
                })
                .expect("atlas paint")
        };
        draw(vec![]);
        let (initial, view) = atlas(&draw(vec![]));
        assert!((initial.width() / 320. - 3.).abs() < 0.001);
        let pointer = view.center();
        draw(vec![egui::Event::PointerMoved(pointer)]);
        draw(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0., -1.),
            modifiers: egui::Modifiers::NONE,
        }]);
        draw(vec![egui::Event::PointerMoved(egui::pos2(160., 520.))]);
        for _ in 0..12 {
            draw(vec![]);
        }
        let (scaled, zoomed_view) = atlas(&draw(vec![]));
        let zoom = scaled.width() / 320.;
        assert!((zoom - 2.64).abs() < 0.001, "{zoom}");
        assert_eq!(view, zoomed_view);
        assert!(((pointer - initial.min) / 3. - (pointer - scaled.min) / zoom).length() < 0.1);
        assert_eq!(view.bottom(), 879.);
        draw(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0., -1.),
            modifiers: egui::Modifiers::NONE,
        }]);
        for _ in 0..12 {
            draw(vec![]);
        }
        let (scrolled, scrolled_view) = atlas(&draw(vec![]));
        assert!((scrolled.width() - scaled.width()).abs() < 0.1);
        assert!(scrolled_view.top() < view.top());
    }

    #[test]
    fn tileset_metadata_is_edited_through_the_atlas() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Marks.png");
        image::RgbaImage::from_pixel(96, 64, image::Rgba([255, 255, 255, 255]))
            .save(&path)
            .unwrap();
        let mut project = Project::new();
        let uid = project.add_tileset(&path, 16, 96, 64).unwrap();
        let kind = project
            .add_enum_definition("TerrainKind", &["Water", "Ground"])
            .unwrap();
        project
            .update_tileset_definition(uid, &json!({"tagsSourceEnumUid":kind}))
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Tilesets);
        app.definition_uid = Some(uid);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut time = 0.;
        let mut draw = |app: &mut EditorApp, events| {
            time += 0.05;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.assets.poll(ctx);
                    assert!(app.source_panel(ctx, Panel::Tilesets));
                },
            )
        };
        draw(&mut app, vec![]);
        let texture = (0..100)
            .find_map(|_| {
                app.assets.poll(&ctx);
                let texture = app.assets.texture(&ctx, &path);
                if texture.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                texture
            })
            .expect("atlas loaded");
        fn text_position(shape: &egui::Shape, expected: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == expected => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_position(s, expected)),
                _ => None,
            }
        }
        draw(&mut app, vec![]);
        let output = draw(&mut app, vec![]);
        let water = output
            .shapes
            .iter()
            .find_map(|s| text_position(&s.shape, "Water"))
            .expect("Water is selectable next to the atlas");
        let atlas = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => {
                    Some(mesh.calc_bounds())
                }
                _ => None,
            })
            .unwrap();
        let stroke =
            |app: &mut EditorApp,
             draw: &mut dyn FnMut(&mut EditorApp, Vec<egui::Event>) -> egui::FullOutput,
             from,
             to,
             button| {
                let event = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                draw(app, vec![egui::Event::PointerMoved(from)]);
                draw(app, vec![event(from, true)]);
                draw(app, vec![egui::Event::PointerMoved(to)]);
                draw(app, vec![event(to, false)]);
            };
        stroke(
            &mut app,
            &mut draw,
            water,
            water,
            egui::PointerButton::Primary,
        );
        let cell = |x: f32, y: f32| atlas.min + egui::vec2(24. + 48. * x, 24. + 48. * y);
        let marked = |app: &EditorApp| {
            app.project.as_ref().unwrap().tileset(uid).unwrap()["enumTags"][0]["tileIds"].clone()
        };
        stroke(
            &mut app,
            &mut draw,
            cell(0., 0.),
            cell(1., 1.),
            egui::PointerButton::Primary,
        );
        assert_eq!(marked(&app), json!([0, 1, 6, 7]));
        stroke(
            &mut app,
            &mut draw,
            cell(1., 0.),
            cell(1., 1.),
            egui::PointerButton::Secondary,
        );
        assert_eq!(marked(&app), json!([0, 6]));
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(marked(&app), json!([0, 1, 6, 7]));

        app.project
            .as_mut()
            .unwrap()
            .update_tileset_definition(uid, &json!({"tagsSourceEnumUid":null,"enumTags":[]}))
            .unwrap();
        draw(&mut app, vec![]);
        stroke(
            &mut app,
            &mut draw,
            cell(2., 0.),
            cell(2., 0.),
            egui::PointerButton::Primary,
        );
        assert!(
            text_editor::is_open(&ctx),
            "clicking an atlas tile opens its data editor without an enum"
        );
        let dialog = draw(&mut app, vec![]);
        assert!(
            dialog
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "TILE 2 CUSTOM DATA").is_some())
        );
        draw(
            &mut app,
            vec![egui::Event::Paste("  {\"cost\":2} \n".into())],
        );
        text_editor::request_validate(&ctx);
        draw(&mut app, vec![]);
        let data = |app: &EditorApp| {
            app.project.as_ref().unwrap().tileset(uid).unwrap()["customData"].clone()
        };
        let saved = json!([{"tileId":2,"data":"{\"cost\":2}"}]);
        assert_eq!(data(&app), saved);

        stroke(
            &mut app,
            &mut draw,
            cell(2., 0.),
            cell(2., 0.),
            egui::PointerButton::Secondary,
        );
        let dialog = draw(&mut app, vec![]);
        assert!(
            dialog
                .shapes
                .iter()
                .any(|s| text_position(&s.shape, "Clear custom data for tile 2?").is_some())
        );
        assert_eq!(data(&app), saved);
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&mut app, vec![key(Key::Escape)]);
        assert_eq!(data(&app), saved);
        draw(&mut app, vec![]);
        stroke(
            &mut app,
            &mut draw,
            cell(2., 0.),
            cell(2., 0.),
            egui::PointerButton::Secondary,
        );
        draw(&mut app, vec![]);
        draw(&mut app, vec![key(Key::Enter)]);
        assert_eq!(data(&app), json!([]));
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(data(&app), saved);
    }

    #[test]
    fn tileset_form_uses_source_row_order_and_restricts_embedded_atlases() {
        fn label<'a>(shape: &'a egui::Shape, name: &str) -> Option<&'a egui::epaint::TextShape> {
            match shape {
                egui::Shape::Text(text)
                    if text
                        .galley
                        .job
                        .text
                        .split_whitespace()
                        .eq(name.split_whitespace()) =>
                {
                    Some(text)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, name)),
                _ => None,
            }
        }
        fn draw(app: &mut EditorApp, ctx: &egui::Context, width: f32) -> egui::FullOutput {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1200.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Tilesets);
                },
            )
        }
        let mut project = Project::new();
        project.json["defs"]["tilesets"] = json!([{"uid":13,"identifier":"Atlas","relPath":null,"embedAtlas":null,"pxWid":32,"pxHei":32,"tileGridSize":16,"spacing":0,"padding":0,"tags":[],"tagsSourceEnumUid":null,"enumTags":[],"savedSelections":[],"customData":[]}]);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Tilesets);
        app.definition_uid = Some(13);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        for width in [1000., 900.] {
            draw(&mut app, &ctx, width);
            let out = draw(&mut app, &ctx, width);
            let position = |name| {
                let (clip, text) = out
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, name).map(|text| (shape.clip_rect, text)))
                    .unwrap_or_else(|| panic!("missing {name}"));
                assert_eq!(text.galley.rows.len(), 1, "{name} must stay on one line");
                let bounds = Rect::from_min_size(text.pos, text.galley.size());
                assert!(
                    clip.expand(0.1).contains_rect(bounds),
                    "{name} glyphs {bounds:?} cannot be clipped by {clip:?}"
                );
                text.pos.y
            };
            assert!(position("Source image") < position("Identifier"));
            assert!(position("Identifier") < position("Tiles layout"));
            for name in [
                "Saved tile selections",
                "Selected tile data",
                "Delete tileset",
            ] {
                assert!(
                    !out.shapes
                        .iter()
                        .any(|shape| label(&shape.shape, name).is_some())
                );
            }
            let layout_top = position("Tiles layout");
            let tags_top = position("Tags");
            assert!(layout_top < tags_top);
            assert!(tags_top < position("Enum for tile marking"));
            if width == 1000. {
                let first_caption = position("px tiles, spaced by");
                assert_eq!(position("px, with an atlas padding of"), first_caption);
                assert_eq!(position("px"), first_caption);
            }
            let pane_right = source_panel_width(Panel::Tilesets) - 20.;
            for item in &out.shapes {
                let bounds = item.shape.visual_bounding_rect();
                if bounds.top() >= layout_top - 12.
                    && bounds.bottom() < tags_top - 2.
                    && bounds.left() > 315.
                {
                    assert!(
                        bounds.right() <= pane_right + 0.1,
                        "Tiles layout control {bounds:?} crosses right pane {pane_right}"
                    );
                }
            }
            let panel = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
                .unwrap()
                .rect();
            assert_eq!(panel.right(), width);
        }
        app.project.as_mut().unwrap().json["defs"]["tilesets"][0]["embedAtlas"] =
            json!("LdtkIcons");
        let out = draw(&mut app, &ctx, 1000.);
        for name in [
            "Source image",
            "Identifier",
            "Tiles layout",
            "Enum for tile marking",
        ] {
            assert!(
                !out.shapes
                    .iter()
                    .any(|shape| label(&shape.shape, name).is_some()),
                "embedded atlas must hide {name}"
            );
        }
        assert!(
            out.shapes
                .iter()
                .any(|shape| label(&shape.shape, "Tags").is_some())
        );
        for text in ["Image by", "FinalBossBlues", "website", "Patreon"] {
            assert!(
                out.shapes
                    .iter()
                    .any(|shape| label(&shape.shape, text).is_some()),
                "missing embedded tileset credit {text}"
            );
        }
        let notice_border = out
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::LineSegment { points, stroke }
                    if stroke.color == Color32::from_rgb(115, 127, 157) =>
                {
                    Some(points[0])
                }
                _ => None,
            })
            .expect("the notice has a dashed border");
        assert!(
            (notice_border.x - 336.5).abs() < 0.5 && (notice_border.y - 153.5).abs() <= 3.,
            "notice starts at {notice_border:?}"
        );
        assert!(
            out.shapes
                .iter()
                .any(|shape| { label(&shape.shape, EMBEDDED_TILESET_DESCRIPTION).is_some() })
        );
        assert!(
            out.shapes
                .iter()
                .any(|shape| { label(&shape.shape, EMBEDDED_TILESET_LICENSE).is_some() })
        );
        for name in [
            "Saved tile selections",
            "Selected tile data",
            "Delete tileset",
        ] {
            assert!(
                !out.shapes
                    .iter()
                    .any(|shape| label(&shape.shape, name).is_some()),
                "embedded atlas must hide {name}"
            );
        }
        let original = app.project.as_ref().unwrap().json.clone();
        let target = ClipTarget::Definition {
            kind: "tilesets".into(),
            uid: Some(13),
        };
        app.clipboard_copy(&ctx, &target, true);
        app.clipboard_duplicate(&target);
        assert_eq!(app.project.as_ref().unwrap().json, original);
    }
    #[test]
    fn tileset_layout_row_commits_grid_edit_and_undoes_it() {
        fn positions(shape: &egui::Shape, found: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == "16" => {
                    found.push(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| positions(s, found)),
                _ => {}
            }
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
                        egui::vec2(1000., 1200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.properties(ui, Panel::Tilesets));
                },
            )
        }
        let mut project = Project::new();
        project.json["defs"]["tilesets"] = json!([{"uid":13,"identifier":"Atlas","relPath":null,"embedAtlas":null,"pxWid":32,"pxHei":32,"tileGridSize":16,"spacing":0,"padding":0,"tags":[],"tagsSourceEnumUid":null,"enumTags":[],"savedSelections":[],"customData":[]}]);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Tilesets);
        app.definition_uid = Some(13);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let out = frame(&mut app, &ctx, vec![]);
        let mut found = Vec::new();
        for shape in &out.shapes {
            positions(&shape.shape, &mut found);
        }
        let pos = *found.last().expect("grid size");
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
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
            vec![key(
                Key::A,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            )],
        );
        frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Text("8".into()),
                key(Key::Enter, Default::default()),
            ],
        );
        frame(&mut app, &ctx, vec![]);
        assert_eq!(
            app.project.as_ref().unwrap().tileset(13).unwrap()["tileGridSize"],
            8
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().tileset(13).unwrap()["tileGridSize"],
            16
        );
    }
    #[test]
    fn tileset_tag_source_picker_uses_names_and_disables_external_enums() {
        fn find(shape: &egui::Shape, name: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == name => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, name)),
                _ => None,
            }
        }
        let mut project = Project::new();
        project.json["defs"]["enums"] =
            json!([{"uid": 11, "identifier":"Terrain", "tags":["Ground"], "values":[]}]);
        project.json["defs"]["externalEnums"] = json!([{"uid": 12, "identifier":"External", "externalRelPath":"data.cdb", "tags":[], "values":[]}]);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let selected = std::cell::Cell::new(None);
        let draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700., 500.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(value) = tileset_tag_enum_picker(ui, &project, None) {
                            selected.set(Some(value));
                        }
                    });
                },
            )
        };
        let out = draw(vec![]);
        let center = |out: &egui::FullOutput, s| {
            out.shapes
                .iter()
                .find_map(|s0| find(&s0.shape, s))
                .unwrap_or_else(|| panic!("missing {s}"))
        };
        let click = |pos| {
            draw(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                draw(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
            draw(vec![])
        };
        let out = click(center(&out, "-- None --"));
        center(&out, "Ground");
        center(&out, "data.cdb");
        let external = center(&out, "External (unsupported external enum)");
        let out = click(external);
        assert_eq!(selected.get(), None);
        let out = click(center(&out, "-- None --"));
        let local = center(&out, "Terrain");
        click(local);
        assert_eq!(selected.get(), Some(Some(11)));
    }
    #[test]
    fn tileset_tag_source_change_confirms_and_preserves_matching_tags() {
        fn find(shape: &egui::Shape, name: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == name => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, name)),
                _ => None,
            }
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
                        egui::vec2(1000., 1200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| app.properties(ui, Panel::Tilesets));
                },
            )
        }
        fn click(app: &mut EditorApp, ctx: &egui::Context, name: &str) {
            let output = frame(app, ctx, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|s| find(&s.shape, name))
                .unwrap_or_else(|| panic!("missing {name}"));
            frame(app, ctx, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    app,
                    ctx,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        }
        let mut project = Project::new();
        project.json["defs"]["enums"] = json!([
            {"uid":11,"identifier":"Terrain","values":[{"id":"Grass"},{"id":"Water"}],"tags":[]},
            {"uid":12,"identifier":"Biome","values":[{"id":"Grass"}],"tags":[]}
        ]);
        project.json["defs"]["tilesets"] = json!([{"uid":13,"identifier":"Atlas","relPath":null,"pxWid":32,"pxHei":32,"tileGridSize":16,"spacing":0,"padding":0,"tags":[],"tagsSourceEnumUid":11,"enumTags":[{"enumValueId":"Grass","tileIds":[0]},{"enumValueId":"Water","tileIds":[1]}],"savedSelections":[],"customData":[]}]);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Tilesets);
        app.definition_uid = Some(13);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        click(&mut app, &ctx, "Terrain");
        click(&mut app, &ctx, "Biome");
        assert_eq!(
            app.project.as_ref().unwrap().tileset(13).unwrap()["tagsSourceEnumUid"],
            11
        );
        click(&mut app, &ctx, "Cancel");
        assert_eq!(
            app.project.as_ref().unwrap().tileset(13).unwrap()["enumTags"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        click(&mut app, &ctx, "Terrain");
        click(&mut app, &ctx, "Biome");
        click(&mut app, &ctx, "Confirm");
        let tileset = app.project.as_ref().unwrap().tileset(13).unwrap();
        assert_eq!(tileset["tagsSourceEnumUid"], 12);
        assert_eq!(
            tileset["enumTags"],
            json!([{"enumValueId":"Grass","tileIds":[0]}])
        );
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        let tileset = app.project.as_ref().unwrap().tileset(13).unwrap();
        assert_eq!(tileset["tagsSourceEnumUid"], 11);
        assert_eq!(tileset["enumTags"].as_array().unwrap().len(), 2);
        click(&mut app, &ctx, "Terrain");
        click(&mut app, &ctx, "-- None --");
        click(&mut app, &ctx, "Confirm");
        let tileset = app.project.as_ref().unwrap().tileset(13).unwrap();
        assert!(tileset["tagsSourceEnumUid"].is_null());
        assert_eq!(tileset["enumTags"], json!([]));
    }
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
                        Some(text.pos + text.galley.rect.center().to_vec2())
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
        p.update_field_definition(array, &json!({"arrayMinLength":1}))
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

        let output = frame(&mut app, vec![]);
        let clear = point(&output, "Empty array");
        assert!(
            clear.x < point(&output, "7").x,
            "clear belongs in the label column"
        );
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(clear),
                    egui::Event::PointerButton {
                        pos: clear,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.level(key).unwrap()["fieldInstances"][2]["__value"],
            json!([])
        );
        assert!(
            !project
                .field_value_errors(key, None, array)
                .unwrap()
                .is_empty()
        );
        let output = frame(&mut app, vec![]);
        let warning = point(&output, "Array should have 1+ value(s)");
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(button) if button.fill == Color32::from_rgb(49, 55, 66)
                && warning.y < button.rect.top())
        }));
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Vec(parts) if parts.iter().filter(|part|
                matches!(part, egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(121, 17, 5))
            ).count() == 2)
        }));
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
            json!({"worldX":0,"worldY":-128,"pxWid":256,"pxHei":128})
        );
        for sign in [-1, 1] {
            let mut patch = json!({"worldX":sign * 128,"worldY":sign * 192});
            snap_level_form_patch(&p, p.level_keys()[0], &mut patch).unwrap();
            assert_eq!(patch, json!({"worldX":sign * 256,"worldY":sign * 256}));
        }
        p.json["worldLayout"] = json!("Free");
        let mut patch = json!({"worldX":100,"pxWid":300});
        snap_level_form_patch(&p, p.level_keys()[0], &mut patch).unwrap();
        assert_eq!(patch, json!({"worldX":100,"pxWid":300}));
    }
    #[test]
    fn enum_value_rows_keep_fixed_columns_at_two_panel_widths() {
        fn flatten<'a>(shape: &'a egui::Shape, shapes: &mut Vec<&'a egui::Shape>) {
            if let egui::Shape::Vec(children) = shape {
                for child in children {
                    flatten(child, shapes);
                }
            } else {
                shapes.push(shape);
            }
        }

        let mut project = Project::new();
        let local = project.add_enum_definition("State", &["Idle"]).unwrap();
        project.json["defs"]["enums"][0]["values"][0]["color"] = json!(0x47_95_5d);
        let mut external_def = project.json["defs"]["enums"][0].clone();
        let external = project.json["nextUid"].as_i64().unwrap();
        external_def["uid"] = json!(external);
        external_def["identifier"] = json!("ImportedState");
        external_def["externalRelPath"] = json!("values.json");
        project.json["defs"]["externalEnums"] = json!([external_def]);
        project.json["nextUid"] = json!(external + 1);
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);

        for uid in [local, external] {
            app.definition_uid = Some(uid);
            let width_at = |app: &mut EditorApp, screen_width| {
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                ctx.style_mut(|style| style.animation_time = 0.);
                let frame = |app: &mut EditorApp, events| {
                    ctx.run(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(screen_width, 800.),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            app.source_panel(ctx, Panel::Enums);
                        },
                    )
                };
                frame(app, vec![]);
                frame(app, vec![]);
                let panel_width = source_panel_width(Panel::Enums);
                let offset = (panel_width - screen_width).max(0.);
                if offset > 0. {
                    frame(
                        app,
                        vec![
                            egui::Event::PointerMoved(egui::pos2(650., 190.)),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(-500., 0.),
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                    for _ in 0..4 {
                        frame(app, vec![]);
                    }
                }
                let output = frame(app, vec![]);
                let mut shapes = Vec::new();
                for shape in &output.shapes {
                    flatten(&shape.shape, &mut shapes);
                }
                let rects = shapes.iter().filter_map(|shape| match shape {
                    egui::Shape::Rect(rect) => Some(rect),
                    _ => None,
                });
                let swatch_color = Color32::from_rgb(71, 149, 93);
                let swatch = rects
                    .clone()
                    .filter(|rect| rect.fill == swatch_color)
                    .map(|rect| rect.rect)
                    .next()
                    .expect("enum swatch is rendered");
                let name = rects
                    .clone()
                    .find(|rect| {
                        if uid == local {
                            rect.fill == Color32::from_rgb(35, 74, 46)
                        } else {
                            rect.stroke.color == swatch_color
                        }
                    })
                    .expect("enum name is rendered")
                    .rect;
                assert_eq!(swatch.size(), Vec2::splat(32.));
                assert_eq!(name.height(), 32.);
                assert_eq!(name.left() - swatch.right(), 3.);

                let text_pos = |label, region: Rect| {
                    shapes.iter().find_map(|shape| match shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => {
                            let center = text.pos + text.galley.rect.center().to_vec2();
                            region.contains(center).then_some(center)
                        }
                        _ => None,
                    })
                };
                let control_rect = |label, region| {
                    let center =
                        text_pos(label, region).unwrap_or_else(|| panic!("missing {label}"));
                    rects
                        .clone()
                        .filter(|shape| {
                            shape.stroke.width > 0.
                                && (shape.fill != Color32::TRANSPARENT
                                    || shape.stroke.color == Color32::from_rgb(115, 127, 157))
                        })
                        .map(|shape| shape.rect)
                        .filter(|rect| rect.contains(center) && rect.width() >= 24.)
                        .min_by(|a, b| a.area().total_cmp(&b.area()))
                        .unwrap_or_else(|| panic!("missing {label} control"))
                };
                let values = Rect::from_min_max(
                    egui::pos2(name.right(), name.top()),
                    egui::pos2(screen_width, name.bottom()),
                );
                let remove = rects.clone().find(|shape| {
                    let rect = shape.rect;
                    values.contains(rect.center())
                        && rect.height() == name.height()
                        && rect.width() == ENUM_VALUE_SIZE
                });
                if uid == local {
                    let remove = remove.expect("remove button is rendered").rect;
                    assert_eq!(remove.size(), Vec2::splat(32.));
                    assert_eq!(remove.left() - name.right(), 3.);

                    let header = Rect::from_min_max(
                        egui::pos2(300. - offset, 90.),
                        egui::pos2(screen_width, name.top()),
                    );
                    let identifier = control_rect("State", header);
                    let atlas = control_rect("-- Select a tileset --", header);
                    let tags = text_pos("(No tag)", header).expect("empty tag label is rendered");
                    assert_eq!(identifier.left(), atlas.left());
                    assert!((tags.x - identifier.left() - 32.5).abs() < 1.);
                    assert_eq!(identifier.right(), atlas.right());
                    let add = control_rect("ADD A NEW VALUE", header);
                    assert!(add.bottom() < name.top());
                    assert_eq!(add.height(), 35.);
                    assert_eq!(identifier.left() + offset, 404.);
                    assert_eq!(identifier.right() + offset, panel_width - 20.);
                } else {
                    assert!(remove.is_none());
                }
                name.width()
            };
            let narrow = width_at(&mut app, 700.);
            let wide = width_at(&mut app, 1200.);
            assert_eq!(wide, narrow, "window width must not resize the enum values");
        }
    }

    #[test]
    fn adding_an_enum_value_selects_the_first_unused_styled_name_for_typing() {
        let mut project = Project::new();
        let existing: Vec<_> = (0..24).map(|index| format!("Loot{index}")).collect();
        let ids: Vec<_> = existing.iter().map(String::as_str).collect();
        let uid = project.add_enum_definition("Loot", &ids).unwrap();
        project.apply_identifier_style("Lowercase").unwrap();

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        app.definition_uid = Some(uid);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100., 760.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        frame(&mut app, vec![]);
        let pos = frame(&mut app, vec![])
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "ADD A NEW VALUE" => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("add value button is visible");
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        frame(&mut app, vec![]);
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["enums"][0]["values"][24]["id"],
            "loot24"
        );
        let focused = ctx
            .memory(|memory| memory.focused())
            .expect("new value has focus");
        let edit = egui::text_edit::TextEditState::load(&ctx, focused).unwrap();
        assert_eq!(
            edit.cursor.char_range().unwrap().as_sorted_char_range(),
            0..6
        );

        frame(&mut app, vec![egui::Event::Text("replacement".into())]);
        frame(
            &mut app,
            vec![egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["enums"][0]["values"][24]["id"],
            "replacement"
        );
    }

    #[test]
    fn dragging_native_enum_values_preserves_references_and_undo() {
        fn handles(shape: &egui::Shape, positions: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == "≡" => {
                    let center = text.pos + text.galley.size() * 0.5;
                    if center.x > 286. && center.y > 100. {
                        positions.push(center);
                    }
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        handles(shape, positions);
                    }
                }
                _ => {}
            }
        }
        let mut project = Project::new();
        let uid = project
            .add_enum_definition("State", &["Idle", "Run", "Jump"])
            .unwrap();
        let field = project
            .add_field_definition(None, "State", "LocalEnum.State", false)
            .unwrap();
        let level = project.level_keys()[0];
        project.set_field_default(field, json!("Run")).unwrap();
        project
            .set_field_value(level, None, field, json!("Idle"))
            .unwrap();
        let tileset = project
            .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
            .unwrap();
        project
            .update_tileset_definition(
                tileset,
                &json!({
                    "tagsSourceEnumUid": uid,
                    "enumTags":[{"enumValueId":"Jump","tileIds":[0]}]
                }),
            )
            .unwrap();
        project.json["defs"]["enums"][0]["values"][0]["color"] = json!(0x44_88_aa);

        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        app.definition_uid = Some(uid);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        let drag = |app: &mut EditorApp, from: usize, to: usize| {
            let mut positions = Vec::new();
            for shape in frame(app, vec![]).shapes {
                handles(&shape.shape, &mut positions);
            }
            positions.sort_by(|a, b| a.y.total_cmp(&b.y));
            assert_eq!(positions.len(), 3, "one handle per local enum value");
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                app,
                vec![
                    egui::Event::PointerMoved(positions[from]),
                    pointer(positions[from], true),
                ],
            );
            frame(app, vec![egui::Event::PointerMoved(positions[to])]);
            frame(app, vec![pointer(positions[to], false)]);
        };

        frame(&mut app, vec![]);
        let before = app.project.as_ref().unwrap().json.clone();
        let mut expected = before.clone();
        drag(&mut app, 0, 2);
        expected["defs"]["enums"][0]["values"]
            .as_array_mut()
            .unwrap()
            .rotate_left(1);
        assert_eq!(app.project.as_ref().unwrap().json, expected);
        let moved_to_bottom = expected.clone();

        drag(&mut app, 2, 1);
        expected["defs"]["enums"][0]["values"]
            .as_array_mut()
            .unwrap()
            .swap(1, 2);
        assert_eq!(app.project.as_ref().unwrap().json, expected);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, moved_to_bottom);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn enum_icon_tileset_picker_offers_integrated_icons_and_restores_previous_icons() {
        let mut project = Project::new();
        let previous = project
            .add_tileset(Path::new("Plain.png"), 16, 32, 32)
            .unwrap();
        let uid = project.add_enum_definition("State", &["Idle"]).unwrap();
        project
            .update_enum_definition(
                uid,
                &json!({"iconTilesetUid":previous,"values":[{
                    "id":"Idle","tileRect":{"tilesetUid":previous,"x":16,"y":0,"w":16,"h":16}
                }]}),
            )
            .unwrap();
        let before = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        app.definition_uid = Some(uid);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        let label = |out: &egui::FullOutput, name| {
            out.shapes
                .iter()
                .find_map(|item| match &item.shape {
                    egui::Shape::Text(text) if text.galley.job.text == name => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        let click = |app: &mut EditorApp, pos| {
            for pressed in [true, false] {
                frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        frame(&mut app, vec![]);
        let closed = frame(&mut app, vec![]);
        click(&mut app, label(&closed, "Plain"));
        let opened = frame(&mut app, vec![]);
        assert!(label(&opened, "All tilesets").y < label(&opened, "Integrated LDtk tilesets").y);
        click(
            &mut app,
            label(&opened, "⚙ Internal icons by FinalBossBlues"),
        );
        let project = app.project.as_ref().unwrap();
        let definition = &project.json["defs"]["enums"][0];
        let atlas = definition["iconTilesetUid"].as_i64().unwrap();
        assert_eq!(project.tileset(atlas).unwrap()["embedAtlas"], "LdtkIcons");
        assert!(definition["values"][0]["tileRect"].is_null());
        assert_eq!(
            app.last_chance.as_ref().map(|(text, _)| text.as_str()),
            Some("Enum icons changed")
        );
        app.restore_last_chance();
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn enum_icon_picker_matches_thumbnail_and_atlas_mouse_buttons() {
        let mut project = Project::new();
        let image = ldtk_core::exports::embedded_atlas().unwrap();
        let atlas = project
            .add_tileset(
                std::path::Path::new("Icons.png"),
                16,
                image.width() as i32,
                image.height() as i32,
            )
            .unwrap();
        project.json["defs"]["tilesets"][0]["relPath"] = Value::Null;
        project.json["defs"]["tilesets"][0]["embedAtlas"] = json!("LdtkIcons");
        let uid = project
            .add_enum_definition("State", &["Idle", "Run"])
            .unwrap();
        let initial_icon = json!({"tilesetUid":atlas,"x":0,"y":0,"w":16,"h":16});
        project.json["defs"]["enums"][0]["iconTilesetUid"] = json!(atlas);
        project.json["defs"]["enums"][0]["values"][0]["tileRect"] = initial_icon;
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        app.definition_uid = Some(uid);
        app.set_atlas_selection(atlas, vec![7]);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let texture = app.assets.embedded(&ctx).unwrap().id();
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        fn image_rect(shape: &egui::Shape, texture: egui::TextureId, large: bool) -> Option<Rect> {
            match shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    let rect = mesh.calc_bounds();
                    ((rect.width() > 40.) == large && (large || rect.left() > 280.)).then_some(rect)
                }
                egui::Shape::Vec(shapes) => shapes
                    .iter()
                    .find_map(|shape| image_rect(shape, texture, large)),
                _ => None,
            }
        }
        let image_in = |out: egui::FullOutput, large| {
            out.shapes
                .iter()
                .find_map(|item| image_rect(&item.shape, texture, large))
        };
        let gesture = |app: &mut EditorApp, from, to, button| {
            for (pos, pressed) in [(from, true), (to, false)] {
                frame(
                    app,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
            }
        };
        let click = |app: &mut EditorApp, pos, button| gesture(app, pos, pos, button);
        let value = |app: &EditorApp| {
            app.project.as_ref().unwrap().json["defs"]["enums"][0]["values"][0].clone()
        };

        frame(&mut app, vec![]);
        let preview = image_in(frame(&mut app, vec![]), false).expect("32-pixel value preview");
        assert_eq!(preview.size(), egui::vec2(32., 32.));
        let before = app.project.as_ref().unwrap().json.clone();
        click(&mut app, preview.center(), egui::PointerButton::Secondary);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.history.can_undo());
        assert!(image_in(frame(&mut app, vec![]), true).is_none());

        click(&mut app, preview.center(), egui::PointerButton::Primary);
        frame(&mut app, vec![]);
        let picker = image_in(frame(&mut app, vec![]), true).expect("atlas picker");
        gesture(
            &mut app,
            picker.min + egui::vec2(72., 24.),
            picker.min + egui::vec2(120., 72.),
            egui::PointerButton::Primary,
        );
        let chosen = json!({"tilesetUid":atlas,"x":16,"y":0,"w":32,"h":32});
        assert_eq!(value(&app)["tileRect"], chosen);
        assert_eq!(value(&app)["id"], "Idle");
        assert_eq!(app.selected_tile_ids_for(atlas), vec![7]);
        assert!(image_in(frame(&mut app, vec![]), true).is_none());

        let preview = image_in(frame(&mut app, vec![]), false).unwrap();
        click(&mut app, preview.center(), egui::PointerButton::Primary);
        frame(&mut app, vec![]);
        let picker = image_in(frame(&mut app, vec![]), true).unwrap();
        click(
            &mut app,
            picker.min + egui::vec2(120., 72.),
            egui::PointerButton::Secondary,
        );
        assert_eq!(
            value(&app)["tileRect"],
            json!({"tilesetUid":atlas,"x":32,"y":16,"w":16,"h":16})
        );
        assert!(image_in(frame(&mut app, vec![]), true).is_none());
        assert_eq!(app.selected_tile_ids_for(atlas), vec![7]);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(value(&app)["tileRect"], chosen);

        let project = app.project.as_mut().unwrap();
        let uid = project.json["nextUid"].as_i64().unwrap();
        let mut imported = project.json["defs"]["enums"][0].clone();
        imported["uid"] = json!(uid);
        imported["identifier"] = json!("Imported");
        imported["externalRelPath"] = json!("imported.json");
        project.json["defs"]["externalEnums"] = json!([imported]);
        project.json["nextUid"] = json!(uid + 1);
        app.definition_uid = Some(uid);

        frame(&mut app, vec![]);
        let preview = image_in(frame(&mut app, vec![]), false).unwrap();
        let before = app.project.as_ref().unwrap().json.clone();
        click(&mut app, preview.center(), egui::PointerButton::Secondary);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(image_in(frame(&mut app, vec![]), true).is_none());
        click(&mut app, preview.center(), egui::PointerButton::Primary);
        frame(&mut app, vec![]);
        let picker = image_in(frame(&mut app, vec![]), true).expect("imported enum atlas picker");
        click(
            &mut app,
            picker.min + egui::vec2(24., 24.),
            egui::PointerButton::Primary,
        );
        let project = app.project.as_mut().unwrap();
        let imported = &mut project.json["defs"]["externalEnums"][0];
        assert_eq!(imported["values"][0]["tileRect"]["x"], 0);
        assert_eq!(imported["values"][0]["id"], "Idle");
        imported["externalRelPath"] = json!("imported.CDB");
        let before = project.json.clone();

        frame(&mut app, vec![]);
        let preview = image_in(frame(&mut app, vec![]), false).expect("read-only preview");
        click(&mut app, preview.center(), egui::PointerButton::Primary);
        assert!(image_in(frame(&mut app, vec![]), true).is_none());
        click(&mut app, preview.center(), egui::PointerButton::Secondary);
        assert_eq!(app.project.as_ref().unwrap().json, before);
    }

    #[test]
    fn source_external_enum_panel_exposes_the_changed_file_without_auto_selecting_an_enum() {
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, out)),
                _ => (),
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Values.json");
        std::fs::write(&path, r#"{"State":["Idle"]}"#).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Project.ldtk"));
        project.sync_external_enums(&path).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.panel = Some(Panel::Enums);
        std::fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let frame = |app: &mut EditorApp| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.source_panel(ctx, Panel::Enums);
                },
            )
        };
        frame(&mut app);
        let output = frame(&mut app);
        let mut labels = Vec::new();
        for shape in output.shapes {
            texts(&shape.shape, &mut labels);
        }
        assert_eq!(app.definition_uid, None);
        assert!(
            labels.iter().any(|label| label == "Values.json"),
            "{labels:?}"
        );
        assert!(
            labels
                .iter()
                .any(|label| label == "FILE WAS MODIFIED, PLEASE USE SYNC."),
            "{labels:?}"
        );
        assert!(!app.dirty);
    }

    #[test]
    fn importing_external_enums_without_removals_applies_immediately() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        std::fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let ctx = egui::Context::default();
        app.begin_enum_sync(&ctx, path.clone());
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["externalEnums"][0]["values"][1]["id"],
            "Run"
        );
        assert!(app.dirty);
        assert!(app.history.can_undo());
        assert!(!app.enum_sync_is_open());
        let project = app.project.as_ref().unwrap();
        assert!(!project.external_enum_source_changed(&path).unwrap());
        std::fs::write(&path, r#"{"State":["Idle","Run","Fly"]}"#).unwrap();
        assert!(project.external_enum_source_changed(&path).unwrap());
    }
}
