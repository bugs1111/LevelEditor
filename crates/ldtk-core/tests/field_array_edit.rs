use ldtk_core::{FieldArrayEdit as Edit, LevelKey, Project};
use serde_json::json;
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
#[test]
fn editing_one_slot_keeps_other_defaults_live_and_preserves_extensions() {
    let mut p = Project::new();
    let uid = p.add_field_definition(None, "Flags", "Bool", true).unwrap();
    p.set_field_default(uid, json!(true)).unwrap();
    p.set_field_editor_value(K, None, uid, json!([null, false, false]))
        .unwrap();
    let f = &mut p.level_mut(K).unwrap()["fieldInstances"][0];
    f["extra"] = json!({"field":"metadata"});
    f["realEditorValues"][2]["extra"] = json!({"wrapper":"metadata"});
    let untouched = f["realEditorValues"][2].clone();
    p.edit_field_array(
        K,
        None,
        uid,
        Edit::Set {
            index: 1,
            value: json!(true),
        },
    )
    .unwrap();
    p.set_field_default(uid, json!(false)).unwrap();
    let f = &p.level(K).unwrap()["fieldInstances"][0];
    assert_eq!(f["__value"], json!([false, true, false]));
    assert_eq!(f["realEditorValues"][0], json!(null));
    assert_eq!(f["realEditorValues"][2], untouched);
    assert_eq!(f["extra"], json!({"field":"metadata"}));
}
#[test]
fn insert_remove_move_and_reset_preserve_original_wrappers_and_allow_length_diagnostics() {
    let mut p = Project::new();
    let uid = p
        .add_field_definition(None, "Words", "String", true)
        .unwrap();
    p.set_field_default(uid, json!("default")).unwrap();
    p.update_field_definition(uid, &json!({"arrayMinLength":2,"arrayMaxLength":2}))
        .unwrap();
    p.set_field_editor_value(K, None, uid, json!(["first", null]))
        .unwrap();
    p.level_mut(K).unwrap()["fieldInstances"][0]["realEditorValues"][0]["extension"] = json!(17);
    let first = p.level(K).unwrap()["fieldInstances"][0]["realEditorValues"][0].clone();
    p.edit_field_array(K, None, uid, Edit::Insert { index: 1 })
        .unwrap();
    assert!(!p.field_value_errors(K, None, uid).unwrap().is_empty());
    p.edit_field_array(K, None, uid, Edit::Move { from: 0, to: 2 })
        .unwrap();
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["realEditorValues"],
        json!([null, null, first])
    );
    p.edit_field_array(K, None, uid, Edit::Reset { index: 2 })
        .unwrap();
    p.edit_field_array(K, None, uid, Edit::Remove { index: 0 })
        .unwrap();
    p.edit_field_array(K, None, uid, Edit::Remove { index: 0 })
        .unwrap();
    assert!(!p.field_value_errors(K, None, uid).unwrap().is_empty());
    assert_eq!(
        p.level(K).unwrap()["fieldInstances"][0]["__value"],
        json!(["default"])
    );
}
#[test]
fn invalid_indices_and_non_array_targets_are_atomic_errors() {
    let mut p = Project::new();
    let uid = p.add_field_definition(None, "Count", "Int", true).unwrap();
    p.set_field_editor_value(K, None, uid, json!([1])).unwrap();
    let before = p.json.clone();
    for edit in [
        Edit::Set {
            index: 1,
            value: json!(2),
        },
        Edit::Reset { index: 1 },
        Edit::Remove { index: 1 },
        Edit::Insert { index: 2 },
        Edit::InsertValue { index: 2, value: json!(2) },
        Edit::Move { from: 0, to: 1 },
        Edit::Move { from: 1, to: 0 },
    ] {
        assert!(p.edit_field_array(K, None, uid, edit).is_err());
        assert_eq!(p.json, before);
    }
    let scalar = p
        .add_field_definition(None, "Scalar", "Int", false)
        .unwrap();
    let before = p.json.clone();
    assert!(
        p.edit_field_array(K, None, scalar, Edit::Insert { index: 0 })
            .is_err()
    );
    assert_eq!(p.json, before);
}
#[test]
fn set_normalizes_only_new_path_value_and_preserves_other_raw_strings() {
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("project with spaces");
    let mut p = Project::new();
    let uid = p
        .add_field_definition(None, "Files", "FilePath", true)
        .unwrap();
    p.path = Some(project_dir.join("World.ldtk"));
    p.set_field_editor_value(K, None, uid, json!(["old.txt"]))
        .unwrap();
    let before = p.level(K).unwrap()["fieldInstances"][0]["realEditorValues"][0].clone();
    p.edit_field_array(K, None, uid, Edit::Insert { index: 1 })
        .unwrap();
    for (path, expected) in [
        (project_dir.join("new.txt"), "new.txt"),
        (project_dir.join("assets/ui/new.txt"), "assets/ui/new.txt"),
        (dir.path().join("outside/new.txt"), "../outside/new.txt"),
    ] {
        assert!(
            path.is_absolute(),
            "fixture must be an absolute path on this host"
        );
        p.edit_field_array(
            K,
            None,
            uid,
            Edit::Set {
                index: 1,
                value: json!(path.to_string_lossy()),
            },
        )
        .unwrap();
        let f = &p.level(K).unwrap()["fieldInstances"][0];
        assert_eq!(f["__value"], json!(["old.txt", expected]));
        assert_eq!(f["realEditorValues"][0], before);
    }
}
#[test]
fn indexed_entity_reference_edits_update_both_ends_and_reject_invalid_targets_atomically() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let uid = p
        .add_field_definition(Some(def), "Links", "EntityRef", true)
        .unwrap();
    p.update_field_definition(uid, &json!({"symmetricalRef":true}))
        .unwrap();
    for x in [16, 32, 48] {
        p.add_entity(K, 0, def, x, 16).unwrap();
    }
    let ids: Vec<String> = p.layer(K, 0).unwrap()["entityInstances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["iid"].as_str().unwrap().to_owned())
        .collect();
    let field = |p: &Project, i: usize| {
        p.layer(K, 0).unwrap()["entityInstances"][i]["fieldInstances"][0].clone()
    };
    p.edit_field_array(K, Some(&ids[0]), uid, Edit::Insert { index: 0 })
        .unwrap();
    p.edit_field_array(
        K,
        Some(&ids[0]),
        uid,
        Edit::Set {
            index: 0,
            value: json!(ids[1]),
        },
    )
    .unwrap();
    assert_eq!(field(&p, 1)["__value"][0]["entityIid"], ids[0]);
    p.edit_field_array(K, Some(&ids[0]), uid, Edit::Insert { index: 1 })
        .unwrap();
    p.edit_field_array(
        K,
        Some(&ids[0]),
        uid,
        Edit::Set {
            index: 1,
            value: json!(ids[2]),
        },
    )
    .unwrap();
    let mut cleared = p.clone();
    cleared
        .set_field_editor_value(K, Some(&ids[0]), uid, json!([]))
        .unwrap();
    assert_eq!(field(&cleared, 0)["__value"], json!([]));
    assert_eq!(field(&cleared, 1)["__value"], json!([]));
    assert_eq!(field(&cleared, 2)["__value"], json!([]));
    let before = p.json.clone();
    assert!(
        p.edit_field_array(
            K,
            Some(&ids[0]),
            uid,
            Edit::Set {
                index: 0,
                value: json!("missing-iid")
            }
        )
        .is_err()
    );
    assert_eq!(p.json, before);
    p.edit_field_array(K, Some(&ids[0]), uid, Edit::Move { from: 0, to: 1 })
        .unwrap();
    assert_eq!(field(&p, 0)["__value"][0]["entityIid"], ids[2]);
    assert_eq!(field(&p, 1)["__value"][0]["entityIid"], ids[0]);
    p.edit_field_array(K, Some(&ids[0]), uid, Edit::Reset { index: 1 })
        .unwrap();
    assert_eq!(field(&p, 1)["__value"], json!([]));
    assert_eq!(field(&p, 0)["realEditorValues"][1], json!(null));
    p.edit_field_array(K, Some(&ids[0]), uid, Edit::Remove { index: 0 })
        .unwrap();
    assert_eq!(field(&p, 2)["__value"], json!([]));
    assert_eq!(field(&p, 0)["__value"], json!([null]));
}
