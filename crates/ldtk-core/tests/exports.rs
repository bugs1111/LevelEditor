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
fn configured_tiled_grid_follows_source_definition_order() {
    let directory = tempfile::tempdir().unwrap();
    for (index, (sizes, grid)) in [
        (&[16, 32][..], 16),
        (&[32, 16][..], 32),
        (&[8, 8, 8, 16, 16][..], 16),
        (&[8, 16, 16, 8, 8][..], 8),
    ]
    .into_iter()
    .enumerate()
    {
        let mut project = Project::new();
        let level = project.level_keys()[0];
        project.resize_level(level, 32, 32, 0, 0).unwrap();
        for (layer, &size) in sizes.iter().enumerate().rev() {
            project
                .add_layer(&format!("Grid{layer}"), "IntGrid", size)
                .unwrap();
        }
        for layer in 0..sizes.len() {
            assert!(project.paint_int_grid(level, layer, 0, 0, 1).unwrap());
        }
        project.json["exportTiled"] = json!(true);
        let stem = format!("GridOrder{index}");
        let path = directory.path().join(format!("{stem}.ldtk"));
        let report = project.save_with_exports(&path).unwrap();
        assert_eq!(
            report.export_errors,
            [
                "Tiled supports one grid size per map; layers with incompatible grid size are discarded"
            ]
        );

        let xml =
            fs::read_to_string(directory.path().join(stem).join("tiled/Level_0.tmx")).unwrap();
        let cells = 32 / grid;
        assert!(
            xml.contains(&format!(
                "width=\"{cells}\" height=\"{cells}\" tilewidth=\"{grid}\" tileheight=\"{grid}\""
            )),
            "definition sizes {sizes:?} should export a {grid}px map: {xml}"
        );
        for (layer, &size) in sizes.iter().enumerate() {
            assert_eq!(
                xml.contains(&format!("name=\"Grid{layer}\" width=")),
                size == grid
            );
        }
    }
}

#[test]
fn configured_tiled_grid_without_layers_uses_the_project_default() {
    let directory = tempfile::tempdir().unwrap();
    for (stem, default, grid) in [("Configured", Some(24), 24), ("Legacy", None, 16)] {
        let mut project = Project::new();
        let level = project.level_keys()[0];
        project.resize_level(level, 48, 48, 0, 0).unwrap();
        if let Some(default) = default {
            project.json["defaultGridSize"] = json!(default);
        } else {
            project.json["jsonVersion"] = json!("1.5.3");
            project
                .json
                .as_object_mut()
                .unwrap()
                .remove("defaultGridSize");
        }
        project.json["exportTiled"] = json!(true);
        let path = directory.path().join(format!("{stem}.ldtk"));
        let report = project.save_with_exports(&path).unwrap();
        assert!(
            report.export_errors.is_empty(),
            "{:?}",
            report.export_errors
        );

        let xml =
            fs::read_to_string(directory.path().join(stem).join("tiled/Level_0.tmx")).unwrap();
        let cells = 48 / grid;
        assert!(
            xml.contains(&format!(
                "width=\"{cells}\" height=\"{cells}\" tilewidth=\"{grid}\" tileheight=\"{grid}\""
            )),
            "{stem} should export a {grid}px map: {xml}"
        );
        assert!(!xml.contains("<layer "));
    }
}

