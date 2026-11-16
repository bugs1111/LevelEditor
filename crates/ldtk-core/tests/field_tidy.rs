use ldtk_core::Project;
use serde_json::{Value, json};
#[test]
fn explicit_refresh_tidies_invalid_enum_wrappers_and_defaults_without_guessing_identifiers() {
    let mut p = Project::new();
    p.add_enum_definition("State", &["Idle", "Run"]).unwrap();
    let f = p
        .add_field_definition(None, "Mode", "LocalEnum.State", true)
        .unwrap();
    p.json["defs"]["levelFields"][0]["defaultOverride"] =
        json!({"id":"V_String","params":["LostDefault"]});
    p.json["defs"]["levelFields"][0]["extension"] = json!({"keep":true});
    let field = &mut p.json["levels"][0]["fieldInstances"][0];
    field["realEditorValues"] = json!([{"id":"V_String","params":["Idle"]},{"id":"V_String","params":["Lost"]},null,{"id":"V_String","params":["Run"]}]);
    field["__value"] = json!(["Idle", "Lost", null, "Run"]);
    field["extension"] = json!({"future":[1,2]});
    let raw = p.json.clone();
    let mut loaded = Project::from_json(raw.clone()).unwrap();
    assert_eq!(loaded.json, raw);
    loaded.refresh_derived_data().unwrap();
    assert_eq!(
        loaded.json["defs"]["levelFields"][0]["defaultOverride"],
        Value::Null
    );
    let field = &loaded.json["levels"][0]["fieldInstances"][0];
    assert_eq!(field["defUid"], f);
    assert_eq!(field["__value"], json!(["Idle", null, null, "Run"]));
    assert_eq!(field["realEditorValues"][1], Value::Null);
    assert_eq!(
        field["realEditorValues"][0],
        raw["levels"][0]["fieldInstances"][0]["realEditorValues"][0]
    );
    assert_eq!(field["extension"], json!({"future":[1,2]}));
    assert_eq!(
        loaded.json["defs"]["levelFields"][0]["extension"],
        json!({"keep":true})
    );
    loaded.json["defs"]["levelFields"][0]["defaultOverride"] =
        json!({"id":"V_String","params":["Run"]});
    loaded.refresh_derived_data().unwrap();
    assert_eq!(
        loaded.json["levels"][0]["fieldInstances"][0]["__value"],
        json!(["Idle", "Run", "Run", "Run"])
    );
}
#[test]
fn original_entities_fixture_legacy_spaced_value_stays_valid_through_source_lookup() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/original/Entities.ldtk");
    let mut p = Project::load(path).unwrap();
    let key = p.level_keys()[0];
    let find = |p: &Project| {
        p.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|l| l["entityInstances"].as_array().into_iter().flatten())
            .find(|e| e["__identifier"] == "Chest")
            .unwrap()["fieldInstances"][0]
            .clone()
    };
    let raw = find(&p);
    assert_eq!(raw["__value"], json!(["Sword", "Heavy sword", "Bow"]));
    p.migrate_to_current().unwrap();
    let fixed = find(&p);
    assert_eq!(fixed["__value"], json!(["Sword", "Heavy sword", "Bow"]));
    assert_eq!(fixed["realEditorValues"][1], raw["realEditorValues"][1]);
    assert_eq!(fixed["realEditorValues"][0], raw["realEditorValues"][0]);
    assert_eq!(fixed["realEditorValues"][2], raw["realEditorValues"][2]);
}
#[test]
fn normalized_local_default_and_smart_caches_use_canonical_lookup_but_external_ids_are_verbatim() {
    let mut p = Project::new();
    let e = p.add_enum_definition("Items", &["Heavy_sword"]).unwrap();
    p.json["defs"]["enums"][0]["values"][0]["tileRect"] =
        json!({"tilesetUid":3,"x":80,"y":432,"w":16,"h":16});
    p.json["defs"]["enums"][0]["values"][0]["color"] = json!(0x123456);
    let owner = p.add_entity_def("Chest", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(owner), "Item", "LocalEnum.Items", false)
        .unwrap();
    p.add_layer("Objects", "Entities", 16).unwrap();
    let key = p.level_keys()[0];
    p.add_entity(key, 0, owner, 16, 16).unwrap();
    let d = &mut p.json["defs"]["entities"][0]["fieldDefs"][0];
    d["defaultOverride"] = json!({"id":"V_String","params":["Heavy sword"]});
    d["editorDisplayMode"] = json!("EntityTile");
    d["useForSmartColor"] = json!(true);
    p.refresh_derived_data().unwrap();
    assert_eq!(
        p.layer(key, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"],
        "Heavy_sword"
    );
    let iid = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_editor_value(key, Some(&iid), f, json!("Heavy sword"))
        .unwrap();
    let instance = &p.layer(key, 0).unwrap()["entityInstances"][0];
    assert_eq!(instance["fieldInstances"][0]["__value"], "Heavy sword");
    assert_eq!(instance["__tile"]["x"], 80);
    assert_eq!(instance["__smartColor"], "#123456");
    assert_eq!(p.enum_value(e, "Heavy sword").unwrap()["id"], "Heavy_sword");
    p.json["defs"]["externalEnums"] = json!([{"uid":1000,"identifier":"External","externalRelPath":"external.json","values":[{"id":"Heavy_sword"}]}]);
    assert!(p.enum_value(1000, "Heavy sword").is_none());
    assert!(p.enum_value(1000, "Heavy_sword").is_some());
}
