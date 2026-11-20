use ldtk_core::Project;
use serde_json::json;
fn sample() -> (Project, i64, i64, i64, i64) {
    let mut p = Project::new();
    let from = p.add_layer("Origin", "Entities", 16).unwrap();
    let to = p.add_layer("Target", "Entities", 8).unwrap();
    p.reorder_layer(from, 0).unwrap();
    let a = p.add_entity_def("Actor", 16, 16).unwrap();
    let b = p.add_entity_def("Item", 16, 16).unwrap();
    p.update_entity_definition(
        a,
        &json!({"tags":["actor"],"exportToToc":true,"pivotX":0.5,"pivotY":1}),
    )
    .unwrap();
    p.update_entity_definition(b, &json!({"tags":["item"]}))
        .unwrap();
    for key in [
        p.level_keys()[0],
        p.add_level(None, "Second", 256, 256).unwrap(),
    ] {
        p.add_entity(key, 0, a, 24, 16).unwrap();
        p.add_entity(key, 0, b, 32, 32).unwrap();
    }
    p.update_layer_properties(to, &json!({"requiredTags":["actor"],"excludedTags":[]}))
        .unwrap();
    (p, from, to, a, b)
}
#[test]
fn all_filter_moves_only_allowed_instances_across_levels_with_identity_and_refs() {
    let (mut p, from, to, a, _) = sample();
    let key = p.level_keys()[0];
    let entity = p.layer(key, 0).unwrap()["entityInstances"][0].clone();
    let iid = entity["iid"].clone();
    let f = p
        .add_field_definition(Some(a), "Target", "EntityRef", false)
        .unwrap();
    let owner = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(key, Some(&owner), f, json!({"entityIid":iid}))
        .unwrap();
    assert_eq!(p.count_entities_to_transfer(from, to, "all.*").unwrap(), 2);
    assert_eq!(
        p.move_entities_between_layers(from, to, "all.*").unwrap(),
        2
    );
    for key in p.level_keys() {
        assert_eq!(
            p.layer(key, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let moved = &p.layer(key, 1).unwrap()["entityInstances"][0];
        assert_eq!(moved["__grid"], json!([3, 1]));
        assert_eq!(moved["px"], json!([24, 16]));
    }
    let moved = &p.layer(key, 1).unwrap()["entityInstances"][0];
    assert_eq!(moved["iid"], iid);
    assert_eq!(
        moved["fieldInstances"][0]["__value"]["layerIid"],
        p.layer(key, 1).unwrap()["iid"]
    );
    assert_eq!(
        p.json["toc"][0]["instancesData"].as_array().unwrap().len(),
        2
    );
}
#[test]
fn identifier_filter_rejects_excluded_tags_atomically_and_tag_filter_preserves_source_quirk() {
    let (mut p, from, to, _, _) = sample();
    let before = p.json.clone();
    assert!(p.move_entities_between_layers(from, to, "id.Item").is_err());
    assert_eq!(p.json, before);
    assert_eq!(
        p.move_entities_between_layers(from, to, "tag.item")
            .unwrap(),
        2
    );
    for key in p.level_keys() {
        assert_eq!(
            p.layer(key, 1).unwrap()["entityInstances"][0]["__identifier"],
            "Item"
        );
    }
    let before = p.json.clone();
    assert!(p.move_entities_between_layers(from, from, "all.*").is_err());
    assert_eq!(p.json, before);
}
#[test]
fn batch_transfer_preflights_every_destination_and_supports_external_worlds() {
    let (mut p, from, to, a, _) = sample();
    let before = p.json.clone();
    p.json["levels"][1]["layerInstances"]
        .as_array_mut()
        .unwrap()
        .retain(|l| l["layerDefUid"] != to);
    let invalid = p.json.clone();
    assert!(p.move_entities_between_layers(from, to, "all.*").is_err());
    assert_eq!(
        p.json, invalid,
        "a late missing destination must not partly move earlier levels"
    );
    p = Project::from_json(before).unwrap();
    let world = p.add_world("Other").unwrap();
    let key = p.add_level(Some(world), "Third", 256, 256).unwrap();
    p.add_entity(key, 0, a, 24, 16).unwrap();
    let ids = p
        .level_keys()
        .into_iter()
        .flat_map(|k| {
            p.layer(k, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["iid"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::BTreeSet<_>>();
    let dir = tempfile::tempdir().unwrap();
    p.set_external_levels(true).unwrap();
    p.save(dir.path().join("external.ldtk")).unwrap();
    let mut loaded = Project::load(dir.path().join("external.ldtk")).unwrap();
    assert_eq!(
        loaded
            .move_entities_between_layers(from, to, "id.Actor")
            .unwrap(),
        3
    );
    let after = loaded
        .level_keys()
        .into_iter()
        .flat_map(|k| {
            [0, 1]
                .into_iter()
                .flat_map(|i| {
                    loaded.layer(k, i).unwrap()["entityInstances"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| e["iid"].as_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids, after);
    loaded.save(dir.path().join("external.ldtk")).unwrap();
    Project::load(dir.path().join("external.ldtk")).unwrap();
}
