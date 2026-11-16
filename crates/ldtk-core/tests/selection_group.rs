use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
#[test]
fn mixed_grid_move_keeps_layer_cells_and_identity_and_zero_holes_do_not_erase_destinations() {
    let mut p = Project::new();
    p.add_layer("Small", "IntGrid", 8).unwrap();
    p.add_layer("Large", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 0, 2, 0, 1).unwrap();
    let group = p.copy_selection_group(K, &[0, 1], [0, 0, 32, 16]).unwrap();
    assert_eq!(group["compatibleGrids"], true);
    p.move_selection_group(K, &group, [32, 0], false, false)
        .unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][2], 1);
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][4], 1);
}
#[test]
fn nonmultiple_grid_group_can_copy_but_move_is_rejected_atomically() {
    let mut p = Project::new();
    p.add_layer("Small", "IntGrid", 12).unwrap();
    p.add_layer("Large", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
    let group = p.copy_selection_group(K, &[0, 1], [0, 0, 48, 48]).unwrap();
    assert_eq!(group["compatibleGrids"], false);
    let before = p.json.clone();
    assert!(
        p.move_selection_group(K, &group, [48, 0], false, false)
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn group_copy_remaps_references_between_selected_entities_in_different_layers() {
    let mut p = Project::new();
    p.add_layer("One", "Entities", 16).unwrap();
    p.add_layer("Two", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(def), "Link", "EntityRef", false)
        .unwrap();
    p.add_entity(K, 0, def, 16, 16).unwrap();
    p.add_entity(K, 1, def, 16, 16).unwrap();
    let a = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = p.layer(K, 1).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&a), field, json!(b)).unwrap();
    let group = p.copy_selection_group(K, &[0, 1], [0, 0, 32, 32]).unwrap();
    p.paste_selection_group(K, &group, [32, 0], false, false, false)
        .unwrap();
    let copied_a = &p.layer(K, 0).unwrap()["entityInstances"][1];
    let copied_b = &p.layer(K, 1).unwrap()["entityInstances"][1];
    assert_ne!(copied_a["iid"], a);
    assert_ne!(copied_b["iid"], b);
    assert_eq!(
        copied_a["fieldInstances"][0]["__value"]["entityIid"],
        copied_b["iid"]
    );
}
#[test]
fn independently_selected_point_moves_without_moving_its_owner() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(def), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, def, 128, 128).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), f, json!([{"cx":1,"cy":1},{"cx":8,"cy":8}]))
        .unwrap();
    let group = p.copy_selection_group(K, &[0], [16, 16, 16, 16]).unwrap();
    assert_eq!(
        group["layers"][0]["clipboard"]["entityInstances"],
        json!([])
    );
    p.move_selection_group(K, &group, [16, 0], false, false)
        .unwrap();
    let e = &p.layer(K, 0).unwrap()["entityInstances"][0];
    assert_eq!(e["px"], json!([128, 128]));
    assert_eq!(
        e["fieldInstances"][0]["__value"],
        json!([{"cx":2,"cy":1},{"cx":8,"cy":8}])
    );
}
#[test]
fn mixed_cut_cleans_refs_and_select_all_includes_outside_allowed_entities() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    p.update_entity_definition(def, &json!({"allowOutOfBounds":true}))
        .unwrap();
    p.add_entity(K, 0, def, 16, 16).unwrap();
    p.move_entity(K, 0, 0, -16, -16).unwrap();
    let group = p.select_all_group(K, &[0]).unwrap();
    assert_eq!(
        group["layers"][0]["clipboard"]["entityInstances"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(p.delete_selection_group(K, &group).unwrap(), 1);
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"], json!([]));
    let _ = Value::Null;
}
#[test]
fn refreshing_or_moving_a_group_preserves_fields_edited_after_selection() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(def), "Count", "Int", false)
        .unwrap();
    p.add_entity(K, 0, def, 16, 16).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let group = p.copy_selection_group(K, &[0], [0, 0, 32, 32]).unwrap();
    p.set_field_value(K, Some(&id), f, json!(23)).unwrap();
    p.layer_mut(K, 0).unwrap()["entityInstances"][0]["future"] = json!({"keep":true});
    let refreshed = p.refresh_selection_group(K, &group).unwrap();
    assert_eq!(
        refreshed["layers"][0]["clipboard"]["entityInstances"][0]["fieldInstances"][0]["__value"],
        23
    );
    p.move_selection_group(K, &group, [16, 0], false, false)
        .unwrap();
    let e = &p.layer(K, 0).unwrap()["entityInstances"][0];
    assert_eq!(e["iid"], id);
    assert_eq!(e["fieldInstances"][0]["__value"], 23);
    assert_eq!(e["future"], json!({"keep":true}));
}
#[test]
fn selection_group_flips_tiles_and_preserves_extensions() {
    let mut p = Project::new();
    let layer = p.add_layer("Art", "Tiles", 16).unwrap();
    let ts = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    p.assign_tileset(layer, ts).unwrap();
    p.paint_tile(K, 0, 0, 0, 0, [0, 0], 0, false).unwrap();
    p.layer_mut(K, 0).unwrap()["gridTiles"][0]["future"] = json!(42);
    let group = p.copy_selection_group(K, &[0], [0, 0, 32, 16]).unwrap();
    p.paste_selection_group(K, &group, [32, 0], true, false, false)
        .unwrap();
    assert!(
        p.layer(K, 0).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["px"] == json!([48, 0]) && t["f"] == 1 && t["future"] == 42)
    );
}
#[test]
fn selected_points_outside_new_bounds_are_removed_without_corrupting_other_indices() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(e), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, e, 16, 16).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), f, json!([{"cx":14,"cy":0},{"cx":15,"cy":0}]))
        .unwrap();
    let group = p.copy_selection_group(K, &[0], [224, 0, 32, 16]).unwrap();
    p.move_selection_group(K, &group, [16, 0], false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"],
        json!([{"cx":15,"cy":0}])
    );
}
#[test]
fn selection_group_uses_layer_offsets_and_empty_space_policy() {
    let mut p = Project::new();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.layer_mut(K, 0).unwrap()["pxOffsetX"] = json!(4);
    p.layer_mut(K, 0).unwrap()["pxOffsetY"] = json!(8);
    p.refresh_derived_data().unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 0, 3, 0, 1).unwrap();
    let mut group = p.copy_selection_group(K, &[0], [4, 8, 32, 16]).unwrap();
    p.paste_selection_group(K, &group, [36, 8], false, false, false)
        .unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][3], 1);
    group["emptySpace"] = json!(true);
    p.paste_selection_group(K, &group, [36, 8], false, false, false)
        .unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][3], 0);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][2], 1);
}
#[test]
fn malformed_or_missing_layer_group_paste_is_atomic() {
    let mut p = Project::new();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    let mut group = p.select_all_group(K, &[0]).unwrap();
    group["layers"][0]["layerDefUid"] = json!(9999);
    let before = p.json.clone();
    assert!(
        p.paste_selection_group(K, &group, [16, 0], false, false, false)
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn an_empty_group_never_erases_arrival_data_even_with_empty_space_selection() {
    let mut p = Project::new();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 1, 0, 1).unwrap();
    let mut group = p.copy_selection_group(K, &[0], [0, 0, 16, 16]).unwrap();
    group["emptySpace"] = json!(true);
    assert_eq!(
        p.paste_selection_group(K, &group, [16, 0], false, false, false)
            .unwrap(),
        0
    );
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 1);
}
#[test]
fn result_groups_can_move_repeatedly_without_collecting_other_cells() {
    let mut p = Project::new();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 0, 1, 1, 1).unwrap();
    let group = p.copy_selection_group(K, &[0], [0, 0, 16, 16]).unwrap();
    let group = p
        .move_selection_group_with_result(K, &group, [16, 0], false, false)
        .unwrap();
    assert_eq!(group["origin"], json!([16, 0]));
    let group = p
        .move_selection_group_with_result(K, &group, [16, 0], false, false)
        .unwrap();
    assert_eq!(group["origin"], json!([32, 0]));
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][2], 1);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][17], 1);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 0);
}
#[test]
fn pasted_point_result_tracks_inserted_indices_and_can_move_again() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(e), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, e, 128, 128).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), f, json!([{"cx":0,"cy":0},{"cx":1,"cy":0}]))
        .unwrap();
    let group = p.copy_selection_group(K, &[0], [0, 0, 32, 16]).unwrap();
    let group = p
        .paste_selection_group_with_result(K, &group, [16, 0], false, false, false)
        .unwrap();
    let indices: Vec<_> = group["layers"][0]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["arrayIndex"].as_u64().unwrap())
        .collect();
    assert_eq!(indices, vec![0, 2]);
    p.move_selection_group_with_result(K, &group, [16, 0], false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"],
        json!([{"cx":2,"cy":0},{"cx":0,"cy":0},{"cx":3,"cy":0},{"cx":1,"cy":0}])
    );
}
#[test]
fn pasted_entity_result_tracks_new_iid_and_preserves_original_on_next_move() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Actor", 16, 16).unwrap();
    p.add_entity(K, 0, e, 16, 16).unwrap();
    let original = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"].clone();
    let group = p.copy_selection_group(K, &[0], [0, 0, 32, 32]).unwrap();
    let group = p
        .paste_selection_group_with_result(K, &group, [32, 0], false, false, false)
        .unwrap();
    assert_ne!(
        group["layers"][0]["clipboard"]["entityInstances"][0]["iid"],
        original
    );
    p.move_selection_group_with_result(K, &group, [16, 0], false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["px"],
        json!([16, 16])
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["px"],
        json!([64, 16])
    );
}
#[test]
fn explicit_layer_geometry_uses_scaled_grids_and_source_entity_point_deltas() {
    let mut p = Project::new();
    p.add_layer("Grid", "IntGrid", 16).unwrap();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(def), "Path", "Point", true)
        .unwrap();
    p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
    p.add_entity(K, 0, def, 8, 8).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), field, json!([{"cx":0,"cy":0}]))
        .unwrap();
    let geometry = json!([{"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[100.,50.],"scale":0.5},{"layerDefUid":p.layer(K,1).unwrap()["layerDefUid"],"origin":[100.,50.],"scale":1.}]);
    let regions = json!([{"layerIndex":0,"rect":[0,0,2,2],"geometry":geometry[0]},{"layerIndex":1,"rect":[0,0,1,1],"geometry":geometry[1]}]);
    let group = p
        .copy_selection_group_from_regions(K, &regions, [100, 50, 16, 16])
        .unwrap();
    assert_eq!(group["scaledGridSize"], 16.);
    let next = p
        .move_selection_group_with_geometry(K, &group, [16., 0.], &geometry, false, false)
        .unwrap();
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][1], 1);
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["px"],
        json!([24, 8])
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"][0]["cx"],
        2
    );
    p.move_selection_group_with_geometry(K, &next, [16., 0.], &geometry, false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["px"],
        json!([40, 8])
    );
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][2], 1);
}
#[test]
fn scaled_group_copy_keeps_cross_layer_refs_and_live_point_selection() {
    let mut p = Project::new();
    p.add_layer("A", "Entities", 16).unwrap();
    p.add_layer("B", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let link = p
        .add_field_definition(Some(def), "Link", "EntityRef", false)
        .unwrap();
    let point = p
        .add_field_definition(Some(def), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, def, 8, 8).unwrap();
    p.add_entity(K, 1, def, 8, 8).unwrap();
    let a = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = p.layer(K, 1).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&a), link, json!(b)).unwrap();
    p.set_field_value(K, Some(&a), point, json!([{"cx":0,"cy":0}]))
        .unwrap();
    let geometries = json!([{"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0,0],"scale":0.5},{"layerDefUid":p.layer(K,1).unwrap()["layerDefUid"],"origin":[0,0],"scale":1.}]);
    let regions = json!([{"layerIndex":0,"rect":[0,0,1,1],"geometry":geometries[0]},{"layerIndex":1,"rect":[0,0,1,1],"geometry":geometries[1]}]);
    let group = p
        .copy_selection_group_from_regions(K, &regions, [0, 0, 16, 16])
        .unwrap();
    let pasted = p
        .paste_selection_group_with_geometry(K, &group, [16., 0.], &geometries, false, false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][0]["__value"]["entityIid"],
        p.layer(K, 1).unwrap()["entityInstances"][1]["iid"]
    );
    assert_eq!(pasted["layers"][0]["points"].as_array().unwrap().len(), 1);
    p.move_selection_group_with_geometry(K, &pasted, [16., 0.], &geometries, false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][1]["__value"][0]["cx"],
        4
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][1]["__value"][0]["cx"],
        0
    );
}

