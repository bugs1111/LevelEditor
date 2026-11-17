use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
#[test]
fn fields_encode_original_editor_wrappers_for_all_types() {
    let mut p = Project::new();
    let cases = [
        ("Int", json!(7), json!({"id":"V_Int","params":[7]})),
        ("Float", json!(1.5), json!({"id":"V_Float","params":[1.5]})),
        ("Bool", json!(true), json!({"id":"V_Bool","params":[true]})),
        (
            "Color",
            json!("#AABBCC"),
            json!({"id":"V_Int","params":[11189196]}),
        ),
        (
            "String",
            json!("hello"),
            json!({"id":"V_String","params":["hello"]}),
        ),
        (
            "Multilines",
            json!("hello\nworld"),
            json!({"id":"V_String","params":["hello\\\\nworld"]}),
        ),
        (
            "FilePath",
            json!("art/hero.png"),
            json!({"id":"V_String","params":["art/hero.png"]}),
        ),
        (
            "Point",
            json!({"cx":2,"cy":3}),
            json!({"id":"V_String","params":["2,3"]}),
        ),
    ];
    for (i, (kind, value, wrapper)) in cases.into_iter().enumerate() {
        let uid = p
            .add_field_definition(None, &format!("Field{i}"), kind, false)
            .unwrap();
        p.set_field_value(K, None, uid, value).unwrap();
        let f = &p.level(K).unwrap()["fieldInstances"][i];
        assert_eq!(f["realEditorValues"], json!([wrapper]), "{kind}");
    }
}
#[test]
fn numeric_defaults_ranges_arrays_and_reset() {
    let mut p = Project::new();
    let uid = p.add_field_definition(None, "Count", "Int", false).unwrap();
    p.update_field_definition(uid, &json!({"min":3,"max":8}))
        .unwrap();
    p.set_field_default(uid, json!(5)).unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], 5);
    p.set_field_value(K, None, uid, json!(99)).unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], 8);
    p.reset_field_value(K, None, uid).unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], 5);
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["realEditorValues"],
        json!([])
    );
    p.convert_field_type(uid, "Float", true, false).unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__type"],
        "Array<Float>"
    );
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        json!([])
    );
    p.set_field_value(K, None, uid, json!([1., 9.])).unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        json!([3, 8])
    );
}
#[test]
fn tile_and_enum_defaults_preserve_original_formats() {
    let mut p = Project::new();
    let ts = p
        .add_tileset(std::path::Path::new("a.png"), 16, 32, 32)
        .unwrap();
    let uid = p.add_field_definition(None, "Tile", "Tile", false).unwrap();
    p.update_field_definition(uid, &json!({"tilesetUid":ts}))
        .unwrap();
    p.set_field_default(uid, json!({"tilesetUid":ts,"x":16,"y":0,"w":16,"h":16}))
        .unwrap();
    let f = &p.level(K).unwrap()["fieldInstances"][0];
    assert_eq!(f["__value"]["x"], 16);
    assert_eq!(
        p.json["defs"]["levelFields"][0]["defaultOverride"]["params"][0],
        "16,0,16,16"
    );
    let e = p.add_enum_definition("Mood", &["Happy", "Sad"]).unwrap();
    let f = p
        .add_field_definition(None, "Mood", "LocalEnum.Mood", false)
        .unwrap();
    p.set_field_value(K, None, f, json!("Happy")).unwrap();
    p.rename_enum_value(e, "Happy", "Joy").unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][1]["__value"], "Joy");
    p.remove_enum_value(e, "Joy").unwrap();
    assert!(p.level(K).unwrap()["fieldInstances"][1]["__value"].is_null());
}
#[test]
fn field_crud_updates_existing_entities_and_keeps_unknown_metadata() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Hero", 16, 16).unwrap();
    p.add_entity(K, 0, e, 0, 0).unwrap();
    let f = p
        .add_field_definition(Some(e), "Name", "String", false)
        .unwrap();
    let iid = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&iid), f, json!("Hero")).unwrap();
    p.update_field_definition(f, &json!({"identifier":"Title","future":{"x":2}}))
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__identifier"],
        "Title"
    );
    p.remove_field_definition(f).unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][0]["fieldInstances"],
        json!([])
    );
}
#[test]
fn invalid_field_type_or_value_never_mutates_project() {
    let mut p = Project::new();
    let old = p.json.clone();
    assert!(
        p.add_field_definition(None, "Bad", "MadeUp", false)
            .is_err()
    );
    assert_eq!(p.json, old);
    let f = p.add_field_definition(None, "Count", "Int", false).unwrap();
    let old = p.json.clone();
    assert!(p.set_field_value(K, None, f, json!("oops")).is_err());
    assert_eq!(p.json, old);
    let _ = Value::Null;
}
#[test]
fn symmetrical_entity_refs_update_both_ends_and_clear_previous_target() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Hero", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(e), "Friend", "EntityRef", false)
        .unwrap();
    p.update_field_definition(f, &json!({"symmetricalRef":true}))
        .unwrap();
    for x in [16, 32, 48] {
        p.add_entity(K, 0, e, x, 16).unwrap();
    }
    let ids: Vec<String> = p.layer(K, 0).unwrap()["entityInstances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["iid"].as_str().unwrap().into())
        .collect();
    p.set_field_value(K, Some(&ids[0]), f, json!(ids[1]))
        .unwrap();
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][0]["__value"]["entityIid"],
        ids[0]
    );
    p.set_field_value(K, Some(&ids[0]), f, json!(ids[2]))
        .unwrap();
    assert!(p.layer(K, 0).unwrap()["entityInstances"][1]["fieldInstances"][0]["__value"].is_null());
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"][2]["fieldInstances"][0]["__value"]["entityIid"],
        ids[0]
    );
}
#[test]
fn enum_rename_updates_internal_type_and_instances() {
    let mut p = Project::new();
    let e = p.add_enum_definition("State", &["Idle"]).unwrap();
    let f = p
        .add_field_definition(None, "State", "LocalEnum.State", false)
        .unwrap();
    p.set_field_value(K, None, f, json!("Idle")).unwrap();
    p.rename_definition("enums", e, "Mode").unwrap();
    assert_eq!(p.json["defs"]["levelFields"][0]["__type"], "LocalEnum.Mode");
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__type"],
        "LocalEnum.Mode"
    );
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], "Idle");
}
#[test]
fn absolute_file_fields_rebase_against_project_directory() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("game with spaces");
    let mut p = Project::new();
    p.path = Some(project_dir.join("project.ldtk"));
    let f = p
        .add_field_definition(None, "Path", "FilePath", false)
        .unwrap();
    let asset = project_dir.join("assets/hero.png");
    assert!(
        asset.is_absolute(),
        "fixture must be an absolute path on this host"
    );
    p.set_field_value(K, None, f, json!(asset.to_string_lossy()))
        .unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        "assets/hero.png"
    );
}
#[test]
#[ignore = "requires pinned upstream historical fixtures under .reference/ldtk"]
fn original_fixture_defaults_match_saved_public_values() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.reference/ldtk/tests");
    let mut count = 0;
    for name in [
        "labRefs.ldtk",
        "lab1.ldtk",
        "lab2.ldtk",
        "multiLinesBug.ldtk",
        "lotsOfEntities.ldtk",
        "multiWorldsTest.ldtk",
    ] {
        let p = Project::load(root.join(name)).unwrap();
        let mut defs = p.json["defs"]["levelFields"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for e in p.json["defs"]["entities"].as_array().unwrap() {
            defs.extend(e["fieldDefs"].as_array().unwrap().iter().cloned());
        }
        for key in p.level_keys() {
            let l = p.level(key).unwrap();
            let mut fields = l["fieldInstances"].as_array().cloned().unwrap_or_default();
            for layer in l["layerInstances"].as_array().unwrap() {
                for e in layer["entityInstances"].as_array().unwrap() {
                    fields.extend(e["fieldInstances"].as_array().unwrap().iter().cloned());
                }
            }
            for f in fields {
                if f["realEditorValues"] == json!([]) {
                    let d = defs.iter().find(|d| d["uid"] == f["defUid"]).unwrap();
                    let generated = p.make_field_instance(d).unwrap();
                    assert_eq!(
                        generated["__value"], f["__value"],
                        "{name}: {}",
                        f["__identifier"]
                    );
                    count += 1;
                }
            }
        }
    }
    assert!(count > 40, "{count}");
}
#[test]
fn strings_trim_filter_regex_and_numeric_wrappers_store_clamped_values() {
    let mut p = Project::new();
    let f = p
        .add_field_definition(None, "Letters", "String", false)
        .unwrap();
    p.update_field_definition(f, &json!({"regex":"/[a-z]+/gi"}))
        .unwrap();
    p.set_field_value(K, None, f, json!("  A1b2C  ")).unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], "AbC");
    let n = p
        .add_field_definition(None, "Number", "Int", false)
        .unwrap();
    p.update_field_definition(n, &json!({"min":2,"max":8}))
        .unwrap();
    p.set_field_value(K, None, n, json!(99)).unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][1]["realEditorValues"][0]["params"][0],
        8
    );
}
#[test]
fn duplicate_field_identifier_patch_is_rejected_in_its_owner() {
    let mut p = Project::new();
    let f = p.add_field_definition(None, "First", "Int", false).unwrap();
    p.add_field_definition(None, "Second", "Int", false)
        .unwrap();
    let old = p.json.clone();
    assert!(
        p.update_field_definition(f, &json!({"identifier":"Second"}))
            .is_err()
    );
    assert_eq!(p.json, old);
}
#[test]
fn string_regex_supports_original_ecmascript_lookaround_and_backrefs() {
    let mut p = Project::new();
    let f = p
        .add_field_definition(None, "Words", "String", false)
        .unwrap();
    p.update_field_definition(f, &json!({"regex":"/(?<=#)([a-z]+)\\1/gi"}))
        .unwrap();
    p.set_field_value(K, None, f, json!("#abab no #cdcd"))
        .unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        "ababcdcd"
    );
}
#[test]
fn field_smart_color_and_tile_update_entity_export_caches() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Hero", 16, 16).unwrap();
    let color = p
        .add_field_definition(Some(e), "Tint", "Color", false)
        .unwrap();
    let tile = p
        .add_field_definition(Some(e), "Tile", "Tile", false)
        .unwrap();
    p.add_entity(K, 0, e, 16, 16).unwrap();
    let id = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    p.set_field_value(K, Some(&id), color, json!("#010203"))
        .unwrap();
    p.set_field_value(
        K,
        Some(&id),
        tile,
        json!({"tilesetUid":null,"x":0,"y":0,"w":16,"h":16}),
    )
    .unwrap();
    let ent = &p.layer(K, 0).unwrap()["entityInstances"][0];
    assert_eq!(ent["__smartColor"], "#010203");
    assert_eq!(ent["__tile"]["w"], 16);
}
#[test]
fn raw_definition_default_refresh_updates_public_fields() {
    let mut p = Project::new();
    p.add_field_definition(None, "Count", "Int", false).unwrap();
    p.json["defs"]["levelFields"][0]["defaultOverride"] = json!({"id":"V_Int","params":[9]});
    p.refresh_derived_data().unwrap();
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["__value"], 9);
}
#[test]
fn official_example_field_exports_match_original_saved_values() {
    fn fields(p: &Project) -> std::collections::BTreeMap<String, Value> {
        let mut out = std::collections::BTreeMap::new();
        for k in p.level_keys() {
            let l = p.level(k).unwrap();
            let id = l["iid"].as_str().unwrap();
            for f in l["fieldInstances"].as_array().unwrap() {
                out.insert(
                    format!("{id}:{}", f["defUid"]),
                    json!([f["__value"], f["__tile"]]),
                );
            }
            for li in l["layerInstances"].as_array().unwrap() {
                for e in li["entityInstances"].as_array().unwrap() {
                    let id = e["iid"].as_str().unwrap();
                    for f in e["fieldInstances"].as_array().unwrap() {
                        out.insert(
                            format!("{id}:{}", f["defUid"]),
                            json!([f["__value"], f["__tile"]]),
                        );
                    }
                }
            }
        }
        out
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/original");
    let mut count = 0;
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "ldtk") {
            continue;
        }
        let mut p = Project::load(&path).unwrap();
        let before = fields(&p);
        p.refresh_derived_data().unwrap();
        let after = fields(&p);
        for (id, value) in &before {
            assert_eq!(after.get(id), Some(value), "{}: {id}", path.display());
        }
        count += before.len();
    }
    assert!(count > 100);
    eprintln!("Compared {count} original saved field exports");
}
#[test]
fn changing_numeric_limits_swaps_reversed_bounds_and_clamps_default_override() {
    let mut p = Project::new();
    let f = p.add_field_definition(None, "Count", "Int", false).unwrap();
    p.set_field_default(f, json!(9)).unwrap();
    p.update_field_definition(f, &json!({"min":8,"max":3}))
        .unwrap();
    let d = &p.json["defs"]["levelFields"][0];
    assert_eq!(d["min"], 3);
    assert_eq!(d["max"], 8);
    assert_eq!(d["defaultOverride"]["params"][0], 8);
}
#[test]
fn field_definition_reordering_reorders_matching_instances_without_changing_ids() {
    let mut p = Project::new();
    let a = p.add_field_definition(None, "A", "Int", false).unwrap();
    let b = p.add_field_definition(None, "B", "Int", false).unwrap();
    p.set_field_value(K, None, a, json!(7)).unwrap();
    p.reorder_field_definition(b, 0).unwrap();
    assert_eq!(p.json["defs"]["levelFields"][0]["uid"], b);
    assert_eq!(p.level(K).unwrap()["fieldInstances"][0]["defUid"], b);
    assert_eq!(p.level(K).unwrap()["fieldInstances"][1]["defUid"], a);
    assert_eq!(p.level(K).unwrap()["fieldInstances"][1]["__value"], 7);
}
#[test]
fn enum_value_rename_updates_biome_requirements_and_tileset_tags() {
    let mut p = Project::new();
    let e = p.add_enum_definition("Biome", &["Grass", "Snow"]).unwrap();
    let f = p
        .add_field_definition(None, "Biome", "LocalEnum.Biome", false)
        .unwrap();
    let layer = p.add_layer("Terrain", "IntGrid", 16).unwrap();
    let ts = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    p.json["defs"]["layers"][0]["biomeFieldUid"] = json!(f);
    p.json["defs"]["layers"][0]["autoRuleGroups"] = json!([{"uid":100,"active":true,"requiredBiomeValues":["Grass"],"biomeRequirementMode":1,"rules":[]}]);
    p.json["defs"]["tilesets"][0]["tagsSourceEnumUid"] = json!(e);
    p.json["defs"]["tilesets"][0]["enumTags"] = json!([{"enumValueId":"Grass","tileIds":[0]}]);
    p.rename_enum_value(e, "Grass", "Forest").unwrap();
    assert_eq!(
        p.layer_def(layer).unwrap()["autoRuleGroups"][0]["requiredBiomeValues"],
        json!(["Forest"])
    );
    assert_eq!(
        p.tileset(ts).unwrap()["enumTags"][0]["enumValueId"],
        "Forest"
    );
    p.remove_enum_value(e, "Forest").unwrap();
    assert_eq!(
        p.layer_def(layer).unwrap()["autoRuleGroups"][0]["requiredBiomeValues"],
        json!([])
    );
    assert_eq!(p.tileset(ts).unwrap()["enumTags"], json!([]));
}
#[test]
fn removing_biome_field_clears_layer_biome_gating() {
    let mut p = Project::new();
    p.add_enum_definition("Biome", &["Grass"]).unwrap();
    let f = p
        .add_field_definition(None, "Biome", "LocalEnum.Biome", false)
        .unwrap();
    let layer = p.add_layer("Auto", "IntGrid", 16).unwrap();
    p.json["defs"]["layers"][0]["biomeFieldUid"] = json!(f);
    p.json["defs"]["layers"][0]["autoRuleGroups"] =
        json!([{"uid":100,"active":true,"requiredBiomeValues":["Grass"],"rules":[]}]);
    p.remove_field_definition(f).unwrap();
    assert!(p.layer_def(layer).unwrap()["biomeFieldUid"].is_null());
    assert_eq!(
        p.layer_def(layer).unwrap()["autoRuleGroups"][0]["requiredBiomeValues"],
        json!([])
    );
}
#[test]
fn retargeting_a_symmetric_link_clears_displaced_reverse_links() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let e = p.add_entity_def("Actor", 16, 16).unwrap();
    let f = p
        .add_field_definition(Some(e), "Link", "EntityRef", false)
        .unwrap();
    p.update_field_definition(f, &json!({"symmetricalRef":true}))
        .unwrap();
    for x in [16, 32, 48, 64] {
        p.add_entity(K, 0, e, x, 16).unwrap();
    }
    let ids: Vec<String> = p.layer(K, 0).unwrap()["entityInstances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["iid"].as_str().unwrap().into())
        .collect();
    p.set_field_value(K, Some(&ids[0]), f, json!(ids[1]))
        .unwrap();
    p.set_field_value(K, Some(&ids[2]), f, json!(ids[3]))
        .unwrap();
    p.set_field_value(K, Some(&ids[0]), f, json!(ids[2]))
        .unwrap();
    assert!(p.layer(K, 0).unwrap()["entityInstances"][3]["fieldInstances"][0]["__value"].is_null());
}
#[test]
fn removing_enum_definition_clears_tileset_tag_source_and_values() {
    let mut p = Project::new();
    let e = p.add_enum_definition("Tags", &["Grass"]).unwrap();
    let ts = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 32)
        .unwrap();
    p.json["defs"]["tilesets"][0]["tagsSourceEnumUid"] = json!(e);
    p.json["defs"]["tilesets"][0]["enumTags"] = json!([{"enumValueId":"Grass","tileIds":[0]}]);
    p.remove_definition("enums", e).unwrap();
    assert!(p.tileset(ts).unwrap()["tagsSourceEnumUid"].is_null());
    assert_eq!(p.tileset(ts).unwrap()["enumTags"], json!([]));
}
#[test]
fn enum_values_patch_tidies_removed_and_truly_unknown_values() {
    let mut p = Project::new();
    let en = p.add_enum_definition("Kind", &["A", "B"]).unwrap();
    let f = p
        .add_field_definition(None, "Kind", "LocalEnum.Kind", true)
        .unwrap();
    p.set_field_value(K, None, f, json!(["A", "B", "LegacyUnknown"]))
        .unwrap();
    p.update_enum_definition(
        en,
        &json!({"values":[{"id":"B","color":null,"tileRect":null}]}),
    )
    .unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        json!([null, "B", null])
    );
}

