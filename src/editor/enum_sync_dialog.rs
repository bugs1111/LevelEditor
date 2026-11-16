use super::*;
use ldtk_core::enum_sync::EnumSyncReport;
use std::collections::BTreeMap;

#[derive(Clone)]
struct PendingSync {
    path: PathBuf,
    source: Vec<u8>,
    report: EnumSyncReport,
    renames: BTreeMap<String, String>,
    remove_used: bool,
}
fn dialog_id(project: &Project) -> egui::Id {
    egui::Id::new(("external-enum-sync", text(&project.json, "iid", "")))
}
impl EditorApp {
    pub(in crate::app) fn begin_enum_sync(&mut self, ctx: &egui::Context, path: PathBuf) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let result = (|| -> ldtk_core::Result<PendingSync> {
            let source = std::fs::read(&path).map_err(|source| ldtk_core::Error::Io {
                path: path.clone(),
                source,
            })?;
            let mut preview = project.clone();
            let report = preview.sync_external_enums(&path)?;
            let requires_confirmation = report.requires_confirmation;
            let mut report = if requires_confirmation {
                preview.apply_external_enum_sync(&path, &BTreeMap::new(), true)?
            } else {
                report
            };
            report.requires_confirmation = requires_confirmation;
            report.applied = false;
            Ok(PendingSync {
                path,
                source,
                report,
                renames: BTreeMap::new(),
                remove_used: false,
            })
        })();
        match result {
            Ok(pending) => ctx.data_mut(|data| data.insert_temp(dialog_id(project), pending)),
            Err(error) => {
                self.status = error.to_string();
                self.status_error = true;
            }
        }
    }
    pub(in crate::app) fn enum_sync_dialog(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let id = dialog_id(project);
        let Some(mut pending) = ctx.data_mut(|data| data.get_temp::<PendingSync>(id)) else {
            return;
        };
        let mut close = false;
        let mut apply = false;
        egui::Modal::new(id).show(ctx, |ui| {
            ui.set_max_width(620.0);
            ui.heading("Synchronize external enums");
            ui.label(pending.path.display().to_string());
            if pending.report.added.is_empty() && pending.report.removed.is_empty() && pending.report.changed.is_empty() {
                ui.label("The source is already up to date.");
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for added in &pending.report.added { ui.colored_label(Color32::from_rgb(100,210,120), format!("+ {added}")); }
                for changed in &pending.report.changed { ui.label(format!("Updated {changed}")); }
                for removed in &pending.report.removed {
                    ui.horizontal(|ui| {
                        ui.label(format!("− {removed}"));
                        let mut replacement = pending.renames.get(removed).cloned().unwrap_or_default();
                        let old = replacement.clone();
                        egui::ComboBox::from_id_salt(("enum-sync-resolution", removed))
                            .selected_text(if replacement.is_empty() { "Remove" } else { &replacement })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut replacement, String::new(), "Remove");
                                if let Some((enu, _)) = removed.split_once('.') {
                                    let enu = pending.renames.get(enu).map(String::as_str).unwrap_or(enu);
                                    for target in &pending.report.added {
                                        if let Some((target_enum, value)) = target.split_once('.') && target_enum == enu {
                                            ui.selectable_value(&mut replacement, value.to_owned(), format!("Rename to {value}"));
                                        }
                                    }
                                } else {
                                    for target in &pending.report.added {
                                        if !target.contains('.') { ui.selectable_value(&mut replacement, target.clone(), format!("Rename to {target}")); }
                                    }
                                }
                            });
                        if old != replacement {
                            if replacement.is_empty() { pending.renames.remove(removed); }
                            else { pending.renames.insert(removed.clone(), replacement); }
                        }
                    });
                }
            });
            if pending.report.requires_confirmation {
                ui.label("Some removed enums or values are used in this project. Match their replacements above, or allow removal. Removing an enum also removes its custom fields.");
                ui.checkbox(&mut pending.remove_used, "Allow removal of used values and fields");
            }
            ui.horizontal(|ui| {
                if source_controls::button(ui,"Apply synchronization").clicked() { apply = true; }
                if source_controls::button(ui,"Cancel").clicked() { close = true; }
            });
        });
        if apply {
            if std::fs::read(&pending.path).ok().as_deref() != Some(pending.source.as_slice()) {
                self.begin_enum_sync(ctx, pending.path.clone());
                self.status =
                    "The enum source changed. Review its updated changes before applying.".into();
                self.status_error = true;
                return;
            }
            let operation = |project: &mut Project| {
                project.apply_external_enum_sync(
                    &pending.path,
                    &pending.renames,
                    pending.remove_used,
                )
            };
            let result = if pending.report.requires_confirmation {
                self.edit_recoverable("External enums synchronized", operation)
            } else {
                self.edit("External enums synchronized", operation)
            };
            if let Some(report) = result {
                if report.applied {
                    self.status = format!(
                        "External enums synchronized: {} added, {} removed",
                        report.added.len(),
                        report.removed.len()
                    );
                    self.file_snapshot = self
                        .project
                        .as_ref()
                        .and_then(|project| project.filesystem_snapshot().ok());
                    close = true;
                } else {
                    self.status =
                        "Resolve every used enum/value removal, or explicitly allow removal."
                            .into();
                    self.status_error = true;
                }
            }
        }
        ctx.data_mut(|data| {
            if close {
                data.remove::<PendingSync>(id);
            } else {
                data.insert_temp(id, pending);
            }
        });
    }
}
impl EditorApp {
    pub(super) fn pick_project_file(
        &mut self,
        ui_id: &str,
        extensions: &[&str],
    ) -> Option<PathBuf> {
        let project = self.project.as_ref().and_then(|p| p.path.clone());
        let mut dialog = rfd::FileDialog::new().add_filter("File", extensions);
        if let Some(dir) = self.settings.ui_dir(project.as_deref(), ui_id, None) {
            dialog = dialog.set_directory(dir);
        }
        let path = dialog.pick_file()?;
        if let Some(dir) = path.parent() {
            self.settings.store_ui_dir(project.as_deref(), ui_id, dir);
        }
        Some(path)
    }
    pub(super) fn import_enum_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Import / sync external…", |ui| {
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
