use ldtk_core::{DeferredAutoLayers, LevelKey, Project};
use serde_json::{Value, json};
fn fixture() -> (Project, LevelKey) {
    let mut p = Project::new();
    let k = p.level_keys()[0];
    p.json["defs"]["layers"] = json!([{"uid":1,"type":"IntGrid","gridSize":16,"tilesetDefUid":2,"intGridValues":[{"value":1,"groupUid":0}],"autoRuleGroups":[{"uid":3,"active":true,"rules":[{"uid":4,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[2]],"chance":1,"breakOnMatch":true}]}]}]);
    p.json["defs"]["tilesets"] =
        json!([{"uid":2,"tileGridSize":16,"pxWid":64,"pxHei":64,"padding":0,"spacing":0}]);
    p.level_mut(k).unwrap()["layerInstances"] = json!([{"layerDefUid":1,"__cWid":3,"__cHei":2,"__gridSize":16,"seed":100,"intGridCsv":[1,0,1,0,1,0],"autoLayerTiles":[]}]);
    (p, k)
}
fn tiles(p: &Project, k: LevelKey) -> &Value {
    &p.level(k).unwrap()["layerInstances"][0]["autoLayerTiles"]
}

#[test]
fn baking_intgrid_flattens_tiles_and_handles_original_by_mode() {
    use ldtk_core::BakeMode;

    let (mut fixture, key) = fixture();
    fixture.json["defs"]["layers"][0]["identifier"] = json!("Terrain");
    fixture.json["defs"]["layers"][0]["pxOffsetX"] = json!(3);
    fixture.json["defs"]["layers"][0]["autoRuleGroups"]
        .as_array_mut()
        .unwrap()
        .push(json!({"uid":5,"isOptional":true,"rules":[{"uid":6}]}));
    fixture.json["defs"]["tilesets"][0]["cachedPixelData"]["opaqueTiles"] =
        json!("0010000000000000");
    let source = &mut fixture.level_mut(key).unwrap()["layerInstances"][0];
    source["optionalRules"] = json!([5]);
    source["pxOffsetX"] = json!(8);
    source["autoLayerTiles"] = json!([
        {"px":[0,0],"t":1,"f":0,"d":[4,0]},
        {"px":[0,0],"t":2,"f":0,"d":[4,1],"a":0.5},
        {"px":[0,0],"t":3,"f":0,"d":[4,2]},
        {"px":[-1,0],"t":3,"f":1,"d":[4,3]},
        {"px":[0,0],"t":3,"f":0,"d":[4,4]},
        {"px":[0,0],"t":1,"f":0,"d":[6,0]}
    ]);
    let source = source.clone();

    for mode in [BakeMode::Keep, BakeMode::Delete, BakeMode::Empty] {
        let mut p = fixture.clone();
        let uid = p.bake_auto_layer(1, mode, |_, _| {}).unwrap();
        let defs = p.json["defs"]["layers"].as_array().unwrap();
        assert_eq!(p.json["nextUid"], uid + 5);
        let last = defs.last().unwrap();
        assert_eq!(last["identifier"], "Terrain_baked");
        assert_eq!(last["type"], "Tiles");
        assert_eq!(last["autoRuleGroups"], json!([]));
        assert_eq!(
            last["intGridValues"],
            fixture.json["defs"]["layers"][0]["intGridValues"]
        );
        let instances = p.level(key).unwrap()["layerInstances"].as_array().unwrap();
        let baked = instances.last().unwrap();
        assert_eq!(baked["layerDefUid"], uid);
        assert_eq!(baked["__pxTotalOffsetX"], 3);
        assert_eq!(baked["optionalRules"], json!([]));
        assert_eq!(
            baked["gridTiles"],
            json!([
                {"px":[0,0],"src":[32,0],"t":2,"f":0,"d":[0],"a":1},
                {"px":[0,0],"src":[48,0],"t":3,"f":1,"d":[0],"a":1},
                {"px":[0,0],"src":[48,0],"t":3,"f":0,"d":[0],"a":1}
            ])
        );
        match mode {
            BakeMode::Keep => {
                assert_eq!(instances[0]["intGridCsv"], source["intGridCsv"]);
                assert_eq!(instances[0]["autoLayerTiles"], source["autoLayerTiles"]);
            }
            BakeMode::Delete => assert_eq!(instances.len(), 1),
            BakeMode::Empty => {
                assert_eq!(instances[0]["layerDefUid"], 1);
                assert_eq!(instances[0]["intGridCsv"], json!([0, 0, 0, 0, 0, 0]));
                assert_eq!(instances[0]["autoLayerTiles"], json!([]));
            }
        }
    }
    fixture.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"uid":7,"type":"AutoLayer","autoSourceLayerDefUid":1}));
    let before = fixture.json.clone();
    assert!(
        fixture
            .bake_auto_layer(1, BakeMode::Keep, |_, _| {})
            .is_err()
    );
    assert_eq!(fixture.json, before);
}

#[test]
fn bake_applies_to_external_levels_and_undo_restores_them() {
    use ldtk_core::{BakeMode, History};

    let mut p = Project::new();
    let other = p.add_world("Other").unwrap();
    p.add_level(Some(other), "Room", 32, 32).unwrap();
    let source = p.add_layer("Walls", "IntGrid", 16).unwrap();
    let atlas = p
        .add_tileset(std::path::Path::new("tiles.png"), 16, 64, 64)
        .unwrap();
    let rule = atlas + 2;
    let def = &mut p.json["defs"]["layers"][0];
    def["tilesetDefUid"] = json!(atlas);
    def["autoRuleGroups"] = json!([{"uid":atlas+1,"active":true,"rules":[{"uid":rule}]}]);
    let keys = p.level_keys();
    assert_eq!(keys.len(), 2);
    for (i, &key) in keys.iter().enumerate() {
        let layer = p.layer_mut(key, 0).unwrap();
        layer["intGridCsv"][i] = json!(1);
        layer["autoLayerTiles"] =
            json!([{"px":[i*16,0],"src":[0,16],"t":4,"f":0,"d":[rule,i],"a":1}]);
    }
    p.refresh_derived_data().unwrap();
    p.set_external_levels(true).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("External.ldtk");
    p.save(&path).unwrap();
    let mut p = Project::load(&path).unwrap();
    let before = p.clone();
    let mut history = History::default();
    history.begin_project(&p);
    let mut progress = Vec::new();
    let baked = p
        .bake_auto_layer(source, BakeMode::Empty, |done, total| {
            progress.push((done, total));
        })
        .unwrap();
    assert_eq!(progress, [(0, 2), (1, 2), (2, 2)]);
    assert!(history.commit(&p));
    assert!(history.undo(&mut p));
    assert_eq!(p.json, before.json);
    for &key in &keys {
        assert_eq!(p.level(key), before.level(key));
    }
    assert!(history.redo(&mut p));
    p.save(&path).unwrap();
    let reloaded = Project::load(&path).unwrap();
    for (i, key) in keys.into_iter().enumerate() {
        let original = reloaded.layer(key, 0).unwrap();
        assert!(
            original["intGridCsv"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v == 0)
        );
        let output = reloaded.layer(key, 1).unwrap();
        assert_eq!(output["layerDefUid"], baked);
        assert_eq!(
            output["gridTiles"],
            json!([
                {"px":[i*16,0],"src":[0,16],"t":4,"f":0,"d":[i],"a":1}
            ])
        );
    }
}

