use ldtk_core::{History, LevelKey, Project};
use serde_json::json;
use std::fs;

#[test]
fn world_level_iteration_keeps_original_keys_when_an_external_level_is_unavailable() {
    let mut project = Project::new();
    project.json["worlds"] = json!([{
        "levels": [
            {"externalRelPath":"missing.ldtkl"},
            {"externalRelPath":"available.ldtkl"},
            {"identifier":"Inline"}
        ]
    }]);
    project
        .external_levels
        .insert("available.ldtkl".into(), json!({"identifier":"External"}));

    let levels: Vec<_> = project.levels_in_world(Some(0)).collect();
    assert_eq!(levels.len(), 2);
    for ((key, level), (index, name)) in levels.into_iter().zip([(1, "External"), (2, "Inline")]) {
        assert_eq!(
            key,
            LevelKey {
                world: Some(0),
                level: index
            }
        );
        assert_eq!(level["identifier"], name);
        assert!(std::ptr::eq(level, project.level(key).unwrap()));
    }
    assert_eq!(project.level_keys_in_world(Some(0)).len(), 3);
    assert_eq!(project.levels_in_world(Some(1)).count(), 0);
}

#[test]
fn preview_154_save_filters_and_orders_cached_auto_tiles_without_changing_draw_opacity() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("CachedRules.ldtk");
    let mut project = Project::new();
    let layer = project.add_layer("Walls", "IntGrid", 16).unwrap();
    let key = project.level_keys()[0];
    project.json["defs"]["layers"][0]["autoRuleGroups"] = json!([
        {"uid": 10, "rules": [
            {"uid": 11, "alpha": 0.5}, {"uid": 12}, {"uid": 13, "active": false}
        ]},
        {"uid": 20, "isOptional": true, "rules": [{"uid": 21}]},
        {"uid": 30, "isOptional": true, "rules": [{"uid": 31}]},
    ]);
    let tile = |rule, cell, id| json!({"d":[rule,cell],"t":id,"a":1,"future":"keep"});
    let cached = [
        tile(11, 2, 0),
        tile(12, 3, 1),
        tile(13, 4, 2),
        tile(21, 6, 3),
        tile(31, 8, 4),
        tile(500, 10, 5),
        tile(12, 2, 6),
        tile(12, 2, 7),
    ];
    let instance = project.layer_mut(key, 0).unwrap();
    assert_eq!(instance["layerDefUid"], layer);
    instance["autoLayerTiles"] = json!(cached);
    instance["optionalRules"] = json!([30]);
    project.set_external_levels(true).unwrap();

    project.save(&path).unwrap();
    let mut expected = json!([cached[4], cached[6], cached[7], cached[1], cached[0]]);
    assert_eq!(project.layer(key, 0).unwrap()["autoLayerTiles"], expected);
    expected[4]["a"] = json!(0.5);
    let mut reopened = Project::load(&path).unwrap();
    assert_eq!(reopened.layer(key, 0).unwrap()["autoLayerTiles"], expected);
    let relative = reopened.json["levels"][0]["externalRelPath"]
        .as_str()
        .unwrap();
    let external = temp.path().join(relative);
    let mut original = fs::read(&external).unwrap();
    original.extend_from_slice(b" \n");
    fs::write(&external, &original).unwrap();
    reopened = Project::load(&path).unwrap();
    reopened.save(&path).unwrap();
    assert_eq!(fs::read(external).unwrap(), original);
}

#[test]
fn preview_154_save_tidies_rules_and_uses_the_linked_intgrid_values() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Rules.ldtk");
    let mut project = Project::new();
    project.json["jsonVersion"] = json!("1.5.4");
    project.json["defs"]["layers"] = json!([
        {
            "uid": 10, "type": "IntGrid",
            "intGridValues": [{"value": 1}],
            "autoRuleGroups": [{"rules": [
                {"uid": 11, "size": 3, "pattern": [0,0,0,0,7000,0,0,0,0],
                 "flipX": true, "flipY": true, "checker": "Horizontal",
                 "xModulo": 1, "yModulo": 1, "outOfBoundsValue": 1, "future": {"value": 42}},
                {"uid": 12, "size": 3, "pattern": [0,0,0,1,0,0,0,0,0],
                 "flipX": true, "flipY": true, "checker": "Horizontal",
                 "xModulo": 1, "yModulo": 2, "outOfBoundsValue": 0},
                {"uid": 13, "size": 5, "pattern": [
                  0,0,0,0,0, 0,0,1,0,0, 0,0,1,0,0, 0,0,0,0,0, 0,0,0,0,0
                 ], "flipX": true, "flipY": true, "checker": "Vertical",
                 "xModulo": 3, "yModulo": 1, "outOfBoundsValue": 99}
            ]}]
        },
        {
            "uid": 20, "type": "AutoLayer", "autoSourceLayerDefUid": 10,
            "intGridValues": [{"value": 99}],
            "autoRuleGroups": [{"rules": [
                {"uid": 21, "size": 1, "pattern": [-1], "outOfBoundsValue": 1},
                {"uid": 22, "active": false, "size": 1, "pattern": [-1],
                 "checker": "Vertical", "xModulo": 2, "yModulo": 2,
                 "outOfBoundsValue": 99}
            ]}]
        }
    ]);

    let before = project.json.clone();
    let blocked = temp.path().join("Occupied.ldtk");
    fs::create_dir(&blocked).unwrap();
    assert!(project.save(&blocked).is_err());
    assert_eq!(project.json, before);

    project.save_with_exports(&path).unwrap();
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.json["defs"], project.json["defs"]);
    let rules = &reloaded.layer_def(10).unwrap()["autoRuleGroups"][0]["rules"];
    assert_eq!(rules[0]["size"], 1);
    assert_eq!(rules[0]["pattern"], json!([7000]));
    assert_eq!(rules[0]["flipX"], false);
    assert_eq!(rules[0]["flipY"], false);
    assert_eq!(rules[0]["checker"], "None");
    assert_eq!(rules[0]["outOfBoundsValue"], 1);
    assert_eq!(rules[0]["future"], json!({"value":42}));
    assert_eq!(rules[1]["size"], 3);
    assert_eq!(rules[1]["flipX"], true);
    assert_eq!(rules[1]["flipY"], false);
    assert_eq!(rules[1]["checker"], "Vertical");
    assert_eq!(rules[1]["outOfBoundsValue"], 0);
    assert_eq!(rules[2]["size"], 3);
    assert_eq!(rules[2]["pattern"], json!([0, 1, 0, 0, 1, 0, 0, 0, 0]));
    assert_eq!(rules[2]["flipX"], false);
    assert_eq!(rules[2]["flipY"], true);
    assert_eq!(rules[2]["checker"], "Horizontal");
    assert!(rules[2]["outOfBoundsValue"].is_null());
    let linked = &reloaded.layer_def(20).unwrap()["autoRuleGroups"][0]["rules"];
    assert_eq!(linked[0]["outOfBoundsValue"], 1);
    assert_eq!(linked[1]["checker"], "Vertical");
    assert_eq!(linked[1]["outOfBoundsValue"], serde_json::Value::Null);

    let saved = fs::read(&path).unwrap();
    project.save(&path).unwrap();
    assert_eq!(fs::read(path).unwrap(), saved);
}

#[test]
fn preview_154_save_omits_inactive_nine_slice_borders_without_discarding_them_in_memory() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Borders.ldtk");
    let mut project = Project::new();
    let actor = project.add_entity_def("Actor", 32, 32).unwrap();
    let borders = json!([3, 4, 5, 6]);
    project
        .update_entity_definition(
            actor,
            &json!({"tileRenderMode":"NineSlice","nineSliceBorders":borders}),
        )
        .unwrap();
    project.save(&path).unwrap();
    let reopen = || Project::load(&path).unwrap();
    assert_eq!(
        reopen().json["defs"]["entities"][0]["nineSliceBorders"],
        borders
    );

    project
        .update_entity_definition(actor, &json!({"tileRenderMode":"Stretch"}))
        .unwrap();
    project.save(&path).unwrap();
    let reopened = reopen();
    let saved = &reopened.json["defs"]["entities"][0];
    assert_eq!(saved["tileRenderMode"], "Stretch");
    assert_eq!(saved["nineSliceBorders"], json!([]));
    assert_eq!(
        project.json["defs"]["entities"][0]["nineSliceBorders"],
        borders
    );

    project
        .update_entity_definition(actor, &json!({"tileRenderMode":"NineSlice"}))
        .unwrap();
    project.save(&path).unwrap();
    assert_eq!(
        reopen().json["defs"]["entities"][0]["nineSliceBorders"],
        borders
    );
}

