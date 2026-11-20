use super::*;
use ldtk_core::enum_sync::EnumSyncReport;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq)]
enum SyncAction {
    Rename(String),
    Remove,
}

pub(in crate::app) struct PendingSync {
    path: PathBuf,
    source: Vec<u8>,
    report: EnumSyncReport,
    actions: BTreeMap<String, SyncAction>,
}

fn dialog_id(project: &Project) -> egui::Id {
    egui::Id::new(("external-enum-sync", text(&project.json, "iid", "")))
}

fn enum_name(key: &str) -> &str {
    key.split_once('.').map_or(key, |(name, _)| name)
}

fn rename_options<'a>(report: &'a EnumSyncReport, removed: &str) -> impl Iterator<Item = &'a str> {
    let owner = removed.split_once('.').map(|(name, _)| name.to_owned());
    report
        .added
        .iter()
        .filter_map(move |key| match (owner.as_deref(), key.split_once('.')) {
            (Some(owner), Some((target, value))) if owner == target => Some(value),
            (None, None) => Some(key.as_str()),
            _ => None,
        })
}

impl PendingSync {
    fn ready(&self) -> bool {
        self.report.removed.iter().all(|key| {
            self.actions.contains_key(key) || rename_options(&self.report, key).next().is_none()
        })
    }

    fn removal_picker(&mut self, ui: &mut egui::Ui, key: &str) {
        let options = rename_options(&self.report, key).collect::<Vec<_>>();
        if options.is_empty() {
            return;
        }
        let name = key.split_once('.').map_or(key, |(_, value)| value);
        let remove = if key.contains('.') {
            format!("REMOVE {key} FROM PROJECT")
        } else {
            format!("REMOVE {key} ENUM FROM PROJECT")
        };
        let mut choice = self.actions.get(key).cloned();
        let summary = match &choice {
            Some(SyncAction::Rename(target)) => format!("Rename {name} ➜ {target}"),
            Some(SyncAction::Remove) => remove.clone(),
            None => "-- Choose an action --".to_owned(),
        };
        let text = RichText::new(summary).size(14.).color(if choice.is_none() {
            theme::TEXT.gamma_multiply(0.6)
        } else {
            Color32::WHITE
        });
        egui::ComboBox::from_id_salt(("enum-sync-action", key))
            .width(ui.available_width().clamp(90., 348.))
            .selected_text(text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, None, "-- Choose an action --");
                for target in options {
                    ui.selectable_value(
                        &mut choice,
                        Some(SyncAction::Rename(target.to_owned())),
                        format!("Rename {name} ➜ {target}"),
                    );
                }
                ui.selectable_value(&mut choice, Some(SyncAction::Remove), remove);
            });
        match choice {
            Some(choice) => {
                self.actions.insert(key.to_owned(), choice);
            }
            None => {
                self.actions.remove(key);
            }
        }
    }
}

const ADDED: Color32 = Color32::from_rgb(207, 221, 54);
const REMOVED: Color32 = Color32::from_rgb(227, 77, 0);

fn change_badge(ui: &mut egui::Ui, added: bool) {
    let (label, background, foreground) = if added {
        ("ADDED", ADDED, Color32::BLACK)
    } else {
        ("REMOVED", REMOVED, Color32::WHITE)
    };
    egui::Frame::new()
        .fill(background)
        .corner_radius(2.)
        .inner_margin(egui::Margin::symmetric(4, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(10.).color(foreground));
        });
}

