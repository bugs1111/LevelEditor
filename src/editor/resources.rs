use super::*;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn clean(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
fn document_paths(project: &Project) -> HashSet<PathBuf> {
    let base = project
        .path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or(Path::new("."));
    project
        .path
        .iter()
        .cloned()
        .chain(project.external_levels.keys().map(|rel| base.join(rel)))
        .map(|path| clean(&path))
        .collect()
}
pub(super) fn is_missing_file(path: &Path) -> bool {
    std::fs::exists(path).is_ok_and(|exists| !exists)
}
fn atlas_paths(project: &Project) -> Vec<(PathBuf, i64)> {
    project.json["defs"]["tilesets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|def| {
            Some((
                clean(&canvas::asset_path(project, def["relPath"].as_str()?)),
                def["uid"].as_i64()?,
            ))
        })
        .collect()
}
fn background_paths(project: &Project) -> impl Iterator<Item = PathBuf> + '_ {
    project
        .level_keys()
        .into_iter()
        .filter_map(|key| project.level(key)?.get("bgRelPath")?.as_str())
        .map(|relative| clean(&canvas::asset_path(project, relative)))
}

fn enum_paths(project: &Project) -> HashSet<PathBuf> {
    project.json["defs"]["externalEnums"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|def| def["externalRelPath"].as_str())
        .map(|rel| clean(&canvas::asset_path(project, rel)))
        .collect()
}
fn replace_resource_paths(
    value: &mut Value,
    keys: &[&str],
    base: &Path,
    old: &Path,
    new: &str,
) -> usize {
    match value {
        Value::Object(object) => object
            .iter_mut()
            .map(|(key, value)| {
                if keys.contains(&key.as_str())
                    && value
                        .as_str()
                        .is_some_and(|rel| clean(&base.join(rel)) == old)
                {
                    *value = json!(new);
                    1
                } else {
                    replace_resource_paths(value, keys, base, old, new)
                }
            })
            .sum(),
        Value::Array(array) => array
            .iter_mut()
            .map(|value| replace_resource_paths(value, keys, base, old, new))
            .sum(),
        _ => 0,
    }
}

pub(super) fn atlas_pixel_data(
    assets: &mut Assets,
    path: &Path,
    size: [usize; 2],
    grid: i64,
    padding: i64,
    spacing: i64,
) -> Option<Value> {
    let stride = grid + spacing;
    if grid <= 0 || stride <= 0 || padding < 0 {
        return None;
    }
    let width = size[0] as i64 - padding * 2;
    let height = size[1] as i64 - padding * 2;
    if width <= 0 || height <= 0 {
        return None;
    }
    let cols = (width + stride - 1) / stride;
    let rows = (height + stride - 1) / stride;
    let mut opaque = String::with_capacity((cols * rows) as usize);
    let mut colors = String::with_capacity((cols * rows * 4) as usize);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for y in 0..rows {
        for x in 0..cols {
            let src = [padding + x * stride, padding + y * stride];
            // Original _parseTilePixels leaves partial edge cells opaque/black.
            let (solid, rgba) = if src[0] + grid > size[0] as i64 || src[1] + grid > size[1] as i64
            {
                (true, [0; 4])
            } else {
                let src = [src[0] as i32, src[1] as i32];
                let area = [grid as i32; 2];
                (
                    assets.tile_is_opaque(path, src, area)?,
                    assets.average_tile_color(path, src, area)?,
                )
            };
            opaque.push(if solid { '1' } else { '0' });
            for channel in [rgba[3], rgba[0], rgba[1], rgba[2]] {
                // dn.legacy.Color.intToHex3_ARGB rounds normalized channels to 15.
                colors.push(HEX[(channel as f64 / 255.0 * 15.0).round() as usize] as char);
            }
        }
    }
    Some(json!({"opaqueTiles":opaque,"averageColors":colors}))
}

pub(super) fn trimmed_atlas_padding(old: &Value, width: i64, height: i64, padding: i64) -> i64 {
    let old_w = number(old, "pxWid", 0.0) as i64;
    let old_h = number(old, "pxHei", 0.0) as i64;
    if padding > 0
        && old_w > width
        && old_h > height
        && old_w - width <= padding * 2
        && old_h - height <= padding * 2
        && width % 2 == 0
        && height % 2 == 0
    {
        padding - (old_w - width).min(old_h - height) / 2
    } else {
        padding
    }
}