#[test]
fn configured_tiled_export_uses_live_linear_world_positions() {
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Project::new();
    let first = fixture.level_keys()[0];
    fixture.resize_level(first, 32, 48, 0, 0).unwrap();
    fixture.add_level(None, "Second", 64, 96).unwrap();
    fixture.add_level(None, "Third", 80, 80).unwrap();
    fixture.json["exportTiled"] = json!(true);

    for (layout, expected) in [
        ("LinearHorizontal", [[0, 0], [80, 0], [192, 0]]),
        ("LinearVertical", [[0, 0], [0, 96], [0, 240]]),
    ] {
        let mut project = fixture.clone();
        project.set_world_layout(None, layout, 256, 256).unwrap();
        project
            .set_external_levels(layout == "LinearVertical")
            .unwrap();
        let project_path = directory.path().join(format!("{layout}.ldtk"));
        let report = project.save_with_exports(&project_path).unwrap();
        assert!(
            report.export_errors.is_empty(),
            "{:?}",
            report.export_errors
        );

        let tiled = directory.path().join(layout).join("tiled");
        let world: Value =
            serde_json::from_slice(&fs::read(tiled.join(format!("{layout}.world"))).unwrap())
                .unwrap();
        let files = ["0001_Level_0.tmx", "0002_Second.tmx", "0003_Third.tmx"];
        let reopened = Project::load(&project_path).unwrap();
        for (index, key) in reopened.level_keys().into_iter().enumerate() {
            let level = reopened.level(key).unwrap();
            assert_eq!(
                [&level["worldX"], &level["worldY"]],
                [&json!(-1), &json!(-1)]
            );
            assert!(tiled.join(files[index]).is_file());
            assert_eq!(
                world["maps"][index],
                json!({
                    "fileName": files[index], "x": expected[index][0], "y": expected[index][1]
                })
            );
        }
    }
}

#[test]
fn configured_tiled_export_keeps_the_first_world_when_it_becomes_empty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.ldtk");
    let tiled = directory.path().join("game/tiled");
    let mut project = Project::new();
    let later_world = project.add_world("LaterWorld").unwrap();
    let later_level = project
        .add_level(Some(later_world), "LaterLevel", 32, 32)
        .unwrap();
    project.json["exportTiled"] = json!(true);
    project.save_with_exports(&path).unwrap();
    assert!(tiled.join("Level_0.tmx").is_file());
    assert!(!tiled.join("LaterLevel.tmx").exists());

    project
        .remove_level(LevelKey {
            world: Some(0),
            level: 0,
        })
        .unwrap();
    project.save_with_exports(&path).unwrap();
    let world_path = tiled.join("game.world");
    let world: Value = serde_json::from_slice(&fs::read(&world_path).unwrap()).unwrap();
    assert_eq!(world["maps"], json!([]));
    assert!(!tiled.join("Level_0.tmx").exists());
    assert!(!tiled.join("LaterLevel.tmx").exists());
    assert_eq!(
        Project::load(&path).unwrap().level(later_level).unwrap()["identifier"],
        "LaterLevel"
    );

    let report = project.export_tiled_with_report(&tiled).unwrap();
    assert_eq!(report.outputs, vec![world_path]);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("first world only"))
    );
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
fn intgrid_png_uses_first_palette_match_and_preserves_transparent_cells() {
    let mut p = project();
    p.json["defs"]["layers"][0]["intGridValues"] = json!([
        {"value":7,"color":"#aa113377"}, {"value":7,"color":"#ffffff"},
        {"value":0,"color":"#010203"}
    ]);
    let layer = &mut p.json["levels"][0]["layerInstances"][0];
    layer["__cWid"] = json!(4);
    layer["intGridCsv"] = json!([7, 999, null]);
    let dir = tempfile::tempdir().unwrap();
    p.export_png(dir.path()).unwrap();
    let image = image::open(dir.path().join("Level_0__Solid-int.png"))
        .unwrap()
        .to_rgba8();
    let pixels: Vec<_> = image.pixels().map(|p| p.0).collect();
    assert_eq!(
        pixels,
        [[170, 17, 51, 119], [0; 4], [1, 2, 3, 255], [1, 2, 3, 255]]
    );
}

