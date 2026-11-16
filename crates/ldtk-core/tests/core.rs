use ldtk_core::{History, LevelKey, Project};
use serde_json::{Value, json};
use std::fs;

const KEY: LevelKey = LevelKey {
    world: None,
    level: 0,
};

fn project() -> Project {
    Project::from_json(json!({
        "jsonVersion":"1.5.3", "extension":{"keep":[1,{"x":true}]},
        "defs":{"layers":[{"uid":1,"type":"IntGrid","intGridValues":(1..=31).chain([77]).map(|value|json!({"value":value})).collect::<Vec<_>>()},{"uid":2,"type":"Tiles"},{"uid":3,"type":"Entities"}],
        "tilesets":[{"uid":9,"relPath":"atlas.png","pxWid":32,"pxHei":32,"tileGridSize":16}],
        "entities":[{"uid":4,"identifier":"Hero","width":16,"height":16,"pivotX":0.5,"pivotY":1.0,"color":"#ff0000","tags":["actor"],"maxCount":1,"limitScope":"PerLevel","limitBehavior":"PreventAdding","fieldDefs":[{"uid":5,"identifier":"Health","__type":"Int","type":"F_Int","canBeNull":false,"isArray":false,"defaultOverride":{"id":"V_Int","params":[7]}}]}]},
        "levels":[{"identifier":"Level","iid":"l","pxWid":32,"pxHei":32,"worldX":10,"worldY":20,"externalRelPath":null,"layerInstances":[
            {"__type":"IntGrid","layerDefUid":1,"__cWid":2,"__cHei":2,"__gridSize":16,"intGridCsv":[0,0,0,0],"future":42},
            {"__type":"Tiles","layerDefUid":2,"__cWid":2,"__cHei":2,"__gridSize":16,"__tilesetDefUid":9,"gridTiles":[]},
            {"__type":"Entities","layerDefUid":3,"__cWid":2,"__cHei":2,"__gridSize":16,"entityInstances":[]}
        ]}],"worlds":[]
    })).unwrap()
}

#[test]
fn unknown_fields_round_trip_and_save_path_changes_only_after_success() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.ldtk");
    let mut p = project();
    let expected = p.json.clone();
    p.save(&path).unwrap();
    assert_eq!(Project::load(&path).unwrap().json, expected);
    let saved_path = p.path.clone();
    assert!(p.save(temp.path().join("missing/map.ldtk")).is_err());
    assert_eq!(p.path, saved_path);
    assert_eq!(Project::load(&path).unwrap().json, expected);
}

#[test]
fn external_levels_stay_external_after_edit_save_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.ldtk");
    let mut raw = project().json;
    let mut full = raw["levels"][0].clone();
    full["futureLevel"] = json!({"preserve":true});
    raw["levels"][0]["externalRelPath"] = json!("level.ldtkl");
    raw["levels"][0]["layerInstances"] = Value::Null;
    raw["externalLevels"] = json!(true);
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    fs::write(
        temp.path().join("level.ldtkl"),
        serde_json::to_vec(&full).unwrap(),
    )
    .unwrap();
    let mut p = Project::load(&path).unwrap();
    let mut h = History::default();
    h.begin(&p, KEY);
    assert!(p.paint_int_grid(KEY, 0, 1, 1, 8).unwrap());
    assert!(h.commit(&p));
    assert!(h.undo(&mut p));
    assert_eq!(p.layer(KEY, 0).unwrap()["intGridCsv"][3], 0);
    assert!(h.redo(&mut p));
    p.save(&path).unwrap();
    // ProjectSaver regenerates owned external filenames from the project/level identifiers.
    raw["levels"][0]["externalRelPath"] = json!("map/Level.ldtkl");
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(temp.path().join("level.ldtkl")).unwrap())
            .unwrap(),
        full
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
        raw
    );
    let reloaded = Project::load(&path).unwrap();
    assert_eq!(
        reloaded.level(KEY).unwrap()["futureLevel"],
        full["futureLevel"]
    );
    assert_eq!(reloaded.layer(KEY, 0).unwrap()["intGridCsv"][3], 8);
}

