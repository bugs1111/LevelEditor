use ldtk_core::{History, LevelKey, Project};
use serde_json::json;
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
fn sample() -> Project {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 16).unwrap();
    p.add_entity(K, 0, uid, 16, 16).unwrap();
    p
}
#[test]
fn duplicate_remaps_ids_and_internal_refs_and_preserves_unknowns() {
    let mut p = sample();
    p.json["levels"][0]["unknown"] = json!({"x":7});
    let old = p.level(K).unwrap().clone();
    let k = p.duplicate_level(K, "Copy").unwrap();
    let copied = p.level(k).unwrap();
    assert_ne!(old["iid"], copied["iid"]);
    assert_ne!(old["uid"], copied["uid"]);
    assert_ne!(
        old["layerInstances"][0]["iid"],
        copied["layerInstances"][0]["iid"]
    );
    assert_ne!(
        old["layerInstances"][0]["entityInstances"][0]["iid"],
        copied["layerInstances"][0]["entityInstances"][0]["iid"]
    );
    assert_eq!(copied["unknown"], old["unknown"]);
    assert_eq!(copied["layerInstances"][0]["levelId"], copied["uid"]);
}

#[test]
fn duplicate_level_preserves_source_reference_slots_and_targets() {
    let mut p = sample();
    let actor = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let other = p.add_level(None, "Other", 256, 256).unwrap();
    p.add_entity(K, 0, actor, 48, 16).unwrap();
    p.add_entity(other, 0, actor, 16, 16).unwrap();
    let entity_id = |project: &Project, level, index: usize| {
        project.layer(level, 0).unwrap()["entityInstances"][index]["iid"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let owner = entity_id(&p, K, 0);
    let local = entity_id(&p, K, 1);
    let remote = entity_id(&p, other, 0);
    let entity_field = p
        .add_field_definition(Some(actor), "Targets", "EntityRef", true)
        .unwrap();
    let level_field = p
        .add_field_definition(None, "LevelTargets", "EntityRef", true)
        .unwrap();
    let local_field = p
        .add_field_definition(Some(actor), "LocalTarget", "EntityRef", false)
        .unwrap();
    let remote_field = p
        .add_field_definition(Some(actor), "RemoteTarget", "EntityRef", false)
        .unwrap();
    for uid in [entity_field, level_field, local_field, remote_field] {
        p.update_field_definition(uid, &json!({"canBeNull":true, "allowedRefs":"Any"}))
            .unwrap();
    }
    p.set_field_editor_value(
        K,
        Some(&owner),
        entity_field,
        json!([null, local, null, remote, null]),
    )
    .unwrap();
    p.set_field_editor_value(K, None, level_field, json!([local, null, remote]))
        .unwrap();
    p.set_field_editor_value(K, Some(&owner), local_field, json!(local))
        .unwrap();
    p.set_field_editor_value(K, Some(&owner), remote_field, json!(remote))
        .unwrap();
    let original_level_field = p.level(K).unwrap()["fieldInstances"][0].clone();
    let copy = p.duplicate_level(K, "Copy").unwrap();
    let copied_target = entity_id(&p, copy, 1);
    let fields = &p.layer(copy, 0).unwrap()["entityInstances"][0]["fieldInstances"];
    let field = &fields[0];
    assert_eq!(
        field["__value"],
        json!([null, {
            "entityIid":copied_target,
            "layerIid":p.layer(copy, 0).unwrap()["iid"],
            "levelIid":p.level(copy).unwrap()["iid"],
            "worldIid":p.json["dummyWorldIid"],
        }, null, null])
    );
    assert_eq!(
        field["realEditorValues"],
        json!([null, {"id":"V_String","params":[copied_target]}, null, null])
    );
    assert_eq!(
        p.level(copy).unwrap()["fieldInstances"][0],
        original_level_field
    );
    assert_eq!(fields[1]["__value"], field["__value"][1]);
    assert_eq!(
        fields[1]["realEditorValues"][0],
        field["realEditorValues"][1]
    );
    assert!(fields[2]["__value"].is_null());
    assert_eq!(fields[2]["realEditorValues"], json!([]));
}

#[test]
fn entity_remove_repairs_reference_and_toc() {
    let mut p = sample();
    let uid = p.json["defs"]["entities"][0]["uid"].clone();
    p.json["defs"]["entities"][0]["exportToToc"] = json!(true);
    let iid = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"].clone();
    p.json["levels"][0]["fieldInstances"] = json!([{"__type":"EntityRef","__value":{"entityIid":iid},"realEditorValues":[{"id":"V_String","params":[iid]}]}]);
    p.refresh_derived_data().unwrap();
    assert_eq!(
        p.json["toc"][0]["instancesData"].as_array().unwrap().len(),
        1
    );
    assert!(p.remove_entity_with_references(K, 0, 0).unwrap());
    assert!(p.json["levels"][0]["fieldInstances"][0]["__value"].is_null());
    assert_eq!(p.json["toc"][0]["instancesData"], json!([]));
    assert_eq!(p.json["defs"]["entities"][0]["uid"], uid);
}

#[test]
fn entity_toc_toggle_clears_field_exports_and_rebuilds_the_table() {
    let mut p = sample();
    let uid = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let field = p.add_field_definition(Some(uid), "Health", "Int", false).unwrap();
    p.update_entity_definition(uid, &json!({"exportToToc":true})).unwrap();
    p.update_field_definition(field, &json!({"exportToToc":true})).unwrap();
    let first_entry = p.json["toc"][0]["instancesData"][0]["fields"].clone();
    assert!(first_entry.get("Health").is_some());

    p.update_entity_definition(uid, &json!({"exportToToc":false})).unwrap();
    assert_eq!(p.json["defs"]["entities"][0]["fieldDefs"][0]["exportToToc"], false);
    assert_eq!(p.json["toc"], json!([]));

    p.update_entity_definition(uid, &json!({"exportToToc":true})).unwrap();
    assert_eq!(p.json["toc"][0]["instancesData"][0]["fields"], json!({}));
    p.update_field_definition(field, &json!({"exportToToc":true})).unwrap();
    assert_eq!(p.json["toc"][0]["instancesData"][0]["fields"], first_entry);
}

#[test]
fn linear_toc_positions_restart_in_each_world() {
    let mut p = sample();
    p.resize_level(K, 64, 96, 0, 0).unwrap();
    p.duplicate_level(K, "Second").unwrap();
    p.set_world_layout(None, "LinearHorizontal", 256, 256).unwrap();
    p.enable_multiworlds().unwrap();
    let vertical = p.duplicate_world(0, "Vertical").unwrap();
    p.set_world_layout(Some(vertical), "LinearVertical", 256, 256).unwrap();
    p.json["defs"]["entities"][0]["exportToToc"] = json!(true);
    p.refresh_derived_data().unwrap();

    let positions: Vec<_> = p.json["toc"][0]["instancesData"].as_array().unwrap().iter()
        .map(|entry| [entry["worldX"].as_i64().unwrap(), entry["worldY"].as_i64().unwrap()])
        .collect();
    assert_eq!(positions, [[16, 16], [128, 16], [16, 16], [16, 160]]);
}

#[test]
fn save_rebuilds_table_of_contents_after_flag_and_position_changes() {
    let dir = tempfile::tempdir().unwrap();
    for external in [false, true] {
        let path = dir.path().join(if external {
            "External.ldtk"
        } else {
            "Inline.ldtk"
        });
        let mut p = sample();
        p.json["externalLevels"] = json!(external);
        p.json["defs"]["entities"][0]["exportToToc"] = json!(true);
        p.refresh_derived_data().unwrap();
        p.save(&path).unwrap();
        assert_eq!(p.json["toc"][0]["instances"], json!([]));

        p.json["flags"] = json!(["ExportOldTableOfContentData"]);
        p.layer_mut(K, 0).unwrap()["entityInstances"][0]["px"] = json!([32, 48]);
        p.save(&path).unwrap();
        let saved = Project::load(&path).unwrap();
        let entry = &saved.json["toc"][0];
        let entity = &saved.layer(K, 0).unwrap()["entityInstances"][0];
        let iids = json!({
            "worldIid": saved.json["dummyWorldIid"],
            "levelIid": saved.level(K).unwrap()["iid"],
            "layerIid": saved.layer(K, 0).unwrap()["iid"],
            "entityIid": entity["iid"]
        });
        assert_eq!(entry["instances"], json!([iids]));
        assert_eq!(
            entry["instancesData"],
            json!([{
                "iids": iids,
                "worldX": 32,
                "worldY": 48,
                "widPx": 16,
                "heiPx": 16,
                "fields": {}
            }])
        );
        assert_eq!(p.json["toc"], saved.json["toc"]);

        p.json["flags"] = json!([]);
        p.save(&path).unwrap();
        let saved = Project::load(&path).unwrap();
        assert_eq!(saved.json["toc"][0]["instances"], json!([]));
        assert_eq!(
            saved.json["toc"][0]["instancesData"],
            entry["instancesData"]
        );
    }
}

#[test]
fn preview_154_saves_level_smart_colors_for_inline_and_external_levels() {
    let directory = tempfile::tempdir().unwrap();
    for external in [false, true] {
        let path = directory.path().join(if external {
            "external.ldtk"
        } else {
            "inline.ldtk"
        });
        let mut project = Project::new();
        let other = project.add_level(None, "Other", 256, 256).unwrap();
        let tint = project
            .add_field_definition(None, "Tint", "Color", true)
            .unwrap();
        project
            .update_field_definition(tint, &json!({"canBeNull":true}))
            .unwrap();
        project.add_enum_definition("Biome", &["Water"]).unwrap();
        project.json["defs"]["enums"][0]["values"][0]["color"] = json!(0x7695D1);
        let region = project
            .add_field_definition(None, "Region", "LocalEnum.Biome", false)
            .unwrap();
        project
            .update_field_definition(region, &json!({"useForSmartColor":true}))
            .unwrap();
        project
            .set_field_editor_value(other, None, tint, json!([null, "#204080", "#FFFFFF"]))
            .unwrap();
        project
            .set_field_value(other, None, region, json!("Water"))
            .unwrap();
        project.level_mut(K).unwrap()["bgColor"] = json!("#C24A58");
        project.json["externalLevels"] = json!(external);
        project.save(&path).unwrap();

        let saved = Project::load(&path).unwrap();
        assert_eq!(saved.level(K).unwrap()["__smartColor"], "#DD9BA3");
        assert_eq!(saved.level(other).unwrap()["__smartColor"], "#8496B9");

        project
            .set_field_editor_value(other, None, tint, json!([null]))
            .unwrap();
        project.save(&path).unwrap();
        let saved = Project::load(&path).unwrap();
        assert_eq!(saved.level(other).unwrap()["__smartColor"], "#B4C5E6");
        let stub = &saved.json["levels"][other.level];
        assert_eq!(stub["__smartColor"], "#B4C5E6");
    }
}

#[test]
fn world_level_resize_reflows_intgrid_without_losing_cells() {
    let mut p = Project::new();
    let w = p.add_world("World").unwrap();
    let k = p.add_level(Some(w), "Room", 32, 32).unwrap();
    p.add_layer("Solid", "IntGrid", 16).unwrap();
    p.layer_mut(k, 0).unwrap()["intGridCsv"] = json!([1, 0, 0, 1]);
    p.resize_level(k, 48, 32, 16, 0).unwrap();
    assert_eq!(
        p.layer(k, 0).unwrap()["intGridCsv"],
        json!([0, 1, 0, 0, 0, 1])
    );
    p.move_level(k, 100, 200, 2).unwrap();
    assert_eq!(p.level(k).unwrap()["worldX"], 100);
    p.remove_level(k).unwrap();
    assert_eq!(p.json["worlds"][w]["levels"], json!([]));
}
#[test]
fn transaction_rolls_back_error() {
    let mut p = sample();
    let old = p.json.clone();
    let result: ldtk_core::Result<()> = p.edit_document(|p| {
        p.json["nextUid"] = json!(999);
        Err(ldtk_core::Error::Invalid("abort".into()))
    });
    assert!(result.is_err());
    assert_eq!(p.json, old);
}
#[test]
fn history_project_changes_include_external_and_do_not_overwrite_unrelated_edits() {
    let mut p = sample();
    let mut h = History::default();
    h.begin_project(&p);
    p.rename_definition(
        "entities",
        p.json["defs"]["entities"][0]["uid"].as_i64().unwrap(),
        "Player",
    )
    .unwrap();
    h.commit(&p);
    p.json["bgColor"] = json!("#010203");
    assert!(h.undo(&mut p));
    assert_eq!(p.json["defs"]["entities"][0]["identifier"], "Hero");
    assert_eq!(p.json["bgColor"], "#010203");
    assert!(h.redo(&mut p));
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["__identifier"],
        "Player"
    );
}
#[test]
fn definition_remove_propagates_layer_instances() {
    let mut p = sample();
    let uid = p.json["defs"]["layers"][0]["uid"].as_i64().unwrap();
    p.remove_definition("layers", uid).unwrap();
    assert_eq!(p.level(K).unwrap()["layerInstances"], json!([]));
    assert_eq!(p.json["defs"]["layers"], json!([]));
}
#[test]
#[ignore = "requires pinned upstream historical fixture under .reference/ldtk"]
fn official_reference_fixture_can_rebuild_derived_data() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/labRefs.ldtk");
    let mut p = Project::load(root).unwrap();
    let unknown = json!({"keep":123});
    p.json["future"] = unknown.clone();
    p.refresh_derived_data().unwrap();
    assert_eq!(p.json["future"], unknown);
    assert!(p.json["toc"].is_array());
    for k in p.level_keys() {
        assert!(p.level(k).unwrap()["layerInstances"].is_array());
    }
}
#[test]
fn definition_properties_propagate_and_rollback_invalid_dimensions() {
    let mut p = sample();
    let uid = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    p.update_entity_definition(
        uid,
        &json!({"identifier":"Player","width":32,"pivotX":1,"tags":["player"],"color":"#123456"}),
    )
    .unwrap();
    let e = &p.layer(K, 0).unwrap()["entityInstances"][0];
    assert_eq!(e["width"], 32);
    assert_eq!(e["__pivot"], json!([1, 0]));
    assert_eq!(e["__identifier"], "Player");
    assert_eq!(e["__smartColor"], "#123456");
    let old = p.json.clone();
    assert!(
        p.update_entity_definition(uid, &json!({"height":0}))
            .is_err()
    );
    assert_eq!(p.json, old);
}
#[test]
fn entity_definition_preserves_outside_pivots_on_instances_and_reopen() {
    let mut p = sample();
    let uid = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    p.update_entity_definition(uid, &json!({"pivotX":-0.25,"pivotY":1.75}))
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["__pivot"],
        json!([-0.25, 1.75])
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Pivot.ldtk");
    p.save(&path).unwrap();
    let mut reopened = Project::load(&path).unwrap();
    reopened
        .update_entity_definition(uid, &json!({"doc":"Outside origin"}))
        .unwrap();
    assert_eq!(reopened.json["defs"]["entities"][0]["pivotX"], json!(-0.25));
    assert_eq!(reopened.json["defs"]["entities"][0]["pivotY"], json!(1.75));

    let before = reopened.json.clone();
    assert!(
        reopened
            .update_entity_definition(uid, &json!({"pivotX":"bad"}))
            .is_err()
    );
    assert_eq!(reopened.json, before);

    let ratio = 7. / 22.;
    let rounded = 318. / 1000.;
    reopened
        .update_entity_definition(uid, &json!({"pivotX":ratio}))
        .unwrap();
    reopened.save(&path).unwrap();
    let saved = Project::load(&path).unwrap();
    assert_eq!(saved.json["defs"]["entities"][0]["pivotX"], json!(rounded));
    assert_eq!(
        saved.layer(K, 0).unwrap()["entityInstances"][0]["__pivot"],
        json!([rounded, 1.75])
    );
    assert_eq!(reopened.json["defs"]["entities"][0]["pivotX"], json!(ratio));
}
#[test]
fn history_cancel_restores_pending_cross_document_changes() {
    let mut p = sample();
    let mut h = History::default();
    let old = p.json.clone();
    h.begin_project(&p);
    p.add_world("World").unwrap();
    assert!(h.cancel(&mut p));
    assert_eq!(p.json, old);
    assert!(!h.can_undo());
    assert!(!h.cancel(&mut p));
}
#[test]
fn layer_properties_reorder_and_offsets_propagate() {
    let mut p = sample();
    let uid = p.json["defs"]["layers"][0]["uid"].as_i64().unwrap();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.reorder_layer(uid, 0).unwrap();
    p.update_layer_properties(
        uid,
        &json!({"identifier":"People","displayOpacity":0.25,"pxOffsetX":8}),
    )
    .unwrap();
    let l = p.layer(K, 0).unwrap();
    assert_eq!(l["__identifier"], "People");
    assert_eq!(l["__opacity"], 0.25);
    assert_eq!(l["__pxTotalOffsetX"], 8);
}
#[test]
fn tileset_geometry_updates_remap_sources_and_preserve_unknown_tile_keys() {
    let mut p = Project::new();
    let ts = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    let layer = p.add_layer("Art", "Tiles", 16).unwrap();
    p.assign_tileset(layer, ts).unwrap();
    p.paint_tile(K, 0, 0, 0, 2, [0, 16], 0, false).unwrap();
    p.layer_mut(K, 0).unwrap()["gridTiles"][0]["meta"] = json!("keep");
    p.update_tileset_definition(ts, &json!({"pxWid":64}))
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["gridTiles"][0]["src"],
        json!([0, 16])
    );
    assert_eq!(p.layer(K, 0).unwrap()["gridTiles"][0]["t"], 4);
    assert_eq!(p.layer(K, 0).unwrap()["gridTiles"][0]["meta"], "keep");
}
#[test]
fn entity_grid_uses_original_truncation_for_negative_anchor() {
    let mut p = sample();
    p.move_entity(K, 0, 0, -1, -1).unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["__grid"],
        json!([0, 0])
    );
}
#[test]
fn changing_layer_grid_keeps_cell_coordinates_and_scales_entity_anchors() {
    let mut p = sample();
    let e = p.json["defs"]["layers"][0]["uid"].as_i64().unwrap();
    p.change_layer_grid_size(e, 8).unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["px"],
        json!([8, 8])
    );
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"][0]["width"], 16);
    let uid = p.add_layer("Grid", "IntGrid", 16).unwrap();
    p.layer_mut(K, 0).unwrap()["intGridCsv"][1] = json!(1);
    p.change_layer_grid_size(uid, 8).unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 1);
    assert_eq!(p.layer(K, 0).unwrap()["__cWid"], 32);
}
#[test]
fn layer_conversion_requires_explicit_data_loss_and_can_bake_auto_tiles() {
    let mut p = Project::new();
    let uid = p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.layer_mut(K, 0).unwrap()["autoLayerTiles"] =
        json!([{"px":[0,0],"src":[0,0],"t":0,"f":0,"a":1,"d":[7,0,0]}]);
    let old = p.json.clone();
    assert!(p.convert_layer_type(uid, "Tiles", false).is_err());
    assert_eq!(p.json, old);
    p.convert_layer_type(uid, "Tiles", true).unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["__type"], "Tiles");
    assert_eq!(p.layer(K, 0).unwrap()["gridTiles"][0]["d"], json!([0]));
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"], json!([]));
}
#[test]
fn gridvania_grid_resize_preserves_defaults_until_world_tidy() {
    let mut p = Project::new();
    p.json["defaultLevelWidth"] = json!(512);
    p.set_world_layout(None, "GridVania", 256, 128).unwrap();
    p.move_level(K, -256, 128, 0).unwrap();

    p.set_world_layout(None, "GridVania", 300, 128).unwrap();
    assert_eq!(p.level(K).unwrap()["worldX"], -300);
    assert_eq!(p.level(K).unwrap()["worldY"], 128);
    assert_eq!(p.json["defaultLevelWidth"], 512);
    assert_eq!(p.json["defaultLevelHeight"], 256);

    let mut reopened = p.clone();
    reopened.migrate_to_current().unwrap();
    assert_eq!(reopened.json["defaultLevelWidth"], 600);

    let mut duplicated = p.clone();
    duplicated.duplicate_level(K, "Copy").unwrap();
    assert_eq!(duplicated.json["defaultLevelWidth"], 600);

    let width = p.json["defaultLevelWidth"].as_i64().unwrap() as i32;
    let created = p.add_level(None, "Another", width, 256).unwrap();
    assert_eq!(p.level(created).unwrap()["pxWid"], 512);
    assert_eq!(p.json["defaultLevelWidth"], 600);
}