#[test]
fn preview_154_save_writes_tileset_tags_in_enum_value_order() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("Tags.ldtk");
    let mut project = Project::new();
    project.json["jsonVersion"] = json!("1.5.4");
    let terrain = project
        .add_enum_definition("Terrain", &["Stone", "Water", "Grass"])
        .unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    let stored = json!([
        {"enumValueId":"Grass","tileIds":[1],"extension":{"preserve":true}},
        {"enumValueId":"Stone","tileIds":[0]}
    ]);
    project
        .update_tileset_definition(
            atlas,
            &json!({"tagsSourceEnumUid":terrain,"enumTags":stored}),
        )
        .unwrap();

    project.save(&path).unwrap();
    let expected = json!([
        {"enumValueId":"Stone","tileIds":[0]},
        {"enumValueId":"Water","tileIds":[]},
        {"enumValueId":"Grass","tileIds":[1],"extension":{"preserve":true}}
    ]);
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.tileset(atlas).unwrap()["enumTags"], expected);
    assert_eq!(project.tileset(atlas).unwrap()["enumTags"], stored);
}

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
fn preview_154_same_path_save_keeps_byte_identical_external_levels() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.ldtk");
    let mut project = Project::new();
    project.add_level(None, "Edited", 32, 32).unwrap();
    let levels = project.level_keys();
    project.level_mut(levels[1]).unwrap()["extension"] = json!(11);
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();
    let targets = [0, 1].map(|i| {
        directory.path().join(
            project.json["levels"][i]["externalRelPath"]
                .as_str()
                .unwrap(),
        )
    });
    let old_bytes = targets.each_ref().map(|target| fs::read(target).unwrap());
    let paths = [&path, &targets[0], &targets[1]];
    let mut old_times = Vec::new();
    let old_time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
    for target in paths {
        fs::OpenOptions::new()
            .write(true)
            .open(target)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old_time))
            .unwrap();
        old_times.push(fs::metadata(target).unwrap().modified().unwrap());
    }

    project.level_mut(levels[1]).unwrap()["extension"] = json!(22);
    project.save(&path).unwrap();
    let new_times = paths.map(|target| fs::metadata(target).unwrap().modified().unwrap());
    assert_ne!(new_times[0], old_times[0]);
    assert_eq!(new_times[1], old_times[1]);
    assert_ne!(new_times[2], old_times[2]);
    assert_eq!(fs::read(&targets[0]).unwrap(), old_bytes[0]);
    assert_eq!(fs::read(&targets[1]).unwrap().len(), old_bytes[1].len());
    let reopened = Project::load(&path).unwrap();
    assert_eq!(reopened.level(levels[1]).unwrap()["extension"], 22);
}

#[test]
fn preview_154_external_project_clones_share_only_unmodified_level_documents() {
    let directory = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let edited = project.add_level(None, "Edited", 32, 32).unwrap();
    project.set_external_levels(true).unwrap();
    project.save(directory.path().join("map.ldtk")).unwrap();
    let untouched = project.level_keys()[0];

    let mut clone = project.clone();
    assert!(
        std::ptr::eq(project.level(edited).unwrap(), clone.level(edited).unwrap()),
        "copying a project must share its unedited external levels"
    );
    clone.level_mut(edited).unwrap()["extension"] = json!("edited");
    assert!(project.level(edited).unwrap().get("extension").is_none());
    assert_eq!(clone.level(edited).unwrap()["extension"], "edited");
    assert!(std::ptr::eq(
        project.level(untouched).unwrap(),
        clone.level(untouched).unwrap()
    ));
}

#[test]
fn preview_154_same_path_save_reuses_verified_external_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.ldtk");
    let mut project = Project::new();
    project.json["backupOnSave"] = json!(false);
    project.json["jsonStyle"] = json!("Minified");
    let edited = project.add_level(None, "Edited", 32, 32).unwrap();
    project
        .add_field_definition(None, "Notes", "Multilines", false)
        .unwrap();
    project.level_mut(edited).unwrap()["extension"] = json!("old value");
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();

    let targets = [0, edited.level].map(|index| {
        directory.path().join(
            project.json["levels"][index]["externalRelPath"]
                .as_str()
                .unwrap(),
        )
    });
    let original = targets.each_ref().map(|target| {
        let mut bytes = fs::read(target).unwrap();
        bytes.extend_from_slice(b" \t\n");
        fs::write(target, &bytes).unwrap();
        bytes
    });
    let mut project = Project::load(&path).unwrap();
    let prior_modified = fs::metadata(&targets[1]).unwrap().modified().unwrap();
    let changed = String::from_utf8(original[1].clone())
        .unwrap()
        .replace("old value", "new value");
    assert_eq!(changed.len(), original[1].len());
    fs::write(&targets[1], changed).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&targets[1])
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(prior_modified))
        .unwrap();

    project.save(&path).unwrap();
    assert_eq!(fs::read(&targets[0]).unwrap(), original[0]);
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(reloaded.level(edited).unwrap()["extension"], "old value");

    project.json["jsonStyle"] = json!("Full");
    project.save(&path).unwrap();
    let rewritten = fs::read(&targets[0]).unwrap();
    assert_ne!(rewritten, original[0]);
    assert!(String::from_utf8(rewritten).unwrap().contains("\n\t"));
}

#[test]
fn preview_154_editing_one_level_field_preserves_other_external_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("map.ldtk");
    let mut project = Project::new();
    project.json["backupOnSave"] = json!(false);
    let edited = project.level_keys()[0];
    let untouched = project.add_level(None, "Other", 128, 128).unwrap();
    let flag = project
        .add_field_definition(None, "Visited", "Bool", false)
        .unwrap();
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();

    let rel = project.json["levels"][untouched.level]["externalRelPath"]
        .as_str()
        .unwrap();
    let target = directory.path().join(rel);
    let mut original = fs::read(&target).unwrap();
    original.extend_from_slice(b" \t\n");
    fs::write(&target, &original).unwrap();
    let modified = fs::metadata(&target).unwrap().modified().unwrap();
    let mut project = Project::load(&path).unwrap();
    project.migrate_to_current().unwrap();

    project
        .set_field_editor_value(edited, None, flag, json!(true))
        .unwrap();
    project.save(&path).unwrap();

    assert!(fs::read(&target).unwrap() == original);
    assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), modified);
    let reopened = Project::load(path).unwrap();
    assert_eq!(
        reopened.level(edited).unwrap()["fieldInstances"][0]["__value"],
        true
    );
    assert_eq!(
        reopened.level(untouched).unwrap()["fieldInstances"][0]["__value"],
        false
    );
}

#[test]
fn preview_154_save_as_copies_verified_external_bytes_and_rebases_changed_values() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("source.ldtk");
    let destination = directory.path().join("other/copy.ldtk");
    fs::create_dir(destination.parent().unwrap()).unwrap();
    let mut project = Project::new();
    let resource = project.add_level(None, "Resource", 32, 32).unwrap();
    let edited = project.add_level(None, "Edited", 32, 32).unwrap();
    let disk_edit = project.add_level(None, "DiskEdit", 32, 32).unwrap();
    let levels = project.level_keys();
    project.level_mut(resource).unwrap()["bgRelPath"] = json!("art/resource.png");
    project.level_mut(edited).unwrap()["extension"] = json!("old");
    project.set_external_levels(true).unwrap();
    project.save(&original).unwrap();
    let original_paths: Vec<_> = levels
        .iter()
        .map(|key| {
            directory.path().join(
                project.json["levels"][key.level]["externalRelPath"]
                    .as_str()
                    .unwrap(),
            )
        })
        .collect();
    let original_bytes: Vec<_> = original_paths
        .iter()
        .map(|path| {
            let mut bytes = fs::read(path).unwrap();
            bytes.extend_from_slice(b"\n \t\n");
            fs::write(path, &bytes).unwrap();
            bytes
        })
        .collect();

    let mut project = Project::load(&original).unwrap();
    let mut changed_on_disk = original_bytes[disk_edit.level].clone();
    let tab = changed_on_disk.len() - 2;
    changed_on_disk[tab] = b' ';
    fs::write(&original_paths[disk_edit.level], &changed_on_disk).unwrap();
    project.level_mut(edited).unwrap()["extension"] = json!("new");
    project.save(&destination).unwrap();
    let published = Project::load(&destination).unwrap();
    let copies: Vec<_> = levels
        .iter()
        .map(|key| {
            let path = published.json["levels"][key.level]["externalRelPath"]
                .as_str()
                .unwrap();
            fs::read(destination.parent().unwrap().join(path)).unwrap()
        })
        .collect();
    assert_eq!(copies[0], original_bytes[0]);
    assert_ne!(copies[resource.level], original_bytes[resource.level]);
    assert_eq!(
        published.level(resource).unwrap()["bgRelPath"],
        "../art/resource.png"
    );
    assert_ne!(copies[edited.level], original_bytes[edited.level]);
    assert_eq!(published.level(edited).unwrap()["extension"], "new");
    assert_ne!(copies[disk_edit.level], original_bytes[disk_edit.level]);
    assert_ne!(copies[disk_edit.level], changed_on_disk);
    assert_eq!(
        fs::read(&original_paths[disk_edit.level]).unwrap(),
        changed_on_disk
    );

    let next = destination.with_file_name("next.ldtk");
    project.save(&next).unwrap();
    let rel = project.json["levels"][0]["externalRelPath"]
        .as_str()
        .unwrap();
    assert_eq!(
        fs::read(next.parent().unwrap().join(rel)).unwrap(),
        original_bytes[0]
    );
}

