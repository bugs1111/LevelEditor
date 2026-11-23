use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) struct Settings {
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
    pub fn load() -> Self {
        let root = std::env::var_os("LDTK_CONFIG_DIR").map(PathBuf::from).or_else(|| {
            let legacy = std::env::var_os("HOME").map(|home| {
                let home = PathBuf::from(home);
                if cfg!(target_os = "macos") {
                    home.join("Library/Application Support/LDtk Rust")
                } else {
                    home.join(".config/ldtk-rust")
                }
            });
            if cfg!(windows)
                && !legacy.as_ref().is_some_and(|p| p.join("settings.json").is_file())
                && let Some(appdata) = std::env::var_os("APPDATA")
            {
                return Some(PathBuf::from(appdata).join("LDtk Rust"));
            }
            legacy
        });
        Self::load_at(root.map(|p| p.join("settings.json")))
    }

    pub(super) fn load_at(path: Option<PathBuf>) -> Self {
        let mut settings = Self {
            path,
            ..Self::default()
        };
        if let Some(path) = &settings.path
            && let Ok(bytes) = std::fs::read(path)
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
    pub fn trusted(&self, iid: &str) -> bool {
        self.value["projectTrusts"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|p| p["iid"] == iid && p["trusted"] == true)
        })
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
        assert!(!s.trusted("p"));
        s.set_trusted("p", true);
        assert!(s.trusted("p"));
        s.set_trusted("p", false);
        assert!(!s.trusted("p"));
    }
}