#[test]
fn linear_world_layout_serializes_original_world_coordinates() {
    let mut p = sample();
    p.set_world_layout(None, "LinearHorizontal", 128, 128)
        .unwrap();
    assert_eq!(p.level(K).unwrap()["worldX"], -1);
    assert_eq!(p.level(K).unwrap()["worldY"], -1);
    assert!(
        p.layer(K, 0).unwrap()["entityInstances"][0]
            .get("__worldX")
            .is_none()
    );
    assert_eq!(p.level(K).unwrap()["__neighbours"], json!([]));
}
#[test]
fn enum_definition_removal_removes_its_field_definitions_and_instances() {
    let mut p = Project::new();
    let e = p.add_enum_definition("Mode", &["Idle"]).unwrap();
    p.add_field_definition(None, "Mode", "LocalEnum.Mode", false)
        .unwrap();
    p.remove_definition("enums", e).unwrap();
    assert_eq!(p.json["defs"]["levelFields"], json!([]));
    assert_eq!(p.level(K).unwrap()["fieldInstances"], json!([]));
}
#[test]
fn cross_level_entity_transfer_preserves_iid_and_rewrites_reference_location() {
    let mut p = sample();
    let original = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let f = p
        .add_field_definition(None, "Ref", "EntityRef", false)
        .unwrap();
    p.set_field_value(K, None, f, json!(original)).unwrap();
    let other = p.add_level(None, "Other", 64, 64).unwrap();
    p.transfer_entity(K, 0, 0, other, 0, 32, 32).unwrap();
    assert_eq!(
        p.layer(other, 0).unwrap()["entityInstances"][0]["iid"],
        original
    );
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"]["levelIid"],
        p.level(other).unwrap()["iid"]
    );
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"], json!([]));
}
#[test]
fn removing_source_layer_detaches_the_auto_layer_without_removing_its_groups() {
    let mut p = Project::new();
    let uid = p.add_layer("Source", "IntGrid", 16).unwrap();
    let target = p.add_layer("Target", "AutoLayer", 16).unwrap();
    let groups = json!([{"uid":999,"active":true,"rules":[]}]);
    p.update_layer_properties(target, &json!({"autoSourceLayerDefUid":uid,"autoRuleGroups":groups}))
        .unwrap();
    p.remove_definition("layers", uid).unwrap();
    let target = p.layer_def(target).unwrap();
    assert!(target["autoSourceLayerDefUid"].is_null());
    assert_eq!(target["autoRuleGroups"], groups);
}
#[test]
fn level_resize_crops_manual_tiles_by_the_offset_layer_grid() {
    let mut p = Project::new();
    let layer = p.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    p.assign_tileset(layer, atlas).unwrap();
    let layer = p.layer_mut(K, 0).unwrap();
    layer["pxOffsetX"] = json!(-4);
    layer["pxOffsetY"] = json!(4);
    layer["__cWid"] = json!(17);
    layer["gridTiles"] = json!([
        {"px":[32,16],"src":[0,0],"f":0,"t":0,"d":[19],"a":1},
        {"px":[32,16],"src":[16,0],"f":1,"t":1,"d":[19],"a":1,"extension":true},
        {"px":[16,32],"src":[0,16],"f":0,"t":2,"d":[35],"a":1},
        {"px":[48,16],"src":[16,16],"f":0,"t":3,"d":[20],"a":1}
    ]);

    p.resize_level(K, 32, 35, 0, 0).unwrap();
    let layer = p.layer(K, 0).unwrap();
    assert_eq!(layer["__cWid"], 3);
    assert_eq!(layer["__cHei"], 2);
    assert_eq!(layer["pxOffsetX"], -4);
    assert_eq!(layer["pxOffsetY"], 4);
    assert_eq!(
        layer["gridTiles"],
        json!([
            {"px":[32,16],"src":[0,0],"f":0,"t":0,"d":[5],"a":1},
            {"px":[32,16],"src":[16,0],"f":1,"t":1,"d":[5],"a":1,"extension":true}
        ])
    );
}

