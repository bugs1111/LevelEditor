use crate::grid_presence::GridPresences;
use crate::{Error, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, BufRead, BufReader, BufWriter, Read, Seek, Write},
    ops::Index,
    path::{Component, Path, PathBuf},
    sync::{Arc, LazyLock, Weak},
};

mod levels;
mod neighbours;
mod snapshot;
mod toc;
pub use levels::{EntityMatch, LevelKey};
pub use snapshot::FilesystemSnapshot;
use snapshot::bytes_fingerprint;

/// External documents share data between project snapshots until edited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalLevels(BTreeMap<String, Arc<Value>>);

impl ExternalLevels {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &String> {
        self.0.keys()
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.0.values().map(Arc::as_ref)
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Value> {
        self.0.values_mut().map(Arc::make_mut)
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        self.0.get(path).map(Arc::as_ref)
    }

    pub fn get_mut(&mut self, path: &str) -> Option<&mut Value> {
        self.0.get_mut(path).map(Arc::make_mut)
    }

    pub fn insert(&mut self, path: String, level: Value) -> Option<Value> {
        self.0
            .insert(path, Arc::new(level))
            .map(Arc::unwrap_or_clone)
    }

    pub fn remove(&mut self, path: &str) -> Option<Value> {
        self.0.remove(path).map(Arc::unwrap_or_clone)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    // Keep the saved revision when a refresh leaves the document unchanged.
    pub(crate) fn reuse_equal_documents(&mut self, previous: &Self) {
        for (path, level) in &mut self.0 {
            if let Some(old) = previous.0.get(path)
                && !Arc::ptr_eq(level, old)
                && level.as_ref() == old.as_ref()
            {
                *level = Arc::clone(old);
            }
        }
    }
}

impl Index<&str> for ExternalLevels {
    type Output = Value;

