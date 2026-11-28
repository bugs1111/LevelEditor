use serde_json::{Value, json};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Normal window bounds in OS logical pixels, independent of editor UI zoom.
#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct WindowPlacement {
    size: Option<[f32; 2]>,
    position: Option<[f32; 2]>,
    pub(super) maximized: bool,
}

impl WindowPlacement {
    pub(crate) fn viewport(&self) -> eframe::egui::ViewportBuilder {
        let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size(
            self.size
                .filter(|s| s.iter().all(|&v| v.is_finite() && v > 0.))
                .unwrap_or([800., 600.]),
        );
        if let Some(position) = self.position.filter(|p| p.iter().all(|v| v.is_finite())) {
            viewport = viewport.with_position(position);
        }
        viewport
    }

    pub(super) fn observe(
        &mut self,
        input: &eframe::egui::RawInput,
        zoom: f32,
        maximized: Option<bool>,
    ) {
        let viewport = input.viewport();
        if viewport.minimized == Some(true) || viewport.fullscreen == Some(true) {
            return;
        }
        // Wayland exposes the client size but leaves global positions to the compositor.
        #[cfg(target_os = "linux")]
        let inner = viewport.inner_rect.or(input.screen_rect);
        #[cfg(not(target_os = "linux"))]
        let inner = viewport.inner_rect;
        let Some(inner) = inner.filter(|r| r.is_positive() && r.is_finite()) else {
            return;
        };
        if let Some(maximized) = maximized {
            self.maximized = maximized;
        }
        if self.maximized {
            return;
        }
        self.size = Some((inner.size() * zoom).into());
        if let Some(outer) = viewport.outer_rect {
            self.position = Some((outer.min * zoom).into());
        }
    }
}

// egui-winit only queries this flag at window creation on macOS. Read it from
// our titled native window so a later zoom or unzoom preserves the normal frame.
#[cfg(target_os = "macos")]
pub(super) fn window_maximized(frame: &eframe::Frame) -> Option<bool> {
    use objc2_app_kit::NSView;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = frame.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // The frame owns this view, and eframe calls update on AppKit's main thread.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    Some(view.window()?.isZoomed())
}

pub(crate) struct Settings {
    pub value: Value,
    pub path: Option<PathBuf>,
    // Compare against this instance's previous preferences to identify its own edits.
    last_saved: Option<Value>,
    previous_profile: Option<PathBuf>,
}
impl Default for Settings {
    fn default() -> Self {
        Self::in_memory(json!({
            "zenMode":false,"grid":true,"emptySpaceSelection":true,"tileStacking":true,
            "tileEnumOverlays":false,"showDetails":true,"useBestGPU":true,"startFullScreen":false,
            "colorBlind":false,"blurMask":true,"navigationKeys":"Wasd","singleLayerMode":false,"language":"en",
            "singleLayerModeIntensity":0.75,"openLastProject":false,"lastProject":null,
            "autoWorldModeSwitch":"ZoomInAndOut","fieldsRender":"FR_Outline",
            "nearbyTilesRenderingDist":1.0,"appUiScale":1.0,"editorUiScale":1.0,"mouseWheelSpeed":1.0,
            "recentProjects":[],"recentDirs":[],"recentDirColors":[],"uiStates":[],"lastUiDirs":[],"projectTrusts":[]
        }))
    }
}
impl Settings {
    pub(super) fn in_memory(value: Value) -> Self {
        Self {
            value,
            path: None,
            last_saved: None,
            previous_profile: None,
        }
    }

    pub(crate) fn window_placement(&self) -> WindowPlacement {
        serde::Deserialize::deserialize(&self.value["windowPlacement"]).unwrap_or_default()
    }