fn sync_changes(ui: &mut egui::Ui, project: &Project, pending: &mut PendingSync) {
    let added = pending.report.added.clone();
    let removed = pending.report.removed.clone();
    let mut changed_enums = Vec::new();
    for key in added.iter().chain(&removed) {
        let name = enum_name(key);
        if !changed_enums.contains(&name) {
            changed_enums.push(name);
        }
    }
    let relative = project.make_relative_file_path(&pending.path).ok();
    let definitions = project.json["defs"]["externalEnums"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|def| Some(text(def, "externalRelPath", "")) == relative.as_deref())
        .collect::<Vec<_>>();

    for name in &changed_enums {
        let name = *name;
        let remove_enum = removed.iter().any(|key| key == name);
        let add_enum = added.iter().any(|key| key == name);
        ui.horizontal(|ui| {
            let title = RichText::new(name)
                .color(if remove_enum {
                    REMOVED
                } else if add_enum {
                    ADDED
                } else {
                    Color32::WHITE
                })
                .size(16.);
            ui.label(if remove_enum {
                title.strikethrough()
            } else {
                title
            });
            if remove_enum {
                pending.removal_picker(ui, name);
            } else if add_enum {
                change_badge(ui, true);
            }
        });
        ui.indent(("enum-sync-values", name), |ui| {
            for key in removed
                .iter()
                .filter(|key| key.contains('.') && enum_name(key) == name)
            {
                let (_, value) = key.split_once('.').unwrap();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(value)
                            .color(REMOVED)
                            .size(13.333)
                            .strikethrough(),
                    );
                    change_badge(ui, false);
                    pending.removal_picker(ui, key);
                });
            }
            for key in added
                .iter()
                .filter(|key| key.contains('.') && enum_name(key) == name)
            {
                let (_, value) = key.split_once('.').unwrap();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(value).color(ADDED).size(13.333));
                    change_badge(ui, true);
                });
            }
            if let Some(def) = definitions.iter().find(|def| def["identifier"] == name) {
                let unchanged = def["values"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(5)
                    .enumerate();
                for (index, value) in unchanged {
                    let value = text(value, "id", "");
                    if index == 4 || !removed.iter().any(|key| key == &format!("{name}.{value}")) {
                        ui.label(
                            RichText::new(if index == 4 { "(...)" } else { value })
                                .color(theme::LIGHT)
                                .size(13.333)
                                .italics(),
                        );
                    }
                }
            }
        });
        ui.add_space(12.);
    }
    for def in definitions {
        let name = text(def, "identifier", "");
        if !changed_enums.contains(&name) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme::TEXT.gamma_multiply(0.62)));
                ui.label(
                    RichText::new("UNCHANGED")
                        .color(theme::TEXT.gamma_multiply(0.55))
                        .size(10.)
                        .italics(),
                );
            });
        }
    }
    if added.is_empty() && removed.is_empty() && pending.report.changed.is_empty() {
        ui.label("The source is already up to date.");
    }
}

impl EditorApp {
    pub(in crate::app) fn enum_sync_is_open(&self) -> bool {
        self.pending_enum_sync.is_some()
            || self.pending_enum_removal.is_some()
            || !self.external_enum_reloads.is_empty()
    }