#[test]
fn world_and_legacy_level_access_are_both_lossless() {
    let mut p = project();
    let level = p.json["levels"][0].clone();
    p.json["worlds"] = json!([{"identifier":"World","levels":[level]}]);
    let world_key = LevelKey {
        world: Some(0),
        level: 0,
    };
    assert_eq!(p.level_keys(), vec![KEY, world_key]);
    p.layer_mut(world_key, 0).unwrap()["future"] = json!(99);
    assert_eq!(p.layer(KEY, 0).unwrap()["future"], 42);
    assert_eq!(p.layer(world_key, 0).unwrap()["future"], 99);
    assert!(p.layer_def(1).is_some());
    assert!(p.tileset(9).is_some());
}

#[test]
fn int_grid_checks_bounds_and_rejects_malformed_cells_without_mutating() {
    let mut p = project();
    assert!(p.paint_int_grid(KEY, 0, 1, 0, 2).unwrap());
    assert!(!p.paint_int_grid(KEY, 0, 1, 0, 2).unwrap());
    assert!(!p.paint_int_grid(KEY, 0, -1, 0, 2).unwrap());
    assert!(!p.paint_int_grid(KEY, 0, 2, 0, 2).unwrap());
    assert_eq!(p.layer(KEY, 0).unwrap()["future"], 42);
    p.layer_mut(KEY, 0).unwrap()["intGridCsv"] = json!([0]);
    let before = p.json.clone();
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 2).is_err());
    assert_eq!(p.json, before);
    p.layer_mut(KEY, 0).unwrap()["__cWid"] = json!(-2);
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 2).is_err());
}

