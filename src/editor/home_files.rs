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
const DELETE_CRASH_BACKUPS: &str = "delete-home-crash-backups";

fn backup_labels(
    ui: &egui::Ui,
    entry: &ldtk_core::BackupInfo,
    age: Option<u64>,
) -> (String, Option<String>) {
    let stamp = &entry.timestamp;
    let date = (|| {
        let month = stamp.get(5..7)?.parse::<usize>().ok()?;
        let month_name = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ]
        .get(month.checked_sub(1)?)?;
        let day = stamp.get(8..10)?.parse::<u8>().ok()?;
        let ordinal = match day {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        };
        Some(locale::format(
            ui,
            "{month_name} {day}{ordinal} {year} at {hour}:{minute}",
            &[
                ("month_name", month_name),
                ("month", &month.to_string()),
                ("day", &day.to_string()),
                ("ordinal", ordinal),
                ("year", stamp.get(..4)?),
                ("hour", stamp.get(11..13)?),
                ("minute", stamp.get(14..16)?),
            ],
        ))
    })()
    .unwrap_or_else(|| stamp.clone());
    let title = if entry.crash {
        locale::tr(ui, "Crash recovery").to_owned()
    } else {
        match age {
            Some(seconds @ 0..=59) => locale::format(
                ui,
                "{count} seconds ago",
                &[("count", &seconds.to_string())],
            ),
            Some(60..=119) => locale::tr(ui, "1 minute ago").to_owned(),
            Some(seconds @ 120..=3599) => locale::format(
                ui,
                "{count} minutes ago",
                &[("count", &(seconds / 60).to_string())],
            ),
            Some(3600..=7199) => locale::tr(ui, "1 hour ago").to_owned(),
            Some(seconds @ 7200..=86399) => locale::format(
                ui,
                "{count} hours ago",
                &[("count", &(seconds / 3600).to_string())],
            ),
            _ => date.clone(),
        }
    };
    let subtitle = (title != date).then_some(date);
    (title.to_uppercase(), subtitle)
}

