use super::*;
use serde_json::json;
use std::path::Path;

fn clean(path: &Path) -> PathBuf {
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

fn atlas_pixel_data(
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

impl EditorApp {
    pub(super) fn changed_document_files(&self) -> Vec<PathBuf> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let documents = document_paths(project);
        self.file_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.changed_files().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|path| documents.contains(&clean(path)))
            .collect()
    }
    pub(super) fn changed_resource_files(&self) -> Vec<PathBuf> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let documents = document_paths(project);
        self.file_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.changed_files().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|path| !documents.contains(&clean(path)))
            .collect()
    }
    fn resource_failure(&mut self, path: PathBuf, uids: Vec<i64>, error: String) {
        self.status = format!("Could not reload {}: {error}", path.display());
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
            self.status = format!("The selected file does not exist: {}", new.display());
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
    pub(super) fn resource_scan(&mut self, ctx: &egui::Context) {
        let Some(project) = &self.project else {
            return;
        };
        let atlases = atlas_paths(project);
        let enums = if project.is_backup() {
            HashSet::new()
        } else {
            enum_paths(project)
        };
        let backgrounds = project
            .level_keys()
            .into_iter()
            .filter_map(|key| project.level(key)?.get("bgRelPath")?.as_str())
            .map(|rel| clean(&canvas::asset_path(project, rel)))
            .collect::<Vec<_>>();
        for (path, uid) in atlases {
            let uids = self.resource_checks.entry(path).or_default();
            if !uids.contains(&uid) {
                uids.push(uid);
            }
        }
        for path in backgrounds {
            self.resource_checks.entry(path).or_default();
        }
        if !enums.is_empty() {
            self.changed_resources(ctx, enums.into_iter().collect());
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
        let backgrounds = project
            .level_keys()
            .into_iter()
            .filter_map(|key| project.level(key)?.get("bgRelPath")?.as_str())
            .map(|rel| clean(&canvas::asset_path(project, rel)))
            .collect::<HashSet<_>>();
        let backup = project.is_backup();
        // Preserve an existing document conflict even if only a resource retry
        // triggered this pass; accepting its baseline would hide disk edits.
        let mut conflicts = self.changed_document_files();
        let mut enum_files = HashSet::new();
        for path in paths {
            let path = clean(&path);
            if documents.contains(&path) {
                if !conflicts.contains(&path) {
                    conflicts.push(path);
                }
                continue;
            }
            if enums.contains(&path) {
                if !backup {
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
            match self.edit("External enums updated", |p| p.sync_external_enums(&path)) {
                Some(report) => {
                    self.ignore_resource(&path);
                    if report.requires_confirmation {
                        self.begin_enum_sync(ctx, path);
                    }
                }
                None => self.resource_failure(path, Vec::new(), self.status.clone()),
            }
        }
        if conflicts.is_empty() {
            self.file_snapshot = self
                .project
                .as_ref()
                .and_then(|p| p.filesystem_snapshot().ok());
        } else {
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
                let Some(def) = self.project.as_ref().and_then(|p| p.tileset(uid)).cloned() else {
                    continue;
                };
                let geometry_changed = def["pxWid"].as_u64() != Some(size[0] as u64)
                    || def["pxHei"].as_u64() != Some(size[1] as u64);
                let width = size[0] as i64;
                let height = size[1] as i64;
                let max = width.min(height).max(1);
                let old_w = number(&def, "pxWid", 0.0) as i64;
                let old_h = number(&def, "pxHei", 0.0) as i64;
                let old_padding = (number(&def, "padding", 0.0) as i64).min(max);
                let grid = (number(&def, "tileGridSize", 16.0) as i64).min(max);
                let spacing = (number(&def, "spacing", 0.0) as i64).min(max);
                let trim = old_padding > 0
                    && old_w > width
                    && old_h > height
                    && (old_w - width) <= old_padding * 2
                    && (old_h - height) <= old_padding * 2
                    && width % 2 == 0
                    && height % 2 == 0;
                let padding = if trim {
                    old_padding - (old_w - width).min(old_h - height) / 2
                } else {
                    old_padding
                };
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
                    self.edit("Atlas pixel data updated", |p| {
                        let def = p.json["defs"]["tilesets"]
                            .as_array_mut()
                            .and_then(|defs| defs.iter_mut().find(|def| def["uid"] == uid))
                            .ok_or_else(|| {
                                ldtk_core::Error::Invalid("tileset no longer exists".into())
                            })?;
                        def["cachedPixelData"] = pixels;
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
            self.resource_failure(path, uids, error);
        }
        if !self.resource_checks.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
    }
    pub(super) fn resource_dialog(&mut self, ctx: &egui::Context) {
        let Some((path, _, error)) = self.resource_missing.first().cloned() else {
            return;
        };
        let id = egui::Id::new(("resource-relocation", &path));
        if let Some(new) = ctx.data_mut(|data| data.get_temp::<PathBuf>(id)) {
            let mut apply = false;
            let mut cancel = false;
            egui::Modal::new(id).show(ctx, |ui| {
                ui.set_width(560.0);
                ui.heading("The selected file has a different name");
                ui.label(
                    "Selecting a different file may change your project. Is this the correct file?",
                );
                ui.monospace(new.display().to_string());
                ui.horizontal(|ui| {
                    apply = ui.button("Use selected file").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if apply || cancel {
                ctx.data_mut(|data| data.remove::<PathBuf>(id));
            }
            if apply {
                self.relocate_resource(ctx, &path, &new);
            }
            return;
        }
        let missing = !path.exists();
        let mut locate = false;
        let mut retry = false;
        let mut ignore = false;
        egui::Modal::new(egui::Id::new("resource-load-failure")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading(if missing {
                "File not found!"
            } else {
                "Could not load resource"
            });
            ui.monospace(path.display().to_string());
            ui.label(error);
            ui.horizontal(|ui| {
                locate = ui
                    .add_enabled(
                        !self.project.as_ref().is_some_and(Project::is_backup),
                        egui::Button::new("Locate the file"),
                    )
                    .clicked();
                retry = ui.button("Retry").clicked();
                ignore = ui
                    .button(if missing { "Fix that later" } else { "Ignore" })
                    .clicked();
            });
        });
        if ignore {
            self.ignore_resource(&path);
        } else if retry {
            self.retry_resource(ctx, &path);
        } else if locate {
            let mut dialog = rfd::FileDialog::new().set_title("Locate the missing resource");
            if let Some(parent) = path
                .parent()
                .filter(|p| p.is_dir())
                .or_else(|| self.project.as_ref()?.path.as_deref()?.parent())
            {
                dialog = dialog.set_directory(parent);
            }
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                dialog = dialog.add_filter("Original file type", &[ext]);
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
