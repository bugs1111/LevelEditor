use super::*;

impl Project {
    /// Write the current in-memory document to a new source-compatible crash backup.
    /// Returns its main project file. Never changes this project, rotates backups,
    /// invokes save hooks/exports, or reads the saved document as recovery content.
    /// An unsaved project uses `fallback_root/Untitled.ldtk` as its virtual source;
    /// pass that path to `load_backup` when recovering its relative resources.
    pub fn write_crash_recovery(&self, fallback_root: impl AsRef<Path>) -> Result<PathBuf> {
        let original = match &self.path {
            Some(path) => absolute(path)?,
            None => absolute(fallback_root.as_ref())?.join("Untitled.ldtk"),
        };
        let base = original
            .parent()
            .ok_or_else(|| Error::Invalid("project path has no parent".into()))?;
        let filename = original
            .file_name()
            .ok_or_else(|| Error::Invalid("project path has no filename".into()))?;
        let stem = original.file_stem().unwrap_or_default();
        let directory = match self.json["backupRelPath"].as_str() {
            Some(rel) => absolute(&base.join(rel))?,
            None => base.join(stem).join("backups"),
        };
        let id = self.json["iid"].as_str().unwrap_or("backup");
        if id.contains('/') || id.contains('\\') {
            return Err(Error::Invalid("unsafe backup project IID".into()));
        }
        let mut snapshot = self.clone();
        if self.path.is_none() {
            // Unsaved relative resources resolve against the working directory.
            let old_base = absolute(Path::new("."))?;
            rebase_paths(&mut snapshot.json, &old_base, base)?;
            for level in snapshot.external_levels.values_mut() {
                rebase_paths(level, &old_base, base)?;
            }
        }
        let compatibility = if snapshot.json["jsonVersion"] == "1.5.4" {
            snapshot.refresh_table_of_contents()?;
            Some(OutputCompatibility::new(&snapshot.json))
        } else {
            None
        };
        let minify = snapshot.json["minifyJson"] == true;
        let mut outputs = Vec::new();
        let mut paths = BTreeSet::from([PathBuf::from(filename)]);
        for (index, key) in snapshot.level_keys().into_iter().enumerate() {
            let Some(rel) = external_path(snapshot.level_stub(key).expect("enumerated level"))?
                .map(str::to_owned)
            else {
                continue;
            };
            let mut level = snapshot.external_levels.get(&rel).cloned().ok_or_else(|| {
                Error::Invalid(format!("external level {rel} has not been loaded"))
            })?;
            if !level.is_object() {
                return Err(Error::Invalid(format!(
                    "external level {rel} must be an object"
                )));
            }
            // Crash recovery must never escape its new directory, even when a
            // legitimate source level lives outside the source project folder.
            let target = if Path::new(&rel)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
                && !rel.contains('\\')
            {
                PathBuf::from(&rel)
            } else {
                PathBuf::from(stem).join(format!("external-{index:04}.ldtkl"))
            };
            if !paths.insert(target.clone()) {
                return Err(Error::Invalid(format!(
                    "duplicate recovery output path: {}",
                    target.display()
                )));
            }
            let target_rel = target.to_string_lossy().replace('\\', "/");
            snapshot.level_stub_mut(key).expect("enumerated level")["externalRelPath"] =
                Value::String(target_rel.clone());
            snapshot.level_stub_mut(key).expect("enumerated level")["layerInstances"] = Value::Null;
            if level.get("externalRelPath").is_some() {
                level["externalRelPath"] = Value::String(target_rel);
            }
            sync_summary(&mut snapshot.json, key, &level);
            if let Some(compatibility) = &compatibility {
                if let Some(header) = updated_external_level_header(&level) {
                    apply_external_level_header(&mut level, header);
                }
                compatibility.apply_level(&mut level, &snapshot.grid_presence);
            }
            outputs.push((target, encode(&level, minify)?));
        }
        if let Some(compatibility) = compatibility {
            compatibility.project_output(&mut snapshot.json, &snapshot.grid_presence);
        }
        outputs.push((PathBuf::from(filename), encode(&snapshot.json, minify)?));
        fs::create_dir_all(&directory).map_err(|source| Error::Io {
            path: directory.clone(),
            source,
        })?;
        let name = format!("{id}_{}_crash", backup_timestamp());
        let mut destination = directory.join(&name);
        let mut suffix = 2;
        loop {
            match fs::create_dir(&destination) {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    destination = directory.join(format!("{name}_{suffix:04}"));
                    suffix += 1;
                }
                Err(source) => {
                    return Err(Error::Io {
                        path: destination,
                        source,
                    });
                }
            }
        }
        let publish = (|| {
            let outputs: Vec<_> = outputs
                .into_iter()
                .map(|(rel, bytes)| (destination.join(rel), bytes))
                .collect();
            for (path, _) in &outputs {
                let parent = path.parent().expect("recovery file parent");
                fs::create_dir_all(parent).map_err(|source| Error::Io {
                    path: parent.into(),
                    source,
                })?;
            }
            atomic_save(outputs)
        })();
        if let Err(error) = publish {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(destination.join(filename))
    }
}