#[test]
fn empty_large_grid_layers_clear_arrival_rects_and_follow_repeated_moves() {
    let mut p = Project::new();
    p.add_layer("Large", "IntGrid", 32).unwrap();
    p.add_layer("Small", "IntGrid", 8).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 1, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 2, 0, 1).unwrap();
    let geometry = json!([
        {"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.,"visible":true},
        {"layerDefUid":p.layer(K,1).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.,"visible":true}
    ]);
    let regions = json!([
        {"layerIndex":0,"rect":[0,0,4,4],"geometry":geometry[0]},
        {"layerIndex":1,"rect":[0,0,1,1],"geometry":geometry[1]}
    ]);
    let mut group = p
        .copy_selection_group_from_regions(K, &regions, [0, 0, 32, 32])
        .unwrap();
    group["emptySpace"] = json!(true);
    assert_eq!(group["scaledGridSize"], 8.);
    for (x, next_cell) in [(32., 1), (64., 2)] {
        group = p
            .move_selection_group_with_geometry(K, &group, [32., 0.], &geometry, false, false)
            .unwrap();
        assert_eq!(
            p.layer(K, 1).unwrap()["intGridCsv"][next_cell],
            0,
            "empty large layer must clear the moved world rectangle at {x}"
        );
        assert_eq!(group["originalRects"][0]["leftPx"], x);
    }
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][8], 1);
}

