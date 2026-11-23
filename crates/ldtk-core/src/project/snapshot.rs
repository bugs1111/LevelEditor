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
#[derive(Clone, Debug)]
pub struct FilesystemSnapshot {
    pub(super) files: BTreeMap<PathBuf, Option<u64>>,
}
impl FilesystemSnapshot {
    pub fn refresh_file(&mut self, path: &Path) -> Result<()> {
        self.files.insert(path.to_owned(), file_fingerprint(path)?);
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
        };
        self.snapshot_resources(&mut snapshot)?;
        Ok(snapshot)
    }

    pub(super) fn snapshot_resources(&self, snapshot: &mut FilesystemSnapshot) -> Result<()> {
        let mut paths = BTreeSet::new();
        if let Some(path) = &self.path {
            paths.insert(path.clone());
            let base = path.parent().expect("absolute project path");
            collect_resource_paths(&self.json, base, &mut paths)?;
            for level in self.external_levels.values() {
                collect_resource_paths(level, base, &mut paths)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