#[test]
fn tile_paint_replaces_one_cell_and_preserves_other_tile_metadata() {
    let mut p = project();
    p.layer_mut(KEY, 1).unwrap()["gridTiles"] =
        json!([{"px":[16,16],"t":1,"src":[16,0],"f":0,"future":true}]);
    assert!(p.paint_tile(KEY, 1, 0, 0, 2, [0, 16], 3, false).unwrap());
    assert!(!p.paint_tile(KEY, 1, 0, 0, 2, [0, 16], 3, false).unwrap());
    assert_eq!(p.layer(KEY, 1).unwrap()["gridTiles"][0]["future"], true);
    assert!(p.paint_tile(KEY, 1, 0, 0, 3, [16, 16], 0, false).unwrap());
    assert_eq!(
        p.layer(KEY, 1).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(p.paint_tile(KEY, 1, 0, 0, 0, [0, 0], 0, true).unwrap());
    assert_eq!(
        p.layer(KEY, 1).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let before = p.json.clone();
    assert!(!p.paint_tile(KEY, 1, 8, 8, 0, [0, 0], 0, false).unwrap());
    assert!(p.paint_tile(KEY, 1, 0, 0, -2, [0, 0], 0, false).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn entities_use_definition_defaults_pivot_fields_and_limits() {
    let mut p = project();
    assert!(p.add_entity(KEY, 2, 4, 16, 16).unwrap());
    let entity = &p.layer(KEY, 2).unwrap()["entityInstances"][0];
    assert_eq!(entity["__identifier"], "Hero");
    assert_eq!(entity["__grid"], json!([1, 0]));
    assert_eq!(entity["__pivot"], json!([0.5, 1]));
    assert_eq!(entity["px"], json!([16, 16]));
    assert_eq!(entity["fieldInstances"][0]["__value"], 7);
    assert_eq!(entity["fieldInstances"][0]["realEditorValues"], json!([]));
    assert!(uuid::Uuid::parse_str(entity["iid"].as_str().unwrap()).is_ok());
    assert!(!p.add_entity(KEY, 2, 4, 0, 0).unwrap());
    assert!(!p.add_entity(KEY, 2, 4, -1, 0).unwrap());
    assert!(p.add_entity(KEY, 2, 999, 0, 0).is_err());
}

#[test]
fn stroke_history_groups_changes_and_discards_redo_on_new_commit() {
    let mut p = project();
    let mut h = History::default();
    h.begin(&p, KEY);
    assert!(!h.commit(&p));
    assert!(!h.can_undo());
    h.begin(&p, KEY);
    p.paint_int_grid(KEY, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(KEY, 0, 1, 0, 1).unwrap();
    assert!(h.commit(&p));
    assert!(h.undo(&mut p));
    assert_eq!(p.layer(KEY, 0).unwrap()["intGridCsv"], json!([0, 0, 0, 0]));
    assert!(!h.can_undo());
    assert!(h.can_redo());
    h.begin(&p, KEY);
    p.paint_int_grid(KEY, 0, 0, 1, 9).unwrap();
    assert!(h.commit(&p));
    assert!(!h.can_redo());
}

#[test]
fn save_as_rebases_relative_assets_in_new_directory_without_touching_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.ldtk");
    let mut p = project();
    p.save(&path).unwrap();
    fs::create_dir(temp.path().join("other")).unwrap();
    let before = fs::read(&path).unwrap();
    p.save(temp.path().join("other/map.ldtk")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(temp.path().join("other/map.ldtk").exists());
    assert!(
        p.json["defs"]["tilesets"][0]["relPath"]
            .as_str()
            .unwrap()
            .starts_with("../")
    );
}

#[test]
fn empty_fixture_round_trips_and_new_projects_have_unique_ids() {
    let value: Value = serde_json::from_str(include_str!("fixtures/empty.ldtk")).unwrap();
    assert_eq!(Project::from_json(value.clone()).unwrap().json, value);
    let a = Project::new();
    let b = Project::new();
    assert_ne!(a.json["iid"], b.json["iid"]);
    assert_eq!(a.level_keys().len(), 1);
    assert!(Project::from_json(json!([])).is_err());
    assert!(Project::from_json(json!({"levels":"broken"})).is_err());
}

#[test]
fn missing_external_document_is_an_error_before_overwriting_project() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.ldtk");
    let mut raw = project().json;
    raw["levels"][0]["externalRelPath"] = json!("missing.ldtkl");
    raw["levels"][0]["layerInstances"] = Value::Null;
    fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(Project::load(&path).is_err());
    let mut p = Project::from_json(raw.clone()).unwrap();
    assert!(p.save(&path).is_err());
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
        raw
    );
}

#[test]
fn history_keeps_source_timeline_window_without_changing_other_levels() {
    let mut p = project();
    p.json["defs"]["layers"][0]["intGridValues"]
        .as_array_mut()
        .unwrap()
        .extend((32..=40).map(|value| json!({"value": value})));
    let mut second = p.json["levels"][0].clone();
    second["iid"] = json!("second-level");
    p.json["levels"].as_array_mut().unwrap().push(second);
    let mut h = History::default();
    for value in 1..=40 {
        h.begin(&p, KEY);
        p.paint_int_grid(KEY, 0, 0, 0, value).unwrap();
        assert!(h.commit(&p));
    }
    p.json["extension"]["later"] = json!(true);
    let second = LevelKey {
        world: None,
        level: 1,
    };
    p.paint_int_grid(second, 0, 1, 1, 77).unwrap();
    // Forty states include the baseline. Overflow drops eleven states at once.
    for _ in 0..29 {
        assert!(h.undo(&mut p));
    }
    assert!(!h.undo(&mut p));
    assert_eq!(p.layer(KEY, 0).unwrap()["intGridCsv"][0], 11);
    assert_eq!(p.layer(second, 0).unwrap()["intGridCsv"][3], 77);
    assert_eq!(p.json["extension"]["later"], true);
}

#[test]
fn wrong_layer_types_and_malformed_tile_lists_are_rejected() {
    let mut p = project();
    assert!(p.paint_int_grid(KEY, 1, 0, 0, 1).is_err());
    assert!(p.paint_tile(KEY, 0, 0, 0, 1, [0, 0], 0, false).is_err());
    assert!(p.add_entity(KEY, 0, 4, 0, 0).is_err());
    p.layer_mut(KEY, 1).unwrap()["gridTiles"] = json!([{"px":"bad"}]);
    let before = p.json.clone();
    assert!(p.paint_tile(KEY, 1, 0, 0, 1, [0, 0], 0, false).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn external_save_preflights_all_files_before_replacing_any_document() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.ldtk");
    let mut p = project();
    let full = p.json["levels"][0].clone();
    p.json["levels"][0]["externalRelPath"] = json!("a.ldtkl");
    p.json["levels"][0]["layerInstances"] = Value::Null;
    let mut second = p.json["levels"][0].clone();
    second["externalRelPath"] = json!("missing/b.ldtkl");
    p.json["levels"].as_array_mut().unwrap().push(second);
    p.external_levels.insert("a.ldtkl".into(), full.clone());
    p.external_levels.insert("missing/b.ldtkl".into(), full);
    fs::write(&path, b"original-root").unwrap();
    fs::write(temp.path().join("a.ldtkl"), b"original-level").unwrap();
    assert!(p.save(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"original-root");
    assert_eq!(
        fs::read(temp.path().join("a.ldtkl")).unwrap(),
        b"original-level"
    );
    assert!(p.path.is_none());
}

#[test]
fn int_grid_with_auto_rules_rebuilds_generated_tiles_and_rolls_back_rule_errors() {
    let mut p = project();
    p.json["defs"]["layers"][0]["gridSize"] = json!(16);
    p.json["defs"]["layers"][0]["tilesetDefUid"] = json!(9);
    p.json["defs"]["layers"][0]["autoRuleGroups"] = json!([{"uid":100,"active":true,"rules":[{"uid":101,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1}]}]);
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 1).unwrap());
    assert_eq!(
        p.layer(KEY, 0).unwrap()["autoLayerTiles"][0]["px"],
        json!([0, 0])
    );
    let before = p.json.clone();
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["xModulo"] = json!(0);
    let broken = p.json.clone();
    assert!(p.paint_int_grid(KEY, 0, 1, 0, 1).is_err());
    assert_eq!(p.json, broken);
    assert_eq!(
        p.layer(KEY, 0).unwrap()["intGridCsv"],
        before["levels"][0]["layerInstances"][0]["intGridCsv"]
    );
}

#[test]
fn entity_move_limit_preserves_identity_and_existing_custom_values() {
    let mut p = project();
    p.json["defs"]["entities"][0]["limitBehavior"] = json!("MoveLastOne");
    assert!(p.add_entity(KEY, 2, 4, 0, 0).unwrap());
    let entity = &mut p.layer_mut(KEY, 2).unwrap()["entityInstances"][0];
    let iid = entity["iid"].clone();
    entity["future"] = json!("keep");
    entity["fieldInstances"][0]["realEditorValues"] = json!([{"id":"V_Int","params":[99]}]);
    assert!(p.add_entity(KEY, 2, 4, 16, 16).unwrap());
    let entities = p.layer(KEY, 2).unwrap()["entityInstances"]
        .as_array()
        .unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["iid"], iid);
    assert_eq!(entities[0]["future"], "keep");
    assert_eq!(entities[0]["fieldInstances"][0]["__value"], 99);
    assert_eq!(entities[0]["px"], json!([16, 16]));
}

#[test]
fn entity_tag_filters_and_world_count_limits_are_honored() {
    let mut p = project();
    p.json["defs"]["layers"][2]["requiredTags"] = json!(["enemy"]);
    assert!(!p.add_entity(KEY, 2, 4, 0, 0).unwrap());
    p.json["defs"]["layers"][2]["requiredTags"] = json!(["actor"]);
    p.json["defs"]["entities"][0]["limitScope"] = json!("PerWorld");
    let second = p.json["levels"][0].clone();
    p.json["levels"].as_array_mut().unwrap().push(second);
    assert!(p.add_entity(KEY, 2, 4, 0, 0).unwrap());
    let second_key = LevelKey {
        world: None,
        level: 1,
    };
    assert!(!p.add_entity(second_key, 2, 4, 0, 0).unwrap());
    p.json["defs"]["entities"][0]["limitBehavior"] = json!("MoveLastOne");
    let iid = p.layer(KEY, 2).unwrap()["entityInstances"][0]["iid"].clone();
    assert!(p.add_entity(second_key, 2, 4, 0, 0).unwrap());
    assert_eq!(p.layer(KEY, 2).unwrap()["entityInstances"], json!([]));
    assert_eq!(
        p.layer(second_key, 2).unwrap()["entityInstances"][0]["iid"],
        iid
    );
}

#[test]
fn minify_setting_is_respected_for_written_project() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = project();
    p.json["minifyJson"] = json!(true);
    let path = temp.path().join("map.ldtk");
    p.save(&path).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
    assert_eq!(Project::load(path).unwrap().json, p.json);
}

#[test]
fn edited_external_summary_is_synced_without_copying_unknown_fields_or_layers() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = project();
    let mut external = p.json["levels"][0].clone();
    external["identifier"] = json!("Renamed");
    external["worldX"] = json!(256);
    external["externalFuture"] = json!({"keep":true});
    p.json["levels"][0]["externalRelPath"] = json!("level.ldtkl");
    p.json["levels"][0]["layerInstances"] = Value::Null;
    p.json["levels"][0]["rootFuture"] = json!(55);
    p.external_levels.insert("level.ldtkl".into(), external);
    let path = temp.path().join("map.ldtk");
    p.save(&path).unwrap();
    let raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(raw["levels"][0]["identifier"], "Renamed");
    assert_eq!(raw["levels"][0]["worldX"], 256);
    assert_eq!(raw["levels"][0]["rootFuture"], 55);
    assert!(raw["levels"][0].get("externalFuture").is_none());
    assert!(raw["levels"][0]["layerInstances"].is_null());
    assert_eq!(p.json, raw);
    assert_eq!(
        Project::load(path).unwrap().level(KEY).unwrap()["externalFuture"],
        json!({"keep":true})
    );
}

