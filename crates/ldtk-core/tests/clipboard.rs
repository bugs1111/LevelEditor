use ldtk_core::{
    Project,
    clipboard::{ClipboardData, ClipboardKind},
};
use serde_json::json;
#[test]
fn exact_upstream_envelope_is_readable_with_crlf_and_rejects_foreign_text() {
    let d = ClipboardData {
        kind: ClipboardKind::EntityDef,
        value: json!({"uid":7,"identifier":"Actor"}),
    };
    let raw = d.encode("1.5.3").unwrap();
    assert_eq!(
        raw,
        "LDtk 1.5.3 clipboard\n----\nCEntityDef\n----\n{\"uid\":7,\"identifier\":\"Actor\"}"
    );
    assert_eq!(
        ClipboardData::decode(&raw.replace('\n', "\r\n")).unwrap(),
        d
    );
    assert!(ClipboardData::decode("{\"uid\":7}").is_err());
    assert!(ClipboardData::decode("LDtk 1.5.3 clipboard\n----\nUnknown\n----\n{}").is_err());
}
#[test]
fn copied_definitions_and_fields_regenerate_nested_uids_and_keep_metadata() {
    let mut p = Project::new();
    let actor = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(actor), "Health", "Int", false)
        .unwrap();
    p.json["defs"]["entities"][0]["custom"] = json!({"future":true});
    let copied = p.copy_definition("entities", actor).unwrap();
    let copy = p.paste_definition(&copied, Some(actor)).unwrap();
    assert_ne!(copy, actor);
    let defs = p.json["defs"]["entities"].as_array().unwrap();
    assert_eq!(defs[1]["custom"], json!({"future":true}));
    assert_eq!(defs[1]["identifier"], "Actor2");
    assert_ne!(defs[1]["fieldDefs"][0]["uid"], field);
    let clip = p.copy_field_definition(field).unwrap();
    let new = p
        .paste_field_definition(Some(actor), &clip, Some(field))
        .unwrap();
    assert_ne!(field, new);
    assert_eq!(
        p.json["defs"]["entities"][0]["fieldDefs"][1]["identifier"],
        "Health2"
    );
    let before = p.json.clone();
    let mut bad = copied.clone();
    bad.value["tilesetId"] = json!(9999);
    assert!(p.paste_definition(&bad, None).is_err());
    assert_eq!(p.json, before);
}
#[test]
fn layer_rule_and_group_clipboards_are_atomic_and_remap_every_owned_uid() {
    let mut p = Project::new();
    let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
    let group = p.add_auto_rule_group(layer, "Walls").unwrap();
    let rule = p.add_auto_rule(layer, group, 1).unwrap();
    p.update_auto_rule(
        layer,
        group,
        rule,
        &json!({"pattern":[1],"futureRuleData":{"a":true}}),
    )
    .unwrap();
    let clip = p.copy_auto_rule(layer, group, None).unwrap();
    let new_group = p.paste_auto_rule(layer, group, &clip, Some(group)).unwrap();
    assert_ne!(new_group, group);
    let groups = p.layer_def(layer).unwrap()["autoRuleGroups"]
        .as_array()
        .unwrap();
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[1]["rules"][0]["uid"], rule);
    assert_eq!(groups[1]["rules"][0]["futureRuleData"], json!({"a":true}));
    let clip = p.copy_auto_rule(layer, group, Some(rule)).unwrap();
    let new_rule = p.paste_auto_rule(layer, group, &clip, Some(rule)).unwrap();
    assert_ne!(rule, new_rule);
    assert_eq!(
        p.layer_def(layer).unwrap()["autoRuleGroups"][0]["rules"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let layer_clip = p.copy_definition("layers", layer).unwrap();
    let new_layer = p.paste_definition(&layer_clip, Some(layer)).unwrap();
    assert_ne!(new_layer, layer);
    assert_eq!(p.layer(p.level_keys()[0], 1).unwrap()["intGridCsv"][0], 0);
}
#[test]
fn duplicate_remap_preserves_pattern_sign_and_checks_tile_coordinates() {
    let mut p = Project::new();
    let layer = p.add_layer("Ground", "IntGrid", 16).unwrap();
    p.json["defs"]["tilesets"] = json!([{"uid":400,"tileGridSize":16,"pxWid":64,"pxHei":32}]);
    p.assign_tileset(layer, 400).unwrap();
    let group = p.add_auto_rule_group(layer, "Walls").unwrap();
    let rule = p.add_auto_rule(layer, group, 3).unwrap();
    p.update_auto_rule(
        layer,
        group,
        rule,
        &json!({"pattern":[0,-1,0,1,1,0,0,0,0],"tileRectsIds":[[0,1]],"outOfBoundsValue":1}),
    )
    .unwrap();
    let uid = p
        .duplicate_rule_group_remapped(layer, group, "Grass", &[(1, 2)].into(), [1, 1])
        .unwrap();
    let copy = p.layer_def(layer).unwrap()["autoRuleGroups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["uid"] == uid)
        .unwrap();
    assert_eq!(copy["name"], "Grass");
    assert_eq!(
        copy["rules"][0]["pattern"],
        json!([0, -2, 0, 2, 2, 0, 0, 0, 0])
    );
    assert_eq!(copy["rules"][0]["tileRectsIds"], json!([[5, 6]]));
    assert_eq!(copy["rules"][0]["outOfBoundsValue"], 2);
    let before = p.json.clone();
    assert!(
        p.duplicate_rule_group_remapped(layer, group, "Invalid", &Default::default(), [-1, 0])
            .is_err()
    );
    assert_eq!(p.json, before);
}