impl EditorApp {
    pub(super) fn changed_document_files(&self) -> Vec<PathBuf> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let root_missing = project.path.as_deref().is_some_and(is_missing_file);
        let Some(snapshot) = &self.file_snapshot else {
            return Vec::new();
        };
        let changed = match snapshot.changed_files_matching(|p| snapshot.is_document(p)) {
            Ok(paths) => paths,
            // A document we cannot verify must also be resolved before saving.
            Err(ldtk_core::Error::Io { path, .. }) => vec![path],
            Err(_) => return Vec::new(),
        };
        changed
            .into_iter()
            .filter(|path| !root_missing || !is_missing_file(path))
            .collect()
    }
    pub(super) fn changed_resource_files(&self) -> Vec<PathBuf> {
        self.file_snapshot
            .as_ref()
            .map(ldtk_core::FilesystemSnapshot::changed_resource_files)
            .unwrap_or_default()
    }
    fn resource_failure(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        uids: Vec<i64>,
        error: String,
    ) {
        self.status = locale::format_ctx(
            ctx,
            "Could not reload {path}: {error}",
            &[("path", &path.to_string_lossy()), ("error", &error)],
        );
        self.status_error = true;
        // Original backup inspection does not offer to relink missing files.
        if !path.exists() && self.project.as_ref().is_some_and(Project::is_backup) {
            return;
        }
        if let Some(entry) = self
            .resource_missing
            .iter_mut()
            .find(|entry| entry.0 == path)
        {
            for uid in uids {
                if !entry.1.contains(&uid) {
                    entry.1.push(uid);
                }
            }
            entry.2 = error;
        } else {
            self.resource_missing.push((path, uids, error));
        }
    }
    pub(super) fn retry_resource(&mut self, ctx: &egui::Context, path: &Path) {
        self.ignore_resource(path);
        self.changed_resources(ctx, vec![clean(path)]);
    }
    pub(super) fn ignore_resource(&mut self, path: &Path) {
        let path = clean(path);
        self.resource_missing
            .retain(|entry| clean(&entry.0) != path);
    }
    pub(super) fn relocate_resource(
        &mut self,
        ctx: &egui::Context,
        old: &Path,
        new: &Path,
    ) -> bool {
        let old = clean(old);
        let new = clean(new);
        if !new.is_file() {
            self.status = locale::format_ctx(
                ctx,
                "The selected file does not exist: {path}",
                &[("path", &new.to_string_lossy())],
            );
            self.status_error = true;
            return false;
        }
        if old == new {
            self.retry_resource(ctx, &old);
            return true;
        }
        let result = self.edit_recoverable("Relocated a lost file", |project| {
            let relative = if project.path.is_some() {
                project.make_relative_file_path(&new)?
            } else {
                new.to_string_lossy().into_owned()
            };
            let base = project
                .path
                .as_deref()
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .to_owned();
            let mut count = replace_resource_paths(
                &mut project.json["defs"]["tilesets"],
                &["relPath"],
                &base,
                &old,
                &relative,
            );
            count += replace_resource_paths(
                &mut project.json["defs"]["externalEnums"],
                &["externalRelPath"],
                &base,
                &old,
                &relative,
            );
            count += replace_resource_paths(
                &mut project.json,
                &["bgRelPath", "__tilesetRelPath"],
                &base,
                &old,
                &relative,
            );
            for level in project.external_levels.values_mut() {
                count += replace_resource_paths(
                    level,
                    &["bgRelPath", "__tilesetRelPath"],
                    &base,
                    &old,
                    &relative,
                );
            }
            if count == 0 {
                return Err(ldtk_core::Error::Invalid(
                    "this resource is no longer used by the project".into(),
                ));
            }
            Ok(())
        });
        if result.is_none() {
            return false;
        }
        self.ignore_resource(&old);
        self.resource_checks.remove(&old);
        self.assets.invalidate(&old);
        self.changed_resources(ctx, vec![new]);
        true
    }
    pub(super) fn discard_unused_images(&mut self, project: &Project) {
        let referenced = atlas_paths(project)
            .into_iter()
            .map(|(path, _)| path)
            .chain(background_paths(project))
            .collect::<HashSet<_>>();
        self.assets
            .discard_unused_file_images(|path| referenced.contains(&clean(path)));
    }

    pub(super) fn resource_scan(&mut self, ctx: &egui::Context) {
        let Some(project) = &self.project else {
            return;
        };
        let atlases = atlas_paths(project);
        let mut enums = enum_paths(project).into_iter().collect::<Vec<_>>();
        enums.sort();
        let enum_checks = enums
            .into_iter()
            .filter(|_| !project.is_backup())
            .map(|path| {
                let changed = project.external_enum_source_changed(&path);
                (path, changed)
            })
            .collect::<Vec<_>>();
        let backgrounds = background_paths(project).collect::<Vec<_>>();
        for (path, uid) in atlases {
            let uids = self.resource_checks.entry(path).or_default();
            if !uids.contains(&uid) {
                uids.push(uid);
            }
        }
        for path in backgrounds {
            self.resource_checks.entry(path).or_default();
        }
        for (path, changed) in enum_checks {
            match changed {
                Ok(true) => self.external_enum_reloads.push_back(path),
                Ok(false) => (),
                Err(error) => self.resource_failure(ctx, path, Vec::new(), error.to_string()),
            }
        }
        self.pump_resources(ctx);
    }
    pub(super) fn changed_resources(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        let Some(project) = &self.project else {
            return;
        };
        let documents = document_paths(project);
        let enums = enum_paths(project);
        let atlases = atlas_paths(project);
        let backgrounds = background_paths(project).collect::<HashSet<_>>();
        let backup = project.is_backup();
        let root_missing = project.path.as_deref().is_some_and(is_missing_file);
        let mut conflicts = self.changed_document_files();
        let paths: BTreeSet<_> = paths.into_iter().map(|path| clean(&path)).collect();
        if let Some(snapshot) = &mut self.file_snapshot {
            project.refresh_resource_watches(snapshot, &paths);
        }
        let mut enum_files = HashSet::new();
        for path in paths {
            if documents.contains(&path)
                || self
                    .file_snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.is_document(&path))
            {
                if (!root_missing || !is_missing_file(&path)) && !conflicts.contains(&path) {
                    conflicts.push(path);
                }
                continue;
            }
            if enums.contains(&path) {
                if !backup && !self.external_enum_reloads.contains(&path) {
                    enum_files.insert(path);
                }
                continue;
            }
            self.assets.invalidate(&path);
            let uids = atlases
                .iter()
                .filter_map(|(p, id)| (*p == path).then_some(*id))
                .collect::<Vec<_>>();
            if !uids.is_empty()
                || backgrounds.contains(&path)
                || matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("png" | "jpg" | "jpeg" | "gif" | "ase" | "aseprite")
                )
            {
                self.resource_checks.insert(path, uids);
            }
        }
        for path in enum_files {
            if self.begin_enum_sync(ctx, path.clone()) {
                self.ignore_resource(&path);
            } else {
                self.resource_failure(ctx, path, Vec::new(), self.status.clone());
            }
        }
        // Keep the files shown in the open dialog stable until the user responds.
        if self.external_changes.is_none() && !conflicts.is_empty() {
            self.external_changes = Some(conflicts);
        }
        self.pump_resources(ctx);
    }
    pub(super) fn pump_resources(&mut self, ctx: &egui::Context) {
        if self.is_busy() || self.stroke || self.gesture.is_some() || self.level_resize.is_some() {
            return;
        }
        for path in self.resource_checks.keys() {
            self.assets.texture(ctx, path);
        }
        let images = self.assets.take_loaded_images();
        for (path, size) in images {
            let Some(uids) = self.resource_checks.remove(&clean(&path)) else {
                continue;
            };
            self.ignore_resource(&path);
            for uid in uids {
                let Some(def) = self
                    .project
                    .as_ref()
                    .and_then(|p| {
                        p.tileset(uid).filter(|def| {
                            def["relPath"].as_str().is_some_and(|relative| {
                                clean(&canvas::asset_path(p, relative)) == clean(&path)
                            })
                        })
                    })
                    .cloned()
                else {
                    continue;
                };
                let geometry_changed = def["pxWid"].as_u64() != Some(size[0] as u64)
                    || def["pxHei"].as_u64() != Some(size[1] as u64);
                let width = size[0] as i64;
                let height = size[1] as i64;
                let max = width.min(height).max(1);
                let old_padding = (number(&def, "padding", 0.0) as i64).min(max);
                let grid = (number(&def, "tileGridSize", 16.0) as i64).min(max);
                let spacing = (number(&def, "spacing", 0.0) as i64).min(max);
                let padding = trimmed_atlas_padding(&def, width, height, old_padding);
                let pixels =
                    atlas_pixel_data(&mut self.assets, &path, size, grid, padding, spacing)
                        .unwrap_or(Value::Null);
                if !geometry_changed && def["cachedPixelData"] == pixels {
                    continue;
                }
                if geometry_changed {
                    // The core remapper applies border trimming once. Passing the
                    // already trimmed value would trim that border a second time.
                    let patch = json!({"pxWid":width,"pxHei":height,"tileGridSize":grid,"padding":old_padding,"spacing":spacing,"cachedPixelData":pixels});
                    self.edit("Atlas changed; tile coordinates remapped", |p| {
                        p.update_tileset_definition_with_policy(
                            uid,
                            &patch,
                            ldtk_core::TilesetRemap::AtlasCoordinates,
                            true,
                        )
                    });
                } else {
                    let opacity_changed =
                        def["cachedPixelData"]["opaqueTiles"] != pixels["opaqueTiles"];
                    self.edit("Atlas pixel data updated", |p| {
                        let def = p.json["defs"]["tilesets"]
                            .as_array_mut()
                            .and_then(|defs| defs.iter_mut().find(|def| def["uid"] == uid))
                            .ok_or_else(|| {
                                ldtk_core::Error::Invalid("tileset no longer exists".into())
                            })?;
                        def["cachedPixelData"] = pixels;
                        if !opacity_changed {
                            return Ok(());
                        }
                        let levels = p
                            .level_keys()
                            .into_iter()
                            .filter(|key| {
                                p.level(*key)
                                    .and_then(|level| level["layerInstances"].as_array())
                                    .is_some_and(|layers| {
                                        layers.iter().any(|layer| {
                                            layer["__tilesetDefUid"] == uid
                                                && matches!(
                                                    layer["__type"].as_str(),
                                                    Some("IntGrid" | "AutoLayer")
                                                )
                                        })
                                    })
                            })
                            .collect::<Vec<_>>();
                        for key in levels {
                            p.rebuild_auto_layers(key)?;
                        }
                        Ok(())
                    });
                }
            }
        }
        let failed = self
            .resource_checks
            .keys()
            .filter_map(|p| {
                self.assets
                    .errors
                    .get(p)
                    .map(|error| (p.clone(), error.clone()))
            })
            .collect::<Vec<_>>();
        for (path, error) in failed {
            let uids = self.resource_checks.remove(&path).unwrap_or_default();
            self.resource_failure(ctx, path, uids, error);
        }
        if !self.resource_checks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
    }
    pub(super) fn resource_dialog(&mut self, ctx: &egui::Context) {
        let Some((path, _, _)) = self.resource_missing.first() else {
            return;
        };
        let path = path.clone();
        let id = egui::Id::new(("resource-relocation", &path));
        if let Some(new) = ctx.data_mut(|data| data.get_temp::<PathBuf>(id)) {
            let mut apply = false;
            let mut cancel = false;
            let modal = egui::Modal::new(id).show(ctx, |ui| {
                ui.set_width(560.0);
                ui.heading(locale::tr(ui, "The selected file has a different name"));
                ui.label(locale::tr(
                    ui,
                    "Selecting a different file may change your project. Is this the correct file?",
                ));
                ui.monospace(new.display().to_string());
                ui.horizontal(|ui| {
                    apply = ui.button(locale::tr(ui, "Use selected file")).clicked();
                    cancel = ui.button(locale::tr(ui, "Cancel")).clicked();
                });
            });
            if apply || cancel || modal.should_close() {
                ctx.data_mut(|data| data.remove::<PathBuf>(id));
            }
            if apply {
                self.relocate_resource(ctx, &path, &new);
            }
            return;
        }
        let missing = !path.exists();
        let relative = self
            .project
            .as_ref()
            .and_then(|project| project.make_relative_file_path(&path).ok())
            .unwrap_or_else(|| path.display().to_string());
        let title = if missing { "FILE NOT FOUND!" } else { "ERROR" };
        let message = if missing {
            locale::tr_ctx(ctx, "The following file cannot be found anymore:").to_owned()
        } else {
            let filename = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            locale::format_ctx(
                ctx,
                "Couldn't read file: {filename}",
                &[("filename", &filename)],
            )
        };
        let (action, dismiss) = if missing {
            ("LOCATE THE FILE", "FIX THAT LATER")
        } else {
            ("RETRY", "IGNORE")
        };
        let modal = egui::Modal::new(egui::Id::new("resource-load-failure"))
            .frame(project_dialogs::dialog_frame(0))
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                let body = |text| RichText::new(text).size(14.667).color(Color32::WHITE);
                let measured = ui.fonts_mut(|fonts| {
                    let mut measure = |text: &str| {
                        fonts
                            .layout_no_wrap(
                                text.to_owned(),
                                FontId::proportional(14.667),
                                Color32::WHITE,
                            )
                            .size()
                            .x
                    };
                    let width = measure(&message);
                    if missing {
                        width.max(measure(&relative) + 10.)
                    } else {
                        width
                    }
                });
                let min_width = if missing { 324. } else { 216. };
                let max_width = (ui.ctx().content_rect().width() * 0.5).max(min_width);
                let width = measured.clamp(min_width, max_width);
                ui.set_width(width + 40.);
                ui.spacing_mut().item_spacing = egui::vec2(2., 0.);
                project_dialogs::dialog_title(ui, title, width + 40., theme::LIGHT);
                egui::Frame::new()
                    .inner_margin(20)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(body(message.as_str()));
                        if missing {
                            ui.add_space(14.);
                            egui::Frame::new()
                                .fill(theme::DARK)
                                .stroke(Stroke::new(1_f32, Color32::from_rgb(227, 77, 0)))
                                .inner_margin(4)
                                .show(ui, |ui| {
                                    ui.set_width(width - 10.);
                                    ui.label(body(relative.as_str()).strong());
                                });
                            ui.add_space(14.);
                            ui.label(body(locale::tr(ui, "What do you want to do?")));
                        }
                        ui.add_space(16.);
                        ui.horizontal(|ui| {
                            let size = egui::vec2((width - 2.) * 0.5, 35.);
                            let enabled =
                                !missing || !self.project.as_ref().is_some_and(Project::is_backup);
                            let primary = ui
                                .add_enabled_ui(enabled, |ui| {
                                    project_dialogs::button(ui, action, size, false, 14.667)
                                })
                                .inner;
                            let secondary =
                                project_dialogs::button(ui, dismiss, size, missing, 14.667);
                            (primary.clicked(), secondary.clicked())
                        })
                        .inner
                    })
                    .inner
            });
        if modal.inner.1 || modal.should_close() {
            self.ignore_resource(&path);
        } else if modal.inner.0 && !missing {
            self.retry_resource(ctx, &path);
        } else if modal.inner.0 {
            let mut dialog =
                FileDialog::new().set_title(locale::tr_ctx(ctx, "Locate the missing resource"));
            if let Some(parent) = path
                .parent()
                .filter(|p| p.is_dir())
                .or_else(|| self.project.as_ref()?.path.as_deref()?.parent())
            {
                dialog = dialog.set_directory(parent);
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                dialog = dialog.add_filter(locale::tr_ctx(ctx, "Original file type"), &[ext]);
            }
            if let Some(new) = dialog.pick_file() {
                if new.file_stem() != path.file_stem() {
                    ctx.data_mut(|data| data.insert_temp(id, new));
                } else {
                    self.relocate_resource(ctx, &path, &new);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_enum_addition_waits_for_reload_when_opening_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Values.json");
        std::fs::write(&path, r#"{"State":["Idle"]}"#).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        project.sync_external_enums(&path).unwrap();
        let before = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.resource_scan(&ctx);
        assert!(!app.enum_sync_is_open());
        std::fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
        app.resource_scan(&ctx);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.dirty);
        assert!(app.enum_sync_is_open());
    }

    #[test]
    fn live_enum_rename_waits_for_confirmation_without_reporting_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Values.json");
        std::fs::write(&path, r#"{"State":["Idle"]}"#).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        project.sync_external_enums(&path).unwrap();
        let field = project
            .add_field_definition(None, "Status", "ExternEnum.State", false)
            .unwrap();
        project
            .set_field_editor_value(project.level_keys()[0], None, field, json!("Idle"))
            .unwrap();
        let before = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let status = app.status.clone();
        let ctx = egui::Context::default();
        std::fs::write(&path, r#"{"State":["Run"]}"#).unwrap();
        app.changed_resources(&ctx, vec![path]);
        assert!(app.enum_sync_is_open());
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.dirty);
        assert_eq!(app.status, status);
    }

    #[test]
    fn external_enum_json_updates_enums_without_a_project_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Values.json");
        std::fs::write(&path, r#"{"State":["Idle"]}"#).unwrap();
        let mut p = Project::new();
        p.path = Some(dir.path().join("Map.ldtk"));
        p.sync_external_enums(&path).unwrap();
        let target = p.path.clone().unwrap();
        p.save(target).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        std::fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
        app.last_file_check = Instant::now() - std::time::Duration::from_secs(3);
        app.poll_services(&egui::Context::default());
        assert!(
            app.external_changes.is_none(),
            "An enum JSON file is not a project document"
        );
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["externalEnums"][0]["values"][1]["id"],
            "Run"
        );
    }
    #[test]
    fn resized_external_atlas_updates_geometry_and_remaps_existing_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Atlas.png");
        image::RgbaImage::new(32, 32).save(&path).unwrap();
        let mut p = Project::new();
        let uid = p.add_tileset(&path, 16, 32, 32).unwrap();
        let layer = p.add_layer("Tiles", "Tiles", 16).unwrap();
        p.assign_tileset(layer, uid).unwrap();
        let key = p.level_keys()[0];
        p.paint_tile(key, 0, 0, 0, 2, [0, 16], 0, false).unwrap();
        p.save(dir.path().join("Map.ldtk")).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        image::RgbaImage::new(64, 32).save(&path).unwrap();
        app.last_file_check = Instant::now() - std::time::Duration::from_secs(3);
        let ctx = egui::Context::default();
        let until = Instant::now() + std::time::Duration::from_secs(2);
        while Instant::now() < until {
            app.assets.poll(&ctx);
            app.poll_services(&ctx);
            if app.project.as_ref().unwrap().tileset(uid).unwrap()["pxWid"] == 64 {
                break;
            }
            std::thread::yield_now();
        }
        let p = app.project.as_ref().unwrap();
        assert_eq!(p.tileset(uid).unwrap()["pxWid"], 64);
        assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["t"], 4);
        assert!(app.history.can_undo());
    }
    #[test]
    fn resource_edits_during_a_save_are_replayed_before_accepting_the_new_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let values = dir.path().join("Values.json");
        std::fs::write(&values, r#"{"State":["Idle"]}"#).unwrap();
        let mut p = Project::new();
        p.path = Some(dir.path().join("Map.ldtk"));
        p.sync_external_enums(&values).unwrap();
        let file = p.path.clone().unwrap();
        p.save(&file).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p.clone());
        let report = p.save_with_exports(&file).unwrap();
        let (tx, rx) = mpsc::channel();
        app.saving = Some(rx);
        std::fs::write(&values, r#"{"State":["Idle","Run"]}"#).unwrap();
        tx.send(Ok(services::SavedProject { project: p, report }))
            .unwrap();
        app.poll_services(&egui::Context::default());
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["externalEnums"][0]["values"][1]["id"],
            "Run"
        );
        assert!(
            app.dirty,
            "The save preceded the enum edit; the updated definitions still need saving"
        );
    }
}

