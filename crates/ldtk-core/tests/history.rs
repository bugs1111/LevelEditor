use ldtk_core::{History, LevelKey, Project};
use serde_json::json;
const K: LevelKey = LevelKey {
    world: None,
    level: 0,
};
#[test]
fn feedback_bounds_follow_source_expansion_and_do_not_leak_between_transactions() {
    let mut p = Project::new();
    let mut h = History::default();
    h.mark_rect_change([900., 900., 1., 1.]); // no pending edit
    h.begin(&p, K);
    h.mark_rect_change([10., 20., 16., 16.]);
    h.mark_rect_change([30., 5., 16., 16.]);
    p.move_level(K, 4, 0, 0).unwrap();
    assert!(h.commit(&p));
    // The first source rectangle includes x+w; later expansions use x+w-1.
    assert_eq!(
        h.level_change_bounds(&p, K, false),
        Some([10., 5., 35., 31.])
    );
    assert!(h.undo_level(&mut p, K));
    assert_eq!(
        h.level_change_bounds(&p, K, true),
        Some([10., 5., 35., 31.])
    );
    assert!(h.redo_level(&mut p, K));
    h.begin(&p, K);
    h.mark_rect_change([800., 800., 8., 8.]);
    assert!(!h.commit(&p));
    h.begin(&p, K);
    h.mark_rect_change([700., 700., 7., 7.]);
    p.move_level(K, 8, 0, 0).unwrap();
    assert!(h.cancel(&mut p));
    h.begin(&p, K);
    p.move_level(K, 12, 0, 0).unwrap();
    assert!(h.commit(&p));
    assert_eq!(h.level_change_bounds(&p, K, false), None);
}
#[test]
fn feedback_marks_raw_layer_offsets_and_rounds_entity_pivots() {
    let mut p = Project::new();
    let layer_uid = p.add_layer("Actors", "Entities", 16).unwrap();
    p.update_layer_properties(layer_uid, &json!({"pxOffsetX":3,"pxOffsetY":-4}))
        .unwrap();
    let uid = p.add_entity_def("Actor", 17, 19).unwrap();
    p.update_entity_definition(uid, &json!({"pivotX":0.5,"pivotY":0.5}))
        .unwrap();
    p.add_entity(K, 0, uid, 10, 20).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    h.mark_entity_change(&p, K, 0, &p.layer(K, 0).unwrap()["entityInstances"][0]);
    p.move_level(K, 4, 0, 0).unwrap();
    assert!(h.commit(&p));
    assert_eq!(
        h.level_change_bounds(&p, K, false),
        Some([5., 7., 17., 19.])
    );
    h.begin_layer(&p, K, 0);
    h.mark_grid_change(&p, K, 0, -1, 2);
    p.move_level(K, 8, 0, 0).unwrap();
    assert!(h.commit(&p));
    assert_eq!(
        h.level_change_bounds(&p, K, false),
        Some([-13., 28., 16., 16.])
    );
}
#[test]
fn history_restores_external_documents_and_changed_root_properties_only() {
    let mut p = Project::new();
    let full = p.json["levels"][0].clone();
    p.json["levels"][0]["externalRelPath"] = json!("level.ldtkl");
    p.json["levels"][0]["layerInstances"] = serde_json::Value::Null;
    p.external_levels.insert("level.ldtkl".into(), full);
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 8, 16, 2).unwrap();
    assert!(h.commit(&p));
    p.json["levels"][0]["future"] = json!("later");
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    assert_eq!(p.json["levels"][0]["future"], "later");
    assert!(h.redo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 8);
}
#[test]
fn bounded_history_discards_oldest_actions_and_keeps_redo_correct() {
    let mut p = Project::new();
    let mut h = History::default();
    for i in 1..=35 {
        h.begin_project(&p);
        p.json["defaultGridSize"] = json!(i);
        assert!(h.commit(&p));
    }
    for _ in 0..30 {
        assert!(h.undo(&mut p));
    }
    assert!(!h.undo(&mut p));
    assert_eq!(p.json["defaultGridSize"], 5);
    for _ in 0..30 {
        assert!(h.redo(&mut p));
    }
    assert_eq!(p.json["defaultGridSize"], 35);
}
#[test]
fn failed_empty_or_unfinished_transactions_do_not_create_actions() {
    let mut p = Project::new();
    let mut h = History::default();
    assert!(!h.commit(&p));
    h.begin_project(&p);
    assert!(!h.commit(&p));
    h.begin_project(&p);
    p.json["defaultGridSize"] = json!(99);
    assert!(h.cancel(&mut p));
    assert_eq!(p.json["defaultGridSize"], 16);
    assert!(!h.can_undo());
}
#[test]
fn level_insertion_after_commit_does_not_redirect_entity_undo() {
    let mut p = Project::new();
    let mut h = History::default();
    h.begin_project(&p);
    p.move_level(K, 20, 40, 0).unwrap();
    h.commit(&p);
    let mut later = p.json["levels"][0].clone();
    later["iid"] = json!("later");
    later["worldX"] = json!(999);
    p.json["levels"].as_array_mut().unwrap().insert(0, later);
    assert!(h.undo(&mut p));
    assert_eq!(p.json["levels"][0]["worldX"], 999);
    assert_eq!(p.json["levels"][1]["worldX"], 0);
}
#[test]
fn undo_structural_addition_preserves_later_level() {
    let mut p = Project::new();
    let mut h = History::default();
    h.begin_project(&p);
    p.add_level(None, "Second", 32, 32).unwrap();
    h.commit(&p);
    p.add_level(None, "Later", 32, 32).unwrap();
    assert!(h.undo(&mut p));
    let names: Vec<_> = p.json["levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["identifier"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Level_0", "Later"]);
    assert!(h.redo(&mut p));
    assert_eq!(p.json["levels"].as_array().unwrap().len(), 3);
}

