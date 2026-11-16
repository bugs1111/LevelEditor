use ldtk_core::Project;
use serde_json::json;

#[test]
fn style_updates_definitions_enum_references_and_instances_without_changing_field_names_or_uids() {
    let mut p = Project::new();
    let layer = p.add_layer("Actors", "Entities", 16).unwrap();
    let entity = p.add_entity_def("Hero", 16, 16).unwrap();
    let local = p
        .add_enum_definition("Mood", &["VeryHappy", "Sad"])
        .unwrap();
    let field = p
        .add_field_definition(Some(entity), "currentMood", "LocalEnum.Mood", false)
        .unwrap();
    let level_field = p
        .add_field_definition(None, "allowedMoods", "LocalEnum.Mood", true)
        .unwrap();
    p.update_field_definition(
        field,
        &json!({"defaultOverride":{"id":"V_String","params":["Sad"]}}),
    )
    .unwrap();
    let key = p.level_keys()[0];
    p.add_entity(key, 0, entity, 8, 8).unwrap();
    let iid = p.level(key).unwrap()["layerInstances"][0]["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(key, Some(&iid), field, json!("VeryHappy"))
        .unwrap();
    p.set_field_value(key, None, level_field, json!(["Sad", "VeryHappy"]))
        .unwrap();
    p.json["extension"] = json!({"identifier":"KeepMe"});
    p.apply_identifier_style("Lowercase").unwrap();
    assert_eq!(p.json["identifierStyle"], "Lowercase");
    assert_eq!(p.layer_def(layer).unwrap()["identifier"], "actors");
    let def = p.json["defs"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["uid"] == entity)
        .unwrap();
    assert_eq!(def["identifier"], "hero");
    assert_eq!(def["fieldDefs"][0]["identifier"], "currentMood");
    assert_eq!(def["fieldDefs"][0]["__type"], "LocalEnum.mood");
    assert_eq!(def["fieldDefs"][0]["defaultOverride"]["params"][0], "sad");
    assert_eq!(p.json["defs"]["enums"][0]["uid"], local);
    let instance = &p.level(key).unwrap()["layerInstances"][0]["entityInstances"][0];
    assert_eq!(instance["iid"], iid);
    assert_eq!(instance["__identifier"], "hero");
    assert_eq!(instance["fieldInstances"][0]["__value"], "veryhappy");
    assert_eq!(
        instance["fieldInstances"][0]["realEditorValues"][0]["params"][0],
        "veryhappy"
    );
    assert_eq!(
        p.level(key).unwrap()["fieldInstances"][0]["__value"],
        json!(["sad", "veryhappy"])
    );
    assert_eq!(p.json["extension"]["identifier"], "KeepMe");
}

#[test]
fn automatic_names_expand_world_grid_depth_and_indices_and_avoid_manual_names() {
    let mut p = Project::new();
    let first = p.level_keys()[0];
    p.level_mut(first).unwrap()["useAutoIdentifier"] = json!(false);
    let second = p.add_level(None, "Manual", 256, 256).unwrap();
    p.level_mut(first).unwrap()["identifier"] = json!("World_2_1_1_0_256_0_3");
    p.json["worldLayout"] = json!("GridVania");
    p.json["worldGridWidth"] = json!(256);
    p.json["worldGridHeight"] = json!(256);
    p.level_mut(first).unwrap()["worldX"] = json!(0);
    p.level_mut(first).unwrap()["worldY"] = json!(0);
    p.level_mut(second).unwrap()["worldX"] = json!(256);
    p.level_mut(second).unwrap()["worldY"] = json!(0);
    p.level_mut(second).unwrap()["worldDepth"] = json!(3);
    p.level_mut(second).unwrap()["useAutoIdentifier"] = json!(true);
    p.json["levelNamePattern"] = json!("%world_%idx1_%idx_%gx_%gy_%x_%y_%depth");
    p.refresh_level_names().unwrap();
    assert_eq!(
        p.level(second).unwrap()["identifier"],
        "World_2_1_1_0_256_0_4"
    );
    assert_eq!(
        p.level(first).unwrap()["identifier"],
        "World_2_1_1_0_256_0_3"
    );
    p.json["levelNamePattern"] = json!("Same");
    p.level_mut(first).unwrap()["useAutoIdentifier"] = json!(true);
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(first).unwrap()["identifier"], "Same");
    assert_eq!(p.level(second).unwrap()["identifier"], "Same2");
}

#[test]
fn linear_names_omit_coordinates_and_external_enums_are_unchanged() {
    let mut p = Project::new();
    let key = p.level_keys()[0];
    p.json["worldLayout"] = json!("LinearHorizontal");
    p.json["levelNamePattern"] = json!("  12-room-%gx-%gy-%x-%y-%d  ");
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(key).unwrap()["identifier"], "_Room_0");
    p.json["defs"]["externalEnums"] = json!([{"uid":99,"identifier":"ImportedEnum","externalRelPath":"enum.hx","values":[{"id":"MixedCase"}]}]);
    let before = p.json["defs"]["externalEnums"].clone();
    p.apply_identifier_style("Uppercase").unwrap();
    assert_eq!(p.json["defs"]["externalEnums"], before);
    let before = p.clone();
    assert!(p.apply_identifier_style("NotAStyle").is_err());
    assert_eq!(p.json, before.json);
}