#[test]
#[ignore = "requires LDtk 1.5.4 bake captures"]
fn baked_layers_match_ldtk_154_saved_projects() {
    use ldtk_core::BakeMode;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools");
    let input = root.join("ldtk-154/autoTileBlocking.ldtk");
    let captures = root.join("compare-154/auto-layer-settings");
    for (name, source, mode) in [
        ("keep", 14, BakeMode::Keep),
        ("delete", 14, BakeMode::Delete),
        ("empty", 27, BakeMode::Empty),
        ("intgrid-delete", 27, BakeMode::Delete),
    ] {
        let mut actual = Project::load(&input).unwrap();
        actual.migrate_to_current().unwrap();
        let expected =
            Project::load(captures.join(format!("source-bake-{name}-saved.ldtk"))).unwrap();
        let uid = actual.bake_auto_layer(source, mode, |_, _| {}).unwrap();
        assert_eq!(actual.json["nextUid"], expected.json["nextUid"], "{name}");
        assert_eq!(
            actual.layer_def(uid).unwrap(),
            expected.layer_def(uid).unwrap(),
            "{name} baked definition"
        );
        for key in expected.level_keys() {
            let actual = actual.level(key).unwrap()["layerInstances"]
                .as_array()
                .unwrap();
            let expected = expected.level(key).unwrap()["layerInstances"]
                .as_array()
                .unwrap();
            assert_eq!(actual.len(), expected.len(), "{name}");
            for (actual, expected) in actual.iter().zip(expected) {
                for attr in ["layerDefUid", "intGridCsv", "gridTiles", "autoLayerTiles"] {
                    assert_eq!(
                        actual[attr], expected[attr],
                        "{name} {} {attr}",
                        expected["__identifier"]
                    );
                }
            }
        }
        assert_eq!(
            actual
                .layer_def(14)
                .map(|d| &d["layerUidsPreventingAutoTilingHere"]),
            expected
                .layer_def(14)
                .map(|d| &d["layerUidsPreventingAutoTilingHere"]),
            "{name} blocker references"
        );
    }
}
#[test]
fn single_rule_emits_canonical_tiles() {
    let (mut p, k) = fixture();
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        tiles(&p, k),
        &json!([
 {"px":[0,0],"src":[32,0],"f":0,"t":2,"d":[4,0],"a":1},
 {"px":[32,0],"src":[32,0],"f":0,"t":2,"d":[4,2],"a":1},
 {"px":[16,16],"src":[32,0],"f":0,"t":2,"d":[4,4],"a":1}])
    );
}
#[test]
#[ignore = "requires pinned upstream examples"]
fn official_examples_saved_autotiles() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/app/extraFiles/samples");
    let mut total = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "ldtk") {
            continue;
        }
        let mut p = Project::load(&path).unwrap();
        for k in p.level_keys() {
            let old = p.level(k).unwrap().clone();
            for layer in p.level_mut(k).unwrap()["layerInstances"]
                .as_array_mut()
                .unwrap()
            {
                layer["autoLayerTiles"] = json!([]);
            }
            p.rebuild_auto_layers(k).unwrap();
            for (i, layer) in old["layerInstances"].as_array().unwrap().iter().enumerate() {
                let actual = &p.level(k).unwrap()["layerInstances"][i]["autoLayerTiles"];
                assert_eq!(
                    actual,
                    &layer["autoLayerTiles"],
                    "{} {:?} layer {}",
                    path.display(),
                    k,
                    i
                );
                total += actual.as_array().unwrap().len();
            }
        }
    }
    eprintln!("Compared {total} saved auto tiles");
}

