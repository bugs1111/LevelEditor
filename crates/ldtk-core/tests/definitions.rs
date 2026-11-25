use ldtk_core::{LevelKey, Project};
use serde_json::json;
use std::path::Path;

#[test]
fn removing_tileset_image_keeps_tile_ids_and_allows_reattachment() {
    let mut p = Project::new();
    let layer = p.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    p.assign_tileset(layer, atlas).unwrap();
    let key = p.level_keys()[0];
    p.paint_tile(key, 0, 0, 0, 3, [16, 16], 0, false).unwrap();
    p.update_tileset_definition(
        atlas,
        &json!({"savedSelections":[{"ids":[3],"mode":"Stamp"}]}),
    )
    .unwrap();
    p.remove_tileset_image(atlas).unwrap();
    let def = p.tileset(atlas).unwrap();
    assert_eq!(def["relPath"], serde_json::Value::Null);
    assert_eq!(def["pxWid"], 0);
    assert_eq!(def["__cWid"], 0);
    assert_eq!(def["savedSelections"], json!([]));
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["t"], 3);
    p.update_tileset_definition(atlas, &json!({"relPath":"new.png","pxWid":32,"pxHei":32}))
        .unwrap();
    let tile = &p.layer(key, 0).unwrap()["gridTiles"][0];
    assert_eq!(tile["t"], 3);
    assert_eq!(tile["src"], json!([16, 16]));
}

#[test]
fn rename_entity_tags_can_update_related_filters_atomically() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    let first = p.add_entity_def("First", 16, 16).unwrap();
    let second = p.add_entity_def("Second", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(first), "Target", "EntityRef", false)
        .unwrap();
    p.update_entity_definition(first, &json!({"tags":["enemy", "boss"]}))
        .unwrap();
    p.update_entity_definition(second, &json!({"tags":["enemy"]}))
        .unwrap();
    p.update_layer_properties(
        layer,
        &json!({"requiredTags":["enemy"],"excludedTags":["other"]}),
    )
    .unwrap();
    p.update_field_definition(field, &json!({"allowedRefTags":["enemy"]}))
        .unwrap();
    p.json["defs"]["entities"][1]
        .as_object_mut()
        .unwrap()
        .remove("fieldDefs");
    p.json["defs"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("excludedTags");
    let before = p.json.clone();
    p.rename_definition_tag("entities", first, "enemy", "red foe", false)
        .unwrap();
    assert_eq!(
        p.json["defs"]["entities"][0]["tags"],
        json!(["boss", "red_foe"])
    );
    assert_eq!(p.json["defs"]["entities"][1]["tags"], json!(["enemy"]));
    assert_eq!(
        p.layer_def(layer).unwrap()["requiredTags"],
        json!(["enemy"])
    );
    p.json = before;
    p.rename_definition_tag("entities", first, "enemy", "red foe", true)
        .unwrap();
    assert_eq!(p.json["defs"]["entities"][1]["tags"], json!(["red_foe"]));
    assert_eq!(
        p.layer_def(layer).unwrap()["requiredTags"],
        json!(["red_foe"])
    );
    assert_eq!(
        p.json["defs"]["entities"][0]["fieldDefs"][0]["allowedRefTags"],
        json!(["red_foe"])
    );
    assert!(p.json["defs"]["entities"][1].get("fieldDefs").is_none());
    assert!(p.json["defs"]["layers"][0].get("excludedTags").is_none());
    let before = p.json.clone();
    assert!(
        p.rename_definition_tag("entities", first, "red_foe", "***", true)
            .is_err()
    );
    assert_eq!(p.json, before);
}

#[test]
fn rename_tileset_tags_only_affects_tilesets_and_deduplicates() {
    let mut p = Project::new();
    let first = p.add_tileset(Path::new("first.png"), 16, 32, 32).unwrap();
    let second = p.add_tileset(Path::new("second.png"), 16, 32, 32).unwrap();
    p.update_tileset_definition(first, &json!({"tags":["outdoor","nature"]}))
        .unwrap();
    p.update_tileset_definition(second, &json!({"tags":["outdoor"]}))
        .unwrap();
    p.rename_definition_tag("tilesets", first, "outdoor", "nature", true)
        .unwrap();
    assert_eq!(p.tileset(first).unwrap()["tags"], json!(["nature"]));
    assert_eq!(p.tileset(second).unwrap()["tags"], json!(["nature"]));
}

