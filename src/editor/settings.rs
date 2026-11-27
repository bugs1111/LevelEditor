use serde_json::{Value, json};
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
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            path: None,
            value: json!({
                "zenMode":false,"grid":true,"emptySpaceSelection":true,"tileStacking":true,
                "tileEnumOverlays":false,"showDetails":true,"useBestGPU":true,"startFullScreen":false,
                "colorBlind":false,"blurMask":true,"navigationKeys":"Wasd","singleLayerMode":false,"language":"en",
                "singleLayerModeIntensity":0.75,"openLastProject":false,"lastProject":null,
                "autoWorldModeSwitch":"ZoomInAndOut","fieldsRender":"FR_Outline",
                "nearbyTilesRenderingDist":1.0,"appUiScale":1.0,"editorUiScale":1.0,"mouseWheelSpeed":1.0,
                "recentProjects":[],"recentDirs":[],"recentDirColors":[],"uiStates":[],"lastUiDirs":[],"projectTrusts":[]
            }),
        }
    }
}
impl Settings {
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
            ..Self::default()
        };
        // A fresh profile follows the desktop. The saved value can override it below.
        settings.value["language"] = "system".into();
        if let Some(path) = &settings.path
            && let Ok(bytes) = std::fs::read(path).or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound
                    && let Some(previous) = previous
                {
                    std::fs::read(previous)
                } else {
                    Err(error)
                }
            })
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && let Some(object) = value.as_object()
        {
            for (k, v) in object {
                settings.value[k] = v.clone();
            }
        }
        settings
    }
    pub(super) fn language(&self) -> super::locale::Language {
        super::locale::Language::from_code(self.value["language"].as_str())
    }
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.value)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, path)
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
    pub fn dir_color(&self, path: &Path) -> u32 {
        let path = path.to_string_lossy().replace('\\', "/");
        if let Some(color) = self.value["recentDirColors"]
            .as_array()
            .and_then(|colors| colors.iter().find(|color| color["path"] == path))
            .and_then(|color| color["col"].as_str())
            .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
        {
            return color;
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
        palette[sum % palette.len()]
    }
    pub fn force_dir_color(&mut self, path: &Path, color: Option<u32>) {
        let path = path.to_string_lossy().replace('\\', "/");
        if !self.value["recentDirColors"].is_array() {
            self.value["recentDirColors"] = json!([]);
        }
        let colors = self.value["recentDirColors"].as_array_mut().unwrap();
        colors.retain(|color| color["path"] != path);
        if let Some(color) = color {
            colors.push(json!({"path":path,"col":format!("#{color:06X}")}));
        }
        let _ = self.save();
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
        let name = path.to_string_lossy().into_owned();
        let recent = self.value["recentProjects"].as_array_mut().unwrap();
        recent.retain(|p| p.as_str() != Some(&name));
        recent.insert(0, json!(name));
        recent.truncate(30);
        if let Some(dir) = path.parent() {
            let dir = dir.to_string_lossy().into_owned();
            let dirs = self.value["recentDirs"].as_array_mut().unwrap();
            dirs.retain(|p| p.as_str() != Some(&dir));
            dirs.insert(0, json!(dir));
            dirs.truncate(20);
        }
        self.value["lastProject"] = json!({"filePath":name,"levelUid":level_uid});
        let _ = self.save();
    }
    pub fn trust_state(&self, iid: &str) -> Option<bool> {
        self.value["projectTrusts"]
            .as_array()?
            .iter()
            .find(|p| p["iid"] == iid)?["trusted"]
            .as_bool()
    }
    pub fn set_trusted(&mut self, iid: &str, trusted: bool) {
        let list = self.value["projectTrusts"].as_array_mut().unwrap();
        list.retain(|p| p["iid"] != iid);
        list.push(json!({"iid":iid,"trusted":trusted}));
        let _ = self.save();
    }
    pub fn clear_trust(&mut self, iid: &str) {
        self.value["projectTrusts"]
            .as_array_mut()
            .unwrap()
            .retain(|p| p["iid"] != iid);
        let _ = self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{RawInput, Rect, ViewportId, pos2, vec2};

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
        let original = br#"{"language":"zh-CN","grid":false,"recentProjects":["map.ldtk"]}"#;
        std::fs::write(&previous, original).unwrap();

        let mut settings = Settings::load_default(Some(&home), Some(&appdata));
        assert_eq!(settings.path.as_deref(), Some(current.as_path()));
        assert_eq!(settings.value["language"], "zh-CN");
        assert_eq!(settings.value["recentProjects"], json!(["map.ldtk"]));
        assert!(!settings.bool("grid"));
        settings.value["grid"] = true.into();
        settings.save().unwrap();
        assert!(current.is_file());
        assert_eq!(std::fs::read(previous).unwrap(), original);
        assert!(Settings::load_default(Some(&home), Some(&appdata)).bool("grid"));
    }

    #[test]
    fn settings_default_to_system_language_and_preserve_explicit_preferences() {
        use crate::app::locale::Language;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let settings = Settings::load_at(Some(path.clone()));
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
        assert_eq!(s.dir_color(Path::new("/a")), 0x123456);
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
        s.set_trusted("p", true);
        assert_eq!(s.trust_state("p"), Some(true));
        s.set_trusted("p", false);
        assert_eq!(s.trust_state("p"), Some(false));
    }
}