#[test]
fn invalid_rule_failure_is_atomic() {
    let (mut p, k) = fixture();
    p.level_mut(k).unwrap()["layerInstances"][0]["autoLayerTiles"] = json!([{"sentinel":true}]);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["xModulo"] = json!(0);
    let before = p.json.clone();
    assert!(p.rebuild_auto_layers(k).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn late_auto_layer_failure_keeps_earlier_cache_and_source_cells() {
    let (mut p, key) = fixture();
    p.layer_mut(key, 0).unwrap()["__type"] = json!("IntGrid");
    p.rebuild_auto_layers(key).unwrap();
    let mut definition = p.json["defs"]["layers"][0].clone();
    definition["uid"] = json!(7);
    definition["type"] = json!("AutoLayer");
    definition["autoSourceLayerDefUid"] = json!(1);
    definition["autoRuleGroups"][0]["rules"][0]["xModulo"] = json!(0);
    p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .push(definition);
    let mut derived = p.layer(key, 0).unwrap().clone();
    derived["layerDefUid"] = json!(7);
    derived["__type"] = json!("AutoLayer");
    p.level_mut(key).unwrap()["layerInstances"]
        .as_array_mut()
        .unwrap()
        .push(derived);

    let before = p.json.clone();
    assert!(p.paint_int_grid(key, 0, 1, 0, 1).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn incremental_rebuild_matches_full_after_source_edits() {
    let (mut p, k) = fixture();
    p.rebuild_auto_layers(k).unwrap();
    p.level_mut(k).unwrap()["layerInstances"][0]["intGridCsv"][1] = json!(1);
    let mut expected = p.clone();
    expected.rebuild_auto_layers(k).unwrap();
    p.rebuild_auto_layers_area(k, 1, [1, 0, 1, 1]).unwrap();
    assert_eq!(p.json, expected.json);
}
#[test]
fn incremental_paint_rebuilds_four_cells_around_short_patterns() {
    let (mut fixture, key) = fixture();
    fixture.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([-1]);
    let outside = json!({"px":[0,0],"src":[16,0],"t":1,"f":0,"d":[4,0],"a":1});
    let layer = fixture.layer_mut(key, 0).unwrap();
    layer["__type"] = json!("IntGrid");
    layer["__cWid"] = json!(16);
    layer["__cHei"] = json!(16);
    layer["intGridCsv"] = json!(vec![0; 16 * 16]);
    layer["autoLayerTiles"] = json!([outside]);

    let expected = std::iter::once(0)
        .chain((4..=12).flat_map(|y| (4..=12).map(move |x| y * 16 + x)))
        .filter(|&id| id != 8 * 16 + 8)
        .collect::<Vec<_>>();
    for deferred in [false, true] {
        let mut project = fixture.clone();
        project.json["defs"]["layers"][0]["useAsyncRender"] = json!(deferred);
        let mut pending = DeferredAutoLayers::default();
        project
            .paint_int_grid_cells_with_deferred(
                key,
                0,
                &[([8, 8], 1)],
                deferred.then_some(&mut pending),
            )
            .unwrap();
        if deferred {
            assert!(!pending.is_empty());
            assert_eq!(tiles(&project, key), &json!([outside]));
            project.settle_auto_layers(key, &mut pending).unwrap();
        }
        let generated = tiles(&project, key).as_array().unwrap();
        let cells = generated
            .iter()
            .map(|tile| tile["d"][1].as_i64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cells, expected, "deferred = {deferred}");
        assert_eq!(generated[0], outside);
    }
}
#[test]
fn distant_paint_cells_preserve_the_auto_tiles_between_them() {
    let (mut fixture, key) = fixture();
    let layer = fixture.layer_mut(key, 0).unwrap();
    layer["__type"] = json!("IntGrid");
    layer["__cWid"] = json!(30);
    layer["__cHei"] = json!(1);
    let mut grid = vec![0; 30];
    grid[15] = 1;
    layer["intGridCsv"] = json!(grid);
    let preserved = json!({"px":[240,0],"src":[32,0],"t":2,"f":0,"d":[4,15],"a":0.5});
    layer["autoLayerTiles"] = json!([preserved]);
    let edits = [([1, 0], 1), ([29, 0], 1)];

    let mut sequential = fixture.clone();
    for ([x, y], value) in edits {
        sequential.paint_int_grid(key, 0, x, y, value).unwrap();
    }
    let expected = tiles(&sequential, key);
    assert_eq!(expected.as_array().unwrap().len(), 3);
    assert_eq!(expected[1], preserved);

    for (deferred, split) in [(false, false), (true, false), (true, true)] {
        let mut project = fixture.clone();
        project.json["defs"]["layers"][0]["useAsyncRender"] = json!(deferred);
        let mut pending = DeferredAutoLayers::default();
        for batch in edits.chunks(if split { 1 } else { edits.len() }) {
            assert_eq!(
                project
                    .paint_int_grid_cells_with_deferred(
                        key,
                        0,
                        batch,
                        deferred.then_some(&mut pending),
                    )
                    .unwrap(),
                batch.len()
            );
        }
        if deferred {
            assert_eq!(tiles(&project, key), &json!([preserved]));
            project.settle_auto_layers(key, &mut pending).unwrap();
        }
        assert_eq!(
            tiles(&project, key),
            expected,
            "deferred = {deferred}, split = {split}"
        );
    }
}

#[test]
fn missing_cache_area_update_builds_entire_layer() {
    let (mut p, k) = fixture();
    p.level_mut(k).unwrap()["layerInstances"][0]
        .as_object_mut()
        .unwrap()
        .remove("autoLayerTiles");
    let mut full = p.clone();
    full.rebuild_auto_layers(k).unwrap();
    p.rebuild_auto_layers_area(k, 1, [0, 0, 1, 1]).unwrap();
    assert_eq!(tiles(&p, k), tiles(&full, k));
}
#[test]
fn group_value_and_negative_anything_patterns_match() {
    let (mut p, k) = fixture();
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([1000]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([-1000001]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["d"][1].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [1, 3, 5]
    );
}
#[test]
fn flipped_asymmetric_rules_and_break_order() {
    let (mut p, k) = fixture();
    let r = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0];
    r["size"] = json!(3);
    r["pattern"] = json!([0, 0, 0, 1, -1000001, 0, 0, 0, 0]);
    r["flipX"] = json!(true);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t["d"][1].as_i64().unwrap(), t["f"].as_i64().unwrap()))
            .collect::<Vec<_>>(),
        [(1, 0), (3, 1), (5, 0)]
    );
}
#[test]
fn opaque_top_rule_culls_lower_rules_even_without_break_flag() {
    let (mut p, k) = fixture();
    let mut second = p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0].clone();
    second["uid"] = json!(5);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"]
        .as_array_mut()
        .unwrap()
        .push(second);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["breakOnMatch"] = json!(false);
    p.json["defs"]["tilesets"][0]["cachedPixelData"] = json!({"opaqueTiles":"0010000000000000"});
    p.rebuild_auto_layers(k).unwrap();
    assert!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["d"][0] == 4)
    );
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["tileXOffset"] = json!(1);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 6);
    assert_eq!(tiles(&p, k)[0]["d"][0], 5);
}
#[test]
fn killer_layer_suppresses_anchor_and_area_update_restores_it() {
    let (mut p, k) = fixture();
    p.json["defs"]["layers"][0]["autoTilesKilledByOtherLayerUid"] = json!(9);
    p.level_mut(k).unwrap()["layerInstances"].as_array_mut().unwrap().push(json!({"layerDefUid":9,"__gridSize":16,"__cWid":3,"__cHei":2,"gridTiles":[{"px":[0,0],"t":1}]}));
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 2);
    p.level_mut(k).unwrap()["layerInstances"][1]["gridTiles"] = json!([]);
    p.rebuild_auto_layers_area(k, 9, [0, 0, 1, 1]).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
}