#[test]
fn preview_154_save_as_reencodes_external_levels_after_output_option_changes() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("source.ldtk");
    let mut project = Project::new();
    project.add_layer("Collision", "IntGrid", 16).unwrap();
    project.set_external_levels(true).unwrap();
    project.save(&original).unwrap();
    let rel = project.json["levels"][0]["externalRelPath"]
        .as_str()
        .unwrap();
    let original_bytes = fs::read(directory.path().join(rel)).unwrap();

    for setting in ["jsonStyle", "flags"] {
        let mut project = Project::load(&original).unwrap();
        project.json[setting] = if setting == "jsonStyle" {
            json!("Minified")
        } else {
            json!(["ExportPreCsvIntGridFormat"])
        };
        let path = directory.path().join(format!("settings-{setting}.ldtk"));
        project.save(&path).unwrap();
        let rel = project.json["levels"][0]["externalRelPath"]
            .as_str()
            .unwrap();
        let bytes = fs::read(directory.path().join(rel)).unwrap();
        assert_ne!(bytes, original_bytes);
        if setting == "jsonStyle" {
            assert!(!bytes.contains(&b'\n'));
        } else {
            let reopened = Project::load(path).unwrap();
            assert!(reopened.layer(reopened.level_keys()[0], 0).unwrap()["intGrid"].is_array());
        }
    }
}

#[test]
fn preview_154_load_repairs_externally_renamed_project_and_level_folder() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.ldtk");
    let renamed = directory.path().join("renamed.ldtk");
    let mut project = Project::new();
    project.enable_multiworlds().unwrap();
    let level = project.level_keys()[0];
    project.level_mut(level).unwrap()["future"] = json!({"keep": 11});
    project.set_external_levels(true).unwrap();
    project.json["backupOnSave"] = json!(true);
    project.save(&source).unwrap();
    let old_stub = project.json["worlds"][0]["levels"][0].clone();
    fs::rename(&source, &renamed).unwrap();
    fs::rename(
        directory.path().join("original"),
        directory.path().join("renamed"),
    )
    .unwrap();

    let mut loaded = Project::load(&renamed).unwrap();
    assert_eq!(
        loaded.json["worlds"][0]["levels"][0]["externalRelPath"],
        "renamed/Level_0.ldtkl"
    );
    assert_eq!(loaded.level(level).unwrap()["iid"], old_stub["iid"]);
    assert_eq!(loaded.level(level).unwrap()["future"], json!({"keep": 11}));
    let saved_root: serde_json::Value =
        serde_json::from_slice(&fs::read(&renamed).unwrap()).unwrap();
    assert_eq!(saved_root["worlds"][0]["levels"][0], old_stub);
    assert!(loaded.migrate_to_current().unwrap());
    assert!(!loaded.migrate_to_current().unwrap());

    loaded.save(&renamed).unwrap();
    let saved = Project::load(&renamed).unwrap();
    assert_eq!(
        saved.json["worlds"][0]["levels"][0]["externalRelPath"],
        "renamed/Level_0.ldtkl"
    );
    assert_eq!(saved.level(level).unwrap()["future"], json!({"keep": 11}));
    let backups = saved.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    let backup_root: serde_json::Value =
        serde_json::from_slice(&fs::read(&backups[0].path).unwrap()).unwrap();
    assert_eq!(backup_root["worlds"][0]["levels"][0], old_stub);
    assert!(
        backups[0]
            .path
            .parent()
            .unwrap()
            .join("renamed/Level_0.ldtkl")
            .is_file()
    );
    let backed_up = Project::load(&backups[0].path).unwrap();
    assert_eq!(
        backed_up.level(level).unwrap()["future"],
        json!({"keep": 11})
    );
}

#[test]
fn preview_154_external_path_repair_preserves_missing_and_invalid_file_errors() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.ldtk");
    let renamed = directory.path().join("renamed.ldtk");
    let mut project = Project::new();
    project.set_external_levels(true).unwrap();
    project.save(&source).unwrap();
    fs::rename(source, &renamed).unwrap();
    fs::rename(
        directory.path().join("original"),
        directory.path().join("renamed"),
    )
    .unwrap();
    let path = directory.path().join("renamed/Level_0.ldtkl");
    let mut level: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    fs::write(&path, b"invalid JSON").unwrap();
    assert!(
        matches!(Project::load(&renamed), Err(ldtk_core::Error::Json {path: error_path, ..}) if error_path == path)
    );
    level["iid"] = json!("unrelated-level");
    fs::write(&path, serde_json::to_vec(&level).unwrap()).unwrap();
    assert!(
        matches!(Project::load(&renamed), Err(ldtk_core::Error::Invalid(message)) if message.contains("identity"))
    );
    fs::remove_file(path).unwrap();
    let original_level = directory.path().join("original/Level_0.ldtkl");
    assert!(
        matches!(Project::load(&renamed), Err(ldtk_core::Error::Io {path: error_path, source}) if error_path == original_level && source.kind() == std::io::ErrorKind::NotFound)
    );
}

#[test]
fn explicit_legacy_migration_converts_aliases_cells_tiles_fields_and_is_idempotent() {
    let mut p=Project::from_json(json!({"jsonVersion":"0.3.0","advancedOptionFlags":[],"exportPng":true,"defs":{"layers":[{"uid":1,"identifier":"Wall","type":"IntGrid","gridSize":16,"intGridValues":[{"color":"#ff0000"}],"autoTilesetDefUid":7}],"tilesets":[{"uid":7,"path":"atlas.png","pxWid":32,"pxHei":16,"tileGridSize":16}],"entities":[{"uid":2,"name":"Player","width":16,"height":16,"fieldDefs":[{"uid":3,"name":"path","type":"F_File"}]}]},"levels":[{"uid":0,"identifier":"Room","pxWid":32,"pxHei":16,"layerInstances":[{"layerDefId":1,"intGrid":[{"coordId":1,"v":0}],"gridTiles":[{"coordId":0,"tileId":1,"f":0}],"entityInstances":[{"defId":2,"x":8,"y":16,"fieldInstances":[{"defId":3,"realEditorValue":{"id":"V_String","params":["asset.txt"]}}]}]}]}]})).unwrap();
    assert!(p.migrate_to_current().unwrap());
    assert_eq!(p.json["jsonVersion"], "1.5.4");
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
fn preview_154_migration_reconciles_level_and_entity_fields() {
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Project::new();
    fixture.add_layer("Actors", "Entities", 16).unwrap();
    let actor = fixture.add_entity_def("Actor", 16, 16).unwrap();
    let health = fixture
        .add_field_definition(None, "Health", "Int", false)
        .unwrap();
    let speed = fixture
        .add_field_definition(Some(actor), "Speed", "Int", false)
        .unwrap();
    let armor = fixture
        .add_field_definition(Some(actor), "Armor", "Int", false)
        .unwrap();
    fixture.set_field_default(health, json!(100)).unwrap();
    fixture.set_field_default(speed, json!(5)).unwrap();
    let key = fixture.level_keys()[0];
    fixture.add_entity(key, 0, actor, 0, 0).unwrap();
    let instance = fixture.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    fixture
        .set_field_value(key, Some(&instance), armor, json!(7))
        .unwrap();
    fixture.json["jsonVersion"] = json!("1.5.3");
    fixture.level_mut(key).unwrap()["fieldInstances"] = json!([
        {"defUid":9999,"realEditorValues":[]},
        {"defUid":speed,"realEditorValues":[]},
    ]);
    let fields = &mut fixture.layer_mut(key, 0).unwrap()["entityInstances"][0]["fieldInstances"];
    let mut explicit = fields
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["defUid"] == armor)
        .unwrap()
        .clone();
    explicit["extension"] = json!({"stored":true});
    *fields = json!([explicit, {"defId":9999,"realEditorValues":[]}]);

    for external in [false, true] {
        let path = directory.path().join(format!("fields-{external}.ldtk"));
        let mut project = fixture.clone();
        project.set_external_levels(external).unwrap();
        project.save(&path).unwrap();
        let mut project = Project::load(&path).unwrap();
        assert_eq!(
            project.level(key).unwrap()["fieldInstances"][0]["defUid"],
            9999
        );
        assert!(project.migrate_to_current().unwrap());

        let expected_health = json!([{"defUid":health,"__identifier":"Health","__type":"Int","__value":100,"__tile":null,"realEditorValues":[]}]);
        assert_eq!(
            project.level(key).unwrap()["fieldInstances"],
            expected_health
        );
        if external {
            assert_eq!(project.json["levels"][0]["fieldInstances"], expected_health);
        }
        let fields = &project.layer(key, 0).unwrap()["entityInstances"][0]["fieldInstances"];
        assert_eq!(fields.as_array().unwrap().len(), 2);
        assert_eq!(fields[0]["defUid"], speed);
        assert_eq!(fields[0]["__value"], 5);
        assert_eq!(fields[0]["realEditorValues"], json!([]));
        assert_eq!(fields[1], explicit);
        assert!(!project.migrate_to_current().unwrap());

        project.set_field_default(health, json!(120)).unwrap();
        project.set_field_default(speed, json!(9)).unwrap();
        project.save(&path).unwrap();
        let reopened = Project::load(&path).unwrap();
        assert_eq!(
            reopened.level(key).unwrap()["fieldInstances"][0]["__value"],
            120
        );
        let fields = &reopened.layer(key, 0).unwrap()["entityInstances"][0]["fieldInstances"];
        assert_eq!(fields[0]["__value"], 9);
        assert_eq!(fields[1], explicit);
    }
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
    let (mut p, snapshot) = Project::load_with_snapshot(&path).unwrap();
    p.set_external_levels(false).unwrap();
    p.save_if_unchanged(&path, &snapshot).unwrap();
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
    assert_eq!(fs::read(backup).unwrap(), before);
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
    let report = p.rename_project("B").unwrap();
    assert!(!old.exists());
    assert!(!t.path().join("A").exists());
    assert_eq!(fs::read(t.path().join("B.ldtk.meta")).unwrap(), b"meta");
    assert_eq!(fs::read(t.path().join("B/keep.txt")).unwrap(), b"generated");
    assert_eq!(p.path, Some(new.clone()));
    assert!(Project::load(&new).is_ok());
    let snapshot = report.document_snapshot.unwrap();
    let external = t.path().join(p.external_levels.keys().next().unwrap());
    assert!(snapshot.is_document(&new) && snapshot.is_document(&external));
    assert!(!snapshot.is_document(&old));
    assert!(snapshot.changed_files().unwrap().is_empty());
    fs::write(t.path().join("Collision.ldtk"), b"keep").unwrap();
    assert!(p.rename_project("Collision").is_err());
    assert_eq!(p.path, Some(t.path().join("B.ldtk")));
    assert_eq!(fs::read(t.path().join("Collision.ldtk")).unwrap(), b"keep");
}