    pub fn load() -> Self {
        if let Some(root) = std::env::var_os("LDTK_CONFIG_DIR") {
            return Self::load_at(Some(PathBuf::from(root).join("settings.json")));
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        Self::load_default(home.as_deref(), appdata.as_deref())
    }

    fn load_default(home: Option<&Path>, appdata: Option<&Path>) -> Self {
        const PREVIOUS_TITLE: &str = concat!("LDtk", " Rust");
        const PREVIOUS_SLUG: &str = concat!("ldtk", "-rust");
        let mac = cfg!(target_os = "macos");
        let home_config = home.map(|path| {
            path.join(if mac {
                "Library/Application Support"
            } else {
                ".config"
            })
        });
        let appdata = appdata.filter(|_| cfg!(windows));
        let current = appdata
            .or(home_config.as_deref())
            .map(|root| root.join("level-editor/settings.json"));
        let home_previous = home_config.as_ref().map(|root| {
            root.join(if mac { PREVIOUS_TITLE } else { PREVIOUS_SLUG })
                .join("settings.json")
        });
        // Earlier Windows installations prefer the home profile when it exists.
        let previous = if let Some(appdata) = appdata
            && !home_previous.as_deref().is_some_and(Path::is_file)
        {
            Some(appdata.join(PREVIOUS_TITLE).join("settings.json"))
        } else {
            home_previous
        };
        Self::load_with_fallback(current, previous.as_deref())
    }

    pub(super) fn load_at(path: Option<PathBuf>) -> Self {
        Self::load_with_fallback(path, None)
    }

    fn load_with_fallback(path: Option<PathBuf>, previous: Option<&Path>) -> Self {
        let mut settings = Self {
            path,
            previous_profile: previous.map(Path::to_owned),
            ..Self::default()
        };
        // A fresh profile follows the desktop. The saved value can override it below.
        settings.value["language"] = "system".into();
        if let Ok(Some(object)) = settings.read_profile() {
            for (k, v) in object {
                settings.value[k] = v;
            }
        }
        if settings.path.is_some() {
            settings.last_saved = Some(settings.value.clone());
        }
        settings
    }

    fn read_profile(&mut self) -> std::io::Result<Option<serde_json::Map<String, Value>>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let primary = std::fs::read(path);
        let bytes = match (&primary, &self.previous_profile) {
            (Err(error), Some(previous)) if error.kind() == ErrorKind::NotFound => {
                std::fs::read(previous)
            }
            _ => {
                self.previous_profile = None;
                primary
            }
        };
        match bytes {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.previous_profile = None;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
    pub(super) fn language(&self) -> super::locale::Language {
        super::locale::Language::from_code(self.value["language"].as_str())
    }
    pub fn save(&mut self) -> std::io::Result<()> {
        self.persist(&[], |_| {})
    }

    fn persist(
        &mut self,
        edited_keys: &[&str],
        edit: impl FnOnce(&mut Value),
    ) -> std::io::Result<()> {
        let Some(path) = self.path.clone() else {
            edit(&mut self.value);
            return Ok(());
        };
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        // The data file is replaced atomically, so lock a stable sibling across the read and write.
        let lock = std::fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.with_extension("lock"))?;
        lock.lock()?;
        let mut disk = self.read_profile()?;
        let preferences_unchanged = disk.is_some()
            && self.previous_profile.is_none()
            && self.last_saved.as_ref() == Some(&self.value);
        if preferences_unchanged && edited_keys.is_empty() {
            return Ok(());
        }
        let trusts = take_trust_list(disk.as_mut());
        let mut merged = if let (Some(mut disk), Some(previous)) =
            (disk, self.last_saved.as_ref().and_then(Value::as_object))
        {
            let local = self.value.as_object().expect("settings are a JSON object");
            disk.retain(|key, _| !previous.contains_key(key) || local.contains_key(key));
            for (key, value) in local {
                if previous.get(key) != Some(value) {
                    disk.insert(key.clone(), value.clone());
                }
            }
            Value::Object(disk)
        } else {
            self.value.clone()
        };
        merged["projectTrusts"] = trusts;
        let before = preferences_unchanged.then(|| {
            edited_keys
                .iter()
                .map(|&key| (key, merged[key].clone()))
                .collect::<Vec<_>>()
        });
        // History and trust edits operate on the latest lists while holding the lock.
        edit(&mut merged);
        if before.is_none_or(|values| values.into_iter().any(|(key, value)| merged[key] != value)) {
            let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
            tmp.write_all(&serde_json::to_vec_pretty(&merged)?)?;
            tmp.persist(path)?;
        }
        self.previous_profile = None;
        let merged = merged.as_object_mut().expect("settings are a JSON object");
        for key in edited_keys.iter().copied().chain(["projectTrusts"]) {
            if let Some(value) = merged.remove(key) {
                self.value[key] = value;
            }
        }
        self.last_saved = Some(self.value.clone());
        Ok(())
    }
    pub fn ui_state_int(&self, id: &str, project_iid: Option<&str>, default: i64) -> i64 {
        let id = project_iid.map_or_else(|| id.to_owned(), |iid| format!("{iid}_{id}"));
        self.value["uiStates"]
            .as_array()
            .and_then(|states| states.iter().find(|state| state["id"] == id))
            .and_then(|state| state["val"].as_i64())
            .unwrap_or(default)
    }
    pub fn set_ui_state_int(&mut self, id: &str, project_iid: Option<&str>, value: i64) {
        let id = project_iid.map_or_else(|| id.to_owned(), |iid| format!("{iid}_{id}"));
        if !self.value["uiStates"].is_array() {
            self.value["uiStates"] = json!([]);
        }
        let states = self.value["uiStates"].as_array_mut().unwrap();
        if let Some(state) = states.iter_mut().find(|state| state["id"] == id) {
            state["val"] = value.into();
        } else {
            states.push(json!({"id":id,"val":value}));
        }
        let _ = self.save();
    }
    pub fn delete_ui_state(&mut self, id: &str, project_iid: Option<&str>) {
        let id = project_iid.map_or_else(|| id.to_owned(), |iid| format!("{iid}_{id}"));
        if let Some(states) = self.value["uiStates"].as_array_mut() {
            states.retain(|state| state["id"] != id);
        }
        let _ = self.save();
    }
    pub fn ui_dir(
        &self,
        project: Option<&Path>,
        ui_id: &str,
        fallback: Option<&Path>,
    ) -> Option<PathBuf> {
        let project = project.map(|p| p.to_string_lossy().replace('\\', "/"));
        self.value["lastUiDirs"]
            .as_array()
            .and_then(|dirs| {
                dirs.iter().find(|dir| {
                    (project.is_none() || dir["project"].as_str() == project.as_deref())
                        && dir["uiId"] == ui_id
                })
            })
            .and_then(|dir| dir["path"].as_str())
            .map(PathBuf::from)
            .or_else(|| fallback.map(Path::to_owned))
            .or_else(|| {
                project
                    .as_ref()
                    .and_then(|p| Path::new(p).parent())
                    .map(Path::to_owned)
            })
    }
    pub fn store_ui_dir(&mut self, project: Option<&Path>, ui_id: &str, path: &Path) {
        let project = project.map(|p| p.to_string_lossy().replace('\\', "/"));
        let path = path.to_string_lossy().replace('\\', "/");
        if !self.value["lastUiDirs"].is_array() {
            self.value["lastUiDirs"] = json!([]);
        }
        let dirs = self.value["lastUiDirs"].as_array_mut().unwrap();
        if let Some(dir) = dirs.iter_mut().find(|dir| {
            (project.is_none() || dir["project"].as_str() == project.as_deref())
                && dir["uiId"] == ui_id
        }) {
            dir["path"] = path.into();
        } else {
            let mut dir = json!({"uiId":ui_id,"path":path});
            if let Some(project) = project {
                dir["project"] = project.into();
            }
            dirs.push(dir);
        }
        let _ = self.save();
    }

    pub(super) fn most_recent_project(&self) -> Option<&Path> {
        self.value["recentProjects"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find(|path| !path.is_empty())
            .map(Path::new)
    }

    pub(super) fn default_project_dialog_dir(&self) -> Option<PathBuf> {
        self.most_recent_project()
            .and_then(Path::parent)
            .map(Path::to_owned)
            .or_else(|| std::env::current_exe().ok()?.parent().map(Path::to_owned))
    }
    /// The displayed color and whether the user assigned it.
    pub fn dir_color(&self, path: &Path) -> (u32, bool) {
        let path = dir_color_key(path);
        if let Some(color) = self.value["recentDirColors"]
            .as_array()
            .and_then(|colors| {
                colors
                    .iter()
                    .find(|color| color["path"].as_str() == Some(&path))
            })
            .and_then(|color| color["col"].as_str())
            .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
        {
            return (color, true);
        }
        let palette = super::panels::source_colors::palette(self.bool("colorBlind"))
            .iter()
            .copied()
            .filter(|color| {
                let r = (color >> 16) & 255;
                let g = (color >> 8) & 255;
                let b = color & 255;
                (299 * r + 587 * g + 114 * b) as f64 / 255000. >= 0.4
            })
            .collect::<Vec<_>>();
        let sum = path.encode_utf16().map(usize::from).sum::<usize>();
        (palette[sum % palette.len()], false)
    }
    pub fn force_dir_color(&mut self, path: &Path, color: Option<u32>) {
        let path = dir_color_key(path);
        self.edit_history(&["recentDirColors"], |profile| {
            if !profile["recentDirColors"].is_array() {
                profile["recentDirColors"] = json!([]);
            }
            let colors = profile["recentDirColors"].as_array_mut().unwrap();
            colors.retain(|entry| entry["path"].as_str() != Some(&path));
            if let Some(color) = color {
                colors.push(json!({"path":path,"col":format!("#{color:06X}")}));
            }
        });
    }
    pub fn bool(&self, key: &str) -> bool {
        self.value[key].as_bool().unwrap_or(false)
    }
    pub fn float(&self, key: &str, fallback: f32) -> f32 {
        self.value[key]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(fallback)
    }
    pub fn remember(&mut self, path: &Path, level_uid: i64) {
        self.remember_project(path, level_uid, None);
    }
    pub(super) fn remember_project(
        &mut self,
        path: &Path,
        level_uid: i64,
        renamed_from: Option<&Path>,
    ) {
        self.edit_history(
            &["recentProjects", "recentDirs", "lastProject"],
            |profile| {
                let recent = &mut profile["recentProjects"];
                if let Some(previous) = renamed_from {
                    remove_history_path(recent, previous);
                }
                remember_path(recent, path, 30);
                if let Some(dir) = path.parent() {
                    remember_path(&mut profile["recentDirs"], dir, 20);
                }
                profile["lastProject"] =
                    json!({"filePath":path.to_string_lossy(),"levelUid":level_uid});
            },
        );
    }
    pub(super) fn remove_recent(&mut self, key: &str, path: &Path) {
        self.edit_history(&[key], |profile| {
            remove_history_path(&mut profile[key], path)
        });
    }
    pub(super) fn clear_recent(&mut self, key: &str) {
        self.edit_history(&[key], |profile| profile[key] = json!([]));
    }
    fn edit_history(&mut self, keys: &[&str], edit: impl Fn(&mut Value)) {
        if self.persist(keys, &edit).is_err() {
            // Keep the history usable in this window. An unrelated preferences save
            // must not replay a whole stale list after the profile becomes writable.
            edit(&mut self.value);
            if let Some(baseline) = &mut self.last_saved {
                for &key in keys {
                    baseline[key] = self.value[key].clone();
                }
            }
        }
    }
    pub fn trust_state(&self, iid: &str) -> Option<bool> {
        self.value["projectTrusts"]
            .as_array()?
            .iter()
            .find(|p| p["iid"] == iid)?["trusted"]
            .as_bool()
    }
    pub(super) fn set_trust(&mut self, iid: &str, trusted: Option<bool>) -> std::io::Result<()> {
        self.persist(&["projectTrusts"], |profile| {
            update_trust_list(&mut profile["projectTrusts"], iid, trusted)
        })
    }
    pub(super) fn refresh_trust(&mut self) -> std::io::Result<()> {
        if self.path.is_some() {
            let mut profile = self.read_profile();
            let trusts = take_trust_list(profile.as_mut().ok().and_then(Option::as_mut));
            if let Some(previous) = &mut self.last_saved {
                previous["projectTrusts"] = trusts.clone();
            }
            self.value["projectTrusts"] = trusts;
            profile?;
        }
        Ok(())
    }
}

fn dir_color_key(path: &Path) -> std::borrow::Cow<'_, str> {
    let path = path.to_string_lossy();
    #[cfg(windows)]
    if path.contains('\\') {
        return path.replace('\\', "/").into();
    }
    path
}

fn remove_history_path(list: &mut Value, path: &Path) {
    if let Some(entries) = list.as_array_mut() {
        let name = path.to_string_lossy();
        entries.retain(|entry| entry.as_str().map(Path::new) != Some(Path::new(name.as_ref())));
    }
}

fn remember_path(list: &mut Value, path: &Path, limit: usize) {
    remove_history_path(list, path);
    let name = path.to_string_lossy();
    if let Some(entries) = list.as_array_mut() {
        entries.insert(0, json!(name));
        entries.truncate(limit);
    } else {
        *list = json!([name]);
    }
}

fn take_trust_list(profile: Option<&mut serde_json::Map<String, Value>>) -> Value {
    profile
        .and_then(|map| map.remove("projectTrusts"))
        .filter(Value::is_array)
        .unwrap_or_else(|| json!([]))
}

fn update_trust_list(trusts: &mut Value, iid: &str, trusted: Option<bool>) {
    let mut list = match trusts.take() {
        Value::Array(entries) => entries,
        _ => Vec::new(),
    };
    list.retain(|entry| entry["iid"] != iid);
    if let Some(trusted) = trusted {
        list.push(json!({"iid":iid,"trusted":trusted}));
    }
    *trusts = Value::Array(list);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{RawInput, Rect, ViewportId, pos2, vec2};

    #[test]
    fn closing_an_older_editor_preserves_settings_changed_in_another_window() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut newer = Settings::load_at(Some(path.clone()));
        newer.value["windowPlacement"] =
            json!({"size":[800.,600.],"position":[10.,20.],"maximized":false});
        newer.save().unwrap();
        let older = Settings::load_at(Some(path.clone()));
        let placement = older.window_placement();
        let mut editor = crate::app::EditorApp::with_settings(older, placement);
        editor.apply_initial_preferences(&Default::default());

        newer.value["language"] = "zh-CN".into();
        newer.value["grid"] = false.into();
        newer.value["windowPlacement"]["size"] = json!([1100., 700.]);
        newer.remember(Path::new("maps/forest.ldtk"), 4);
        eframe::App::on_exit(&mut editor);

        let saved = Settings::load_at(Some(path));
        assert_eq!(saved.value["language"], "zh-CN");
        assert!(!saved.bool("grid"));
        assert_eq!(
            saved.value["windowPlacement"],
            newer.value["windowPlacement"]
        );
        assert_eq!(
            saved.most_recent_project(),
            Some(Path::new("maps/forest.ldtk"))
        );
    }