#[test]
fn biome_gating_precedes_optional_switch_and_supports_and_mode() {
    let (mut p, k) = fixture();
    p.json["defs"]["layers"][0]["biomeFieldUid"] = json!(10);
    let group = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0];
    group["isOptional"] = json!(true);
    group["requiredBiomeValues"] = json!(["Forest", "Snow"]);
    group["biomeRequirementMode"] = json!(1);
    p.level_mut(k).unwrap()["fieldInstances"] = json!([{"defUid":10,"__value":["Forest"]}]);
    p.rebuild_auto_layers(k).unwrap();
    assert!(tiles(&p, k).as_array().unwrap().is_empty());
    p.level_mut(k).unwrap()["fieldInstances"][0]["__value"] = json!(["Forest", "Snow"]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["active"] = json!(false);
    p.rebuild_auto_layers(k).unwrap();
    assert!(tiles(&p, k).as_array().unwrap().is_empty());
    // Upstream's optionalRules presence overrides the group's inactive flag.
    p.level_mut(k).unwrap()["layerInstances"][0]["optionalRules"] = json!([3]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
}
#[test]
fn checker_offsets_and_out_of_bounds_follow_original_semantics() {
    let (mut p, k) = fixture();
    p.level_mut(k).unwrap()["layerInstances"][0]["intGridCsv"] = json!([1, 1, 1, 1, 1, 1]);
    let r = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0];
    r["xModulo"] = json!(2);
    r["checker"] = json!("Horizontal");
    r["xOffset"] = json!(1);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["d"][1].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [0, 2, 4]
    );
    let r = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0];
    r["checker"] = json!("None");
    r["xModulo"] = json!(1);
    r["size"] = json!(3);
    r["pattern"] = json!([1, 1, 1, 1, 1, 1, 1, 1, 1]);
    p.rebuild_auto_layers(k).unwrap();
    assert!(tiles(&p, k).as_array().unwrap().is_empty());
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["outOfBoundsValue"] = json!(1);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 6);
}
#[test]
fn separate_auto_layer_uses_source_grid_and_override_atlas() {
    let (mut p, k) = fixture();
    let mut auto = p.json["defs"]["layers"][0].clone();
    auto["uid"] = json!(8);
    auto["type"] = json!("AutoLayer");
    auto["autoSourceLayerDefUid"] = json!(1);
    p.json["defs"]["layers"].as_array_mut().unwrap().push(auto);
    let mut atlas = p.json["defs"]["tilesets"][0].clone();
    atlas["uid"] = json!(9);
    atlas["padding"] = json!(1);
    atlas["spacing"] = json!(2);
    p.json["defs"]["tilesets"]
        .as_array_mut()
        .unwrap()
        .push(atlas);
    let mut layer = p.level(k).unwrap()["layerInstances"][0].clone();
    layer["layerDefUid"] = json!(8);
    layer["intGridCsv"] = json!([]);
    layer["overrideTilesetUid"] = json!(9);
    p.level_mut(k).unwrap()["layerInstances"]
        .as_array_mut()
        .unwrap()
        .push(layer);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        p.level(k).unwrap()["layerInstances"][1]["autoLayerTiles"][0]["src"],
        json!([37, 1])
    );
    p.level_mut(k).unwrap()["layerInstances"][0]["intGridCsv"][1] = json!(1);
    let mut full = p.clone();
    full.rebuild_auto_layers(k).unwrap();
    p.rebuild_auto_layers_area(k, 1, [1, 0, 1, 1]).unwrap();
    assert_eq!(p.json, full.json);
}
#[test]
#[ignore = "requires pinned upstream examples"]
fn official_examples_incremental_matches_full_rebuild() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/app/extraFiles/samples");
    let mut checked = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ldtk") {
            continue;
        }
        let mut p = Project::load(path).unwrap();
        for key in p.level_keys() {
            let sources = p.level(key).unwrap()["layerInstances"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, l)| l["__type"] == "IntGrid")
                .map(|(i, l)| {
                    (
                        i,
                        l["layerDefUid"].as_i64().unwrap(),
                        l["__cWid"].as_i64().unwrap() as i32,
                        l["__cHei"].as_i64().unwrap() as i32,
                    )
                })
                .collect::<Vec<_>>();
            for (idx, uid, w, h) in sources {
                if w == 0 || h == 0 {
                    continue;
                }
                p.rebuild_auto_layers(key).unwrap();
                let x = w / 2;
                let y = h / 2;
                let csv = &mut p.level_mut(key).unwrap()["layerInstances"][idx]["intGridCsv"];
                let current = csv[(x + y * w) as usize].as_i64().unwrap();
                csv[(x + y * w) as usize] = json!(if current == 0 { 1 } else { 0 });
                let mut full = p.clone();
                full.rebuild_auto_layers(key).unwrap();
                p.rebuild_auto_layers_area(key, uid, [x, y, 1, 1]).unwrap();
                assert_eq!(p.level(key), full.level(key));
                checked += 1;
            }
        }
    }
    eprintln!("Verified {checked} incremental source edits across official examples");
}

#[test]
#[ignore = "release paint benchmark for the pinned upstream large world fixture"]
fn benchmark_large_world_intgrid_paint() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let mut project = Project::load(path).unwrap();
    let key = project.level_keys()[3];
    project.rebuild_auto_layers(key).unwrap();
    let index = project.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .position(|layer| layer["__identifier"] == "Walls")
        .unwrap();
    let layer = project.layer(key, index).unwrap();
    assert!(layer["autoLayerTiles"].as_array().unwrap().len() > 30_000);
    let width = layer["__cWid"].as_i64().unwrap() as i32;
    let height = layer["__cHei"].as_i64().unwrap() as i32;
    let [x, y] = [width / 2, height / 2];
    let cell = (x + y * width) as usize;
    let initial = layer["intGridCsv"][cell].as_i64().unwrap() as i32;
    let replacement = i32::from(initial == 0);
    let mut full = project.clone();
    let mut paint_times = [std::time::Duration::ZERO; 7];
    let mut full_times = paint_times;
    for (step, (paint_time, full_time)) in paint_times.iter_mut().zip(&mut full_times).enumerate() {
        let value = if step % 2 == 0 { replacement } else { initial };
        let start = std::time::Instant::now();
        assert!(project.paint_int_grid(key, index, x, y, value).unwrap());
        *paint_time = start.elapsed();
        full.layer_mut(key, index).unwrap()["intGridCsv"][cell] = json!(value);
        let start = std::time::Instant::now();
        full.rebuild_auto_layers(key).unwrap();
        *full_time = start.elapsed();
        assert_eq!(project.level(key), full.level(key));
    }
    paint_times.sort_unstable();
    full_times.sort_unstable();
    eprintln!(
        "Large world IntGrid paint: median {:?}; samples {paint_times:?}",
        paint_times[3]
    );
    eprintln!(
        "Large world full auto rebuild: median {:?}; samples {full_times:?}",
        full_times[3]
    );
}