#[test]
fn extension_rename_preserves_file_bytes_external_levels_and_watcher() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("Map.ldtk");
    let new = dir.path().join("Map.json");
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.json["levels"][0]["bgRelPath"] = json!("Map/art.png");
    p.save(&old).unwrap();
    let resource = dir.path().join("Map/art.png");
    fs::write(&resource, b"resource bytes").unwrap();
    let external = dir.path().join(p.external_levels.keys().next().unwrap());
    let external_bytes = fs::read(&external).unwrap();
    let root_bytes = fs::read(&old).unwrap();
    let mut snapshot = p.filesystem_snapshot().unwrap();

    fs::write(&new, b"existing project").unwrap();
    assert!(
        p.rename_project_extension(false, Some(&mut snapshot))
            .is_err()
    );
    assert_eq!(p.path.as_deref(), Some(old.as_path()));
    assert_eq!(fs::read(&old).unwrap(), root_bytes);
    assert_eq!(fs::read(&new).unwrap(), b"existing project");
    fs::remove_file(&new).unwrap();

    assert!(
        p.rename_project_extension(false, Some(&mut snapshot))
            .unwrap()
    );
    assert!(!old.exists());
    assert_eq!(fs::read(&new).unwrap(), root_bytes);
    assert_eq!(fs::read(&external).unwrap(), external_bytes);
    assert_eq!(fs::read(&resource).unwrap(), b"resource bytes");
    assert_eq!(
        Project::load(&new).unwrap().external_levels,
        p.external_levels
    );
    assert!(snapshot.is_document(&new) && snapshot.is_document(&external));
    assert!(!snapshot.is_document(&old) && !snapshot.is_document(&resource));
    assert!(snapshot.changed_files().unwrap().is_empty());

    // Renaming must retain the original fingerprint even if the file had been
    // edited outside the app since it was loaded.
    fs::write(&new, b"changed on disk").unwrap();
    assert!(
        p.rename_project_extension(true, Some(&mut snapshot))
            .unwrap()
    );
    assert_eq!(snapshot.changed_files().unwrap(), vec![old.clone()]);
    assert_eq!(fs::read(&old).unwrap(), b"changed on disk");
    assert!(!new.exists());

    fs::remove_file(&old).unwrap();
    assert!(
        p.rename_project_extension(false, Some(&mut snapshot))
            .is_err()
    );
    assert_eq!(p.path.as_deref(), Some(old.as_path()));
    assert!(
        !new.exists(),
        "failed rename cleans up the destination reservation"
    );
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

#[test]
fn raw_legacy_save_preserves_both_intgrid_encodings_and_text_types() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.ldtk");
    let mut fixture = json!({
        "jsonVersion": "0.9.3",
        "flags": [],
        "defs": {"levelFields": [{"uid": 10, "type": "F_Text", "__type": "Multilines"}]},
        "levels": [{
            "identifier": "Room",
            "externalRelPath": null,
            "fieldInstances": [{"defUid": 10, "__type": "Multilines", "__value": "Keep"}],
            "layerInstances": [{"intGrid": [{"coordId": 0, "v": 0}], "intGridCsv": [1, 0]}]
        }],
        "toc": [{"legacyData": true}]
    });
    fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let mut project = Project::load(&path).unwrap();
    project.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap().json, fixture);

    let level = fixture["levels"][0].clone();
    fixture["levels"][0]["externalRelPath"] = json!("legacy/Room.ldtkl");
    fixture["levels"][0]["layerInstances"] = serde_json::Value::Null;
    fixture["externalLevels"] = json!(true);
    fs::create_dir(directory.path().join("legacy")).unwrap();
    fs::write(
        directory.path().join("legacy/Room.ldtkl"),
        serde_json::to_vec(&level).unwrap(),
    )
    .unwrap();
    fs::write(&path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let mut project = Project::load(&path).unwrap();
    project.save(&path).unwrap();
    let reopened = Project::load(&path).unwrap();
    assert_eq!(reopened.json, fixture);
    assert_eq!(reopened.external_levels["legacy/Room.ldtkl"], level);
}

#[test]
fn migration_ignores_null_legacy_png_export_alias() {
    let directory = tempfile::tempdir().unwrap();
    for (alias, expected) in [
        (serde_json::Value::Null, "OneImagePerLevel"),
        (json!(false), "None"),
        (json!(true), "OneImagePerLayer"),
    ] {
        let path = directory.path().join(format!("export-{alias}.ldtk"));
        let mut project = Project::new();
        project.json["jsonVersion"] = json!("1.5.3");
        project.json["imageExportMode"] = json!("OneImagePerLevel");
        project.json["exportPng"] = alias.clone();
        project.save(&path).unwrap();

        let mut project = Project::load(&path).unwrap();
        assert_eq!(project.json.get("exportPng"), Some(&alias));
        assert_eq!(project.json["imageExportMode"], "OneImagePerLevel");
        assert!(project.migrate_to_current().unwrap());
        project.save(&path).unwrap();

        let reopened = Project::load(&path).unwrap();
        assert_eq!(reopened.json["imageExportMode"], expected);
        assert!(reopened.json.get("exportPng").is_none());
    }
}

#[test]
fn migration_rejects_newer_projects_without_modifying_them() {
    let mut p = Project::new();
    p.json["jsonVersion"] = json!("1.5.5");
    p.json["previewFeature"] = json!({"keep": true});
    let before = p.json.clone();
    let error = p.migrate_to_current().unwrap_err().to_string();
    assert!(error.contains("1.5.5"), "{error}");
    assert_eq!(p.json, before);
}