#[test]
fn gamemaker_matches_original_experimental_room_schema() {
    let p = project();
    let t = tempfile::tempdir().unwrap();
    p.export_gamemaker(t.path()).unwrap();
    let room: Value =
        serde_json::from_slice(&fs::read(t.path().join("Level_0.yy")).unwrap()).unwrap();
    assert_eq!(room["resourceType"], "GMRoom");
    let inheritance: Vec<_> = room
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| key.starts_with("inherit"))
        .map(String::as_str)
        .collect();
    assert_eq!(
        inheritance,
        ["inheritCode", "inheritLayers", "inheritCreationOrder"]
    );
    assert_eq!(room["roomSettings"]["Width"], 32);
    assert_eq!(room["layers"][0]["resourceType"], "?");
}
#[test]
fn late_gamemaker_error_keeps_existing_room_and_discards_staged_files() {
    let mut p = project();
    p.json["defs"]["tilesets"] = json!([{"identifier":"Atlas"}]);
    let last = p.add_level(None, "Last", 32, 16).unwrap();
    p.level_mut(last).unwrap()["layerInstances"] =
        json!([{"__type":"Tiles", "__tilesetDefUid":42}]);
    let dir = tempfile::tempdir().unwrap();
    let room = dir.path().join("Level_0.yy");
    fs::write(&room, b"previous export").unwrap();

    let mut progress = Vec::new();
    let error = p
        .export_gamemaker_with_progress(dir.path(), |done, total| {
            progress.push((done, total));
            assert_eq!(fs::read(&room).unwrap(), b"previous export");
            assert!(!dir.path().join("Atlas.yy").exists());
        })
        .unwrap_err();
    assert!(error.to_string().contains("no tileset"));
    assert_eq!(progress, [(0, 2), (1, 2)]);
    assert_eq!(fs::read(room).unwrap(), b"previous export");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_late_level_failure_keeps_previous_png_and_simplified_exports() {
    let mut p = project();
    p.json["imageExportMode"] = json!("OneImagePerLevel");
    let invalid = p.add_level(None, "Invalid", 32, 16).unwrap();
    p.level_mut(invalid).unwrap()["pxWid"] = json!(0);
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("Level_0.png");
    fs::write(&png, b"previous PNG").unwrap();
    assert!(p.export_png(dir.path()).is_err());
    assert_eq!(fs::read(&png).unwrap(), b"previous PNG");

    let first = dir.path().join("Level_0");
    fs::create_dir_all(&first).unwrap();
    fs::write(first.join("data.json"), b"previous JSON").unwrap();
    assert!(p.export_simplified(dir.path()).is_err());
    assert_eq!(fs::read(first.join("data.json")).unwrap(), b"previous JSON");
    assert_eq!(fs::read_dir(first).unwrap().count(), 1);
    assert!(!dir.path().join("Invalid/data.json").exists());
}

#[test]
fn repeated_png_filenames_across_levels_are_rejected_before_publication() {
    let mut p = project();
    p.add_level(None, "Second", 32, 16).unwrap();
    p.json["imageExportMode"] = json!("OneImagePerLevel");
    p.json["pngFilePattern"] = json!("shared");
    let dir = tempfile::tempdir().unwrap();
    let error = p.export_png(dir.path()).unwrap_err();
    assert!(error.to_string().contains("duplicate export output"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
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

fn tiles_png_project() -> Project {
    let mut project = Project::new();
    let level = project.level_keys()[0];
    project.resize_level(level, 64, 48, 0, 0).unwrap();
    project.level_mut(level).unwrap()["identifier"] = json!("Tiles");
    project.add_layer("Ground", "Tiles", 16).unwrap();
    project.layer_mut(level, 0).unwrap()["gridTiles"] = json!([
        {"px":[0,0], "src":[0,0], "t":0, "f":0, "a":1, "d":[0]},
        {"px":[0,0], "src":[16,0], "t":1, "f":1, "a":1, "d":[0]},
        {"px":[32,16], "src":[48,0], "t":3, "f":3, "a":1, "d":[6]},
    ]);
    project.json["imageExportMode"] = json!("OneImagePerLayer");
    project.json["exportLevelBg"] = json!(false);
    project
}

#[test]
fn png_exports_follow_tileset_switches_and_reload_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = tiles_png_project();
    p.path = Some(dir.path().join("map.ldtk"));
    p.json["defs"]["tilesets"] = json!([
        {"uid":11, "tileGridSize":16, "relPath":"a.png"},
        {"uid":12, "tileGridSize":16, "relPath":"b.png"}
    ]);
    let template = p.json["levels"][0].clone();
    let mut levels = Vec::new();
    for (index, uid) in [11, 11, 12, 11].into_iter().enumerate() {
        let mut level = template.clone();
        level["identifier"] = json!(format!("Level_{index}"));
        level["layerInstances"][0]["__tilesetDefUid"] = json!(uid);
        level["layerInstances"][0]["gridTiles"] = json!([{"px":[0,0], "src":[0,0], "t":0}]);
        levels.push(level);
    }
    p.json["levels"] = json!(levels);
    let write_atlas = |name, rgba| {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba))
            .save(dir.path().join(name))
            .unwrap();
    };
    let green = [0, 255, 0, 255];
    write_atlas("b.png", green);
    for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
        write_atlas("a.png", color);
        p.export_png(dir.path().join("png")).unwrap();
        p.export_simplified(dir.path().join("simple")).unwrap();
        for (index, expected) in [color, color, green, color].into_iter().enumerate() {
            for path in [
                format!("png/Level_{index}__Ground.png"),
                format!("simple/Level_{index}/Ground.png"),
            ] {
                let image = image::open(dir.path().join(path)).unwrap().to_rgba8();
                assert_eq!(image.get_pixel(0, 0).0, expected);
            }
        }
    }
}