#[test]
#[ignore = "release performance measurement; requires pinned upstream examples"]
fn autotile_release_performance() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/app/extraFiles/samples/Typical_TopDown_example.ldtk");
    let mut p = Project::load(path).unwrap();
    let key = p.level_keys()[0];
    let start = std::time::Instant::now();
    for _ in 0..20 {
        p.rebuild_auto_layers(key).unwrap();
    }
    let full = start.elapsed();
    let source = p.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["__type"] == "IntGrid")
        .unwrap()["layerDefUid"]
        .as_i64()
        .unwrap();
    let start = std::time::Instant::now();
    for _ in 0..20 {
        p.rebuild_auto_layers_area(key, source, [15, 15, 1, 1])
            .unwrap();
    }
    let area = start.elapsed();
    eprintln!("Typical_TopDown first level, 20 iterations: full={full:?}, area={area:?}");
    let mut expected = p.clone();
    expected.rebuild_auto_layers(key).unwrap();
    assert_eq!(p.level(key), expected.level(key));
}

#[test]
fn legacy_sparse_intgrid_values_are_zero_based() {
    let (mut p, k) = fixture();
    p.level_mut(k).unwrap()["layerInstances"][0]
        .as_object_mut()
        .unwrap()
        .remove("intGridCsv");
    p.level_mut(k).unwrap()["layerInstances"][0]["intGrid"] =
        json!([{"coordId":0,"v":0},{"coordId":2,"v":0},{"coordId":4,"v":0}]);
    p.json["defs"]["layers"][0]["intGridValues"] =
        json!([{"identifier":"Walls","color":"#000000"}]);
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["pattern"] = json!([1000]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
}

#[test]
fn checker_axis_is_normalized_when_its_modulo_is_one() {
    let (mut p, k) = fixture();
    p.level_mut(k).unwrap()["layerInstances"][0]["intGridCsv"] = json!([1, 1, 1, 1, 1, 1]);
    let r = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0];
    r["checker"] = json!("Horizontal");
    r["xModulo"] = json!(1);
    r["yModulo"] = json!(2);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["d"][1].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [0, 2, 4]
    );
}
#[test]
#[ignore = "requires pinned upstream historical fixtures"]
fn historical_random_caches_rebuild_to_compiled_153_values() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference/ldtk/tests");
    for (name, layer, rule, cell, key, expected) in [
        ("gridSizeChange.ldtk", 3, 12, 380, "t", json!(213)),
        ("ruleRandomOffsets.ldtk", 0, 6, 878, "px", json!([73, 88])),
    ] {
        let mut p = Project::load(root.join(name)).unwrap();
        let k = p.level_keys()[0];
        p.rebuild_auto_layers(k).unwrap();
        let tile = p.level(k).unwrap()["layerInstances"][layer]["autoLayerTiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["d"] == json!([rule, cell]))
            .unwrap();
        assert_eq!(tile[key], expected);
    }
}