#[test]
fn preview_154_json_styles_preserve_values_and_format_external_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("styles.ldtk");
    let mut p = Project::new();
    let header = p.json["__header__"].clone();
    p.json["__header__"]["projectOnly"] = json!(true);
    p.set_external_levels(true).unwrap();
    p.json["sampleList"] = json!([1, 2, 3]);
    for style in ["Full", "Compact", "Minified"] {
        p.json["jsonStyle"] = json!(style);
        p.save(&path).unwrap();
        let root = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&root).unwrap();
        assert_eq!(parsed["sampleList"], json!([1, 2, 3]));
        assert!(parsed["levels"][0].get("__header__").is_none());
        let external = dir
            .path()
            .join(parsed["levels"][0]["externalRelPath"].as_str().unwrap());
        let level = fs::read_to_string(external).unwrap();
        let level_json: serde_json::Value = serde_json::from_str(&level).unwrap();
        assert_eq!(level_json["__header__"], header);
        assert_eq!(
            level_json.as_object().unwrap().keys().next().unwrap(),
            "__header__"
        );
        match style {
            "Full" => assert!(root.contains("\"sampleList\": [\n")),
            "Compact" => assert!(root.contains("\"sampleList\": [1,2,3]")),
            _ => {
                assert!(!root.contains('\n'));
                assert!(!level.contains('\n'));
            }
        }
    }
}

#[test]
fn preview_154_regenerated_external_headers_preserve_extension_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("headers.ldtk");
    let key = LevelKey {
        world: None,
        level: 0,
    };
    let mut project = Project::new();
    let mut header = project.json["__header__"].clone();
    header["levelHeaderExtension"] = json!({"extra": 123});
    project.json["jsonVersion"] = json!("1.5.3");
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();
    let level = project.level_mut(key).unwrap();
    level["__header__"] = header.clone();
    level["__header__"]["appVersion"] = json!("1.5.3");
    level["levelExtension"] = json!({"raw": true});
    project.save(&path).unwrap();

    let mut raw = Project::load(&path).unwrap();
    let saved_level = raw.level(key).unwrap().clone();
    assert_eq!(saved_level["__header__"]["appVersion"], "1.5.3");
    raw.save(&path).unwrap();
    assert_eq!(
        Project::load(&path).unwrap().level(key).unwrap(),
        &saved_level
    );

    raw.migrate_to_current().unwrap();
    let recovery_path = raw.write_crash_recovery(dir.path()).unwrap();
    raw.save(&path).unwrap();
    for saved_path in [&path, &recovery_path] {
        let reopened = Project::load(saved_path).unwrap();
        assert!(reopened.json["levels"][0].get("__header__").is_none());
        let level = reopened.level(key).unwrap();
        assert_eq!(level["__header__"], header);
        assert_eq!(level["levelExtension"], saved_level["levelExtension"]);
    }
}

#[test]
fn preview_154_json_styles_roundtrip_strings_and_numeric_grids_without_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("escape.ldtk");
    let mut p = Project::new();
    p.json["future"] = json!({"text":"quote \" slash \\ newline\n tab\t", "numbers":(0..100).collect::<Vec<_>>(), "nested":[{"flag":true}, null]});
    for style in ["Full", "Compact", "Minified"] {
        p.json["jsonStyle"] = json!(style);
        p.save(&path).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["future"], p.json["future"]);
    }
    let before = fs::read(&path).unwrap();
    p.json["jsonStyle"] = json!("FutureMode");
    assert!(p.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn preview_154_save_projects_multiline_types_without_changing_editor_fields() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let actor = p.add_entity_def("Actor", 16, 16).unwrap();
    let notes = p
        .add_field_definition(None, "Notes", "Multilines", false)
        .unwrap();
    p.add_field_definition(None, "Title", "String", false)
        .unwrap();
    let lines = p
        .add_field_definition(Some(actor), "Lines", "Multilines", true)
        .unwrap();
    let key = p.level_keys()[0];
    p.add_entity(key, 0, actor, 0, 0).unwrap();
    let entity = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(key, None, notes, json!("Two\nlines"))
        .unwrap();
    p.set_field_value(key, Some(&entity), lines, json!(["One\nentry", "Another"]))
        .unwrap();
    p.json["extensionValue"] = json!({"defUid":notes,"__type":"Multilines"});
    let editor_level = p.level(key).unwrap().clone();
    for (external, enabled) in [(false, false), (true, true), (true, false)] {
        let mut saved = p.clone();
        if external {
            saved.enable_multiworlds().unwrap();
            saved.set_external_levels(true).unwrap();
        }
        if enabled {
            saved.json["flags"]
                .as_array_mut()
                .unwrap()
                .push(json!("UseMultilinesType"));
        }
        let path = dir.path().join(format!("multi-{external}-{enabled}.ldtk"));
        let key = saved.level_keys()[0];
        saved.save(&path).unwrap();
        let disk = Project::load(&path).unwrap();
        let disk_level = disk.level(key).unwrap();
        let expected = if enabled { "Multilines" } else { "String" };
        let expected_array = format!("Array<{expected}>");
        let defs = &disk.json["defs"];
        assert_eq!(defs["levelFields"][0]["__type"], expected);
        assert_eq!(defs["levelFields"][0]["type"], "F_Text");
        assert_eq!(defs["levelFields"][1]["__type"], "String");
        assert_eq!(
            defs["entities"][0]["fieldDefs"][0]["__type"],
            expected_array
        );
        assert_eq!(disk_level["fieldInstances"][0]["__type"], expected);
        assert_eq!(disk_level["fieldInstances"][1]["__type"], "String");
        if external {
            assert_eq!(
                disk.json["worlds"][0]["levels"][0]["fieldInstances"][0]["__type"],
                expected
            );
        }
        assert_eq!(disk.json["extensionValue"], p.json["extensionValue"]);
        let disk_entity_field =
            &disk_level["layerInstances"][0]["entityInstances"][0]["fieldInstances"][0];
        assert_eq!(disk_entity_field["__type"], expected_array);
        for property in ["__value", "realEditorValues"] {
            assert_eq!(
                disk_level["fieldInstances"][0][property],
                editor_level["fieldInstances"][0][property]
            );
            assert_eq!(
                disk_entity_field[property],
                editor_level["layerInstances"][0]["entityInstances"][0]["fieldInstances"][0]
                    [property]
            );
        }
        assert_eq!(saved.json["defs"]["levelFields"][0]["__type"], "Multilines");
        assert_eq!(
            saved.level(key).unwrap()["fieldInstances"][0]["__type"],
            "Multilines"
        );
        let mut reopened = disk;
        reopened.migrate_to_current().unwrap();
        assert_eq!(
            reopened.level(key).unwrap()["fieldInstances"][0]["__type"],
            "Multilines"
        );
        reopened.save(&path).unwrap();
        let mut latest = Project::load(&path).unwrap();
        assert_eq!(
            latest.level(key).unwrap()["fieldInstances"][0]["__type"],
            expected
        );
        assert!(
            !latest.migrate_to_current().unwrap(),
            "the saved public type is not a format upgrade"
        );
        assert_eq!(
            latest.level(key).unwrap()["fieldInstances"][0]["__type"],
            "Multilines"
        );
        if external && !enabled {
            let recovery = latest.write_crash_recovery(dir.path()).unwrap();
            let recovered = Project::load(recovery).unwrap();
            assert_eq!(
                recovered.level(key).unwrap()["fieldInstances"][0]["__type"],
                "String"
            );
        }
    }
}