#[test]
fn external_history_survives_save_as_and_retains_current_paths_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.set_external_levels(true).unwrap();
    p.save(dir.path().join("first.ldtk")).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 20, 40, 0).unwrap();
    h.commit(&p);
    p.save(dir.path().join("second.ldtk")).unwrap();
    let path = p.json["levels"][0]["externalRelPath"].clone();
    p.json["levels"][0]["stubFuture"] = json!("keep");
    p.level_mut(K).unwrap()["documentFuture"] = json!("also keep");
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    assert_eq!(p.json["levels"][0]["externalRelPath"], path);
    assert_eq!(p.json["levels"][0]["stubFuture"], "keep");
    assert_eq!(p.level(K).unwrap()["documentFuture"], "also keep");
    assert!(h.redo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 20);
    assert_eq!(p.external_levels.len(), 1);
}

#[test]
fn inline_history_survives_externalization_and_saved_toggle_undo_redo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.ldtk");
    let mut p = Project::new();
    p.save(&path).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 20, 40, 0).unwrap();
    h.commit(&p);
    h.begin_project(&p);
    p.set_external_levels(true).unwrap();
    h.commit(&p);
    p.save(&path).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.json["externalLevels"], false);
    assert_eq!(p.level(K).unwrap()["worldX"], 20);
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    assert!(h.redo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 20);
    assert!(h.redo(&mut p));
    p.save(&path).unwrap();
    assert_eq!(Project::load(path).unwrap().level(K).unwrap()["worldX"], 20);
}

#[test]
fn external_history_survives_inlining_and_renamed_level_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.ldtk");
    let mut p = Project::new();
    p.set_external_levels(true).unwrap();
    p.save(&path).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.level_mut(K).unwrap()["identifier"] = json!("Renamed");
    h.commit(&p);
    p.save(&path).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["identifier"], "Level_0");
    assert!(h.redo(&mut p));
    p.set_external_levels(false).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["identifier"], "Level_0");
    assert!(p.external_levels.is_empty());
}

