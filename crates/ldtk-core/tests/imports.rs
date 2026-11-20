use ldtk_core::{LevelKey, Project, TilesetRemap};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
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

#[test]
fn ogmo_reports_a_removed_entity_definition_and_imports_the_remaining_levels() {
    let directory = tempfile::tempdir().unwrap();
    let levels = directory.path().join("levels");
    fs::create_dir(&levels).unwrap();
    let source = directory.path().join("game.ogmo");
    fs::write(
        &source,
        json!({
            "ogmoVersion": "3.4.0", "levelPaths": ["levels"], "tilesets": [],
            "layers": [{"name": "Actors", "definition": "entity", "gridSize": {"x": 16, "y": 16}}],
            "entities": [{"name": "Player", "size": {"x": 8, "y": 16}, "origin": {"x": 0, "y": 0}, "values": []}]
        }).to_string(),
    ).unwrap();
    let player = json!({"name": "Player", "x": 16, "y": 12});
    for (name, entities) in [
        (
            "room.json",
            json!([{"name": "Removed Enemy", "x": 0, "y": 0}, player]),
        ),
        ("second.json", json!([player])),
    ] {
        fs::write(
            levels.join(name),
            json!({"width": 32, "height": 32, "layers": [{"name": "Actors", "entities": entities}]}).to_string(),
        ).unwrap();
    }

    let mut report = Project::import_ogmo_with_report(source).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("Removed Enemy"));
    assert!(report.warnings[0].contains("room.json"));
    let output = directory.path().join("game.ldtk");
    report.project.save(&output).unwrap();

    let reopened = Project::load(output).unwrap();
    let keys = reopened.level_keys();
    assert_eq!(keys.len(), 2);
    for (key, name) in keys.into_iter().zip(["room", "second"]) {
        assert_eq!(reopened.level(key).unwrap()["identifier"], name);
        let entities = reopened.layer(key, 0).unwrap()["entityInstances"]
            .as_array()
            .unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0]["__identifier"], "Player");
        assert_eq!(entities[0]["px"], json!([16, 12]));
    }
}

