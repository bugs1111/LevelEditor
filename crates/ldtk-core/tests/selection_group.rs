use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};

#[test]
#[ignore = "manual IntGrid clipboard performance comparison"]
fn benchmark_large_intgrid_group_paste() {
    use std::time::{Duration, Instant};

    let mut project = Project::new();
    let side = 128;
    project.resize_level(K, 2 * side * 16, side * 16, 0, 0).unwrap();
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    project.json["defs"]["layers"][0]["intGridValues"] = (1..=256)
        .map(|v| json!({"value":v,"color":"#FFFFFF"})).collect::<Vec<_>>().into();
    let value = |x, y| 193 + (x + y * 3) % 64;
    project.layer_mut(K, 0).unwrap()["intGridCsv"] =
        (0..side).flat_map(|y| (0..2 * side).map(move |x| {
            if x < side { value(x, y) } else { 0 }
        })).collect::<Vec<_>>().into();
    let group = project.copy_selection_group(K, &[0], [0, 0, side * 16, side * 16]).unwrap();
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let mut edited = project.clone();
        let start = Instant::now();
        let count = edited.paste_selection_group(K, &group, [side * 16, 0], true, false, false).unwrap();
        *time = start.elapsed();
        assert_eq!(count, (side * side) as usize);
        let cells = edited.layer(K, 0).unwrap()["intGridCsv"].as_array().unwrap();
        for y in 0..side {
            for x in 0..side {
                let offset = (y * side * 2 + x) as usize;
                assert_eq!(cells[offset], value(x, y));
                assert_eq!(cells[offset + side as usize], value(side - 1 - x, y));
            }
        }
    }
    times.sort_unstable();
    eprintln!("Large IntGrid selection paste: median {:?}; samples {times:?}", times[2]);
}

#[test]
#[ignore = "release point array selection edit benchmark"]
fn benchmark_large_point_selection_edit() {
    use std::time::{Duration, Instant};

    let mut source = Project::new();
    source.resize_level(K, 128 * 16, 128 * 16, 0, 0).unwrap();
    source.add_layer("Actors", "Entities", 16).unwrap();
    let def = source.add_entity_def("Actor", 16, 16).unwrap();
    let field = source.add_field_definition(Some(def), "Path", "Point", true).unwrap();
    let entities = (0..4_000)
        .map(|index| {
            let (x, y) = (index % 64, index / 64);
            json!({
                "iid": format!("entity-{index}"), "defUid": def, "px": [x * 16, 100 * 16],
                "fieldInstances": [{
                    "defUid": field, "realEditorValues": [
                        {"id":"V_String","params":[format!("127,{y}")]},
                        {"id":"V_String","params":[format!("{x},{y}")]}
                    ]
                }]
            })
        })
        .collect();
    source.layer_mut(K, 0).unwrap()["entityInstances"] = Value::Array(entities);
    source.refresh_derived_data().unwrap();
    let group = source.copy_selection_group(K, &[0], [0, 0, 128 * 16, 64 * 16]).unwrap();
    assert_eq!(group["elementCount"], 8_000);
    assert_eq!(group["layers"][0]["clipboard"]["entityInstances"], json!([]));
    let mut move_times = [Duration::ZERO; 5];
    let mut delete_times = move_times;
    for (move_time, delete_time) in move_times.iter_mut().zip(&mut delete_times) {
        let mut moved = source.clone();
        let start = Instant::now();
        let result = moved.move_selection_group_with_result(K, &group, [16, 0], false, false).unwrap();
        *move_time = start.elapsed();
        let points = result["layers"][0]["points"].as_array().unwrap();
        assert_eq!(points.len(), 4_000);
        let mut deleted = source.clone();
        let start = Instant::now();
        assert_eq!(deleted.delete_selection_group(K, &group).unwrap(), 8_000);
        *delete_time = start.elapsed();
        let moved_entities = moved.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap();
        let deleted_entities = deleted.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap();
        assert_eq!((moved_entities.len(), deleted_entities.len()), (4_000, 4_000));
        for (index, ((entity, deleted), point)) in moved_entities.iter().zip(deleted_entities).zip(points).enumerate() {
            let id = format!("entity-{index}");
            let value = json!({"cx":index % 64 + 1,"cy":index / 64});
            assert_eq!(entity["iid"], id);
            assert_eq!(deleted["iid"], id);
            assert_eq!(entity["px"], json!([index % 64 * 16, 100 * 16]));
            assert_eq!(deleted["px"], entity["px"]);
            assert_eq!(entity["fieldInstances"][0]["__value"], json!([value]));
            assert_eq!(deleted["fieldInstances"][0]["__value"], json!([]));
            assert_eq!(point["entityIid"], id);
            assert_eq!(point["arrayIndex"], 0);
            assert_eq!(point["value"], value);
        }
    }
    for (name, times) in [("move", &mut move_times), ("delete", &mut delete_times)] {
        times.sort_unstable();
        eprintln!("Large point selection {name}: median {:?}; samples {times:?}", times[2]);
    }
}

