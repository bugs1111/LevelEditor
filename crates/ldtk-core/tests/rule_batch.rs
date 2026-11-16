use ldtk_core::Project;
use serde_json::json;

#[test]
fn group_policy_updates_only_its_rules_and_rebuilds_edges_in_every_level_atomically() {
    let mut p = Project::new();
    let first = p.level_keys()[0];
    p.json["defs"]["layers"] = json!([{"uid":1,"type":"IntGrid","gridSize":16,"tilesetDefUid":2,"intGridValues":[{"value":1}],"autoRuleGroups":[{"uid":3,"active":true,"rules":[{"uid":4,"active":true,"size":3,"pattern":[1,1,1,1,1,1,1,1,1],"tileRectsIds":[[2]],"chance":1,"breakOnMatch":false,"extension":{"keep":true}},{"uid":5,"active":true,"size":3,"pattern":[1,1,1,1,1,1,1,1,1],"tileRectsIds":[[3]],"chance":1,"breakOnMatch":false}]},{"uid":6,"active":false,"rules":[{"uid":7,"active":true,"size":1,"pattern":[1],"tileRectsIds":[[4]],"outOfBoundsValue":0}]}]}]);
    p.json["defs"]["tilesets"] = json!([{"uid":2,"tileGridSize":16,"pxWid":64,"pxHei":64}]);
    p.level_mut(first).unwrap()["layerInstances"] = json!([{"layerDefUid":1,"__cWid":1,"__cHei":1,"__gridSize":16,"seed":100,"intGridCsv":[1],"autoLayerTiles":[]}]);
    let second = p.duplicate_level(first, "Second").unwrap();
    assert_eq!(p.set_rule_group_out_of_bounds(1, 3, Some(1)).unwrap(), 2);
    for key in [first, second] {
        assert_eq!(
            p.layer(key, 0).unwrap()["autoLayerTiles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["t"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![3, 2]
        );
    }
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][0]["rules"][0]["extension"],
        json!({"keep":true})
    );
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][1]["rules"][0]["outOfBoundsValue"],
        0
    );
    let before = p.json.clone();
    for (layer, group, value) in [
        (1, 3, Some(-1)),
        (1, 3, Some(99)),
        (1, 99, Some(1)),
        (99, 3, None),
    ] {
        assert!(p.set_rule_group_out_of_bounds(layer, group, value).is_err());
        assert_eq!(p.json, before);
    }
    assert_eq!(p.set_rule_group_out_of_bounds(1, 3, None).unwrap(), 2);
    for key in [first, second] {
        assert!(
            p.layer(key, 0).unwrap()["autoLayerTiles"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
