use super::{Project, absolute};
use crate::{Error, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    hash::{DefaultHasher, Hasher},
    io::{self, Read},
    path::{Path, PathBuf},
};

/// A content snapshot, independent of the mutable project, for filesystem watchers.
#[derive(Clone, Debug, Default)]
pub struct FilesystemSnapshot {
    files: BTreeMap<PathBuf, FileFingerprint>,
    documents: BTreeSet<PathBuf>,
    pub(super) loaded_external_paths: Option<(PathBuf, Vec<String>)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileFingerprint {
    Known(Option<u64>),
    Unreadable,
}

impl FilesystemSnapshot {
    /// Capture the destination document and its registered external levels.
    /// An absent destination is tracked so a later creation is detected.
    pub fn for_save_destination(path: &Path) -> Result<Self> {
        let path = absolute(path)?;
        let mut snapshot = Self::default();
        snapshot.record_document(path.clone(), file_fingerprint(&path)?);
        let levels = super::previous_external_paths(&path);
        let parent = path.parent().expect("absolute project path");
        for level in &levels {
            let level = absolute(&parent.join(level))?;
            let fingerprint = file_fingerprint(&level)?;
            snapshot.record_document(level, fingerprint);
        }
        snapshot.loaded_external_paths = Some((path, levels));
        Ok(snapshot)
    }

    pub fn refresh_file(&mut self, path: &Path) -> Result<()> {
        self.files.insert(
            path.to_owned(),
            FileFingerprint::Known(file_fingerprint(path)?),
        );
        if self
            .loaded_external_paths
            .as_ref()
            .is_some_and(|(root, _)| root == path)
        {
            self.loaded_external_paths = None;
        }
        Ok(())
    }

    pub(super) fn record_document(&mut self, path: PathBuf, fingerprint: Option<u64>) {
        self.documents.insert(path.clone());
        self.files.insert(path, FileFingerprint::Known(fingerprint));
    }

    pub(super) fn rename_document(&mut self, old: &Path, new: &Path) {
        if let Some(fingerprint) = self.files.remove(old) {
            self.documents.remove(old);
            self.documents.insert(new.to_owned());
            self.files.insert(new.to_owned(), fingerprint);
        }
        if let Some((root, _)) = &mut self.loaded_external_paths
            && root == old
        {
            *root = new.to_owned();
        }
    }

    pub fn is_document(&self, path: &Path) -> bool {
        self.documents.contains(path)
    }

    pub(super) fn document_fingerprint(&self, path: &Path) -> Option<u64> {
        match self.files.get(path) {
            Some(FileFingerprint::Known(fingerprint)) if self.is_document(path) => *fingerprint,
            _ => None,
        }
    }

    /// Accept the listed document contents while retaining the loaded manifest and other watches.
    pub fn acknowledge_document_changes(&mut self, paths: &[PathBuf]) -> Result<()> {
        let fingerprints = paths
            .iter()
            .map(|path| Ok((path.clone(), file_fingerprint(path)?)))
            .collect::<Result<Vec<_>>>()?;
        for (path, fingerprint) in fingerprints {
            self.record_document(path, fingerprint);
        }
        Ok(())
    }

    pub fn refresh_resource_file(&mut self, path: &Path) -> Result<()> {
        let path = absolute(path)?;
        if !self.is_document(&path) {
            self.refresh_file(&path)?;
        }
        Ok(())
    }

    /// Preserve existing resource fingerprints while accepting a new document baseline.
    pub fn carry_resource_watches(&mut self, previous: &Self) {
        for (path, fingerprint) in &previous.files {
            if !previous.is_document(path) && !self.is_document(path) {
                self.files.entry(path.clone()).or_insert(*fingerprint);
            }
        }
    }

    /// Check readable resources independently so an unreadable one cannot mask another.
    pub fn changed_resource_files(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|(path, old)| {
                !self.is_document(path)
                    && file_fingerprint(path).is_ok_and(|new| FileFingerprint::Known(new) != **old)
            })
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        self.changed_files_matching(|_| true)
    }

