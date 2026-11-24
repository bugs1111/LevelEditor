use ldtk_core::Project;
use serde_json::json;
use std::fs;
#[test]
fn json_and_text_enums_sync_order_colors_and_checksums() {
    let t = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("game.ldtk"));
    let file = t.path().join("values.json");
    fs::write(&file, r#"{"State":["Idle","Run"]}"#).unwrap();
    let report = p.sync_external_enums(&file).unwrap();
    assert!(report.applied);
    assert_eq!(p.json["defs"]["externalEnums"][0]["values"][1]["id"], "Run");
    let id = p.json["defs"]["externalEnums"][0]["uid"].clone();
    assert_eq!(
        p.json["defs"]["externalEnums"][0]["externalRelPath"],
        "values.json"
    );
    let checksum = p.json["defs"]["externalEnums"][0]["externalFileChecksum"]
        .as_str()
        .unwrap();
    assert_eq!(checksum.len(), 32);
    fs::write(&file, r#"{"State":["Run","Jump"]}"#).unwrap();
    p.sync_external_enums(&file).unwrap();
    assert_eq!(p.json["defs"]["externalEnums"][0]["uid"], id);
    assert_eq!(p.json["defs"]["externalEnums"][0]["values"][0]["id"], "Run");
    assert_eq!(
        p.json["defs"]["externalEnums"][0]["values"][1]["id"],
        "Jump"
    );
}
#[test]
fn haxe_parameterized_enums_are_skipped_and_used_removed_values_need_resolution() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("State.hx");
    fs::write(
        &path,
        "// enum Bogus { No; }\nenum State { Idle; Run; }\nenum Params { Data(x:Int); }\n",
    )
    .unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.sync_external_enums(&path).unwrap();
    assert_eq!(p.json["defs"]["externalEnums"].as_array().unwrap().len(), 1);
    p.json["levels"][0]["fieldInstances"] = json!([{"__identifier":"state","__type":"ExternEnum.State","__value":"Idle","realEditorValues":[{"id":"V_String","params":["Idle"]}]}]);
    fs::write(&path, "enum State { Run; Jump; }").unwrap();
    let before = p.json.clone();
    let report = p.sync_external_enums(&path).unwrap();
    assert!(report.requires_confirmation);
    assert!(!report.applied);
    assert_eq!(p.json, before);
    let mut renames = std::collections::BTreeMap::new();
    renames.insert("State.Idle".into(), "Jump".into());
    p.apply_external_enum_sync(&path, &renames, false).unwrap();
    assert_eq!(p.json["levels"][0]["fieldInstances"][0]["__value"], "Jump");
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["realEditorValues"][0]["params"][0],
        "Jump"
    );
}
#[test]
fn castledb_preserves_colors_and_unique_ids() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("data.cdb");
    fs::write(&path,json!({"sheets":[{"name":"Items","props":{},"columns":[{"name":"id","typeStr":"0"},{"name":"tint","typeStr":"11"}],"lines":[{"id":"A","tint":16711680},{"id":"A","tint":0},{"id":"B","tint":255}]}]}).to_string()).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("game.ldtk"));
    p.sync_external_enums(path).unwrap();
    let values = &p.json["defs"]["externalEnums"][0]["values"];
    assert_eq!(values.as_array().unwrap().len(), 2);
    assert_eq!(values[0]["color"], 16711680);
    assert_eq!(values[1]["color"], 255);
}

