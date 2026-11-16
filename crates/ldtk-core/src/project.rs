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
        self.save_document(path, true)
    }
    fn save_document(&mut self, path: impl AsRef<Path>, make_backup: bool) -> Result<()> {
        validate_project(&self.json)?;
        let path = absolute(path.as_ref())?;
        let base = path
            .parent()
            .ok_or_else(|| Error::Invalid("project path has no parent".into()))?;
        let retired = read_json(&path)
            .ok()
            .and_then(|json| Self::from_json(json).ok())
            .map(|saved| {
                saved
                    .level_keys()
                    .into_iter()
                    .filter_map(|key| {
                        saved
                            .level_stub(key)
                            .and_then(|level| level["externalRelPath"].as_str())
                            .map(str::to_owned)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if backup_file(&path, self.json["iid"].as_str().unwrap_or("")) {
            return Err(Error::Unsupported(
                "backup files are read-only; restore the backup or save as a new project".into(),
            ));
        }
        let mut candidate = self.clone();
        if let Some(previous) = &self.path
            && previous != &path
        {
            let old_base = previous.parent().expect("absolute project path");
            if old_base != base {
                rebase_paths(&mut candidate.json, old_base, base)?;
                for level in candidate.external_levels.values_mut() {
                    rebase_paths(level, old_base, base)?;
                }
            }
            let mut levels = BTreeMap::new();
            for (idx, key) in candidate.level_keys().into_iter().enumerate() {
                let old = candidate
                    .level_stub(key)
                    .and_then(|v| v["externalRelPath"].as_str())
                    .map(str::to_owned);
                if let Some(old) = old {
                    let level = candidate
                        .external_levels
                        .get(&old)
                        .cloned()
                        .ok_or_else(|| {
                            Error::Invalid(format!("external level {old} has not been loaded"))
                        })?;
                    let id = level["identifier"].as_str().unwrap_or("Level");
                    if id.contains('/') || id.contains('\\') || id == "." || id == ".." {
                        return Err(Error::Invalid("unsafe external level identifier".into()));
                    }
                    let indexed = candidate.json["flags"].as_array().is_some_and(|flags| {
                        flags.iter().any(|f| f == "PrependIndexToLevelFileNames")
                    });
                    let rel = format!(
                        "{}/{}{}.ldtkl",
                        path.file_stem().unwrap_or_default().to_string_lossy(),
                        if indexed {
                            format!("{idx:04}-")
                        } else {
                            String::new()
                        },
                        id
                    );
                    if levels.contains_key(&rel) {
                        return Err(Error::Invalid(format!(
                            "duplicate external level path: {rel}"
                        )));
                    }
                    candidate.level_stub_mut(key).expect("enumerated level")["externalRelPath"] =
                        Value::String(rel.clone());
                    levels.insert(rel, level);
                }
            }
            candidate.external_levels = levels;
        }
        if candidate.json["externalLevels"].as_bool().unwrap_or(false) {
            let mut external = BTreeMap::new();
            for (idx, key) in candidate.level_keys().into_iter().enumerate() {
                let old_rel = candidate
                    .level_stub(key)
                    .and_then(|v| v["externalRelPath"].as_str())
                    .map(str::to_owned);
                let mut level = candidate
                    .level(key)
                    .ok_or_else(|| Error::Invalid("external level has not been loaded".into()))?
                    .clone();
                let id = level["identifier"].as_str().unwrap_or("Level");
                if id.contains('/') || id.contains('\\') || id == "." || id == ".." {
                    return Err(Error::Invalid("unsafe external level identifier".into()));
                }
                let indexed = candidate.json["flags"]
                    .as_array()
                    .is_some_and(|flags| flags.iter().any(|f| f == "PrependIndexToLevelFileNames"));
                let rel = if self.path.is_none()
                    && let Some(old_rel) = old_rel
                {
                    old_rel
                } else {
                    format!(
                        "{}/{}{}.ldtkl",
                        path.file_stem().unwrap_or_default().to_string_lossy(),
                        if indexed {
                            format!("{idx:04}-")
                        } else {
                            String::new()
                        },
                        id
                    )
                };
                if external.contains_key(&rel) {
                    return Err(Error::Invalid(format!(
                        "duplicate external level path: {rel}"
                    )));
                }
                let stub = candidate.level_stub_mut(key).expect("enumerated level");
                stub["externalRelPath"] = Value::String(rel.clone());
                stub["layerInstances"] = Value::Null;
                if self.path.is_some() {
                    level["externalRelPath"] = Value::Null;
                }
                external.insert(rel, level);
            }
            candidate.external_levels = external;
        }
        let mut root = candidate.json.clone();
        let minify = self
            .json
            .get("minifyJson")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut outputs = Vec::new();
        let mut paths = BTreeSet::from([path.clone()]);
        for key in candidate.level_keys() {
            if let Some(rel) = external_path(candidate.level_stub(key).expect("enumerated level"))?
            {
                let target = absolute(&base.join(rel))?;
                if !paths.insert(target.clone()) {
                    return Err(Error::Invalid(format!(
                        "duplicate output path: {}",
                        target.display()
                    )));
                }
                let level = candidate.external_levels.get(rel).ok_or_else(|| {
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
        if make_backup && candidate.json["backupOnSave"].as_bool().unwrap_or(false) {
            let mut backup = candidate.clone();
            backup.path = Some(path.clone());
            backup.backup()?;
        }
        // Create external document folders, but keep a missing project directory an error.
        for (target, _) in &outputs {
            if target != &path
                && target.starts_with(base.join(path.file_stem().unwrap_or_default()))
            {
                fs::create_dir_all(target.parent().expect("absolute output path")).map_err(
                    |source| Error::Io {
                        path: target.clone(),
                        source,
                    },
                )?;
            }
        }
        atomic_save(outputs)?;
        self.external_levels = candidate.external_levels;
        self.json = root;
        let document_dir = base.join(path.file_stem().unwrap_or_default());
        let published_real_paths = paths
            .iter()
            .filter_map(|path| fs::canonicalize(path).ok())
            .collect::<BTreeSet<_>>();
        #[cfg(unix)]
        let published_files = paths
            .iter()
            .filter_map(|path| file_identity(path))
            .collect::<BTreeSet<_>>();
        for rel in retired {
            let target = absolute(&base.join(rel))?;
            let live_alias = fs::canonicalize(&target)
                .ok()
                .is_some_and(|real| published_real_paths.contains(&real));
            #[cfg(unix)]
            let live_alias = live_alias
                || file_identity(&target).is_some_and(|id| published_files.contains(&id));
            // Case-only renames on case-insensitive volumes still resolve the retired
            // spelling to the newly published file. Never unlink a live output alias.
            if target.starts_with(&document_dir) && !paths.contains(&target) && !live_alias {
                let _ = fs::remove_file(target);
            }
        }
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

    fn level_stub_mut(&mut self, key: LevelKey) -> Option<&mut Value> {
        let container = match key.world {
            Some(world) => self.json.get_mut("worlds")?.get_mut(world)?,
            None => &mut self.json,
        };
        container.get_mut("levels")?.get_mut(key.level)
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
pub(crate) fn absolute(path: &Path) -> Result<PathBuf> {
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
pub(crate) fn read_json(path: &Path) -> Result<Value> {
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
pub(crate) fn atomic_save(outputs: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
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
    #[cfg(unix)]
    for file in &files {
        if let Some(parent) = file.target.parent() {
            // Windows cannot open directory handles this way; file sync and rename still apply.
            fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|source| Error::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }
    }
    Ok(())
}

pub(crate) fn sync_summary(root: &mut Value, key: LevelKey, external: &Value) {
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

/// A content snapshot, independent of the mutable project, for filesystem watchers.
#[derive(Clone, Debug)]
pub struct FilesystemSnapshot {
    files: BTreeMap<PathBuf, Option<u64>>,
}
impl FilesystemSnapshot {
    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        for (path, fingerprint) in &self.files {
            if &file_fingerprint(path)? != fingerprint {
                changed.push(path.clone());
            }
        }
        Ok(changed)
    }
}
impl Project {
    pub fn filesystem_snapshot(&self) -> Result<FilesystemSnapshot> {
        let mut paths = BTreeSet::new();
        if let Some(path) = &self.path {
            paths.insert(path.clone());
            let base = path.parent().expect("absolute project path");
            collect_resource_paths(&self.json, base, &mut paths)?;
            for level in self.external_levels.values() {
                collect_resource_paths(level, base, &mut paths)?;
            }
        }
        let files = paths
            .into_iter()
            .map(|path| Ok((path.clone(), file_fingerprint(&path)?)))
            .collect::<Result<_>>()?;
        Ok(FilesystemSnapshot { files })
    }
    /// Copy the currently saved project and external documents, retaining backupLimit copies.
    pub fn backup(&self) -> Result<Option<PathBuf>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let base = path.parent().expect("absolute project path");
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let directory = match self.json["backupRelPath"].as_str() {
            Some(rel) => absolute(&base.join(rel))?,
            None => base.join(stem.as_ref()).join("backups"),
        };
        let id = self.json["iid"].as_str().unwrap_or("backup");
        if id.contains('/') || id.contains('\\') {
            return Err(Error::Invalid("unsafe backup project IID".into()));
        }
        let name = format!("{}_{}", id, backup_timestamp());
        let mut destination = directory.join(&name);
        let mut suffix = 2;
        while destination.exists() {
            destination = directory.join(format!("{name}_{suffix:04}"));
            suffix += 1;
        }
        fs::create_dir_all(&destination).map_err(|source| Error::Io {
            path: destination.clone(),
            source,
        })?;
        let original = fs::read(path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let saved_json = serde_json::from_slice::<Value>(&original)
            .ok()
            .and_then(|json| Project::from_json(json).ok());
        let corrupt = saved_json.is_none();
        let mut saved = saved_json.unwrap_or_else(|| self.clone());
        let mut outputs = Vec::new();
        let mut remapped = false;
        if corrupt {
            let old_dir = base.join(stem.as_ref());
            if old_dir.is_dir() {
                for entry in fs::read_dir(&old_dir).map_err(|source| Error::Io {
                    path: old_dir.clone(),
                    source,
                })? {
                    let entry = entry.map_err(|source| Error::Io {
                        path: old_dir.clone(),
                        source,
                    })?;
                    let source = entry.path();
                    if entry.file_type().is_ok_and(|t| t.is_file())
                        && source.extension().is_some_and(|e| e == "ldtkl")
                    {
                        let target = destination.join(stem.as_ref()).join(entry.file_name());
                        fs::create_dir_all(target.parent().unwrap()).map_err(|source| {
                            Error::Io {
                                path: target.clone(),
                                source,
                            }
                        })?;
                        outputs.push((
                            target,
                            fs::read(&source).map_err(|error| Error::Io {
                                path: source,
                                source: error,
                            })?,
                        ));
                    }
                }
            }
        }
        for (index, key) in if corrupt { vec![] } else { saved.level_keys() }
            .into_iter()
            .enumerate()
        {
            if let Some(rel) =
                external_path(saved.level_stub(key).expect("enumerated level"))?.map(str::to_owned)
            {
                let source = absolute(&base.join(&rel))?;
                let relative = if Path::new(&rel)
                    .components()
                    .any(|c| c == Component::ParentDir)
                {
                    remapped = true;
                    let rel = format!(
                        "{stem}/external-{index:04}-{}",
                        source.file_name().unwrap_or_default().to_string_lossy()
                    );
                    saved.level_stub_mut(key).expect("enumerated level")["externalRelPath"] =
                        Value::String(rel.clone());
                    rel
                } else {
                    rel
                };
                let target = destination.join(relative);
                fs::create_dir_all(target.parent().expect("backup file parent")).map_err(
                    |source| Error::Io {
                        path: target.clone(),
                        source,
                    },
                )?;
                outputs.push((
                    target,
                    fs::read(&source).map_err(|source_error| Error::Io {
                        path: source,
                        source: source_error,
                    })?,
                ));
            }
        }
        outputs.push((
            destination.join(path.file_name().expect("project filename")),
            if remapped {
                encode(&saved.json, saved.json["minifyJson"] == true)?
            } else {
                original
            },
        ));
        atomic_save(outputs)?;
        let mut backups = self.list_backups()?;
        let mut directories = BTreeSet::new();
        backups.retain(|backup| directories.insert(backup.path.parent().unwrap().to_path_buf()));
        let limit = self.json["backupLimit"].as_u64().unwrap_or(10) as usize;
        for backup in backups.into_iter().skip(limit) {
            let old = backup.path.parent().unwrap().to_path_buf();
            fs::remove_dir_all(&old).map_err(|source| Error::Io { path: old, source })?;
        }
        Ok(Some(destination))
    }
}
fn file_fingerprint(path: &Path) -> Result<Option<u64>> {
    use std::hash::{Hash, Hasher};
    match fs::read(path) {
        Ok(bytes) => {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hash);
            Ok(Some(hash.finish()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}
fn collect_resource_paths(value: &Value, base: &Path, paths: &mut BTreeSet<PathBuf>) -> Result<()> {
    match value {
        Value::Object(object) => {
            let file = object
                .get("__type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains("FilePath"))
                || object
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t == "F_Path" || t == "F_File");
            for (key, value) in object {
                if key != "backupRelPath"
                    && matches!(
                        key.as_str(),
                        "relPath" | "externalRelPath" | "bgRelPath" | "__tilesetRelPath"
                    )
                    && value.as_str().is_some_and(|s| !s.is_empty())
                {
                    paths.insert(absolute(&base.join(value.as_str().unwrap()))?);
                }
                if file
                    && [
                        "__value",
                        "realEditorValues",
                        "realEditorValue",
                        "defaultOverride",
                    ]
                    .contains(&key.as_str())
                {
                    collect_field_paths(value, base, paths)?;
                }
                collect_resource_paths(value, base, paths)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_resource_paths(value, base, paths)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn collect_field_paths(value: &Value, base: &Path, paths: &mut BTreeSet<PathBuf>) -> Result<()> {
    match value {
        Value::String(s) if !s.is_empty() => {
            paths.insert(absolute(&base.join(s))?);
        }
        Value::Array(v) => {
            for v in v {
                collect_field_paths(v, base, paths)?;
            }
        }
        Value::Object(o) => {
            if let Some(v) = o.get("params") {
                collect_field_paths(v, base, paths)?;
            }
        }
        _ => (),
    }
    Ok(())
}
pub(crate) fn relative_to(path: &Path, base: &Path) -> Result<String> {
    let path = absolute(path)?;
    let base = absolute(base)?;
    let p: Vec<_> = path.components().collect();
    let b: Vec<_> = base.components().collect();
    let common = p.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &p[common..] {
        out.push(c.as_os_str());
    }
    Ok(out.to_string_lossy().replace('\\', "/"))
}
pub(crate) fn rebase_paths(value: &mut Value, old: &Path, new: &Path) -> Result<()> {
    match value {
        Value::Object(o) => {
            let is_level = o.contains_key("layerInstances") && o.contains_key("identifier");
            let file = o
                .get("__type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains("FilePath"))
                || o.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t == "F_Path" || t == "F_File");
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "relPath" | "externalRelPath" | "bgRelPath" | "__tilesetRelPath"
                ) && !(is_level && k == "externalRelPath")
                {
                    rebase_string(v, old, new)?;
                } else if file
                    && [
                        "__value",
                        "realEditorValues",
                        "realEditorValue",
                        "defaultOverride",
                    ]
                    .contains(&k.as_str())
                {
                    rebase_field(v, old, new)?;
                } else {
                    rebase_paths(v, old, new)?;
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                rebase_paths(v, old, new)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn asset_is_absolute(path: &str) -> bool {
    Path::new(path).is_absolute()
        || path.starts_with("\\\\")
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && matches!(path.as_bytes()[2], b'/' | b'\\'))
}
fn rebase_string(v: &mut Value, old: &Path, new: &Path) -> Result<()> {
    if let Some(s) = v.as_str()
        && !s.is_empty()
        && !asset_is_absolute(s)
    {
        *v = Value::String(relative_to(&old.join(s), new)?);
    }
    Ok(())
}
fn rebase_field(v: &mut Value, old: &Path, new: &Path) -> Result<()> {
    match v {
        Value::String(_) => rebase_string(v, old, new)?,
        Value::Array(a) => {
            for v in a {
                rebase_field(v, old, new)?;
            }
        }
        Value::Object(o) => {
            if let Some(v) = o.get_mut("params") {
                rebase_field(v, old, new)?;
            }
        }
        _ => (),
    }
    Ok(())
}
fn same_existing_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    if file_identity(a)
        .zip(file_identity(b))
        .is_some_and(|(a, b)| a == b)
    {
        return true;
    }
    fs::canonicalize(a)
        .ok()
        .zip(fs::canonicalize(b).ok())
        .is_some_and(|(a, b)| a == b)
}
#[cfg(unix)]
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.dev(), metadata.ino()))
}
fn backup_timestamp() -> String {
    // Use the same local calendar as the original Date-based backup names.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    #[cfg(any(unix, windows))]
    if let Some(local) = recovery::local_backup_timestamp(secs) {
        return local;
    }
    let z = (secs / 86400) as i64 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}",
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60
    )
}

#[derive(Clone, Debug, Default)]
pub struct SaveReport {
    pub outputs: Vec<PathBuf>,
    pub export_errors: Vec<String>,
}
impl Project {
    /// Save the document first, then attempt configured exports independently.
    /// Export diagnostics never disguise a successfully published document as an unsaved one.
    pub fn save_with_exports(&mut self, path: impl AsRef<Path>) -> Result<SaveReport> {
        self.save(path)?;
        Ok(self.configured_exports_report())
    }
    pub fn configured_exports_report(&self) -> SaveReport {
        let Some(path) = self.path.as_ref() else {
            return SaveReport::default();
        };
        let dir = path
            .parent()
            .expect("absolute path")
            .join(path.file_stem().unwrap_or_default());
        let mut report = SaveReport {
            outputs: vec![path.clone()],
            export_errors: Vec::new(),
        };
        for rel in self.external_levels.keys() {
            report
                .outputs
                .push(path.parent().expect("absolute project path").join(rel));
        }
        let simple = dir.join("simplified");
        let png = dir.join("png");
        let tiled = dir.join("tiled");
        let wrap = |result: Result<Vec<PathBuf>>| {
            result.map(|outputs| crate::exports::ExportReport {
                outputs,
                warnings: vec![],
            })
        };
        if self.json["simplifiedExport"] == true {
            collect_configured_export(
                &mut report,
                &simple,
                &["json", "csv", "png"],
                wrap(self.export_simplified(&simple)),
            );
            collect_configured_export(&mut report, &png, &["png"], wrap(Ok(vec![])));
        } else {
            collect_configured_export(
                &mut report,
                &simple,
                &["json", "csv", "png"],
                wrap(Ok(vec![])),
            );
            let outputs = if self.json["imageExportMode"]
                .as_str()
                .is_some_and(|mode| mode != "None")
            {
                self.export_png(&png)
            } else {
                Ok(vec![])
            };
            collect_configured_export(&mut report, &png, &["png"], wrap(outputs));
        }
        let output = if self.json["exportTiled"] == true {
            self.export_tiled_with_report(&tiled)
        } else {
            wrap(Ok(vec![]))
        };
        collect_configured_export(&mut report, &tiled, &["tmx", "world", "png"], output);
        collect_configured_export(&mut report, &dir.join("gms2"), &["yy"], wrap(Ok(vec![])));
        report
    }
    /// Convert external level storage without changing any document values.
    pub fn set_external_levels(&mut self, enabled: bool) -> Result<()> {
        let mut candidate = self.clone();
        if !enabled {
            for key in candidate.level_keys() {
                let mut level = candidate
                    .level(key)
                    .ok_or_else(|| Error::Invalid("external level not loaded".into()))?
                    .clone();
                level["externalRelPath"] = Value::Null;
                *candidate.level_stub_mut(key).expect("enumerated level") = level;
            }
            candidate.external_levels.clear();
        }
        candidate.json["externalLevels"] = Value::Bool(enabled);
        *self = candidate;
        Ok(())
    }
    pub fn save_if_unchanged(
        &mut self,
        path: impl AsRef<Path>,
        snapshot: &FilesystemSnapshot,
    ) -> Result<()> {
        let changed = snapshot.changed_files()?;
        if !changed.is_empty() {
            return Err(Error::Invalid(format!(
                "files changed outside the editor: {}",
                changed
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        self.save(path)
    }
}

#[path = "project_migrate.rs"]
mod migrate;

fn valid_backup_name(name: &str, id: &str) -> bool {
    let Some(date) = name.strip_prefix(&format!("{id}_")) else {
        return false;
    };
    if date.len() < 19 || !date.as_bytes()[..19].is_ascii() {
        return false;
    }
    let suffix = &date[19..];
    let suffix = suffix.strip_prefix("_crash").unwrap_or(suffix);
    if !suffix.is_empty()
        && !(suffix.len() == 5
            && suffix.starts_with('_')
            && suffix[1..].bytes().all(|c| c.is_ascii_digit()))
    {
        return false;
    }
    date[..19].bytes().enumerate().all(|(i, c)| match i {
        4 | 7 | 13 | 16 => c == b'-',
        10 => c == b'_',
        _ => c.is_ascii_digit(),
    })
}
fn backup_file(path: &Path, id: &str) -> bool {
    path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .is_some_and(|name| valid_backup_name(name, id) || valid_backup_name(name, "backup"))
}
#[path = "project_crash.rs"]
mod crash;
#[path = "project_backup.rs"]
mod recovery;
pub use recovery::BackupInfo;
impl Project {
    pub fn make_absolute_file_path(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        if asset_is_absolute(&path.to_string_lossy()) {
            return Ok(path.to_path_buf());
        }
        let base = match self.path.as_ref().and_then(|p| p.parent()) {
            Some(base) => base.to_path_buf(),
            None => std::env::current_dir().map_err(|source| Error::Io {
                path: path.into(),
                source,
            })?,
        };
        absolute(&base.join(path))
    }
    pub fn make_relative_file_path(&self, path: impl AsRef<Path>) -> Result<String> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Ok(path.to_string_lossy().replace('\\', "/"));
        }
        let base = self.path.as_ref().and_then(|p| p.parent()).ok_or_else(|| {
            Error::Invalid("save the project before making resource paths relative".into())
        })?;
        relative_to(path, base)
    }
}

fn collect_configured_export(
    report: &mut SaveReport,
    dir: &Path,
    extensions: &[&str],
    result: Result<crate::exports::ExportReport>,
) {
    match result {
        Ok(output) => {
            if let Err(error) = prune_managed_exports(dir, &output.outputs, extensions) {
                report.export_errors.push(error.to_string());
            }
            report.outputs.extend(output.outputs);
            report.export_errors.extend(output.warnings);
        }
        Err(error) => report.export_errors.push(error.to_string()),
    }
}
fn prune_managed_exports(dir: &Path, keep: &[PathBuf], extensions: &[&str]) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.into(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.into(),
            source,
        })?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        if kind.is_dir() {
            prune_managed_exports(&path, keep, extensions)?;
        } else if kind.is_file()
            && !keep.contains(&path)
            && !keep.iter().any(|live| same_existing_file(&path, live))
            && path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| extensions.contains(&ext))
        {
            fs::remove_file(&path).map_err(|source| Error::Io { path, source })?;
        }
    }
    if fs::read_dir(dir)
        .map_err(|source| Error::Io {
            path: dir.into(),
            source,
        })?
        .next()
        .is_none()
    {
        fs::remove_dir(dir).map_err(|source| Error::Io {
            path: dir.into(),
            source,
        })?;
    }
    Ok(())
}