#[test]
fn preview_154_save_generates_legacy_intgrid_from_csv_and_omits_it_when_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    let int_grid = p.add_layer("Collision", "IntGrid", 16).unwrap();
    p.add_layer("Actors", "Entities", 16).unwrap();
    p.update_layer_properties(
        int_grid,
        &json!({"intGridValues":[{"value":1,"color":"#123456"},{"value":2,"color":"#987654"}]}),
    )
    .unwrap();
    let key = p.level_keys()[0];
    p.resize_level(key, 32, 32, 0, 0).unwrap();
    let index = p.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .position(|layer| layer["layerDefUid"] == int_grid)
        .unwrap();
    p.paint_int_grid(key, index, 1, 0, 1).unwrap();
    p.paint_int_grid(key, index, 1, 1, 2).unwrap();
    for external in [false, true] {
        let mut saved = p.clone();
        if external {
            saved.enable_multiworlds().unwrap();
            saved.set_external_levels(true).unwrap();
        }
        saved.json["flags"]
            .as_array_mut()
            .unwrap()
            .push(json!("ExportPreCsvIntGridFormat"));
        let path = dir.path().join(format!("legacy-{external}.ldtk"));
        let key = saved.level_keys()[0];
        saved.save(&path).unwrap();
        let mut disk = Project::load(&path).unwrap();
        assert_eq!(
            disk.layer(key, index).unwrap()["intGridCsv"],
            json!([0, 1, 0, 2])
        );
        assert_eq!(
            disk.layer(key, index).unwrap()["intGrid"],
            json!([{"coordId":1,"v":0}, {"coordId":3,"v":1}])
        );
        assert_eq!(disk.layer(key, 1 - index).unwrap()["intGrid"], json!([]));
        assert!(saved.layer(key, index).unwrap().get("intGrid").is_none());

        let mut history = History::default();
        history.begin_layer(&disk, key, index);
        assert!(!disk.paint_int_grid(key, index, 0, 0, 0).unwrap());
        disk.paint_int_grid(key, index, 1, 0, 0).unwrap();
        assert!(history.commit(&disk));
        let read_legacy = |project: &mut Project| {
            project.save(&path).unwrap();
            Project::load(&path).unwrap().layer(key, index).unwrap()["intGrid"].clone()
        };
        assert_eq!(
            read_legacy(&mut disk),
            json!([{"coordId":0,"v":-1},{"coordId":2,"v":-1},{"coordId":3,"v":1}])
        );
        let mut resized = disk.clone();
        assert!(history.undo_level(&mut disk, key));
        assert_eq!(
            read_legacy(&mut disk),
            json!([{"coordId":0,"v":-1},{"coordId":1,"v":0},{"coordId":2,"v":-1},{"coordId":3,"v":1}])
        );
        assert!(history.redo_level(&mut disk, key));
        assert_eq!(
            read_legacy(&mut disk),
            json!([{"coordId":0,"v":-1},{"coordId":1,"v":-1},{"coordId":2,"v":-1},{"coordId":3,"v":1}])
        );
        let mut resize_history = History::default();
        resize_history.begin(&resized, key);
        resized.resize_level(key, 48, 32, 16, 0).unwrap();
        assert!(resize_history.commit(&resized));
        assert_eq!(
            read_legacy(&mut resized),
            json!([{"coordId":1,"v":-1},{"coordId":4,"v":-1},{"coordId":5,"v":1}])
        );
        assert!(resize_history.undo_level(&mut resized, key));
        assert!(resize_history.redo_level(&mut resized, key));
        assert_eq!(
            read_legacy(&mut resized),
            json!([{"coordId":0,"v":-1},{"coordId":1,"v":-1},{"coordId":2,"v":-1},{"coordId":3,"v":-1},{"coordId":4,"v":-1},{"coordId":5,"v":1}])
        );
        assert!(history.undo_level(&mut disk, key));

        let mut bulk = disk.clone();
        let group = bulk
            .copy_selection_group(key, &[index], [16, 0, 16, 16])
            .unwrap();
        bulk.move_selection_group(key, &group, [0, 16], false, false)
            .unwrap();
        assert_eq!(
            read_legacy(&mut bulk),
            json!([{"coordId":0,"v":-1},{"coordId":2,"v":-1},{"coordId":3,"v":0}])
        );
        let empty = bulk.copy_region(key, index, [0, 0, 1, 1]).unwrap();
        bulk.paste_region(key, index, &empty, 1, 1, false, false, false)
            .unwrap();
        assert_eq!(
            read_legacy(&mut bulk),
            json!([{"coordId":0,"v":-1},{"coordId":2,"v":-1}])
        );
        bulk.change_layer_grid_size(int_grid, 32).unwrap();
        assert_eq!(read_legacy(&mut bulk), json!([]));

        disk.paint_int_grid(key, index, 0, 0, 2).unwrap();
        disk.save(&path).unwrap();
        assert_eq!(
            Project::load(&path).unwrap().layer(key, index).unwrap()["intGrid"][0],
            json!({"coordId":0,"v":1})
        );
        disk.json["flags"]
            .as_array_mut()
            .unwrap()
            .retain(|flag| flag != "ExportPreCsvIntGridFormat");
        disk.save(&path).unwrap();
        let reopened = Project::load(&path).unwrap();
        assert_eq!(
            reopened.layer(key, index).unwrap()["intGridCsv"],
            json!([2, 1, 0, 2])
        );
        assert!(reopened.layer(key, index).unwrap().get("intGrid").is_none());

        let layer = disk.layer_mut(key, index).unwrap().as_object_mut().unwrap();
        layer.remove("intGridCsv");
        let sparse = json!([{"coordId":1,"v":0},{"coordId":2,"v":-1}]);
        layer.insert("intGrid".into(), sparse.clone());
        disk.save(&path).unwrap();
        let mut legacy = Project::load(&path).unwrap();
        assert_eq!(legacy.layer(key, index).unwrap()["intGrid"], sparse);
        legacy.migrate_to_current().unwrap();
        assert_eq!(
            legacy.layer(key, index).unwrap()["intGridCsv"],
            json!([0, 1, 0, 0])
        );
        legacy.json["flags"]
            .as_array_mut()
            .unwrap()
            .push(json!("ExportPreCsvIntGridFormat"));
        assert_eq!(read_legacy(&mut legacy), sparse);
    }
}

#[test]
fn preview_154_level_history_reopens_all_grids_on_the_restored_level() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("timeline.ldtk");
    let mut project = Project::new();
    project.add_layer("First", "IntGrid", 16).unwrap();
    project.add_layer("Second", "IntGrid", 16).unwrap();
    let key = project.level_keys()[0];
    let other = project.add_level(None, "Other", 32, 16).unwrap();
    project.resize_level(key, 32, 16, 0, 0).unwrap();
    project.json["flags"]
        .as_array_mut()
        .unwrap()
        .push(json!("ExportPreCsvIntGridFormat"));
    let mut history = History::default();
    let dense_empty = json!([{"coordId":0,"v":-1},{"coordId":1,"v":-1}]);
    let read_project = |project: &mut Project| {
        project.save(&path).unwrap();
        Project::load(&path).unwrap()
    };

    history.begin_layer(&project, key, 0);
    project.paint_int_grid(key, 0, 0, 0, 1).unwrap();
    assert!(history.cancel(&mut project));
    let output = read_project(&mut project);
    assert_eq!(output.layer(key, 0).unwrap()["intGrid"], json!([]));
    assert_eq!(output.layer(key, 1).unwrap()["intGrid"], json!([]));

    history.begin_layer(&project, key, 0);
    project.paint_int_grid(key, 0, 0, 0, 1).unwrap();
    assert!(history.commit(&project));
    assert!(history.undo_level(&mut project, key));
    let output = read_project(&mut project);
    assert_eq!(output.layer(key, 0).unwrap()["intGrid"], dense_empty);
    assert_eq!(output.layer(key, 1).unwrap()["intGrid"], dense_empty);
    assert_eq!(output.layer(other, 0).unwrap()["intGrid"], json!([]));
    assert_eq!(output.layer(other, 1).unwrap()["intGrid"], json!([]));

    assert!(history.redo_level(&mut project, key));
    let output = read_project(&mut project);
    assert_eq!(
        output.layer(key, 0).unwrap()["intGrid"],
        json!([{"coordId":0,"v":0},{"coordId":1,"v":-1}])
    );
    assert_eq!(output.layer(key, 1).unwrap()["intGrid"], dense_empty);
    assert_eq!(output.layer(other, 0).unwrap()["intGrid"], json!([]));
    assert_eq!(output.layer(other, 1).unwrap()["intGrid"], json!([]));
}

#[test]
fn preview_154_migration_preserves_new_fields_and_upgrades_legacy_blocker() {
    let mut p = Project::new();
    let auto = p.add_layer("Auto", "IntGrid", 16).unwrap();
    let blocker = p.add_layer("Blocker", "Tiles", 16).unwrap();
    p.json["jsonVersion"] = json!("1.5.3");
    p.json.as_object_mut().unwrap().remove("jsonStyle");
    let d = p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["uid"] == auto)
        .unwrap();
    d.as_object_mut()
        .unwrap()
        .remove("layerUidsPreventingAutoTilingHere");
    d["autoTilesKilledByOtherLayerUid"] = json!(blocker);
    p.migrate_to_current().unwrap();
    assert_eq!(p.json["jsonVersion"], "1.5.4");
    assert_eq!(p.json["jsonStyle"], "Compact");
    let def = p.layer_def(auto).unwrap();
    assert_eq!(def["layerUidsPreventingAutoTilingHere"], json!([blocker]));
    assert_eq!(def["entityDepthSorting"], "Manual");
    assert_eq!(def["guideOpacity"], 0.33);
    let before = p.json.clone();
    assert!(!p.migrate_to_current().unwrap());
    assert_eq!(p.json, before);
}

#[test]
fn preview_154_saving_migrated_project_updates_header_version() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.json["jsonVersion"] = json!("1.5.3");
    p.json["__header__"]["appVersion"] = json!("1.5.3");
    p.json["__header__"]["extension"] = json!("preserved");
    p.migrate_to_current().unwrap();
    let path = dir.path().join("project.ldtk");
    p.save(&path).unwrap();
    let saved = Project::load(path).unwrap();
    assert_eq!(saved.json["__header__"]["appVersion"], "1.5.4");
    assert_eq!(saved.json["__header__"]["extension"], "preserved");
}