    /// Only read watched paths included in this check.
    pub fn changed_files_matching(&self, include: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        for (path, fingerprint) in &self.files {
            if include(path) {
                match file_fingerprint(path) {
                    Ok(current) if FileFingerprint::Known(current) != *fingerprint => {
                        changed.push(path.clone());
                    }
                    Ok(_) => {}
                    Err(_) if *fingerprint == FileFingerprint::Unreadable => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(changed)
    }
}
impl Project {
    pub fn filesystem_snapshot(&self) -> Result<FilesystemSnapshot> {
        let mut snapshot = FilesystemSnapshot {
            documents: self.saved_document_paths()?,
            ..Default::default()
        };
        self.add_resource_watches(&mut snapshot, true)?;
        Ok(snapshot)
    }

    /// Watch unavailable resources so they can be detected when they become readable.
    pub fn watch_available_resources(&self, snapshot: &mut FilesystemSnapshot) {
        let _ = self.add_resource_watches(snapshot, false);
    }

    /// Refresh reported resources and reconcile references without rebasing documents.
    pub fn refresh_resource_watches(
        &self,
        snapshot: &mut FilesystemSnapshot,
        changed: &BTreeSet<PathBuf>,
    ) {
        let (Ok(mut paths), Ok(documents)) =
            (self.resource_watch_paths(), self.saved_document_paths())
        else {
            return;
        };
        paths.retain(|path| !documents.contains(path));
        snapshot.files.retain(|path, _| {
            snapshot.documents.contains(path) || paths.contains(path) && !changed.contains(path)
        });
        let _ = add_file_watches(snapshot, paths, false);
    }

    fn add_resource_watches(
        &self,
        snapshot: &mut FilesystemSnapshot,
        require_readable: bool,
    ) -> Result<()> {
        add_file_watches(snapshot, self.resource_watch_paths()?, require_readable)
    }

    fn saved_document_paths(&self) -> Result<BTreeSet<PathBuf>> {
        let Some(path) = &self.path else {
            return Ok(BTreeSet::new());
        };
        let base = path.parent().expect("absolute project path");
        let levels = self
            .iter_level_keys()
            .filter_map(|key| self.level_stub(key)?["externalRelPath"].as_str());
        std::iter::once(Ok(path.clone()))
            .chain(levels.map(|path| absolute(&base.join(path))))
            .collect()
    }

    pub(super) fn resource_watch_paths(&self) -> Result<BTreeSet<PathBuf>> {
        let mut paths = self.saved_document_paths()?;
        if let Some(path) = &self.path {
            let base = path.parent().expect("absolute project path");
            // Many layers and fields refer to the same asset.
            let mut references = BTreeSet::new();
            collect_resource_paths(&self.json, &mut references);
            for level in self.external_levels.values() {
                collect_resource_paths(level, &mut references);
            }
            for reference in references {
                paths.insert(absolute(&base.join(reference))?);
            }
        }
        Ok(paths)
    }
}

fn add_file_watches(
    snapshot: &mut FilesystemSnapshot,
    paths: BTreeSet<PathBuf>,
    require_readable: bool,
) -> Result<()> {
    for path in paths {
        if let std::collections::btree_map::Entry::Vacant(entry) = snapshot.files.entry(path) {
            match file_fingerprint(entry.key()) {
                Ok(fingerprint) => {
                    entry.insert(FileFingerprint::Known(fingerprint));
                }
                Err(error) if require_readable => return Err(error),
                Err(_) => {
                    entry.insert(FileFingerprint::Unreadable);
                }
            }
        }
    }
    Ok(())
}

// Feed identical chunks to the hasher for parsed document bytes and watched files.
const HASH_CHUNK: usize = 64 * 1024;

pub(super) fn bytes_fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = DefaultHasher::new();
    for chunk in bytes.chunks(HASH_CHUNK) {
        hash.write(chunk);
    }
    hash.finish()
}

fn file_fingerprint(path: &Path) -> Result<Option<u64>> {
    let io_error = |source| Error::Io {
        path: path.into(),
        source,
    };
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(e)),
    };
    let mut hash = DefaultHasher::new();
    let mut chunk = Vec::with_capacity(HASH_CHUNK);
    loop {
        chunk.clear();
        (&mut file)
            .take(HASH_CHUNK as u64)
            .read_to_end(&mut chunk)
            .map_err(io_error)?;
        if !chunk.is_empty() {
            hash.write(&chunk);
        }
        if chunk.len() < HASH_CHUNK {
            return Ok(Some(hash.finish()));
        }
    }
}