#[test]
fn rule_crud_rebuilds_tiles_and_preserves_unknown_fields() {
    let (mut p, k) = fixture();
    let group = p.add_auto_rule_group(1, "Background").unwrap();
    let rule = p.add_auto_rule(1, group, 1).unwrap();
    assert_ne!(group, rule);
    assert!(rule > 4);
    p.update_auto_rule(
        1,
        group,
        rule,
        &json!({"pattern":[-1000001],"tileRectsIds":[[3]],"futureFlag":{"preserve":true}}),
    )
    .unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 6);
    p.update_auto_rule_group(1, group, &json!({"active":false}))
        .unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
    p.update_auto_rule_group(1, group, &json!({"active":true}))
        .unwrap();
    let saved = &p.json["defs"]["layers"][0]["autoRuleGroups"][1]["rules"][0];
    assert_eq!(saved["futureFlag"], json!({"preserve":true}));
    p.remove_auto_rule(1, group, rule).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
    p.remove_auto_rule_group(1, group).unwrap();
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn rule_edit_rejects_invalid_patterns_and_uid_changes_atomically() {
    let (mut p, _) = fixture();
    let before = p.json.clone();
    assert!(p.add_auto_rule(1, 3, 2).is_err());
    assert_eq!(p.json, before);
    assert!(p.update_auto_rule(1, 3, 4, &json!({"uid":1})).is_err());
    assert_eq!(p.json, before);
    assert!(
        p.update_auto_rule(1, 3, 4, &json!({"size":3,"pattern":[1]}))
            .is_err()
    );
    assert_eq!(p.json, before);
    assert!(
        p.update_auto_rule(1, 3, 4, &json!({"tileRectsIds":[[-1]]}))
            .is_err()
    );
    assert_eq!(p.json, before);
    assert!(
        p.update_auto_rule_group(1, 3, &json!({"rules":[]}))
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn rule_and_group_reordering_updates_break_on_match_precedence() {
    let (mut p, k) = fixture();
    let group = p.add_auto_rule_group(1, "Top").unwrap();
    let first = p.add_auto_rule(1, group, 1).unwrap();
    let second = p.add_auto_rule(1, group, 1).unwrap();
    for (uid, tile) in [(first, 3), (second, 4)] {
        p.update_auto_rule(
            1,
            group,
            uid,
            &json!({"pattern":[1],"tileRectsIds":[[tile]]}),
        )
        .unwrap();
    }
    p.reorder_auto_rule_group(1, group, 0).unwrap();
    assert!(tiles(&p, k).as_array().unwrap().iter().all(|t| t["t"] == 3));
    p.reorder_auto_rule(1, group, second, 0).unwrap();
    assert!(tiles(&p, k).as_array().unwrap().iter().all(|t| t["t"] == 4));
    let before = p.json.clone();
    assert!(p.reorder_auto_rule(1, group, second, 50).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn editing_legacy_rule_tiles_migrates_old_tile_ids() {
    let (mut p, k) = fixture();
    let r = &mut p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0];
    r.as_object_mut().unwrap().remove("tileRectsIds");
    r["tileIds"] = json!([2]);
    p.update_auto_rule(1, 3, 4, &json!({"chance":0.9})).unwrap();
    p.update_auto_rule(1, 3, 4, &json!({"chance":1,"tileRectsIds":[[3]]}))
        .unwrap();
    assert!(
        tiles(&p, k)
            .as_array()
            .unwrap()
            .iter()
            .all(|tile| tile["t"] == 3)
    );
    assert!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]
            .get("tileIds")
            .is_none()
    );
}

#[test]
fn wizard_batch_inserts_first_trims_symmetric_patterns_and_rebuilds_atomically() {
    let (mut p, key) = fixture();
    let rule = json!({"size":3,"pattern":[0,0,0,0,1,0,0,0,0],"tileRectsIds":[[1]],"flipX":true,"flipY":true,"breakOnMatch":true,"outOfBoundsValue":99});
    let group = p
        .apply_auto_rule_wizard(1, None, "Generated", &[rule])
        .unwrap();
    let g = &p.json["defs"]["layers"][0]["autoRuleGroups"][0];
    assert_eq!(g["uid"], group);
    assert_eq!(g["usesWizard"], true);
    assert_eq!(g["rules"][0]["size"], 1);
    assert_eq!(g["rules"][0]["pattern"], json!([1]));
    assert_eq!(g["rules"][0]["flipX"], false);
    assert_eq!(g["rules"][0]["flipY"], false);
    assert!(g["rules"][0]["outOfBoundsValue"].is_null());
    assert_eq!(tiles(&p, key)[0]["t"], 1);
    let old_uid = g["rules"][0]["uid"].clone();
    let updated = json!({"size":1,"pattern":[1],"tileRectsIds":[[3]],"outOfBoundsValue":1});
    p.apply_auto_rule_wizard(1, Some(group), "Changed", &[updated])
        .unwrap();
    let g = &p.json["defs"]["layers"][0]["autoRuleGroups"][0];
    assert_eq!(g["rules"].as_array().unwrap().len(), 1);
    assert_ne!(g["rules"][0]["uid"], old_uid);
    assert_eq!(g["name"], "Changed");
    assert_eq!(g["rules"][0]["outOfBoundsValue"], 1);
    assert_eq!(tiles(&p, key)[0]["t"], 3);
    let before = p.json.clone();
    assert!(
        p.apply_auto_rule_wizard(
            1,
            Some(group),
            "Invalid",
            &[json!({"size":2,"pattern":[1,1,1,1]})]
        )
        .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn rule_cross_group_move_preserves_identity_extensions_and_rolls_back_bad_destination() {
    let (mut p, key) = fixture();
    let group = p.add_auto_rule_group(1, "Other").unwrap();
    p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["extension"] = json!("retain");
    p.move_auto_rule_to_group(1, 3, group, 4, 0).unwrap();
    let groups = &p.json["defs"]["layers"][0]["autoRuleGroups"];
    assert!(groups[0]["rules"].as_array().unwrap().is_empty());
    assert_eq!(groups[1]["rules"][0]["uid"], 4);
    assert_eq!(groups[1]["rules"][0]["extension"], "retain");
    assert_eq!(tiles(&p, key)[0]["t"], 2);
    let before = p.json.clone();
    assert!(p.move_auto_rule_to_group(1, group, 3, 4, 5).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn preview_154_auto_layer_source_change_inherits_the_source_grid() {
    let mut project = Project::new();
    let source = project.add_layer("FineGrid", "IntGrid", 8).unwrap();
    let target = project.add_layer("Generated", "AutoLayer", 16).unwrap();
    let unrelated = project.add_layer("Unrelated", "Tiles", 32).unwrap();
    let key = project.level_keys()[0];

    project
        .update_layer_properties(target, &json!({"autoSourceLayerDefUid":source}))
        .unwrap();
    let generated = project.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layerDefUid"] == target)
        .unwrap();
    assert_eq!(project.layer_def(target).unwrap()["gridSize"], 8);
    assert_eq!(generated["__gridSize"], 8);
    assert_eq!(generated["__cWid"], 32);
    assert_eq!(project.layer_def(unrelated).unwrap()["gridSize"], 32);

    project
        .update_layer_properties(target, &json!({"autoSourceLayerDefUid":null}))
        .unwrap();
    assert!(project.layer_def(target).unwrap()["autoSourceLayerDefUid"].is_null());
    assert_eq!(project.layer_def(target).unwrap()["gridSize"], 8);
}

#[test]
fn deleting_an_auto_layer_source_keeps_rules_across_save_and_rebinding() {
    let mut project = Project::new();
    let source = project.add_layer("OldGrid", "IntGrid", 16).unwrap();
    let replacement = project.add_layer("NewGrid", "IntGrid", 16).unwrap();
    let target = project.add_layer("Generated", "AutoLayer", 16).unwrap();
    let key = project.level_keys()[0];
    project.json["defs"]["tilesets"] =
        json!([{"uid":400,"relPath":"atlas.png","tileGridSize":16,"pxWid":32,"pxHei":16}]);
    project.assign_tileset(target, 400).unwrap();
    project
        .update_layer_properties(target, &json!({"autoSourceLayerDefUid":source}))
        .unwrap();
    let group = project.add_auto_rule_group(target, "Walls").unwrap();
    let rule = project.add_auto_rule(target, group, 1).unwrap();
    project
        .update_auto_rule(
            target,
            group,
            rule,
            &json!({
                "pattern":[1], "tileRectsIds":[[1]], "outOfBoundsValue":1, "extension":"keep"
            }),
        )
        .unwrap();
    let instance_index = |project: &Project, uid| {
        project.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|layer| layer["layerDefUid"] == uid)
            .unwrap()
    };
    project
        .paint_int_grid(key, instance_index(&project, source), 1, 2, 1)
        .unwrap();
    project
        .paint_int_grid(key, instance_index(&project, replacement), 3, 1, 1)
        .unwrap();
    let generated = |project: &Project| {
        project.layer(key, instance_index(project, target)).unwrap()["autoLayerTiles"].clone()
    };
    let old_cache = generated(&project);
    assert_eq!(old_cache[0]["px"], json!([16, 32]));
    let rules = project.layer_def(target).unwrap()["autoRuleGroups"].clone();
    project.remove_definition("layers", source).unwrap();
    let detached = project.layer_def(target).unwrap();
    assert!(detached["autoSourceLayerDefUid"].is_null());
    assert_eq!(detached["autoRuleGroups"], rules);
    assert_eq!(generated(&project), json!([]));

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Detached.ldtk");
    project.save(&path).unwrap();
    let mut reopened = Project::load(&path).unwrap();
    // LDtk clears the obsolete out-of-bounds value on save without a source.
    let mut saved_rules = rules.clone();
    saved_rules[0]["rules"][0]["outOfBoundsValue"] = Value::Null;
    assert_eq!(
        reopened.layer_def(target).unwrap()["autoRuleGroups"],
        saved_rules
    );
    assert_eq!(generated(&reopened), json!([]));

    let orphan = reopened.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|def| def["uid"] == target)
        .unwrap();
    orphan["autoSourceLayerDefUid"] = json!(source);
    let index = instance_index(&reopened, target);
    reopened.layer_mut(key, index).unwrap()["autoLayerTiles"] = old_cache;
    reopened.migrate_to_current().unwrap();
    reopened.reconcile_cached_auto_tiles();
    assert!(reopened.layer_def(target).unwrap()["autoSourceLayerDefUid"].is_null());
    assert_eq!(
        reopened.layer_def(target).unwrap()["autoRuleGroups"],
        saved_rules
    );
    assert_eq!(generated(&reopened), json!([]));

    reopened
        .update_layer_properties(target, &json!({"autoSourceLayerDefUid":replacement}))
        .unwrap();
    assert_eq!(
        reopened.layer_def(target).unwrap()["autoRuleGroups"],
        saved_rules
    );
    assert_eq!(generated(&reopened)[0]["px"], json!([48, 16]));
    assert_eq!(generated(&reopened)[0]["t"], 1);
}

#[test]
fn preview_154_multiple_tile_and_intgrid_blockers_update_incrementally() {
    let (mut p, k) = fixture();
    p.json["defs"]["layers"][0]["layerUidsPreventingAutoTilingHere"] = json!([9, 10]);
    p.json["defs"]["layers"].as_array_mut().unwrap().extend([
        json!({"uid":9,"type":"Tiles","gridSize":16}),
        json!({"uid":10,"type":"IntGrid","gridSize":16,"intGridValues":[{"value":1}]}),
    ]);
    p.level_mut(k).unwrap()["layerInstances"].as_array_mut().unwrap().extend([
        json!({"layerDefUid":9,"__type":"Tiles","__gridSize":16,"__cWid":3,"__cHei":2,"gridTiles":[{"px":[0,0],"t":1}]}),
        json!({"layerDefUid":10,"__type":"IntGrid","__gridSize":16,"__cWid":3,"__cHei":2,"intGridCsv":[0,0,1,0,0,0],"autoLayerTiles":[]}),
    ]);
    p.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 1);
    p.paint_int_grid(k, 2, 2, 0, 0).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 2);
    p.paint_tile(k, 1, 0, 0, 0, [0, 0], 0, true).unwrap();
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
    let mut full = p.clone();
    full.rebuild_auto_layers(k).unwrap();
    assert_eq!(tiles(&p, k), tiles(&full, k));
}

#[test]
fn incremental_rebuild_respects_the_blocker_stride_and_partial_row() {
    let (mut project, key) = fixture();
    project.json["defs"]["layers"][0]["layerUidsPreventingAutoTilingHere"] = json!([9]);
    let layers = project.level_mut(key).unwrap()["layerInstances"]
        .as_array_mut()
        .unwrap();
    layers[0]["intGridCsv"] = json!([0, 0, 1, 1, 0, 1]);
    layers.push(json!({
        "layerDefUid":9, "__type":"IntGrid", "__gridSize":16, "__cWid":2, "__cHei":2,
        "intGridCsv":[0,0,1]
    }));
    let mut full = project.clone();
    full.rebuild_auto_layers(key).unwrap();
    project
        .rebuild_auto_layers_area(key, 1, [2, 1, 1, 1])
        .unwrap();
    assert_eq!(tiles(&project, key), tiles(&full, key));
    let cells = tiles(&project, key)
        .as_array()
        .unwrap()
        .iter()
        .map(|tile| tile["d"][1].as_i64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(cells, [2, 5]);
}

#[test]
fn tile_bucket_erase_updates_blocked_auto_tiles_and_rolls_back_on_error() {
    let (mut project, key) = fixture();
    let rules = &mut project.json["defs"]["layers"];
    rules[0]["layerUidsPreventingAutoTilingHere"] = json!([9]);
    rules
        .as_array_mut()
        .unwrap()
        .push(json!({"uid":9,"type":"Tiles","gridSize":16}));
    project.level_mut(key).unwrap()["layerInstances"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "layerDefUid":9, "__type":"Tiles", "__gridSize":16, "__cWid":3, "__cHei":2,
            "gridTiles":[{"px":[0,0],"t":1},{"px":[32,0],"t":1}]
        }));
    project.rebuild_auto_layers(key).unwrap();
    assert_eq!(tiles(&project, key).as_array().unwrap().len(), 1);
    let cells = [[2, 0], [0, 0], [2, 0], [1, 0], [-1, 0]];

    let mut immediate = project.clone();
    assert_eq!(
        immediate
            .erase_tile_cells_with_deferred(key, 1, &cells, None)
            .unwrap(),
        [[2, 0], [0, 0]]
    );
    assert_eq!(immediate.layer(key, 1).unwrap()["gridTiles"], json!([]));
    assert_eq!(tiles(&immediate, key).as_array().unwrap().len(), 3);

    let mut invalid = project.clone();
    invalid.json["defs"]["layers"][0]["tilesetDefUid"] = json!(999);
    let before = invalid.json.clone();
    assert!(
        invalid
            .erase_tile_cells_with_deferred(key, 1, &cells, None)
            .is_err()
    );
    assert_eq!(invalid.json, before);

    project.json["defs"]["layers"][0]["useAsyncRender"] = json!(true);
    let mut pending = DeferredAutoLayers::default();
    project
        .erase_tile_cells_with_deferred(key, 1, &cells, Some(&mut pending))
        .unwrap();
    assert!(!pending.is_empty());
    assert_eq!(tiles(&project, key).as_array().unwrap().len(), 1);
    project.settle_auto_layers(key, &mut pending).unwrap();
    assert_eq!(tiles(&project, key), tiles(&immediate, key));
}

