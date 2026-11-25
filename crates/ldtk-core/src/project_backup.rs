use super::*;
#[derive(Clone, Debug)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub timestamp: String,
    pub crash: bool,
}
impl BackupInfo {
    /// The timestamp in a backup directory is local time, including for old backups.
    #[cfg(any(unix, windows))]
    fn local_calendar(timestamp: &str) -> Option<(libc::tm, u64)> {
        let number =
            |range: std::ops::Range<usize>| timestamp.get(range)?.parse::<libc::c_int>().ok();
        // SAFETY: tm consists of integers and, on some platforms, a nullable zone pointer.
        let mut date: libc::tm = unsafe { std::mem::zeroed() };
        date.tm_year = number(0..4)? - 1900;
        date.tm_mon = number(5..7)? - 1;
        date.tm_mday = number(8..10)?;
        date.tm_hour = number(11..13)?;
        date.tm_min = number(14..16)?;
        date.tm_sec = number(17..19)?;
        date.tm_isdst = -1;
        #[cfg(windows)]
        unsafe extern "C" {
            #[link_name = "_mktime64"]
            fn mktime(date: *mut libc::tm) -> i64;
        }
        #[cfg(unix)]
        use libc::mktime;
        // SAFETY: date is a writable, initialized tm with a local calendar time.
        let seconds = u64::try_from(unsafe { mktime(&mut date) }).ok()?;
        Some((date, seconds))
    }

    #[cfg(any(unix, windows))]
    pub fn age_in_seconds(&self, now: std::time::SystemTime) -> Option<u64> {
        let (_, seconds) = Self::local_calendar(&self.timestamp)?;
        now.duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs()
            .checked_sub(seconds)
    }

    #[cfg(any(unix, windows))]
    fn date_label(timestamp: &str) -> Option<String> {
        let (mut date, seconds) = Self::local_calendar(timestamp)?;
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let day = days.get(date.tm_wday as usize)?;
        let month = months.get(date.tm_mon as usize)?;
        let mut zone = [0_u8; 128];
        #[cfg(unix)]
        use libc::{strftime, timegm};
        #[cfg(windows)]
        unsafe extern "C" {
            #[link_name = "_mkgmtime64"]
            fn timegm(date: *mut libc::tm) -> i64;
            fn strftime(
                s: *mut libc::c_char,
                max: usize,
                fmt: *const libc::c_char,
                date: *const libc::tm,
            ) -> usize;
        }
        // SAFETY: zone is writable and strftime receives an initialized tm.
        let length =
            unsafe { strftime(zone.as_mut_ptr().cast(), zone.len(), c"%Z".as_ptr(), &date) };
        let zone = String::from_utf8_lossy(&zone[..length]);
        let mut label = format!(
            "{day} {month} {:02} {:04} {:02}:{:02}:{:02}",
            date.tm_mday,
            date.tm_year + 1900,
            date.tm_hour,
            date.tm_min,
            date.tm_sec
        );
        // SAFETY: timegm receives a valid tm and may normalize the writable copy.
        let utc_seconds = i128::from(unsafe { timegm(&mut date) });
        let offset = (utc_seconds - i128::from(seconds)) / 60;
        let sign = if offset < 0 { '-' } else { '+' };
        let minutes = offset.unsigned_abs();
        label.push_str(&format!(" GMT{sign}{:02}{:02}", minutes / 60, minutes % 60));
        if minutes == 0 && matches!(zone.as_ref(), "UTC" | "GMT" | "Coordinated Universal Time") {
            label.push_str(" (Coordinated Universal Time)");
        } else if !zone.is_empty() {
            label.push_str(&format!(" ({zone})"));
        }
        Some(label)
    }
}