#[test]
fn preview_154_castledb_color_reload_preserves_unaffected_external_level() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.ldtk");
    let source = directory.path().join("items.cdb");
    let write_source = |red_item_color| {
        let db = json!({"sheets": [{
            "name": "Items",
            "props": {},
            "columns": [{"name": "id", "typeStr": "0"}, {"name": "tint", "typeStr": "11"}],
            "lines": [{"id": "Red", "tint": red_item_color}, {"id": "Blue", "tint": 0x0000FF}]
        }]});
        fs::write(&source, db.to_string()).unwrap();
    };
    write_source(0xFF0000);
    let mut project = Project::new();
    project.path = Some(path.clone());
    project.sync_external_enums(&source).unwrap();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let actor = project.add_entity_def("Actor", 16, 16).unwrap();
    let items = project
        .add_field_definition(Some(actor), "Items", "ExternEnum.Items", true)
        .unwrap();
    let changed = project.level_keys()[0];
    let unchanged = project.add_level(None, "Other", 128, 128).unwrap();
    for (level, item) in [(changed, "Red"), (unchanged, "Blue")] {
        project.add_entity(level, 0, actor, 16, 16).unwrap();
        let iid = project.layer(level, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        project
            .set_field_value(level, Some(&iid), items, json!([item]))
            .unwrap();
    }
    project.set_external_levels(true).unwrap();
    project.save(&path).unwrap();
    let unchanged_file = directory.path().join(
        project.json["levels"][unchanged.level]["externalRelPath"]
            .as_str()
            .unwrap(),
    );
    let mut original_bytes = fs::read(&unchanged_file).unwrap();
    original_bytes.extend_from_slice(b" \t\n");
    fs::write(&unchanged_file, &original_bytes).unwrap();
    let original_time = fs::metadata(&unchanged_file).unwrap().modified().unwrap();

    let mut project = Project::load(&path).unwrap();
    project.migrate_to_current().unwrap();
    write_source(0x00FF00);
    assert!(project.sync_external_enums(&source).unwrap().applied);
    project.save(&path).unwrap();

    assert!(fs::read(&unchanged_file).unwrap() == original_bytes);
    assert_eq!(
        fs::metadata(&unchanged_file).unwrap().modified().unwrap(),
        original_time
    );
    let reopened = Project::load(&path).unwrap();
    for (level, color) in [(changed, "#00FF00"), (unchanged, "#0000FF")] {
        assert_eq!(
            reopened.layer(level, 0).unwrap()["entityInstances"][0]["__smartColor"],
            color
        );
    }
}

