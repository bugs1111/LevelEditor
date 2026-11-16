use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
use std::fs;

#[test]
fn crash_captures_current_memory_and_external_levels_without_touching_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("game.ldtk");
    let mut project = Project::new();
    project.json["externalLevels"] = json!(true);
    project.json["defs"]["tilesets"] = json!([{"uid":7,"relPath":"art.png"}]);
    project.save(&source).unwrap();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    let external = project.json["levels"][0]["externalRelPath"]
        .as_str()
        .unwrap()
        .to_owned();
    let root_before = fs::read(&source).unwrap();
    let external_before = fs::read(dir.path().join(&external)).unwrap();
    project.json["customMemory"] = json!("unsaved root change");
    project.json["backupRelPath"] = json!("recovery");
    project.json["backupLimit"] = json!(0);
    project.json["exportTiled"] = json!(true);
    project.json["simplifiedExport"] = json!(true);
    let level = project.level_mut(key).unwrap();
    level["pxWid"] = json!(777);
    level["bgRelPath"] = json!("background.png");
    level["customMemory"] = json!("unsaved external change");
    let memory = project.json.clone();
    let external_memory = project.external_levels.clone();
    let backup = project.write_crash_recovery(dir.path()).unwrap();
    let again = project.write_crash_recovery(dir.path()).unwrap();
    assert_ne!(backup, again);
    assert!(backup.starts_with(dir.path().join("recovery")));
    assert!(
        backup
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("_crash")
    );
    assert_eq!(fs::read(&source).unwrap(), root_before);
    assert_eq!(
        fs::read(dir.path().join(&external)).unwrap(),
        external_before
    );
    assert_eq!(project.json, memory);
    assert_eq!(project.external_levels, external_memory);
    assert_eq!(project.path.as_ref(), Some(&source));
    let backups = project.list_backups().unwrap();
    assert_eq!(backups.len(), 2);
    assert!(backups.iter().all(|item| item.crash));
    let loaded = Project::load_backup(&backup, &source).unwrap();
    assert!(loaded.is_backup());
    assert_eq!(loaded.json["customMemory"], "unsaved root change");
    assert_eq!(loaded.json["levels"][0]["pxWid"], 777);
    assert_eq!(
        loaded.level(key).unwrap()["customMemory"],
        "unsaved external change"
    );
    assert_eq!(
        loaded
            .make_absolute_file_path(
                loaded.json["defs"]["tilesets"][0]["relPath"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
        dir.path().join("art.png")
    );
    assert_eq!(
        loaded
            .make_absolute_file_path(loaded.level(key).unwrap()["bgRelPath"].as_str().unwrap())
            .unwrap(),
        dir.path().join("background.png")
    );
    assert!(!backup.with_extension("tmx").exists());
    assert_eq!(fs::read_dir(backup.parent().unwrap()).unwrap().count(), 2);
}

#[test]
fn unsaved_project_uses_fallback_and_recovers_relative_resources() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.json["levels"][0]["identifier"] = json!("NeverSaved");
    project.json["defs"]["tilesets"] = json!([{"uid":2,"relPath":"unsaved-art.png"}]);
    let expected_asset = project.make_absolute_file_path("unsaved-art.png").unwrap();
    let recovery = project.write_crash_recovery(dir.path()).unwrap();
    assert!(recovery.starts_with(dir.path().join("Untitled/backups")));
    assert_eq!(recovery.file_name().unwrap(), "Untitled.ldtk");
    assert!(project.path.is_none());
    let original = dir.path().join("Untitled.ldtk");
    assert!(!original.exists());
    let loaded = Project::load_backup(&recovery, &original).unwrap();
    assert!(loaded.is_backup());
    assert_eq!(loaded.json["levels"][0]["identifier"], "NeverSaved");
    assert_eq!(
        loaded
            .make_absolute_file_path(
                loaded.json["defs"]["tilesets"][0]["relPath"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
        expected_asset
    );
    let mut locator = project.clone();
    locator.path = Some(original);
    assert_eq!(locator.list_backups().unwrap()[0].path, recovery);
}

#[test]
fn parent_external_paths_are_contained_and_missing_loaded_levels_fail_without_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.path = Some(dir.path().join("game.ldtk"));
    let mut external = project.json["levels"][0].clone();
    external["customMemory"] = json!("outside source");
    project.json["levels"][0]["externalRelPath"] = json!("../outside.ldtkl");
    project.json["levels"][0]["layerInstances"] = Value::Null;
    assert!(project.write_crash_recovery(dir.path()).is_err());
    assert!(project.list_backups().unwrap().is_empty());
    project
        .external_levels
        .insert("../outside.ldtkl".into(), external);
    let backup = project.write_crash_recovery(dir.path()).unwrap();
    let loaded = Project::load(&backup).unwrap();
    let rel = loaded.json["levels"][0]["externalRelPath"]
        .as_str()
        .unwrap();
    assert!(!rel.contains(".."));
    assert!(backup.parent().unwrap().join(rel).is_file());
    assert_eq!(
        loaded
            .level(LevelKey {
                world: None,
                level: 0
            })
            .unwrap()["customMemory"],
        "outside source"
    );
}