#[test]
#[ignore = "requires the pinned upstream 1.5.4 autoTileBlocking fixture"]
fn preview_154_upstream_project_migrates_saves_and_reopens_with_blockers_intact() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tools/ldtk-154/autoTileBlocking.ldtk");
    let original = fs::read(&source).unwrap();
    let mut p = Project::load(&source).unwrap();
    p.migrate_to_current().unwrap();
    assert_eq!(p.json["jsonVersion"], "1.5.4");
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("copy.ldtk");
    p.save(&target).unwrap();
    let mut reloaded = Project::load(&target).unwrap();
    reloaded.migrate_to_current().unwrap();
    let auto = reloaded.json["defs"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["identifier"] == "Optional_props")
        .unwrap();
    assert_eq!(auto["layerUidsPreventingAutoTilingHere"], json!([31, 27]));
    let key = reloaded.level_keys()[0];
    reloaded.rebuild_auto_layers(key).unwrap();
    let layer = reloaded.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["__identifier"] == "Optional_props")
        .unwrap();
    assert_eq!(layer["autoLayerTiles"].as_array().unwrap().len(), 99);
    assert_eq!(fs::read(source).unwrap(), original);
}

fn large_external_project_fixture() -> Project {
    let mut p = Project::new();
    p.json["externalLevels"] = json!(true);
    p.json["jsonStyle"] = json!("Minified");
    p.json["backupOnSave"] = json!(false);
    let template = p.json["levels"][0].clone();
    let mut stubs = Vec::new();
    for index in 0..30 {
        let rel = format!("levels/Room{index}.ldtkl");
        let mut level = template.clone();
        level["identifier"] = json!(format!("Room{index}"));
        level["iid"] = json!(format!("benchmark-{index}"));
        level["externalRelPath"] = json!(rel);
        level["payload"] = json!((0..80000).collect::<Vec<i32>>());
        let mut stub = level.clone();
        stub.as_object_mut().unwrap().remove("payload");
        stub["layerInstances"] = serde_json::Value::Null;
        stubs.push(stub);
        p.external_levels.insert(rel, level);
    }
    p.json["levels"] = json!(stubs);
    p
}

#[test]
#[ignore = "controlled repeat-save benchmark for the pinned embedded world fixture"]
fn benchmark_large_embedded_project_repeat_save() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.ldtk");
    let mut project = Project::load(source).unwrap();
    project.migrate_to_current().unwrap();
    project.json["backupOnSave"] = json!(false);
    project.json["jsonStyle"] = json!("Minified");
    project.save(&path).unwrap();
    let mut times = [std::time::Duration::ZERO; 7];
    for elapsed in &mut times {
        let start = std::time::Instant::now();
        project.save(&path).unwrap();
        *elapsed = start.elapsed();
    }
    times.sort_unstable();
    eprintln!(
        "Large embedded repeat save: median {:?}; samples {times:?}",
        times[3]
    );
}

#[test]
#[ignore = "controlled large external level save benchmark"]
fn benchmark_large_external_project_save() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("levels")).unwrap();
    let mut p = large_external_project_fixture();
    let start = std::time::Instant::now();
    p.save(temp.path().join("large.ldtk")).unwrap();
    eprintln!("large external save: {:?}", start.elapsed());
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        eprintln!(
            "{}",
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .unwrap_or("VmHWM unavailable")
        );
    }
}

#[test]
#[ignore = "controlled repeat-save benchmark with a multiline field definition"]
fn benchmark_large_external_project_repeat_save() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("levels")).unwrap();
    let path = temp.path().join("large.ldtk");
    let mut project = large_external_project_fixture();
    project
        .add_field_definition(None, "Notes", "Multilines", false)
        .unwrap();
    project.json["flags"]
        .as_array_mut()
        .unwrap()
        .push(json!("UseMultilinesType"));
    project.save(&path).unwrap();
    drop(project);
    let mut project = Project::load(&path).unwrap();
    project.migrate_to_current().unwrap();
    let mut times = [std::time::Duration::ZERO; 7];
    for elapsed in &mut times {
        let start = std::time::Instant::now();
        project.save(&path).unwrap();
        *elapsed = start.elapsed();
    }
    times.sort_unstable();
    let median = times[3];
    eprintln!("large external repeat save: median {median:?}; samples {times:?}");
}

#[test]
#[ignore = "release cache ordering benchmark for the pinned upstream large world fixture"]
fn benchmark_large_world_cached_tile_order() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let mut expected = Project::load(&path).unwrap();
    expected.migrate_to_current().unwrap();
    expected.reconcile_cached_auto_tiles();
    let mut times = [std::time::Duration::ZERO; 7];
    for elapsed in &mut times {
        let mut project = expected.clone();
        let mut tiles = 0;
        for key in project.level_keys() {
            let level = project.level_mut(key).unwrap();
            for layer in level["layerInstances"].as_array_mut().unwrap() {
                if let Some(cache) = layer["autoLayerTiles"].as_array_mut() {
                    tiles += cache.len();
                    cache.reverse();
                }
            }
        }
        assert!(tiles > 100_000);
        let start = std::time::Instant::now();
        project.reconcile_cached_auto_tiles();
        *elapsed = start.elapsed();
        assert_eq!(project.json, expected.json);
    }
    times.sort_unstable();
    eprintln!(
        "Large world cached tile order: median {:?}; samples {times:?}",
        times[3]
    );
}

#[test]
#[ignore = "release filesystem watcher benchmark for the pinned upstream large world fixture"]
fn benchmark_large_world_filesystem_snapshot() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let project = Project::load(&path).unwrap();
    let mut times = [std::time::Duration::ZERO; 7];
    for elapsed in &mut times {
        let start = std::time::Instant::now();
        let snapshot = project.filesystem_snapshot().unwrap();
        *elapsed = start.elapsed();
        assert!(snapshot.changed_files().unwrap().is_empty());
    }
    times.sort_unstable();
    eprintln!(
        "Large world filesystem snapshot: median {:?}; samples {times:?}",
        times[3]
    );
}

#[test]
#[ignore = "manual filesystem watcher benchmark with shared resource references"]
fn benchmark_shared_resource_filesystem_snapshot() {
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.ldtk");
    let sprite = dir.path().join("sprite.png");
    fs::write(&path, b"{}").unwrap();
    fs::write(&sprite, b"sprite").unwrap();
    let field = json!({
        "__type":"FilePath", "__value":"sprite.png",
        "realEditorValues":[{"id":"V_String","params":["./sprite.png"]}]
    });
    let mut project = Project::new();
    project.path = Some(path);
    project.json["levels"][0]["layerInstances"] = json!([{
        "__tilesetRelPath":"sprite.png",
        "entityInstances":vec![json!({"fieldInstances":[field]}); 16384]
    }]);

    let mut times = [Duration::ZERO; 5];
    for elapsed in &mut times {
        let start = Instant::now();
        let snapshot = project.filesystem_snapshot().unwrap();
        *elapsed = start.elapsed();
        assert!(snapshot.changed_files().unwrap().is_empty());
        fs::write(&sprite, b"change").unwrap();
        assert_eq!(
            snapshot.changed_files().unwrap().as_slice(),
            std::slice::from_ref(&sprite)
        );
        fs::write(&sprite, b"sprite").unwrap();
    }
    times.sort_unstable();
    eprintln!(
        "Shared resource filesystem snapshot: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "release migration timing for the pinned upstream large world fixture"]
fn benchmark_large_world_migration() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let mut times = [std::time::Duration::ZERO; 7];
    for elapsed in &mut times {
        let mut project = Project::load(&path).unwrap();
        let start = std::time::Instant::now();
        project.migrate_to_current().unwrap();
        *elapsed = start.elapsed();
    }
    times.sort_unstable();
    let median = times[3];
    eprintln!("large world migration: median {median:?}; samples {times:?}");
}

#[test]
#[ignore = "controlled large external level migration benchmark"]
fn benchmark_large_external_project_migration() {
    let mut project = large_external_project_fixture();
    let start = std::time::Instant::now();
    project.migrate_to_current().unwrap();
    eprintln!("large external migration: {:?}", start.elapsed());
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        eprintln!(
            "{}",
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .unwrap_or("VmHWM unavailable")
        );
    }
}

#[test]
fn preview_154_loading_progress_counts_completed_external_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("progress.ldtk");
    let mut project = Project::new();
    project.add_level(None, "Second", 256, 256).unwrap();
    project.json["externalLevels"] = json!(true);
    project.save(&path).unwrap();
    let mut updates = Vec::new();
    let loaded =
        Project::load_with_progress(&path, |done, total| updates.push((done, total))).unwrap();
    assert_eq!(loaded.level_keys().len(), 2);
    assert_eq!(updates, [(0, 2), (1, 2), (2, 2)]);
}