// Only these two root keys are needed to discover backups. Unknown document
// content is consumed with IgnoredAny, including large strings and tile arrays.
#[derive(Default)]
struct BackupMetadata {
    iid: Option<String>,
    backup_rel_path: Option<String>,
}
struct MetadataString(Option<String>);
impl<'de> serde::Deserialize<'de> for MetadataString {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = MetadataString;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a string or ignored JSON value")
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(Some(value.into())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(Some(value)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(None))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                _: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(None))
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(None))
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(None))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Self::Value, E> {
                Ok(MetadataString(None))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                while values.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(MetadataString(None))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                while values
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(MetadataString(None))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
impl<'de> serde::Deserialize<'de> for BackupMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = BackupMetadata;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("project metadata")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut values: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut metadata = BackupMetadata::default();
                while let Some(key) = values.next_key::<String>()? {
                    match key.as_str() {
                        "iid" => metadata.iid = values.next_value::<MetadataString>()?.0,
                        "backupRelPath" => {
                            metadata.backup_rel_path = values.next_value::<MetadataString>()?.0
                        }
                        _ => {
                            values.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(metadata)
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
fn read_backup_metadata(path: &Path) -> Result<BackupMetadata> {
    use std::io::BufRead;
    let file = fs::File::open(path).map_err(|source| Error::Io {
        path: path.into(),
        source,
    })?;
    let mut reader = std::io::BufReader::new(file);
    let object = loop {
        let bytes = reader.fill_buf().map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
        if bytes.is_empty() {
            break false;
        }
        if let Some(first) = bytes
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            break bytes[first] == b'{';
        }
        let length = bytes.len();
        reader.consume(length);
    };
    // Valid non-object JSON must not trigger the corrupt-backup fallback. Still
    // consume it fully so malformed arrays/strings or trailing data remain errors.
    if object {
        serde_json::from_reader(reader)
    } else {
        serde_json::from_reader::<_, serde::de::IgnoredAny>(reader)
            .map(|_| BackupMetadata::default())
    }
    .map_err(|source| Error::Json {
        path: path.into(),
        source,
    })
}

fn backup_directory(path: &Path, relative: Option<&str>) -> PathBuf {
    relative
        .map(|relative| path.with_file_name(relative))
        .unwrap_or_else(|| path.with_extension("").join("backups"))
}

fn discover_backups(path: &Path, id: &str, relative: Option<&str>) -> Result<Vec<BackupInfo>> {
    let dir = backup_directory(path, relative);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let matches_iid = valid_backup_name(&name, id);
        if !entry.file_type().is_ok_and(|kind| kind.is_dir())
            || !(matches_iid || valid_backup_name(&name, "backup"))
        {
            continue;
        }
        for file in fs::read_dir(entry.path()).map_err(|source| Error::Io {
            path: entry.path(),
            source,
        })? {
            let file = file.map_err(|source| Error::Io {
                path: entry.path(),
                source,
            })?;
            let path = file.path();
            if !file.file_type().is_ok_and(|kind| kind.is_file())
                || !path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("ldtk")
                            || extension.eq_ignore_ascii_case("json")
                    })
            {
                continue;
            }
            let belongs = match read_backup_metadata(&path) {
                Ok(metadata) => metadata.iid.as_deref() == Some(id),
                Err(_) => matches_iid,
            };
            if belongs {
                let prefix = if name.starts_with("backup_") {
                    7
                } else {
                    id.len() + 1
                };
                out.push(BackupInfo {
                    path,
                    timestamp: name[prefix..prefix + 19].into(),
                    crash: name[prefix + 19..].starts_with("_crash"),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| b.path.cmp(&a.path))
    });
    Ok(out)
}

fn is_crash_backup(path: &Path, iid: &str) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            [iid, "backup"].into_iter().any(|id| {
                valid_backup_name(name, id) && name[id.len() + 20..].starts_with("_crash")
            })
        })
}

fn remove_restored_crash(
    backup: &Path,
    original: &Path,
    external_paths: &[PathBuf],
) -> std::io::Result<()> {
    let directory = backup.parent().expect("saved crash backup directory");
    let real_directory = fs::canonicalize(directory)?;
    if fs::canonicalize(original)?.starts_with(&real_directory) {
        return Err(std::io::Error::other(
            "the restored project is inside the recovery directory",
        ));
    }
    let mut files = BTreeSet::from([backup.to_path_buf()]);
    for relative in external_paths {
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
            || !relative
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ldtkl"))
        {
            return Err(std::io::Error::other(
                "an external level is outside the recovery file layout",
            ));
        }
        files.insert(directory.join(relative));
    }
    let folders = files
        .iter()
        .flat_map(|path| {
            path.ancestors()
                .skip(1)
                .take_while(|path| path.starts_with(directory))
        })
        .collect::<BTreeSet<_>>();
    for folder in &folders {
        if !fs::symlink_metadata(folder)?.is_dir() {
            return Err(std::io::Error::other(
                "a recovery folder is a symbolic link",
            ));
        }
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if !(kind.is_file() && files.contains(&path)
                || kind.is_dir() && folders.contains(path.as_path()))
            {
                return Err(std::io::Error::other(format!(
                    "unrecognized recovery file: {}",
                    path.display()
                )));
            }
        }
    }
    for file in &files {
        fs::remove_file(file)?;
    }
    for folder in folders.into_iter().rev() {
        fs::remove_dir(folder)?;
    }
    Ok(())
}

