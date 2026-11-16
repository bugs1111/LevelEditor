use ldtk_core::{LevelKey, Project};
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