#[test]
#[ignore = "manual entity and point clipboard capture performance comparison"]
fn benchmark_large_entity_selection_capture() {
    use std::time::{Duration, Instant};

    let mut project = Project::new();
    project.resize_level(K, 128 * 16, 128 * 16, 0, 0).unwrap();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let entities = (0..4_000).map(|i| {
        let (x, y) = (i % 64, i / 64);
        let points: Vec<_> = (0..12).map(|point| json!({"cx":x, "cy":100 + point})).collect();
        json!({
            "iid":format!("actor-{i}"), "px":[x * 16, y * 16], "__pivot":[0, 0],
            "fieldInstances":[
                {"defUid":10,"__type":"Array<Point>","__value":points},
                {"defUid":11,"__type":"Point","__value":{"cx":x,"cy":y}}
            ]
        })
    }).collect();
    project.layer_mut(K, 0).unwrap()["entityInstances"] = Value::Array(entities);
    let mut times = [Duration::ZERO; 5];
    for time in &mut times {
        let start = Instant::now();
        let group = project.copy_selection_group(K, &[0], [0, 0, 64 * 16, 64 * 16]).unwrap();
        *time = start.elapsed();
        assert_eq!(group["elementCount"], 8_000);
        let selected = group["layers"][0]["clipboard"]["entityInstances"].as_array().unwrap();
        let originals = project.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap();
        assert_eq!(selected, originals);
        for (i, point) in group["layers"][0]["points"].as_array().unwrap().iter().enumerate() {
            assert_eq!(point["entityIid"], originals[i]["iid"]);
            assert_eq!(point["fieldDefUid"], 11);
            assert_eq!(point["isArray"], false);
            assert_eq!(point["value"], json!({"cx":i % 64,"cy":i / 64}));
        }
    }
    times.sort_unstable();
    eprintln!("Large entity selection capture: median {:?}; samples {times:?}", times[2]);
}

#[test]
#[ignore = "release entity-and-point selection refresh benchmark"]
fn benchmark_large_entity_selection_refresh() {
    use std::time::{Duration, Instant};

    let mut project = Project::new();
    project.resize_level(K, 128 * 16, 64 * 16, 0, 0).unwrap();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let entities = (0..8_000)
        .map(|index| {
            let (x, y) = (index % 128, index / 128);
            json!({
                "iid": format!("entity-{index}"),
                "px": [x * 16, y * 16],
                "fieldInstances": [{
                    "defUid": 24, "__type": "Point", "__value": {"cx": x, "cy": y}
                }]
            })
        })
        .collect();
    project.layer_mut(K, 0).unwrap()["entityInstances"] = Value::Array(entities);
    let selection = project.select_all_group(K, &[0]).unwrap();
    assert_eq!(selection["elementCount"], 16_000);
    let entities = project.layer_mut(K, 0).unwrap()["entityInstances"].as_array_mut().unwrap();
    entities[0]["fieldInstances"][0]["__value"]["cx"] = json!(1);
    entities.reverse();
    let mut expected = selection.clone();
    expected["layers"][0]["clipboard"]["entityInstances"][0]["fieldInstances"][0]["__value"]["cx"] =
        json!(1);
    expected["layers"][0]["points"][0]["value"]["cx"] = json!(1);

    let mut times = [Duration::ZERO; 7];
    for elapsed in &mut times {
        let start = Instant::now();
        let refreshed = project.refresh_selection_group(K, &selection).unwrap();
        *elapsed = start.elapsed();
        assert_eq!(refreshed, expected);
    }
    times.sort_unstable();
    eprintln!("Large entity selection refresh: median {:?}; samples {times:?}", times[3]);
}