#[cfg(any(unix, windows))]
pub(super) fn local_backup_timestamp(seconds: u64) -> Option<String> {
    let seconds = libc::time_t::try_from(seconds).ok()?;
    let mut calendar = std::mem::MaybeUninit::<libc::tm>::uninit();
    // Both platform APIs initialize the entire output on success and use a caller-owned buffer.
    #[cfg(unix)]
    let success = unsafe { !libc::localtime_r(&seconds, calendar.as_mut_ptr()).is_null() };
    #[cfg(windows)]
    let success = unsafe { libc::localtime_s(calendar.as_mut_ptr(), &seconds) == 0 };
    if !success {
        return None;
    }
    // SAFETY: the successful localtime call above initialized this tm.
    let calendar = unsafe { calendar.assume_init() };
    Some(format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        calendar.tm_year + 1900,
        calendar.tm_mon + 1,
        calendar.tm_mday,
        calendar.tm_hour,
        calendar.tm_min,
        calendar.tm_sec
    ))
}
impl Project {
    pub fn external_files_directory(&self) -> Option<PathBuf> {
        self.path.as_ref().map(|path| path.with_extension(""))
    }

    pub fn backup_directory(&self) -> Option<PathBuf> {
        let path = self.path.as_ref()?;
        Some(backup_directory(path, self.json["backupRelPath"].as_str()))
    }