    fn index(&self, path: &str) -> &Value {
        &self.0[path]
    }
}

#[derive(Clone)]
pub struct Project {
    pub json: Value,
    pub path: Option<PathBuf>,
    pub external_levels: ExternalLevels,
    pub(crate) grid_presence: GridPresences,
    external_level_cache: Option<Arc<ExternalLevelCache>>,
    repaired_external_paths: bool,
}

struct ExternalLevelCache {
    settings: [Value; 3],
    fingerprints: BTreeMap<PathBuf, ExternalLevelFingerprint>,
}

struct ExternalLevelFingerprint {
    length: usize,
    bytes: u64,
    // Only a value parsed from these bytes can be reused when reopening a save.
    parsed: bool,
    // A write via Arc::make_mut detaches this reference. The cache never owns a level.
    level: Weak<Value>,
}

#[derive(Default)]
struct PublishedDocuments {
    root: Vec<u8>,
    // These levels were parsed from the same bytes that the save left on disk.
    reusable_levels: BTreeMap<PathBuf, (Arc<Value>, usize, u64)>,
}

enum SaveVerification<'a> {
    DocumentDestinations(&'a FilesystemSnapshot),
    AllWatchedFiles(&'a FilesystemSnapshot),
}

fn external_level_save_settings(root: &Value) -> [&Value; 3] {
    ["jsonStyle", "minifyJson", "flags"].map(|key| &root[key])
}

impl ExternalLevelCache {
    fn capture(
        root: &Value,
        fingerprints: BTreeMap<PathBuf, ExternalLevelFingerprint>,
    ) -> Option<Arc<Self>> {
        (!fingerprints.is_empty()).then(|| {
            Arc::new(Self {
                settings: external_level_save_settings(root).map(Value::clone),
                fingerprints,
            })
        })
    }

    fn verified_file(
        &self,
        path: &Path,
        level: &Value,
        revision: Option<&Arc<Value>>,
        same_path: bool,
        checked_before_reload: Option<u64>,
    ) -> Option<(Option<Vec<u8>>, &ExternalLevelFingerprint)> {
        let cached = self.fingerprints.get(path)?;
        let unchanged =
            revision.is_some_and(|revision| Arc::as_ptr(revision) == cached.level.as_ptr());
        if same_path && !unchanged {
            return None;
        }
        let metadata = fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() != cached.length as u64 {
            return None;
        }
        if same_path && cached.parsed && checked_before_reload == Some(cached.bytes) {
            // Reload checks the disk against these bytes after publication.
            return Some((None, cached));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take((cached.length as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() == cached.length
            && bytes_fingerprint(&bytes) == cached.bytes
            && (unchanged || serde_json::from_slice::<Value>(&bytes).ok().as_ref() == Some(level))
        {
            Some(((!same_path).then_some(bytes), cached))
        } else {
            None
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Self::new()
    }
}

fn project_template() -> &'static Value {
    static TEMPLATE: LazyLock<Value> = LazyLock::new(|| {
        serde_json::from_str(include_str!("empty.ldtk")).expect("bundled project template")
    });
    &TEMPLATE
}

impl Project {
    pub fn new() -> Self {
        let mut json = project_template().clone();
        json["iid"] = Value::String(uuid::Uuid::new_v4().to_string());
        json["dummyWorldIid"] = Value::String(uuid::Uuid::new_v4().to_string());
        json["levels"][0]["iid"] = Value::String(uuid::Uuid::new_v4().to_string());
        Self {
            json,
            path: None,
            external_levels: ExternalLevels::default(),
            grid_presence: GridPresences::new(),
            external_level_cache: None,
            repaired_external_paths: false,
        }
    }

    pub fn from_json(json: Value) -> Result<Self> {
        validate_project(&json)?;
        Ok(Self {
            json,
            path: None,
            external_levels: ExternalLevels::default(),
            grid_presence: GridPresences::new(),
            external_level_cache: None,
            repaired_external_paths: false,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Self::load_with_progress(path, |_, _| {})
    }

    /// Reports completed external level reads; inline-only projects emit no updates.
    pub fn load_with_progress(
        path: impl AsRef<Path>,
        progress: impl FnMut(usize, usize),
    ) -> Result<Self> {
        Self::read_documents(path.as_ref(), progress, None, None)
    }

    /// Bind the watch baseline to the parsed documents and track resources even if unavailable.
    pub fn load_with_snapshot(path: impl AsRef<Path>) -> Result<(Self, FilesystemSnapshot)> {
        Self::load_snapshot_with_documents(path.as_ref(), None, None)
    }

    /// Record the exact bytes parsed for the project and its external levels.
    pub fn load_with_document_snapshot_progress(
        path: impl AsRef<Path>,
        progress: impl FnMut(usize, usize),
    ) -> Result<(Self, FilesystemSnapshot)> {
        Self::read_document_snapshot(path.as_ref(), progress, None)
    }

    fn load_snapshot_with_documents(
        path: &Path,
        published: Option<PublishedDocuments>,
        previous: Option<&FilesystemSnapshot>,
    ) -> Result<(Self, FilesystemSnapshot)> {
        let (project, mut snapshot) = Self::read_document_snapshot(path, |_, _| {}, published)?;
        if let Some(previous) = previous {
            snapshot.carry_resource_watches(previous);
            project.refresh_resource_watches(&mut snapshot, &BTreeSet::new());
        } else {
            project.watch_available_resources(&mut snapshot);
        }
        let changed = snapshot.changed_files()?;
        if !changed.is_empty() {
            return Err(if previous.is_some() {
                Error::FilesChanged(changed)
            } else {
                Error::Invalid("Project files changed during reload. Try reloading again.".into())
            });
        }
        Ok((project, snapshot))
    }

    fn read_document_snapshot(
        path: &Path,
        progress: impl FnMut(usize, usize),
        published: Option<PublishedDocuments>,
    ) -> Result<(Self, FilesystemSnapshot)> {
        let mut snapshot = FilesystemSnapshot::default();
        let project = Self::read_documents(path, progress, Some(&mut snapshot), published)?;
        Ok((project, snapshot))
    }

    fn read_documents(
        path: &Path,
        mut progress: impl FnMut(usize, usize),
        mut snapshot: Option<&mut FilesystemSnapshot>,
        published: Option<PublishedDocuments>,
    ) -> Result<Self> {
        let path = absolute(path)?;
        let (json, bytes, mut reusable_levels) = match published {
            Some(saved) => (
                parse_json_at(&path, &saved.root)?,
                saved.root,
                saved.reusable_levels,
            ),
            None => {
                let (json, bytes) = read_json_with_bytes(&path)?;
                (json, bytes, BTreeMap::new())
            }
        };
        if let Some(snapshot) = &mut snapshot {
            snapshot.record_document(path.clone(), Some(bytes_fingerprint(&bytes)));
        }
        drop(bytes);
        let mut project = Self::from_json(json)?;
        let cache_level = project.json["jsonVersion"] == "1.5.4";
        let fingerprint_levels = cache_level || snapshot.is_some();
        let mut fingerprints = BTreeMap::new();
        let keys = project.level_keys();
        let mut total = 0;
        let mut loaded_paths = snapshot.as_ref().map(|_| Vec::new());
        for key in &keys {
            if let Some(rel) = external_path(project.level_stub(*key).expect("enumerated level"))? {
                total += 1;
                if let Some(paths) = &mut loaded_paths {
                    paths.push(rel.to_owned());
                }
            }
        }
        if let Some(snapshot) = &mut snapshot {
            snapshot.loaded_external_paths = Some((path.clone(), loaded_paths.unwrap_or_default()));
        }
        if total > 0 {
            progress(0, total);
        }
        let mut done = 0;
        for key in keys {
            let stub = project.level_stub(key).expect("enumerated level");
            if let Some(rel) = external_path(stub)? {
                let mut rel = rel.to_owned();
                let base = path.parent().expect("absolute path");
                let mut external = base.join(&rel);
                let (value, length, fingerprint) = if let Some((value, length, fingerprint)) =
                    reusable_levels.remove(&external)
                {
                    (value, length, Some(fingerprint))
                } else {
                    let (value, bytes) = match read_json_with_bytes(&external) {
                        Ok(result) => result,
                        Err(error) if project.json["externalLevels"] == true => {
                            let (repaired, value, bytes) =
                                recover_renamed_external_level(&path, &rel, stub, error)?;
                            if let Some(snapshot) = &mut snapshot {
                                snapshot.record_document(absolute(&external)?, None);
                            }
                            project.level_stub_mut(key).expect("enumerated level")["externalRelPath"] =
                                Value::String(repaired.clone());
                            project.repaired_external_paths = true;
                            rel = repaired;
                            external = base.join(&rel);
                            (value, bytes)
                        }
                        Err(error) => return Err(error),
                    };
                    let fingerprint = fingerprint_levels.then(|| bytes_fingerprint(&bytes));
                    (Arc::new(value), bytes.len(), fingerprint)
                };
                if project.external_levels.0.contains_key(&rel) {
                    return Err(Error::Invalid(format!(
                        "duplicate external level path: {rel}"
                    )));
                }
                if !value.is_object() {
                    return Err(Error::Invalid(format!(
                        "external level {rel} must be an object"
                    )));
                }
                if let Some(fingerprint) = fingerprint {
                    if let Some(snapshot) = &mut snapshot {
                        snapshot.record_document(absolute(&external)?, Some(fingerprint));
                    }
                    if cache_level {
                        fingerprints.insert(
                            external,
                            ExternalLevelFingerprint {
                                length,
                                bytes: fingerprint,
                                parsed: true,
                                level: Arc::downgrade(&value),
                            },
                        );
                    }
                }
                project.external_levels.0.insert(rel, value);
                done += 1;
                progress(done, total);
            }
        }
        if !project.repaired_external_paths {
            project.external_level_cache = ExternalLevelCache::capture(&project.json, fingerprints);
        }
        project.path = Some(path);
        Ok(project)
    }

    /// Project-relative path used when publishing a level under its generated filename.
    pub fn external_level_rel_path(
        &self,
        project_file: &Path,
        index: usize,
        level: &Value,
    ) -> Result<String> {
        let id = level["identifier"].as_str().unwrap_or("Level");
        if id.contains(['/', '\\']) || matches!(id, "." | "..") {
            return Err(Error::Invalid("unsafe external level identifier".into()));
        }
        let directory = project_file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let indexed = self.json["flags"]
            .as_array()
            .is_some_and(|flags| flags.iter().any(|f| f == "PrependIndexToLevelFileNames"));
        Ok(if indexed {
            format!("{directory}/{index:04}-{id}.ldtkl")
        } else {
            format!("{directory}/{id}.ldtkl")
        })
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.save_document(path, true)
    }
    fn save_document(&mut self, path: impl AsRef<Path>, make_backup: bool) -> Result<()> {
        self.save_document_with_progress(path, make_backup, None, None, None, &mut |_, _, _, _| {})
    }
    fn save_document_with_progress(
        &mut self,
        path: impl AsRef<Path>,
        make_backup: bool,
        verification: Option<SaveVerification<'_>>,
        mut published: Option<&mut PublishedDocuments>,
        mut saved_snapshot: Option<&mut FilesystemSnapshot>,
        progress: &mut impl FnMut(SaveStage, usize, usize, Option<&str>),
    ) -> Result<()> {
        validate_project(&self.json)?;
        let (verified_snapshot, verify_all_files) = match verification {
            Some(SaveVerification::DocumentDestinations(snapshot)) => (Some(snapshot), false),
            Some(SaveVerification::AllWatchedFiles(snapshot)) => (Some(snapshot), true),
            None => (None, false),
        };
        let path = absolute(path.as_ref())?;
        let base = path
            .parent()
            .ok_or_else(|| Error::Invalid("project path has no parent".into()))?;
        let retired = verified_snapshot
            .and_then(|snapshot| snapshot.loaded_external_paths.as_ref())
            .filter(|(loaded, _)| loaded == &path)
            .map_or_else(
                || previous_external_paths(&path),
                |(_, paths)| paths.clone(),
            );
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
                    let level = candidate.external_levels.0.remove(&old).ok_or_else(|| {
                        Error::Invalid(format!("external level {old} has not been loaded"))
                    })?;
                    let rel = candidate.external_level_rel_path(&path, idx, &level)?;
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
            candidate.external_levels = ExternalLevels(levels);
        }
        if candidate.json["externalLevels"].as_bool().unwrap_or(false) {
            let mut external = BTreeMap::new();
            for (idx, key) in candidate.level_keys().into_iter().enumerate() {
                let old_rel = candidate
                    .level_stub(key)
                    .and_then(|v| v["externalRelPath"].as_str())
                    .map(str::to_owned);
                let mut level = if let Some(rel) = old_rel.as_deref() {
                    candidate.external_levels.0.remove(rel).ok_or_else(|| {
                        Error::Invalid("external level has not been loaded".into())
                    })?
                } else {
                    Arc::new(
                        candidate
                            .level(key)
                            .ok_or_else(|| {
                                Error::Invalid("external level has not been loaded".into())
                            })?
                            .clone(),
                    )
                };
                let generated = candidate.external_level_rel_path(&path, idx, &level)?;
                let rel = if self.path.is_none() {
                    old_rel.unwrap_or(generated)
                } else {
                    generated
                };
                if external.contains_key(&rel) {
                    return Err(Error::Invalid(format!(
                        "duplicate external level path: {rel}"
                    )));
                }
                let stub = candidate.level_stub_mut(key).expect("enumerated level");
                stub["externalRelPath"] = Value::String(rel.clone());
                stub["layerInstances"] = Value::Null;
                if self.path.is_some() && level.get("externalRelPath") != Some(&Value::Null) {
                    Arc::make_mut(&mut level)["externalRelPath"] = Value::Null;
                }
                external.insert(rel, level);
            }
            candidate.external_levels = ExternalLevels(external);
        }
        let current_version = candidate.json["jsonVersion"] == "1.5.4";
        // Derive the current output dialect only after an explicit migration.
        let compatibility = if current_version {
            candidate.reconcile_cached_auto_tiles();
            candidate.tidy_auto_rules_for_save();
            candidate.refresh_table_of_contents()?;
            Some(OutputCompatibility::new(&candidate.json))
        } else {
            None
        };
        if current_version
            && let Some(header) = candidate
                .json
                .get_mut("__header__")
                .and_then(Value::as_object_mut)
        {
            header.insert("appVersion".into(), Value::String("1.5.4".into()));
        }
        let minify = self
            .json
            .get("minifyJson")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let style = match self.json.get("jsonStyle").and_then(Value::as_str) {
            None => None,
            Some("Full") => Some(JsonStyle::Full),
            Some("Compact") => Some(JsonStyle::Compact),
            Some("Minified") => Some(JsonStyle::Minified),
            Some(style) => return Err(Error::Invalid(format!("unsupported JSON style {style}"))),
        };
        let source_cache = self
            .path
            .as_deref()
            .and_then(Path::parent)
            .zip(self.external_level_cache.as_deref())
            .filter(|(_, cache)| {
                current_version
                    && cache.settings.each_ref() == external_level_save_settings(&candidate.json)
            });
        let mut fingerprints = BTreeMap::new();
        let mut staged = StagedSave::default();
        let mut paths = BTreeSet::from([path.clone()]);
        let document_dir = base.join(path.file_stem().unwrap_or_default());
        let preserve_identical_levels = current_version && self.path.as_ref() == Some(&path);
        let mut external = Vec::new();
        for key in candidate.level_keys() {
            if current_version && let Some(level) = candidate.level(key) {
                let color = candidate.exported_level_smart_color(level);
                if level["__smartColor"] != color {
                    candidate.level_mut(key).expect("enumerated level")["__smartColor"] =
                        Value::String(color);
                }
            }
            if let Some(rel) = external_path(candidate.level_stub(key).expect("enumerated level"))?
                .map(str::to_owned)
            {
                let target = absolute(&base.join(&rel))?;
                if !paths.insert(target.clone()) {
                    return Err(Error::Invalid(format!(
                        "duplicate output path: {}",
                        target.display()
                    )));
                }
                if target.starts_with(&path) {
                    return Err(Error::Invalid(
                        "The project file conflicts with the folder for its external levels. Use a .ldtk or .json extension.".into(),
                    ));
                }
                let level = candidate.external_levels.0.get_mut(&rel).ok_or_else(|| {
                    Error::Invalid(format!("external level {rel} has not been loaded"))
                })?;
                if !level.is_object() {
                    return Err(Error::Invalid(format!(
                        "external level {rel} must be an object"
                    )));
                }
                if current_version && let Some(header) = updated_external_level_header(level) {
                    apply_external_level_header(Arc::make_mut(level), header);
                }
                sync_summary(&mut candidate.json, key, level);
                external.push((key, rel, target));
            }
        }
        let watched_retired = if verified_snapshot.is_some() {
            retired
                .iter()
                .map(|rel| absolute(&base.join(rel)))
                .collect::<Result<BTreeSet<_>>>()?
        } else {
            BTreeSet::new()
        };
        let mut new_level_paths = BTreeSet::new();
        if let Some(snapshot) = verified_snapshot {
            let mut occupied = Vec::new();
            for (_, _, target) in &external {
                if snapshot.document_fingerprint(target).is_some() {
                    continue;
                }
                match fs::symlink_metadata(target) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        new_level_paths.insert(target.clone());
                    }
                    Ok(metadata)
                        if metadata.is_file()
                            && watched_retired.iter().any(|previous| {
                                snapshot.is_document(previous)
                                    && !paths.contains(previous)
                                    && is_case_only_rename_target(previous, target)
                            }) => {}
                    Ok(_) => occupied.push(target.clone()),
                    Err(source) => {
                        return Err(Error::Io {
                            path: target.clone(),
                            source,
                        });
                    }
                }
            }
            if !occupied.is_empty() {
                return Err(Error::FilesChanged(occupied));
            }
        }
        let verify_files = || -> Result<()> {
            if let Some(snapshot) = verified_snapshot {
                let changed = snapshot.changed_files_matching(|path| {
                    verify_all_files || paths.contains(path) || watched_retired.contains(path)
                })?;
                if !changed.is_empty() {
                    return Err(Error::FilesChanged(changed));
                }
            }
            Ok(())
        };
        if make_backup && candidate.json["backupOnSave"].as_bool().unwrap_or(false) {
            verify_files()?;
            candidate.path = Some(path.clone());
            candidate.backup_with_progress(None, &mut |done, total, label| {
                progress(SaveStage::Backup, done, total, label);
            })?;
        }
        progress(SaveStage::WriteProject, 0, 1, None);
        let root_bytes = if let Some(compatibility) = &compatibility {
            if published.is_some() {
                // The caller reloads the saved dialect, so no second editable copy is needed.
                compatibility.project_output(&mut candidate.json, &candidate.grid_presence);
                encode_with_style(&candidate.json, minify, style)?
            } else {
                let mut root = candidate.json.clone();
                compatibility.project_output(&mut root, &candidate.grid_presence);
                encode_with_style(&root, minify, style)?
            }
        } else {
            encode_with_style(&candidate.json, minify, style)?
        };
        let root = StagedFile::final_replacement(path.clone(), &root_bytes)?;
        if let Some(snapshot) = saved_snapshot.as_deref_mut() {
            snapshot.record_document(path.clone(), Some(bytes_fingerprint(&root_bytes)));
            snapshot.loaded_external_paths = Some((
                path.clone(),
                external
                    .iter()
                    .map(|(_, relative, _)| relative.clone())
                    .collect(),
            ));
        }
        if let Some(saved) = published.as_deref_mut() {
            saved.root = root_bytes;
        } else {
            drop(root_bytes);
        }
        let total = external.len();
        for (done, (key, rel, target)) in external.into_iter().enumerate() {
            let document = &candidate.external_levels.0[&rel];
            progress(
                SaveStage::PrepareLevels,
                done,
                total,
                document["identifier"].as_str(),
            );
            let projected = compatibility.as_ref().map(|compatibility| {
                compatibility.level_output(document, &candidate.grid_presence)
            });
            let level = projected.as_deref().unwrap_or(document);
            let revision = std::ptr::eq(level, document.as_ref()).then_some(document);
            let checked_before_reload = if published.is_some() {
                verified_snapshot.and_then(|snapshot| snapshot.document_fingerprint(&target))
            } else {
                None
            };
            let cached = source_cache.and_then(|(source, cache)| {
                let rel = self.level_stub(key)?["externalRelPath"].as_str()?;
                let source = source.join(rel);
                cache.verified_file(
                    &source,
                    level,
                    revision,
                    source == target,
                    checked_before_reload,
                )
            });
            let (bytes, cached) = match cached {
                Some((bytes, fingerprint)) => (bytes, Some(fingerprint)),
                None => (Some(encode_with_style(level, minify, style)?), None),
            };
            if current_version || saved_snapshot.is_some() {
                let (length, fingerprint) = if let Some(cached) = cached {
                    (cached.length, cached.bytes)
                } else {
                    let bytes = bytes.as_deref().expect("serialized level");
                    (bytes.len(), bytes_fingerprint(bytes))
                };
                if let Some(snapshot) = saved_snapshot.as_deref_mut() {
                    snapshot.record_document(target.clone(), Some(fingerprint));
                }
                if current_version {
                    let parsed = bytes.is_none() && cached.is_some_and(|file| file.parsed);
                    fingerprints.insert(
                        target.clone(),
                        ExternalLevelFingerprint {
                            length,
                            bytes: fingerprint,
                            parsed,
                            level: revision.map_or_else(Weak::new, Arc::downgrade),
                        },
                    );
                    if parsed && let Some(saved) = published.as_deref_mut() {
                        saved
                            .reusable_levels
                            .insert(target.clone(), (Arc::clone(document), length, fingerprint));
                    }
                }
            }
            if let Some(bytes) = bytes {
                if target.starts_with(&document_dir) {
                    ensure_parent_directory(&target)?;
                }
                if new_level_paths.contains(&target) {
                    staged.add_new_document(target, &bytes)?;
                } else {
                    staged.add(target, &bytes, preserve_identical_levels)?;
                }
            }
        }
        if total > 0 {
            progress(SaveStage::PrepareLevels, total, total, None);
            progress(SaveStage::PrepareLevels, 0, 0, None);
        }
        // The main document is prepared first but published after all referenced files.
        staged.files.push(root);
        verify_files()?;
        staged.publish()?;
        self.external_levels = candidate.external_levels;
        self.json = candidate.json;
        self.external_level_cache = ExternalLevelCache::capture(&self.json, fingerprints);
        self.repaired_external_paths = false;
        if !retired.is_empty() {
            let total = retired.len();
            progress(SaveStage::RemoveObsoleteLevels, 0, total, None);
            let published_real_paths = paths
                .iter()
                .filter_map(|path| fs::canonicalize(path).ok())
                .collect::<BTreeSet<_>>();
            #[cfg(unix)]
            let published_files = paths
                .iter()
                .filter_map(|path| file_identity(path))
                .collect::<BTreeSet<_>>();
            for (done, rel) in retired.into_iter().enumerate() {
                let target = absolute(&base.join(rel))?;
                progress(
                    SaveStage::RemoveObsoleteLevels,
                    done,
                    total,
                    target.file_name().and_then(|name| name.to_str()),
                );
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
            progress(SaveStage::RemoveObsoleteLevels, total, total, None);
        }
        self.path = Some(path);
        progress(SaveStage::WriteProject, 1, 1, None);
        Ok(())
    }
}

fn previous_external_paths(path: &Path) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Level {
        #[serde(rename = "externalRelPath")]
        external_path: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct World {
        levels: Vec<Level>,
    }
    #[derive(serde::Deserialize)]
    struct Document {
        #[serde(default)]
        levels: Vec<Level>,
        #[serde(default)]
        worlds: Vec<World>,
    }
    let Ok(bytes) = fs::read(path) else {
        return Vec::new();
    };
    // Embedded projects have nothing to remove. Skip their tile and entity
    // payloads, but keep full validation whenever a file may be retired.
    if serde_json::from_slice::<Document>(&bytes).is_ok_and(|doc| {
        doc.levels
            .iter()
            .chain(doc.worlds.iter().flat_map(|world| &world.levels))
            .all(|level| level.external_path.is_none())
    }) {
        return Vec::new();
    }
    let Some(saved) = serde_json::from_slice(&bytes)
        .ok()
        .and_then(|json| Project::from_json(json).ok())
    else {
        return Vec::new();
    };
    saved
        .iter_level_keys()
        .filter_map(|key| {
            saved
                .level_stub(key)?
                .get("externalRelPath")?
                .as_str()
                .map(str::to_owned)
        })
        .collect()
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

fn recover_renamed_external_level(
    project: &Path,
    rel: &str,
    stub: &Value,
    original_error: Error,
) -> Result<(String, Value, Vec<u8>)> {
    if !matches!(&original_error, Error::Io {source, ..} if source.kind() == std::io::ErrorKind::NotFound)
    {
        return Err(original_error);
    }
    let (Some(stem), Some(filename)) = (
        project.file_stem().and_then(std::ffi::OsStr::to_str),
        Path::new(rel).file_name().and_then(std::ffi::OsStr::to_str),
    ) else {
        return Err(original_error);
    };
    let repaired = format!("{stem}/{filename}");
    if repaired == rel {
        return Err(original_error);
    }
    let path = project
        .parent()
        .expect("absolute project path")
        .join(&repaired);
    let (level, bytes) = match read_json_with_bytes(&path) {
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(original_error);
        }
        result => result?,
    };
    let uid = stub.get("uid").filter(|value| value.is_i64());
    let iid = stub.get("iid").filter(|value| value.is_string());
    if (uid.is_none() && iid.is_none())
        || uid.is_some_and(|uid| level.get("uid") != Some(uid))
        || iid.is_some_and(|iid| level.get("iid") != Some(iid))
    {
        return Err(Error::Invalid(format!(
            "external level {repaired} does not match the identity of {rel}"
        )));
    }
    Ok((repaired, level, bytes))
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
    read_json_with_bytes(path).map(|(value, _)| value)
}

fn read_json_with_bytes(path: &Path) -> Result<(Value, Vec<u8>)> {
    let bytes = fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((parse_json_at(path, &bytes)?, bytes))
}

fn parse_json_at(path: &Path, bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|source| Error::Json {
        path: path.to_path_buf(),
        source,
    })
}

struct OutputCompatibility {
    multiline: bool,
    legacy_int_grid: bool,
    text_arrays: BTreeMap<i64, bool>,
    auto_rule_alpha: BTreeMap<(i64, i64), Value>,
}

impl OutputCompatibility {
    fn new(root: &Value) -> Self {
        let flag = |name: &str| {
            root["flags"]
                .as_array()
                .is_some_and(|flags| flags.iter().any(|flag| flag == name))
        };
        let mut result = Self {
            multiline: flag("UseMultilinesType"),
            legacy_int_grid: flag("ExportPreCsvIntGridFormat"),
            text_arrays: BTreeMap::new(),
            auto_rule_alpha: BTreeMap::new(),
        };
        let defs = &root["defs"];
        for layer in defs["layers"].as_array().into_iter().flatten() {
            let Some(layer_uid) = layer["uid"].as_i64() else {
                continue;
            };
            for group in layer["autoRuleGroups"].as_array().into_iter().flatten() {
                for rule in group["rules"].as_array().into_iter().flatten() {
                    if let Some(uid) = rule["uid"].as_i64() {
                        let alpha = rule
                            .get("alpha")
                            .filter(|value| value.is_number())
                            .cloned()
                            .unwrap_or_else(|| Value::from(1));
                        result.auto_rule_alpha.insert((layer_uid, uid), alpha);
                    }
                }
            }
        }
        let entity_fields = defs["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|entity| entity["fieldDefs"].as_array().into_iter().flatten());
        for field in defs["levelFields"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(entity_fields)
        {
            if field["type"] == "F_Text"
                && let Some(uid) = field["uid"].as_i64()
            {
                result.text_arrays.insert(uid, field["isArray"] == true);
            }
        }
        result
    }

    fn text_type(multiline: bool, array: bool) -> &'static str {
        match (multiline, array) {
            (false, false) => "String",
            (true, false) => "Multilines",
            (false, true) => "Array<String>",
            (true, true) => "Array<Multilines>",
        }
    }

    fn project_output(&self, root: &mut Value, presence: &GridPresences) {
        Self::apply_definition_types(root, self.multiline);
        Self::apply_tileset_enum_tags(root);
        for entity in root
            .pointer_mut("/defs/entities")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for axis in ["pivotX", "pivotY"] {
                if let Some(pivot) = entity.get_mut(axis)
                    && let Some(value) = pivot.as_f64()
                {
                    *pivot = crate::workflow::canonical_number(value);
                }
            }
            if entity["tileRenderMode"] != "NineSlice"
                && let Some(borders) = entity.get_mut("nineSliceBorders")
            {
                *borders = Value::Array(Vec::new());
            }
        }
        visit_level_documents(root, |level| self.apply_level(level, presence));
    }

    fn apply_tileset_enum_tags(root: &mut Value) {
        let Some(defs) = root.get_mut("defs") else {
            return;
        };
        let order: BTreeMap<_, Vec<_>> = ["enums", "externalEnums"]
            .into_iter()
            .flat_map(|kind| defs[kind].as_array().into_iter().flatten())
            .filter_map(|definition| {
                let uid = definition["uid"].as_i64()?;
                let values = definition["values"]
                    .as_array()?
                    .iter()
                    .filter_map(|value| value["id"].as_str().map(str::to_owned))
                    .collect();
                Some((uid, values))
            })
            .collect();
        for tileset in defs["tilesets"].as_array_mut().into_iter().flatten() {
            let Some(values) = tileset["tagsSourceEnumUid"]
                .as_i64()
                .and_then(|uid| order.get(&uid))
            else {
                continue;
            };
            let tags = tileset.get_mut("enumTags").and_then(Value::as_array_mut);
            let mut existing: Vec<_> = tags
                .into_iter()
                .flatten()
                .map(std::mem::take)
                .map(Some)
                .collect();
            let by_id: BTreeMap<_, _> = existing
                .iter()
                .enumerate()
                .filter_map(|(index, tag)| {
                    Some((tag.as_ref()?["enumValueId"].as_str()?.to_owned(), index))
                })
                .collect();
            let mut ordered: Vec<_> = values
                .iter()
                .map(|id| {
                    by_id
                        .get(id)
                        .and_then(|index| existing[*index].take())
                        .unwrap_or_else(|| serde_json::json!({"enumValueId":id,"tileIds":[]}))
                })
                .collect();
            ordered.extend(existing.into_iter().flatten());
            tileset["enumTags"] = Value::Array(ordered);
        }
    }

    fn apply_definition_types(root: &mut Value, multiline: bool) {
        let apply_type = |field: &mut Value| {
            if field["type"] == "F_Text" {
                field["__type"] = Self::text_type(multiline, field["isArray"] == true).into();
            }
        };
        if let Some(defs) = root.get_mut("defs") {
            for field in defs
                .get_mut("levelFields")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                apply_type(field);
            }
            for entity in defs
                .get_mut("entities")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for field in entity
                    .get_mut("fieldDefs")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    apply_type(field);
                }
            }
        }
    }

    fn level_output<'a>(
        &self,
        level: &'a Value,
        presence: &GridPresences,
    ) -> std::borrow::Cow<'a, Value> {
        if !self.level_has_stale_text_types(level)
            && !self.legacy_int_grid
            && !level["layerInstances"].as_array().is_some_and(|layers| {
                layers.iter().any(|layer| {
                    layer.get("intGrid").is_some() || self.layer_has_stale_auto_alpha(layer)
                })
            })
        {
            return std::borrow::Cow::Borrowed(level);
        }
        let mut output = level.clone();
        self.apply_level(&mut output, presence);
        std::borrow::Cow::Owned(output)
    }