#[test]
fn names_are_unique_across_worlds_and_external_publication_keeps_iid_and_stub_in_sync() {
    let mut p = Project::new();
    p.enable_multiworlds().unwrap();
    let first = p.level_keys()[0];
    let world = p.add_world("Dungeon").unwrap();
    let second = p.add_level(Some(world), "Temporary", 256, 256).unwrap();
    p.level_mut(first).unwrap()["useAutoIdentifier"] = json!(true);
    p.level_mut(second).unwrap()["useAutoIdentifier"] = json!(true);
    p.json["levelNamePattern"] = json!("Room");
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(first).unwrap()["identifier"], "Room");
    assert_eq!(p.level(second).unwrap()["identifier"], "Room2");
    p.json["levelNamePattern"] = json!("%world_%idx1");
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(second).unwrap()["identifier"], "Dungeon_1");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Project.ldtk");
    p.set_external_levels(true).unwrap();
    p.save(&path).unwrap();
    let iid = p.level(second).unwrap()["iid"].clone();
    p.apply_identifier_style("Uppercase").unwrap();
    assert_eq!(p.level(second).unwrap()["identifier"], "DUNGEON_1");
    assert_eq!(
        p.json["worlds"][world]["levels"][second.level]["identifier"],
        "DUNGEON_1"
    );
    p.save(&path).unwrap();
    let reopened = Project::load(path).unwrap();
    assert_eq!(reopened.level(second).unwrap()["iid"], iid);
    assert_eq!(reopened.level(second).unwrap()["identifier"], "DUNGEON_1");
}

#[test]
fn style_updates_biome_and_tileset_enum_tags_without_rewriting_arbitrary_tags() {
    let mut p = Project::new();
    let local = p.add_enum_definition("Biome", &["Forest", "Snow"]).unwrap();
    let field = p
        .add_field_definition(None, "biomeChoice", "LocalEnum.Biome", false)
        .unwrap();
    p.add_layer("Ground", "IntGrid", 16).unwrap();
    let atlas = p
        .add_tileset(std::path::Path::new("unused.png"), 16, 32, 32)
        .unwrap();
    p.json["defs"]["tilesets"][0]["tagsSourceEnumUid"] = json!(local);
    p.json["defs"]["tilesets"][0]["enumTags"] = json!([{"enumValueId":"Forest","tileIds":[0,2]}]);
    p.json["defs"]["tilesets"][0]["tags"] = json!(["KeepMe"]);
    p.json["defs"]["layers"][0]["biomeFieldUid"] = json!(field);
    p.json["defs"]["layers"][0]["autoRuleGroups"] =
        json!([{"uid":100,"rules":[],"requiredBiomeValues":["Forest","Snow"]}]);
    p.apply_identifier_style("Lowercase").unwrap();
    assert_eq!(
        p.tileset(atlas).unwrap()["enumTags"][0]["enumValueId"],
        "forest"
    );
    assert_eq!(
        p.tileset(atlas).unwrap()["enumTags"][0]["tileIds"],
        json!([0, 2])
    );
    assert_eq!(p.tileset(atlas).unwrap()["tags"], json!(["KeepMe"]));
    assert_eq!(
        p.json["defs"]["layers"][0]["autoRuleGroups"][0]["requiredBiomeValues"],
        json!(["forest", "snow"])
    );
}

#[test]
fn capitalize_only_changes_a_letter_immediately_after_leading_underscores() {
    let mut p = Project::new();
    let key = p.level_keys()[0];
    p.json["levelNamePattern"] = json!("1_3abc");
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(key).unwrap()["identifier"], "_3abc");
}

#[test]
fn loading_keeps_original_names_and_unknown_json_until_an_explicit_edit_refresh() {
    let mut p = Project::new();
    p.json["levels"][0]["identifier"] = json!("StoredAuthorName");
    p.json["levelNamePattern"] = json!("Generated_%idx1");
    p.json["levels"][0]["extension"] = json!({"identifier":"Unrelated","__type":"LocalEnum.Mood"});
    p.json["extension"] = json!([1,{"nested":"Retain"}]);
    let document = p.json.clone();
    let mut loaded = Project::from_json(document.clone()).unwrap();
    assert_eq!(loaded.json, document);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Raw.ldtk");
    loaded.save(&path).unwrap();
    let mut reopened = Project::load(path).unwrap();
    assert_eq!(reopened.json, document);
    reopened.refresh_level_names().unwrap();
    assert_eq!(reopened.json["levels"][0]["identifier"], "Generated_1");
    assert_eq!(
        reopened.json["levels"][0]["extension"],
        document["levels"][0]["extension"]
    );
    assert_eq!(reopened.json["extension"], document["extension"]);
}

#[test]
fn empty_pattern_fallback_obeys_active_style_and_remains_unique() {
    let mut p = Project::new();
    let first = p.level_keys()[0];
    let second = p.add_level(None, "Temporary", 256, 256).unwrap();
    p.level_mut(second).unwrap()["useAutoIdentifier"] = json!(true);
    p.json["identifierStyle"] = json!("Lowercase");
    p.json["levelNamePattern"] = json!("");
    p.refresh_level_names().unwrap();
    assert_eq!(p.level(first).unwrap()["identifier"], "unnamed");
    assert_eq!(p.level(second).unwrap()["identifier"], "unnamed2");
}