#[test]
#[ignore = "release tile clipboard and group move benchmark"]
fn benchmark_large_tile_selection() {
    use std::time::{Duration, Instant};

    let mut source = Project::new();
    source.resize_level(K, 128 * 16, 96 * 16, 0, 0).unwrap();
    let layer = source.add_layer("Art", "Tiles", 16).unwrap();
    let atlas = source
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    source.assign_tileset(layer, atlas).unwrap();
    let tile = |x, y, tag| {
        json!({"px":[x*16,y*16],"src":[0,0],"t":0,"f":0,"a":1,"d":[y*128+x],"tag":tag})
    };
    let original = (0..96)
        .flat_map(|y| (0..128).map(move |x| tile(x, y, x)))
        .collect::<Vec<_>>();
    source.layer_mut(K, 0).unwrap()["gridTiles"] = Value::Array(original.clone());
    let group = source.copy_selection_group(K, &[0], [0, 0, 64 * 16, 96 * 16]).unwrap();
    let clip = &group["layers"][0]["clipboard"];
    let expected = (0..96)
        .flat_map(|y| (0..64).map(move |x| tile(x + 64, y, x)))
        .collect::<Vec<_>>();
    let mut paste_times = [Duration::ZERO; 5];
    let mut move_times = paste_times;
    for (paste_time, move_time) in paste_times.iter_mut().zip(&mut move_times) {
        let mut pasted = source.clone();
        let start = Instant::now();
        assert_eq!(pasted.paste_region(K, 0, clip, 64, 0, false, false, true).unwrap(), expected.len());
        *paste_time = start.elapsed();
        let tiles = pasted.layer(K, 0).unwrap()["gridTiles"].as_array().unwrap();
        let kept = original.iter().filter(|tile| tile["px"][0].as_i64().unwrap() < 64 * 16);
        assert!(tiles.iter().eq(kept.chain(&expected)));

        let mut moved = source.clone();
        let start = Instant::now();
        let next = moved.move_selection_group_with_result(K, &group, [64 * 16, 0], false, false).unwrap();
        *move_time = start.elapsed();
        assert_eq!(next["elementCount"], expected.len());
        assert_eq!(moved.layer(K, 0).unwrap()["gridTiles"].as_array().unwrap(), &expected);
    }
    for (name, times) in [("stack paste", &mut paste_times), ("group move", &mut move_times)] {
        times.sort_unstable();
        eprintln!("Large tile selection {name}: median {:?}; samples {times:?}", times[2]);
    }
}

#[test]
fn moving_selected_entities_preserves_depth_and_removes_only_out_of_bounds_entities() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    p.update_layer_properties(layer, &json!({"entityDepthSorting":"Manual"}))
        .unwrap();
    let actor = p.add_entity_def("Actor", 16, 16).unwrap();
    for x in [32, 80, 128] {
        assert!(p.add_entity(K, 0, actor, x, 32).unwrap());
    }
    let before = p.layer(K, 0).unwrap()["entityInstances"].clone();
    let geometries = json!([{"layerDefUid":layer,"origin":[0.,0.],"scale":1.}]);
    let regions = json!([{"layerIndex":0,"rect":[2,2,1,1],"geometry":geometries[0]}]);
    let group = p
        .copy_selection_group_from_regions(K, &regions, [32, 32, 16, 16])
        .unwrap();
    assert_eq!(group["elementCount"], 1);

    let moved = p
        .drag_selection_group_with_geometry(K, &group, [48., 0.], &geometries, false, true, None)
        .unwrap();
    let mut expected = before.clone();
    expected[0]["px"] = json!([80, 32]);
    expected[0]["__grid"] = json!([5, 2]);
    expected[0]["__worldX"] = json!(80);
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"], expected);

    let outside = p
        .drag_selection_group_with_geometry(K, &moved, [-128., 0.], &geometries, false, true, None)
        .unwrap();
    assert_eq!(outside["elementCount"], 0);
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap(),
        &before.as_array().unwrap()[1..]
    );
}

