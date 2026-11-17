use ldtk_core::{LevelKey, Project};
use serde_json::json;
use std::fs;
#[test]
fn save_as_copies_external_levels_and_rebases_assets_and_file_defaults() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("A.ldtk");
    let b = t.path().join("other/B.ldtk");
    fs::create_dir(t.path().join("other")).unwrap();
    let mut p = Project::new();
    p.json["defs"]["tilesets"] = json!([{"uid":3,"relPath":"art.png"}]);
    p.json["defs"]["levelFields"] =
        json!([{"type":"F_Path","defaultOverride":{"id":"V_String","params":["config.json"]}}]);
    let mut l = p.json["levels"][0].clone();
    l["externalRelPath"] = json!("levels/Room.ldtkl");
    l["bgRelPath"] = json!("bg.png");
    l["fieldInstances"] = json!([{"__type":"FilePath","__value":"notes.txt","realEditorValues":[{"id":"V_String","params":["notes.txt"]}]}]);
    p.json["levels"][0] = l.clone();
    p.json["levels"][0]["layerInstances"] = serde_json::Value::Null;
    p.external_levels.insert("levels/Room.ldtkl".into(), l);
    fs::create_dir(t.path().join("levels")).unwrap();
    p.save(&a).unwrap();
    let original = fs::read(t.path().join("levels/Room.ldtkl")).unwrap();
    p.level_mut(LevelKey {
        world: None,
        level: 0,
    })
    .unwrap()["pxWid"] = json!(512);
    p.save(&b).unwrap();
    assert_eq!(p.json["defs"]["tilesets"][0]["relPath"], "../art.png");
    assert_eq!(
        p.json["defs"]["levelFields"][0]["defaultOverride"]["params"][0],
        "../config.json"
    );
    let reopened = Project::load(&b).unwrap();
    let l = reopened
        .level(LevelKey {
            world: None,
            level: 0,
        })
        .unwrap();
    assert_eq!(l["pxWid"], 512);
    assert_eq!(l["bgRelPath"], "../bg.png");
    assert_eq!(
        l["fieldInstances"][0]["realEditorValues"][0]["params"][0],
        "../notes.txt"
    );
    assert_eq!(
        fs::read(t.path().join("levels/Room.ldtkl")).unwrap(),
        original
    );
    assert!(
        reopened.json["levels"][0]["externalRelPath"]
            .as_str()
            .unwrap()
            .starts_with("B/")
    );
}
#[test]
fn backup_includes_saved_external_documents_and_limit_and_watcher_detects_change() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    p.json["backupLimit"] = json!(1);
    let backup = p.backup().unwrap().unwrap();
    assert!(backup.join("game.ldtk").exists());
    assert!(
        backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(p.json["iid"].as_str().unwrap())
    );
    let snapshot = p.filesystem_snapshot().unwrap();
    assert!(snapshot.changed_files().unwrap().is_empty());
    fs::write(&path, "external change").unwrap();
    assert_eq!(snapshot.changed_files().unwrap(), vec![path]);
}
#[test]
fn enabled_external_levels_are_created_from_inline_documents() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&path).unwrap();
    assert!(p.json["levels"][0]["layerInstances"].is_null());
    assert_eq!(p.json["levels"][0]["externalRelPath"], "game/Level_0.ldtkl");
    assert!(t.path().join("game/Level_0.ldtkl").exists());
    let reloaded = Project::load(path).unwrap();
    assert!(
        reloaded
            .level(LevelKey {
                world: None,
                level: 0
            })
            .unwrap()["layerInstances"]
            .is_array()
    );
}
#[test]
fn explicit_legacy_migration_converts_aliases_cells_tiles_fields_and_is_idempotent() {
    let mut p=Project::from_json(json!({"jsonVersion":"0.3.0","advancedOptionFlags":[],"exportPng":true,"defs":{"layers":[{"uid":1,"identifier":"Wall","type":"IntGrid","gridSize":16,"intGridValues":[{"color":"#ff0000"}],"autoTilesetDefUid":7}],"tilesets":[{"uid":7,"path":"atlas.png","pxWid":32,"pxHei":16,"tileGridSize":16}],"entities":[{"uid":2,"name":"Player","width":16,"height":16,"fieldDefs":[{"uid":3,"name":"path","type":"F_File"}]}]},"levels":[{"uid":0,"identifier":"Room","pxWid":32,"pxHei":16,"layerInstances":[{"layerDefId":1,"intGrid":[{"coordId":1,"v":0}],"gridTiles":[{"coordId":0,"tileId":1,"f":0}],"entityInstances":[{"defId":2,"x":8,"y":16,"fieldInstances":[{"defId":3,"realEditorValue":{"id":"V_String","params":["asset.txt"]}}]}]}]}]})).unwrap();
    assert!(p.migrate_to_current().unwrap());
    assert_eq!(p.json["jsonVersion"], "1.5.3");
    assert_eq!(p.json["imageExportMode"], "OneImagePerLayer");
    assert_eq!(p.json["defs"]["tilesets"][0]["relPath"], "atlas.png");
    assert_eq!(
        p.json["defs"]["entities"][0]["fieldDefs"][0]["type"],
        "F_Path"
    );
    let li = p
        .layer(
            LevelKey {
                world: None,
                level: 0,
            },
            0,
        )
        .unwrap();
    assert_eq!(li["intGridCsv"], json!([0, 1]));
    assert_eq!(li["gridTiles"][0]["t"], 1);
    assert_eq!(li["gridTiles"][0]["px"], json!([0, 0]));
    assert_eq!(li["entityInstances"][0]["px"], json!([8, 16]));
    assert_eq!(
        li["entityInstances"][0]["fieldInstances"][0]["realEditorValues"][0]["params"][0],
        "asset.txt"
    );
    let before = p.json.clone();
    assert!(!p.migrate_to_current().unwrap());
    assert_eq!(p.json, before);
}
#[test]
fn backup_retention_does_not_remove_unrecognized_or_other_project_folders() {
    let t = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.save(t.path().join("map.ldtk")).unwrap();
    p.json["backupLimit"] = json!(1);
    let dir = t.path().join("map/backups");
    fs::create_dir_all(&dir).unwrap();
    let protected = dir.join(format!("{}_personal", p.json["iid"].as_str().unwrap()));
    fs::create_dir(&protected).unwrap();
    fs::write(protected.join("keep"), "valuable").unwrap();
    let old = dir.join(format!(
        "{}_2000-01-01_00-00-00",
        p.json["iid"].as_str().unwrap()
    ));
    fs::create_dir(&old).unwrap();
    fs::write(old.join("map.ldtk"), "old").unwrap();
    let newest = p.backup().unwrap().unwrap();
    assert!(newest.join("map.ldtk").exists());
    assert!(protected.join("keep").exists());
    assert!(!old.exists());
}
#[test]
#[ignore = "requires pinned upstream checkout under .reference/ldtk"]
fn every_original_project_can_migrate_twice_without_further_changes() {
    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, files);
            } else if p.extension().is_some_and(|e| e == "ldtk") {
                files.push(p);
            }
        }
    }
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference/ldtk/tests");
    let mut files = Vec::new();
    walk(&base, &mut files);
    assert!(files.len() >= 49);
    for file in files {
        let mut p = Project::load(&file).unwrap();
        p.migrate_to_current()
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let json = p.json.clone();
        let external = p.external_levels.clone();
        assert!(
            !p.migrate_to_current().unwrap(),
            "{} migration is not idempotent",
            file.display()
        );
        assert_eq!(p.json, json);
        assert_eq!(p.external_levels, external);
    }
}
#[test]
fn backup_reads_the_saved_external_layout_before_unsaved_storage_changes() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&path).unwrap();
    p.set_external_levels(false).unwrap();
    let backup = p.backup().unwrap().unwrap();
    let saved = Project::load(backup.join("game.ldtk")).unwrap();
    assert!(
        saved
            .level(LevelKey {
                world: None,
                level: 0
            })
            .unwrap()["layerInstances"]
            .is_array()
    );
    assert!(backup.join("game/Level_0.ldtkl").exists());
}
#[test]
fn save_as_backs_up_an_existing_destination_before_overwrite() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("A.ldtk");
    let b = t.path().join("B.ldtk");
    let mut p = Project::new();
    p.save(&a).unwrap();
    let mut prior = Project::new();
    prior.json["levels"][0]["identifier"] = json!("Prior");
    prior.save(&b).unwrap();
    p.json["backupOnSave"] = json!(true);
    p.save(&b).unwrap();
    let backups = t.path().join("B/backups");
    assert!(backups.is_dir());
    let folder = fs::read_dir(backups)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let backup = Project::load(folder.join("B.ldtk")).unwrap();
    assert_eq!(backup.json["levels"][0]["identifier"], "Prior");
}
#[test]
fn watcher_conflict_guard_refuses_to_overwrite_outside_edits() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("map.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    let snapshot = p.filesystem_snapshot().unwrap();
    fs::write(&path, b"outside edit").unwrap();
    p.json["levels"][0]["identifier"] = json!("EditorChange");
    assert!(p.save_if_unchanged(&path, &snapshot).is_err());
    assert_eq!(fs::read(path).unwrap(), b"outside edit");
}
#[test]
fn disabling_external_storage_removes_only_retired_owned_level_documents() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&path).unwrap();
    let ext = t.path().join("game/Level_0.ldtkl");
    assert!(ext.exists());
    let unrelated = t.path().join("game/unrelated.ldtkl");
    fs::write(&unrelated, "unrelated").unwrap();
    p.set_external_levels(false).unwrap();
    p.save(&path).unwrap();
    assert!(!ext.exists());
    assert!(unrelated.exists());
    assert!(Project::load(path).unwrap().json["levels"][0]["layerInstances"].is_array());
}
#[test]
fn backup_browser_load_and_restore_use_original_assets_and_preserve_external_levels() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    fs::write(t.path().join("art.png"), b"atlas").unwrap();
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.json["defs"]["tilesets"] = json!([{"uid":7,"relPath":"art.png"}]);
    p.save(&path).unwrap();
    let backup = p.backup().unwrap().unwrap().join("game.ldtk");
    p.level_mut(LevelKey {
        world: None,
        level: 0,
    })
    .unwrap()["identifier"] = json!("After");
    p.save(&path).unwrap();
    let listed = p.list_backups().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].path, backup);
    let mut view = Project::load_backup(&backup, &path).unwrap();
    let asset = view.path.as_ref().unwrap().parent().unwrap().join(
        view.json["defs"]["tilesets"][0]["relPath"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(
        asset.canonicalize().unwrap(),
        t.path().join("art.png").canonicalize().unwrap()
    );
    let before = fs::read(&backup).unwrap();
    assert!(view.save(&backup).is_err());
    assert_eq!(fs::read(&backup).unwrap(), before);
    let restored = Project::restore_backup(&backup, &path).unwrap();
    assert_eq!(
        restored
            .level(LevelKey {
                world: None,
                level: 0
            })
            .unwrap()["identifier"],
        "Level_0"
    );
    assert_eq!(restored.json["defs"]["tilesets"][0]["relPath"], "art.png");
    assert_eq!(Project::load(path).unwrap().external_levels.len(), 1);
}
#[test]
fn rename_moves_external_tree_and_meta_after_success_and_rejects_collisions() {
    let t = tempfile::tempdir().unwrap();
    let old = t.path().join("A.ldtk");
    let new = t.path().join("B.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&old).unwrap();
    fs::write(t.path().join("A.ldtk.meta"), b"meta").unwrap();
    fs::write(t.path().join("A/keep.txt"), b"generated").unwrap();
    p.rename_project("B").unwrap();
    assert!(!old.exists());
    assert!(!t.path().join("A").exists());
    assert_eq!(fs::read(t.path().join("B.ldtk.meta")).unwrap(), b"meta");
    assert_eq!(fs::read(t.path().join("B/keep.txt")).unwrap(), b"generated");
    assert_eq!(p.path, Some(new.clone()));
    assert!(Project::load(new).is_ok());
    fs::write(t.path().join("Collision.ldtk"), b"keep").unwrap();
    assert!(p.rename_project("Collision").is_err());
    assert_eq!(p.path, Some(t.path().join("B.ldtk")));
    assert_eq!(fs::read(t.path().join("Collision.ldtk")).unwrap(), b"keep");
}
#[test]
fn rename_copies_read_only_resources_and_preserves_their_permissions() {
    let t = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(t.path().join("A.ldtk")).unwrap();
    let source = t.path().join("A/read-only.txt");
    let target = t.path().join("B/read-only.txt");
    fs::write(&source, b"read-only resource").unwrap();
    let original = fs::metadata(&source).unwrap().permissions();
    let mut permissions = original.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&source, permissions).unwrap();
    let renamed = p.rename_project("B");
    let bytes = fs::read(&target);
    let copied_permissions = fs::metadata(&target).map(|m| m.permissions());
    // Windows can retain an old read-only resource with a reported cleanup error.
    // Restore only this test's files so its temporary tree can always be removed.
    for path in [&source, &target] {
        if path.exists() {
            fs::set_permissions(path, original.clone()).unwrap();
        }
    }
    renamed.unwrap();
    assert_eq!(bytes.unwrap(), b"read-only resource");
    assert!(copied_permissions.unwrap().readonly());
    assert!(Project::load(t.path().join("B.ldtk")).is_ok());
}
#[test]
fn rename_failure_preserves_every_source_file_and_resource_relocation_updates_paths() {
    let t = tempfile::tempdir().unwrap();
    let old = t.path().join("A.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&old).unwrap();
    fs::write(t.path().join("A/asset.txt"), b"asset").unwrap();
    p.json["defs"]["levelFields"] = json!([{"uid":7,"type":"F_Path","defaultOverride":{"id":"V_String","params":["A/asset.txt"]}}]);
    let original = fs::read(&old).unwrap();
    let key = p.external_levels.keys().next().unwrap().clone();
    let external = p.external_levels.remove(&key).unwrap();
    assert!(p.rename_project("Fail").is_err());
    assert_eq!(fs::read(&old).unwrap(), original);
    assert!(t.path().join("A/asset.txt").exists());
    assert!(!t.path().join("Fail").exists());
    assert!(!t.path().join("Fail.ldtk").exists());
    p.external_levels.insert(key, external);
    p.rename_project("B").unwrap();
    assert_eq!(
        p.json["defs"]["levelFields"][0]["defaultOverride"]["params"][0],
        "B/asset.txt"
    );
    assert_eq!(fs::read(t.path().join("B/asset.txt")).unwrap(), b"asset");
}
#[test]
fn backups_in_one_second_do_not_overwrite_each_other() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("map.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    let first = p.backup().unwrap().unwrap();
    p.json["levels"][0]["identifier"] = json!("Later");
    p.save(&path).unwrap();
    let second = p.backup().unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(
        Project::load(first.join("map.ldtk")).unwrap().json["levels"][0]["identifier"],
        "Level_0"
    );
    assert_eq!(
        Project::load(second.join("map.ldtk")).unwrap().json["levels"][0]["identifier"],
        "Later"
    );
    assert_eq!(p.list_backups().unwrap().len(), 2);
}
#[test]
fn restoring_backup_can_recover_a_corrupt_original_and_preserves_its_raw_bytes() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("map.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    let backup = p.backup().unwrap().unwrap().join("map.ldtk");
    fs::write(&path, b"{corrupt").unwrap();
    let restored = Project::restore_backup(&backup, &path).unwrap();
    assert_eq!(restored.json["levels"][0]["identifier"], "Level_0");
    let backups = restored.list_backups().unwrap();
    assert!(
        backups
            .iter()
            .any(|backup| fs::read(&backup.path).unwrap() == b"{corrupt")
    );
}
#[test]
fn external_level_rename_updates_owned_document_name_on_save() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("map.ldtk");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.save(&path).unwrap();
    p.level_mut(LevelKey {
        world: None,
        level: 0,
    })
    .unwrap()["identifier"] = json!("Renamed");
    p.save(&path).unwrap();
    assert_eq!(p.json["levels"][0]["externalRelPath"], "map/Renamed.ldtkl");
    assert!(t.path().join("map/Renamed.ldtkl").exists());
    assert!(!t.path().join("map/Level_0.ldtkl").exists());
}
#[test]
fn backup_restore_can_recreate_a_missing_original_project() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("missing.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    let backup = p.backup().unwrap().unwrap().join("missing.ldtk");
    fs::remove_file(&path).unwrap();
    let restored = Project::restore_backup(&backup, &path).unwrap();
    assert!(path.is_file());
    assert_eq!(restored.json["iid"], p.json["iid"]);
    assert_eq!(restored.path.as_deref(), Some(path.as_path()));
}