    fn projected_text_type(&self, field: &Value, multiline: bool) -> Option<&'static str> {
        let array = *self.text_arrays.get(&field["defUid"].as_i64()?)?;
        let expected = Self::text_type(multiline, array);
        (field["__type"].as_str() != Some(expected)).then_some(expected)
    }

    fn level_has_stale_text_types(&self, level: &Value) -> bool {
        if self.text_arrays.is_empty() {
            return false;
        }
        // Keep the existing projection for missing containers: the mutable
        // visitor inserts a null slot when it indexes an absent key.
        let stale_fields = |owner: &Value| {
            owner.get("fieldInstances").is_none_or(|fields| {
                fields
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|field| self.projected_text_type(field, self.multiline).is_some())
            })
        };
        stale_fields(level)
            || level.get("layerInstances").is_none_or(|layers| {
                layers.as_array().into_iter().flatten().any(|layer| {
                    layer.get("entityInstances").is_none_or(|entities| {
                        entities.as_array().into_iter().flatten().any(stale_fields)
                    })
                })
            })
    }

    fn apply_field_types(&self, level: &mut Value, multiline: bool) {
        if !self.text_arrays.is_empty() {
            crate::workflow::visit_fields_mut(level, |field| {
                if let Some(expected) = self.projected_text_type(field, multiline) {
                    field["__type"] = expected.into();
                }
            });
        }
    }

    fn apply_project_text_types(&self, project: &mut Project, multiline: bool) {
        Self::apply_definition_types(&mut project.json, multiline);
        visit_level_documents(&mut project.json, |level| {
            self.apply_field_types(level, multiline)
        });
        for level in project.external_levels.values_mut() {
            self.apply_field_types(level, multiline);
        }
    }

    fn apply_level(&self, level: &mut Value, presence: &GridPresences) {
        self.apply_field_types(level, self.multiline);
        for layer in level
            .get_mut("layerInstances")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let layer_uid = layer["layerDefUid"].as_i64();
            if !self.auto_rule_alpha.is_empty()
                && let Some(uid) = layer_uid
            {
                for tile in layer
                    .get_mut("autoLayerTiles")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if let Some(alpha) = self.changed_auto_tile_alpha(uid, tile) {
                        tile["a"] = alpha.clone();
                    }
                }
            }
            let Some(layer) = layer.as_object_mut() else {
                continue;
            };
            if let Some(csv) = layer.get("intGridCsv").and_then(Value::as_array) {
                if self.legacy_int_grid {
                    let state = layer
                        .get("iid")
                        .and_then(Value::as_str)
                        .and_then(|iid| presence.get(iid));
                    let legacy = csv
                        .iter()
                        .enumerate()
                        .filter_map(|(coord, value)| {
                            let value = value.as_i64().filter(|value| {
                                *value > 0
                                    || *value == 0
                                        && state.is_none_or(|state| state.contains(coord))
                            })?;
                            Some(serde_json::json!({"coordId":coord,"v":value - 1}))
                        })
                        .collect();
                    layer.insert("intGrid".into(), Value::Array(legacy));
                } else {
                    layer.remove("intGrid");
                }
            }
        }
    }

    fn changed_auto_tile_alpha(&self, layer_uid: i64, tile: &Value) -> Option<&Value> {
        let rule_uid = tile["d"][0].as_i64()?;
        self.auto_rule_alpha
            .get(&(layer_uid, rule_uid))
            .filter(|alpha| alpha.as_f64() != tile["a"].as_f64())
    }

    fn layer_has_stale_auto_alpha(&self, layer: &Value) -> bool {
        !self.auto_rule_alpha.is_empty()
            && layer["layerDefUid"].as_i64().is_some_and(|uid| {
                layer["autoLayerTiles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|tile| self.changed_auto_tile_alpha(uid, tile).is_some())
            })
    }
}

