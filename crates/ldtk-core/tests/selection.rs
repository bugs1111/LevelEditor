use ldtk_core::{LevelKey, Project};
use serde_json::json;
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
fn tiles() -> Project {
    let mut p = Project::new();
    let ts = p
        .add_tileset(std::path::Path::new("a.png"), 16, 32, 32)
        .unwrap();
    let l = p.add_layer("Tiles", "Tiles", 16).unwrap();
    p.assign_tileset(l, ts).unwrap();
    p.paint_tile(K, 0, 0, 0, 0, [0, 0], 0, false).unwrap();
    p.paint_tile(K, 0, 1, 0, 1, [16, 0], 1, false).unwrap();
    p
}
#[test]
fn clipboard_flip_preserves_metadata_and_tile_stacking() {
    let mut p = tiles();
    p.layer_mut(K, 0).unwrap()["gridTiles"][0]["custom"] = json!(42);
    let clip = p.copy_region(K, 0, [0, 0, 2, 1]).unwrap();
    assert_eq!(clip["width"], 2);
    assert_eq!(
        p.paste_region(K, 0, &clip, 2, 1, true, false, false)
            .unwrap(),
        2
    );
    let ts = p.layer(K, 0).unwrap()["gridTiles"].as_array().unwrap();
    assert!(
        ts.iter()
            .any(|t| t["px"] == json!([48, 16]) && t["t"] == 0 && t["f"] == 1 && t["custom"] == 42)
    );
    p.paste_region(K, 0, &clip, 0, 0, false, false, true)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
#[test]
fn cut_and_move_intgrid_are_atomic_for_overlap() {
    let mut p = Project::new();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    p.paint_int_grid(K, 0, 1, 0, 1).unwrap();
    p.move_region(K, 0, [0, 0, 2, 1], 1, 0, false, false)
        .unwrap();
    assert_eq!(
        &p.layer(K, 0).unwrap()["intGridCsv"].as_array().unwrap()[..4],
        &[json!(0), json!(1), json!(1), json!(0)]
    );
    let clip = p.cut_region(K, 0, [1, 0, 2, 1]).unwrap();
    assert_eq!(clip["intGridCsv"], json!([1, 1]));
    assert_eq!(
        &p.layer(K, 0).unwrap()["intGridCsv"].as_array().unwrap()[..4],
        &[json!(0), json!(0), json!(0), json!(0)]
    );
}
#[test]
fn entity_paste_generates_new_ids_but_move_preserves_identity() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 16).unwrap();
    p.add_entity(K, 0, uid, 0, 0).unwrap();
    let iid = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"].clone();
    let clip = p.copy_region(K, 0, [0, 0, 1, 1]).unwrap();
    p.paste_region(K, 0, &clip, 2, 0, false, false, false)
        .unwrap();
    assert_ne!(p.layer(K, 0).unwrap()["entityInstances"][1]["iid"], iid);
    p.move_region(K, 0, [0, 0, 1, 1], 1, 0, false, false)
        .unwrap();
    assert!(
        p.layer(K, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["iid"] == iid && e["px"] == json!([16, 0]))
    );
}
#[test]
fn incompatible_clipboard_fails_without_cutting_source() {
    let mut p = tiles();
    let old = p.json.clone();
    let mut clip = p.copy_region(K, 0, [0, 0, 2, 1]).unwrap();
    clip["gridSize"] = json!(32);
    assert!(
        p.paste_region(K, 0, &clip, 0, 0, false, false, false)
            .is_err()
    );
    assert_eq!(p.json, old);
}
#[test]
fn tile_stacked_paint_raises_identical_tile_instead_of_duplicating_it() {
    let mut p = tiles();
    p.paint_tile_stacked(K, 0, 0, 0, 1, [16, 0], 0, false, true)
        .unwrap();
    p.paint_tile_stacked(K, 0, 0, 0, 0, [0, 0], 0, false, true)
        .unwrap();
    let tiles = p.layer(K, 0).unwrap()["gridTiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["px"] == json!([0, 0]))
        .collect::<Vec<_>>();
    assert_eq!(tiles.len(), 2);
    assert_eq!(tiles[1]["t"], 0);
}
#[test]
fn batch_intgrid_paint_is_atomic_and_last_write_wins() {
    let mut p = Project::new();
    p.add_layer("Grid", "IntGrid", 16).unwrap();
    assert_eq!(
        p.paint_int_grid_cells(K, 0, &[([0, 0], 1), ([1, 0], 1), ([0, 0], 0)])
            .unwrap(),
        1
    );
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][0], 0);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 1);
    let before = p.json.clone();
    assert!(
        p.paint_int_grid_cells(K, 0, &[([2, 0], 1), ([3, 0], 99)])
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn manual_tile_killer_layer_invalidates_auto_tiles() {
    let mut p = Project::new();
    p.json["levels"][0]["pxWid"] = json!(16);
    p.json["levels"][0]["pxHei"] = json!(16);
    let ts = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    let killer = p.add_layer("Killer", "Tiles", 16).unwrap();
    p.assign_tileset(killer, ts).unwrap();
    let src = p.add_layer("Source", "IntGrid", 16).unwrap();
    p.json["defs"]["layers"][0]["tilesetDefUid"] = json!(ts);
    p.json["defs"]["layers"][0]["autoTilesKilledByOtherLayerUid"] = json!(killer);
    p.json["defs"]["layers"][0]["autoRuleGroups"] = json!([{"uid":100,"active":true,"rules":[{"uid":101,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[0]],"chance":1}]}]);
    p.paint_int_grid_cells(K, 0, &[([0, 0], 1)]).unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["autoLayerTiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    p.paint_tile(K, 1, 0, 0, 0, [0, 0], 0, false).unwrap();
    assert_eq!(p.layer(K, 0).unwrap()["autoLayerTiles"], json!([]));
    p.paint_tile(K, 1, 0, 0, 0, [0, 0], 0, true).unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["autoLayerTiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let _ = src;
}
#[test]
fn stacked_erase_removes_only_top_tile() {
    let mut p = tiles();
    p.paint_tile_stacked(K, 0, 0, 0, 1, [16, 0], 0, false, true)
        .unwrap();
    p.paint_tile_stacked(K, 0, 0, 0, 0, [0, 0], 0, true, true)
        .unwrap();
    let tiles = p.layer(K, 0).unwrap()["gridTiles"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["px"] == json!([0, 0]))
        .collect::<Vec<_>>();
    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0]["t"], 0);
}
#[test]
fn entity_clipboard_respects_scope_limits_and_offsets_copied_point_fields() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(uid), "Path", "Point", true)
        .unwrap();
    p.add_entity(K, 0, uid, 0, 0).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), f, json!([{"cx":1,"cy":1}]))
        .unwrap();
    let clip = p.copy_region(K, 0, [0, 0, 1, 1]).unwrap();
    p.paste_region(K, 0, &clip, 2, 0, false, false, false)
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][0]["__value"][0],
        json!({"cx":3,"cy":1})
    );
    p.update_entity_definition(uid, &json!({"maxCount":2,"limitScope":"PerLevel"}))
        .unwrap();
    let before = p.json.clone();
    assert!(
        p.paste_region(K, 0, &clip, 4, 0, false, false, false)
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
#[ignore = "release hot-path measurement"]
fn batch_intgrid_release_performance() {
    let mut p = Project::new();
    p.resize_level(K, 4096, 4096, 0, 0).unwrap();
    p.add_layer("Cells", "IntGrid", 16).unwrap();
    let cells: Vec<_> = (0..256)
        .flat_map(|y| (0..256).map(move |x| ([x, y], 1)))
        .collect();
    let start = std::time::Instant::now();
    assert_eq!(p.paint_int_grid_cells(K, 0, &cells).unwrap(), 65536);
    eprintln!("65536-cell no-auto batch: {:?}", start.elapsed());
    assert!(
        p.layer(K, 0).unwrap()["intGridCsv"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| *v == 1)
    );
}