#[test]
fn resource_path_undo_after_save_as_remains_relative_to_new_project_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("moved")).unwrap();
    let mut p = Project::new();
    p.json["defs"]["tilesets"] = json!([{"uid":1,"relPath":"before.png"}]);
    p.save(dir.path().join("first.ldtk")).unwrap();
    let mut h = History::default();
    h.begin_project(&p);
    p.json["defs"]["tilesets"][0]["relPath"] = json!("after.png");
    h.commit(&p);
    p.save(dir.path().join("moved/second.ldtk")).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.json["defs"]["tilesets"][0]["relPath"], "../before.png");
    assert!(h.redo(&mut p));
    assert_eq!(p.json["defs"]["tilesets"][0]["relPath"], "../after.png");
}

#[test]
fn inline_edit_undo_follows_a_level_externalized_after_the_action() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 20, 40, 0).unwrap();
    h.commit(&p);
    p.set_external_levels(true).unwrap();
    p.save(dir.path().join("external.ldtk")).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    assert_eq!(p.json["levels"][0]["worldX"], 0);
    assert_eq!(p.external_levels.len(), 1);
    assert!(h.redo(&mut p));
    assert_eq!(p.level(K).unwrap()["worldX"], 20);
}

#[test]
fn deleted_external_level_can_be_undone_and_saved_after_save_as() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.add_level(None, "Second", 32, 32).unwrap();
    p.set_external_levels(true).unwrap();
    p.save(dir.path().join("first.ldtk")).unwrap();
    let original = p.level(K).unwrap().clone();
    let mut h = History::default();
    h.begin_project(&p);
    p.remove_level(K).unwrap();
    h.commit(&p);
    p.save(dir.path().join("second.ldtk")).unwrap();
    assert!(h.undo(&mut p));
    assert_eq!(p.level(K).unwrap(), &original);
    p.save(dir.path().join("second.ldtk")).unwrap();
    assert_eq!(
        p.json["levels"][0]["externalRelPath"],
        "second/Level_0.ldtkl"
    );
    assert!(h.redo(&mut p));
    assert_eq!(p.level_keys().len(), 1);
    assert_eq!(p.level(K).unwrap()["identifier"], "Second");
    assert_eq!(p.external_levels.len(), 1);
}

#[test]
fn level_undo_reverts_selected_level_and_preserves_other_level_redo() {
    let mut p = Project::new();
    let second = p.add_level(None, "Second", 32, 32).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 10, 0, 0).unwrap();
    h.commit(&p);
    h.begin(&p, second);
    p.move_level(second, 20, 0, 0).unwrap();
    h.commit(&p);
    assert!(h.undo_level(&mut p, K));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    assert_eq!(p.level(second).unwrap()["worldX"], 20);
    assert!(h.undo_level(&mut p, second));
    assert!(h.redo_level(&mut p, K));
    assert_eq!(p.level(K).unwrap()["worldX"], 10);
    assert_eq!(p.level(second).unwrap()["worldX"], 0);
    assert!(h.can_redo_level(&p, second));
    assert!(h.redo_level(&mut p, second));
    assert_eq!(p.level(second).unwrap()["worldX"], 20);
}

#[test]
fn layer_history_restores_dependency_output_but_not_unrelated_layers() {
    let mut p = Project::new();
    let active = p.add_layer("Walls", "IntGrid", 16).unwrap();
    let auto = p.add_layer("Auto", "AutoLayer", 16).unwrap();
    let killer = p.add_layer("KillerOutput", "IntGrid", 16).unwrap();
    let unrelated = p.add_layer("Unrelated", "Tiles", 16).unwrap();
    for def in p.json["defs"]["layers"].as_array_mut().unwrap() {
        if def["uid"] == auto {
            def["autoSourceLayerDefUid"] = json!(active);
        }
        if def["uid"] == killer {
            def["autoTilesKilledByOtherLayerUid"] = json!(active);
        }
    }
    let index = p.level(K).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .position(|l| l["layerDefUid"] == active)
        .unwrap();
    let mut h = History::default();
    h.begin_layer(&p, K, index);
    for layer in p.level_mut(K).unwrap()["layerInstances"]
        .as_array_mut()
        .unwrap()
    {
        if layer["layerDefUid"] == active {
            layer["intGridCsv"][0] = json!(1);
        }
        if layer["layerDefUid"] == auto || layer["layerDefUid"] == killer {
            layer["autoLayerTiles"] = json!([{"t":42}]);
        }
        if layer["layerDefUid"] == unrelated {
            layer["future"] = json!("independent");
        }
    }
    assert!(h.commit(&p));
    assert!(h.undo_level(&mut p, K));
    for layer in p.level(K).unwrap()["layerInstances"].as_array().unwrap() {
        if layer["layerDefUid"] == active {
            assert_eq!(layer["intGridCsv"][0], 0);
        }
        if layer["layerDefUid"] == auto || layer["layerDefUid"] == killer {
            assert_eq!(layer["autoLayerTiles"], json!([]));
        }
        if layer["layerDefUid"] == unrelated {
            assert_eq!(layer["future"], "independent");
        }
    }
    assert!(h.redo_level(&mut p, K));
}