fn visit_level_documents(root: &mut Value, mut visit: impl FnMut(&mut Value)) {
    for level in root
        .get_mut("levels")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        visit(level);
    }
    for world in root
        .get_mut("worlds")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for level in world
            .get_mut("levels")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            visit(level);
        }
    }
}

fn updated_external_level_header(level: &Value) -> Option<Value> {
    let mut header = project_template()["__header__"].clone();
    if let Some(previous) = level["__header__"].as_object() {
        let header = header.as_object_mut().expect("bundled project header");
        for (key, value) in previous {
            header.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    (!level
        .as_object()
        .and_then(|level| level.iter().next())
        .is_some_and(|(key, value)| key == "__header__" && *value == header))
    .then_some(header)
}

fn apply_external_level_header(level: &mut Value, header: Value) {
    let level = level.as_object_mut().expect("validated external level");
    level.shift_remove("__header__");
    level.shift_insert(0, "__header__".into(), header);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum JsonStyle {
    Full,
    Compact,
    Minified,
}

fn encode_with_style(
    value: &Value,
    legacy_minify: bool,
    style: Option<JsonStyle>,
) -> Result<Vec<u8>> {
    let Some(style) = style else {
        return encode(value, legacy_minify);
    };
    if style == JsonStyle::Minified {
        return serde_json::to_vec(value).map_err(|e| Error::Invalid(e.to_string()));
    }
    let mut output = Vec::new();
    format_json(value, style, 0, false, &mut output).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(output)
}

fn compact_length(value: &Value, depth: usize) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(v) => {
            if *v {
                4
            } else {
                5
            }
        }
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                4
            } else {
                5
            }
        }
        Value::String(s) => s.encode_utf16().count() + 2,
        Value::Object(map) => {
            if depth == 0 && map.len() <= 5 {
                map.values().map(|v| compact_length(v, depth + 1)).sum()
            } else {
                map.len() * 10
            }
        }
        Value::Array(a) if a.is_empty() => 2,
        Value::Array(a) if a.len() < 50 && a[0].is_number() => a.len(),
        Value::Array(a) if a.len() > 5 || depth > 0 => 99,
        Value::Array(a) => a.iter().map(|v| compact_length(v, depth + 1)).sum(),
    }
}