    pub(in crate::app) fn begin_enum_sync(&mut self, ctx: &egui::Context, path: PathBuf) -> bool {
        let previous_status = (self.status.clone(), self.status_error);
        let Some(report) = self.edit("External enums updated", |p| p.sync_external_enums(&path))
        else {
            return false;
        };
        if !report.requires_confirmation {
            if self
                .pending_enum_sync
                .as_ref()
                .is_some_and(|pending| pending.path == path)
            {
                self.pending_enum_sync = None;
            }
            self.finish_enum_sync(&path, &report);
            return true;
        }

        (self.status, self.status_error) = previous_status;
        let Some(project) = self.project.as_ref() else {
            return false;
        };
        let result = (|| -> ldtk_core::Result<PendingSync> {
            let source = std::fs::read(&path).map_err(|source| ldtk_core::Error::Io {
                path: path.clone(),
                source,
            })?;
            let mut preview = project.clone();
            let report = preview.apply_external_enum_sync(&path, &BTreeMap::new(), true)?;
            Ok(PendingSync {
                path,
                source,
                report,
                actions: BTreeMap::new(),
            })
        })();
        match result {
            Ok(pending) => {
                self.pending_enum_sync = Some(pending);
                ctx.request_repaint();
                true
            }
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
                false
            }
        }
    }

    fn finish_enum_sync(&mut self, path: &std::path::Path, report: &EnumSyncReport) {
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let message =
            if report.added.is_empty() && report.removed.is_empty() && report.changed.is_empty() {
                "Enums are already up-to-date."
            } else {
                "Enums updated successfully."
            };
        self.status = format!("{name}: {message}");
        if let Some(snapshot) = &mut self.file_snapshot {
            let _ = snapshot.refresh_file(path);
        }
    }

    fn enum_reload_dialog(&mut self, ctx: &egui::Context) {
        let Some(path) = self.external_enum_reloads.front() else {
            return;
        };
        let relative = self
            .project
            .as_ref()
            .and_then(|project| project.make_relative_file_path(path).ok())
            .unwrap_or_else(|| path.display().to_string());
        let width = ctx.content_rect().width().clamp(210., 440.);
        let modal = egui::Modal::new(egui::Id::new("external-enum-file-modified"))
            .frame(project_dialogs::dialog_frame(0))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                project_dialogs::dialog_title(ui, "FILE WAS MODIFIED", width, theme::LIGHT);
                egui::Frame::new().inner_margin(20).show(ui, |ui| {
                    let width = width - 40.;
                    ui.set_width(width);
                    ui.label(RichText::new("The following file has been modified externally and should be updated:").size(14.667).color(Color32::WHITE));
                    ui.add_space(14.);
                    egui::Frame::new()
                        .fill(theme::DARK)
                        .stroke(Stroke::new(1., REMOVED))
                        .inner_margin(4)
                        .show(ui, |ui| {
                            ui.set_width(width - 10.);
                            ui.label(RichText::new(relative).size(14.667).strong().color(Color32::WHITE));
                        });
                    ui.add_space(14.);
                    ui.label(RichText::new("What do you want to do?").size(14.667).color(Color32::WHITE));
                    ui.add_space(16.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let size = egui::vec2((width - 2.) / 2., 35.);
                        let reload = project_dialogs::button(ui, "RELOAD IT", size, false, 13.333).clicked();
                        let later = project_dialogs::button(ui, "FIX THAT LATER", size, true, 13.333).clicked();
                        (reload, later)
                    }).inner
                }).inner
            });
        let (reload, later) = modal.inner;
        if reload || later || modal.should_close() {
            if let Some(path) = self.external_enum_reloads.pop_front()
                && reload
            {
                self.begin_enum_sync(ctx, path);
            }
            ctx.request_repaint();
        }
    }

    pub(in crate::app) fn enum_sync_dialog(&mut self, ctx: &egui::Context) {
        if let Some(source) = self.pending_enum_removal.take() {
            self.remove_external_enum_source_dialog(ctx, source);
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let id = dialog_id(project);
        let Some(mut pending) = self.pending_enum_sync.take() else {
            self.enum_reload_dialog(ctx);
            return;
        };
        let name = pending
            .path
            .file_name()
            .unwrap_or(pending.path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let width = ctx.content_rect().width().clamp(210., 800.);
        let modal = egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(90))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                project_dialogs::dialog_title(ui, &format!("SYNCING \"{name}\"..."), width, theme::LIGHT);
                egui::Frame::new().inner_margin(20).show(ui, |ui| {
                    let width = width - 40.;
                    ui.set_width(width);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(85, 3, 1))
                        .stroke(Stroke::new(1., Color32::RED))
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_width(width - 18.);
                            ui.label(RichText::new("This file was updated by an external program and you need to make some decisions with the changes that happened.").size(14.667).color(Color32::WHITE));
                        });
                    ui.add_space(16.);
                    let height = (ctx.content_rect().height() - 205.).clamp(100., 300.);
                    egui::Frame::new()
                        .fill(theme::DARK)
                        .stroke(Stroke::new(1., theme::LIGHT))
                        .inner_margin(12)
                        .show(ui, |ui| {
                            let log_width = (ctx.content_rect().width() * 0.8).min(width);
                            ui.set_width(log_width - 26.);
                            ui.set_height(height - 26.);
                            ui.spacing_mut().item_spacing = egui::vec2(9., 2.);
                            egui::ScrollArea::vertical()
                                .max_height(height - 26.)
                                .auto_shrink([false, false])
                                .show(ui, |ui| sync_changes(ui, project, &mut pending));
                        });
                    ui.add_space(16.);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        let size = egui::vec2((width - 2.) / 2., 35.);
                        let apply = ui.add_enabled_ui(pending.ready(), |ui| {
                            project_dialogs::button(ui, "APPLY THESE CHANGES", size, false, 13.333)
                        }).inner.clicked();
                        let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                        (apply, cancel)
                    }).inner
                }).inner
            });
        let (apply, cancel) = modal.inner;
        let mut close = cancel || modal.should_close();
        if apply {
            if std::fs::read(&pending.path).ok().as_deref() != Some(pending.source.as_slice()) {
                self.begin_enum_sync(ctx, pending.path.clone());
                return;
            }
            let renames = pending
                .actions
                .iter()
                .filter_map(|(key, action)| {
                    if let SyncAction::Rename(target) = action {
                        Some((key.clone(), target.clone()))
                    } else {
                        None
                    }
                })
                .collect();
            let operation = |project: &mut Project| {
                project.apply_external_enum_sync(&pending.path, &renames, true)
            };
            let result = self.edit_recoverable("External enums synchronized", operation);
            if let Some(report) = result {
                if report.applied {
                    self.finish_enum_sync(&pending.path, &report);
                    close = true;
                } else {
                    self.status =
                        "Resolve each changed enum or value before applying synchronization."
                            .into();
                    self.status_error = true;
                }
            }
        }
        if !close {
            self.pending_enum_sync = Some(pending);
        }
    }

    fn remove_external_enum_source_dialog(&mut self, ctx: &egui::Context, source: String) {
        let width = (ctx.content_rect().width() - 40.).clamp(180., 360.);
        let modal = egui::Modal::new(egui::Id::new("remove-external-enum-source"))
            .frame(project_dialogs::dialog_frame(20).fill(Color32::from_rgb(112, 20, 4)))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                ui.label(RichText::new("WARNING: removing this external enum will also remove ALL the external enums from the same source! Please note that this will also affect all Entities using any of these enums in ALL levels.").size(14.).color(Color32::WHITE));
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    let size = egui::vec2((width - 2.) / 2., 35.);
                    let confirm = project_dialogs::button(ui, "CONFIRM", size, false, 13.333).clicked();
                    let cancel = project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                    (confirm, cancel)
                }).inner
            });
        let (confirm, cancel) = modal.inner;
        if confirm {
            let filename = source.rsplit(['/', '\\']).next().unwrap_or(&source);
            if self
                .edit_recoverable(&format!("{filename} enums deleted"), |project| {
                    project.remove_external_enum_source(&source)
                })
                .is_some()
            {
                self.definition_uid = None;
            }
        } else if !cancel && !modal.should_close() {
            self.pending_enum_removal = Some(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_removal_with_a_replacement_requires_its_own_action() {
        let mut pending = PendingSync {
            path: PathBuf::from("Game.cdb"),
            source: Vec::new(),
            report: EnumSyncReport {
                added: ["Loot.New", "Moves.Jump"].map(str::to_owned).to_vec(),
                removed: ["Loot.Old", "Moves.Walk", "Other.Value"]
                    .map(str::to_owned)
                    .to_vec(),
                ..Default::default()
            },
            actions: BTreeMap::new(),
        };
        assert!(!pending.ready());
        assert_eq!(
            rename_options(&pending.report, "Loot.Old").collect::<Vec<_>>(),
            ["New"]
        );
        pending
            .actions
            .insert("Loot.Old".into(), SyncAction::Rename("New".into()));
        assert!(!pending.ready());
        pending
            .actions
            .insert("Moves.Walk".into(), SyncAction::Remove);
        assert!(pending.ready());
    }
}
impl EditorApp {
    pub(super) fn pick_project_file(
        &mut self,
        ui_id: &str,
        extensions: &[&str],
    ) -> Option<PathBuf> {
        let project = self.project.as_ref().and_then(|p| p.path.clone());
        let mut dialog = FileDialog::new().add_filter("File", extensions);
        if let Some(dir) = self.settings.ui_dir(project.as_deref(), ui_id, None) {
            dialog = dialog.set_directory(dir);
        }
        let path = dialog.pick_file()?;
        if let Some(dir) = path.parent() {
            self.settings.store_ui_dir(project.as_deref(), ui_id, dir);
        }
        Some(path)
    }
    pub(super) fn import_enum_menu(&mut self, ui: &mut egui::Ui, label: &str) {
        ui.menu_button(label, |ui| {
            for (label, key, extension) in [
                ("Text file", "ImportEnumText", "txt"),
                ("JSON", "ImportEnumText", "json"),
                ("Haxe source code", "ImportEnumHaxe", "hx"),
                ("CastleDB", "ImportEnumCdb", "cdb"),
            ] {
                if source_controls::button(ui, label).clicked() {
                    if let Some(path) = self.pick_project_file(key, &[extension]) {
                        self.begin_enum_sync(ui.ctx(), path);
                    }
                    ui.close();
                }
            }
        });
    }
}
