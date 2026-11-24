use ldtk_core::{LevelKey, Project, TilePaint};
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
fn stacked_paste_replaces_identical_tiles_in_fifo_order() {
    let mut p = tiles();
    let tile = |x, tag| json!({"px":[x,0],"src":[0,0],"t":0,"f":0,"d":[x/16],"tag":tag});
    p.layer_mut(K, 0).unwrap()["gridTiles"] = json!([
        tile(0, "old first"),
        tile(16, "untouched"),
        tile(0, "old second")
    ]);
    let mut clip = p.copy_region(K, 0, [0, 0, 3, 1]).unwrap();
    clip["gridTiles"] = json!([
        tile(0, "incoming first"),
        tile(32, "new first"),
        tile(0, "incoming second"),
        tile(32, "new last"),
        tile(0, "incoming last")
    ]);
    assert_eq!(
        p.paste_region(K, 0, &clip, 0, 0, false, false, true)
            .unwrap(),
        5
    );
    assert_eq!(
        p.layer(K, 0).unwrap()["gridTiles"],
        json!([
            tile(16, "untouched"),
            tile(0, "incoming second"),
            tile(32, "new last"),
            tile(0, "incoming last")
        ])
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
fn interleaved_tile_stacks_keep_draw_order_and_instance_metadata() {
    let mut project = tiles();
    let original = json!([
        {"px":[16,0],"id":"a"}, {"px":[0,0],"t":1,"src":[16,0],"f":0,"id":"first","a":0.3},
        {"px":[16,0],"id":"b"}, {"px":[0,0],"t":1,"src":[16,0],"f":0,"id":"second"},
        {"px":[16,0],"id":"c"}, {"px":[0,0],"t":2,"id":"top"}
    ]);
    let ids = |p: &Project| {
        p.layer(K, 0).unwrap()["gridTiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tile| tile["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    project.layer_mut(K, 0).unwrap()["gridTiles"] = original.clone();
    project
        .paint_tile(K, 0, 0, 0, 2, [0, 16], 0, false)
        .unwrap();
    assert_eq!(ids(&project), ["a", "first", "b", "c"]);
    assert_eq!(project.layer(K, 0).unwrap()["gridTiles"][1]["a"], 0.3);

    project.layer_mut(K, 0).unwrap()["gridTiles"] = original;
    project
        .paint_tile_stacked(K, 0, 0, 0, 1, [16, 0], 0, false, true)
        .unwrap();
    assert_eq!(ids(&project), ["a", "b", "second", "c", "top", "first"]);
    project
        .paint_tile_stacked(K, 0, 0, 0, 0, [0, 0], 0, true, true)
        .unwrap();
    assert_eq!(ids(&project), ["a", "b", "second", "c", "top"]);
    project.paint_tile(K, 0, 0, 0, 0, [0, 0], 0, true).unwrap();
    assert_eq!(ids(&project), ["a", "b", "c"]);
}

#[test]
fn batch_tile_paint_matches_sequential_draw_order_and_metadata() {
    let mut batch = tiles();
    batch.layer_mut(K, 0).unwrap()["gridTiles"] = json!([
        {"px":[16,0],"t":1,"src":[16,0],"f":1,"marker":"unchanged"},
        {"px":[0,0],"t":1,"src":[16,0],"f":1,"a":0.3,"marker":"first"},
        {"px":[64,0],"t":0,"marker":"outside"},
        {"px":[0,0],"t":1,"src":[16,0],"f":1,"marker":"top"},
        {"px":[48,0],"t":1,"src":[16,0],"f":1,"marker":"raise","a":0.4},
        {"px":[48,0],"t":0,"src":[0,0],"f":1,"marker":"keep"}
    ]);
    let paint = |cell, id, src, stack| TilePaint {
        cell,
        id,
        src,
        stack,
    };
    let edits = [
        paint([4, 1], 0, [0, 0], true),
        paint([1, 0], 1, [16, 0], true),
        paint([0, 0], 1, [16, 0], false),
        paint([2, 1], 2, [0, 16], false),
        paint([3, 0], 1, [16, 0], true),
        paint([-1, 1], 2, [0, 16], false),
    ];
    let mut sequential = batch.clone();
    for &TilePaint {
        cell: [x, y],
        id,
        src,
        stack,
    } in &edits
    {
        sequential
            .paint_tile_stacked(K, 0, x, y, id, src, 1, false, stack)
            .unwrap();
    }
    assert_eq!(
        batch
            .paint_tile_cells_with_deferred(K, 0, &edits, 1, None)
            .unwrap(),
        [0, 2, 3, 4]
    );
    assert_eq!(batch.layer(K, 0), sequential.layer(K, 0));
    let previous = batch.json.clone();
    let invalid = [
        paint([0, 1], 0, [0, 0], true),
        paint([1, 1], 2, [16, 16], false),
    ];
    assert!(
        batch
            .paint_tile_cells_with_deferred(K, 0, &invalid, 1, None)
            .is_err()
    );
    assert_eq!(batch.json, previous);
}

#[test]
fn overlapping_overwrites_keep_order_metadata_and_intermediate_changes() {
    let mut batch = tiles();
    batch.layer_mut(K, 0).unwrap()["gridTiles"][0]["marker"] = json!("preserved");
    let edit = |x, id| TilePaint {
        cell: [x, 0],
        id,
        src: [id % 2 * 16, id / 2 * 16],
        stack: false,
    };
    let edits = [
        edit(0, 0),
        edit(3, 1),
        edit(2, 2),
        edit(3, 0),
        edit(0, 2),
        edit(0, 0),
        edit(3, 0),
    ];
    let mut sequential = batch.clone();
    let mut changes = Vec::new();
    for (
        index,
        &TilePaint {
            cell: [x, y],
            id,
            src,
            ..
        },
    ) in edits.iter().enumerate()
    {
        if sequential
            .paint_tile(K, 0, x, y, id, src, 0, false)
            .unwrap()
        {
            changes.push(index);
        }
    }

    assert_eq!(changes, [1, 2, 3, 4, 5]);
    assert_eq!(
        batch
            .paint_tile_cells_with_deferred(K, 0, &edits, 0, None)
            .unwrap(),
        changes
    );
    assert_eq!(batch.layer(K, 0), sequential.layer(K, 0));
}

#[test]
fn entity_paste_checks_the_selected_limit_scope_before_editing() {
    let mut source = Project::new();
    source.add_layer("Actors", "Entities", 16).unwrap();
    source.add_layer("OtherActors", "Entities", 16).unwrap();
    let def = source.add_entity_def("Actor", 16, 16).unwrap();
    let neighbor = source.add_level(None, "Other", 256, 256).unwrap();
    source.add_entity(K, 1, def, 0, 0).unwrap();
    source.add_entity(neighbor, 0, def, 0, 0).unwrap();
    let clipboard = source.copy_region(K, 1, [0, 0, 1, 1]).unwrap();

    for (scope, limit, allowed) in [
        ("PerLayer", 1, true),
        ("PerLevel", 1, false),
        ("PerLevel", 2, true),
        ("PerWorld", 2, false),
        ("PerWorld", 3, true),
    ] {
        let mut project = source.clone();
        project.json["defs"]["entities"][0]["maxCount"] = json!(limit);
        project.json["defs"]["entities"][0]["limitScope"] = json!(scope);
        let before = project.json.clone();
        let result = project.paste_region(K, 0, &clipboard, 1, 1, false, false, false);
        if allowed {
            assert_eq!(result.unwrap(), 1, "{scope}");
            assert_eq!(
                project.layer(K, 0).unwrap()["entityInstances"][0]["px"],
                json!([16, 16])
            );
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("entity count limit"),
                "{scope}"
            );
            assert_eq!(project.json, before);
        }
    }
}

#[test]
#[ignore = "manual limited-entity paste performance comparison"]
fn benchmark_large_limited_entity_paste() {
    use std::time::{Duration, Instant};

    let mut project = Project::new();
    let layer = project.add_layer("Actors", "Entities", 16).unwrap();
    let def = project.add_entity_def("Actor", 16, 16).unwrap();
    let template = project.json["defs"]["entities"][0].clone();
    let count = 256;
    project.json["defs"]["entities"] = (0..count)
        .map(|index| {
            let mut definition = template.clone();
            definition["uid"] = json!(def + index);
            definition["identifier"] = json!(format!("Actor_{index}"));
            definition["maxCount"] = json!(50);
            definition["limitScope"] =
                json!(["PerLayer", "PerLevel", "PerWorld"][index as usize % 3]);
            definition
        })
        .collect::<Vec<_>>()
        .into();
    let other = project.add_level(None, "Other", 256, 256).unwrap();
    let entities = |level: &str, count| {
        (0..count)
            .map(|i| {
                json!({
                    "iid":format!("{level}-{i}"),"defUid":def + i % 256,"px":[0,0],
                    "fieldInstances":[],"width":16,"height":16
                })
            })
            .collect::<Vec<_>>()
    };
    project.layer_mut(K, 0).unwrap()["entityInstances"] = entities("first", 4_096).into();
    project.layer_mut(other, 0).unwrap()["entityInstances"] = entities("second", 4_096).into();
    let clip = json!({
        "type":"Entities","gridSize":16,"width":1,"height":1,"origin":[0,0],
        "entityInstances":entities("clipboard", 1_024)
    });
    assert_eq!(project.layer(K, 0).unwrap()["layerDefUid"], layer);
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let mut edited = project.clone();
        let start = Instant::now();
        assert_eq!(
            edited
                .paste_region(K, 0, &clip, 2, 1, false, false, false)
                .unwrap(),
            1_024
        );
        *time = start.elapsed();
        let placed = edited.layer(K, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(placed.len(), 5_120);
        let mut ids = std::collections::HashSet::new();
        for (i, entity) in placed.iter().enumerate() {
            assert!(ids.insert(entity["iid"].as_str().unwrap()));
            assert_eq!(entity["defUid"], def + i as i64 % 256);
            assert_eq!(
                entity["px"],
                if i < 4_096 {
                    json!([0, 0])
                } else {
                    json!([32, 16])
                }
            );
        }
        assert_eq!(
            edited.layer(other, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            4_096
        );
    }
    times.sort_unstable();
    eprintln!(
        "Large limited entity paste: median {:?}; samples {times:?}",
        times[2]
    );
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
    let changes = [
        ([1, 0], 0),
        ([0, 0], 1),
        ([1, 0], 1),
        ([0, 0], 0),
        ([1, 0], 0),
    ];
    assert_eq!(p.paint_int_grid_cells(K, 0, &changes).unwrap(), 1);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][0], 0);
    assert_eq!(p.layer(K, 0).unwrap()["intGridCsv"][1], 0);
    assert_eq!(p.paint_int_grid_cells(K, 0, &changes).unwrap(), 0);
    let before = p.json.clone();
    assert!(
        p.paint_int_grid_cells(K, 0, &[([2, 0], 1), ([3, 0], 99), ([3, 0], 0)])
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
    p.json["defs"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("layerUidsPreventingAutoTilingHere");
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