#[test]
fn int_grid_rejects_malformed_touched_cell_without_scanning_unrelated_cells() {
    let mut p = project();
    p.layer_mut(KEY, 0).unwrap()["intGridCsv"] = json!([0, 0, "bad", 0]);
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 1).unwrap());
    let before = p.json.clone();
    assert!(p.paint_int_grid(KEY, 0, 0, 1, 1).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn int_grid_auto_layer_source_rebuilds_dependent_generated_tiles() {
    let mut p = project();
    p.json["defs"]["layers"].as_array_mut().unwrap().push(json!({"uid":99,"type":"AutoLayer","gridSize":16,"tilesetDefUid":9,"autoSourceLayerDefUid":1,"autoRuleGroups":[{"uid":100,"active":true,"rules":[{"uid":101,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1}]}]}));
    p.json["levels"][0]["layerInstances"].as_array_mut().unwrap().push(json!({"__type":"AutoLayer","layerDefUid":99,"__cWid":2,"__cHei":2,"__gridSize":16,"autoLayerTiles":[]}));
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 1).unwrap());
    assert_eq!(p.layer(KEY, 3).unwrap()["autoLayerTiles"][0]["t"], 0);
}

#[test]
fn tile_paint_rejects_out_of_atlas_ids_and_mismatched_sources() {
    let mut p = project();
    let before = p.json.clone();
    assert!(p.paint_tile(KEY, 1, 0, 0, 5, [0, 0], 0, false).is_err());
    assert!(p.paint_tile(KEY, 1, 0, 0, 0, [16, 16], 0, false).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn save_as_detects_relative_file_path_fields_even_without_tilesets() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.json["levels"][0]["fieldInstances"] = json!([{"__type":"Array<FilePath>","__value":["data/config.json"],"realEditorValues":[{"id":"V_String","params":["data/config.json"]}]}]);
    p.save(temp.path().join("map.ldtk")).unwrap();
    fs::create_dir(temp.path().join("other")).unwrap();
    p.save(temp.path().join("other/map.ldtk")).unwrap();
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["__value"][0],
        "../data/config.json"
    );
    assert_eq!(
        p.json["levels"][0]["fieldInstances"][0]["realEditorValues"][0]["params"][0],
        "../data/config.json"
    );
}