#[test]
fn configured_classic_tiles_png_matches_source_for_unavailable_atlases() {
    // RGBA pixels measured from the pinned preview's Ctrl+S PNG for these cells.
    let marker = [
        "................",
        "................",
        "................",
        "...XX......XX...",
        "...XXX....XXX...",
        "....XXX..XXX....",
        ".....XXXXXX.....",
        "......XXXX......",
        "......XXXX......",
        ".....XXXXXX.....",
        "....XXX..XXX....",
        "...XXX....XXX...",
        "...XX......XX...",
        "................",
        "................",
        "................",
    ];
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("corrupt.png"), b"invalid PNG").unwrap();
    for (name, uid, atlas) in [
        ("Unassigned", None, None),
        ("Undefined", Some(17), None),
        ("Unattached", Some(17), Some(Value::Null)),
        ("Missing", Some(17), Some(json!("missing.png"))),
        ("Corrupt", Some(17), Some(json!("corrupt.png"))),
        ("Offset", Some(17), Some(Value::Null)),
    ] {
        let mut project = tiles_png_project();
        project.json["defs"]["layers"][0]["tilesetDefUid"] = json!(uid);
        if let Some(atlas) = atlas {
            project.json["defs"]["tilesets"] = json!([{
                "uid":17, "identifier":"Unusable", "relPath":atlas, "embedAtlas":null,
                "tileGridSize":16, "pxWid":0, "pxHei":0
            }]);
        }
        let (shift, dark, bright) = if name == "Offset" {
            let def = &mut project.json["defs"]["layers"][0];
            def["tilePivotX"] = json!(0.5);
            def["tilePivotY"] = json!(0.25);
            def["displayOpacity"] = json!(0.5);
            def["pxOffsetX"] = json!(3);
            def["pxOffsetY"] = json!(5);
            let level = project.level_keys()[0];
            let layer = project.layer_mut(level, 0).unwrap();
            layer["pxOffsetX"] = json!(4);
            layer["pxOffsetY"] = json!(6);
            layer["__pxTotalOffsetX"] = json!(7);
            layer["__pxTotalOffsetY"] = json!(11);
            (8, [68, 0, 0, 127], [127, 0, 0, 127])
        } else {
            (0, [136, 0, 0, 255], [255, 0, 0, 255])
        };
        let report = project
            .save_with_exports(directory.path().join(format!("{name}.ldtk")))
            .unwrap();
        assert!(
            report.export_errors.is_empty(),
            "{name}: {:?}",
            report.export_errors
        );
        let image = image::open(directory.path().join(name).join("png/Tiles__Ground.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (64, 48));
        for (x, y, pixel) in image.enumerate_pixels() {
            let (x, y) = (i64::from(x) - shift, i64::from(y) - shift);
            let expected = match (x, y) {
                (0..=15, 0..=15) => marker[y as usize].as_bytes()[x as usize],
                (32..=47, 16..=31) => marker[y as usize - 16].as_bytes()[x as usize - 32],
                _ => b' ',
            };
            let expected = match expected {
                b'X' => bright,
                b'.' => dark,
                _ => [0, 0, 0, 0],
            };
            assert_eq!(pixel.0, expected, "{name}, {x},{y}");
        }
    }
}

#[test]
fn configured_classic_tiles_png_applies_source_opacity_to_level_render() {
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = tiles_png_project();
    fixture.json["defs"]["layers"][0]["displayOpacity"] = json!(0.5);
    fixture.json["defaultLevelBgColor"] = json!("#696A79");
    for (name, mode, background, dark, bright, outside) in [
        (
            "Level",
            "OneImagePerLevel",
            false,
            [68, 0, 0, 127],
            [127, 0, 0, 127],
            [0, 0, 0, 0],
        ),
        (
            "Opaque",
            "OneImagePerLevel",
            true,
            [120, 53, 61, 255],
            [180, 53, 61, 255],
            [105, 106, 121, 255],
        ),
        (
            "Composite",
            "LayersAndLevels",
            false,
            [17, 0, 0, 63],
            [31, 0, 0, 63],
            [0, 0, 0, 0],
        ),
    ] {
        let mut project = fixture.clone();
        project.json["imageExportMode"] = json!(mode);
        project.json["exportLevelBg"] = json!(background);
        let report = project
            .save_with_exports(directory.path().join(format!("{name}.ldtk")))
            .unwrap();
        assert!(
            report.export_errors.is_empty(),
            "{name}: {:?}",
            report.export_errors
        );
        let image = image::open(directory.path().join(name).join("png/Tiles.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, dark, "{name}");
        assert_eq!(image.get_pixel(3, 3).0, bright, "{name}");
        assert_eq!(image.get_pixel(16, 0).0, outside, "{name}");
    }
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
fn configured_png_export_uses_level_color_for_unavailable_backgrounds() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("broken.png"), b"invalid PNG").unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(directory.path().join("valid.png"))
        .unwrap();
    let mut project = Project::new();
    let first = project.level_keys()[0];
    project.resize_level(first, 32, 32, 0, 0).unwrap();
    let broken = project.add_level(None, "Broken", 32, 32).unwrap();
    let valid = project.add_level(None, "Valid", 32, 32).unwrap();
    project.add_layer("Solid", "IntGrid", 16).unwrap();
    project.json["defaultLevelBgColor"] = json!("#102030");
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    for (key, resource, background) in [
        (first, "missing.png", json!("#445566")),
        (broken, "broken.png", Value::Null),
        (valid, "valid.png", Value::Null),
    ] {
        let level = project.level_mut(key).unwrap();
        level["bgRelPath"] = json!(resource);
        level["bgColor"] = background;
        level["bgPos"] = json!("CoverDirty");
    }
    let path = directory.path().join("game.ldtk");
    let report = project.save_with_exports(&path).unwrap();
    assert!(
        report.export_errors.is_empty(),
        "{:?}",
        report.export_errors
    );
    let diagnostics = report.export_diagnostics;
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(diagnostics[0].contains("missing.png for level Level_0"));
    assert!(diagnostics[1].contains("broken.png for level Broken"));
    let out = directory.path().join("game/png");
    for (name, expected) in [
        ("Level_0", [0x44, 0x55, 0x66, 255]),
        ("Broken", [0x10, 0x20, 0x30, 255]),
        ("Valid", [255, 0, 0, 255]),
    ] {
        let image = image::open(out.join(format!("{name}.png")))
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (32, 32));
        assert!(image.pixels().all(|pixel| pixel.0 == expected), "{name}");
    }
    let reopened = Project::load(&path).unwrap();
    assert_eq!(reopened.level(first).unwrap()["bgRelPath"], "missing.png");
    assert_eq!(reopened.level(broken).unwrap()["bgRelPath"], "broken.png");

    project.json["simplifiedExport"] = json!(true);
    let simplified = project.save_with_exports(&path).unwrap();
    assert!(simplified.export_errors.is_empty());
    assert_eq!(simplified.export_diagnostics, diagnostics);

    project.json["exportLevelBg"] = json!(false);
    let disabled = project.save_with_exports(&path).unwrap();
    assert!(disabled.export_errors.is_empty());
    assert!(disabled.export_diagnostics.is_empty());
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

#[test]
#[ignore = "requires locally generated Aseprite indexed-color fixture"]
fn preview_154_aseprite_indexed_image_matches_aseprite_png_export() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools/ldtk-154");
    for (source, expected) in [
        ("indexed-1.3.18.aseprite", "indexed-1.3.18.png"),
        ("opaque.aseprite", "opaque-export.png"),
    ] {
        let actual = ldtk_core::exports::load_image_rgba(&root.join(source)).unwrap();
        let expected = image::open(root.join(expected)).unwrap().to_rgba8();
        assert_eq!(actual.dimensions(), expected.dimensions());
        assert_eq!(actual.as_raw(), expected.as_raw(), "{source}");
    }
}

#[test]
fn preview_154_aseprite_atlases_keep_raw_pixels_and_read_both_palettes() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    // Both sprites have two frames and transparent palette index 2. Only the
    // second frame uses purple. The wide sprite uses the legacy opaque palette.
    for (asset, alpha) in [
        ("aseprite-indexed-wide.aseprite", 255),
        ("aseprite-indexed-alpha.aseprite", 128),
    ] {
        let image = ldtk_core::exports::load_image_rgba(&fixtures.join(asset)).unwrap();
        assert_eq!(image.dimensions(), (32, 32), "{asset}");
        for (x, y, pixel) in image.enumerate_pixels() {
            if y >= 16 || (x < 16 && y >= 8) {
                assert_eq!(pixel[3], 0, "{asset} ({x}, {y})");
            } else {
                let expected = if x < 8 {
                    [27, 233, 215, 255]
                } else if x < 16 {
                    [236, 74, 51, alpha]
                } else {
                    [82, 28, 188, 255]
                };
                assert_eq!(pixel.0, expected, "{asset} ({x}, {y})");
            }
        }
    }
}