#[cfg(test)]
mod missing_tests {
    use super::*;
    fn pump_until(app: &mut EditorApp, ctx: &egui::Context, ready: impl Fn(&EditorApp) -> bool) {
        let until = Instant::now() + std::time::Duration::from_secs(2);
        while Instant::now() < until {
            app.assets.poll(ctx);
            app.pump_resources(ctx);
            if ready(app) {
                return;
            }
            std::thread::yield_now();
        }
        panic!("resource decode did not reach expected state");
    }

    fn dialog_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 700.),
            )),
            focused: true,
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest());
        })
    }

    fn visible_text(output: &egui::FullOutput, text: &str) -> bool {
        output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Text(shape) if shape.galley.text() == text),
        )
    }

    #[test]
    fn chinese_resource_prompt_and_relocation_keep_original_file_names() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("Original Atlas.png");
        let replacement = directory.path().join("Moved Atlas.png");
        image::RgbaImage::new(32, 32).save(&replacement).unwrap();
        let mut project = Project::new();
        project.path = Some(directory.path().join("Map.ldtk"));
        let uid = project.add_tileset(&old, 16, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        app.settings.value["language"] = json!("zh-CN");
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        dialog_frame(&mut app, &ctx, vec![]);
        app.changed_resources(&ctx, vec![old.clone()]);
        pump_until(&mut app, &ctx, |app| !app.resource_missing.is_empty());
        dialog_frame(&mut app, &ctx, vec![]);
        let missing = dialog_frame(&mut app, &ctx, vec![]);
        for text in [
            "找不到文件",
            "找不到以下文件",
            "Original Atlas.png",
            "定位文件",
            "稍后处理",
        ] {
            assert!(visible_text(&missing, text), "missing label {text}");
        }

        let id = egui::Id::new(("resource-relocation", &old));
        ctx.data_mut(|data| data.insert_temp(id, replacement.clone()));
        dialog_frame(&mut app, &ctx, vec![]);
        let confirm = dialog_frame(&mut app, &ctx, vec![]);
        for text in [
            "所选文件的名称不同",
            "选择不同的文件可能会更改工程。确定使用这个文件吗？",
            "取消",
        ] {
            assert!(visible_text(&confirm, text), "missing label {text}");
        }
        assert!(visible_text(&confirm, &replacement.to_string_lossy()));
        let pos = confirm
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "使用所选文件" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            dialog_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(app.resource_missing.is_empty() && app.dirty);
        assert_eq!(
            app.project.as_ref().unwrap().tileset(uid).unwrap()["relPath"],
            "Moved Atlas.png"
        );
    }

    #[test]
    fn chinese_unreadable_resource_prompt_shows_the_file_and_localized_actions() {
        let directory = tempfile::tempdir().unwrap();
        let atlas = directory.path().join("Broken Atlas.png");
        std::fs::write(&atlas, b"invalid PNG").unwrap();
        let mut project = Project::new();
        project.path = Some(directory.path().join("Map.ldtk"));
        project.add_tileset(&atlas, 16, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        app.settings.value["language"] = json!("zh-CN");
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        dialog_frame(&mut app, &ctx, vec![]);
        app.changed_resources(&ctx, vec![atlas.clone()]);
        pump_until(&mut app, &ctx, |app| !app.resource_missing.is_empty());
        dialog_frame(&mut app, &ctx, vec![]);
        let output = dialog_frame(&mut app, &ctx, vec![]);
        for text in ["错误", "无法读取文件「Broken Atlas.png」", "重试", "忽略"] {
            assert!(visible_text(&output, text), "missing label {text}");
        }
        assert!(
            app.status
                .starts_with(&format!("无法重新加载 {}", atlas.display()))
        );
        assert!(app.status.contains(&app.resource_missing[0].2));
    }

    #[test]
    fn resource_prompt_allows_normal_save_without_dismissing_the_error() {
        for corrupt in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("Map.ldtk");
            let atlas = directory.path().join("Atlas.png");
            if corrupt {
                std::fs::write(&atlas, b"Invalid PNG bytes").unwrap();
            }
            let mut project = Project::new();
            project.path = Some(path.clone());
            let level = project.level_keys()[0];
            project.resize_level(level, 32, 16, 0, 0).unwrap();
            let uid = project.add_tileset(&atlas, 16, 32, 16).unwrap();
            project.add_layer("Ground", "Tiles", 16).unwrap();
            project
                .set_layer_tileset_override(
                    level,
                    0,
                    Some(uid),
                    ldtk_core::TilesetRemap::TileIds,
                    false,
                )
                .unwrap();
            project
                .paint_tile(level, 0, 1, 0, 1, [16, 0], 0, false)
                .unwrap();
            project.json["imageExportMode"] = json!("OneImagePerLayer");
            project.json["exportLevelBg"] = json!(false);
            project.save(&path).unwrap();
            let mut app = EditorApp::empty();
            app.set_project(project);
            app.project.as_mut().unwrap().json["doc"] = json!("Saved while showing the error");
            app.dirty = true;
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            app.repaint = Some(ctx.clone());
            app.changed_resources(&ctx, vec![atlas.clone()]);
            pump_until(&mut app, &ctx, |a| !a.resource_missing.is_empty());
            dialog_frame(&mut app, &ctx, vec![]);

            let save = |alt, pressed| egui::Event::Key {
                key: Key::S,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers {
                    alt,
                    command: true,
                    ctrl: !cfg!(target_os = "macos"),
                    mac_cmd: cfg!(target_os = "macos"),
                    ..Default::default()
                },
            };
            dialog_frame(&mut app, &ctx, vec![save(true, true), save(true, false)]);
            assert!(!app.is_busy());
            dialog_frame(&mut app, &ctx, vec![save(false, true), save(false, false)]);
            assert!(
                app.is_busy(),
                "Ctrl/Cmd+S should reach the normal save workflow"
            );
            let until = Instant::now() + std::time::Duration::from_secs(5);
            while app.is_busy() && Instant::now() < until {
                dialog_frame(&mut app, &ctx, vec![]);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(!app.is_busy());
            assert!(!app.dirty);
            assert_eq!(app.resource_missing.len(), 1);
            assert_eq!(app.resource_missing[0].0, atlas);
            let saved = Project::load(&path).unwrap();
            assert_eq!(saved.json["doc"], "Saved while showing the error");
            assert_eq!(saved.tileset(uid).unwrap()["relPath"], "Atlas.png");
            assert_eq!(saved.layer(level, 0).unwrap()["gridTiles"][0]["t"], 1);
            let files = std::fs::read_dir(directory.path().join("Map/png")).unwrap();
            assert!(files.filter_map(Result::ok).any(|file| {
                let image = image::open(file.path()).unwrap().to_rgba8();
                image.get_pixel(16, 0).0 == [136, 0, 0, 255]
            }));
        }
    }

    #[test]
    fn resource_prompt_uses_source_actions_and_dismisses_without_an_edit() {
        for corrupt in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let atlas = directory.path().join("Atlas.png");
            if corrupt {
                std::fs::write(&atlas, b"Invalid PNG bytes").unwrap();
            }
            let mut project = Project::new();
            project.path = Some(directory.path().join("Map.ldtk"));
            let uid = project.add_tileset(&atlas, 16, 32, 32).unwrap();
            let mut app = EditorApp::empty();
            app.set_project(project);
            let before = app.project.as_ref().unwrap().json.clone();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            app.repaint = Some(ctx.clone());
            app.changed_resources(&ctx, vec![atlas]);
            pump_until(&mut app, &ctx, |a| !a.resource_missing.is_empty());
            dialog_frame(&mut app, &ctx, vec![]);
            let output = dialog_frame(&mut app, &ctx, vec![]);
            assert!(visible_text(
                &output,
                if corrupt { "ERROR" } else { "FILE NOT FOUND!" }
            ));
            assert_eq!(visible_text(&output, "RETRY"), corrupt);
            assert_eq!(visible_text(&output, "IGNORE"), corrupt);
            assert_eq!(visible_text(&output, "LOCATE THE FILE"), !corrupt);
            assert_eq!(visible_text(&output, "FIX THAT LATER"), !corrupt);

            let escape = egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            };
            dialog_frame(&mut app, &ctx, vec![escape]);
            assert!(app.resource_missing.is_empty());
            assert_eq!(app.project.as_ref().unwrap().json, before);
            assert!(!app.dirty);
            assert_eq!(
                app.project.as_ref().unwrap().tileset(uid).unwrap()["relPath"],
                "Atlas.png"
            );
        }
    }

    #[test]
    fn missing_atlas_is_queued_once_and_retry_loads_repaired_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Missing.png");
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&path, 16, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![path.clone(), path.clone()]);
        pump_until(&mut app, &ctx, |a| !a.resource_missing.is_empty());
        assert_eq!(app.resource_missing.len(), 1);
        assert_eq!(app.resource_missing[0].0, path);
        assert_eq!(app.resource_missing[0].1, vec![uid]);
        assert!(!app.resource_missing[0].2.is_empty());
        image::RgbaImage::new(64, 32).save(&path).unwrap();
        app.retry_resource(&ctx, &path);
        pump_until(&mut app, &ctx, |a| {
            a.resource_checks.is_empty() && a.assets.errors.is_empty()
        });
        assert!(app.resource_missing.is_empty());
        assert_eq!(
            app.project.as_ref().unwrap().tileset(uid).unwrap()["pxWid"],
            64
        );
    }

    #[test]
    fn a_decode_for_the_previous_atlas_image_cannot_modify_its_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Old.png");
        image::RgbaImage::new(32, 32).save(&old).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&old, 16, 32, 32).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.resource_checks.insert(old.clone(), vec![uid]);
        app.assets.texture(&ctx, &old);
        let p = app.project.as_mut().unwrap();
        p.update_tileset_definition(uid, &json!({"relPath":"New.png","pxWid":64,"pxHei":64}))
            .unwrap();
        pump_until(&mut app, &ctx, |app| app.resource_checks.is_empty());
        let def = app.project.as_ref().unwrap().tileset(uid).unwrap();
        assert_eq!(def["relPath"], "New.png");
        assert_eq!(def["pxWid"], 64);
        assert!(def["cachedPixelData"].is_null());
    }

    #[test]
    fn locate_relinks_every_matching_atlas_background_and_instance_path_undoably() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Old.png");
        let new = dir.path().join("Moved.png");
        image::RgbaImage::new(32, 32).save(&new).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let a = project.add_tileset(&old, 16, 32, 32).unwrap();
        let b = project.add_tileset(&old, 16, 32, 32).unwrap();
        let layer = project.add_layer("Tiles", "Tiles", 16).unwrap();
        project.assign_tileset(layer, a).unwrap();
        let key = project.level_keys()[0];
        project.level_mut(key).unwrap()["bgRelPath"] = json!("Old.png");
        project.layer_mut(key, 0).unwrap()["__tilesetRelPath"] = json!("Old.png");
        let before = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.resource_missing
            .push((old.clone(), vec![a, b], "missing".into()));
        assert!(app.relocate_resource(&egui::Context::default(), &old, &new));
        let project = app.project.as_ref().unwrap();
        for uid in [a, b] {
            assert_eq!(project.tileset(uid).unwrap()["relPath"], "Moved.png");
        }
        assert_eq!(project.level(key).unwrap()["bgRelPath"], "Moved.png");
        assert_eq!(
            project.layer(key, 0).unwrap()["__tilesetRelPath"],
            "Moved.png"
        );
        assert!(app.resource_missing.is_empty() && app.dirty);
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!old.exists());
    }

    #[test]
    fn missing_enum_can_be_relocated_without_losing_its_identity() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Enums.json");
        let new = dir.path().join("Moved.json");
        std::fs::write(&old, r#"{"State":["Idle"]}"#).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        project.sync_external_enums(&old).unwrap();
        let uid = project.json["defs"]["externalEnums"][0]["uid"].clone();
        std::fs::rename(&old, &new).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![old.clone()]);
        assert_eq!(app.resource_missing.len(), 1);
        assert!(app.relocate_resource(&ctx, &old, &new));
        let value = &app.project.as_ref().unwrap().json["defs"]["externalEnums"][0];
        assert_eq!(value["uid"], uid);
        assert_eq!(value["externalRelPath"], "Moved.json");
        assert_eq!(value["values"][0]["id"], "Idle");
        assert!(app.resource_missing.is_empty());
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().json["defs"]["externalEnums"][0]["externalRelPath"],
            "Enums.json"
        );
    }

    #[test]
    fn document_conflicts_use_registered_paths_not_json_extensions() {
        let dir = tempfile::tempdir().unwrap();
        let enums = dir.path().join("Enums.json");
        std::fs::write(&enums, r#"{"State":["Idle"]}"#).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.json"));
        project.json["externalLevels"] = json!(true);
        project.sync_external_enums(&enums).unwrap();
        let source = project.path.clone().unwrap();
        project.save(&source).unwrap();
        let rel = project.external_levels.keys().next().unwrap().clone();
        let external = dir.path().join(rel);
        let mut app = EditorApp::empty();
        app.set_project(project);
        std::fs::write(&enums, r#"{"State":["Idle","Run"]}"#).unwrap();
        assert!(app.changed_document_files().is_empty());
        std::fs::write(&external, "changed external document").unwrap();
        std::fs::write(&source, "changed root document").unwrap();
        let actual: std::collections::HashSet<_> =
            app.changed_document_files().into_iter().collect();
        assert_eq!(actual, std::collections::HashSet::from([source, external]));
        assert_eq!(app.changed_resource_files(), vec![enums]);
    }

    #[test]
    fn refreshing_artwork_preserves_conflicts_on_external_levels_removed_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let artwork = dir.path().join("Atlas.png");
        std::fs::write(&artwork, b"old artwork").unwrap();
        let mut project = Project::new();
        project.add_tileset(&artwork, 16, 16, 16).unwrap();
        let removed = project.add_level(None, "Removed", 32, 32).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let external = dir.path().join(
            project.json["levels"][removed.level]["externalRelPath"]
                .as_str()
                .unwrap(),
        );
        let mut app = EditorApp::empty();
        let (project, snapshot) = Project::load_with_snapshot(&path).unwrap();
        app.install_project(project, Some(snapshot));
        app.project.as_mut().unwrap().remove_level(removed).unwrap();
        app.dirty = true;

        let mut outside = std::fs::read(&external).unwrap();
        outside.push(b'\n');
        std::fs::write(&external, &outside).unwrap();
        std::fs::write(&artwork, b"new artwork").unwrap();
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![artwork]);
        assert_eq!(app.external_changes, Some(vec![external.clone()]));
        app.save_document(&ctx, path);
        assert!(app.saving.is_none());
        assert!(app.dirty);
        assert_eq!(std::fs::read(external).unwrap(), outside);
    }

    #[test]
    fn refreshing_unreadable_artwork_does_not_drop_document_watches() {
        let dir = tempfile::tempdir().unwrap();
        let artwork = dir.path().join("Atlas.png");
        let other = dir.path().join("Other.png");
        std::fs::write(&artwork, b"old artwork").unwrap();
        std::fs::write(&other, b"old second artwork").unwrap();
        let mut project = Project::new();
        project.add_tileset(&artwork, 16, 16, 16).unwrap();
        project.add_tileset(&other, 16, 16, 16).unwrap();
        let path = dir.path().join("Map.ldtk");
        project.save(&path).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        std::fs::remove_file(&artwork).unwrap();
        std::fs::create_dir(&artwork).unwrap();
        std::fs::write(&other, b"changed second artwork").unwrap();
        assert_eq!(app.changed_resource_files(), std::slice::from_ref(&other));
        app.changed_resources(&egui::Context::default(), vec![artwork]);
        assert_eq!(app.changed_resource_files(), [other]);

        let mut outside = std::fs::read(&path).unwrap();
        outside.push(b'\n');
        std::fs::write(&path, outside).unwrap();
        assert_eq!(app.changed_document_files(), [path]);
    }

    #[test]
    fn unreadable_resource_does_not_hide_a_document_conflict_before_save() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("atlas.png");
        std::fs::write(&atlas, b"original resource").unwrap();
        let mut project = Project::new();
        project.add_tileset(&atlas, 16, 32, 32).unwrap();
        let path = dir.path().join("project.ldtk");
        project.save(&path).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        std::fs::remove_file(&atlas).unwrap();
        std::fs::create_dir(&atlas).unwrap();
        std::fs::write(&path, b"external edit").unwrap();

        app.save_document(&egui::Context::default(), path.clone());

        assert_eq!(app.external_changes, Some(vec![path.clone()]));
        assert!(app.saving.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), b"external edit");

        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(app.changed_document_files(), vec![path]);
    }

    #[test]
    fn same_size_artwork_change_rebuilds_source_pixel_cache() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        let mut image = image::RgbaImage::new(32, 16);
        for y in 0..16 {
            for x in 0..16 {
                image.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        image.save(&atlas).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&atlas, 16, 32, 16).unwrap();
        project.json["defs"]["tilesets"][0]["cachedPixelData"] =
            json!({"opaqueTiles":"11","averageColors":"f00ff00f"});
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![atlas]);
        pump_until(&mut app, &ctx, |a| a.resource_checks.is_empty());
        let cache = &app.project.as_ref().unwrap().tileset(uid).unwrap()["cachedPixelData"];
        assert_eq!(cache["opaqueTiles"], "10");
        assert_eq!(cache["averageColors"], "ff000000");
        assert!(app.dirty && app.history.can_undo());
    }

    #[test]
    fn preview_154_color_only_tileset_reload_keeps_auto_layer_cache() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]))
            .save(&atlas)
            .unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&atlas, 16, 16, 16).unwrap();
        project.json["defs"]["tilesets"][0]["cachedPixelData"] =
            json!({"opaqueTiles":"1","averageColors":"ffff"});
        let layer = project.add_layer("Auto", "IntGrid", 16).unwrap();
        project.assign_tileset(layer, uid).unwrap();
        let key = project.level_keys()[0];
        let saved_cache = json!([{"px":[0,0],"src":[0,0],"t":0,"f":0,"extension":"keep"}]);
        project.layer_mut(key, 0).unwrap()["autoLayerTiles"] = saved_cache.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![atlas]);
        pump_until(&mut app, &ctx, |a| a.resource_checks.is_empty());
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.tileset(uid).unwrap()["cachedPixelData"]["averageColors"],
            "ff00"
        );
        assert_eq!(
            project.layer(key, 0).unwrap()["autoLayerTiles"],
            saved_cache
        );
    }

    #[test]
    fn changed_opacity_rebuilds_affected_auto_layer_culling() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        let mut image = image::RgbaImage::new(32, 16);
        for y in 0..16 {
            for x in 16..32 {
                image.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        image.save(&atlas).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&atlas, 16, 32, 16).unwrap();
        project.json["defs"]["tilesets"][0]["cachedPixelData"] =
            json!({"opaqueTiles":"11","averageColors":"ffffffff"});
        let layer = project.add_layer("Auto", "IntGrid", 16).unwrap();
        project.assign_tileset(layer, uid).unwrap();
        let group = project.add_auto_rule_group(layer, "Rules").unwrap();
        for tile in [0, 1] {
            let rule = project.add_auto_rule(layer, group, 1).unwrap();
            project
                .update_auto_rule(
                    layer,
                    group,
                    rule,
                    &json!({"pattern":[1],"tileRectsIds":[[tile]],"breakOnMatch":false}),
                )
                .unwrap();
        }
        let key = project.level_keys()[0];
        project.paint_int_grid(key, 0, 0, 0, 1).unwrap();
        assert_eq!(
            project.layer(key, 0).unwrap()["autoLayerTiles"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![atlas]);
        pump_until(&mut app, &ctx, |a| a.resource_checks.is_empty());
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.tileset(uid).unwrap()["cachedPixelData"]["opaqueTiles"],
            "01"
        );
        assert_eq!(
            project.layer(key, 0).unwrap()["autoLayerTiles"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn pixel_cache_rounds_argb_channels_and_preserves_source_partial_edge_behavior() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([9, 26, 43, 128]))
            .save(&atlas)
            .unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&atlas, 2, 3, 2).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![atlas]);
        pump_until(&mut app, &ctx, |a| a.resource_checks.is_empty());
        let cache = &app.project.as_ref().unwrap().tileset(uid).unwrap()["cachedPixelData"];
        assert_eq!(cache["opaqueTiles"], "01");
        assert_eq!(cache["averageColors"], "81230000");
    }

    #[test]
    fn border_crop_trims_padding_once_and_builds_cache_for_the_final_grid() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        image::RgbaImage::from_pixel(34, 34, image::Rgba([255, 255, 255, 255]))
            .save(&atlas)
            .unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        let uid = project.add_tileset(&atlas, 16, 36, 36).unwrap();
        project.json["defs"]["tilesets"][0]["padding"] = json!(2);
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.changed_resources(&ctx, vec![atlas]);
        pump_until(&mut app, &ctx, |a| a.resource_checks.is_empty());
        let def = app.project.as_ref().unwrap().tileset(uid).unwrap();
        assert_eq!(def["padding"], 1);
        assert_eq!(def["__cWid"], 2);
        assert_eq!(def["cachedPixelData"]["opaqueTiles"], "1111");
        assert_eq!(def["cachedPixelData"]["averageColors"], "ffffffffffffffff");
    }

    #[test]
    fn missing_background_is_checked_on_initial_scan_and_can_be_dismissed() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("Background.png");
        let mut project = Project::new();
        project.path = Some(dir.path().join("Map.ldtk"));
        project.json["levels"][0]["bgRelPath"] = json!("Background.png");
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.resource_scan(&ctx);
        pump_until(&mut app, &ctx, |a| !a.resource_missing.is_empty());
        assert_eq!(app.resource_missing[0].0, missing);
        assert!(app.resource_missing[0].1.is_empty());
        app.ignore_resource(&missing);
        app.pump_resources(&ctx);
        assert!(app.resource_missing.is_empty());
        assert_eq!(
            app.project.as_ref().unwrap().json["levels"][0]["bgRelPath"],
            "Background.png"
        );
    }

    #[test]
    fn failed_decode_can_retry_without_accepting_an_existing_document_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let atlas = dir.path().join("Atlas.png");
        std::fs::write(&atlas, "not an image").unwrap();
        let mut project = Project::new();
        project.add_tileset(&atlas, 16, 32, 32).unwrap();
        let source = dir.path().join("Map.ldtk");
        project.save(&source).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        app.resource_scan(&ctx);
        pump_until(&mut app, &ctx, |a| !a.resource_missing.is_empty());
        std::fs::write(&source, "edited elsewhere").unwrap();
        image::RgbaImage::new(32, 32).save(&atlas).unwrap();
        app.retry_resource(&ctx, &atlas);
        pump_until(&mut app, &ctx, |a| {
            a.resource_checks.is_empty() && a.assets.errors.is_empty()
        });
        assert_eq!(
            app.external_changes.as_ref().unwrap(),
            &vec![source.clone()]
        );
        assert_eq!(app.changed_document_files(), vec![source]);
        assert!(app.resource_missing.is_empty());
    }

    #[test]
    fn ignore_dismisses_only_selected_failure_without_rewriting_the_project() {
        let mut app = EditorApp::empty();
        app.project = Some(Project::new());
        let before = app.project.as_ref().unwrap().json.clone();
        app.resource_missing = vec![
            (PathBuf::from("a.png"), vec![1], "missing".into()),
            (PathBuf::from("b.png"), vec![2], "bad image".into()),
        ];
        app.ignore_resource(std::path::Path::new("a.png"));
        assert_eq!(app.resource_missing.len(), 1);
        assert_eq!(app.resource_missing[0].0, PathBuf::from("b.png"));
        assert_eq!(app.project.as_ref().unwrap().json, before);
        assert!(!app.dirty);
    }
}
