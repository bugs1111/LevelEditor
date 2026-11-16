use ldtk_core::{LevelKey, Project};
use serde_json::json;
use std::fs;
#[test]
fn ogmo_import_maps_legend_offsets_tiles_and_fields() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("levels")).unwrap();
    let source = json!({"ogmoVersion":"3.4.0","name":"test","backgroundColor":"#123456ff","layerGridDefaultSize":{"x":16,"y":16},"tilesets":[],"entities":[{"name":"Player","size":{"x":8,"y":16},"origin":{"x":4,"y":16},"color":"#ff0000ff","values":[{"name":"Health","definition":"Integer","defaults":3,"display":0}]}],"layers":[{"name":"Walls","definition":"grid","gridSize":{"x":16,"y":16},"legend":{"empty":"#00000000","solid":"#ff00ffff"}},{"name":"Actors","definition":"entity","gridSize":{"x":16,"y":16}}],"levelPaths":["levels"]});
    fs::write(t.path().join("game.ogmo"), source.to_string()).unwrap();
    fs::write(t.path().join("levels/room.json"),json!({"width":32,"height":16,"offsetX":2,"offsetY":3,"layers":[{"name":"Walls","offsetX":4,"offsetY":5,"grid2D":[["empty","solid"]]},{"name":"Actors","entities":[{"name":"Player","x":8,"y":16,"values":{"Health":7}}]}]}).to_string()).unwrap();
    let p = Project::import_ogmo(t.path().join("game.ogmo")).unwrap();
    let l = p
        .level(LevelKey {
            world: None,
            level: 0,
        })
        .unwrap();
    assert_eq!(l["identifier"], "room");
    let layers = l["layerInstances"].as_array().unwrap();
    let walls = layers
        .iter()
        .find(|l| l["__identifier"] == "Walls")
        .unwrap();
    assert_eq!(walls["intGridCsv"], json!([0, 1]));
    assert_eq!(walls["__pxTotalOffsetX"], 6);
    assert_eq!(walls["__pxTotalOffsetY"], 8);
    let actors = layers
        .iter()
        .find(|l| l["__identifier"] == "Actors")
        .unwrap();
    let e = &actors["entityInstances"][0];
    assert_eq!(e["__pivot"], json!([0.5, 1]));
    assert_eq!(e["fieldInstances"][0]["__value"], 7);
    assert_eq!(p.json["bgColor"], "#123456");
}
#[test]
fn old_ogmo_versions_and_nonsquare_grids_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("x.ogmo");
    fs::write(&path, json!({"ogmoVersion":"3.2"}).to_string()).unwrap();
    assert!(Project::import_ogmo(&path).is_err());
}
#[test]
#[ignore = "requires pinned upstream checkout under .reference/ldtk"]
fn imports_original_ogmo_fixture_with_entities_tiles_and_intgrid() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.reference/ldtk/tests/ogmo/test/ogmoImport.ogmo");
    let p = Project::import_ogmo(root).unwrap();
    assert_eq!(p.level_keys().len(), 2);
    assert_eq!(p.json["defs"]["tilesets"].as_array().unwrap().len(), 2);
    assert_eq!(p.json["defs"]["entities"].as_array().unwrap().len(), 2);
    assert_eq!(p.json["defs"]["layers"].as_array().unwrap().len(), 3);
    let count: usize = p
        .level_keys()
        .iter()
        .map(|k| {
            p.level(*k).unwrap()["layerInstances"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["entityInstances"].as_array().unwrap().len())
                .sum::<usize>()
        })
        .sum();
    assert!(count >= 3);
    let temp = tempfile::tempdir().unwrap();
    p.export_tiled(temp.path()).unwrap();
}
#[test]
fn ogmo_directory_rows_use_maximum_level_height() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("levels/sub")).unwrap();
    fs::write(t.path().join("game.ogmo"),json!({"ogmoVersion":"3.4","levelPaths":["levels"],"layers":[],"tilesets":[],"entities":[]}).to_string()).unwrap();
    for (file, height) in [("a.json", 16), ("b.json", 64), ("sub/c.json", 32)] {
        fs::write(
            t.path().join("levels").join(file),
            json!({"width":16,"height":height,"layers":[]}).to_string(),
        )
        .unwrap();
    }
    let p = Project::import_ogmo(t.path().join("game.ogmo")).unwrap();
    assert_eq!(p.json["levels"][2]["worldY"], 128);
    assert_eq!(p.json["levels"][1]["worldX"], 32);
}
#[test]
fn ogmo_reports_unknown_grid_values_instead_of_silently_discarding_them() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("levels")).unwrap();
    fs::write(t.path().join("game.ogmo"),json!({"ogmoVersion":"3.4","levelPaths":["levels"],"layers":[{"name":"Wall","definition":"grid","legend":{"0":"#00000000","1":"#ff0000ff"}}],"tilesets":[],"entities":[]}).to_string()).unwrap();
    fs::write(
        t.path().join("levels/room.json"),
        json!({"width":16,"height":16,"layers":[{"name":"Wall","grid":["missing"]}]}).to_string(),
    )
    .unwrap();
    let report = Project::import_ogmo_with_report(t.path().join("game.ogmo")).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("missing")));
    assert_eq!(
        report.project.json["levels"][0]["layerInstances"][0]["intGridCsv"][0],
        0
    );
}