#[test]
fn preview_154_png_export_progress_counts_completed_levels() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.add_level(None, "Second", 16, 16).unwrap();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let mut updates = Vec::new();
    let files = project
        .export_png_with_progress(dir.path(), |done, total| updates.push((done, total)))
        .unwrap();
    assert_eq!(updates, [(0, 2), (1, 2), (2, 2), (0, 0)]);
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|path| path.is_file()));
}

#[test]
fn preview_154_other_export_progress_tracks_real_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.add_level(None, "Second", 16, 16).unwrap();
    let mut updates = Vec::new();
    project
        .export_csv_with_progress(dir.path().join("csv"), |a, b| updates.push((a, b)))
        .unwrap();
    assert_eq!(updates, [(0, 2), (1, 2), (2, 2), (0, 0)]);
    updates.clear();
    project
        .export_gamemaker_with_progress(dir.path().join("gms"), |a, b| updates.push((a, b)))
        .unwrap();
    assert_eq!(updates, [(0, 2), (1, 2), (2, 2), (0, 0)]);
    updates.clear();
    project
        .export_tiled_with_report_progress(dir.path().join("tiled"), |a, b| updates.push((a, b)))
        .unwrap();
    assert_eq!(updates, [(0, 2), (1, 2), (2, 2), (0, 0)]);
    updates.clear();
    project
        .export_simplified_with_progress(dir.path().join("simple"), |a, b| updates.push((a, b)))
        .unwrap();
    assert_eq!(
        updates,
        [
            (0, 6),
            (1, 6),
            (2, 6),
            (3, 6),
            (4, 6),
            (5, 6),
            (6, 6),
            (0, 0)
        ]
    );
}
