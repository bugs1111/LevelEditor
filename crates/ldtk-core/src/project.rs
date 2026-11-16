use crate::{Error, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LevelKey {
    pub world: Option<usize>,
    pub level: usize,
}

#[derive(Clone)]
pub struct Project {
    pub json: Value,
    pub path: Option<PathBuf>,
    pub external_levels: BTreeMap<String, Value>,
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

impl Project {
    pub fn new() -> Self {
        let mut json: Value =
            serde_json::from_str(include_str!("empty.ldtk")).expect("bundled project template");
        json["iid"] = Value::String(uuid::Uuid::new_v4().to_string());
        json["dummyWorldIid"] = Value::String(uuid::Uuid::new_v4().to_string());
        json["levels"][0]["iid"] = Value::String(uuid::Uuid::new_v4().to_string());
        Self {
            json,
            path: None,
            external_levels: BTreeMap::new(),
        }
    }

    pub fn from_json(json: Value) -> Result<Self> {
        validate_project(&json)?;
        Ok(Self {
            json,
            path: None,
            external_levels: BTreeMap::new(),
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = absolute(path.as_ref())?;
        let mut project = Self::from_json(read_json(&path)?)?;
        for key in project.level_keys() {
            if let Some(rel) = external_path(project.level_stub(key).expect("enumerated level"))? {
                if project.external_levels.contains_key(rel) {
                    return Err(Error::Invalid(format!(
                        "duplicate external level path: {rel}"
                    )));
                }
                let value = read_json(&path.parent().expect("absolute path").join(rel))?;
                if !value.is_object() {
                    return Err(Error::Invalid(format!(
                        "external level {rel} must be an object"
                    )));
                }
                project.external_levels.insert(rel.to_owned(), value);
            }
        }
        project.path = Some(path);
        Ok(project)
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        validate_project(&self.json)?;
        let path = absolute(path.as_ref())?;
        let base = path
            .parent()
            .ok_or_else(|| Error::Invalid("project path has no parent".into()))?;
        if let Some(previous) = &self.path {
            let has_external_levels = !self.external_levels.is_empty()
                || self.level_keys().iter().any(|&key| {
                    self.level_stub(key)
                        .and_then(|level| level.get("externalRelPath"))
                        .is_some_and(|path| !path.is_null())
                });
            if previous != &path && has_external_levels {
                return Err(Error::Unsupported("Save As for projects with external levels requires copying and remapping their level paths; save to the original project path".into()));
            }
            let old_base = previous
                .parent()
                .ok_or_else(|| Error::Invalid("previous project path has no parent".into()))?;
            if old_base != base
                && (relative_references(&self.json)
                    || self.external_levels.values().any(relative_references))
            {
                return Err(Error::Unsupported("Save As into another directory with relative assets or external levels requires asset rebasing; save in the original directory".into()));
            }
        }
        let mut root = self.json.clone();
        let minify = self
            .json
            .get("minifyJson")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut outputs = Vec::new();
        let mut paths = BTreeSet::from([path.clone()]);
        for key in self.level_keys() {
            if let Some(rel) = external_path(self.level_stub(key).expect("enumerated level"))? {
                let target = absolute(&base.join(rel))?;
                if !paths.insert(target.clone()) {
                    return Err(Error::Invalid(format!(
                        "duplicate output path: {}",
                        target.display()
                    )));
                }
                let level = self.external_levels.get(rel).ok_or_else(|| {
                    Error::Invalid(format!("external level {rel} has not been loaded"))
                })?;
                if !level.is_object() {
                    return Err(Error::Invalid(format!(
                        "external level {rel} must be an object"
                    )));
                }
                sync_summary(&mut root, key, level);
                outputs.push((target, encode(level, minify)?));
            }
        }
        // Publish the root last, after every referenced document has been staged.
        outputs.push((path.clone(), encode(&root, minify)?));
        atomic_save(outputs)?;
        self.json = root;
        self.path = Some(path);
        Ok(())
    }

    pub fn level_keys(&self) -> Vec<LevelKey> {
        let mut keys = Vec::new();
        if let Some(levels) = self.json.get("levels").and_then(Value::as_array) {
            keys.extend((0..levels.len()).map(|level| LevelKey { world: None, level }));
        }
        if let Some(worlds) = self.json.get("worlds").and_then(Value::as_array) {
            for (world, value) in worlds.iter().enumerate() {
                if let Some(levels) = value.get("levels").and_then(Value::as_array) {
                    keys.extend((0..levels.len()).map(|level| LevelKey {
                        world: Some(world),
                        level,
                    }));
                }
            }
        }
        keys
    }

    fn level_stub(&self, key: LevelKey) -> Option<&Value> {
        let container = match key.world {
            Some(world) => self.json.get("worlds")?.get(world)?,
            None => &self.json,
        };
        container.get("levels")?.get(key.level)
    }

    pub fn level(&self, key: LevelKey) -> Option<&Value> {
        let stub = self.level_stub(key)?;
        match stub.get("externalRelPath").and_then(Value::as_str) {
            Some(rel) => self.external_levels.get(rel),
            None => Some(stub),
        }
    }

    pub fn level_mut(&mut self, key: LevelKey) -> Option<&mut Value> {
        let rel = self
            .level_stub(key)?
            .get("externalRelPath")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(rel) = rel {
            return self.external_levels.get_mut(&rel);
        }
        let container = match key.world {
            Some(world) => self.json.get_mut("worlds")?.get_mut(world)?,
            None => &mut self.json,
        };
        container.get_mut("levels")?.get_mut(key.level)
    }

    pub fn layer(&self, key: LevelKey, layer: usize) -> Option<&Value> {
        self.level(key)?.get("layerInstances")?.get(layer)
    }
    pub fn layer_mut(&mut self, key: LevelKey, layer: usize) -> Option<&mut Value> {
        self.level_mut(key)?
            .get_mut("layerInstances")?
            .get_mut(layer)
    }
    pub fn layer_def(&self, uid: i64) -> Option<&Value> {
        self.definition("layers", uid)
    }
    pub fn tileset(&self, uid: i64) -> Option<&Value> {
        self.definition("tilesets", uid)
    }
    pub(crate) fn definition(&self, kind: &str, uid: i64) -> Option<&Value> {
        self.json
            .get("defs")?
            .get(kind)?
            .as_array()?
            .iter()
            .find(|value| value.get("uid").and_then(Value::as_i64) == Some(uid))
    }
}

fn validate_project(json: &Value) -> Result<()> {
    if !json.is_object() || (json.get("levels").is_none() && json.get("worlds").is_none()) {
        return Err(Error::Invalid(
            "expected an object with levels or worlds".into(),
        ));
    }
    if let Some(defs) = json.get("defs")
        && !defs.is_object()
    {
        return Err(Error::Invalid("defs must be an object".into()));
    }
    if let Some(levels) = json.get("levels") {
        validate_levels(levels)?;
    }
    if let Some(worlds) = json.get("worlds") {
        for world in worlds
            .as_array()
            .ok_or_else(|| Error::Invalid("worlds must be an array".into()))?
        {
            if !world.is_object() {
                return Err(Error::Invalid("world must be an object".into()));
            }
            validate_levels(
                world
                    .get("levels")
                    .ok_or_else(|| Error::Invalid("world has no levels".into()))?,
            )?;
        }
    }
    Ok(())
}
fn validate_levels(levels: &Value) -> Result<()> {
    for level in levels
        .as_array()
        .ok_or_else(|| Error::Invalid("levels must be an array".into()))?
    {
        if !level.is_object() {
            return Err(Error::Invalid("level must be an object".into()));
        }
        external_path(level)?;
    }
    Ok(())
}
fn external_path(level: &Value) -> Result<Option<&str>> {
    match level.get("externalRelPath") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(path)) if !path.is_empty() && !Path::new(path).is_absolute() => {
            Ok(Some(path))
        }
        _ => Err(Error::Invalid(
            "externalRelPath must be a nonempty relative path or null".into(),
        )),
    }
}
fn relative_references(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let file_field = object
                .get("__type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.contains("FilePath"))
                || object.get("type").and_then(Value::as_str) == Some("F_Path");
            let field_reference = file_field
                && [
                    "__value",
                    "realEditorValues",
                    "realEditorValue",
                    "defaultOverride",
                ]
                .iter()
                .any(|key| object.get(*key).is_some_and(relative_path_value));
            field_reference
                || object.iter().any(|(key, value)| {
                    ((key.ends_with("RelPath") || key == "relPath")
                        && value
                            .as_str()
                            .is_some_and(|s| !s.is_empty() && !Path::new(s).is_absolute()))
                        || relative_references(value)
                })
        }
        Value::Array(values) => values.iter().any(relative_references),
        _ => false,
    }
}
fn absolute(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?
            .join(path)
    };
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}
fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}
fn encode(value: &Value, minify: bool) -> Result<Vec<u8>> {
    let mut bytes = if minify {
        serde_json::to_vec(value)
    } else {
        serde_json::to_vec_pretty(value)
    }
    .map_err(|e| Error::Invalid(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

struct StagedFile {
    target: PathBuf,
    staged: PathBuf,
    original: Option<Vec<u8>>,
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.staged);
    }
}
fn stage(target: PathBuf, bytes: &[u8]) -> Result<StagedFile> {
    let original = match fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(Error::Io {
                path: target,
                source,
            });
        }
    };
    let staged = target.with_file_name(format!(".ldtk-{}.tmp", uuid::Uuid::new_v4()));
    let record = StagedFile {
        target,
        staged,
        original,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&record.staged)
        .map_err(|source| Error::Io {
            path: record.staged.clone(),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| Error::Io {
            path: record.staged.clone(),
            source,
        })?;
    if let Ok(metadata) = fs::metadata(&record.target) {
        fs::set_permissions(&record.staged, metadata.permissions()).map_err(|source| {
            Error::Io {
                path: record.staged.clone(),
                source,
            }
        })?;
    }
    Ok(record)
}
fn atomic_save(outputs: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    let files: Vec<StagedFile> = outputs
        .into_iter()
        .map(|(path, bytes)| stage(path, &bytes))
        .collect::<Result<_>>()?;
    for index in 0..files.len() {
        if let Err(source) = fs::rename(&files[index].staged, &files[index].target) {
            let failure = Error::Io {
                path: files[index].target.clone(),
                source,
            };
            for file in files[..index].iter().rev() {
                let rollback = match &file.original {
                    Some(bytes) => stage(file.target.clone(), bytes).and_then(|rollback| {
                        fs::rename(&rollback.staged, &rollback.target).map_err(|source| Error::Io {
                            path: rollback.target.clone(),
                            source,
                        })
                    }),
                    None => fs::remove_file(&file.target).map_err(|source| Error::Io {
                        path: file.target.clone(),
                        source,
                    }),
                };
                if let Err(error) = rollback {
                    return Err(Error::Invalid(format!(
                        "save failed ({failure}); rollback also failed ({error}); inspect project files"
                    )));
                }
            }
            return Err(failure);
        }
    }
    Ok(())
}

fn sync_summary(root: &mut Value, key: LevelKey, external: &Value) {
    let container = match key.world {
        Some(world) => &mut root["worlds"][world],
        None => root,
    };
    let stub = &mut container["levels"][key.level];
    for field in [
        "identifier",
        "iid",
        "uid",
        "worldX",
        "worldY",
        "worldDepth",
        "pxWid",
        "pxHei",
        "__bgColor",
        "bgColor",
        "useAutoIdentifier",
        "bgRelPath",
        "bgPos",
        "bgPivotX",
        "bgPivotY",
        "__smartColor",
        "__bgPos",
        "fieldInstances",
        "__neighbours",
    ] {
        if let Some(value) = external.get(field) {
            stub[field] = value.clone();
        }
    }
}

fn relative_path_value(value: &Value) -> bool {
    match value {
        Value::String(path) => !path.is_empty() && !Path::new(path).is_absolute(),
        Value::Array(values) => values.iter().any(relative_path_value),
        Value::Object(object) => object.get("params").is_some_and(relative_path_value),
        _ => false,
    }
}