    #[test]
    fn simultaneous_settings_saves_keep_independent_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for (key, value) in [("grid", json!(false)), ("language", json!("zh-CN"))] {
                let mut settings = Settings::load_at(Some(path.clone()));
                let start = &start;
                scope.spawn(move || {
                    settings.value[key] = value;
                    start.wait();
                    settings.save().unwrap();
                });
            }
        });
        let saved = Settings::load_at(Some(path));
        assert!(!saved.bool("grid"));
        assert_eq!(saved.value["language"], "zh-CN");
    }

    #[test]
    fn window_placement_keeps_normal_bounds_across_move_maximize_and_exit() {
        let mut placement = WindowPlacement::default();
        let mut raw = RawInput::default();
        let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
        viewport.inner_rect = Some(Rect::from_min_size(pos2(-190., 65.), vec2(480., 320.)));
        viewport.outer_rect = Some(Rect::from_min_size(pos2(-192., 45.), vec2(484., 342.)));
        placement.observe(&raw, 1.5, Some(false));

        let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
        viewport.outer_rect = viewport.outer_rect.map(|r| r.translate(vec2(38., 20.)));
        placement.observe(&raw, 1.5, Some(false));
        let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
        viewport.inner_rect = Some(Rect::from_min_size(pos2(0., 0.), vec2(960., 600.)));
        placement.observe(&raw, 1.5, Some(true));
        raw.viewports.get_mut(&ViewportId::ROOT).unwrap().fullscreen = Some(true);
        placement.observe(&raw, 1.5, Some(false));
        let viewport = raw.viewports.get_mut(&ViewportId::ROOT).unwrap();
        viewport.fullscreen = Some(false);
        viewport.minimized = Some(true);
        placement.observe(&raw, 1.5, Some(false));

        let temp = tempfile::tempdir().unwrap();
        let path = Some(temp.path().join("settings.json"));
        let mut settings = Settings::load_at(path.clone());
        settings.value["windowPlacement"] = json!(placement);
        settings.save().unwrap();
        let saved = Settings::load_at(path).window_placement();
        assert!(saved.maximized);
        assert_eq!(saved.viewport().position, Some(pos2(-231., 97.5)));
        assert_eq!(saved.viewport().inner_size, Some(vec2(720., 480.)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn window_size_is_remembered_when_the_compositor_owns_position() {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0., 0.), vec2(700., 410.))),
            ..Default::default()
        };
        let mut placement = WindowPlacement::default();
        placement.observe(&raw, 2., Some(false));
        assert_eq!(placement.viewport().inner_size, Some(vec2(1400., 820.)));
        assert_eq!(placement.viewport().position, None);
    }

    #[test]
    fn default_settings_load_the_previous_profile_and_save_under_level_editor() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let appdata = directory.path().join("appdata");
        let config = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(windows) {
            appdata.clone()
        } else {
            home.join(".config")
        };
        let previous = config.join(if cfg!(any(target_os = "macos", windows)) {
            concat!("LDtk", " Rust")
        } else {
            concat!("ldtk", "-rust")
        });
        let current = config.join("level-editor/settings.json");
        std::fs::create_dir_all(&previous).unwrap();
        let previous = previous.join("settings.json");
        let original = br#"{"language":"zh-CN","grid":false,"recentProjects":["map.ldtk"],"projectTrusts":[{"iid":"map","trusted":true}]}"#;
        std::fs::write(&previous, original).unwrap();

        let mut settings = Settings::load_default(Some(&home), Some(&appdata));
        assert_eq!(settings.path.as_deref(), Some(current.as_path()));
        assert_eq!(settings.value["language"], "zh-CN");
        assert_eq!(settings.value["recentProjects"], json!(["map.ldtk"]));
        assert!(!settings.bool("grid"));
        settings.refresh_trust().unwrap();
        assert_eq!(settings.trust_state("map"), Some(true));
        settings.value["grid"] = true.into();
        settings.save().unwrap();
        assert!(current.is_file());
        assert_eq!(std::fs::read(previous).unwrap(), original);
        assert!(Settings::load_default(Some(&home), Some(&appdata)).bool("grid"));
        std::fs::remove_file(current).unwrap();
        settings.refresh_trust().unwrap();
        assert_eq!(settings.trust_state("map"), None);
        settings.save().unwrap();
        assert_eq!(
            Settings::load_default(Some(&home), Some(&appdata)).trust_state("map"),
            None
        );
    }

    #[test]
    fn settings_default_to_system_language_and_preserve_explicit_preferences() {
        use crate::app::locale::Language;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::load_at(Some(path.clone()));
        assert_eq!(settings.value["language"], "system");
        settings.save().unwrap();
        assert_eq!(
            Settings::load_at(Some(path.clone())).value["language"],
            "system"
        );

        std::fs::write(&path, br#"{"grid": false}"#).unwrap();
        let settings = Settings::load_at(Some(path.clone()));
        assert_eq!(settings.value["language"], "system");
        assert!(!settings.bool("grid"));

        for (code, expected) in [
            ("en", Language::English),
            ("zh-CN", Language::SimplifiedChinese),
        ] {
            std::fs::write(&path, json!({"language": code}).to_string()).unwrap();
            let settings = Settings::load_at(Some(path.clone()));
            assert_eq!(settings.value["language"], code);
            assert_eq!(settings.language(), expected);
        }
    }

    #[test]
    fn file_dialog_directories_use_source_project_path_and_ui_identifiers() {
        let mut s = Settings::default();
        let a = Path::new("/a/Map.ldtk");
        let b = Path::new("/b/Map.ldtk");
        assert_eq!(
            s.ui_dir(Some(a), "PickImage", None),
            Some(PathBuf::from("/a"))
        );
        s.store_ui_dir(Some(a), "PickImage", Path::new("/images"));
        s.store_ui_dir(Some(b), "PickImage", Path::new("/other"));
        s.store_ui_dir(None, "ImportOgmo", Path::new("/ogmo"));
        assert_eq!(
            s.ui_dir(Some(a), "PickImage", None),
            Some(PathBuf::from("/images"))
        );
        assert_eq!(
            s.ui_dir(Some(b), "PickImage", None),
            Some(PathBuf::from("/other"))
        );
        assert_eq!(
            s.value["lastUiDirs"],
            json!([{"project":"/a/Map.ldtk","uiId":"PickImage","path":"/images"},{"project":"/b/Map.ldtk","uiId":"PickImage","path":"/other"},{"uiId":"ImportOgmo","path":"/ogmo"}])
        );
        s.force_dir_color(Path::new("/a"), Some(0x123456));
        assert_eq!(s.dir_color(Path::new("/a")), (0x123456, true));
        assert_eq!(
            s.value["recentDirColors"],
            json!([{"path":"/a","col":"#123456"}])
        );
        s.force_dir_color(Path::new("/a"), None);
        assert!(s.value["recentDirColors"].as_array().unwrap().is_empty());
    }
    #[test]
    fn source_ui_state_ids_are_project_scoped_and_persist_without_duplicates() {
        let mut s = Settings::default();
        let dir = std::env::temp_dir().join(format!(
            "ldtk-ui-states-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        s.path = Some(dir.join("settings.json"));
        assert_eq!(
            s.ui_state_int("EntityPaletteColumns_4", Some("project-a"), 1),
            1
        );
        s.set_ui_state_int("EntityPaletteColumns_4", Some("project-a"), 3);
        s.set_ui_state_int("EntityPaletteColumns_4", Some("project-a"), 5);
        s.set_ui_state_int("extraEntityDetails", None, 0);
        assert_eq!(
            s.ui_state_int("EntityPaletteColumns_4", Some("project-b"), 1),
            1
        );
        assert_eq!(
            s.value["uiStates"],
            json!([{"id":"project-a_EntityPaletteColumns_4","val":5},{"id":"extraEntityDetails","val":0}])
        );
        let saved: Value =
            serde_json::from_slice(&std::fs::read(s.path.as_ref().unwrap()).unwrap()).unwrap();
        assert_eq!(saved["uiStates"], s.value["uiStates"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn directory_color_reads_existing_slash_keys_using_native_path_separators() {
        let mut settings = Settings::default();
        settings.value["recentDirColors"] = json!([{"path":"art/maps","col":"#2255AA"}]);
        let native = Path::new(r"art\maps");
        assert_eq!(settings.dir_color(native).1, cfg!(windows));
        settings.force_dir_color(native, Some(0x228833));
        assert_eq!(settings.dir_color(native), (0x228833, true));
        let expected = if cfg!(windows) { 0x228833 } else { 0x2255AA };
        assert_eq!(settings.dir_color(Path::new("art/maps")), (expected, true));
    }

    #[test]
    fn directory_color_edits_in_two_windows_preserve_other_folders() {
        let directory = tempfile::tempdir().unwrap();
        let profile = Some(directory.path().join("settings.json"));
        let mut first = Settings::load_at(profile.clone());
        first.force_dir_color(Path::new("forest"), Some(0x228833));
        let mut other = Settings::load_at(profile.clone());
        first.force_dir_color(Path::new("lake"), Some(0x2255AA));
        other.force_dir_color(Path::new("forest"), None);
        assert_eq!(
            Settings::load_at(profile).value["recentDirColors"],
            json!([{"path":"lake","col":"#2255AA"}])
        );
    }

    #[test]
    fn project_history_edits_in_two_windows_preserve_each_others_entries() {
        let directory = tempfile::tempdir().unwrap();
        let profile = Some(directory.path().join("settings.json"));
        let mut first = Settings::load_at(profile.clone());
        first.remember(Path::new("one/map.ldtk"), 1);
        let mut other = Settings::load_at(profile.clone());
        first.remember(Path::new("two/map.ldtk"), 2);
        other.remember(Path::new("three/map.ldtk"), 3);
        let stored = Settings::load_at(profile.clone());
        assert_eq!(
            stored.value["recentProjects"],
            json!(["three/map.ldtk", "two/map.ldtk", "one/map.ldtk"])
        );
        assert_eq!(stored.value["recentDirs"], json!(["three", "two", "one"]));
        first.remember(Path::new("four/map.ldtk"), 4);
        other.remove_recent("recentProjects", Path::new("one/map.ldtk"));
        let stored = Settings::load_at(profile);
        assert_eq!(
            stored.value["recentProjects"],
            json!(["four/map.ldtk", "three/map.ldtk", "two/map.ldtk"])
        );
        assert_eq!(
            stored.value["lastProject"],
            json!({"filePath":"four/map.ldtk","levelUid":4})
        );
    }

    #[test]
    fn a_session_only_history_edit_does_not_overwrite_another_windows_saved_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut first = Settings::load_at(Some(path.clone()));
        first.remember(Path::new("one/map.ldtk"), 1);
        let mut other = Settings::load_at(Some(path.clone()));
        let lock = path.with_extension("lock");
        std::fs::remove_file(&lock).unwrap();
        std::fs::create_dir(&lock).unwrap();
        first.remember(Path::new("offline/map.ldtk"), 2);
        assert_eq!(
            first.most_recent_project(),
            Some(Path::new("offline/map.ldtk"))
        );
        std::fs::remove_dir(lock).unwrap();

        other.remember(Path::new("two/map.ldtk"), 3);
        first.value["grid"] = false.into();
        first.save().unwrap();
        let stored = Settings::load_at(Some(path));
        assert!(!stored.bool("grid"));
        assert_eq!(
            stored.value["recentProjects"],
            other.value["recentProjects"]
        );
        assert_eq!(stored.value["lastProject"], other.value["lastProject"]);
    }

    #[test]
    fn recent_projects_are_unique_and_retained_in_open_order() {
        let mut s = Settings::default();
        s.remember(Path::new("/a/one.ldtk"), 1);
        s.remember(Path::new("/b/two.ldtk"), 2);
        s.remember(Path::new("/a/one.ldtk"), 3);
        assert_eq!(
            s.value["recentProjects"],
            json!(["/a/one.ldtk", "/b/two.ldtk"])
        );
        assert_eq!(s.value["lastProject"]["levelUid"], 3);
    }
    #[test]
    fn project_command_trust_is_explicit_and_reversible() {
        let mut s = Settings::default();
        assert_eq!(s.trust_state("p"), None);
        s.set_trust("p", Some(true)).unwrap();
        assert_eq!(s.trust_state("p"), Some(true));
        s.set_trust("p", Some(false)).unwrap();
        assert_eq!(s.trust_state("p"), Some(false));
    }

    #[test]
    fn trust_decisions_in_another_window_survive_edits_to_a_different_project() {
        let directory = tempfile::tempdir().unwrap();
        let path = Some(directory.path().join("settings.json"));
        let mut first = Settings::load_at(path.clone());
        first.set_trust("forest", Some(true)).unwrap();
        let mut second = Settings::load_at(path.clone());
        first.set_trust("forest", None).unwrap();
        first.set_trust("lake", Some(false)).unwrap();
        second.set_trust("cave", Some(true)).unwrap();

        let saved = Settings::load_at(path);
        assert_eq!(saved.trust_state("forest"), None);
        assert_eq!(saved.trust_state("lake"), Some(false));
        assert_eq!(saved.trust_state("cave"), Some(true));
    }
}
