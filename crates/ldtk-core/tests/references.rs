use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};

const KEY: LevelKey = LevelKey {
    world: None,
    level: 0,
};
fn populated() -> (Project, i64, String) {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 16).unwrap();
    p.add_entity(KEY, 0, uid, 16, 16).unwrap();
    let iid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut second = p.level(KEY).unwrap().clone();
    second["layerInstances"][0]["entityInstances"] = json!([]);
    p.json["levels"].as_array_mut().unwrap().push(second);
    (p, uid, iid)
}

#[test]
fn unreferenced_entities_can_be_removed_once_and_out_of_range_is_noop() {
    let (mut p, _, _) = populated();
    assert!(p.remove_entity(KEY, 0, 0).unwrap());
    assert!(!p.remove_entity(KEY, 0, 0).unwrap());
    assert_eq!(p.layer(KEY, 0).unwrap()["entityInstances"], json!([]));
}

#[test]
fn deleting_cross_level_scalar_or_array_entity_refs_is_refused_atomically() {
    for array in [false, true] {
        let (mut p, _, iid) = populated();
        let value =
            json!({"entityIid":iid,"levelIid":"ignored","layerIid":"ignored","worldIid":"ignored"});
        let field = json!({"__type":if array {"Array<EntityRef>"} else {"EntityRef"},"__value":if array {json!([value])}else{value},"realEditorValues":[]});
        p.json["levels"][1]["fieldInstances"] = json!([field]);
        let before = p.json.clone();
        assert!(p.remove_entity(KEY, 0, 0).is_err());
        assert_eq!(p.json, before);
    }
}

#[test]
fn editor_values_references_are_checked_even_when_exported_value_is_stale() {
    let (mut p, _, iid) = populated();
    p.json["levels"][1]["fieldInstances"] = json!([{"__type":"EntityRef","__value":null,"realEditorValues":[{"id":"V_String","params":[iid]}]}]);
    let before = p.json.clone();
    assert!(p.remove_entity(KEY, 0, 0).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn discard_old_entities_preflights_inbound_references_before_removal() {
    let (mut p, uid, iid) = populated();
    p.json["defs"]["entities"][0]["maxCount"] = json!(1);
    p.json["defs"]["entities"][0]["limitBehavior"] = json!("DiscardOldOnes");
    p.json["levels"][1]["fieldInstances"] =
        json!([{"__type":"EntityRef","__value":{"entityIid":iid},"realEditorValues":[]}]);
    let before = p.json.clone();
    assert!(p.add_entity(KEY, 0, uid, 32, 32).is_err());
    assert_eq!(p.json, before);
    p.json["levels"][1]["fieldInstances"] = json!([]);
    assert!(p.add_entity(KEY, 0, uid, 32, 32).unwrap());
    assert_ne!(
        p.layer(KEY, 0).unwrap()["entityInstances"][0]["iid"],
        before["levels"][0]["layerInstances"][0]["entityInstances"][0]["iid"]
    );
}

#[test]
fn unresolved_external_level_prevents_unchecked_entity_deletion() {
    let (mut p, _, _) = populated();
    p.json["levels"][1]["externalRelPath"] = json!("unloaded.ldtkl");
    p.json["levels"][1]["layerInstances"] = Value::Null;
    assert!(p.remove_entity(KEY, 0, 0).is_err());
}

#[test]
fn toc_exported_entity_edits_are_refused_until_derived_toc_can_be_regenerated() {
    let (mut p, uid, _) = populated();
    p.json["defs"]["entities"][0]["exportToToc"] = json!(true);
    p.json["toc"] = json!([{"identifier":"Hero","instancesData":[],"future":"keep"}]);
    let before = p.json.clone();
    assert!(p.add_entity(KEY, 0, uid, 32, 32).is_err());
    assert_eq!(p.json, before);
    assert!(p.remove_entity(KEY, 0, 0).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn move_limited_entity_across_layers_refuses_stale_layer_reference_metadata() {
    let (mut p, uid, iid) = populated();
    p.add_layer("OtherActors", "Entities", 16).unwrap();
    p.json["defs"]["entities"][0]["maxCount"] = json!(1);
    p.json["defs"]["entities"][0]["limitBehavior"] = json!("MoveLastOne");
    p.json["levels"][1]["fieldInstances"] = json!([{"__type":"EntityRef","__value":{"entityIid":iid,"layerIid":"old-layer"},"realEditorValues":[]}]);
    let before = p.json.clone();
    assert!(p.add_entity(KEY, 0, uid, 32, 32).is_err());
    assert_eq!(p.json, before);
}