#[test]
fn editor_array_values_can_cross_length_limits_while_strict_api_stays_atomic() {
    let mut p = Project::new();
    let uid = p
        .add_field_definition(None, "Numbers", "Int", true)
        .unwrap();
    p.update_field_definition(uid, &json!({"arrayMinLength":3,"arrayMaxLength":4}))
        .unwrap();
    let before = p.json.clone();
    assert!(p.set_field_value(K, None, uid, json!([1])).is_err());
    assert_eq!(p.json, before);
    p.set_field_editor_value(K, None, uid, json!([1])).unwrap();
    assert_eq!(
        p.field_value_errors(K, None, uid).unwrap(),
        vec!["Array too short"]
    );
    p.set_field_editor_value(K, None, uid, json!([1, 2, 3]))
        .unwrap();
    assert!(p.field_value_errors(K, None, uid).unwrap().is_empty());
    p.set_field_editor_value(K, None, uid, json!([])).unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        json!([])
    );
    let before = p.json.clone();
    assert!(
        p.set_field_editor_value(K, None, uid, json!(["not a number"]))
            .is_err()
    );
    assert_eq!(p.json, before);
}

#[test]
fn editor_field_errors_match_required_values_and_do_not_validate_regex() {
    let mut p = Project::new();
    for kind in ["Point", "FilePath", "EntityRef", "Tile"] {
        let uid = p.add_field_definition(None, kind, kind, false).unwrap();
        p.update_field_definition(uid, &json!({"canBeNull":false}))
            .unwrap();
        p.set_field_editor_value(K, None, uid, Value::Null).unwrap();
        assert_eq!(
            p.field_value_errors(K, None, uid).unwrap(),
            vec!["Value required"],
            "{kind}"
        );
        p.update_field_definition(uid, &json!({"canBeNull":true}))
            .unwrap();
        assert!(
            p.field_value_errors(K, None, uid).unwrap().is_empty(),
            "{kind}"
        );
    }
    let uid = p
        .add_field_definition(None, "Code", "String", false)
        .unwrap();
    p.update_field_definition(uid, &json!({"regex":"/^[0-9]{3}$/","canBeNull":false}))
        .unwrap();
    p.set_field_editor_value(K, None, uid, json!("12")).unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["__value"],
        ""
    );
    assert!(p.field_value_errors(K, None, uid).unwrap().is_empty());
    p.set_field_editor_value(K, None, uid, json!("123"))
        .unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["__value"],
        "123"
    );
}

#[test]
fn editor_field_errors_report_missing_files_and_lost_references() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing-field-file.txt");
    assert!(!missing.exists());
    let mut p = Project::new();
    let path = p
        .add_field_definition(None, "File", "FilePath", false)
        .unwrap();
    p.set_field_editor_value(K, None, path, json!(missing.to_string_lossy()))
        .unwrap();
    assert_eq!(
        p.field_value_errors(K, None, path).unwrap(),
        vec!["File not found"]
    );
    let reference = p
        .add_field_definition(None, "Ref", "EntityRef", false)
        .unwrap();
    // Existing files can contain a reference whose target was removed externally.
    p.level_mut(K).unwrap()["fieldInstances"][1]["realEditorValues"] =
        json!([{"id":"V_String","params":["missing-iid"]}]);
    p.level_mut(K).unwrap()["fieldInstances"][1]["__value"] =
        json!({"entityIid":"missing-iid","layerIid":"?","levelIid":"?","worldIid":"?"});
    assert_eq!(
        p.field_value_errors(K, None, reference).unwrap(),
        vec!["Lost reference!"]
    );
}