fn collect_resource_paths<'a>(value: &'a Value, paths: &mut BTreeSet<&'a str>) {
    match value {
        Value::Object(object) => {
            let mut file_field = None;
            for (key, value) in object {
                match key.as_str() {
                    "relPath" | "externalRelPath" | "bgRelPath" | "__tilesetRelPath" => {
                        if let Some(path) = value.as_str().filter(|path| !path.is_empty()) {
                            paths.insert(path);
                        }
                    }
                    "__value" | "realEditorValues" | "realEditorValue" | "defaultOverride" => {
                        let is_file = file_field.get_or_insert_with(|| {
                            object
                                .get("__type")
                                .and_then(Value::as_str)
                                .is_some_and(|t| t.contains("FilePath"))
                                || object
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .is_some_and(|t| t == "F_Path" || t == "F_File")
                        });
                        if *is_file {
                            collect_field_paths(value, paths);
                        }
                    }
                    _ => (),
                }
                if matches!(value, Value::Object(_) | Value::Array(_)) {
                    collect_resource_paths(value, paths);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                if matches!(value, Value::Object(_) | Value::Array(_)) {
                    collect_resource_paths(value, paths);
                }
            }
        }
        _ => (),
    }
}
fn collect_field_paths<'a>(value: &'a Value, paths: &mut BTreeSet<&'a str>) {
    match value {
        Value::String(s) if !s.is_empty() => {
            paths.insert(s);
        }
        Value::Array(v) => {
            for v in v {
                collect_field_paths(v, paths);
            }
        }
        Value::Object(o) => {
            if let Some(v) = o.get("params") {
                collect_field_paths(v, paths);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn watches_shared_paths_and_legacy_fields_across_project_and_external_levels() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("game.ldtk");
        let atlas = dir.path().join("atlas.png");
        let missing = dir.path().join("missing.png");
        let absolute = dir.path().join("absolute.png");
        std::fs::write(&atlas, b"atlas").unwrap();
        let mut project = Project::new();
        project.path = Some(root.clone());
        project.json["extension"] = json!([
            {"relPath":"atlas.png"},
            {"type":"F_File","defaultOverride":{"params":[absolute]}},
            {"__type":"String","__value":"not-a-resource.png"}
        ]);
        project.external_levels.insert(
            "level.ldtkl".into(),
            json!({
                "__tilesetRelPath":"./atlas.png",
                "fields":[{"__type":"Array<FilePath>","__value":["sub/../atlas.png","missing.png",""]}]
            }),
        );

        let snapshot = project.filesystem_snapshot().unwrap();
        assert_eq!(
            snapshot.files.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from([root, atlas.clone(), missing.clone(), absolute])
        );
        std::fs::write(&atlas, b"other").unwrap();
        std::fs::write(&missing, b"created").unwrap();
        assert_eq!(snapshot.changed_files().unwrap(), [atlas, missing]);
    }

    #[test]
    fn disk_and_memory_hashes_match_at_chunk_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data");
        let bytes: Vec<_> = (0..2 * HASH_CHUNK + 1).map(|n| n as u8).collect();
        for size in [
            0,
            1,
            HASH_CHUNK - 1,
            HASH_CHUNK,
            HASH_CHUNK + 1,
            bytes.len(),
        ] {
            let sample = &bytes[..size];
            std::fs::write(&path, sample).unwrap();
            assert_eq!(
                file_fingerprint(&path).unwrap(),
                Some(bytes_fingerprint(sample))
            );
        }
    }
}