fn format_json(
    value: &Value,
    style: JsonStyle,
    indent: usize,
    force_multiline: bool,
    out: &mut Vec<u8>,
) -> serde_json::Result<()> {
    let fits_inline =
        || style == JsonStyle::Compact && !force_multiline && compact_length(value, 0) <= 85;
    let write_indent = |out: &mut Vec<u8>, n| {
        out.resize(out.len() + n, b'\t');
    };
    match value {
        Value::Object(map) if !map.is_empty() => {
            let inline = fits_inline();
            out.push(b'{');
            out.push(if inline { b' ' } else { b'\n' });
            for (i, (key, val)) in map.iter().enumerate() {
                if !inline {
                    write_indent(out, indent + 1);
                }
                serde_json::to_writer(&mut *out, key)?;
                out.extend_from_slice(b": ");
                format_json(val, style, indent + 1, key == "__header__", out)?;
                if i + 1 < map.len() {
                    out.push(b',');
                    out.push(if inline { b' ' } else { b'\n' });
                }
            }
            if inline {
                out.push(b' ');
            } else {
                out.push(b'\n');
                write_indent(out, indent);
            }
            out.push(b'}');
        }
        Value::Array(array) if !array.is_empty() => {
            let numeric = array[0].is_number();
            let grid = numeric && array.len() > 85 && !force_multiline;
            let array_inline = !grid && fits_inline();
            let space = array_inline
                && !numeric
                && (array[0].as_bool().is_none() && array.len() != 1
                    || array[0].is_boolean() && array.len() <= 5);
            out.push(b'[');
            if array_inline {
                if space {
                    out.push(b' ');
                }
            } else {
                out.push(b'\n');
            }
            for (i, val) in array.iter().enumerate() {
                if !array_inline && (!grid || i % 35 == 0) {
                    write_indent(out, indent + 1);
                }
                format_json(val, style, indent + 1, false, out)?;
                if i + 1 < array.len() {
                    out.push(b',');
                    if (grid && i % 35 == 34) || (!grid && !array_inline) {
                        out.push(b'\n');
                    } else if space {
                        out.push(b' ');
                    }
                }
            }
            if array_inline {
                if space {
                    out.push(b' ');
                }
            } else {
                out.push(b'\n');
                write_indent(out, indent);
            }
            out.push(b']');
        }
        _ => serde_json::to_writer(out, value)?,
    }
    Ok(())
}

/// Staging and rollback copies are adjacent to their destination, so publication
/// and recovery can each rename within the same filesystem.
struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let Err(_error) = fs::remove_file(&self.0) else {
            return;
        };
        // A staging copy can inherit the destination's read-only bit.
        #[cfg(windows)]
        #[allow(
            clippy::permissions_set_readonly_false,
            reason = "Only clears the Windows read-only file attribute"
        )]
        if _error.kind() == std::io::ErrorKind::PermissionDenied
            && let Ok(metadata) = fs::metadata(&self.0)
            && metadata.permissions().readonly()
        {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            if fs::set_permissions(&self.0, permissions).is_ok() {
                let _ = fs::remove_file(&self.0);
            }
        }
    }
}

struct StagedFile {
    target: PathBuf,
    staged: TemporaryFile,
    original: Option<TemporaryFile>,
    create_new: bool,
}

impl StagedFile {
    /// The final rename can never need rollback after a later rename fails.
    fn final_replacement(target: PathBuf, bytes: &[u8]) -> Result<Self> {
        let io_error = |source| Error::Io {
            path: target.clone(),
            source,
        };
        let permissions = match fs::File::open(&target) {
            Ok(file) => {
                let metadata = file.metadata().map_err(io_error)?;
                if !metadata.is_file() {
                    return Err(io_error(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "project destination is not a regular file",
                    )));
                }
                Some(metadata.permissions())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(error)),
        };
        let staged = stage_file(&target, bytes, permissions)?;
        Ok(Self {
            target,
            staged,
            original: None,
            create_new: false,
        })
    }

    fn publish(&self) -> Result<()> {
        let result = if self.create_new {
            tempfile::TempPath::try_from_path(&self.staged.0).and_then(|staged| {
                staged
                    .persist_noclobber(&self.target)
                    .map_err(|error| error.error)
            })
        } else {
            fs::rename(&self.staged.0, &self.target)
        };
        result.map_err(|source| {
            if self.create_new && source.kind() == io::ErrorKind::AlreadyExists {
                Error::FilesChanged(vec![self.target.clone()])
            } else {
                Error::Io {
                    path: self.target.clone(),
                    source,
                }
            }
        })
    }
}

fn ensure_parent_directory(target: &Path) -> Result<()> {
    fs::create_dir_all(target.parent().expect("output path parent")).map_err(|source| Error::Io {
        path: target.to_owned(),
        source,
    })
}

fn open_existing_file(target: &Path) -> Result<Option<fs::File>> {
    match fs::File::open(target) {
        Ok(file) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: target.to_owned(),
            source,
        }),
    }
}

fn stage_file(
    target: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<TemporaryFile> {
    stage_file_parts(target, &[bytes], permissions)
}

fn stage_file_parts(
    target: &Path,
    parts: &[&[u8]],
    permissions: Option<fs::Permissions>,
) -> Result<TemporaryFile> {
    stage_file_with(target, permissions, |file| {
        parts.iter().try_for_each(|part| file.write_all(part))
    })
}

fn stage_file_with(
    target: &Path,
    permissions: Option<fs::Permissions>,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> Result<TemporaryFile> {
    let (mut file, staged) = open_staging_file(target).map_err(|source| Error::Io {
        path: target.to_owned(),
        source,
    })?;
    let written = (|| {
        write(&mut file)?;
        sync_staged_file(&file, permissions)
    })();
    drop(file);
    written.map_err(|source| Error::Io {
        path: staged.0.clone(),
        source,
    })?;
    Ok(staged)
}

fn open_staging_file(target: &Path) -> io::Result<(fs::File, TemporaryFile)> {
    let staged = target.with_file_name(format!(".ldtk-{}.tmp", uuid::Uuid::new_v4()));
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&staged)?;
    Ok((file, TemporaryFile(staged)))
}

fn sync_staged_file(file: &fs::File, permissions: Option<fs::Permissions>) -> io::Result<()> {
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()
}

const EXPORT_BUFFER_SIZE: usize = 64 * 1024;

pub(crate) struct StagingWriter<'a> {
    target: &'a Path,
    spilled: Option<(fs::File, TemporaryFile)>,
}