#[test]
fn preview_154_offset_grids_keep_their_cell_stride_through_load_and_grid_edits() {
    let (mut project, key) = fixture();
    project.json["defs"]["layers"][0]["identifier"] = json!("Terrain");
    project.json["defs"]["layers"][0]["layerUidsPreventingAutoTilingHere"] = json!([9, 10]);
    project.json["defs"]["layers"].as_array_mut().unwrap().extend([
        json!({"uid":9,"identifier":"TileBlocker","type":"Tiles","gridSize":16}),
        json!({"uid":10,"identifier":"GridBlocker","type":"IntGrid","gridSize":16,"intGridValues":[{"value":1}]}),
    ]);
    let level = project.level_mut(key).unwrap();
    level["pxWid"] = json!(50);
    level["pxHei"] = json!(32);
    let layers = level["layerInstances"].as_array_mut().unwrap();
    layers[0]["__type"] = json!("IntGrid");
    layers[0]["__cHei"] = json!(3);
    layers[0]["pxOffsetX"] = json!(7);
    layers[0]["pxOffsetY"] = json!(-3);
    layers[0]["intGridCsv"] = json!([1, 0, 1, 0, 1, 0, 0, 0, 1]);
    layers.extend([
        json!({"layerDefUid":9,"__type":"Tiles","__gridSize":16,"__cWid":3,"__cHei":2,"pxOffsetX":3,"gridTiles":[{"px":[32,0],"src":[16,0],"f":0,"t":1,"d":[2],"a":1}]}),
        json!({"layerDefUid":10,"__type":"IntGrid","__gridSize":16,"__cWid":4,"__cHei":2,"pxOffsetX":-5,"intGridCsv":[1,0,0,0,0,0,0,0]}),
    ]);
    let cells = |project: &Project| {
        tiles(project, key)
            .as_array()
            .unwrap()
            .iter()
            .map(|tile| tile["d"][1].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    project.rebuild_auto_layers(key).unwrap();
    assert_eq!(cells(&project), [4, 8]);

    let mut legacy = project.clone();
    let terrain = legacy.layer_mut(key, 0).unwrap();
    terrain["__cWid"] = json!(4);
    terrain["__cHei"] = json!(2);
    terrain["intGridCsv"] = json!([1, 0, 1, 0, 0, 1, 0, 1]);
    legacy.migrate_to_current().unwrap();
    legacy.rebuild_auto_layers(key).unwrap();
    assert_eq!(cells(&legacy), [5, 7]);

    project.layer_mut(key, 0).unwrap()["autoLayerTiles"] = Value::Null;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Offset.ldtk");
    project.save(&path).unwrap();
    let mut project = Project::load(&path).unwrap();
    project.migrate_to_current().unwrap();
    project.rebuild_auto_layers(key).unwrap();
    let terrain = project.layer(key, 0).unwrap();
    assert_eq!(
        (terrain["__cWid"].as_i64(), terrain["__cHei"].as_i64()),
        (Some(3), Some(3))
    );
    assert_eq!(cells(&project), [4, 8]);

    project.paint_int_grid(key, 2, 0, 0, 0).unwrap();
    assert_eq!(cells(&project), [0, 4, 8]);
    project
        .paint_tile(key, 1, 2, 0, 1, [16, 0], 0, true)
        .unwrap();
    assert_eq!(cells(&project), [0, 2, 4, 8]);
    let mut rebuilt = project.clone();
    rebuilt.rebuild_auto_layers(key).unwrap();
    assert_eq!(tiles(&project, key), tiles(&rebuilt, key));

    project.change_layer_grid_size(1, 8).unwrap();
    let terrain = project.layer(key, 0).unwrap();
    assert_eq!(
        (terrain["__cWid"].as_i64(), terrain["__cHei"].as_i64()),
        (Some(6), Some(5))
    );
    assert_eq!(cells(&project), [0, 2, 7, 14]);
    project.save(&path).unwrap();
    let mut reopened = Project::load(path).unwrap();
    assert!(!reopened.migrate_to_current().unwrap());
    assert_eq!(tiles(&reopened, key), tiles(&project, key));
}

#[test]
fn deferred_blocker_updates_follow_the_generated_target_flag() {
    let (mut p, k) = fixture();
    p.json["defs"]["layers"][0]["layerUidsPreventingAutoTilingHere"] = json!([9, 10]);
    p.json["defs"]["layers"].as_array_mut().unwrap().extend([
        json!({"uid":9,"type":"Tiles","gridSize":16,"useAsyncRender":true}),
        json!({"uid":10,"type":"IntGrid","gridSize":16,"intGridValues":[{"value":1}]}),
    ]);
    p.level_mut(k).unwrap()["layerInstances"].as_array_mut().unwrap().extend([
        json!({"layerDefUid":9,"__type":"Tiles","__gridSize":16,"__cWid":3,"__cHei":2,"gridTiles":[{"px":[0,0],"t":1}]}),
        json!({"layerDefUid":10,"__type":"IntGrid","__gridSize":16,"__cWid":3,"__cHei":2,"intGridCsv":[0,0,1,0,0,0],"autoLayerTiles":[]}),
    ]);
    p.rebuild_auto_layers(k).unwrap();
    let mut pending = DeferredAutoLayers::default();
    p.paint_tile_stacked_with_deferred(k, 1, 0, 0, 0, [0, 0], 0, true, false, Some(&mut pending))
        .unwrap();
    assert!(
        pending.is_empty(),
        "the blocker flag cannot delay this target"
    );
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 2);

    p.json["defs"]["layers"][0]["useAsyncRender"] = json!(true);
    p.paint_int_grid_cells_with_deferred(k, 2, &[([2, 0], 0)], Some(&mut pending))
        .unwrap();
    assert!(!pending.is_empty());
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 2);
    p.settle_auto_layers(k, &mut pending).unwrap();
    assert!(pending.is_empty());
    assert_eq!(tiles(&p, k).as_array().unwrap().len(), 3);
}