#[test]
fn resize_keeps_subgrid_offsets_and_moves_point_fields_in_cell_units() {
    let mut p = sample();
    let e = p.json["defs"]["entities"][0]["uid"].as_i64().unwrap();
    let f = p
        .add_field_definition(Some(e), "Path", "Point", true)
        .unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), f, json!([{"cx":1,"cy":1}]))
        .unwrap();
    p.resize_level(K, 48, 48, 20, 0).unwrap();
    let l = p.layer(K, 0).unwrap();
    assert_eq!(l["pxOffsetX"], 4);
    assert_eq!(l["__cWid"], 3);
    assert_eq!(l["entityInstances"][0]["px"], json!([32, 16]));
    assert_eq!(
        l["entityInstances"][0]["fieldInstances"][0]["__value"],
        json!([{"cx":2,"cy":1}])
    );
}
#[test]
fn resize_removes_entities_outside_bounds_and_cleans_incoming_refs() {
    let mut p = sample();
    let f = p
        .add_field_definition(None, "Hero", "EntityRef", false)
        .unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, None, f, json!(id)).unwrap();
    assert_eq!(
        p.resize_level_counting_removed_entities(K, 16, 16, 0, 0)
            .unwrap(),
        1
    );
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"], json!([]));
    assert!(p.level(K).unwrap()["fieldInstances"][0]["__value"].is_null());
}
#[test]
fn official_toc_neighbours_and_entity_caches_match_original_saved_exports() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/original");
    let mut toc_rows = 0;
    let mut entities = 0;
    let mut neighbours = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "ldtk") {
            continue;
        }
        let mut p = Project::load(&path).unwrap();
        let before = p.clone();
        p.refresh_derived_data().unwrap();
        assert_eq!(p.json["toc"], before.json["toc"], "{} TOC", path.display());
        toc_rows += p.json["toc"].as_array().unwrap().len();
        for key in p.level_keys() {
            let actual = p.level(key).unwrap();
            let expected = before.level(key).unwrap();
            assert_eq!(
                actual["__neighbours"],
                expected["__neighbours"],
                "{} neighbours",
                path.display()
            );
            neighbours += actual["__neighbours"].as_array().unwrap().len();
            for (i, layer) in actual["layerInstances"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                for (j, e) in layer["entityInstances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    for field in [
                        "__identifier",
                        "__grid",
                        "__pivot",
                        "__tags",
                        "__tile",
                        "__smartColor",
                        "__worldX",
                        "__worldY",
                    ] {
                        assert_eq!(
                            e[field],
                            expected["layerInstances"][i]["entityInstances"][j][field],
                            "{} {field}",
                            path.display()
                        );
                    }
                    entities += 1;
                }
            }
        }
    }
    assert!(toc_rows > 3 && entities > 50 && neighbours > 100);
    eprintln!("Compared {toc_rows} TOC entries, {entities} entities, {neighbours} neighbours");
}
#[test]
fn moving_level_between_worlds_rewrites_cross_document_entity_refs() {
    let mut p = sample();
    let f = p
        .add_field_definition(None, "Ref", "EntityRef", false)
        .unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = p.add_level(None, "Other", 64, 64).unwrap();
    p.set_field_value(other, None, f, json!(id)).unwrap();
    let world = p.add_world("SecondWorld").unwrap();
    let moved = p
        .move_level_to_world(
            LevelKey {
                world: Some(0),
                level: 0,
            },
            Some(world),
            0,
        )
        .unwrap();
    assert_eq!(moved.world, Some(world));
    assert_eq!(
        p.level(LevelKey {
            world: Some(0),
            level: 0
        })
        .unwrap()["fieldInstances"][0]["__value"]["worldIid"],
        p.json["worlds"][world]["iid"]
    );
}
#[test]
fn duplicate_world_regenerates_all_level_layer_entity_ids() {
    let mut p = Project::new();
    let w = p.add_world("First").unwrap();
    let k = p.add_level(Some(w), "Room", 32, 32).unwrap();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 16).unwrap();
    p.add_entity(k, 0, uid, 16, 16).unwrap();
    let w2 = p.duplicate_world(w, "Second").unwrap();
    let copy = LevelKey {
        world: Some(w2),
        level: 0,
    };
    assert_ne!(p.level(k).unwrap()["iid"], p.level(copy).unwrap()["iid"]);
    assert_ne!(
        p.layer(k, 0).unwrap()["entityInstances"][0]["iid"],
        p.layer(copy, 0).unwrap()["entityInstances"][0]["iid"]
    );
    assert_ne!(
        p.level(k).unwrap()["identifier"],
        p.level(copy).unwrap()["identifier"]
    );
}
#[test]
fn cheap_entity_geometry_helpers_follow_source_handles_limits_and_pivots() {
    let d = json!({"width":20,"height":10,"resizableX":true,"resizableY":true,"keepAspectRatio":true,"minWidth":8,"maxWidth":100,"minHeight":4,"maxHeight":50,"pivotX":0.5,"pivotY":1});
    assert_eq!(
        ldtk_core::constrained_entity_size(&d, [43, 33], [20, 10], Some(8)),
        [40, 20]
    );
    assert_eq!(
        ldtk_core::constrained_entity_size(&d, [20, 26], [20, 10], Some(8)),
        [48, 24]
    );
    let mut fixed = d.clone();
    fixed["resizableY"] = json!(false);
    assert_eq!(
        ldtk_core::constrained_entity_size(&fixed, [43, 33], [20, 10], Some(8)),
        [40, 10]
    );
    assert_eq!(
        ldtk_core::snapped_entity_position(&d, [1., 1.], 16, true),
        [8, 16]
    );
    assert_eq!(
        ldtk_core::snapped_entity_position(&d, [17., 17.], 16, true),
        [24, 32]
    );
    assert_eq!(
        ldtk_core::snapped_entity_position(&d, [1.7, 2.2], 16, false),
        [2, 2]
    );
}
#[test]
fn adding_world_migrates_legacy_levels_to_source_canonical_multiworld_json() {
    let mut p = sample();
    let original = p.level(K).unwrap().clone();
    let index = p.add_world("NewWorld").unwrap();
    assert_eq!(index, 1);
    assert_eq!(p.json["levels"], json!([]));
    assert!(p.json["worldLayout"].is_null());
    assert_eq!(
        p.level(LevelKey {
            world: Some(0),
            level: 0
        })
        .unwrap()["iid"],
        original["iid"]
    );
    assert_eq!(p.json["worlds"][0]["iid"], p.json["dummyWorldIid"]);
    assert_eq!(p.level_keys().len(), 1);
}