impl Write for StagingWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.spilled.is_none() {
            self.spilled = Some(open_staging_file(self.target)?);
        }
        self.spilled.as_mut().unwrap().0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some((file, _)) = &mut self.spilled {
            file.flush()?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct StagedSave {
    files: Vec<StagedFile>,
}

impl StagedSave {
    pub(crate) fn add(
        &mut self,
        target: PathBuf,
        bytes: &[u8],
        preserve_identical: bool,
    ) -> Result<()> {
        self.add_parts(target, &[bytes], preserve_identical)
    }

    pub(crate) fn add_parts(
        &mut self,
        target: PathBuf,
        parts: &[&[u8]],
        preserve_identical: bool,
    ) -> Result<()> {
        let mut existing = open_existing_file(&target)?;
        if preserve_identical
            && let Some(file) = existing.as_mut()
            && fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.is_file())
        {
            let read_error = |source| Error::Io {
                path: target.clone(),
                source,
            };
            if file_matches_parts(file, parts).map_err(read_error)? {
                return Ok(());
            }
            file.rewind().map_err(read_error)?;
        }

        let permissions = existing
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map(|metadata| metadata.permissions());
        let original = stage_original(&target, existing, permissions.clone())?;
        let staged = stage_file_parts(&target, parts, permissions)?;
        self.files.push(StagedFile {
            target,
            staged,
            original,
            create_new: false,
        });
        Ok(())
    }

    fn add_new_document(&mut self, target: PathBuf, bytes: &[u8]) -> Result<()> {
        let staged = stage_file(&target, bytes, None)?;
        self.files.push(StagedFile {
            target,
            staged,
            original: None,
            create_new: true,
        });
        Ok(())
    }

    pub(crate) fn add_writer(
        &mut self,
        target: PathBuf,
        write: impl FnOnce(&mut BufWriter<StagingWriter<'_>>) -> io::Result<()>,
    ) -> Result<()> {
        let mut buffer = BufWriter::with_capacity(
            EXPORT_BUFFER_SIZE,
            StagingWriter {
                target: &target,
                spilled: None,
            },
        );
        let result = write(&mut buffer);
        let (writer, pending) = buffer.into_parts();
        result.map_err(|source| Error::Io {
            path: target.clone(),
            source,
        })?;
        let pending = pending.expect("export writer returned without panicking");
        let Some((mut file, staged)) = writer.spilled else {
            // Small outputs fit entirely in the buffer and can use the byte comparison path.
            return self.add(target, &pending, true);
        };
        let prepared = (|| {
            let write_error = |source| Error::Io {
                path: staged.0.clone(),
                source,
            };
            file.write_all(&pending).map_err(write_error)?;
            let mut existing = open_existing_file(&target)?;
            let permissions = existing
                .as_ref()
                .and_then(|file| file.metadata().ok())
                .map(|metadata| metadata.permissions());
            if let Some(original) = existing.as_mut()
                && fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.is_file())
            {
                let read_error = |source| Error::Io {
                    path: target.clone(),
                    source,
                };
                if equal_file_contents(original, &mut file).map_err(read_error)? {
                    return Ok(None);
                }
                original.rewind().map_err(read_error)?;
            }
            sync_staged_file(&file, permissions.clone()).map_err(write_error)?;
            Ok::<_, Error>(Some((existing, permissions)))
        })();
        drop(file);
        let Some((existing, permissions)) = prepared? else {
            return Ok(());
        };
        let original = stage_original(&target, existing, permissions)?;
        self.files.push(StagedFile {
            target,
            staged,
            original,
            create_new: false,
        });
        Ok(())
    }

    pub(crate) fn publish(self) -> Result<()> {
        for (index, file) in self.files.iter().enumerate() {
            if let Err(failure) = file.publish() {
                let mut rollback_errors = Vec::new();
                for file in self.files[..index].iter().rev() {
                    let rollback = match &file.original {
                        Some(original) => fs::rename(&original.0, &file.target),
                        None => fs::remove_file(&file.target),
                    };
                    if let Err(error) = rollback {
                        rollback_errors.push(format!("{}: {error}", file.target.display()));
                    }
                }
                if !rollback_errors.is_empty() {
                    return Err(Error::Invalid(format!(
                        "save failed ({failure}); rollback also failed ({}); inspect project files",
                        rollback_errors.join(", ")
                    )));
                }
                return Err(failure);
            }
        }
        #[cfg(unix)]
        for parent in self
            .files
            .iter()
            .filter_map(|file| file.target.parent())
            .collect::<BTreeSet<_>>()
        {
            // Windows cannot open directory handles this way; file sync and rename still apply.
            fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|source| Error::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }
        Ok(())
    }
}

fn stage_original(
    target: &Path,
    original: Option<fs::File>,
    permissions: Option<fs::Permissions>,
) -> Result<Option<TemporaryFile>> {
    original
        .map(|mut file| {
            stage_file_with(target, permissions, |copy| {
                std::io::copy(&mut file, copy).map(|_| ())
            })
        })
        .transpose()
}

fn file_matches_parts(file: &mut fs::File, parts: &[&[u8]]) -> std::io::Result<bool> {
    let length: u64 = parts.iter().map(|part| part.len() as u64).sum();
    if file.metadata()?.len() != length {
        return Ok(false);
    }
    let mut file = BufReader::with_capacity(length.clamp(1, 64 * 1024) as usize, file);
    for mut part in parts.iter().copied() {
        while !part.is_empty() {
            let chunk = file.fill_buf()?;
            let count = chunk.len().min(part.len());
            if count == 0 || chunk[..count] != part[..count] {
                return Ok(false);
            }
            file.consume(count);
            part = &part[count..];
        }
    }
    Ok(file.fill_buf()?.is_empty())
}

fn equal_file_contents(original: &mut fs::File, staged: &mut fs::File) -> std::io::Result<bool> {
    if original.metadata()?.len() != staged.metadata()?.len() {
        return Ok(false);
    }
    staged.rewind()?;
    let mut original = BufReader::with_capacity(EXPORT_BUFFER_SIZE, original);
    let mut staged = BufReader::with_capacity(EXPORT_BUFFER_SIZE, staged);
    loop {
        let left = original.fill_buf()?;
        let right = staged.fill_buf()?;
        let count = left.len().min(right.len());
        if count == 0 {
            return Ok(left.is_empty() && right.is_empty());
        }
        if left[..count] != right[..count] {
            return Ok(false);
        }
        original.consume(count);
        staged.consume(count);
    }
}

pub(crate) fn atomic_save(outputs: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    let mut staged = StagedSave::default();
    for (target, bytes) in outputs {
        staged.add(target, &bytes, false)?;
    }
    staged.publish()
}