#[test]
fn preview_154_blocker_configuration_rejects_invalid_and_removes_deleted_layers() {
    let mut p = Project::new();
    let auto = p.add_layer("Auto", "IntGrid", 16).unwrap();
    let tile = p.add_layer("Tiles", "Tiles", 16).unwrap();
    let grid = p.add_layer("Grid", "IntGrid", 16).unwrap();
    let entity = p.add_layer("Entities", "Entities", 16).unwrap();
    p.update_layer_properties(
        auto,
        &json!({"layerUidsPreventingAutoTilingHere":[tile,grid]}),
    )
    .unwrap();
    let before = p.json.clone();
    for invalid in [
        json!([auto]),
        json!([tile, tile]),
        json!([entity]),
        json!([99999]),
    ] {
        assert!(
            p.update_layer_properties(auto, &json!({"layerUidsPreventingAutoTilingHere":invalid}))
                .is_err()
        );
        assert_eq!(p.json, before);
    }
    p.remove_definition("layers", tile).unwrap();
    assert_eq!(
        p.layer_def(auto).unwrap()["layerUidsPreventingAutoTilingHere"],
        json!([grid])
    );
}

#[test]
#[ignore = "requires the pinned upstream 1.5.4 autoTileBlocking fixture"]
fn preview_154_upstream_blocking_fixture_matches_saved_auto_tiles() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tools/ldtk-154/autoTileBlocking.ldtk");
    let mut p = Project::load(path).unwrap();
    let mut matched = 0;
    for key in p.level_keys() {
        let original = p.level(key).unwrap()["layerInstances"].clone();
        p.rebuild_auto_layers(key).unwrap();
        for old in original.as_array().unwrap() {
            if old["__identifier"] != "Optional_props" {
                continue;
            }
            let current = p.level(key).unwrap()["layerInstances"]
                .as_array()
                .unwrap()
                .iter()
                .find(|layer| layer["layerDefUid"] == old["layerDefUid"])
                .unwrap();
            assert_eq!(old["autoLayerTiles"].as_array().unwrap().len(), 99);
            assert_eq!(current["autoLayerTiles"], old["autoLayerTiles"]);
            matched += 1;
        }
    }
    assert_eq!(matched, 1);
}