#[test]
fn scoped_layer_history_follows_external_save_as_and_level_reordering() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.add_layer("Walls", "IntGrid", 16).unwrap();
    p.add_level(None, "Second", 32, 32).unwrap();
    p.set_external_levels(true).unwrap();
    p.save(dir.path().join("a.ldtk")).unwrap();
    let mut h = History::default();
    h.begin_layer(&p, K, 0);
    p.paint_int_grid(K, 0, 0, 0, 1).unwrap();
    h.commit(&p);
    p.save(dir.path().join("b.ldtk")).unwrap();
    p.json["levels"].as_array_mut().unwrap().swap(0, 1);
    let moved = LevelKey {
        world: None,
        level: 1,
    };
    assert!(h.undo_level(&mut p, moved));
    assert_eq!(p.layer(moved, 0).unwrap()["intGridCsv"][0], 0);
    assert!(h.redo_level(&mut p, moved));
    assert_eq!(p.layer(moved, 0).unwrap()["intGridCsv"][0], 1);
    assert!(p.external_levels.keys().all(|key| key.starts_with("b/")));
}

#[test]
fn source_timeline_capacity_is_independent_for_each_level() {
    let mut p = Project::new();
    let second = p.add_level(None, "Second", 32, 32).unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.move_level(K, 1, 0, 0).unwrap();
    h.commit(&p);
    for x in 1..=40 {
        h.begin(&p, second);
        p.move_level(second, x, 0, 0).unwrap();
        h.commit(&p);
    }
    assert!(h.undo_level(&mut p, K));
    assert_eq!(p.level(K).unwrap()["worldX"], 0);
    let mut count = 0;
    while h.undo_level(&mut p, second) {
        count += 1;
    }
    assert_eq!(count, 29);
    assert_eq!(p.level(second).unwrap()["worldX"], 11);
}