#[test]
fn geometry_copy_clears_visible_unselectable_layers_but_not_hidden_layers() {
    let mut p = Project::new();
    let tiles = p.add_layer("Art", "Tiles", 16).unwrap();
    let ts = p
        .add_tileset(std::path::Path::new("art.png"), 16, 32, 32)
        .unwrap();
    p.assign_tileset(tiles, ts).unwrap();
    p.add_layer("Hidden", "IntGrid", 16).unwrap();
    let inactive = p.add_layer("Inactive", "IntGrid", 16).unwrap();
    p.add_layer("Small", "IntGrid", 8).unwrap();
    p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["uid"] == inactive)
        .unwrap()["canSelectWhenInactive"] = json!(false);
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 2, 0, 1).unwrap();
    p.paint_int_grid(K, 2, 2, 0, 1).unwrap();
    p.paint_tile(K, 3, 2, 0, 0, [0, 0], 0, false).unwrap();
    let geometry = json!((0..4).map(|i| json!({"layerDefUid":p.layer(K,i).unwrap()["layerDefUid"],"origin":[100.,50.],"scale":0.5,"visible":i!=2})).collect::<Vec<_>>());
    // Only the active layer contributes selected objects. Arrival cleanup is
    // separately defined over visible layers, using raw offsets/grid in source.
    let regions = json!([{"layerIndex":0,"rect":[0,0,4,2],"geometry":geometry[0]}]);
    let mut group = p
        .copy_selection_group_from_regions(K, &regions, [0, 0, 32, 16])
        .unwrap();
    group["emptySpace"] = json!(true);
    p.paste_selection_group_with_geometry(K, &group, [32., 0.], &geometry, false, false, false)
        .unwrap();
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][2], 0);
    assert_eq!(p.layer(K, 2).unwrap()["intGridCsv"][2], 1);
    assert!(
        p.layer(K, 3).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["intGridCsv"][0],
        1,
        "copy retains original material"
    );
}

