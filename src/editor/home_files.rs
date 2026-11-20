use super::*;
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct BackupIndex {
    paths: Vec<PathBuf>,
    entries: HashMap<PathBuf, Vec<ldtk_core::BackupInfo>>,
    frame: u64,
    loading: bool,
}
fn index(ctx: &egui::Context) -> Arc<Mutex<BackupIndex>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<BackupIndex>>>(egui::Id::new("home-backups"))
            .clone()
    })
}
fn read_backups(path: &Path) -> Vec<ldtk_core::BackupInfo> {
    Project::list_backups_at(path).unwrap_or_default()
}
pub(super) fn prepare(ctx: &egui::Context, paths: Vec<PathBuf>) {
    let state = index(ctx);
    let mut guard = state.lock().unwrap();
    let frame = ctx.cumulative_frame_nr();
    let refresh = guard.paths != paths || frame > guard.frame + 1;
    guard.frame = frame;
    if !refresh {
        return;
    }
    guard.paths = paths.clone();
    guard.entries.clear();
    guard.loading = true;
    drop(guard);
    let ctx = ctx.clone();
    // One serial reader keeps the home screen responsive even with large
    // projects. External level files are unnecessary for backup discovery.
    std::thread::spawn(move || {
        let entries = paths
            .iter()
            .map(|path| (path.clone(), read_backups(path)))
            .collect();
        let mut state = state.lock().unwrap();
        if state.paths == paths {
            state.entries = entries;
            state.loading = false;
        }
        ctx.request_repaint();
    });
}
pub(super) fn locate(path: &Path, select: bool) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "macos")]
    if select {
        command.arg("-R");
    }
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer");
    #[cfg(target_os = "windows")]
    if select {
        command.arg("/select,");
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let path = if select {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    command.arg(path).spawn().map(|_| ())
}
impl EditorApp {
    pub(super) fn home_backups_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        let state = index(ui.ctx());
        let state = state.lock().unwrap();
        let entries = state.entries.get(path).cloned();
        let loading = state.loading;
        drop(state);
        if let Some(entries) = entries.filter(|e| !e.is_empty()) {
            ui.menu_button(locale::tr(ui, "Backups"), |ui| {
                for entry in entries {
                    let title = if entry.crash {
                        locale::format(
                            ui,
                            "Crash recovery · {timestamp}",
                            &[("timestamp", &entry.timestamp)],
                        )
                    } else {
                        entry.timestamp
                    };
                    if ui.button(title).clicked() {
                        self.request(
                            ui.ctx(),
                            Action::OpenBackup {
                                backup: entry.path,
                                original: path.to_owned(),
                            },
                        );
                        ui.close();
                    }
                }
            });
        } else if loading {
            ui.spinner();
        }
    }
    pub(super) fn recent_file_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        let dir = path.parent().unwrap_or(Path::new("."));
        if ui.button(locale::tr(ui, "Load from this folder")).clicked() {
            self.request(ui.ctx(), Action::BrowseInDir(dir.to_owned()));
            ui.close();
        }
        if ui.button(locale::tr(ui, "Locate file")).clicked() {
            if let Err(error) = locate(path, true) {
                self.status = error.to_string();
                self.status_error = true;
            }
            ui.close();
        }
        self.directory_color_menu(ui, dir);
        if ui.button(locale::tr(ui, "Remove from history")).clicked() {
            self.remove_recent("recentProjects", path);
            ui.close();
        }
        if ui.button(locale::tr(ui, "Clear all history")).clicked() {
            self.settings.value["recentProjects"] = serde_json::json!([]);
            let _ = self.settings.save();
            ui.close();
        }
    }
    pub(super) fn recent_folder_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        if ui
            .button(locale::tr(ui, "New project in this folder"))
            .clicked()
        {
            self.request(ui.ctx(), Action::NewInDir(path.to_owned()));
            ui.close();
        }
        if ui.button(locale::tr(ui, "Locate folder")).clicked() {
            if let Err(error) = locate(path, false) {
                self.status = error.to_string();
                self.status_error = true;
            }
            ui.close();
        }
        self.directory_color_menu(ui, path);
        if ui.button(locale::tr(ui, "Remove from history")).clicked() {
            self.remove_recent("recentDirs", path);
            ui.close();
        }
        if ui
            .button(locale::tr(ui, "Clear all folder history"))
            .clicked()
        {
            self.settings.value["recentDirs"] = serde_json::json!([]);
            let _ = self.settings.save();
            ui.close();
        }
    }
    fn remove_recent(&mut self, kind: &str, path: &Path) {
        if let Some(entries) = self.settings.value[kind].as_array_mut() {
            entries.retain(|entry| entry.as_str() != path.to_str());
        }
        let _ = self.settings.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn home_backup_index_reads_metadata_without_building_a_project_or_its_large_ignored_data() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("World.ldtk");
        let folder = dir.path().join("saved/home-id_2026-11-17_01-02-03_crash");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("World.ldtk"), r#"{"iid":"home-id"}"#).unwrap();
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"{\"iid\":\"home-id\",\"backupRelPath\":\"saved\",\"future\":\"")
            .unwrap();
        let chunk = [b'x'; 8192];
        for _ in 0..1024 {
            file.write_all(&chunk).unwrap();
        }
        file.write_all(b"\",\"levels\":[{\"externalRelPath\":\"missing.ldtkl\"}]}")
            .unwrap();
        drop(file);
        let entries = read_backups(&path);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, folder.join("World.ldtk"));
        assert!(entries[0].crash);
        assert_eq!(entries[0].timestamp, "2026-11-17_01-02-03");
    }
    #[test]
    fn home_backup_discovery_does_not_require_external_levels_to_be_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("World.ldtk");
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let backup = project.backup().unwrap().unwrap();
        let external = project.json["levels"][0]["externalRelPath"]
            .as_str()
            .unwrap();
        std::fs::remove_file(dir.path().join(external)).unwrap();
        assert!(Project::load(&path).is_err());
        assert!(
            read_backups(&path)
                .iter()
                .any(|entry| entry.path == backup.join("World.ldtk"))
        );
    }
}