#[test]
fn selective_entity_undo_repairs_later_inbound_references() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(def), "Link", "EntityRef", false)
        .unwrap();
    let second = p.add_level(None, "Second", 64, 64).unwrap();
    p.add_entity(second, 0, def, 16, 16).unwrap();
    let owner = p.layer(second, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut h = History::default();
    h.begin(&p, K);
    p.add_entity(K, 0, def, 16, 16).unwrap();
    h.commit(&p);
    let created = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"].clone();
    p.set_field_value(second, Some(&owner), field, created)
        .unwrap();
    assert!(h.undo_level(&mut p, K));
    assert!(
        p.layer(second, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"].is_null()
    );
}

#[test]
fn selective_entity_undo_does_not_overwrite_newer_reference_edits_in_other_levels() {
    let mut p = Project::new();
    p.add_layer("Actors", "Entities", 16).unwrap();
    let def = p.add_entity_def("Actor", 16, 16).unwrap();
    let field = p
        .add_field_definition(Some(def), "Link", "EntityRef", false)
        .unwrap();
    let second = p.add_level(None, "Second", 64, 64).unwrap();
    p.add_entity(K, 0, def, 16, 16).unwrap();
    p.add_entity(second, 0, def, 16, 16).unwrap();
    p.add_entity(second, 0, def, 32, 16).unwrap();
    let original = p.layer(K, 0).unwrap()["entityInstances"][0]["iid"].clone();
    let owner = p.layer(second, 0).unwrap()["entityInstances"][0]["iid"]
        .as_str()
        .unwrap()
        .to_owned();
    let later = p.layer(second, 0).unwrap()["entityInstances"][1]["iid"].clone();
    p.set_field_value(second, Some(&owner), field, original)
        .unwrap();
    let mut h = History::default();
    h.begin(&p, K);
    p.remove_entity(K, 0, 0).unwrap();
    h.commit(&p);
    p.set_field_value(second, Some(&owner), field, later.clone())
        .unwrap();
    assert!(h.undo_level(&mut p, K));
    assert_eq!(
        p.layer(K, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        p.layer(second, 0).unwrap()["entityInstances"][0]["fieldInstances"][0]["__value"]["entityIid"],
        later
    );
}

#[test]
#[ignore = "release history latency measurement on pinned original large-level fixture"]
fn scoped_history_large_level_release_latency() {
    use std::time::{Duration, Instant};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/largeLevels/world1.ldtk");
    let mut p = Project::load(path).unwrap();
    p.migrate_to_current().unwrap();
    let key = p.level_keys()[0];
    let (layer, uid) = p.level(key).unwrap()["layerInstances"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find_map(|(index, l)| {
            (l["__type"] == "IntGrid").then(|| (index, l["layerDefUid"].as_i64().unwrap()))
        })
        .unwrap();
    let value = p.layer_def(uid).unwrap()["intGridValues"][0]["value"]
        .as_i64()
        .unwrap() as i32;
    let original = p.layer(key, layer).unwrap()["intGridCsv"][0]
        .as_i64()
        .unwrap() as i32;
    let replacement = if original == 0 { value } else { 0 };
    let mut h = History::default();
    let mut totals = Vec::new();
    for _ in 0..6 {
        let start = Instant::now();
        h.begin_layer(&p, key, layer);
        let begin = start.elapsed();
        p.paint_int_grid(key, layer, 0, 0, replacement).unwrap();
        let start = Instant::now();
        assert!(h.commit(&p));
        let commit = start.elapsed();
        let start = Instant::now();
        assert!(h.undo_level(&mut p, key));
        let undo = start.elapsed();
        assert_eq!(p.layer(key, layer).unwrap()["intGridCsv"][0], original);
        eprintln!("scoped history begin={begin:?} commit={commit:?} undo={undo:?}");
        totals.push(begin + commit);
    }
    let warm = totals[1..].iter().copied().sum::<Duration>() / 5;
    eprintln!("warm mean begin+commit={warm:?}; target: approximately 16ms in release mode");
}

#[test]
fn structural_invalidation_clears_level_undo_redo_but_keeps_project_history() {
    let mut p = Project::new();
    let second = p.add_level(None, "Second", 32, 32).unwrap();
    let mut h = History::default();
    h.begin_project(&p);
    p.json["defaultGridSize"] = json!(32);
    h.commit(&p);
    h.begin(&p, K);
    p.move_level(K, 10, 0, 0).unwrap();
    h.commit(&p);
    h.begin(&p, second);
    p.move_level(second, 20, 0, 0).unwrap();
    h.commit(&p);
    assert!(h.undo_level(&mut p, second));
    h.clear_level_timelines();
    assert!(!h.can_undo_level(&p, K));
    assert!(!h.can_redo_level(&p, second));
    assert!(h.undo(&mut p));
    assert_eq!(p.json["defaultGridSize"], 16);
    assert_eq!(p.level(K).unwrap()["worldX"], 10);
    assert_eq!(p.level(second).unwrap()["worldX"], 0);
    h.begin_project(&p);
    p.json["defaultGridSize"] = json!(24);
    h.clear_level_timelines();
    assert!(h.commit(&p));
    assert!(h.undo(&mut p));
    assert_eq!(p.json["defaultGridSize"], 16);
    h.begin(&p, K);
    p.move_level(K, 30, 0, 0).unwrap();
    h.clear_level_timelines();
    assert!(!h.commit(&p));
    assert_eq!(p.level(K).unwrap()["worldX"], 30);
}
