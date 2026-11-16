use ldtk_core::{LevelKey, Project};
use serde_json::json;
use std::path::Path;

#[test]
fn creating_layers_instantiates_every_level_with_schema_defaults_and_unique_ids() {
    let mut p = Project::new();
    p.json["levels"][0]["pxWid"] = json!(33);
    let second = p.json["levels"][0].clone();
    p.json["worlds"] = json!([{"levels":[second]}]);
    let uid = p.add_layer("Collision", "IntGrid", 16).unwrap();
    let def = p.layer_def(uid).unwrap();
    assert_eq!(def["identifier"], "Collision");
    assert_eq!(def["__type"], "IntGrid");
    assert_eq!(def["intGridValues"][0]["value"], 1);
    let mut ids = vec![];
    for key in p.level_keys() {
        let layer = p.layer(key, 0).unwrap();
        assert_eq!(layer["layerDefUid"], uid);
        assert_eq!(layer["__cWid"], 3);
        assert_eq!(layer["intGridCsv"].as_array().unwrap().len(), 48);
        assert_eq!(layer["visible"], true);
        ids.push(layer["iid"].clone());
    }
    assert_ne!(ids[0], ids[1]);
    assert_eq!(p.json["nextUid"], uid + 1);
    let before = p.json.clone();
    assert!(p.add_layer("Collision", "Tiles", 16).is_err());
    assert!(p.add_layer("Invalid name", "Tiles", 16).is_err());
    assert!(p.add_layer("Other", "Unknown", 16).is_err());
    assert!(p.add_layer("Other", "Tiles", 0).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn layer_creation_is_transactional_when_a_level_is_malformed() {
    let mut p = Project::new();
    let second = p.json["levels"][0].clone();
    p.json["levels"].as_array_mut().unwrap().push(second);
    p.json["levels"][1]["pxWid"] = json!(0);
    let before = p.json.clone();
    assert!(p.add_layer("Actors", "Entities", 16).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn tilesets_can_be_created_and_assigned_without_losing_tile_metadata() {
    let mut p = Project::new();
    let layer_uid = p.add_layer("Floor", "Tiles", 16).unwrap();
    let tileset_uid = p.add_tileset(Path::new("atlas.png"), 16, 32, 32).unwrap();
    assert_ne!(layer_uid, tileset_uid);
    p.assign_tileset(layer_uid, tileset_uid).unwrap();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    assert_eq!(
        p.layer_def(layer_uid).unwrap()["tilesetDefUid"],
        tileset_uid
    );
    assert_eq!(p.layer(key, 0).unwrap()["__tilesetDefUid"], tileset_uid);
    assert_eq!(p.layer(key, 0).unwrap()["__tilesetRelPath"], "atlas.png");
    assert_eq!(p.tileset(tileset_uid).unwrap()["__cWid"], 2);
    p.paint_tile(key, 0, 0, 0, 3, [16, 16], 0, false).unwrap();
    p.layer_mut(key, 0).unwrap()["gridTiles"][0]["future"] = json!(true);
    p.assign_tileset(layer_uid, tileset_uid).unwrap();
    assert_eq!(p.layer(key, 0).unwrap()["gridTiles"][0]["future"], true);
    let before = p.json.clone();
    assert!(p.add_tileset(Path::new("bad.png"), 16, 15, 32).is_err());
    assert!(p.assign_tileset(layer_uid, 999).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn incompatible_tileset_assignment_is_rejected_before_mutation() {
    let mut p = Project::new();
    let layer = p.add_layer("Floor", "Tiles", 16).unwrap();
    let atlas = p.add_tileset(Path::new("atlas.png"), 8, 32, 32).unwrap();
    let before = p.json.clone();
    assert!(p.assign_tileset(layer, atlas).is_err());
    assert_eq!(p.json, before);
}

#[test]
fn newly_created_entity_definition_can_be_placed_and_round_trips() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let uid = p.add_entity_def("Hero", 16, 24).unwrap();
    let key = LevelKey {
        world: None,
        level: 0,
    };
    assert!(p.add_entity(key, 0, uid, 32, 32).unwrap());
    let entity = &p.layer(key, 0).unwrap()["entityInstances"][0];
    assert_eq!(entity["__pivot"], json!([0.5, 1.0]));
    assert_eq!(entity["height"], 24);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("new.ldtk");
    p.save(&path).unwrap();
    assert_eq!(Project::load(path).unwrap().json, p.json);
    let before = p.json.clone();
    assert!(p.add_entity_def("Hero", 16, 16).is_err());
    assert!(p.add_entity_def("Other", 0, 16).is_err());
    assert_eq!(p.json, before);
}