    pub fn is_backup(&self) -> bool {
        self.path
            .as_ref()
            .is_some_and(|path| backup_file(path, self.json["iid"].as_str().unwrap_or("")))
    }
    /// Local date and GMT offset shown when previewing a saved backup.
    #[cfg(any(unix, windows))]
    pub fn backup_date_label(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        let name = path.parent()?.file_name()?.to_str()?;
        let iid = self.json["iid"].as_str().unwrap_or("");
        let prefix = [iid, "backup"]
            .into_iter()
            .find(|id| valid_backup_name(name, id))?;
        BackupInfo::date_label(&name[prefix.len() + 1..])
    }
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let Some(path) = &self.path else {
            return Ok(vec![]);
        };
        discover_backups(
            path,
            self.json["iid"].as_str().unwrap_or(""),
            self.json["backupRelPath"].as_str(),
        )
    }
    /// Discover saved backups without loading project data, external levels or resources.
    pub fn list_backups_at(path: impl AsRef<Path>) -> Result<Vec<BackupInfo>> {
        let path = absolute(path.as_ref())?;
        let metadata = read_backup_metadata(&path)?;
        let Some(iid) = metadata.iid else {
            return Ok(Vec::new());
        };
        discover_backups(&path, &iid, metadata.backup_rel_path.as_deref())
    }
    pub fn load_backup(backup: impl AsRef<Path>, original: impl AsRef<Path>) -> Result<Self> {
        let mut project = Self::load(backup)?;
        let original = absolute(original.as_ref())?;
        match read_json(&original) {
            Ok(json) => {
                if json["iid"].as_str().is_some() && json["iid"] != project.json["iid"] {
                    return Err(Error::Invalid(
                        "the selected original project does not match this backup".into(),
                    ));
                }
            }
            Err(Error::Json { .. }) => (),
            Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        if !project.is_backup() {
            return Err(Error::Invalid(
                "selected file is not in a recognized backup directory".into(),
            ));
        }
        let base = project.path.as_ref().unwrap().parent().unwrap();
        let old = original.parent().unwrap();
        rebase_paths(&mut project.json, old, base)?;
        for level in project.external_levels.values_mut() {
            rebase_paths(level, old, base)?;
        }
        Ok(project)
    }
    pub fn restore_backup(backup: impl AsRef<Path>, original: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::restore_backup_with_exports(backup, original)?.0)
    }
    pub fn restore_backup_with_exports(
        backup: impl AsRef<Path>,
        original: impl AsRef<Path>,
    ) -> Result<(Self, SaveReport)> {
        let original = absolute(original.as_ref())?;
        let mut restored = Self::load_backup(backup, &original)?;
        let backup = restored.path.clone().expect("loaded backup path");
        let crash = is_crash_backup(&backup, restored.json["iid"].as_str().unwrap_or(""));
        let recovery_levels = if crash {
            restored.external_levels.keys().map(PathBuf::from).collect()
        } else {
            Vec::new()
        };
        // Capture the original before any replacement, independently of the backed-up settings.
        let mut safety = restored.clone();
        safety.path = Some(original.clone());
        safety.backup_with_progress(crash.then_some(backup.as_path()), &mut |_, _, _| {})?;
        restored.save_document(&original, false)?;
        let mut report = restored.configured_exports_report();
        if crash && let Err(error) = remove_restored_crash(&backup, &original, &recovery_levels) {
            report.warnings.push(format!(
                "Project restored; could not remove crash backup: {error}"
            ));
        }
        Ok((restored, report))
    }
    /// Change the extension of the saved file without publishing unsaved edits.
    pub fn rename_project_extension(
        &mut self,
        use_ldtk: bool,
        snapshot: Option<&mut FilesystemSnapshot>,
    ) -> Result<bool> {
        if self.is_backup() {
            return Err(Error::Unsupported(
                "restore a backup before renaming the project".into(),
            ));
        }
        let old = self
            .path
            .as_ref()
            .ok_or_else(|| Error::Invalid("save the project before renaming it".into()))?;
        let target = old.with_extension(if use_ldtk { "ldtk" } else { "json" });
        if target == *old {
            return Ok(false);
        }
        // Reserve the sibling path before replacing it so an existing file or
        // symlink cannot be clobbered, including on platforms where rename replaces it.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|source| Error::Io {
                path: target.clone(),
                source,
            })?;
        if let Err(source) = fs::rename(old, &target) {
            if let Err(cleanup) = fs::remove_file(&target) {
                return Err(Error::Invalid(format!(
                    "could not rename {}: {source}. Could not remove {}: {cleanup}",
                    old.display(),
                    target.display()
                )));
            }
            return Err(Error::Io {
                path: old.clone(),
                source,
            });
        }
        if let Some(snapshot) = snapshot {
            snapshot.rename_document(old, &target);
        }
        self.path = Some(target);
        Ok(true)
    }
    /// Publish the renamed project before deleting any source file.
    pub fn rename_project(&mut self, new_stem: &str) -> Result<SaveReport> {
        self.rename_project_with_snapshot(new_stem, None)
    }

    /// Reject changes to the source documents recorded when the project was opened.
    pub fn rename_project_with_snapshot(
        &mut self,
        new_stem: &str,
        snapshot: Option<&FilesystemSnapshot>,
    ) -> Result<SaveReport> {
        if self.is_backup() {
            return Err(Error::Unsupported(
                "restore a backup before renaming the project".into(),
            ));
        }
        let old = self
            .path
            .as_ref()
            .ok_or_else(|| Error::Invalid("save the project before renaming it".into()))?
            .clone();
        let stem = new_stem.trim();
        if stem.is_empty()
            || stem == "."
            || stem == ".."
            || stem
                .chars()
                .any(|c| c.is_control() || "/\\<>:\"|?*".contains(c))
        {
            return Err(Error::Invalid("invalid project file name".into()));
        }
        let extension = old.extension().and_then(|e| e.to_str()).unwrap_or("ldtk");
        let target = old.with_file_name(format!("{stem}.{extension}"));
        if target == old {
            return Err(Error::Invalid("enter a different project file name".into()));
        }
        let base = old.parent().expect("absolute path");
        let old_dir = base.join(old.file_stem().unwrap_or_default());
        let new_dir = base.join(stem);
        let old_meta = old.with_extension(format!("{extension}.meta"));
        let new_meta = target.with_extension(format!("{extension}.meta"));
        for path in [&target, &new_dir, &new_meta] {
            if path.exists() {
                return Err(Error::Invalid(format!(
                    "rename destination already exists: {}",
                    path.display()
                )));
            }
        }
        let captured;
        let snapshot = if let Some(snapshot) = snapshot {
            snapshot
        } else {
            captured = FilesystemSnapshot::for_save_destination(&old)?;
            &captured
        };
        let verify_source = || {
            let changed = snapshot.changed_files_matching(|path| snapshot.is_document(path))?;
            if changed.is_empty() {
                Ok(())
            } else {
                Err(Error::FilesChanged(changed))
            }
        };
        verify_source()?;
        let mut candidate = self.clone();
        rewrite_moved_resources(&mut candidate.json, &old_dir, &new_dir, base)?;
        for level in candidate.external_levels.values_mut() {
            rewrite_moved_resources(level, &old_dir, &new_dir, base)?;
        }
        let mut document_snapshot = FilesystemSnapshot::default();
        let publish = (|| -> Result<SaveReport> {
            if old_dir.exists() {
                copy_tree(&old_dir, &new_dir)?;
            }
            if old_meta.exists() {
                copy_tree(&old_meta, &new_meta)?;
            }
            candidate.save_document_with_progress(
                &target,
                false,
                None,
                None,
                Some(&mut document_snapshot),
                &mut |_, _, _, _| {},
            )?;
            let report = candidate.configured_exports_report();
            verify_source()?;
            Ok(report)
        })();
        let mut report = match publish {
            Ok(report) => report,
            Err(error) => {
                let _ = fs::remove_dir_all(&new_dir);
                let _ = fs::remove_file(&new_meta);
                let _ = fs::remove_file(&target);
                return Err(error);
            }
        };
        report.document_snapshot = Some(document_snapshot);
        match fs::remove_file(&old) {
            Ok(()) => {
                if old_dir.exists()
                    && let Err(error) = fs::remove_dir_all(&old_dir)
                {
                    report.export_errors.push(format!(
                        "project renamed; could not remove old directory {}: {error}",
                        old_dir.display()
                    ));
                }
                if old_meta.exists()
                    && let Err(error) = fs::remove_file(&old_meta)
                {
                    report.export_errors.push(format!(
                        "project renamed; could not remove old metadata {}: {error}",
                        old_meta.display()
                    ));
                }
            }
            Err(error) => report.export_errors.push(format!(
                "project renamed; old source retained because it could not be removed: {error}"
            )),
        }
        *self = candidate;
        Ok(report)
    }
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(from).map_err(|source| Error::Io {
        path: from.into(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        let link = fs::read_link(from).map_err(|source| Error::Io {
            path: from.into(),
            source,
        })?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, to).map_err(|source| Error::Io {
            path: to.into(),
            source,
        })?;
        #[cfg(windows)]
        {
            if from.is_dir() {
                std::os::windows::fs::symlink_dir(link, to)
            } else {
                std::os::windows::fs::symlink_file(link, to)
            }
            .map_err(|source| Error::Io {
                path: to.into(),
                source,
            })?;
        }
    } else if metadata.is_dir() {
        fs::create_dir(to).map_err(|source| Error::Io {
            path: to.into(),
            source,
        })?;
        for entry in fs::read_dir(from).map_err(|source| Error::Io {
            path: from.into(),
            source,
        })? {
            let entry = entry.map_err(|source| Error::Io {
                path: from.into(),
                source,
            })?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        fs::set_permissions(to, metadata.permissions()).map_err(|source| Error::Io {
            path: to.into(),
            source,
        })?;
    } else {
        if !metadata.is_file() {
            return Err(Error::Unsupported("resource is not a regular file".into()));
        }
        let mut input = fs::File::open(from).map_err(|source| Error::Io {
            path: from.into(),
            source,
        })?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|source| Error::Io {
                path: to.into(),
                source,
            })?;
        // Keep the writable handle while restoring attributes and flushing:
        // reopening a read-only copy cannot FlushFileBuffers on Windows.
        std::io::copy(&mut input, &mut output)
            .and_then(|_| output.set_permissions(metadata.permissions()))
            .and_then(|()| output.sync_all())
            .map_err(|source| Error::Io {
                path: to.into(),
                source,
            })?;
    }
    Ok(())
}
fn rewrite_moved_resources(value: &mut Value, old: &Path, new: &Path, base: &Path) -> Result<()> {
    fn map(value: &mut Value, old: &Path, new: &Path, base: &Path) -> Result<()> {
        match value {
            Value::String(raw) => {
                let resolved = absolute(&base.join(raw.as_str()))?;
                if let Ok(tail) = resolved.strip_prefix(old) {
                    let path = new.join(tail);
                    *raw = if asset_is_absolute(raw) {
                        path.to_string_lossy().into_owned()
                    } else {
                        relative_to(&path, base)?
                    };
                }
            }
            Value::Array(a) => {
                for v in a {
                    map(v, old, new, base)?;
                }
            }
            Value::Object(o) => {
                if let Some(v) = o.get_mut("params") {
                    map(v, old, new, base)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    match value {
        Value::Object(o) => {
            let file = o
                .get("__type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.contains("FilePath"))
                || o.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t == "F_Path" || t == "F_File");
            let level = o.contains_key("layerInstances");
            for (k, v) in o {
                if matches!(
                    k.as_str(),
                    "relPath" | "bgRelPath" | "__tilesetRelPath" | "externalRelPath"
                ) && !(level && k == "externalRelPath")
                    || file
                        && [
                            "__value",
                            "realEditorValues",
                            "realEditorValue",
                            "defaultOverride",
                        ]
                        .contains(&k.as_str())
                {
                    map(v, old, new, base)?;
                } else {
                    rewrite_moved_resources(v, old, new, base)?;
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                rewrite_moved_resources(v, old, new, base)?;
            }
        }
        _ => (),
    }
    Ok(())
}