fn ogmo_atlas_fixture(directory: &Path) -> PathBuf {
    let levels = directory.join("levels");
    fs::create_dir(&levels).unwrap();
    let source = directory.join("game.ogmo");
    fs::write(
        &source,
        json!({
            "ogmoVersion": "3.4.0", "levelPaths": ["levels"], "entities": [],
            "layers": [{"name": "Terrain", "definition": "tile", "defaultTileset": "Atlas"}],
            "tilesets": [{"label": "Atlas", "path": "atlas.png", "tileWidth": 16, "tileHeight": 16,
                          "tileMarginX": 1, "tileSeparationX": 2}]
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        levels.join("room.json"),
        json!({"width": 32, "height": 16, "layers": [{"name": "Terrain", "tileset": "Atlas",
               "data": [1, 3], "tileFlags": [0, 4]}]})
        .to_string(),
    )
    .unwrap();
    source
}

#[test]
fn ogmo_keeps_tiles_when_the_atlas_has_to_be_reattached() {
    let directory = tempfile::tempdir().unwrap();
    let source = ogmo_atlas_fixture(directory.path());
    let mut report = Project::import_ogmo_with_report(source).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("atlas.png"));
    let path = directory.path().join("game.ldtk");
    report.project.save(&path).unwrap();
    let mut imported = Project::load(&path).unwrap();
    imported.migrate_to_current().unwrap();
    let atlas = &imported.json["defs"]["tilesets"][0];
    let uid = atlas["uid"].as_i64().unwrap();
    assert_eq!(atlas["identifier"], "Atlas");
    assert!(atlas["relPath"].is_null());
    assert_eq!(atlas["pxWid"], 0);
    assert_eq!(atlas["pxHei"], 0);
    assert_eq!(atlas["__cWid"], 0);
    assert_eq!(atlas["__cHei"], 0);
    let level = imported.level_keys()[0];
    let layer = imported.layer(level, 0).unwrap();
    assert_eq!(layer["__tilesetDefUid"], uid);
    assert!(layer["__tilesetRelPath"].is_null());
    let tiles = json!([
        {"px": [0, 0], "src": [19, 1], "t": 1, "f": 0, "d": [0], "a": 1},
        {"px": [16, 0], "src": [55, 1], "t": 3, "f": 1, "d": [1], "a": 1}
    ]);
    assert_eq!(layer["gridTiles"], tiles);

    image::RgbaImage::new(36, 36)
        .save(directory.path().join("atlas.png"))
        .unwrap();
    let remapped = imported
        .update_tileset_definition_with_policy(
            uid,
            &json!({"relPath": "atlas.png", "pxWid": 36, "pxHei": 36}),
            TilesetRemap::AtlasCoordinates,
            false,
        )
        .unwrap();
    assert_eq!(remapped, 0);
    imported.save(&path).unwrap();
    let attached = Project::load(&path).unwrap();
    let layer = attached.layer(level, 0).unwrap();
    assert_eq!(layer["__tilesetDefUid"], uid);
    assert_eq!(layer["__tilesetRelPath"], "atlas.png");
    let mut expected = tiles;
    expected[1]["src"] = json!([19, 19]);
    assert_eq!(layer["gridTiles"], expected);
}

#[test]
fn ogmo_corrupt_atlas_retains_path_and_replacement_remaps_from_zero_columns() {
    let directory = tempfile::tempdir().unwrap();
    let source = ogmo_atlas_fixture(directory.path());
    fs::write(directory.path().join("atlas.png"), b"invalid PNG").unwrap();

    let mut report = Project::import_ogmo_with_report(source).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("atlas.png"));
    assert!(report.warnings[0].contains("Image loading failed"));
    let path = directory.path().join("game.ldtk");
    report.project.json["exportTiled"] = json!(true);
    report.project.save_with_exports(&path).unwrap();
    let tmx = fs::read_to_string(directory.path().join("game/tiled/room.tmx")).unwrap();
    assert!(tmx.contains("tilecount=\"0\" columns=\"0\""));
    assert!(tmx.contains("source=\"../../atlas.png\""));

    let mut reopened = Project::load(&path).unwrap();
    reopened.migrate_to_current().unwrap();
    let atlas = &reopened.json["defs"]["tilesets"][0];
    let uid = atlas["uid"].as_i64().unwrap();
    assert_eq!(atlas["relPath"], "atlas.png");
    for key in ["pxWid", "pxHei", "__cWid", "__cHei"] {
        assert_eq!(atlas[key], 0, "{key}");
    }
    let level = reopened.level_keys()[0];
    let layer = reopened.layer(level, 0).unwrap();
    let first_tile = json!({"px": [0, 0], "src": [19, 1], "t": 1, "f": 0, "d": [0], "a": 1});
    let second_tile = json!({"px": [16, 0], "src": [55, 1], "t": 3, "f": 1, "d": [1], "a": 1});
    assert_eq!(layer["__tilesetDefUid"], uid);
    assert_eq!(layer["__tilesetRelPath"], "atlas.png");
    assert_eq!(layer["gridTiles"], json!([first_tile, second_tile]));

    image::RgbaImage::new(36, 36)
        .save(directory.path().join("fixed.png"))
        .unwrap();
    let patch = json!({"relPath": "fixed.png", "pxWid": 36, "pxHei": 36});
    let before = reopened.json.clone();
    let refused = reopened
        .update_tileset_definition_with_policy(uid, &patch, TilesetRemap::AtlasCoordinates, false)
        .unwrap_err();
    assert!(refused.to_string().contains("would discard tiles"));
    assert_eq!(reopened.json, before);
    let removed = reopened
        .update_tileset_definition_with_policy(uid, &patch, TilesetRemap::AtlasCoordinates, true)
        .unwrap();
    assert_eq!(removed, 1);
    reopened.save(&path).unwrap();
    let attached = Project::load(&path).unwrap();
    let layer = attached.layer(level, 0).unwrap();
    assert_eq!(layer["__tilesetDefUid"], uid);
    assert_eq!(layer["__tilesetRelPath"], "fixed.png");
    assert_eq!(layer["gridTiles"], json!([first_tile]));
}