#[test]
fn same_directory_save_as_with_external_levels_does_not_modify_the_source_project() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("A.ldtk");
    let target = temp.path().join("B.ldtk");
    let external_path = temp.path().join("level.ldtkl");
    let mut root = project().json;
    let external = root["levels"][0].clone();
    root["levels"][0]["externalRelPath"] = json!("level.ldtkl");
    root["levels"][0]["layerInstances"] = Value::Null;
    fs::write(&source, serde_json::to_vec(&root).unwrap()).unwrap();
    fs::write(&external_path, serde_json::to_vec(&external).unwrap()).unwrap();
    let mut p = Project::load(&source).unwrap();
    p.paint_int_grid(KEY, 0, 0, 0, 8).unwrap();
    let source_bytes = fs::read(&source).unwrap();
    let external_bytes = fs::read(&external_path).unwrap();
    p.save(&target).unwrap();
    assert_eq!(p.path.as_deref(), Some(target.as_path()));
    assert!(
        p.json["levels"][0]["externalRelPath"]
            .as_str()
            .unwrap()
            .starts_with("B/")
    );
    assert_eq!(
        Project::load(&target).unwrap().layer(KEY, 0).unwrap()["intGridCsv"][0],
        8
    );
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&external_path).unwrap(), external_bytes);
    assert!(target.exists());
    assert_eq!(
        Project::load(&source).unwrap().layer(KEY, 0).unwrap()["intGridCsv"][0],
        0
    );
}

#[test]
fn int_grid_replacement_must_be_defined_in_the_target_layer_palette() {
    let mut p = Project::new();
    p.add_layer("Collision", "IntGrid", 16).unwrap();
    let before = p.json.clone();
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 2).is_err());
    assert_eq!(p.json, before);
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 1).unwrap());
    assert!(p.paint_int_grid(KEY, 0, 0, 0, 0).unwrap());
}