#[test]
fn creating_layers_instantiates_every_level_with_schema_defaults_and_unique_ids() {
    let mut p = Project::new();
    p.json["levels"][0]["pxWid"] = json!(33);
    let second = p.json["levels"][0].clone();
    p.json["worlds"] = json!([{"levels":[second]}]);
    let uid = p.add_layer("Collision", "IntGrid", 16).unwrap();
    let def = p.layer_def(uid).unwrap();
    assert_eq!(def["identifier"], "Collision");
    assert_eq!(def["__type"], "IntGrid");
    assert_eq!(def["intGridValues"][0]["value"], 1);
    let mut ids = vec![];
    for key in p.level_keys() {
        let layer = p.layer(key, 0).unwrap();
        assert_eq!(layer["layerDefUid"], uid);
        assert_eq!(layer["__cWid"], 3);
        assert_eq!(layer["intGridCsv"].as_array().unwrap().len(), 48);
        assert_eq!(layer["visible"], true);
        ids.push(layer["iid"].clone());
    }
    assert_ne!(ids[0], ids[1]);
    assert_eq!(p.json["nextUid"], uid + 1);
    let before = p.json.clone();
    assert!(p.add_layer("Collision", "Tiles", 16).is_err());
    assert!(p.add_layer("Invalid name", "Tiles", 16).is_err());
    assert!(p.add_layer("Other", "Unknown", 16).is_err());
    assert!(p.add_layer("Other", "Tiles", 0).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn layer_creation_is_transactional_when_a_level_is_malformed() {
    let mut p = Project::new();
    let second = p.json["levels"][0].clone();
    p.json["levels"].as_array_mut().unwrap().push(second);
    p.json["levels"][1]["pxWid"] = json!(0);
    let before = p.json.clone();
    assert!(p.add_layer("Actors", "Entities", 16).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn tilesets_can_be_created_and_assigned_without_losing_tile_metadata() {
    let mut p = Project::new();
    let layer_uid = p.add_layer("Floor", "Tiles", 16).unwrap();
    let tileset_uid = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    assert_ne!(layer_uid, tileset_uid);
    p.assign_tileset(layer_uid, tileset_uid).unwrap();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    assert_eq!(
        p.layer_def(layer_uid).unwrap()["tilesetDefUid"],
        tileset_uid
    );
    assert_eq!(p.layer(key, 0).unwrap()["__tilesetDefUid"], tileset_uid);
    assert_eq!(p.layer(key, 0).unwrap()["__tilesetRelPath"], "atlas.png");
    assert_eq!(p.tileset(tileset_uid).unwrap()["__cWid"], 2);
    p.paint_tile(key, 0, 0, 0, 3, [16, 16], 0, false).unwrap();
    p.layer_mut(key, 0).unwrap()["gridTiles"][0]["future"] = json!(true);
    p.assign_tileset(layer_uid, tileset_uid).unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["future"], true);
    let before = p.json.clone();
    assert!(p.add_tileset(Path::new("bad.png"), 16, 0, 32).is_err());
    assert!(p.assign_tileset(layer_uid, 999).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn tileset_assignment_adopts_source_atlas_grid_size_and_preserves_cells() {
    let mut p = Project::new();
    let layer = p.add_layer("Floor", "Tiles", 16).unwrap();
    let atlas = p.add_tileset(Path::new("atlas.png"), 8, 32, 32).unwrap();
    p.assign_tileset(layer, atlas).unwrap();
    assert_eq!(p.layer_def(layer).unwrap()["gridSize"], 8);
    assert_eq!(p.layer(p.level_keys()[0], 0).unwrap()["__cWid"], 32);
}

#[test]
fn newly_created_entity_definition_can_be_placed_and_round_trips() {
    let mut p = Project::new();
    p.json["defaultPivotX"] = json!(0.25);
    p.json["defaultPivotY"] = json!(0.75);
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 24).unwrap();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    assert!(p.add_entity(key, 0, uid, 32, 32).unwrap());
    let entity = &p.layer(key, 0).unwrap()["entityInstances"][0];
    assert_eq!(entity["__pivot"], json!([0.25, 0.75]));
    assert_eq!(entity["height"], 24);
    p.update_entity_definition(uid, &json!({"pivotX":0.5,"pivotY":0.5}))
        .unwrap();
    assert_eq!(
        p.layer(key, 0).unwrap()["entityInstances"][0]["__pivot"],
        json!([0.5, 0.5])
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("new.ldtk");
    p.save(&path).unwrap();
    assert_eq!(Project::load(path).unwrap().json, p.json);
    let before = p.json.clone();
    assert!(p.add_entity_def("Hero", 16, 16).is_err());
    assert!(p.add_entity_def("Other", 0, 16).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn newly_created_entities_use_zero_pivot_when_legacy_defaults_are_absent() {
    let mut p = Project::new();
    let defaults = p.json.as_object_mut().unwrap();
    defaults.remove("defaultPivotX");
    defaults.remove("defaultPivotY");
    p.add_entity_def("Hero", 16, 16).unwrap();
    assert_eq!(p.json["defs"]["entities"][0]["pivotX"], 0);
    assert_eq!(p.json["defs"]["entities"][0]["pivotY"], 0);
}

#[test]
fn auto_layers_can_be_created_and_assigned_an_atlas() {
    let mut p = Project::new();
    let uid = p.add_layer("Auto", "AutoLayer", 16).unwrap();
    let atlas = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    p.assign_tileset(uid, atlas).unwrap();
    let layer = p.layer(p.level_keys()[0], 0).unwrap();
    assert_eq!(layer["__type"], "AutoLayer");
    assert_eq!(layer["__tilesetDefUid"], atlas);
    assert_eq!(layer["intGridCsv"], json!([]));
}
#[test]
fn assigning_default_atlas_preserves_per_level_overrides_and_remaps_sources() {
    let mut p = Project::new();
    let layer = p.add_layer("Tiles", "Tiles", 16).unwrap();
    let a = p.add_tileset(Path::new("a.png"), 16, 32, 32).unwrap();
    let b = p.add_tileset(Path::new("b.png"), 16, 64, 32).unwrap();
    p.assign_tileset(layer, a).unwrap();
    let key = p.level_keys()[0];
    p.paint_tile(key, 0, 0, 0, 2, [0, 16], 0, false).unwrap();
    p.assign_tileset(layer, b).unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["t"], 2);
    assert_eq!(
        p.layer(key, 0).unwrap()["gridTiles"][0]["src"],
        json!([32, 0])
    );
    p.layer_mut(key, 0).unwrap()["overrideTilesetUid"] = json!(a);
    p.refresh_derived_data().unwrap();
    p.assign_tileset(layer, b).unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["overrideTilesetUid"], a);
    assert_eq!(p.layer(key, 0).unwrap()["__tilesetDefUid"], a);
    assert_eq!(
        p.layer(key, 0).unwrap()["gridTiles"][0]["src"],
        json!([0, 16])
    );
}
#[test]
fn atlas_geometry_remaps_manual_rules_tags_custom_data_and_saved_selections() {
    let mut p = Project::new();
    let manual = p.add_layer("Art", "Tiles", 16).unwrap();
    let ts = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    p.assign_tileset(manual, ts).unwrap();
    let key = p.level_keys()[0];
    p.paint_tile(key, 0, 0, 0, 2, [0, 16], 0, false).unwrap();
    p.layer_mut(key, 0).unwrap()["gridTiles"][0]["future"] = json!(42);
    let source = p.add_layer("Source", "IntGrid", 16).unwrap();
    let layer = p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["uid"] == source)
        .unwrap();
    layer["tilesetDefUid"] = json!(ts);
    layer["autoRuleGroups"] = json!([{"uid":100,"active":true,"rules":[{"uid":101,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0,2,3]],"chance":1}]}]);
    let atlas = p.json["defs"]["tilesets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["uid"] == ts)
        .unwrap();
    atlas["savedSelections"] = json!([{"ids":[0,3],"future":"keep"}]);
    atlas["enumTags"] = json!([{"enumValueId":"Grass","tileIds":[2]}]);
    atlas["customData"] = json!([{"tileId":2,"data":"hello"}]);
    p.update_tileset_definition(ts, &json!({"pxWid":64}))
        .unwrap();
    assert_eq!(p.layer(key, 1).unwrap()["gridTiles"][0]["t"], 4);
    assert_eq!(
        p.layer(key, 1).unwrap()["gridTiles"][0]["src"],
        json!([0, 16])
    );
    assert_eq!(p.layer(key, 1).unwrap()["gridTiles"][0]["future"], 42);
    assert_eq!(
        p.layer_def(source).unwrap()["autoRuleGroups"][0]["rules"][0]["tileRectsIds"],
        json!([[0, 4, 5]])
    );
    let atlas = p.tileset(ts).unwrap();
    assert_eq!(atlas["savedSelections"][0]["ids"], json!([0, 5]));
    assert_eq!(atlas["savedSelections"][0]["future"], "keep");
    assert_eq!(atlas["enumTags"][0]["tileIds"], json!([4]));
    assert_eq!(atlas["customData"][0]["tileId"], 4);
}
#[test]
fn original_partial_atlas_cells_can_be_painted() {
    let mut p = Project::new();
    let layer = p.add_layer("Art", "Tiles", 16).unwrap();
    let ts = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    p.update_tileset_definition(ts, &json!({"spacing":1}))
        .unwrap();
    p.assign_tileset(layer, ts).unwrap();
    let key = p.level_keys()[0];
    assert_eq!(p.tileset(ts).unwrap()["__cWid"], 2);
    assert!(p.paint_tile(key, 0, 0, 0, 1, [17, 0], 0, false).unwrap());
}
#[test]
fn explicit_atlas_loss_policy_preserves_auto_stamp_holes_and_reports_discarded_data() {
    use ldtk_core::TilesetRemap;
    let mut p = Project::new();
    let source = p.add_layer("Source", "IntGrid", 16).unwrap();
    let ts = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    p.assign_tileset(source, ts).unwrap();
    p.json["defs"]["layers"][0]["autoRuleGroups"] = json!([{"uid":100,"active":true,"rules":[{"uid":101,"active":true,"size":1,"pattern":[1],"tileMode":"Stamp","tileRectsIds":[[0,1,2,3]],"chance":1}]}]);
    let key = p.level_keys()[0];
    p.paint_int_grid(key, 0, 1, 1, 1).unwrap();
    assert_eq!(
        p.layer(key, 0).unwrap()["autoLayerTiles"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let before = p.json.clone();
    assert!(
        p.update_tileset_definition(ts, &json!({"pxWid":16}))
            .is_err()
    );
    assert_eq!(p.json, before);
    let removed = p
        .update_tileset_definition_with_policy(
            ts,
            &json!({"pxWid":16}),
            TilesetRemap::AtlasCoordinates,
            true,
        )
        .unwrap();
    assert_eq!(removed, 2);
    assert_eq!(
        p.layer_def(source).unwrap()["autoRuleGroups"][0]["rules"][0]["tileRectsIds"],
        json!([[0, null, 1, null]])
    );
    p.update_auto_rule(source, 100, 101, &json!({"alpha":0.5}))
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Stamp.ldtk");
    p.save(&path).unwrap();
    let mut reopened = Project::load(&path).unwrap();
    assert_eq!(
        reopened.layer_def(source).unwrap()["autoRuleGroups"][0]["rules"][0]["tileRectsIds"],
        json!([[0, null, 1, null]])
    );
    reopened.paint_int_grid(key, 0, 3, 3, 1).unwrap();
    let tiles = reopened.layer(key, 0).unwrap()["autoLayerTiles"]
        .as_array()
        .unwrap();
    let pixels: Vec<_> = tiles
        .iter()
        .map(|tile| json!([tile["t"], tile["src"], tile["px"], tile["a"]]))
        .collect();
    assert_eq!(
        json!(pixels),
        json!([
            [0, [0, 0], [16, 16], 0.5],
            [1, [0, 16], [16, 32], 0.5],
            [0, [0, 0], [48, 48], 0.5],
            [1, [0, 16], [48, 64], 0.5]
        ])
    );
}
#[test]
fn per_level_override_policy_can_remap_by_atlas_coordinate_and_restore_default() {
    use ldtk_core::TilesetRemap;
    let mut p = Project::new();
    let layer = p.add_layer("Art", "Tiles", 16).unwrap();
    let a = p.add_tileset(Path::new("a.png"), 16, 32, 32).unwrap();
    let b = p.add_tileset(Path::new("b.png"), 16, 64, 32).unwrap();
    p.assign_tileset(layer, a).unwrap();
    let key = p.level_keys()[0];
    p.paint_tile(key, 0, 0, 0, 2, [0, 16], 0, false).unwrap();
    p.set_layer_tileset_override(key, 0, Some(b), TilesetRemap::AtlasCoordinates, false)
        .unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["t"], 4);
    assert_eq!(
        p.layer(key, 0).unwrap()["gridTiles"][0]["src"],
        json!([0, 16])
    );
    p.set_layer_tileset_override(key, 0, None, TilesetRemap::AtlasCoordinates, false)
        .unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["t"], 2);
    assert!(p.layer(key, 0).unwrap()["overrideTilesetUid"].is_null());
}

#[test]
fn duplicating_definitions_preserves_metadata_and_regenerates_owned_ids() {
    let mut p = Project::new();
    let layer = p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(
        LevelKey {
            world: None,
            level: 0,
        },
        0,
        0,
        0,
        1,
    )
    .unwrap();
    p.json["defs"]["layers"][0]["future"] = json!({"data":[1,2]});
    p.json["defs"]["layers"][0]["autoRuleGroups"] = json!([{"uid":100,"name":"Rules","rules":[{"uid":101,"active":false,"size":1,"pattern":[0],"tileIds":[0],"tileRectsIds":[[0]]}]}]);
    let copied = p
        .duplicate_definition("layers", layer, "CellsCopy")
        .unwrap();
    let d = p.layer_def(copied).unwrap();
    assert_eq!(d["future"], json!({"data":[1,2]}));
    assert_ne!(d["autoRuleGroups"][0]["uid"], 100);
    assert_ne!(d["autoRuleGroups"][0]["rules"][0]["uid"], 101);
    let key = LevelKey {
        world: None,
        level: 0,
    };
    assert_eq!(p.layer(key, 1).unwrap()["layerDefUid"], copied);
    assert!(
        p.layer(key, 1).unwrap()["intGridCsv"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v == 0)
    );
    assert_ne!(
        p.layer(key, 0).unwrap()["iid"],
        p.layer(key, 1).unwrap()["iid"]
    );
    let entity = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(entity), "Health", "Int", false)
        .unwrap();
    let entity_copy = p
        .duplicate_definition("entities", entity, "ActorCopy")
        .unwrap();
    assert_ne!(
        p.json["defs"]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uid"] == entity_copy)
            .unwrap()["fieldDefs"][0]["uid"],
        field
    );
    let en = p.add_enum_definition("Kind", &["A", "B"]).unwrap();
    let ec = p.duplicate_definition("enums", en, "KindCopy").unwrap();
    assert_eq!(
        p.json["defs"]["enums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uid"] == ec)
            .unwrap()["values"],
        p.json["defs"]["enums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["uid"] == en)
            .unwrap()["values"]
    );
    p.reorder_definition("entities", entity_copy, 0).unwrap();
    assert_eq!(p.json["defs"]["entities"][0]["uid"], entity_copy);
    let before = p.json.clone();
    assert!(
        p.duplicate_definition("entities", entity, "ActorCopy")
            .is_err()
    );
    assert!(p.reorder_definition("entities", entity, 99).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn tileset_import_accepts_partial_edge_cells_and_clamps_grid_like_source() {
    let mut project = Project::new();
    let uid = project
        .add_tileset(Path::new("partial.png"), 16, 35, 18)
        .unwrap();
    let atlas = project.tileset(uid).unwrap();
    assert_eq!(atlas["tileGridSize"], 16);
    assert_eq!(atlas["__cWid"], 3);
    assert_eq!(atlas["__cHei"], 2);
    let small = project
        .add_tileset(Path::new("small.png"), 16, 15, 32)
        .unwrap();
    let atlas = project.tileset(small).unwrap();
    assert_eq!(atlas["tileGridSize"], 15);
    assert_eq!(atlas["__cWid"], 1);
    assert_eq!(atlas["__cHei"], 3);
    let before = project.json.clone();
    for (grid, width, height) in [(0, 32, 32), (16, 0, 32), (16, 32, -1)] {
        assert!(
            project
                .add_tileset(Path::new("invalid.png"), grid, width, height)
                .is_err()
        );
        assert_eq!(project.json, before);
    }
    let huge = project
        .add_tileset(Path::new("huge.png"), 2, i32::MAX, 2)
        .unwrap();
    assert_eq!(project.tileset(huge).unwrap()["__cWid"], 1_073_741_824i64);
}