#[test]
fn disabling_multiworlds_preserves_external_levels_and_entity_references() {
    let mut p = sample();
    let original_dummy_iid = p.json["dummyWorldIid"].clone();
    let other_world = p.add_world("Remaining").unwrap();
    let before = p.json.clone();
    assert!(p.disable_multiworlds().is_err());
    assert_eq!(p.json, before);
    let target = LevelKey {
        world: Some(0),
        level: 0,
    };
    p.move_level_to_world(target, Some(other_world), 0).unwrap();
    p.remove_world(0).unwrap();
    let source = p.add_level(Some(0), "Other", 64, 64).unwrap();
    let field = p
        .add_field_definition(None, "Link", "EntityRef", false)
        .unwrap();
    let entity_iid = p.layer(target, 0).unwrap()["entityInstances"][0]["iid"].clone();
    p.set_field_value(source, None, field, entity_iid.clone())
        .unwrap();
    p.json["defs"]["entities"][0]["exportToToc"] = json!(true);
    p.json["worlds"][0]["worldLayout"] = json!("GridVania");
    p.json["worlds"][0]["worldGridWidth"] = json!(128);
    p.json["worlds"][0]["worldGridHeight"] = json!(96);
    p.json["worlds"][0]["defaultLevelWidth"] = json!(384);
    p.json["worlds"][0]["defaultLevelHeight"] = json!(192);
    p.json["projectExtension"] = json!(true);
    p.json["flags"] = json!(["MultiWorlds", "IgnoreBackupSuggest"]);
    p.json["levelNamePattern"] = json!("Room_%world_%idx");
    p.level_mut(target).unwrap()["useAutoIdentifier"] = json!(true);
    p.level_mut(source).unwrap()["levelExtension"] = json!(42);
    p.refresh_derived_data().unwrap();
    p.set_external_levels(true).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("World.ldtk");
    p.save(&path).unwrap();
    let mut p = Project::load(&path).unwrap();
    let world_iid = p.json["worlds"][0]["iid"].clone();
    let target_level_iid = p.level(target).unwrap()["iid"].clone();
    let source_level_iid = p.level(source).unwrap()["iid"].clone();
    let layer_iid = p.layer(target, 0).unwrap()["iid"].clone();
    let source_path = p.json["worlds"][0]["levels"][1]["externalRelPath"].clone();
    assert_ne!(world_iid, original_dummy_iid);
    let mut history = History::default();
    history.begin_project(&p);
    assert!(p.disable_multiworlds().unwrap());
    assert!(!p.disable_multiworlds().unwrap());
    assert!(history.commit(&p));
    assert!(history.undo(&mut p));
    assert_eq!(p.level(source).unwrap()["iid"], source_level_iid);
    assert_eq!(p.json["dummyWorldIid"], original_dummy_iid);
    assert!(history.redo(&mut p));
    p.save(&path).unwrap();
    let p = Project::load(&path).unwrap();

    let reference = json!({
        "worldIid": world_iid,
        "levelIid": target_level_iid,
        "layerIid": layer_iid,
        "entityIid": entity_iid,
    });
    assert_eq!(p.json["flags"], json!(["IgnoreBackupSuggest"]));
    assert_eq!(p.json["worlds"], json!([]));
    assert_eq!(p.json["dummyWorldIid"], world_iid);
    assert_eq!(p.json["worldLayout"], "GridVania");
    assert_eq!(p.json["worldGridWidth"], 128);
    assert_eq!(p.json["worldGridHeight"], 96);
    assert_eq!(p.json["defaultLevelWidth"], 384);
    assert_eq!(p.json["defaultLevelHeight"], 192);
    assert_eq!(p.json["projectExtension"], true);
    assert_eq!(p.json["externalLevels"], true);
    assert_eq!(p.json["levels"][1]["externalRelPath"], source_path);
    assert_eq!(p.level(K).unwrap()["iid"], target_level_iid);
    assert_eq!(p.level(K).unwrap()["identifier"], "Room_Remaining_0");
    assert_eq!(p.layer(K, 0).unwrap()["iid"], layer_iid);
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["iid"],
        entity_iid
    );
    let source = p
        .level(LevelKey {
            world: None,
            level: 1,
        })
        .unwrap();
    assert_eq!(source["iid"], source_level_iid);
    assert_eq!(source["levelExtension"], 42);
    assert_eq!(source["fieldInstances"][0]["__value"], reference);
    assert_eq!(p.json["toc"][0]["instancesData"][0]["iids"], reference);
}