#[cfg(test)]
mod save_transaction_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn guarded_save_detects_resource_edits_during_staging() {
        check_resource_edit_during_guarded_save(SaveStage::PrepareLevels);
    }

    #[test]
    fn reopening_a_guarded_save_detects_resource_edits_after_publication() {
        check_resource_edit_during_guarded_save(SaveStage::WriteProject);
    }

    fn check_resource_edit_during_guarded_save(change_stage: SaveStage) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let atlas = dir.path().join("Atlas.png");
        fs::write(&atlas, b"before").unwrap();
        let mut project = Project::new();
        project.add_tileset(&atlas, 16, 16, 16).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let (mut project, snapshot) = Project::load_with_snapshot(&path).unwrap();
        project.json["defaultLevelBgColor"] = json!("#123456");

        let mut published = PublishedDocuments::default();
        let result = project.save_document_with_progress(
            &path,
            false,
            Some(SaveVerification::AllWatchedFiles(&snapshot)),
            Some(&mut published),
            None,
            &mut |stage, done, total, _| {
                if stage == change_stage && total > 0 && done == total {
                    fs::write(&atlas, b"edited").unwrap();
                }
            },
        );

        assert_eq!(fs::read(&atlas).unwrap(), b"edited");
        if change_stage == SaveStage::PrepareLevels {
            assert!(matches!(result, Err(Error::FilesChanged(paths)) if paths == [atlas]));
            assert_eq!(fs::read(&path).unwrap(), before);
        } else {
            result.unwrap();
            assert_eq!(
                Project::load(&path).unwrap().json["defaultLevelBgColor"],
                "#123456"
            );
            let reopened =
                Project::load_snapshot_with_documents(&path, Some(published), Some(&snapshot));
            assert!(matches!(reopened, Err(Error::FilesChanged(paths)) if paths == [atlas]));
        }
    }

    #[test]
    fn guarded_save_preserves_a_new_output_path_created_during_staging() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let mut project = Project::new();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let existing = dir.path().join("Map/Level_0.ldtkl");
        let previous_level = fs::read(&existing).unwrap();
        let (mut project, snapshot) = Project::load_with_snapshot(&path).unwrap();
        let first = project.level_keys()[0];
        project.level_mut(first).unwrap()["bgColor"] = json!("#123456");
        project.add_level(None, "Next", 48, 48).unwrap();
        let target = dir.path().join("Map/Next.ldtkl");
        let mut created = false;
        let result = project.save_document_with_progress(
            &path,
            false,
            Some(SaveVerification::DocumentDestinations(&snapshot)),
            None,
            None,
            &mut |stage, done, total, _| {
                if stage == SaveStage::PrepareLevels && total > 0 && done == total {
                    fs::write(&target, b"other writer's level").unwrap();
                    created = true;
                }
            },
        );

        assert!(created);
        assert!(fs::read(&target).is_ok_and(|bytes| bytes == b"other writer's level"));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(existing).unwrap(), previous_level);
        assert!(matches!(result, Err(Error::FilesChanged(paths)) if paths == [target]));
        assert_eq!(project.level_keys().len(), 2);
    }

    #[test]
    fn guarded_save_does_not_retire_a_level_changed_during_staging() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world.ldtk");
        let mut project = Project::new();
        let kept = project.level_keys()[0];
        let removed = project.add_level(None, "Removed", 32, 32).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let removed_path = dir.path().join(
            project.level_stub(removed).unwrap()["externalRelPath"]
                .as_str()
                .unwrap(),
        );
        let mut outside = fs::read(&removed_path).unwrap();
        outside.push(b'\n');
        let (mut edited, snapshot) = Project::load_with_snapshot(&path).unwrap();
        edited.remove_level(removed).unwrap();
        edited.level_mut(kept).unwrap()["bgColor"] = json!("#112233");
        let mut replaced = false;
        let result = edited.save_document_with_progress(
            &path,
            false,
            Some(SaveVerification::DocumentDestinations(&snapshot)),
            None,
            None,
            &mut |stage, done, total, _| {
                if stage == SaveStage::PrepareLevels && total > 0 && done == total {
                    fs::write(&removed_path, &outside).unwrap();
                    replaced = true;
                }
            },
        );
        assert!(replaced);
        assert!(
            matches!(result, Err(Error::FilesChanged(paths)) if paths == [removed_path.clone()])
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(removed_path).unwrap(), outside);
        assert_eq!(edited.level_mut(kept).unwrap()["bgColor"], "#112233");
    }

    #[test]
    fn reopening_a_save_checks_the_disk_bytes_of_reused_external_levels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world.ldtk");
        let mut project = Project::new();
        let edited = project.level_keys()[0];
        let untouched = project.add_level(None, "Untouched", 256, 256).unwrap();
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let (mut loaded, snapshot) = Project::load_with_snapshot(&path).unwrap();
        let previous = loaded.clone();
        loaded.level_mut(edited).unwrap()["bgColor"] = Value::String("#112233".into());

        let mut published = PublishedDocuments::default();
        loaded
            .save_if_unchanged_capturing_documents(&path, &snapshot, Some(&mut published))
            .unwrap();
        assert_eq!(published.reusable_levels.len(), 1);
        let (target, (level, _, _)) = published.reusable_levels.iter().next().unwrap();
        assert!(std::ptr::eq(
            level.as_ref(),
            previous.level(untouched).unwrap()
        ));
        let mut bytes = fs::read(target).unwrap();
        bytes.push(b'\n');
        fs::write(target, bytes).unwrap();
        let error = Project::load_snapshot_with_documents(&path, Some(published), None)
            .err()
            .expect("changed reused level must fail reload");
        assert!(
            error
                .to_string()
                .contains("Project files changed during reload")
        );
    }

    #[test]
    fn save_and_reload_detects_same_size_changes_to_untouched_levels_after_publish() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world.ldtk");
        let mut project = Project::new();
        let edited = project.level_keys()[0];
        let untouched = project.add_level(None, "Untouched", 256, 256).unwrap();
        project.level_mut(untouched).unwrap()["note"] = json!("original");
        project.set_external_levels(true).unwrap();
        project.save(&path).unwrap();
        let (mut project, snapshot) = Project::load_with_snapshot(&path).unwrap();
        let target = dir.path().join(
            project.level_stub(untouched).unwrap()["externalRelPath"]
                .as_str()
                .unwrap(),
        );
        let original = fs::read_to_string(&target).unwrap();
        let changed = original.replace("\"original\"", "\"modified\"");
        assert_ne!(changed, original);
        assert_eq!(changed.len(), original.len());
        assert!(snapshot.changed_files().unwrap().is_empty());
        project.level_mut(edited).unwrap()["bgColor"] = json!("#112233");

        let mut published = PublishedDocuments::default();
        project
            .save_document_with_progress(
                &path,
                false,
                Some(SaveVerification::AllWatchedFiles(&snapshot)),
                Some(&mut published),
                None,
                &mut |stage, done, _, _| {
                    if stage == SaveStage::WriteProject && done == 1 {
                        fs::write(&target, &changed).unwrap();
                    }
                },
            )
            .unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), changed);
        let error = Project::load_snapshot_with_documents(&path, Some(published), None)
            .err()
            .expect("changed untouched level must fail reload");
        assert!(
            error
                .to_string()
                .contains("Project files changed during reload")
        );
        assert_eq!(
            Project::load(&path).unwrap().level(edited).unwrap()["bgColor"],
            "#112233"
        );
    }

    #[test]
    fn streamed_exports_preserve_identical_files_and_publish_changes_after_spilling() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("map.json");
        for len in [32, 2 * EXPORT_BUFFER_SIZE + 1] {
            let mut bytes = vec![b'a'; len];
            fs::write(&target, &bytes).unwrap();
            let mut staged = StagedSave::default();
            staged
                .add_writer(target.clone(), |writer| {
                    for chunk in bytes.chunks(4097) {
                        writer.write_all(chunk)?;
                    }
                    if len < EXPORT_BUFFER_SIZE {
                        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
                    }
                    Ok(())
                })
                .unwrap();
            assert!(staged.files.is_empty());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);

            bytes[len - 1] = b'z';
            staged
                .add_writer(target.clone(), |writer| writer.write_all(&bytes))
                .unwrap();
            assert_eq!(staged.files.len(), 1);
            assert_eq!(fs::read(&target).unwrap()[len - 1], b'a');
            staged.publish().unwrap();
            assert_eq!(fs::read(&target).unwrap(), bytes);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn writer_failure_cleans_up_both_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing.json");
        fs::write(&target, b"original").unwrap();

        for len in [32, EXPORT_BUFFER_SIZE + 1] {
            let mut staged = StagedSave::default();
            let result = staged.add_writer(target.clone(), |file| {
                file.write_all(&vec![b'p'; len])?;
                Err(io::Error::other("injected write failure"))
            });
            assert!(result.is_err());
            assert!(staged.files.is_empty());
            assert_eq!(fs::read(&target).unwrap(), b"original");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn a_late_rename_failure_restores_existing_files_and_removes_new_ones() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("existing.ldtkl");
        let map = dir.path().join("map.tmx");
        let created = dir.path().join("created.ldtkl");
        let last = dir.path().join("project.ldtk");
        fs::write(&first, b"old level").unwrap();
        fs::write(&last, b"old project").unwrap();
        let prefix = vec![b'0'; 64 * 1024 + 1];
        let original_map = [prefix.as_slice(), b"old"].concat();
        fs::write(&map, &original_map).unwrap();

        let mut staged = StagedSave::default();
        staged
            .add_parts(
                map.clone(),
                &[&prefix[..3], b"", &prefix[3..], b"old"],
                true,
            )
            .unwrap();
        assert!(staged.files.is_empty());
        staged
            .add_parts(
                map.clone(),
                &[&prefix[..3], b"", &prefix[3..], b"new"],
                true,
            )
            .unwrap();
        assert_eq!(staged.files.len(), 1);
        staged
            .add_writer(first.clone(), |file| file.write_all(b"new level"))
            .unwrap();
        staged.add(created.clone(), b"new level", false).unwrap();
        staged
            .files
            .push(StagedFile::final_replacement(last.clone(), b"new project").unwrap());
        fs::remove_file(&staged.files.last().unwrap().staged.0).unwrap();

        assert!(staged.publish().is_err());
        assert_eq!(fs::read(map).unwrap(), original_map);
        assert_eq!(fs::read(first).unwrap(), b"old level");
        assert_eq!(fs::read(last).unwrap(), b"old project");
        assert!(!created.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}

const LEVEL_SUMMARY_FIELDS: &[&str] = &[
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
];

fn summary_mut(root: &mut Value, key: LevelKey) -> &mut Value {
    let container = match key.world {
        Some(world) => &mut root["worlds"][world],
        None => root,
    };
    &mut container["levels"][key.level]
}

pub(crate) fn sync_summary(root: &mut Value, key: LevelKey, external: &Value) {
    let stub = summary_mut(root, key);
    for &field in LEVEL_SUMMARY_FIELDS {
        if let Some(value) = external.get(field) {
            stub[field] = value.clone();
        }
    }
}

impl Project {
    pub(crate) fn refresh_external_summary(&mut self, key: LevelKey) {
        let stub = summary_mut(&mut self.json, key);
        let Some(rel) = stub["externalRelPath"].as_str() else {
            return;
        };
        let external = self
            .external_levels
            .get(rel)
            .expect("validated external level");
        if let Some(doc) = external.as_object() {
            // Newly added summary keys follow the external document's order during editing.
            for (name, value) in doc {
                if LEVEL_SUMMARY_FIELDS.contains(&name.as_str()) {
                    stub[name] = value.clone();
                }
            }
            stub["layerInstances"] = Value::Null;
        }
    }

    /// Copy the currently saved project and external documents, retaining backupLimit copies.
    pub fn backup(&self) -> Result<Option<PathBuf>> {
        self.backup_with_progress(None, &mut |_, _, _| {})
    }

    fn backup_with_progress(
        &self,
        restoring_from: Option<&Path>,
        progress: &mut impl FnMut(usize, usize, Option<&str>),
    ) -> Result<Option<PathBuf>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        if !path.exists() {
            return Ok(None);
        }
        let base = path.parent().expect("absolute project path");
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let directory = absolute(&self.backup_directory().expect("saved project path"))?;
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
        let mut staged = StagedSave::default();
        let mut remapped = false;
        let mut loose_levels = Vec::new();
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
                        loose_levels.push(source);
                    }
                }
            }
        }
        let keys = if corrupt { vec![] } else { saved.level_keys() };
        let total = 2
            + loose_levels.len()
            + keys
                .iter()
                .filter(|&&key| {
                    saved.level_stub(key).expect("enumerated level")["externalRelPath"].is_string()
                })
                .count();
        let mut done = 0;
        for source in loose_levels {
            let filename = source.file_name().expect("backup level filename");
            progress(done, total, filename.to_str());
            let target = destination.join(stem.as_ref()).join(filename);
            ensure_parent_directory(&target)?;
            let bytes = fs::read(&source).map_err(|error| Error::Io {
                path: source,
                source: error,
            })?;
            staged.add(target, &bytes, false)?;
            done += 1;
        }
        for (index, key) in keys.into_iter().enumerate() {
            if let Some(rel) =
                external_path(saved.level_stub(key).expect("enumerated level"))?.map(str::to_owned)
            {
                let source = absolute(&base.join(&rel))?;
                progress(
                    done,
                    total,
                    source.file_name().and_then(|name| name.to_str()),
                );
                let (bytes, repaired) = match fs::read(&source) {
                    Ok(bytes) => (bytes, None),
                    Err(source_error) => {
                        let error = Error::Io {
                            path: source.clone(),
                            source: source_error,
                        };
                        if saved.json["externalLevels"] != true {
                            return Err(error);
                        }
                        let stub = saved.level_stub(key).expect("enumerated level");
                        let (repaired, _, bytes) =
                            recover_renamed_external_level(path, &rel, stub, error)?;
                        (bytes, Some(repaired))
                    }
                };
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
                    repaired.unwrap_or(rel)
                };
                let target = destination.join(relative);
                ensure_parent_directory(&target)?;
                staged.add(target, &bytes, false)?;
                done += 1;
            }
        }
        let filename = path.file_name().expect("project filename");
        progress(done, total, filename.to_str());
        let root = if remapped {
            encode(&saved.json, saved.json["minifyJson"] == true)?
        } else {
            original
        };
        staged.add(destination.join(filename), &root, false)?;
        staged.publish()?;
        progress(done + 1, total, None);
        let mut backups = self.list_backups()?;
        let mut directories = BTreeSet::new();
        backups.retain(|backup| directories.insert(backup.path.parent().unwrap().to_path_buf()));
        let mut limit = self.json["backupLimit"].as_u64().unwrap_or(10) as usize;
        if let Some(source) = restoring_from.and_then(Path::parent) {
            backups.retain(|backup| !same_existing_file(backup.path.parent().unwrap(), source));
            limit = limit.max(1);
        }
        for backup in backups.into_iter().skip(limit) {
            let old = backup.path.parent().unwrap().to_path_buf();
            fs::remove_dir_all(&old).map_err(|source| Error::Io { path: old, source })?;
        }
        progress(total, total, None);
        Ok(Some(destination))
    }
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