fn backup_menu_row(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    subtitle: Option<&str>,
    fill: Color32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, if subtitle.is_some() { 48. } else { 32. }),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    ui.painter().rect_filled(
        rect,
        0.,
        if response.hovered() {
            theme::LIGHT
        } else {
            fill
        },
    );
    let name_pos = rect.left_top() + egui::vec2(20., 16.);
    ui.painter().text(
        name_pos,
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.333),
        theme::ACCENT,
    );
    if let Some(subtitle) = subtitle {
        ui.painter().text(
            name_pos + egui::vec2(0., 16.),
            Align2::LEFT_CENTER,
            subtitle,
            FontId::proportional(12.),
            Color32::from_rgb(115, 127, 157),
        );
    }
    response.clicked()
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
            let button = |ui: &mut egui::Ui| {
                ui.spacing_mut().button_padding = egui::vec2(2., 0.);
                panels::source_controls::add_sized_button(
                    ui,
                    ui.max_rect().size(),
                    egui::Button::image(
                        egui::Image::new(egui::include_image!("../../assets/icons/history.svg"))
                            .fit_to_exact_size(Vec2::splat(24.)),
                    )
                    .fill(theme::LIGHT),
                )
            };
            let button = button(ui).on_hover_text(locale::tr(ui, "Backups"));
            button.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, locale::tr(ui, "Backups"))
            });
            egui::Popup::menu(&button)
                .anchor(Rect::from_pos(button.rect.left_top() + egui::vec2(2., 8.)))
                .align(egui::emath::RectAlign::TOP_START)
                .frame(panels::source_controls::context_menu_frame(
                    &ui.ctx().style(),
                ))
                .show(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    let now = std::time::SystemTime::now();
                    let labels = entries
                        .iter()
                        .map(|entry| backup_labels(ui, entry, entry.age_in_seconds(now)))
                        .collect::<Vec<_>>();
                    let crashes = entries
                        .iter()
                        .filter(|entry| entry.crash)
                        .map(|entry| entry.path.clone())
                        .collect::<Vec<_>>();
                    let delete = locale::tr(ui, "Delete all crash recovery files").to_uppercase();
                    let width = labels
                        .iter()
                        .flat_map(|(title, subtitle)| {
                            std::iter::once(title.as_str()).chain(subtitle.as_deref())
                        })
                        .chain((!crashes.is_empty()).then_some(delete.as_str()))
                        .map(|label| {
                            ui.painter()
                                .layout_no_wrap(
                                    label.to_owned(),
                                    FontId::proportional(13.333),
                                    Color32::WHITE,
                                )
                                .size()
                                .x
                        })
                        .fold(0., f32::max)
                        + 40.;
                    for (entry, (title, subtitle)) in entries.into_iter().zip(labels) {
                        let fill = if entry.crash {
                            Color32::from_rgb(60, 97, 72)
                        } else {
                            theme::DARK
                        };
                        if backup_menu_row(ui, width, &title, subtitle.as_deref(), fill) {
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
                    if !crashes.is_empty()
                        && backup_menu_row(ui, width, &delete, None, Color32::from_rgb(121, 17, 5))
                    {
                        ui.ctx().data_mut(|data| {
                            data.insert_temp(
                                egui::Id::new(DELETE_CRASH_BACKUPS),
                                (path.to_owned(), crashes),
                            )
                        });
                        ui.close();
                    }
                });
        } else if loading {
            ui.spinner();
        }
    }

    pub(super) fn home_backup_delete_open(ctx: &egui::Context) -> bool {
        ctx.data(|data| {
            data.get_temp::<(PathBuf, Vec<PathBuf>)>(egui::Id::new(DELETE_CRASH_BACKUPS))
                .is_some()
        })
    }

    pub(super) fn home_backup_delete_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new(DELETE_CRASH_BACKUPS);
        let Some((project, files)) = ctx.data(|data| data.get_temp::<(PathBuf, Vec<PathBuf>)>(id))
        else {
            return;
        };
        let modal = egui::Modal::new(id)
            .frame(project_dialogs::dialog_frame(20).fill(Color32::from_rgb(121, 17, 5)))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 40.).clamp(0., 358.));
                let name = project.file_stem().unwrap_or_default().to_string_lossy();
                ui.label(locale::format(
                    ui,
                    "Delete all crash recovery files project {name}?",
                    &[("name", &name)],
                ));
                ui.horizontal(|ui| {
                    let size = egui::vec2((ui.available_width() - 2.) * 0.5, 35.);
                    let confirm =
                        project_dialogs::button(ui, "CONFIRM", size, false, 13.333).clicked();
                    let cancel =
                        project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                    (confirm, cancel)
                })
                .inner
            });
        let (confirm, cancel) = modal.inner;
        let cancel = cancel || modal.should_close();
        let confirm = confirm
            || modal.is_top_modal
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        if confirm || cancel {
            ctx.data_mut(|data| data.remove::<(PathBuf, Vec<PathBuf>)>(id));
        }
        if confirm && !cancel {
            let result = Project::list_backups_at(&project)
                .map_err(|error| error.to_string())
                .and_then(|entries| {
                    for entry in entries
                        .into_iter()
                        .filter(|entry| entry.crash && files.contains(&entry.path))
                    {
                        if let Err(error) = std::fs::remove_file(&entry.path)
                            && error.kind() != std::io::ErrorKind::NotFound
                        {
                            return Err(format!("{}: {error}", entry.path.display()));
                        }
                    }
                    Ok(())
                });
            index(ctx).lock().unwrap().paths.clear();
            ctx.request_repaint();
            if let Err(error) = result {
                self.status = error;
                self.status_error = true;
            }
        }
    }
    pub(super) fn recent_path_menu(&mut self, ui: &mut egui::Ui, path: &Path, file: bool) {
        enum RecentAction {
            Open,
            Locate,
            AssignColor,
            ResetColor,
            Remove,
            Clear,
        }
        let (directory, history, open, open_icon, locate_label, clear) = if file {
            (
                path.parent().unwrap_or(Path::new(".")),
                "recentProjects",
                "Load from this folder",
                egui::include_image!("../../assets/icons/open.svg"),
                "Locate file",
                "Clear all history",
            )
        } else {
            (
                path,
                "recentDirs",
                "New project in this folder",
                egui::include_image!("../../assets/icons/new.svg"),
                "Locate folder",
                "Clear all folder history",
            )
        };
        let assigned = self.settings.value["recentDirColors"]
            .as_array()
            .is_some_and(|colors| {
                colors
                    .iter()
                    .any(|color| color["path"].as_str() == directory.to_str())
            });
        let color = egui::include_image!("../../assets/icons/color.svg");
        let font = FontId::proportional(13.333);
        let rows: Vec<_> = [
            Some((RecentAction::Open, open, Some(open_icon), false)),
            Some((
                RecentAction::Locate,
                locate_label,
                Some(egui::include_image!("../../assets/icons/locate.svg")),
                false,
            )),
            Some((
                RecentAction::AssignColor,
                "Assign custom color",
                Some(color.clone()),
                true,
            )),
            assigned.then_some((
                RecentAction::ResetColor,
                "Reset assigned color",
                Some(color),
                false,
            )),
            Some((RecentAction::Remove, "Remove from history", None, true)),
            Some((RecentAction::Clear, clear, None, false)),
        ]
        .into_iter()
        .flatten()
        .map(|(action, label, icon, gap)| {
            let label = locale::tr(ui, label).to_uppercase();
            let galley = ui
                .painter()
                .layout_no_wrap(label, font.clone(), Color32::PLACEHOLDER);
            let left = if icon.is_some() { 50. } else { 20. };
            (action, galley, icon, gap, left)
        })
        .collect();
        let width = rows
            .iter()
            .map(|(_, text, _, _, left)| text.size().x + left + 20.)
            .fold(0., f32::max);
        ui.spacing_mut().item_spacing.y = 0.;
        for (index, (action, text, icon, gap, left)) in rows.into_iter().enumerate() {
            if gap {
                ui.add_space(8.);
            }
            let border = if index == 0 { 0. } else { 1. };
            let height = if icon.is_some() { 38. } else { 31. };
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(width, height + border), Sense::click());
            let center = rect.left_center() + egui::vec2(0., border * 0.5);
            let text_color = if response.hovered() {
                ui.painter().rect_filled(rect, 0., theme::ACCENT);
                Color32::BLACK
            } else {
                theme::ACCENT
            };
            if border > 0. {
                ui.painter().hline(
                    rect.x_range(),
                    rect.top() + 0.5,
                    Stroke::new(1_f32, theme::MEDIUM),
                );
            }
            if let Some(icon) = icon {
                egui::Image::new(icon)
                    .tint(Color32::from_white_alpha(179))
                    .paint_at(
                        ui,
                        Rect::from_center_size(center + egui::vec2(31., 0.), Vec2::splat(22.)),
                    );
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text.text())
            });
            ui.painter().galley(
                center + egui::vec2(left, -text.size().y * 0.5),
                text,
                text_color,
            );
            if !response.clicked() {
                continue;
            }
            ui.close();
            match action {
                RecentAction::Open => self.request(
                    ui.ctx(),
                    if file {
                        Action::BrowseInDir(directory.to_owned())
                    } else {
                        Action::NewInDir(directory.to_owned())
                    },
                ),
                RecentAction::Locate => {
                    if let Err(error) = locate(path, file) {
                        self.status = error.to_string();
                        self.status_error = true;
                    }
                }
                RecentAction::AssignColor => self.edit_recent_directory_color(ui.ctx(), directory),
                RecentAction::ResetColor => self.settings.force_dir_color(directory, None),
                RecentAction::Remove => self.remove_recent(history, path),
                RecentAction::Clear => {
                    self.settings.value[history] = serde_json::json!([]);
                    let _ = self.settings.save();
                }
            }
        }
    }
    pub(super) fn remove_recent(&mut self, kind: &str, path: &Path) {
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
    fn backup_menu_labels_distinguish_recent_old_and_crash_backups_in_both_languages() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut entry = ldtk_core::BackupInfo {
                    path: "World.ldtk".into(),
                    timestamp: "2024-01-06_13-15-00".into(),
                    crash: false,
                };
                let date = "Jan 6th 2024 at 13:15";
                assert_eq!(backup_labels(ui, &entry, None), (date.to_uppercase(), None));
                assert_eq!(
                    backup_labels(ui, &entry, Some(120)),
                    ("2 MINUTES AGO".into(), Some(date.into()))
                );
                entry.crash = true;
                assert_eq!(
                    backup_labels(ui, &entry, Some(120)),
                    ("CRASH RECOVERY".into(), Some(date.into()))
                );
                locale::set(ctx, locale::Language::SimplifiedChinese);
                assert_eq!(
                    backup_labels(ui, &entry, None),
                    ("崩溃恢复".into(), Some("2024年1月6日 13:15".into()))
                );
                locale::set(ctx, locale::Language::English);
            });
        });
    }

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