#[test]
fn removing_palette_values_clears_cells_and_reparents_orphaned_groups() {
    let mut p = Project::new();
    let uid = p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.update_layer_properties(uid,&json!({"intGridValuesGroups":[{"uid":5,"identifier":"Group","color":null}],"intGridValues":[{"value":1,"identifier":"Wall","color":"#FFFFFF","groupUid":5},{"value":2,"identifier":"Floor","color":"#AAAAAA","groupUid":5}]})).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 0, 1, 0, 2).unwrap();
    p.update_layer_properties(uid,&json!({"intGridValuesGroups":[],"intGridValues":[{"value":2,"identifier":"Floor","color":"#AAAAAA","groupUid":5}]})).unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][0], 0);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 2);
    assert_eq!(p.layer_def(uid).unwrap()["intGridValues"][0]["groupUid"], 0);
}

#[test]
fn int_grid_palette_edits_preserve_unrelated_auto_layer_caches() {
    let mut p = Project::new();
    let cells = p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.add_layer("Decor", "AutoLayer", 16).unwrap();
    let cache = json!([{"px":[16,0],"src":[0,0],"t":0,"f":0,"d":[3,1],"a":1}]);
    p.layer_mut(K, 0).unwrap()["autoLayerTiles"] = cache.clone();
    p.update_layer_properties(cells, &json!({"intGridValues":[]})).unwrap();
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][0], 0);
    assert_eq!(p.layer(K, 0).unwrap()["autoLayerTiles"], cache);
}
#[test]
fn derived_external_summaries_keep_document_extensions_in_their_own_document() {
    let mut p = Project::new();
    let mut full = p.json["levels"][0].clone();
    full["externalFuture"] = json!({"keep":true});
    p.json["levels"][0]["externalRelPath"] = json!("level.ldtkl");
    p.json["levels"][0]["layerInstances"] = serde_json::Value::Null;
    p.json["levels"][0]["summaryFuture"] = json!(7);
    p.external_levels.insert("level.ldtkl".into(), full);
    p.json["defaultLevelBgColor"] = json!("#654321");
    p.move_level(K, 10, 20, 0).unwrap();
    assert!(p.json["levels"][0].get("externalFuture").is_none());
    assert_eq!(p.json["levels"][0]["summaryFuture"], 7);
    assert_eq!(
        p.external_levels["level.ldtkl"]["externalFuture"],
        json!({"keep":true})
    );
    assert_eq!(p.json["levels"][0]["worldX"], 10);
    assert_eq!(p.json["levels"][0]["__bgColor"], "#654321");
    assert_eq!(p.external_levels["level.ldtkl"]["__bgColor"], "#654321");
}