fn is_case_only_rename_target(previous: &Path, target: &Path) -> bool {
    let (Some(previous_name), Some(target_name), Some(parent)) =
        (previous.file_name(), target.file_name(), target.parent())
    else {
        return false;
    };
    previous.parent() == Some(parent)
        && previous_name.eq_ignore_ascii_case(target_name)
        && same_existing_file(previous, target)
        // Distinct entries can share an inode on a case-sensitive filesystem.
        && fs::read_dir(parent).is_ok_and(|mut entries| {
            entries.all(|entry| entry.is_ok_and(|entry| entry.file_name() != target_name))
        })
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
    /// Fingerprints of the documents published by the save, before configured exports.
    pub document_snapshot: Option<FilesystemSnapshot>,
    pub outputs: Vec<PathBuf>,
    pub export_errors: Vec<String>,
    /// Resource problems to log separately from the success or failure of an export.
    pub export_diagnostics: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStage {
    PrepareLevels,
    Backup,
    WriteProject,
    RemoveObsoleteLevels,
    ExportPng,
    ExportSimplified,
    ExportTiled,
}

impl Project {
    /// Save the document first, then attempt configured exports independently.
    /// Export diagnostics never disguise a successfully published document as an unsaved one.
    pub fn save_with_exports(&mut self, path: impl AsRef<Path>) -> Result<SaveReport> {
        self.save_with_operation_progress(path, |_, _, _| {})
    }
    /// Reports completed preparation and export tasks, with (0, 0) ending each stage.
    pub fn save_with_exports_progress(
        &mut self,
        path: impl AsRef<Path>,
        mut progress: impl FnMut(&'static str, usize, usize),
    ) -> Result<SaveReport> {
        self.save_with_operation_progress(path, |stage, done, total| match stage {
            SaveStage::PrepareLevels => progress("Preparing levels", done, total),
            SaveStage::Backup | SaveStage::WriteProject | SaveStage::RemoveObsoleteLevels => {}
            _ => progress("Processing export tasks", done, total),
        })
    }
    /// Report the active operation while preparing, publishing, and exporting a project.
    /// Export and preparation counters use (0, 0) to close their current progress bar.
    /// WriteProject reaches (1, 1) only after the project is published.
    pub fn save_with_operation_progress(
        &mut self,
        path: impl AsRef<Path>,
        mut progress: impl FnMut(SaveStage, usize, usize),
    ) -> Result<SaveReport> {
        self.save_with_labeled_operation_progress(path, |stage, done, total, _| {
            progress(stage, done, total);
        })
    }
    /// Also reports the current level identifier or filename when available.
    pub fn save_with_labeled_operation_progress(
        &mut self,
        path: impl AsRef<Path>,
        progress: impl FnMut(SaveStage, usize, usize, Option<&str>),
    ) -> Result<SaveReport> {
        self.save_with_snapshot_and_progress(path, None, progress)
    }

    /// Save with progress, checking watched destinations just before publishing them.
    pub fn save_with_snapshot_and_progress(
        &mut self,
        path: impl AsRef<Path>,
        snapshot: Option<&FilesystemSnapshot>,
        mut progress: impl FnMut(SaveStage, usize, usize, Option<&str>),
    ) -> Result<SaveReport> {
        let mut document_snapshot = FilesystemSnapshot::default();
        self.save_document_with_progress(
            path,
            true,
            snapshot.map(SaveVerification::DocumentDestinations),
            None,
            Some(&mut document_snapshot),
            &mut progress,
        )?;
        let mut report = self.configured_exports_report_progress(&mut |stage, done, total| {
            progress(stage, done, total, None);
        });
        report.document_snapshot = Some(document_snapshot);
        Ok(report)
    }
    pub fn configured_exports_report(&self) -> SaveReport {
        self.configured_exports_report_progress(&mut |_, _, _| {})
    }
    fn configured_exports_report_progress(
        &self,
        progress: &mut impl FnMut(SaveStage, usize, usize),
    ) -> SaveReport {
        let Some(path) = self.path.as_ref() else {
            return SaveReport::default();
        };
        let dir = self.external_files_directory().expect("saved project path");
        let mut report = SaveReport {
            outputs: vec![path.clone()],
            ..SaveReport::default()
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
            let outputs = self.export_simplified_with_diagnostics(
                &simple,
                |done, total| progress(SaveStage::ExportSimplified, done, total),
                &mut report.export_diagnostics,
            );
            if collect_configured_export(
                &mut report,
                &simple,
                &["json", "csv", "png"],
                wrap(outputs),
            ) {
                collect_configured_export(&mut report, &png, &["png"], wrap(Ok(vec![])));
            }
        } else {
            let outputs = if self.json["imageExportMode"]
                .as_str()
                .is_some_and(|mode| mode != "None")
            {
                self.export_png_with_diagnostics(
                    &png,
                    |done, total| progress(SaveStage::ExportPng, done, total),
                    &mut report.export_diagnostics,
                )
            } else {
                Ok(vec![])
            };
            if collect_configured_export(&mut report, &png, &["png"], wrap(outputs)) {
                collect_configured_export(
                    &mut report,
                    &simple,
                    &["json", "csv", "png"],
                    wrap(Ok(vec![])),
                );
            }
        }
        let output = if self.json["exportTiled"] == true {
            self.export_tiled_with_report_progress(&tiled, |done, total| {
                progress(SaveStage::ExportTiled, done, total)
            })
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
        self.save_if_unchanged_capturing_documents(path, snapshot, None)
    }

    /// Save and reopen the published document against a new watch baseline.
    /// The outer result describes the save and the inner result describes reloading.
    pub fn save_and_reload_if_unchanged(
        &mut self,
        path: impl AsRef<Path>,
        snapshot: &FilesystemSnapshot,
    ) -> Result<Result<FilesystemSnapshot>> {
        let mut published = PublishedDocuments::default();
        self.save_if_unchanged_capturing_documents(path, snapshot, Some(&mut published))?;
        let path = self.path.as_deref().expect("published document path");
        Ok(
            Self::load_snapshot_with_documents(path, Some(published), Some(snapshot)).map(
                |(project, snapshot)| {
                    *self = project;
                    snapshot
                },
            ),
        )
    }

    fn save_if_unchanged_capturing_documents(
        &mut self,
        path: impl AsRef<Path>,
        snapshot: &FilesystemSnapshot,
        published: Option<&mut PublishedDocuments>,
    ) -> Result<()> {
        let changed = snapshot.changed_files()?;
        if !changed.is_empty() {
            return Err(Error::FilesChanged(changed));
        }
        self.save_document_with_progress(
            path,
            true,
            Some(SaveVerification::AllWatchedFiles(snapshot)),
            published,
            None,
            &mut |_, _, _, _| {},
        )
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
    let numbered = suffix
        .strip_prefix('_')
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|c| c.is_ascii_digit()));
    if !suffix.is_empty() && !numbered {
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

/// Return whether the requested output set was published, independently of cleanup.
fn collect_configured_export(
    report: &mut SaveReport,
    dir: &Path,
    extensions: &[&str],
    result: Result<crate::exports::ExportReport>,
) -> bool {
    match result {
        Ok(output) => {
            if let Err(error) = prune_managed_exports(dir, &output.outputs, extensions) {
                report.export_errors.push(error.to_string());
            }
            report.outputs.extend(output.outputs);
            report.export_errors.extend(output.warnings);
            true
        }
        Err(error) => {
            report.export_errors.push(error.to_string());
            false
        }
    }
}
fn prune_managed_exports(dir: &Path, keep: &[PathBuf], extensions: &[&str]) -> Result<()> {
    // Check the project-associated directory before entering a managed export folder.
    for path in dir.parent().into_iter().chain([dir]) {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_symlink() => return Ok(()),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(());
            }
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        }
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
