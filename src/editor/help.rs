use super::*;

struct ChangelogEntry {
    version: &'static str,
    title: &'static str,
    notes: &'static str,
}
fn changelog_entries() -> &'static [ChangelogEntry] {
    static ENTRIES: std::sync::OnceLock<Vec<ChangelogEntry>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        include_str!("../../assets/CHANGELOG.md")
            .trim_start_matches("# ")
            .split("\n# ")
            .map(|section| {
                let (heading, notes) = section.split_once('\n').unwrap_or((section, ""));
                let (version, title) = heading.split_once(" - ").unwrap_or((heading, ""));
                ChangelogEntry {
                    version,
                    title,
                    notes,
                }
            })
            .collect()
    })
}
fn changelog_markdown(entry: &ChangelogEntry) -> String {
    let root = resource_path("assets/changelogImg")
        .to_string_lossy()
        .replace('\\', "/");
    entry.notes.replace("![](", &format!("![](file://{root}/"))
}
pub(super) fn changelog_is_open(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.get_temp::<bool>(egui::Id::new("release-notes-open")))
        .unwrap_or(false)
}

impl EditorApp {
    pub(super) fn help_contents(&mut self, ui: &mut egui::Ui) {
        ui.heading("LDtk help");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Release notes").clicked() {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("release-notes-open"), true));
            }
            for (label, url) in [
                ("Website documentation", "https://ldtk.io/docs"),
                ("JSON file format", "https://ldtk.io/json"),
                ("Discord", "https://ldtk.io/go/discord"),
            ] {
                ui.hyperlink_to(label, url);
            }
        });
        ui.label("Video guides");
        ui.hyperlink_to(
            "General LDtk introduction by Gamefromscratch",
            "https://youtu.be/O1pmIImzEts",
        );
        ui.hyperlink_to(
            "Unity integration by Cammin",
            "https://youtu.be/ah5MLaU5m8s",
        );
        ui.separator();
        ui.columns(2, |columns| {
            for (column, title, rows) in [
                (
                    0,
                    "Most important commands",
                    &[
                        ("Left mouse", "Draw"),
                        ("Right mouse", "Erase"),
                        ("Middle mouse / Space + left drag", "Move the view"),
                        ("Wheel / Space + right drag / + / −", "Zoom"),
                        ("Escape", "Clear selection, close a panel or cancel"),
                    ][..],
                ),
                (
                    0,
                    "General commands",
                    &[
                        ("H", "Help"),
                        ("Tab", "Zen mode"),
                        ("Cmd/Ctrl + F / K / Shift + P", "Search"),
                        (
                            if cfg!(target_os = "macos") {
                                "Shift + H"
                            } else {
                                "Ctrl + H"
                            },
                            "Show/hide details",
                        ),
                        ("F1–F10", "Select visible layers"),
                        ("F11 / Cmd/Ctrl + Enter", "Fullscreen"),
                        ("F12", "App settings"),
                        ("Cmd/Ctrl + S", "Save"),
                        ("Cmd/Ctrl + Shift + S", "Save as"),
                        ("Cmd/Ctrl + W", "Close project"),
                        ("Cmd/Ctrl + Q", "Quit"),
                        ("G", "Grid"),
                        ("Shift + A", "Single layer mode"),
                        ("Cmd/Ctrl + A", "Select all"),
                        ("Delete", "Delete selected elements"),
                        ("Cmd/Ctrl + R", "Run project command"),
                    ][..],
                ),
                (
                    0,
                    "Sections",
                    &[
                        ("P", "Project"),
                        ("L", "Layers"),
                        ("E", "Entities"),
                        ("U", "Enumerations"),
                        ("T", "Tilesets"),
                        ("C", "Current level"),
                    ][..],
                ),
                (
                    1,
                    "Editing commands",
                    &[
                        ("Alt + left mouse", "Select a single element"),
                        (
                            "Shift + Alt + left drag",
                            "Select all elements in a rectangle",
                        ),
                        ("Cmd/Ctrl + Alt + left drag", "Duplicate selected elements"),
                        ("Space", "Fit view"),
                        ("Shift + left drag", "Draw a rectangle"),
                        ("Shift + single left click", "Flood fill"),
                    ][..],
                ),
                (
                    1,
                    "Auto-layers",
                    &[("Shift + R", "Toggle auto-layer rendering")][..],
                ),
                (
                    1,
                    "Tile layers",
                    &[
                        ("Shift + S", "Save tile selection"),
                        ("Shift + L", "Recall saved tile selection"),
                        ("R", "Stamp / Random mode"),
                        ("X / Y", "Mirror selected tiles"),
                        ("Shift + T", "Toggle stacking"),
                        ("Shift + E", "Select empty cells"),
                    ][..],
                ),
                (
                    1,
                    "World mode",
                    &[
                        ("Shift + W / `", "World / level view"),
                        ("Page Up / Page Down", "Change active world depth"),
                        (
                            "Cmd/Ctrl + Page Up / Page Down",
                            "Move level to another depth",
                        ),
                        ("Double click a level", "Enter level"),
                    ][..],
                ),
            ] {
                let ui = &mut columns[column];
                ui.add_space(12.0);
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(
                            18.0,
                            egui::FontFamily::Name("panel-title".into()),
                        ))
                        .color(theme::ACCENT),
                );
                egui::Grid::new(title)
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for (key, description) in rows {
                            ui.label(RichText::new(*key).strong());
                            ui.label(*description);
                            ui.end_row();
                        }
                    });
            }
        });
    }
    pub(super) fn changelog_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("release-notes-open");
        let mut open = ctx.data_mut(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !open {
            return;
        }
        let entries = changelog_entries();
        let mut version = ctx
            .data_mut(|d| d.get_temp::<String>(id.with("version")))
            .unwrap_or_else(|| {
                entries
                    .iter()
                    .find(|e| e.version.ends_with(".0"))
                    .unwrap_or(&entries[0])
                    .version
                    .into()
            });
        let modal=egui::Modal::new(id).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width()*0.8).min(980.));
                egui::ComboBox::from_id_salt("changelog-version").selected_text(&version).show_ui(ui,|ui| {
                    for entry in entries {ui.selectable_value(&mut version,entry.version.into(),entry.version);}
                });
                let entry=entries.iter().find(|e|e.version==version).unwrap_or(&entries[0]);
                ui.heading(if entry.title.is_empty() {format!("LDtk {}",entry.version)}else{format!("LDtk {} — {}",entry.version,entry.title)});
                let cache=ctx.data_mut(|d|d.get_temp_mut_or_default::<std::sync::Arc<std::sync::Mutex<egui_commonmark::CommonMarkCache>>>(id.with("cache")).clone());
                let markdown=changelog_markdown(entry);
                egui::ScrollArea::vertical()
                    .id_salt(("changelog-body",entry.version))
                    .max_height(ctx.content_rect().height()*0.7)
                    .show(ui, |ui| {
                        egui_commonmark::CommonMarkViewer::new()
                            .max_image_width(Some(ui.available_width() as usize))
                            .show(ui,&mut cache.lock().unwrap(),&markdown);
                    });
                if panels::source_controls::button(ui,"Close").clicked() {open=false;}
            });
        if modal.should_close() {
            open = false;
        }
        ctx.data_mut(|d| d.insert_temp(id, open));
        ctx.data_mut(|d| d.insert_temp(id.with("version"), version));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_release_entries_preserve_versions_text_and_bundled_images() {
        let entries = changelog_entries();
        assert_eq!(entries[0].version, "1.5.3");
        let major = entries.iter().find(|e| e.version == "1.5.0").unwrap();
        assert_eq!(major.title, "New LDtk logo");
        let markdown = changelog_markdown(major);
        assert!(markdown.contains("file://"));
        for image in major
            .notes
            .split("![](")
            .skip(1)
            .filter_map(|s| s.split_once(')').map(|s| s.0))
        {
            assert!(
                resource_path(&format!("assets/changelogImg/{image}")).is_file(),
                "{image}"
            );
        }
    }
}