#[test]
fn derived_background_preserves_explicit_color_and_falls_back_to_workspace_for_old_projects() {
    let mut p = Project::new();
    let custom = p.add_level(None, "Custom", 16, 16).unwrap();
    p.level_mut(custom).unwrap()["bgColor"] = json!("#654321");
    p.json["defaultLevelBgColor"] = json!("#123456");
    p.refresh_derived_data().unwrap();
    assert_eq!(p.level(K).unwrap()["__bgColor"], "#123456");
    assert_eq!(p.level(custom).unwrap()["__bgColor"], "#654321");

    p.json
        .as_object_mut()
        .unwrap()
        .remove("defaultLevelBgColor");
    p.json["bgColor"] = json!("#ABCDEF");
    p.level_mut(custom).unwrap()["bgColor"] = serde_json::Value::Null;
    p.refresh_derived_data().unwrap();
    assert_eq!(p.level(K).unwrap()["__bgColor"], "#ABCDEF");
    assert_eq!(p.level(custom).unwrap()["__bgColor"], "#ABCDEF");
    p.migrate_to_current().unwrap();
    assert_eq!(p.json["defaultLevelBgColor"], "#ABCDEF");
    assert_eq!(p.level(K).unwrap()["__bgColor"], "#ABCDEF");
}