#[test]
fn preview_154_save_progress_counts_prepared_external_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("progress.ldtk");
    let mut project = Project::new();
    project.add_level(None, "Second", 256, 256).unwrap();
    project.json["externalLevels"] = json!(true);
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let mut updates = Vec::new();
    project
        .save_with_exports_progress(&path, |stage, done, total| {
            updates.push((stage, done, total))
        })
        .unwrap();
    assert_eq!(
        updates,
        [
            ("Preparing levels", 0, 2),
            ("Preparing levels", 1, 2),
            ("Preparing levels", 2, 2),
            ("Preparing levels", 0, 0),
            ("Processing export tasks", 0, 2),
            ("Processing export tasks", 1, 2),
            ("Processing export tasks", 2, 2),
            ("Processing export tasks", 0, 0)
        ]
    );
    assert_eq!(Project::load(path).unwrap().level_keys().len(), 2);
}

#[test]
fn preview_154_stages_external_levels_as_they_are_prepared() {
    use ldtk_core::SaveStage;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Streamed.ldtk");
    let mut project = Project::new();
    project.json["backupOnSave"] = json!(false);
    project.add_level(None, "Second", 32, 32).unwrap();
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();

    let root_before = fs::read(&path).unwrap();
    let outputs = project
        .level_keys()
        .iter()
        .map(|key| {
            dir.path().join(
                project.json["levels"][key.level]["externalRelPath"]
                    .as_str()
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let levels_before = outputs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    let temp_files = || {
        fs::read_dir(path.with_extension(""))
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name();
                let name = name.to_str().unwrap();
                (name.starts_with(".ldtk-") && name.ends_with(".tmp")).then_some(())
            })
            .count()
    };
    for key in project.level_keys() {
        project.level_mut(key).unwrap()["extension"] = json!("new level content");
    }
    project
        .save_with_operation_progress(&path, |stage, done, total| {
            if stage == SaveStage::PrepareLevels && (done, total) == (1, 2) {
                assert!(
                    temp_files() > 0,
                    "first level must be staged before preparing the next"
                );
                assert!(
                    fs::read_dir(dir.path()).unwrap().any(|entry| {
                        entry.unwrap().file_name().to_str().is_some_and(|name| {
                            name.starts_with(".ldtk-") && name.ends_with(".tmp")
                        })
                    }),
                    "main document must be staged before the external levels"
                );
                assert_eq!(fs::read(&path).unwrap(), root_before);
                for (file, bytes) in outputs.iter().zip(&levels_before) {
                    assert_eq!(&fs::read(file).unwrap(), bytes);
                }
            }
        })
        .unwrap();
    assert_eq!(temp_files(), 0);
    let reopened = Project::load(&path).unwrap();
    assert!(
        reopened
            .level_keys()
            .iter()
            .all(|&key| { reopened.level(key).unwrap()["extension"] == "new level content" })
    );
}

#[test]
fn preview_154_backup_progress_reports_each_file_before_project_publication() {
    use ldtk_core::SaveStage;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Map.ldtk");
    let mut project = Project::new();
    project.add_level(None, "Town", 32, 32).unwrap();
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();
    let old_root = fs::read(&path).unwrap();
    let key = project.level_keys()[0];
    let old_level_file = path.with_extension("").join("Level_0.ldtkl");
    project.level_mut(key).unwrap()["extension"] = json!("edited");
    project.level_mut(key).unwrap()["identifier"] = json!("Renamed");
    project.json["backupOnSave"] = json!(true);

    let mut backup = Vec::new();
    let mut stages = Vec::new();
    project
        .save_with_labeled_operation_progress(&path, |stage, done, total, label| {
            if stages.last() != Some(&stage) {
                stages.push(stage);
            }
            if stage == SaveStage::Backup {
                assert_eq!(fs::read(&path).unwrap(), old_root);
                backup.push((done, total, label.map(str::to_owned)));
            } else if stage == SaveStage::RemoveObsoleteLevels && done == 0 && label.is_none() {
                assert!(old_level_file.is_file());
                let published = Project::load(&path).unwrap();
                assert_eq!(published.level(key).unwrap()["identifier"], "Renamed");
            }
        })
        .unwrap();
    assert_eq!(
        stages,
        [
            SaveStage::Backup,
            SaveStage::WriteProject,
            SaveStage::PrepareLevels,
            SaveStage::RemoveObsoleteLevels,
            SaveStage::WriteProject,
        ]
    );
    assert_eq!(
        backup,
        [
            (0, 4, Some("Level_0.ldtkl".to_owned())),
            (1, 4, Some("Town.ldtkl".to_owned())),
            (2, 4, Some("Map.ldtk".to_owned())),
            (3, 4, None),
            (4, 4, None),
        ]
    );

    let backups = project.list_backups().unwrap();
    let restored = Project::load(&backups[0].path).unwrap();
    assert!(restored.level(key).unwrap().get("extension").is_none());
    let current = Project::load(&path).unwrap();
    assert_eq!(current.level(key).unwrap()["extension"], "edited");
    assert!(!old_level_file.exists());
}

#[test]
fn preview_154_save_operations_distinguish_publication_from_level_exports() {
    use ldtk_core::SaveStage;

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.json["imageExportMode"] = json!("OneImagePerLevel");

    let failed_path = dir.path().join("missing/Map.ldtk");
    let mut failed = Vec::new();
    assert!(
        project
            .save_with_operation_progress(&failed_path, |stage, done, total| {
                failed.push((stage, done, total));
            })
            .is_err()
    );
    assert_eq!(failed, [(SaveStage::WriteProject, 0, 1)]);

    let path = dir.path().join("Map.ldtk");
    let mut events = Vec::new();
    project
        .save_with_operation_progress(&path, |stage, done, total| {
            if stage == SaveStage::WriteProject {
                assert_eq!(path.exists(), done == 1);
            }
            events.push((stage, done, total));
        })
        .unwrap();
    assert_eq!(
        events,
        [
            (SaveStage::WriteProject, 0, 1),
            (SaveStage::WriteProject, 1, 1),
            (SaveStage::ExportPng, 0, 1),
            (SaveStage::ExportPng, 1, 1),
            (SaveStage::ExportPng, 0, 0),
        ]
    );
    assert!(dir.path().join("Map/png/Level_0.png").is_file());
}

#[test]
fn preview_154_migration_uses_default_level_dimensions_for_missing_world_grid_steps() {
    let directory = tempfile::tempdir().unwrap();
    let mut single = Project::new();
    single.json["jsonVersion"] = json!("1.5.3");
    single.json["worldLayout"] = json!("GridVania");
    single.json["defaultLevelWidth"] = json!(320);
    single.json["defaultLevelHeight"] = json!(192);
    single
        .json
        .as_object_mut()
        .unwrap()
        .remove("worldGridWidth");
    single.json["worldGridHeight"] = serde_json::Value::Null;
    let mut multi = single.clone();
    multi.enable_multiworlds().unwrap();
    let secondary = multi.add_world("Secondary").unwrap();
    let world = &mut multi.json["worlds"][0];
    world.as_object_mut().unwrap().remove("worldGridWidth");
    world["worldGridHeight"] = serde_json::Value::Null;
    multi.json["worlds"][secondary]["defaultLevelWidth"] = serde_json::Value::Null;
    multi.json["worlds"][secondary]["worldGridWidth"] = json!(128);

    for (mut project, name) in [(single, "single"), (multi, "multi")] {
        assert!(project.migrate_to_current().unwrap());
        assert!(!project.migrate_to_current().unwrap());
        let path = directory.path().join(format!("{name}.ldtk"));
        project.save(&path).unwrap();
        let reopened = Project::load(path).unwrap();
        let world = if name == "multi" {
            assert!(reopened.json["worldGridWidth"].is_null());
            assert!(reopened.json["worldGridHeight"].is_null());
            assert_eq!(reopened.json["worlds"][secondary]["defaultLevelWidth"], 256);
            assert_eq!(reopened.json["worlds"][secondary]["worldGridWidth"], 128);
            &reopened.json["worlds"][0]
        } else {
            &reopened.json
        };
        assert_eq!(world["worldLayout"], "GridVania");
        assert_eq!(world["worldGridWidth"], 320);
        assert_eq!(world["worldGridHeight"], 192);
    }
}

#[test]
fn legacy_world_layout_migration_and_missing_external_rollback() {
    let mut legacy = Project::new();
    legacy.json["jsonVersion"] = json!("0.5.0");
    legacy.json.as_object_mut().unwrap().remove("worldLayout");
    legacy.migrate_to_current().unwrap();
    assert_eq!(legacy.json["worldLayout"], "LinearHorizontal");
    assert!(!legacy.migrate_to_current().unwrap());

    let mut broken = Project::new();
    broken.json["jsonVersion"] = json!("0.5.0");
    broken.json["levels"][0]["externalRelPath"] = json!("missing.ldtkl");
    let before = broken.json.clone();
    assert!(broken.migrate_to_current().is_err());
    assert_eq!(broken.json, before);
    assert!(broken.external_levels.is_empty());
}