#[test]
fn disjoint_original_rectangles_do_not_clear_the_gap_between_them() {
    let mut p = Project::new();
    p.add_layer("Grid", "IntGrid", 8).unwrap();
    for x in [0, 3, 4, 5, 6, 7] {
        p.paint_int_grid(K, 0, x, 0, 1).unwrap();
    }
    let geometry =
        json!([{"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.}]);
    let regions = json!([{"layerIndex":0,"rect":[0,0,4,1],"geometry":geometry[0]}]);
    let mut group = p
        .copy_selection_group_from_regions(K, &regions, [0, 0, 32, 8])
        .unwrap();
    group["emptySpace"] = json!(true);
    group["originalRects"] = json!([
        {"leftPx":0.,"rightPx":7.,"topPx":0.,"bottomPx":7.},
        {"leftPx":24.,"rightPx":31.,"topPx":0.,"bottomPx":7.}
    ]);
    p.move_selection_group_with_geometry(K, &group, [32., 0.], &geometry, false, false)
        .unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][5], 1);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][6], 1);
}

#[test]
fn a_rectangle_without_elements_does_not_move_or_clear_arrival_with_geometry() {
    let mut p = Project::new();
    p.add_layer("Grid", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 2, 0, 1).unwrap();
    let geometry =
        json!([{"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.}]);
    let regions = json!([{"layerIndex":0,"rect":[0,0,1,1],"geometry":geometry[0]}]);
    let mut group = p
        .copy_selection_group_from_regions(K, &regions, [0, 0, 16, 16])
        .unwrap();
    group["emptySpace"] = json!(true);
    let before = p.json.clone();
    let next = p
        .move_selection_group_with_geometry(K, &group, [32., 0.], &geometry, false, false)
        .unwrap();
    assert_eq!(p.json, before);
    assert_eq!(next["origin"], group["origin"]);
    assert_eq!(next["originalRects"], group["originalRects"]);
}

#[test]
fn arrival_cleanup_uses_raw_offsets_and_source_half_open_truncation() {
    let mut p = Project::new();
    p.add_layer("Inactive", "IntGrid", 16).unwrap();
    p.add_layer("Active", "IntGrid", 8).unwrap();
    p.layer_mut(K, 1).unwrap()["pxOffsetX"] = json!(4);
    p.refresh_derived_data().unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 1, 1, 0, 1).unwrap();
    let geometry = json!([
        {"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.},
        {"layerDefUid":p.layer(K,1).unwrap()["layerDefUid"],"origin":[500.,500.],"scale":0.25}
    ]);
    let mut group = p
        .copy_selection_group_from_regions(
            K,
            &json!([{"layerIndex":0,"rect":[0,0,2,2],"geometry":geometry[0]}]),
            [0, 0, 16, 16],
        )
        .unwrap();
    group["emptySpace"] = json!(true);
    p.move_selection_group_with_geometry(K, &group, [16., 0.], &geometry, false, false)
        .unwrap();
    // Source loops trunc((16-4)/16)..trunc((31+1-4)/16) = 0..1.
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][0], 0);
    assert_eq!(p.layer(K, 1).unwrap()["intGridCsv"][1], 1);
}

#[test]
fn smart_grid_uses_first_actual_element_even_for_equal_or_subpixel_scaled_grids() {
    let mut p = Project::new();
    p.add_layer("Large", "IntGrid", 32).unwrap();
    p.add_layer("Small", "IntGrid", 16).unwrap();
    for i in 0..2 {
        p.paint_int_grid(K, i, 0, 0, 1).unwrap();
    }
    let geometry = json!([
        {"layerDefUid":p.layer(K,0).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":1.},
        {"layerDefUid":p.layer(K,1).unwrap()["layerDefUid"],"origin":[0.,0.],"scale":0.5}
    ]);
    for (order, raw) in [([0, 1], 16), ([1, 0], 32)] {
        let regions =
            json!(order.map(|i| json!({"layerIndex":i,"rect":[0,0,1,1],"geometry":geometry[i]})));
        let group = p
            .copy_selection_group_from_regions(K, &regions, [0, 0, 32, 32])
            .unwrap();
        assert_eq!(group["gridSize"], raw);
        assert_eq!(group["scaledGridSize"], 16.);
    }
    let mut tiny = Project::new();
    tiny.add_layer("Tiny", "IntGrid", 1).unwrap();
    tiny.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    let regions =
        json!([{"layerIndex":0,"rect":[0,0,1,1],"geometry":{"origin":[0.,0.],"scale":0.25}}]);
    let group = tiny
        .copy_selection_group_from_regions(K, &regions, [0, 0, 1, 1])
        .unwrap();
    assert_eq!(group["scaledGridSize"], 0.25);
}