#[test]
fn finishing_entity_transform_checks_only_the_touched_entity_and_repairs_refs_and_points() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    let d = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(d), "Link", "EntityRef", false)
        .unwrap();
    let points = p
        .add_field_definition(Some(d), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, d, 16, 16).unwrap();
    p.add_entity(K, 0, d, 32, 16).unwrap();
    let first = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = p.layer(K, 0).unwrap()["entityInstances"][1]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&second), f, json!(first))
        .unwrap();
    p.set_field_value(
        K,
        Some(&second),
        points,
        json!([{"cx":0,"cy":0},{"cx":9999,"cy":0}]),
    )
    .unwrap();
    p.layer_mut(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][1]
        ["realEditorValues"][0]["extension"] = json!({"keep":true});
    p.layer_mut(K, 0).unwrap()["entityInstances"][0]["px"] = json!([-1, 0]);
    assert!(!p.finish_entity_transform(K, 0, 1).unwrap());
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][1]["__value"],
        json!([{"cx":0,"cy":0}])
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][1]["realEditorValues"],
        json!([{"id":"V_String","params":["0,0"],"extension":{"keep":true}}])
    );
    assert!(p.finish_entity_transform(K, 0, 0).unwrap());
    assert!(p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"].is_null());
    p.update_layer_properties(
        layer,
        &json!({"parallaxFactorX":0.5,"parallaxScaling":true}),
    )
    .unwrap();
    let right = p.layer(K, 0).unwrap()["__cWid"].as_i64().unwrap() * 8;
    p.layer_mut(K, 0).unwrap()["entityInstances"][0]["px"] = json!([right, 0]);
    assert!(!p.finish_entity_transform(K, 0, 0).unwrap());
    p.layer_mut(K, 0).unwrap()["entityInstances"][0]["px"] = json!([right + 1, 0]);
    assert!(p.finish_entity_transform(K, 0, 0).unwrap());
}