#[test]
fn removed_enum_definitions_require_resolution_and_explicit_removal_tidies_fields() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state.json");
    fs::write(&path, r#"{"State":["Idle","Run"],"Other":["A","B"]}"#).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("game.ldtk"));
    p.sync_external_enums(&path).unwrap();
    let id = p.json["defs"]["externalEnums"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["identifier"] == "State")
        .unwrap()["uid"]
        .as_i64()
        .unwrap();
    p.json["defs"]["levelFields"] = json!([{"uid":77,"identifier":"state","type":format!("F_Enum({id})"),"__type":"ExternEnum.State","defaultOverride":{"id":"V_String","params":["Idle"]}}]);
    p.json["levels"][0]["fieldInstances"] =
        json!([{"defUid":77,"__type":"ExternEnum.State","__value":"Idle"}]);
    fs::write(&path, r#"{"Other":["A","B"]}"#).unwrap();
    let report = p.sync_external_enums(&path).unwrap();
    assert!(report.requires_confirmation);
    p.apply_external_enum_sync(&path, &std::collections::BTreeMap::new(), true)
        .unwrap();
    assert_eq!(p.json["defs"]["levelFields"], json!([]));
    assert_eq!(p.json["levels"][0]["fieldInstances"], json!([]));
}
#[test]
fn identifier_style_and_duplicate_suffixes_follow_original_cleanup() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("values.json");
    fs::write(&path, r#"{" 123_state":["a b","a__b","a b","123go","_"]}"#).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.sync_external_enums(path).unwrap();
    let e = &p.json["defs"]["externalEnums"][0];
    assert_eq!(e["identifier"], "_State");
    assert_eq!(
        e["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["A_b", "A_b2", "A_b3", "_Go", "Unnamed"]
    );
}
#[test]
fn simultaneous_enum_and_value_renames_preserve_uid_and_references() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state.json");
    fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.sync_external_enums(&path).unwrap();
    let uid = p.json["defs"]["externalEnums"][0]["uid"].clone();
    p.json["levels"][0]["fieldInstances"] = json!([{"__type":"ExternEnum.State","__value":"Idle","realEditorValues":[{"id":"V_String","params":["Idle"]}]}]);
    fs::write(&path, r#"{"Status":["Waiting","Run"]}"#).unwrap();
    let renames = std::collections::BTreeMap::from([
        ("State".into(), "Status".into()),
        ("State.Idle".into(), "Waiting".into()),
    ]);
    let report = p.apply_external_enum_sync(path, &renames, false).unwrap();
    assert!(report.applied);
    assert_eq!(p.json["defs"]["externalEnums"][0]["uid"], uid);
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["__type"],
        "ExternEnum.Status"
    );
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["__value"],
        "Waiting"
    );
}
#[test]
fn enum_rename_cannot_merge_two_definition_uids_and_orphan_fields() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state.json");
    fs::write(&path, r#"{"State":["Idle","Run"],"Other":["A","B"]}"#).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.sync_external_enums(&path).unwrap();
    let before = p.json.clone();
    fs::write(&path, r#"{"Other":["A","B"]}"#).unwrap();
    assert!(
        p.apply_external_enum_sync(
            &path,
            &std::collections::BTreeMap::from([("State".into(), "Other".into())]),
            false
        )
        .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn external_enum_resolutions_propagate_biomes_tile_tags_and_canonical_null_wrappers() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state.json");
    fs::write(&path, r#"{"State":["Idle","Run"]}"#).unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.sync_external_enums(&path).unwrap();
    let uid = p.json["defs"]["externalEnums"][0]["uid"].as_i64().unwrap();
    p.json["defs"]["levelFields"] = json!([{"uid":99,"identifier":"State","type":format!("F_Enum({uid})"),"__type":"ExternEnum.State","isArray":false,"canBeNull":true,"defaultOverride":null}]);
    p.json["levels"][0]["fieldInstances"] = json!([{"defUid":99,"__type":"ExternEnum.State","__value":"Idle","realEditorValues":[{"id":"V_String","params":["Idle"]}]}]);
    p.json["defs"]["layers"] = json!([{"uid":2,"type":"IntGrid","biomeFieldUid":99,"autoRuleGroups":[{"uid":3,"requiredBiomeValues":["Idle"],"rules":[]}]}]);
    p.json["defs"]["tilesets"] = json!([{"uid":4,"tagsSourceEnumUid":uid,"enumTags":[{"enumValueId":"Idle","tileIds":[1,2]}]}]);
    fs::write(&path, r#"{"State":["Jump","Run"]}"#).unwrap();
    p.apply_external_enum_sync(
        &path,
        &std::collections::BTreeMap::from([("State.Idle".into(), "Jump".into())]),
        false,
    )
    .unwrap();
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][0]["requiredBiomeValues"],
        json!(["Jump"])
    );
    assert_eq!(
        p.json["defs"]["tilesets"][0]["enumTags"][0]["enumValueId"],
        "Jump"
    );
    fs::write(&path, r#"{"State":["Run","Other"]}"#).unwrap();
    p.apply_external_enum_sync(path, &std::collections::BTreeMap::new(), true)
        .unwrap();
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["realEditorValues"],
        json!([null])
    );
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][0]["requiredBiomeValues"],
        json!([])
    );
    assert_eq!(p.json["defs"]["tilesets"][0]["enumTags"], json!([]));
}
#[test]
fn removing_external_source_drops_all_its_enums_and_dependent_fields_only() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("states.json");
    std::fs::write(&path, r#"{"State":["Idle"],"Team":["Red"]}"#).unwrap();
    let mut p = Project::new();
    p.sync_external_enums(&path).unwrap();
    let uid = p.add_entity_def("Actor", 16, 16).unwrap();
    p.add_field_definition(Some(uid), "State", "ExternEnum.State", false)
        .unwrap();
    let rel = p.json["defs"]["externalEnums"][0]["externalRelPath"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(p.remove_external_enum_source(&rel).unwrap(), 2);
    assert!(
        p.json["defs"]["externalEnums"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        p.json["defs"]["entities"][0]["fieldDefs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(path.exists());
}

#[test]
fn preview_154_castledb_extracts_unique_ids_from_first_level_lists() {
    let source = json!({"sheets":[
        {"name":"Actors","props":{},"columns":[{"name":"id","typeStr":"0"}],"lines":[
            {"id":"A","items":[{"id":"Sword","tint":16711680},{"id":"Shield","tint":255}]},
            {"id":"B","items":[{"id":"Sword","tint":1},{"id":"Bow","tint":2}]}]},
        {"name":"Actors@items","props":{},"columns":[{"name":"id","typeStr":"0"},{"name":"tint","typeStr":"11"}],"lines":[]}
    ]}).to_string();
    let parsed = ldtk_core::enum_sync::parse_external_enums("cdb", &source).unwrap();
    let list = parsed
        .iter()
        .find(|e| e.identifier == "Actors@items")
        .unwrap();
    assert_eq!(
        list.values
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Sword", "Shield", "Bow"]
    );
    assert_eq!(list.values[0]["color"], 16711680);
}

#[test]
fn preview_154_castledb_list_icons_preserve_tileset_grid_and_unique_name() {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = dir.path().join("db");
    fs::create_dir(&source_dir).unwrap();
    let source = source_dir.join("game.cdb");
    let icon = source_dir.join("hero.png");
    image::RgbaImage::new(35, 34).save(&icon).unwrap();
    let mut project = Project::new();
    project.path = Some(dir.path().join("game.ldtk"));
    project.json["identifierStyle"] = json!("Uppercase");
    let existing = project
        .add_tileset(std::path::Path::new("GAME_CDB_HERO_PNG.png"), 16, 16, 16)
        .unwrap();
    fs::write(
        &source,
        json!({"sheets":[
            {"name":"Units","columns":[{"name":"id","typeStr":"0"}],"lines":[
                {"id":"Mage","spells":[{"name":"Bolt","icon":{"file":"hero.png","size":8,"x":1,"y":1,"width":2}}]}]},
            {"name":"Units@spells","props":{"displayIcon":"icon"},"columns":[
                {"name":"name","typeStr":"0"},{"name":"icon","typeStr":"16"}],"lines":[]}
        ]}).to_string(),
    )
    .unwrap();
    project.sync_external_enums(&source).unwrap();

    let list = project.json["defs"]["externalEnums"]
        .as_array()
        .unwrap()
        .iter()
        .find(|enu| enu["identifier"] == "UNITS_SPELLS")
        .unwrap();
    let uid = list["iconTilesetUid"].as_i64().unwrap();
    let atlas = project.tileset(uid).unwrap();
    assert_eq!(atlas["identifier"], "GAME_CDB_HERO_PNG2");
    assert_eq!(
        project.tileset(existing).unwrap()["identifier"],
        "GAME_CDB_HERO_PNG"
    );
    assert_eq!(atlas["relPath"], "db/hero.png");
    assert_eq!(atlas["tileGridSize"], 16);
    assert_eq!(
        (atlas["__cWid"].as_i64(), atlas["__cHei"].as_i64()),
        (Some(3), Some(3))
    );
    assert_eq!(atlas["tags"], json!(["CastleDB"]));
    assert_eq!(
        list["values"][0]["tileRect"],
        json!({"tilesetUid":uid,"x":8,"y":8,"w":16,"h":8})
    );

    project.sync_external_enums(&source).unwrap();
    assert_eq!(
        project.json["defs"]["tilesets"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        project.tileset(uid).unwrap()["identifier"],
        "GAME_CDB_HERO_PNG2"
    );
}

#[test]
fn preview_154_castledb_nested_list_sync_updates_and_rejects_missing_parent_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("data.cdb");
    let make = |color| {
        json!({"sheets":[
        {"name":"Actors","props":{},"columns":[{"name":"id","typeStr":"0"}],"lines":[{"id":"A","items":[{"id":"Sword","tint":color}]}]},
        {"name":"Actors@items","props":{},"columns":[{"name":"id","typeStr":"0"},{"name":"tint","typeStr":"11"}],"lines":[]}
    ]}).to_string()
    };
    fs::write(&source, make(12)).unwrap();
    let mut p = Project::new();
    p.path = Some(dir.path().join("game.ldtk"));
    p.sync_external_enums(&source).unwrap();
    fs::write(&source, make(42)).unwrap();
    p.sync_external_enums(&source).unwrap();
    let defs = p.json["defs"]["externalEnums"].as_array().unwrap();
    assert!(
        defs.iter()
            .any(|e| e["values"][0]["id"] == "Sword" && e["values"][0]["color"] == 42)
    );
    let before = p.json.clone();
    fs::write(&source, json!({"sheets":[{"name":"Missing@items","columns":[{"name":"id","typeStr":"0"}],"props":{},"lines":[]}]}).to_string()).unwrap();
    assert!(p.sync_external_enums(&source).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn preview_154_castledb_level_only_values_require_rename_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.cdb");
    let source = |id| {
        json!({"sheets":[{"name":"Items","props":{},"columns":[{"name":"id","typeStr":"0"}],"lines":[{"id":id}]}]}).to_string()
    };
    fs::write(&path, source("Old")).unwrap();
    let mut p = Project::new();
    p.path = Some(dir.path().join("game.ldtk"));
    p.sync_external_enums(&path).unwrap();
    let uid = p
        .add_field_definition(None, "Item", "ExternEnum.Items", false)
        .unwrap();
    let key = p.level_keys()[0];
    p.set_field_editor_value(key, None, uid, json!("Old"))
        .unwrap();
    fs::write(&path, source("New")).unwrap();
    let before = p.json.clone();
    let report = p.sync_external_enums(&path).unwrap();
    assert!(report.requires_confirmation && !report.applied);
    assert_eq!(p.json, before);
    p.apply_external_enum_sync(
        &path,
        &std::collections::BTreeMap::from([("Items.Old".into(), "New".into())]),
        false,
    )
    .unwrap();
    assert_eq!(p.level(key).unwrap()["fieldInstances"][0]["__value"], "New");
}

#[test]
fn external_appearance_survives_sync_without_allowing_id_or_castledb_edits() {
    let temp = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.path = Some(temp.path().join("game.ldtk"));
    let source = temp.path().join("state.json");
    fs::write(&source, r#"{"State":["Idle","Run"]}"#).unwrap();
    project.sync_external_enums(&source).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("icons.png"), 16, 32, 32)
        .unwrap();
    let uid = project.json["defs"]["externalEnums"][0]["uid"]
        .as_i64()
        .unwrap();
    let icon = json!({"tilesetUid":atlas,"x":16,"y":0,"w":16,"h":16});
    let mut values = project.json["defs"]["externalEnums"][0]["values"]
        .as_array()
        .unwrap()
        .clone();
    values[0]["tileRect"] = icon.clone();
    values[0]["color"] = json!(0x44_88_aa);
    project
        .update_external_enum_appearance(uid, Some(atlas), &values)
        .unwrap();

    fs::write(&source, r#"{"State":["Run","Idle","Jump"]}"#).unwrap();
    project.sync_external_enums(&source).unwrap();
    let definition = &project.json["defs"]["externalEnums"][0];
    assert_eq!(definition["iconTilesetUid"], atlas);
    assert_eq!(definition["values"][1]["id"], "Idle");
    assert_eq!(definition["values"][1]["tileRect"], icon);
    assert_eq!(definition["values"][1]["color"], 0x44_88_aa);
    let mut values = definition["values"].as_array().unwrap().clone();
    values[0]["id"] = json!("Renamed");
    let before = project.json.clone();
    assert!(
        project
            .update_external_enum_appearance(uid, Some(atlas), &values)
            .is_err()
    );
    assert_eq!(project.json, before);

    project.json["defs"]["externalEnums"][0]["externalRelPath"] = json!("state.CDB");
    let before = project.json.clone();
    let values = before["defs"]["externalEnums"][0]["values"]
        .as_array()
        .unwrap();
    assert!(
        project
            .update_external_enum_appearance(uid, None, values)
            .is_err()
    );
    assert_eq!(project.json, before);
}
