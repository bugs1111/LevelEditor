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
    pub(super) files: BTreeMap<PathBuf, Option<u64>>,
    pub(super) loaded_external_paths: Option<(PathBuf, Vec<String>)>,
}
impl FilesystemSnapshot {
    /// Capture the destination document and its registered external levels.
    /// An absent destination is tracked so a later creation is detected.
    pub fn for_save_destination(path: &Path) -> Result<Self> {
        let path = absolute(path)?;
        let mut snapshot = Self::default();
        snapshot.refresh_file(&path)?;
        let levels = super::previous_external_paths(&path);
        let parent = path.parent().expect("absolute project path");
        for level in &levels {
            snapshot.refresh_file(&absolute(&parent.join(level))?)?;
        }
        snapshot.loaded_external_paths = Some((path, levels));
        Ok(snapshot)
    }

    pub fn refresh_file(&mut self, path: &Path) -> Result<()> {
        self.files.insert(path.to_owned(), file_fingerprint(path)?);
        if self
            .loaded_external_paths
            .as_ref()
            .is_some_and(|(root, _)| root == path)
        {
            self.loaded_external_paths = None;
        }
        Ok(())
    }

    pub fn changed_files(&self) -> Result<Vec<PathBuf>> {
        self.changed_files_matching(|_| true)
    }

    /// Only read watched paths included in this check.
    pub fn changed_files_matching(&self, include: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
        let mut changed = Vec::new();
        for (path, fingerprint) in &self.files {
            if include(path) && &file_fingerprint(path)? != fingerprint {
                changed.push(path.clone());
            }
        }
        Ok(changed)
    }
}
impl Project {
    pub fn filesystem_snapshot(&self) -> Result<FilesystemSnapshot> {
        let mut snapshot = FilesystemSnapshot {
            files: BTreeMap::new(),
            loaded_external_paths: None,
        };
        self.snapshot_resources(&mut snapshot)?;
        Ok(snapshot)
    }

    pub(super) fn snapshot_resources(&self, snapshot: &mut FilesystemSnapshot) -> Result<()> {
        let mut paths = BTreeSet::new();
        if let Some(path) = &self.path {
            paths.insert(path.clone());
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
        for path in paths {
            if let std::collections::btree_map::Entry::Vacant(entry) = snapshot.files.entry(path) {
                let fingerprint = file_fingerprint(entry.key())?;
                entry.insert(fingerprint);
            }
        }
        Ok(())
    }
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
