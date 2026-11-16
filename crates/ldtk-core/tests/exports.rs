use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
use std::fs;
fn project() -> Project {
    let mut p = Project::new();
    p.json["levels"][0]["pxWid"] = json!(32);
    p.json["levels"][0]["pxHei"] = json!(16);
    p.json["defs"]["layers"] = json!([{"uid":1,"identifier":"Solid","type":"IntGrid","gridSize":16,"displayOpacity":1.0,"intGridValues":[{"value":1,"color":"#ff0000"},{"value":257,"color":"#00ff00"}]}]);
    p.json["levels"][0]["layerInstances"] = json!([{"layerDefUid":1,"__identifier":"Solid","__type":"IntGrid","__gridSize":16,"__cWid":2,"__cHei":1,"intGridCsv":[1,257],"gridTiles":[],"autoLayerTiles":[],"entityInstances":[]}]);
    p
}
#[test]
fn csv_and_simplified_preserve_source_schema_and_byte_csv() {
    let p = project();
    let t = tempfile::tempdir().unwrap();
    p.export_simplified(t.path()).unwrap();
    let simple: Value =
        serde_json::from_slice(&fs::read(t.path().join("Level_0/data.json")).unwrap()).unwrap();
    assert_eq!(simple["identifier"], "Level_0");
    assert_eq!(
        simple["uniqueIdentifer"],
        p.level(LevelKey {
            world: None,
            level: 0
        })
        .unwrap()["iid"]
    );
    assert_eq!(simple["layers"], json!([]));
    assert_eq!(
        fs::read_to_string(t.path().join("Level_0/Solid.csv")).unwrap(),
        "1,1\n"
    );
}
#[test]
fn tmx_exports_colored_intgrid_and_world_position() {
    let p = project();
    let t = tempfile::tempdir().unwrap();
    p.export_tiled(t.path()).unwrap();
    let xml = fs::read_to_string(t.path().join("Level_0.tmx")).unwrap();
    assert!(xml.contains("tilewidth=\"16\""));
    assert!(xml.contains("<data encoding=\"csv\">1,257</data>"));
    assert!(t.path().join("Solid.intgrid.png").exists());
    let w: Value =
        serde_json::from_slice(&fs::read(t.path().join("Untitled.world")).unwrap()).unwrap();
    assert_eq!(w["maps"][0]["fileName"], "Level_0.tmx");
}
#[test]
fn png_renders_intgrid_pixels() {
    let p = project();
    let t = tempfile::tempdir().unwrap();
    p.export_png(t.path()).unwrap();
    let image = image::open(t.path().join("Level_0__Solid-int.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(image.dimensions(), (2, 1));
    assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [0, 255, 0, 255]);
}
#[test]
fn gamemaker_matches_original_experimental_room_schema() {
    let p = project();
    let t = tempfile::tempdir().unwrap();
    p.export_gamemaker(t.path()).unwrap();
    let room: Value =
        serde_json::from_slice(&fs::read(t.path().join("Level_0.yy")).unwrap()).unwrap();
    assert_eq!(room["resourceType"], "GMRoom");
    assert_eq!(room["roomSettings"]["Width"], 32);
    assert_eq!(room["layers"][0]["resourceType"], "?");
}
#[test]
fn configured_exports_report_errors_after_publishing_document() {
    let mut p = project();
    p.json["simplifiedExport"] = json!(true);
    let t = tempfile::tempdir().unwrap();
    let report = p.save_with_exports(t.path().join("game.ldtk")).unwrap();
    assert!(report.export_errors.is_empty());
    assert!(t.path().join("game/simplified/Level_0/data.json").exists());
    assert!(
        t.path()
            .join("game/simplified/Level_0/Solid-int.png")
            .exists()
    );
    assert!(!report.outputs.is_empty());
}
#[test]
fn tiled_preserves_unsigned_flip_bits_and_stacked_tiles() {
    let mut p = project();
    p.json["defs"]["layers"][0]["type"] = json!("Tiles");
    p.json["defs"]["tilesets"] = json!([{"uid":7,"identifier":"Atlas","relPath":"atlas.png","pxWid":32,"pxHei":16,"tileGridSize":16}]);
    let li = &mut p.json["levels"][0]["layerInstances"][0];
    li["__type"] = json!("Tiles");
    li["__tilesetDefUid"] = json!(7);
    li["gridTiles"] = json!([{"px":[0,0],"t":0,"f":3},{"px":[0,0],"t":1,"f":0}]);
    let t = tempfile::tempdir().unwrap();
    p.export_tiled(t.path()).unwrap();
    let xml = fs::read_to_string(t.path().join("Level_0.tmx")).unwrap();
    assert!(xml.contains("3221225473,0"));
    assert!(xml.contains("name=\"Solid_1\""));
    assert!(xml.contains("name=\"Solid_2\""));
    assert!(xml.contains(">2,0</data>"));
}
#[test]
fn png_uses_source_tileset_pivot_when_atlas_grid_differs_from_layer_grid() {
    let t = tempfile::tempdir().unwrap();
    let atlas = image::RgbaImage::from_pixel(32, 32, image::Rgba([255, 0, 0, 255]));
    atlas.save(t.path().join("atlas.png")).unwrap();
    let mut p = project();
    p.path = Some(t.path().join("map.ldtk"));
    p.json["levels"][0]["pxHei"] = json!(32);
    p.json["defs"]["layers"][0]["type"] = json!("Tiles");
    p.json["defs"]["layers"][0]["tilePivotX"] = json!(0.5);
    p.json["defs"]["layers"][0]["tilePivotY"] = json!(0.5);
    p.json["defs"]["tilesets"] = json!([{"uid":7,"identifier":"Atlas","relPath":"atlas.png","pxWid":32,"pxHei":32,"tileGridSize":32}]);
    let li = &mut p.json["levels"][0]["layerInstances"][0];
    li["__type"] = json!("Tiles");
    li["__tilesetDefUid"] = json!(7);
    li["gridTiles"] = json!([{"px":[0,0],"src":[0,0],"t":0,"f":0}]);
    p.export_png(t.path().join("out")).unwrap();
    let image = image::open(t.path().join("out/Level_0__Solid.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(image.get_pixel(23, 23)[3], 255);
    assert_eq!(image.get_pixel(24, 24)[3], 0);
}
#[test]
fn all_original_sample_exports_use_real_atlases_and_complete_files() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/original");
    let mut files: Vec<_> = fs::read_dir(base)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ldtk"))
        .collect();
    files.sort();
    assert!(files.len() >= 14);
    let mut total = 0;
    for file in files {
        let project = Project::load(&file).unwrap();
        let t = tempfile::tempdir().unwrap();
        let png = project
            .export_png(t.path().join("png"))
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        total += png.len();
        project
            .export_simplified(t.path().join("simple"))
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        if let Err(error) = project.export_tiled(t.path().join("tmx")) {
            assert!(
                error.to_string().contains("invalid Tiled custom data"),
                "{}: {error}",
                file.display()
            );
            assert!(
                project.json["defs"]["tilesets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|td| td["customData"]
                        .as_array()
                        .is_some_and(|data| data.iter().any(|d| serde_json::from_str::<Value>(
                            d["data"].as_str().unwrap_or("")
                        )
                        .is_err())))
            );
            eprintln!("Original Tiled limitation: {}: {error}", file.display());
        }
        project
            .export_gamemaker(t.path().join("gms"))
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    }
    assert!(total > 20);
    eprintln!(
        "Exported {total} original sample PNG files with real Aseprite/PNG/embedded resources"
    );
}
#[test]
fn simplified_entity_colors_are_rgb_integers_and_instance_order_is_preserved() {
    let mut p = project();
    p.json["levels"][0]["layerInstances"] = json!([{"__identifier":"Top","__type":"Entities","entityInstances":[{"__identifier":"Player","iid":"top","px":[1,2],"width":4,"height":8,"__smartColor":"#FF0000","fieldInstances":[]}]},{"__identifier":"Bottom","__type":"Entities","entityInstances":[{"__identifier":"Player","iid":"bottom","px":[3,4],"width":4,"height":8,"__smartColor":"#0000FF","fieldInstances":[]}]}]);
    let t = tempfile::tempdir().unwrap();
    p.export_simplified(t.path()).unwrap();
    let value: Value =
        serde_json::from_slice(&fs::read(t.path().join("Level_0/data.json")).unwrap()).unwrap();
    assert_eq!(value["entities"]["Player"][0]["iid"], "top");
    assert_eq!(value["entities"]["Player"][0]["color"], 16711680);
}
#[test]
fn png_recomputes_background_scale_and_source_composite_background_quirk() {
    let t = tempfile::tempdir().unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(t.path().join("bg.png"))
        .unwrap();
    let mut p = Project::new();
    p.path = Some(t.path().join("map.ldtk"));
    p.json["levels"][0]["pxWid"] = json!(2);
    p.json["levels"][0]["pxHei"] = json!(2);
    p.json["levels"][0]["bgRelPath"] = json!("bg.png");
    p.json["levels"][0]["bgPos"] = json!("CoverDirty");
    p.json["imageExportMode"] = json!("LayersAndLevels");
    p.export_png(t.path().join("out")).unwrap();
    let bg = image::open(t.path().join("out/Level_0_bg.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(bg.get_pixel(1, 1).0, [255, 0, 0, 255]);
    let composite = image::open(t.path().join("out/Level_0.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(composite.get_pixel(0, 0).0, [105, 106, 121, 255]);
}
#[test]
fn tiled_reports_source_format_limitations() {
    let mut p = project();
    p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"uid":2,"identifier":"Different","type":"Tiles","gridSize":8}));
    p.json["defs"]["tilesets"] = json!([{"uid":7,"identifier":"Atlas","relPath":"atlas.aseprite","pxWid":32,"pxHei":16,"tileGridSize":16,"padding":1}]);
    let t = tempfile::tempdir().unwrap();
    let report = p.export_tiled_with_report(t.path()).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("grid size")));
    assert!(report.warnings.iter().any(|w| w.contains("padding")));
    assert!(report.warnings.iter().any(|w| w.contains("Aseprite")));
    assert!(!report.outputs.is_empty());
}
#[test]
fn tiled_custom_data_emits_scalar_properties_and_rejects_malformed_json() {
    let mut p = project();
    p.json["defs"]["tilesets"] = json!([{"uid":7,"identifier":"Atlas","relPath":"atlas.png","pxWid":16,"pxHei":16,"tileGridSize":16,"customData":[{"tileId":0,"data":"{\"solid\":true,\"cost\":7,\"skip\":[1]}"}]}]);
    let t = tempfile::tempdir().unwrap();
    p.export_tiled(t.path()).unwrap();
    let xml = fs::read_to_string(t.path().join("Level_0.tmx")).unwrap();
    assert!(xml.contains("name=\"solid\" type=\"bool\" value=\"true\""));
    assert!(xml.contains("name=\"cost\" type=\"int\" value=\"7\""));
    assert!(!xml.contains("name=\"skip\""));
    p.json["defs"]["tilesets"][0]["customData"][0]["data"] = json!("invalid");
    assert!(p.export_tiled(t.path().join("bad")).is_err());
}
#[test]
fn tiled_entity_fields_match_source_point_tile_string_and_pivot_conversions() {
    let mut p = project();
    p.json["defs"]["entities"] = json!([{"uid":2,"width":16,"height":16,"pivotX":1,"pivotY":1}]);
    p.json["defs"]["layers"][0]["type"] = json!("Entities");
    let li = &mut p.json["levels"][0]["layerInstances"][0];
    li["__type"] = json!("Entities");
    li["entityInstances"] = json!([{"__identifier":"Player","defUid":2,"px":[16,16],"fieldInstances":[{"__identifier":"tile","__type":"Tile","__value":{"tilesetUid":9,"x":1,"y":2,"w":3,"h":4}},{"__identifier":"point","__type":"Point","__value":null},{"__identifier":"text","__type":"String","__value":"a\\nb"}]}]);
    let t = tempfile::tempdir().unwrap();
    p.export_tiled(t.path()).unwrap();
    let xml = fs::read_to_string(t.path().join("Level_0.tmx")).unwrap();
    assert!(xml.contains("name=\"__cx\" type=\"int\" value=\"0\""));
    assert!(xml.contains("name=\"tile\" type=\"tile\" value=\"1,2,3,4\""));
    assert!(xml.contains("name=\"point\" value=\"null\""));
    assert!(xml.contains("name=\"text\" value=\"a\nb\""));
}
#[test]
fn configured_exports_retire_stale_generated_files_and_preserve_unrelated_files() {
    let mut p = project();
    p.json["imageExportMode"] = json!("OneImagePerLayer");
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("game.ldtk");
    p.save_with_exports(&path).unwrap();
    let dir = t.path().join("game/png");
    assert!(dir.join("Level_0__Solid-int.png").exists());
    fs::write(dir.join("keep.txt"), "unrelated").unwrap();
    p.json["levels"][0]["identifier"] = json!("Renamed");
    p.save_with_exports(&path).unwrap();
    assert!(!dir.join("Level_0__Solid-int.png").exists());
    assert!(dir.join("Renamed__Solid-int.png").exists());
    p.json["imageExportMode"] = json!("None");
    p.save_with_exports(&path).unwrap();
    assert!(!dir.join("Renamed__Solid-int.png").exists());
    assert_eq!(
        fs::read_to_string(dir.join("keep.txt")).unwrap(),
        "unrelated"
    );
}