#[test]
fn moving_a_selection_outside_drops_an_entity_and_its_selected_points_together() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    let actor = p.add_entity_def("Actor", 16, 16).unwrap();
    let point = p
        .add_field_definition(Some(actor), "Target", "Point", false)
        .unwrap();
    p.add_entity(K, 0, actor, 32, 32).unwrap();
    let iid = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&iid), point, json!({"cx":3,"cy":3}))
        .unwrap();
    let group = p.select_all_group(K, &[0]).unwrap();
    assert_eq!(group["elementCount"], 2);
    let geometries = json!([{"layerDefUid":layer,"origin":[0.,0.],"scale":1.}]);
    let (next, report) = p
        .drag_selection_group_with_report(K, &group, [-64., 0.], &geometries, false, false, None)
        .unwrap();
    assert_eq!(report.out_of_bounds, ["Actor"]);
    assert_eq!(next["elementCount"], 0);
    assert_eq!(p.layer(K, 0).unwrap()["entityInstances"], json!([]));
}

#[test]
fn limited_copy_drag_moves_the_original_and_its_point_for_every_policy() {
    for policy in ["PreventAdding", "DiscardOldOnes", "MoveLastOne"] {
        let mut p = Project::new();
        let layer = p.add_layer("Actors", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let point = p
            .add_field_definition(Some(def), "Target", "Point", false)
            .unwrap();
        p.update_entity_definition(
            def,
            &json!({"maxCount":1,"limitBehavior":policy,"limitScope":"PerLevel"}),
        )
        .unwrap();
        p.add_entity(K, 0, def, 32, 32).unwrap();
        let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_value(K, Some(&id), point, json!({"cx":6,"cy":6}))
            .unwrap();
        let geometries = json!([{"layerDefUid":layer,"origin":[0.,0.],"scale":1.}]);
        let regions = json!([{"layerIndex":0,"rect":[2,2,1,1],"geometry":geometries[0]}]);
        let group = p.copy_selection_group_from_regions(K, &regions, [32, 32, 16, 16]).unwrap();
        let (next, report) = p
            .drag_selection_group_with_report(K, &group, [32., 16.], &geometries, true, false, None)
            .unwrap();
        assert_eq!(report.copy_limits, [(1, "Actor".to_owned())]);
        let entities = p.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["iid"], id);
        assert_eq!(entities[0]["px"], json!([64, 48]));
        assert_eq!(entities[0]["fieldInstances"][0]["__value"], json!({"cx":8,"cy":7}));
        assert_eq!(next["layers"][0]["clipboard"]["entityInstances"][0]["iid"], id);
    }
}

#[test]
fn copy_drag_uses_the_available_slots_in_selection_order_and_moves_the_rest() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    p.update_entity_definition(def, &json!({"maxCount":3,"limitScope":"PerLevel"}))
        .unwrap();
    for x in [32, 96] {
        assert!(p.add_entity(K, 0, def, x, 32).unwrap());
    }
    let original = p.layer(K, 0).unwrap()["entityInstances"].clone();
    let group = p.select_all_group(K, &[0]).unwrap();
    let geometries = json!([{"layerDefUid":layer,"origin":[0.,0.],"scale":1.}]);
    let (next, report) = p
        .drag_selection_group_with_report(K, &group, [32., 16.], &geometries, true, false, None)
        .unwrap();
    assert_eq!(report.copy_limits, [(3, "Actor".to_owned())]);
    let entities = p.layer(K, 0).unwrap()["entityInstances"].as_array().unwrap();
    assert_eq!(entities.len(), 3);
    assert_eq!(entities[0], original[0]);
    assert_eq!(entities[1]["iid"], original[1]["iid"]);
    assert_eq!(entities[1]["px"], json!([128, 48]));
    assert_ne!(entities[2]["iid"], original[0]["iid"]);
    assert_eq!(entities[2]["px"], json!([64, 48]));
    assert_eq!(next["elementCount"], 2);
}

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
