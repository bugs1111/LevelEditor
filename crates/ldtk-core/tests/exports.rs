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
fn png_tile_flipped_at_the_atlas_and_canvas_edges() {
    use image::{Rgba, RgbaImage};
    use ldtk_core::exports::{PngExportMode, PngExportOptions};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("edges.ldtk"));
    project.resize_level(key, 32, 16, 0, 0).unwrap();
    let layer = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 16, 16)
        .unwrap();
    project.assign_tileset(layer, atlas).unwrap();
    RgbaImage::from_fn(16, 16, |x, y| Rgba([x as u8, y as u8, 29, 255]))
        .save(dir.path().join("atlas.png"))
        .unwrap();
    project.layer_mut(key, 0).unwrap()["gridTiles"] = json!([
        {"px":[-4,-3],"src":[-2,-1],"f":3},
        {"px":[16,0],"src":[0,0],"f":3},
        {"px":[20,4],"src":[3,6],"f":0}
    ]);
    let options = PngExportOptions {
        mode: PngExportMode::OneImagePerLevel,
        include_background: false,
        file_pattern: None,
    };
    let files = project
        .export_png_with_options(dir.path().join("png"), &options)
        .unwrap();
    let image = image::open(&files[0]).unwrap().into_rgba8();
    for (x, y, pixel) in image.enumerate_pixels() {
        let expected = if x <= 9 && y <= 11 {
            [9 - x as u8, 11 - y as u8, 29, 255]
        } else if x >= 20 && (4..14).contains(&y) {
            [(x - 20 + 3) as u8, (y - 4 + 6) as u8, 29, 255]
        } else if x >= 16 {
            [31 - x as u8, 15 - y as u8, 29, 255]
        } else {
            [0; 4]
        };
        assert_eq!(pixel.0, expected, "{x},{y}");
    }
}

#[test]
fn png_skips_tiles_with_overflowing_coordinates() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("map.ldtk"));
    project.resize_level(key, 32, 16, 0, 0).unwrap();
    let layer_uid = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 16, 16)
        .unwrap();
    project.assign_tileset(layer_uid, atlas).unwrap();
    let color = image::Rgba([12, 34, 56, 255]);
    image::RgbaImage::from_pixel(16, 16, color)
        .save(dir.path().join("atlas.png"))
        .unwrap();
    project.json["exportLevelBg"] = json!(false);

    for (name, offset, tiles) in [
        (
            "destination",
            i64::MAX,
            json!([
                {"px":[-i64::MAX,0],"src":[0,0],"f":0},
                {"px":[1,0],"src":[0,0],"f":0},
            ]),
        ),
        (
            "atlas",
            0,
            json!([
                {"px":[0,0],"src":[0,0],"f":0},
                {"px":[16,0],"src":[i64::MAX,i64::MAX],"f":3},
                {"px":[16,0],"src":[i64::MIN,i64::MIN],"f":0},
            ]),
        ),
    ] {
        let layer = project.layer_mut(key, 0).unwrap();
        layer["__pxTotalOffsetX"] = json!(offset);
        layer["gridTiles"] = tiles;
        let files = project.export_png(dir.path().join(name)).unwrap();
        let image = image::open(&files[0]).unwrap().into_rgba8();
        for (x, y, pixel) in image.enumerate_pixels() {
            let expected = if x < 16 { color } else { image::Rgba([0; 4]) };
            assert_eq!(*pixel, expected, "{name} at {x},{y}");
        }
    }
}

#[test]
#[ignore = "manual PNG re-export performance and peak memory comparison"]
fn benchmark_large_png_reexport() {
    use image::{Rgba, RgbaImage};
    use ldtk_core::exports::{PngExportMode, PngExportOptions};
    use std::io::{BufRead, BufReader, Read};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("background.ldtk"));
    let (width, height) = (2048, 1536);
    project.resize_level(key, width, height, 0, 0).unwrap();
    project.level_mut(key).unwrap()["bgRelPath"] = json!("texture.png");
    let source = dir.path().join("texture.png");
    let mut state = 0x7A28B3D4u32;
    RgbaImage::from_fn(width as u32, height as u32, |_, _| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let [r, g, b, _] = state.to_le_bytes();
        Rgba([r, g, b, 255])
    })
    .save(&source)
    .unwrap();
    let options = PngExportOptions {
        mode: PngExportMode::OneImagePerLevel,
        include_background: true,
        file_pattern: None,
    };
    let target = dir.path().join("png");
    let mut times = [Duration::ZERO; 4];
    for time in &mut times {
        let start = Instant::now();
        let files = project.export_png_with_options(&target, &options).unwrap();
        *time = start.elapsed();
        let mut buffer = [0; 8192];
        let mut expected = BufReader::with_capacity(buffer.len(), fs::File::open(&source).unwrap());
        let mut actual = BufReader::new(fs::File::open(&files[0]).unwrap());
        loop {
            let block = expected.fill_buf().unwrap();
            if block.is_empty() {
                assert!(actual.fill_buf().unwrap().is_empty());
                break;
            }
            actual.read_exact(&mut buffer[..block.len()]).unwrap();
            assert_eq!(&buffer[..block.len()], block);
            let len = block.len();
            expected.consume(len);
        }
    }
    let (first, repeats) = times.split_first_mut().unwrap();
    eprintln!("Large PNG first export: {first:?}");
    repeats.sort_unstable();
    eprintln!(
        "Large PNG re-export: median {:?}; samples {repeats:?}",
        repeats[1]
    );
}

#[test]
#[ignore = "manual alternating PNG atlas performance comparison"]
fn benchmark_png_export_alternating_atlases() {
    use image::{Rgba, RgbaImage};
    use ldtk_core::exports::{PngExportMode, PngExportOptions};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    project.path = Some(dir.path().join("atlases.ldtk"));
    let first = project.level_keys()[0];
    project.resize_level(first, 32, 32, 0, 0).unwrap();
    for i in 1..8 {
        project
            .add_level(None, &format!("Room_{i}"), 32, 32)
            .unwrap();
    }
    let back = project.add_layer("Back", "Tiles", 16).unwrap();
    let front = project.add_layer("Front", "Tiles", 16).unwrap();
    let colors = [Rgba([35, 82, 125, 255]), Rgba([197, 201, 35, 255])];
    for (index, (layer, pixel)) in [back, front].into_iter().zip(colors).enumerate() {
        let name = format!("atlas-{index}.png");
        let atlas = project
            .add_tileset(std::path::Path::new(&name), 16, 1024, 1024)
            .unwrap();
        project.assign_tileset(layer, atlas).unwrap();
        RgbaImage::from_pixel(1024, 1024, pixel)
            .save(dir.path().join(name))
            .unwrap();
        for key in project.level_keys() {
            project.layer_mut(key, 1 - index).unwrap()["gridTiles"] = json!([
                {"px":[index * 16, 0],"src":[0,0],"t":0,"f":0}
            ]);
        }
    }
    let options = PngExportOptions {
        mode: PngExportMode::OneImagePerLevel,
        include_background: false,
        file_pattern: None,
    };
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let paths = project
            .export_png_with_options(dir.path().join(format!("png-{run}")), &options)
            .unwrap();
        *time = start.elapsed();
        assert_eq!(paths.len(), 8);
        for path in paths {
            let image = image::open(path).unwrap().into_rgba8();
            assert_eq!(image.dimensions(), (32, 32));
            for (x, y, pixel) in image.enumerate_pixels() {
                assert_eq!(
                    *pixel,
                    if y < 16 {
                        colors[(x / 16) as usize]
                    } else {
                        Rgba([0; 4])
                    }
                );
            }
        }
    }
    times.sort_unstable();
    eprintln!(
        "Alternating PNG atlases: median {:?}; samples {times:?}",
        times[2]
    );

    let updated = Rgba([4, 18, 31, 255]);
    RgbaImage::from_pixel(16, 16, updated)
        .save(dir.path().join("atlas-0.png"))
        .unwrap();
    let paths = project
        .export_png_with_options(dir.path().join("updated"), &options)
        .unwrap();
    for path in paths {
        let image = image::open(path).unwrap().into_rgba8();
        assert_eq!(*image.get_pixel(0, 0), updated);
        assert_eq!(*image.get_pixel(16, 0), colors[1]);
    }
}

#[test]
#[ignore = "manual dense PNG export performance comparison"]
fn benchmark_dense_png_export() {
    use image::{Rgba, RgbaImage};
    use ldtk_core::exports::{PngExportMode, PngExportOptions};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("dense.ldtk"));
    project.resize_level(key, 1024, 1024, 0, 0).unwrap();
    let ground = project.add_layer("Ground", "Tiles", 16).unwrap();
    let overlay = project.add_layer("Overlay", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(ground, atlas).unwrap();
    project.assign_tileset(overlay, atlas).unwrap();
    RgbaImage::from_fn(32, 16, |x, _| {
        if x < 16 {
            Rgba([100, 150, 200, 255])
        } else {
            Rgba([200, 20, 40, 128])
        }
    })
    .save(dir.path().join("atlas.png"))
    .unwrap();
    for (index, uid) in [overlay, ground].into_iter().enumerate() {
        assert_eq!(project.layer(key, index).unwrap()["layerDefUid"], uid);
        project.layer_mut(key, index).unwrap()["gridTiles"] = (0..64 * 64)
            .filter(|cell| {
                if uid == overlay {
                    cell % 2 == 1
                } else {
                    cell % 64 < 60
                }
            })
            .map(|cell| {
                json!({
                    "px":[cell % 64 * 16,cell / 64 * 16],
                    "src":[if uid == overlay {16} else {0},0],
                    "f":cell % 4,"a":1
                })
            })
            .collect::<Vec<_>>()
            .into();
    }
    let options = PngExportOptions {
        mode: PngExportMode::OneImagePerLevel,
        include_background: false,
        file_pattern: None,
    };
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let files = project
            .export_png_with_options(dir.path().join(format!("png-{run}")), &options)
            .unwrap();
        *time = start.elapsed();
        assert_eq!(files.len(), 1);
        let image = image::open(&files[0]).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (1024, 1024));
        for (x, _, pixel) in image.enumerate_pixels() {
            let cell = x / 16;
            let expected = match (cell < 60, cell % 2 == 1) {
                (true, false) => [100, 150, 200, 255],
                (true, true) => [150, 85, 120, 255],
                (false, true) => [200, 20, 40, 128],
                (false, false) => [0; 4],
            };
            assert_eq!(pixel.0, expected);
        }
    }
    times.sort_unstable();
    eprintln!("Dense PNG export: median {:?}; samples {times:?}", times[2]);
}

#[test]
#[ignore = "manual repeating PNG background performance comparison"]
fn benchmark_repeating_png_background() {
    benchmark_repeating_background("masked", &[0, 255, 255]);
    benchmark_repeating_background("translucent", &[0, 64, 128, 192, 255]);
}

fn benchmark_repeating_background(case: &str, alpha: &[u8]) {
    use image::{Rgba, RgbaImage};
    use ldtk_core::exports::{PngExportMode, PngExportOptions};
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("repeat.ldtk"));
    let (width, height) = (1536, 1024);
    let (source_width, source_height) = (47, 31);
    let (pivot_x, pivot_y) = (0.321, 0.647);
    project.resize_level(key, width, height, 0, 0).unwrap();
    let level = project.level_mut(key).unwrap();
    level["bgColor"] = json!("#101e28");
    level["bgRelPath"] = json!("bg.png");
    level["bgPos"] = json!("Repeat");
    level["bgPivotX"] = json!(pivot_x);
    level["bgPivotY"] = json!(pivot_y);
    let pixels = |x: u32, y: u32| {
        Rgba([
            (x * 3) as u8,
            (y * 5) as u8,
            121,
            alpha[(x + y) as usize % alpha.len()],
        ])
    };
    RgbaImage::from_fn(source_width, source_height, pixels)
        .save(dir.path().join("bg.png"))
        .unwrap();
    let options = PngExportOptions {
        mode: PngExportMode::OneImagePerLevel,
        include_background: true,
        file_pattern: None,
    };
    let mut times = [Duration::ZERO; 5];
    let mut fingerprint = None;
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let files = project
            .export_png_with_options(dir.path().join(format!("out-{run}")), &options)
            .unwrap();
        *time = start.elapsed();
        assert_eq!(files.len(), 1);
        let mut hasher = DefaultHasher::new();
        fs::read(&files[0]).unwrap().hash(&mut hasher);
        let digest = hasher.finish();
        assert_eq!(*fingerprint.get_or_insert(digest), digest);
        let output = image::open(&files[0]).unwrap().into_rgba8();
        assert_eq!(output.dimensions(), (width as u32, height as u32));
        let expected = |index: u32, output_size: i32, input_size: u32, pivot: f64| {
            (f64::from(index) - (f64::from(output_size) - f64::from(input_size)) * pivot)
                .rem_euclid(f64::from(input_size)) as u32
        };
        for (x, y, pixel) in output.enumerate_pixels() {
            let src = pixels(
                expected(x, width, source_width, pivot_x),
                expected(y, height, source_height, pivot_y),
            );
            let rgb = [16, 30, 40];
            let a = u32::from(src[3]);
            let blend = |channel| {
                let foreground = u32::from(src[channel]);
                let background = rgb[channel];
                ((foreground * a + background * (255 - a) + 127) / 255) as u8
            };
            assert_eq!(*pixel, Rgba([blend(0), blend(1), blend(2), 255]));
        }
    }
    times.sort_unstable();
    eprintln!(
        "Repeating PNG background {case}: median {:?}; output {:016x}; samples {times:?}",
        times[2],
        fingerprint.unwrap()
    );
}

#[test]
#[ignore = "manual entity-heavy Tiled export performance comparison"]
fn benchmark_tiled_entity_properties_export() {
    use std::hash::{DefaultHasher, Hash, Hasher};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.add_layer("Actors", "Entities", 16).unwrap();
    let raw_names: Vec<_> = (0..48).map(|n| format!("物品{n} < & > \"\\n")).collect();
    let count = 2048;
    project.layer_mut(key, 0).unwrap()["entityInstances"] = (0..count)
        .map(|n| json!({
            "__identifier":"Actor & Companion", "px":[n % 16, n / 16], "width":16, "height":16,
            "fieldInstances":[
                {"__identifier":"Item", "__type":"Array<String>", "__value":raw_names},
                {"__identifier":"Destination", "__type":"EntityRef", "__value":{"entityIid":"other"}}
            ]
        }))
        .collect::<Vec<_>>()
        .into();
    let mut times = [Duration::ZERO; 5];
    let mut checksum = 0;
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let files = project
            .export_tiled(dir.path().join(format!("out-{run}")))
            .unwrap();
        *time = start.elapsed();
        let map = files
            .iter()
            .find(|path| path.extension().is_some_and(|e| e == "tmx"))
            .unwrap();
        let xml = fs::read_to_string(map).unwrap();
        assert_eq!(xml.matches("name=\"Actor &amp; Companion\"").count(), count);
        assert_eq!(
            xml.matches("name=\"Item_47\" value=\"物品47 &lt; &amp; &gt; &quot;\n\"")
                .count(),
            count
        );
        assert_eq!(
            xml.matches("name=\"Destination\" value=\"other\"").count(),
            count
        );
        let mut hasher = DefaultHasher::new();
        xml.hash(&mut hasher);
        if run == 0 {
            checksum = hasher.finish();
        } else {
            assert_eq!(checksum, hasher.finish());
        }
    }
    times.sort_unstable();
    eprintln!(
        "Tiled entity properties: median {:?}; output {checksum:016x}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual entity-heavy TMJ performance and memory comparison"]
fn benchmark_tmj_entity_properties_export() {
    use ldtk_core::exports::TiledFormat;
    use std::hash::{DefaultHasher, Hasher};
    use std::io::{BufReader, Read};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.add_layer("Actors", "Entities", 16).unwrap();
    let names: Vec<_> = (0..48).map(|n| format!("物品{n} < & > \"\\n")).collect();
    let count = 2048;
    project.layer_mut(key, 0).unwrap()["entityInstances"] = (0..count)
        .map(|n| json!({
            "__identifier":"Actor & Companion", "px":[n % 16, n / 16], "width":16, "height":16,
            "fieldInstances":[
                {"__identifier":"Item", "__type":"Array<String>", "__value":names},
                {"__identifier":"Destination", "__type":"EntityRef", "__value":{"entityIid":"other"}}
            ]
        }))
        .collect::<Vec<_>>()
        .into();

    let mut times = [Duration::ZERO; 5];
    let mut checksum = 0;
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let report = project
            .export_tiled_format_with_report_progress(
                dir.path().join(format!("out-{run}")),
                TiledFormat::Tmj,
                |_, _| {},
            )
            .unwrap();
        *time = start.elapsed();
        let map = report
            .outputs
            .iter()
            .find(|path| path.extension().is_some_and(|e| e == "tmj"))
            .unwrap();
        let mut reader = BufReader::new(fs::File::open(map).unwrap());
        let mut hasher = DefaultHasher::new();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = reader.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            hasher.write(&buffer[..read]);
        }
        if run == 0 {
            checksum = hasher.finish();
        } else {
            assert_eq!(checksum, hasher.finish());
        }
    }
    times.sort_unstable();
    eprintln!(
        "TMJ entity properties: median {:?}; output {checksum:016x}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual dense IntGrid CSV export performance comparison"]
fn benchmark_dense_csv_export() {
    use std::fmt::Write;
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    let (width, height) = (1024, 512);
    project
        .resize_level(key, width * 16, height * 16, 0, 0)
        .unwrap();
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let value = |x, y| {
        if (x + y) % 31 == 0 {
            -1
        } else {
            (x * 3 + y) % 512
        }
    };
    let mut expected = String::new();
    let mut cells = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let value = value(x, y);
            cells.push(json!(value));
            if x != 0 {
                expected.push(',');
            }
            write!(expected, "{}", value as u8).unwrap();
        }
        if y + 1 < height {
            expected.push(',');
        }
        expected.push('\n');
    }
    project.layer_mut(key, 0).unwrap()["intGridCsv"] = Value::Array(cells);
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let outputs = project
            .export_csv(dir.path().join(format!("csv-{run}")))
            .unwrap();
        *time = start.elapsed();
        assert_eq!(outputs.len(), 1);
        assert_eq!(fs::read(&outputs[0]).unwrap(), expected.as_bytes());
    }
    times.sort_unstable();
    eprintln!("Dense CSV export: median {:?}; samples {times:?}", times[2]);
}

#[test]
#[ignore = "manual entity-heavy simplified export performance comparison"]
fn benchmark_entity_heavy_simplified_export() {
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 32, 32, 0, 0).unwrap();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let count = 3072;
    let payload: Vec<_> = (0..64).map(|i| format!("value-{i}")).collect();
    let layer = project.layer_mut(key, 0).unwrap();
    layer["entityInstances"] = (0..count)
        .map(|i| {
            json!({
                "__identifier": if i % 2 == 0 { "Actor" } else { "Prop" },
                "iid": format!("entity-{i}"), "__smartColor":"#112233",
                "px":[0,0], "width":16, "height":16,
                "fieldInstances":[{"__identifier":"Inventory","__value":payload}]
            })
        })
        .collect::<Vec<_>>()
        .into();
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let paths = project
            .export_simplified(dir.path().join(format!("simple-{run}")))
            .unwrap();
        *time = start.elapsed();
        let file = paths
            .iter()
            .find(|path| path.ends_with("data.json"))
            .unwrap();
        let data: Value = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        for (offset, name) in ["Actor", "Prop"].iter().enumerate() {
            let entities = data["entities"][name].as_array().unwrap();
            assert_eq!(entities.len(), count / 2);
            for (i, entity) in entities.iter().enumerate() {
                assert_eq!(entity["iid"], format!("entity-{}", i * 2 + offset));
                assert_eq!(entity["layer"], "Actors");
                assert_eq!(entity["color"], 0x112233);
                assert!(
                    entity["customFields"]["Inventory"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .eq(&payload)
                );
            }
        }
    }
    times.sort_unstable();
    eprintln!(
        "Entity-heavy simplified export: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
fn tiled_writes_mixed_dense_and_sparse_stacks_in_cell_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    p.path = Some(dir.path().join("mixed.ldtk"));
    let key = p.level_keys()[0];
    p.resize_level(key, 320, 320, 0, 0).unwrap();
    let layer = p.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    p.assign_tileset(layer, atlas).unwrap();
    let tile = |i, id| json!({"px":[i % 20 * 16, i / 20 * 16],"t":id,"f":0});
    let mut tiles = Vec::new();
    for i in (0..400).rev() {
        tiles.push(tile(i, 0));
        if i % 2 == 0 {
            tiles.push(tile(i, 1));
        }
        if i == 398 || i == 0 {
            tiles.push(tile(i, 0));
        }
    }
    p.layer_mut(key, 0).unwrap()["gridTiles"] = tiles.into();
    let paths = p.export_tiled(dir.path().join("map")).unwrap();
    let map = paths
        .iter()
        .find(|path| path.extension().is_some_and(|e| e == "tmx"))
        .unwrap();
    let xml = fs::read_to_string(map).unwrap();
    let layers: Vec<_> = xml
        .split("<data encoding=\"csv\">")
        .skip(1)
        .map(|tail| {
            tail.split_once("</data>")
                .unwrap()
                .0
                .split(',')
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(layers.len(), 3);
    assert_eq!(layers[0], vec!["1"; 400]);
    assert!(
        layers[1]
            .iter()
            .copied()
            .eq((0..400).map(|i| if i % 2 == 0 { "2" } else { "0" }))
    );
    assert!(
        layers[2]
            .iter()
            .copied()
            .eq((0..400).map(|i| if i == 0 || i == 398 { "1" } else { "0" }))
    );

    let output = dir.path().join("json");
    p.export_tiled_format_with_report_progress(
        &output,
        ldtk_core::exports::TiledFormat::Tmj,
        |_, _| {},
    )
    .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(output.join("Level_0.tmj")).unwrap()).unwrap();
    let json_layers = map["layers"].as_array().unwrap();
    assert_eq!(json_layers.len(), layers.len());
    for (json_layer, xml_cells) in json_layers.iter().zip(layers) {
        let gids = json_layer["data"].as_array().unwrap();
        assert!(
            gids.iter()
                .map(|gid| gid.as_u64().unwrap())
                .eq(xml_cells.into_iter().map(|gid| gid.parse::<u64>().unwrap()))
        );
    }
}

#[test]
#[ignore = "manual re-export comparison for projects with many small rooms"]
fn benchmark_many_small_gamemaker_rooms_reexport() {
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    for i in 1..192 {
        project
            .add_level(None, &format!("Room_{i}"), 64, 64)
            .unwrap();
    }
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let expected = project.export_gamemaker(dir.path()).unwrap();
    assert_eq!(expected.len(), 192);
    let known_mtime = std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    for path in &expected {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(known_mtime)
            .unwrap();
    }
    let mut times = [Duration::ZERO; 7];
    for time in &mut times {
        let start = Instant::now();
        let files = project.export_gamemaker(dir.path()).unwrap();
        *time = start.elapsed();
        assert_eq!(files, expected);
        assert!(
            files
                .iter()
                .all(|path| fs::metadata(path).unwrap().modified().unwrap() == known_mtime)
        );
    }
    times.sort_unstable();
    eprintln!(
        "Small GameMaker rooms re-export: median {:?}; samples {times:?}",
        times[3]
    );
}

#[test]
#[ignore = "manual GameMaker export time and memory comparison"]
fn benchmark_large_gamemaker_room_export() {
    exercise_gamemaker_room_export(false);
}

#[test]
#[ignore = "manual GameMaker re-export into an existing directory"]
fn benchmark_large_gamemaker_room_reexport() {
    exercise_gamemaker_room_export(true);
}

fn exercise_gamemaker_room_export(overwrite: bool) {
    use serde::Deserialize;
    use std::time::{Duration, Instant};

    #[derive(Deserialize)]
    struct Room {
        layers: Vec<Layer>,
    }

    #[derive(Deserialize)]
    struct Layer {
        tiles: Tiles,
    }

    #[derive(Deserialize)]
    struct Tiles {
        #[serde(rename = "SerialiseWidth")]
        width: i32,
        #[serde(rename = "SerialiseHeight")]
        height: i32,
        #[serde(rename = "TileSerialiseData")]
        cells: Vec<u32>,
    }

    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new();
    let key = p.level_keys()[0];
    let (width, height) = (512, 256);
    p.resize_level(key, width * 16, height * 16, 0, 0).unwrap();
    let atlas = p
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    for name in ["Ground", "Walls", "Props"] {
        let layer = p.add_layer(name, "Tiles", 16).unwrap();
        p.assign_tileset(layer, atlas).unwrap();
        p.layer_mut(key, 0).unwrap()["gridTiles"] = json!([
            {"px":[0,0],"t":1},
            {"px":[0,0],"t":0},
            {"px":[(width-1)*16,(height-1)*16],"t":0}
        ]);
    }
    let mut times = [Duration::ZERO; 5];
    let mut bytes = 0;
    let repeat_dir = dir.path().join("repeat");
    if overwrite {
        p.export_gamemaker(&repeat_dir).unwrap();
    }
    for (run, time) in times.iter_mut().enumerate() {
        let target = if overwrite {
            repeat_dir.clone()
        } else {
            dir.path().join(format!("gms-{run}"))
        };
        let start = Instant::now();
        let files = p.export_gamemaker(target).unwrap();
        *time = start.elapsed();
        let room = files
            .iter()
            .find(|path| path.file_name().is_some_and(|name| name == "Level_0.yy"))
            .unwrap();
        let contents = fs::read(room).unwrap();
        bytes = contents.len();
        let room: Room = serde_json::from_slice(&contents).unwrap();
        assert_eq!(room.layers.len(), 3);
        for Layer { tiles } in room.layers {
            assert_eq!((tiles.width, tiles.height), (width, height));
            assert_eq!(tiles.cells.len(), (width * height) as usize);
            assert_eq!(tiles.cells[0], 1);
            assert_eq!(tiles.cells.last(), Some(&0));
            assert!(
                tiles.cells[1..tiles.cells.len() - 1]
                    .iter()
                    .all(|&tile| tile == 1 << 31)
            );
        }
    }
    times.sort_unstable();
    eprintln!(
        "Large GameMaker room export: median {:?}; samples {times:?}",
        times[2]
    );
    eprintln!("GameMaker room: {bytes} bytes");
}

#[test]
#[ignore = "manual sparse TMX export performance and memory comparison"]
fn benchmark_sparse_tiled_stacks_export() {
    exercise_sparse_tiled_stacks(false);
}

#[test]
#[ignore = "manual same-directory TMX export performance and memory comparison"]
fn benchmark_sparse_tiled_stacks_reexport() {
    exercise_sparse_tiled_stacks(true);
}

fn exercise_sparse_tiled_stacks(overwrite: bool) {
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("sparse.ldtk"));
    let (width, height, depth) = (512, 256, 48);
    let cells = width * height;
    let positions = [cells - 1, 0, width * 74 + 8];
    project
        .resize_level(key, (width * 16) as i32, (height * 16) as i32, 0, 0)
        .unwrap();
    let layer = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(layer, atlas).unwrap();
    project.layer_mut(key, 0).unwrap()["gridTiles"] = (0..depth)
        .flat_map(|z| {
            positions.into_iter().map(move |i| {
                json!({
                    "px":[i % width * 16, i / width * 16],"t":z % 2,"f":z % 4
                })
            })
        })
        .collect::<Vec<_>>()
        .into();

    let mut times = [Duration::ZERO; 5];
    let repeat_dir = dir.path().join("repeat");
    if overwrite {
        project.export_tiled(&repeat_dir).unwrap();
    }
    for (run, time) in times.iter_mut().enumerate() {
        let target = if overwrite {
            repeat_dir.clone()
        } else {
            dir.path().join(format!("tiled-{run}"))
        };
        let start = Instant::now();
        let files = project.export_tiled(target).unwrap();
        *time = start.elapsed();
        let map = files
            .iter()
            .find(|path| path.extension().is_some_and(|e| e == "tmx"))
            .unwrap();
        let mut xml = BufReader::new(fs::File::open(map).unwrap());
        let mut chunk = Vec::new();
        let mut stack = 0;
        while xml.read_until(b'>', &mut chunk).unwrap() != 0 {
            if chunk == b"<data encoding=\"csv\">" {
                chunk.clear();
                assert_ne!(xml.read_until(b'>', &mut chunk).unwrap(), 0);
                let data = std::str::from_utf8(chunk.strip_suffix(b"</data>").unwrap()).unwrap();
                let gid = ["1", "2147483650", "1073741825", "3221225474"][stack % 4];
                let mut count = 0;
                for (i, value) in data.split(',').enumerate() {
                    assert_eq!(value, if positions.contains(&i) { gid } else { "0" });
                    count += 1;
                }
                assert_eq!(count, cells);
                stack += 1;
            }
            chunk.clear();
        }
        assert_eq!(stack, depth);
    }
    times.sort_unstable();
    eprintln!(
        "Sparse Tiled stacks export: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
#[ignore = "manual TMJ export performance comparison"]
fn benchmark_sparse_tmj_export() {
    use ldtk_core::exports::TiledFormat;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    let (width, height, depth) = (512, 256, 48);
    let area = (width * height) as usize;
    project
        .resize_level(key, width * 16, height * 16, 0, 0)
        .unwrap();
    let layer = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(layer, atlas).unwrap();
    let positions = [0, width * 74 + 8, width * height - 1];
    project.layer_mut(key, 0).unwrap()["gridTiles"] = (0..depth)
        .flat_map(|z| {
            positions.into_iter().map(move |i| {
                json!({
                    "px":[i % width * 16, i / width * 16],"t":z % 2,"f":z % 4
                })
            })
        })
        .collect::<Vec<_>>()
        .into();

    let output = dir.path().join("tiled");
    let mut times = [Duration::ZERO; 6];
    for time in &mut times {
        let start = Instant::now();
        project
            .export_tiled_format_with_report_progress(&output, TiledFormat::Tmj, |_, _| {})
            .unwrap();
        *time = start.elapsed();
    }
    times[1..].sort_unstable();
    eprintln!(
        "Sparse TMJ: initial {:?}, re-export median {:?}",
        times[0], times[3]
    );

    // Validate every emitted GID while keeping the benchmark's memory usage bounded.
    let mut file = BufReader::new(fs::File::open(output.join("Level_0.tmj")).unwrap());
    let mut chunk = Vec::new();
    let mut stack = 0;
    while file.read_until(b'[', &mut chunk).unwrap() != 0 {
        if chunk.ends_with(b"\"data\":[") {
            chunk.clear();
            assert_ne!(file.read_until(b']', &mut chunk).unwrap(), 0);
            let data: Vec<u32> =
                serde_json::from_slice(&[b"[", chunk.as_slice()].concat()).unwrap();
            let gid = [1, 0x80000002, 0x40000001, 0xc0000002][stack % 4];
            assert_eq!(data.len(), area);
            for (i, tile) in data.into_iter().enumerate() {
                assert_eq!(
                    tile,
                    if positions.contains(&(i as i32)) {
                        gid
                    } else {
                        0
                    }
                );
            }
            stack += 1;
        }
        chunk.clear();
    }
    assert_eq!(stack, depth);
}

#[test]
#[ignore = "manual Tiled IntGrid CSV rendering comparison"]
fn benchmark_tiled_intgrid_csv() {
    use std::time::{Duration, Instant};

    let directory = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    let (width, height) = (1024, 512);
    project
        .resize_level(key, width * 16, height * 16, 0, 0)
        .unwrap();
    project.add_layer("Terrain", "IntGrid", 16).unwrap();
    let cells = (width * height) as usize;
    for dense in [false, true] {
        let value = |index| {
            if dense {
                u32::from(index % 3 != 0)
            } else {
                u32::from(index == 0 || index == 8192 || index == cells - 1)
            }
        };
        project.layer_mut(key, 0).unwrap()["intGridCsv"] = (0..cells)
            .map(|index| json!(value(index)))
            .collect::<Vec<_>>()
            .into();
        let target = directory
            .path()
            .join(if dense { "dense" } else { "sparse" });
        project.export_tiled(&target).unwrap();
        let mut times = [Duration::ZERO; 5];
        for time in &mut times {
            let start = Instant::now();
            project.export_tiled(&target).unwrap();
            *time = start.elapsed();
            let xml = fs::read_to_string(target.join("Level_0.tmx")).unwrap();
            let (_, csv) = xml.split_once("<data encoding=\"csv\">").unwrap();
            let (csv, _) = csv.split_once("</data>").unwrap();
            assert!(
                csv.split(',')
                    .eq((0..cells).map(|index| if value(index) == 0 { "0" } else { "1" }))
            );
        }
        times.sort_unstable();
        let layout = if dense { "dense" } else { "sparse" };
        eprintln!(
            "Tiled {layout} IntGrid: median {:?}; samples {times:?}",
            times[2]
        );
    }
}

#[test]
#[ignore = "manual stacked Tiled export performance comparison"]
fn benchmark_deep_tiled_stacks_export() {
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.path = Some(dir.path().join("stacked.ldtk"));
    let layer = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(layer, atlas).unwrap();
    let depth = 192;
    project.layer_mut(key, 0).unwrap()["gridTiles"] = (0..depth)
        .flat_map(|z| {
            (0..16 * 16).map(move |i| {
                json!({
                    "px":[i % 16 * 16, i / 16 * 16],"t":z % 2,"f":z % 4
                })
            })
        })
        .collect::<Vec<_>>()
        .into();
    let mut times = [Duration::ZERO; 5];
    for (run, time) in times.iter_mut().enumerate() {
        let start = Instant::now();
        let files = project
            .export_tiled(dir.path().join(format!("tiled-{run}")))
            .unwrap();
        *time = start.elapsed();
        let map = files
            .iter()
            .find(|path| path.extension().is_some_and(|e| e == "tmx"))
            .unwrap();
        let xml = fs::read_to_string(map).unwrap();
        let layers = xml
            .split("<data encoding=\"csv\">")
            .skip(1)
            .collect::<Vec<_>>();
        assert_eq!(layers.len(), depth);
        for (z, layer) in layers.into_iter().enumerate() {
            let data = layer.split_once("</data>").unwrap().0;
            let gid = [1u32, 0x80000002, 0x40000001, 0xc0000002][z % 4];
            assert!(
                data.split(',')
                    .map(|value| value.parse::<u32>().unwrap())
                    .eq(std::iter::repeat_n(gid, 256))
            );
        }
    }
    times.sort_unstable();
    eprintln!(
        "Deep Tiled stacks export: median {:?}; samples {times:?}",
        times[2]
    );
}

#[test]
fn sparse_tmx_preserves_csv_boundaries_and_does_not_replace_identical_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    let (width, height) = (128, 512);
    let area = width * height;
    project
        .resize_level(key, width * 16, height * 16, 0, 0)
        .unwrap();
    let ground = project.add_layer("Ground", "Tiles", 16).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(ground, atlas).unwrap();
    let tile = |index, id| json!({"px":[index % width * 16,index / width * 16],"t":id});
    project.layer_mut(key, 0).unwrap()["gridTiles"] = json!([
        tile(4096, 0),
        tile(area - 2, 0),
        tile(area - 1, 0),
        tile(4096, 1),
        tile(area - 2, 1),
        tile(area - 1, 1),
        tile(4096, 0),
        tile(area - 2, 0)
    ]);
    let output = dir.path().join("tiled");
    let files = project.export_tiled(&output).unwrap();
    let path = output.join("Level_0.tmx");
    let xml = fs::read_to_string(&path).unwrap();
    assert!(xml.contains("nextlayerid=\"4\""));
    let layers: Vec<_> = xml
        .split("<data encoding=\"csv\">")
        .skip(1)
        .map(|data| data.split_once("</data>").unwrap().0)
        .collect();
    assert_eq!(layers.len(), 3);
    for (layer, data) in layers.iter().enumerate() {
        assert!(
            data.split(',')
                .eq((0..area).map(|index| match (layer, index) {
                    (0 | 2, 4096) => "1",
                    (1, 4096) => "2",
                    (0 | 2, i) if i == area - 2 => "1",
                    (0, i) if i == area - 1 => "1",
                    (1, i) if i >= area - 2 => "2",
                    _ => "0",
                }))
        );
    }
    let output_json = dir.path().join("tiled-json");
    project
        .export_tiled_format_with_report_progress(
            &output_json,
            ldtk_core::exports::TiledFormat::Tmj,
            |_, _| {},
        )
        .unwrap();
    let tmj: Value =
        serde_json::from_slice(&fs::read(output_json.join("Level_0.tmj")).unwrap()).unwrap();
    let json_layers = tmj["layers"].as_array().unwrap();
    assert_eq!(json_layers.len(), layers.len());
    for (json_layer, csv) in json_layers.iter().zip(layers) {
        assert!(
            json_layer["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|gid| gid.as_u64().unwrap())
                .eq(csv.split(',').map(|gid| gid.parse::<u64>().unwrap()))
        );
    }
    let earlier = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(earlier)
        .unwrap();
    assert_eq!(project.export_tiled(&output).unwrap(), files);
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), earlier);
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
    assert!(xml.contains("<data encoding=\"csv\">1,2</data>"));
    assert!(t.path().join("Solid.intgrid.png").exists());
    let w: Value =
        serde_json::from_slice(&fs::read(t.path().join("Untitled.world")).unwrap()).unwrap();
    assert_eq!(w["maps"][0]["fileName"], "Level_0.tmx");
}

#[test]
fn tiled_intgrid_gids_follow_each_palette_in_tmx_and_tmj() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.json["defs"]["layers"][0]["intGridValues"] = json!([
        {"value":257,"color":"#00ff00"}, {"value":1,"color":"#ff0000"}
    ]);
    p.json["levels"][0]["layerInstances"][0]["intGridCsv"] = json!([1, 257, 1]);
    p.json["defs"]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "uid":8,"identifier":"Zones","type":"IntGrid","gridSize":16,
            "intGridValues":[{"value":257,"color":"#0000ff"}]
        }));
    p.json["levels"][0]["layerInstances"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "layerDefUid":8,"__identifier":"Zones","__type":"IntGrid","__gridSize":16,
            "__cWid":2,"__cHei":1,"intGridCsv":[257]
        }));

    p.export_tiled(dir.path()).unwrap();
    let xml = fs::read_to_string(dir.path().join("Level_0.tmx")).unwrap();
    assert!(xml.contains("<data encoding=\"csv\">2,1</data>"), "{xml}");
    assert!(xml.contains("<data encoding=\"csv\">3,0</data>"), "{xml}");
    p.export_tiled_format_with_report_progress(dir.path(), TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(dir.path().join("Level_0.tmj")).unwrap()).unwrap();
    assert_eq!(map["tilesets"][0]["firstgid"], 1);
    assert_eq!(map["tilesets"][1]["firstgid"], 3);
    assert_eq!(map["layers"][0]["data"], json!([3, 0]));
    assert_eq!(map["layers"][1]["data"], json!([2, 1]));

    let solid = image::open(dir.path().join("Solid.intgrid.png"))
        .unwrap()
        .into_rgba8();
    let zones = image::open(dir.path().join("Zones.intgrid.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!(solid.get_pixel(0, 0).0, [0, 255, 0, 255]);
    assert_eq!(solid.get_pixel(16, 0).0, [255, 0, 0, 255]);
    assert_eq!(zones.get_pixel(0, 0).0, [0, 0, 255, 255]);
}

#[test]
fn tiled_exports_an_empty_intgrid_palette_without_a_tileset_image() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.json["defs"]["layers"][0]["intGridValues"] = json!([]);
    p.json["levels"][0]["layerInstances"][0]["intGridCsv"] = json!([0, 0]);
    p.export_tiled(dir.path()).unwrap();
    let xml = fs::read_to_string(dir.path().join("Level_0.tmx")).unwrap();
    assert!(!xml.contains("<tileset"));
    assert!(xml.contains("<data encoding=\"csv\">0,0</data>"));
    p.export_tiled_format_with_report_progress(dir.path(), TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(dir.path().join("Level_0.tmj")).unwrap()).unwrap();
    assert_eq!(map["tilesets"], json!([]));
    assert_eq!(map["layers"][0]["data"], json!([0, 0]));
    assert!(!dir.path().join("Solid.intgrid.png").exists());
}

#[test]
fn tiled_rejects_intgrid_tileset_dimensions_that_would_truncate() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.json["defs"]["layers"][0]["gridSize"] = json!((1_i64 << 32) + 1);
    let error = p.export_tiled(dir.path()).unwrap_err().to_string();
    assert!(error.contains("IntGrid tileset Solid"), "{error}");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn tmj_exports_typed_properties_unsigned_gids_and_world() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut p = project();
    p.path = Some(dir.path().join("map.ldtk"));
    p.json["worldLayout"] = json!("Free");
    p.json["defs"]["tilesets"] = json!([{
        "uid":7,"identifier":"Atlas","relPath":"atlas.png","pxWid":32,"pxHei":16,
        "tileGridSize":16,"customData":[{"tileId":1,"data":"{\"solid\":true}"}]
    }]);
    p.json["defs"]["entities"] = json!([{"uid":8,"width":16,"height":16,"pivotX":1}]);
    p.json["defs"]["layers"].as_array_mut().unwrap().extend([
        json!({"uid":3,"identifier":"Actors","type":"Entities","gridSize":16}),
        json!({"uid":4,"identifier":"Empty","type":"Entities","gridSize":16}),
        json!({"uid":5,"identifier":"Upper","type":"Entities","gridSize":16}),
    ]);
    let level = &mut p.json["levels"][0];
    level["worldX"] = json!(-32);
    level["layerInstances"][0]["__tilesetDefUid"] = json!(7);
    level["layerInstances"][0]["intGridCsv"] = json!([1, 257]);
    level["layerInstances"][0]["autoLayerTiles"] = json!([
        {"px":[0,0],"t":0,"f":3},{"px":[0,0],"t":1,"f":0}
    ]);
    let instances = level["layerInstances"].as_array_mut().unwrap();
    instances.insert(
        0,
        json!({
            "layerDefUid":3,"__type":"Entities","__identifier":"Actors","__gridSize":16,
            "entityInstances":[{
                "__identifier":"Player","defUid":8,"px":[16,0],
                "fieldInstances":[
                    {"__identifier":"hp","__type":"Int","__value":20},
                    {"__identifier":"rate","__type":"Float","__value":1.25},
                    {"__identifier":"alive","__type":"Bool","__value":true},
                    {"__identifier":"tint","__type":"Color","__value":"#a1b2c3"},
                    {"__identifier":"lines","__type":"Array<String>","__value":["甲\\n乙","引号\""]}
                ]
            }, {"__identifier":"Player","defUid":8,"px":[0,0]}]
        }),
    );
    instances.insert(
        0,
        json!({
            "layerDefUid":4,"__type":"Entities","__identifier":"Empty","__gridSize":16,
            "entityInstances":[]
        }),
    );
    instances.insert(
        0,
        json!({
            "layerDefUid":5,"__type":"Entities","__identifier":"Upper","__gridSize":16,
            "entityInstances":[{"__identifier":"Marker","defUid":8,"px":[0,0]}]
        }),
    );

    let output = dir.path().join("tiled");
    p.export_tiled_format_with_report_progress(&output, TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let read_json =
        |name| -> Value { serde_json::from_slice(&fs::read(output.join(name)).unwrap()).unwrap() };
    let map = read_json("Level_0.tmj");
    assert_eq!(map["type"], "map");
    assert_eq!(map["orientation"], "orthogonal");
    assert_eq!(map["width"], 2);
    assert_eq!(map["tilewidth"], 16);
    assert_eq!(map["tilesets"][0]["image"], "../atlas.png");
    assert_eq!(
        map["tilesets"][0]["tiles"][0]["properties"][0],
        json!({"name":"solid","type":"bool","value":true})
    );
    assert_eq!(map["tilesets"][1]["firstgid"], 3);
    assert_eq!(map["tilesets"][1]["image"], "Solid.intgrid.png");
    assert!(output.join("Solid.intgrid.png").is_file());
    let layers = map["layers"].as_array().unwrap();
    assert_eq!(layers[0]["name"], "Solid_values");
    assert_eq!(layers[0]["type"], "tilelayer");
    assert_eq!(layers[0]["data"], json!([3, 4]));
    assert_eq!(layers[1]["data"], json!([3221225473u32, 0]));
    assert_eq!(layers[2]["data"], json!([2, 0]));
    assert_eq!(layers[3]["type"], "objectgroup");
    let objects = &layers[3]["objects"];
    assert_eq!(objects[0]["x"], 0);
    assert_eq!(objects[0]["type"], "Player");
    assert_eq!(
        objects[0]["properties"].as_array().unwrap()[4..],
        [
            json!({"name":"hp","type":"int","value":20}),
            json!({"name":"rate","type":"float","value":1.25}),
            json!({"name":"alive","type":"bool","value":true}),
            json!({"name":"tint","type":"color","value":"#ffa1b2c3"}),
            json!({"name":"lines_0","type":"string","value":"甲\n乙"}),
            json!({"name":"lines_1","type":"string","value":"引号\""}),
        ]
    );
    assert_eq!(objects[1]["id"], 2);
    assert_eq!(layers[4]["name"], "Empty");
    assert_eq!(layers[4]["objects"], json!([]));
    assert_eq!(layers[5]["name"], "Upper");
    assert_eq!(layers[5]["objects"][0]["id"], 3);
    assert_eq!(map["nextobjectid"], 4);
    assert_eq!(map["nextlayerid"], 7);
    let world = read_json("map.world");
    assert_eq!(world["maps"][0]["fileName"], "Level_0.tmj");
    assert_eq!(world["maps"][0]["x"], -32);
    assert_eq!(world["maps"][0]["width"], 32);
    assert_eq!(world["maps"][0]["height"], 16);
    assert!(!output.join("Level_0.tmx").exists());

    p.export_tiled(&output).unwrap();
    let xml = fs::read_to_string(output.join("Level_0.tmx")).unwrap();
    let object_ids: Vec<_> = xml
        .split("<object id=\"")
        .skip(1)
        .map(|object| object.split_once('"').unwrap().0)
        .collect();
    assert_eq!(object_ids, ["1", "2", "3"]);
    assert!(xml.contains("nextobjectid=\"4\""));
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
fn intgrid_png_rejects_invalid_dimensions_without_replacing_existing_exports() {
    let mut p = project();
    let dir = tempfile::tempdir().unwrap();
    let files = p.export_png(dir.path()).unwrap();
    let previous: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();

    for (columns, rows) in [((1_i64 << 32) + 1, 1), (2, -1), (16_384, 16_385)] {
        let layer = &mut p.json["levels"][0]["layerInstances"][0];
        layer["__cWid"] = json!(columns);
        layer["__cHei"] = json!(rows);
        layer["intGridCsv"] = json!([257]);
        let error = p.export_png(dir.path()).unwrap_err().to_string();
        assert!(
            error.contains("IntGrid") && error.contains("Solid"),
            "{error}"
        );
        for (file, original) in files.iter().zip(&previous) {
            assert_eq!(&fs::read(file).unwrap(), original, "{}", file.display());
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), files.len());
    }
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
fn gamemaker_preserves_manual_tile_flips_and_layer_offsets() {
    let mut p = project();
    p.json["defs"]["tilesets"] = json!([{"uid":42,"identifier":"Atlas"}]);
    let level = &mut p.json["levels"][0];
    level["pxWid"] = json!(80);
    let layer = &mut level["layerInstances"][0];
    layer["__type"] = json!("Tiles");
    layer["__tilesetDefUid"] = json!(42);
    layer["__cWid"] = json!(5);
    layer["__pxTotalOffsetX"] = json!(7);
    layer["__pxTotalOffsetY"] = json!(-9);
    layer["gridTiles"] = json!([
        {"px":[-1,0],"t":99,"f":0},
        {"px":[0,0],"t":17,"f":0},
        {"px":[16,0],"t":17,"f":1},
        {"px":[16,0],"t":99,"f":0},
        {"px":[32,0],"t":17,"f":2},
        {"px":[48,0],"t":17,"f":3}
    ]);
    let directory = tempfile::tempdir().unwrap();
    p.export_gamemaker(directory.path()).unwrap();
    let room: Value =
        serde_json::from_slice(&fs::read(directory.path().join("Level_0.yy")).unwrap()).unwrap();
    let layer = &room["layers"][0];
    assert_eq!(
        (layer["x"].as_i64(), layer["y"].as_i64()),
        (Some(7), Some(-9))
    );
    assert_eq!(
        layer["tiles"],
        json!({"SerialiseWidth":5,"SerialiseHeight":1,"TileSerialiseData":[
            17, 17 | (1_u32 << 28), 17 | (1_u32 << 29), 17 | (3_u32 << 28), 1_u32 << 31
        ]})
    );
}

#[test]
fn sparse_gamemaker_layers_keep_empty_cells_and_the_first_tile_in_each_stack() {
    let mut p = project();
    p.json["defs"]["tilesets"] = json!([{"uid":42,"identifier":"Atlas"}]);
    let layer = p.layer_mut(p.level_keys()[0], 0).unwrap();
    layer["__type"] = json!("Tiles");
    layer["__tilesetDefUid"] = json!(42);
    let (width, height) = (128, 32);
    layer["__cWid"] = json!(width);
    layer["__cHei"] = json!(height);
    layer["gridTiles"] = json!([
        {"px":[(width-1)*16,(height-1)*16],"t":1,"f":3},
        {"px":[-1,0],"t":-1},
        {"px":[16,0],"t":0},
        {"px":[16,0],"t":-1},
        {"px":[width*16,0],"t":-1}
    ]);
    let directory = tempfile::tempdir().unwrap();
    p.export_gamemaker(directory.path()).unwrap();
    let room: Value =
        serde_json::from_slice(&fs::read(directory.path().join("Level_0.yy")).unwrap()).unwrap();
    let area = width * height;
    let expected = (0..area).map(|i| match i {
        1 => 0,
        i if i == area - 1 => 1 | (3_u64 << 28),
        _ => 1_u64 << 31,
    });
    let tiles = &room["layers"][0]["tiles"];
    assert_eq!(tiles["SerialiseWidth"], width);
    assert_eq!(tiles["SerialiseHeight"], height);
    assert!(
        tiles["TileSerialiseData"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tile| tile.as_u64().unwrap())
            .eq(expected)
    );
}

#[test]
fn gamemaker_rejects_invalid_tile_dimensions_and_ids_without_replacing_outputs() {
    let mut p = project();
    p.json["defs"]["tilesets"] = json!([{"uid":42,"identifier":"Atlas"}]);
    let layer = &mut p.json["levels"][0]["layerInstances"][0];
    layer["__type"] = json!("Tiles");
    layer["__tilesetDefUid"] = json!(42);
    layer["gridTiles"] = json!([{"px":[0,0],"t":17,"f":0}]);
    let valid = layer.clone();
    let directory = tempfile::tempdir().unwrap();
    let files = p.export_gamemaker(directory.path()).unwrap();
    let before: Vec<_> = files.iter().map(|file| fs::read(file).unwrap()).collect();
    for (field, invalid) in [
        ("__cWid", json!(i64::MAX)),
        ("__cWid", json!(u64::MAX)),
        ("__cHei", json!(-1)),
        ("__gridSize", json!(0)),
        ("gridTiles", json!([{"px":[0,0],"t":1_u32 << 19}])),
    ] {
        let layer = &mut p.json["levels"][0]["layerInstances"][0];
        *layer = valid.clone();
        layer[field] = invalid;
        let error = p
            .export_gamemaker(directory.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Solid"), "{field}: {error}");
        for (file, bytes) in files.iter().zip(&before) {
            assert_eq!(&fs::read(file).unwrap(), bytes, "{field}: {file:?}");
        }
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), files.len());
    }
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
fn invalid_tiled_dimensions_keep_previous_outputs_in_both_formats() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut previous = Project::new();
    let first = previous.level_keys()[0];
    previous.resize_level(first, 32, 32, 0, 0).unwrap();
    previous.add_layer("Solid", "IntGrid", 16).unwrap();
    let second = previous.add_level(None, "Later", 32, 32).unwrap();

    for (format, name) in [(TiledFormat::Tmx, "tmx"), (TiledFormat::Tmj, "tmj")] {
        let output = dir.path().join(name);
        let outputs = previous
            .export_tiled_format_with_report_progress(&output, format, |_, _| {})
            .unwrap()
            .outputs;
        let old: Vec<_> = outputs.iter().map(|path| fs::read(path).unwrap()).collect();
        for (field, invalid) in [
            ("__cWid", -1),
            ("__cWid", i64::MAX),
            ("__cHei", 8_388_609),
            ("pxWid", 0),
        ] {
            let mut project = previous.clone();
            assert!(project.paint_int_grid(first, 0, 0, 0, 1).unwrap());
            if field == "pxWid" {
                project.level_mut(second).unwrap()[field] = json!(invalid);
            } else {
                project.layer_mut(second, 0).unwrap()[field] = json!(invalid);
            }
            let mut progress = Vec::new();
            let error = project
                .export_tiled_format_with_report_progress(&output, format, |done, total| {
                    progress.push((done, total))
                })
                .unwrap_err();
            assert!(
                matches!(&error, ldtk_core::Error::Invalid(message) if message.contains("Later") && message.contains("dimensions")),
                "{error}"
            );
            if field != "pxWid" {
                assert!(error.to_string().contains("Solid"), "{error}");
            }
            assert_eq!(progress, [(0, 2), (1, 2)]);
            for (file, original) in outputs.iter().zip(&old) {
                assert_eq!(&fs::read(file).unwrap(), original, "{}", file.display());
            }
            assert_eq!(fs::read_dir(&output).unwrap().count(), outputs.len());
        }
    }
}

#[test]
fn png_export_preserves_files_created_after_outputs_were_staged() {
    let dir = tempfile::tempdir().unwrap();
    let previous = dir.path().join("Level_0.png");
    let new = dir.path().join("New.png");
    let collision = dir.path().join("Collision.png");
    fs::write(&previous, b"previous export").unwrap();
    let mut project = project();
    project.add_level(None, "New", 32, 16).unwrap();
    project.add_level(None, "Collision", 32, 16).unwrap();
    project.json["imageExportMode"] = json!("OneImagePerLevel");

    let result = project.export_png_with_progress(dir.path(), |done, total| {
        if total == 3 && done == total {
            assert!(!new.exists() && !collision.exists());
            fs::write(&collision, b"another writer's file").unwrap();
        }
    });

    assert!(fs::read(&collision).is_ok_and(|bytes| bytes == b"another writer's file"));
    assert_eq!(fs::read(previous).unwrap(), b"previous export");
    assert!(!new.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert!(matches!(result, Err(ldtk_core::Error::FilesChanged(paths)) if paths == [collision]));
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
    p.json["defs"]["layers"][0]["tilePivotY"] = json!(0.25);
    p.json["defs"]["tilesets"] = json!([{"uid":7,"identifier":"Atlas","relPath":"atlas.png","pxWid":32,"pxHei":32,"tileGridSize":32}]);
    let li = &mut p.json["levels"][0]["layerInstances"][0];
    li["__type"] = json!("Tiles");
    li["__tilesetDefUid"] = json!(7);
    li["gridTiles"] = json!([{"px":[0,0],"src":[0,0],"t":0,"f":0}]);
    p.export_png(t.path().join("out")).unwrap();
    let image = image::open(t.path().join("out/Level_0__Solid.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(image.get_pixel(23, 31)[3], 255);
    assert_eq!(image.get_pixel(24, 31)[3], 0);
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
fn png_clips_tiles_at_layer_instance_bounds() {
    use ldtk_core::exports::{PngExportMode, PngExportOptions};

    let directory = tempfile::tempdir().unwrap();
    let mut project = tiles_png_project();
    project.path = Some(directory.path().join("map.ldtk"));
    let level = project.level_keys()[0];
    project.layer_mut(level, 0).unwrap()["gridTiles"] = json!([]);
    let uid = project.json["defs"]["layers"][0]["uid"].as_i64().unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 16, 16)
        .unwrap();
    project.assign_tileset(uid, atlas).unwrap();
    let solid = image::Rgba([12, 34, 56, 255]);
    image::RgbaImage::from_pixel(16, 16, solid)
        .save(directory.path().join("atlas.png"))
        .unwrap();
    let definition = &mut project.json["defs"]["layers"][0];
    definition["pxOffsetX"] = json!(3);
    definition["pxOffsetY"] = json!(-2);
    let layer = project.layer_mut(level, 0).unwrap();
    layer["pxOffsetX"] = json!(5);
    layer["pxOffsetY"] = json!(6);
    layer["__pxTotalOffsetX"] = json!(8);
    layer["__pxTotalOffsetY"] = json!(4);
    layer["gridTiles"] = json!([
        {"px":[0,0], "src":[0,0], "t":0, "f":0, "d":[0]},
        {"px":[48,32], "src":[0,0], "t":0, "f":3, "d":[11]},
    ]);

    for mode in [
        PngExportMode::OneImagePerLevel,
        PngExportMode::LayersAndLevels,
    ] {
        let files = project
            .export_png_with_options(
                directory.path().join(format!("{mode:?}")),
                &PngExportOptions {
                    mode,
                    include_background: false,
                    file_pattern: None,
                },
            )
            .unwrap();
        assert_eq!(
            files.len(),
            if mode == PngExportMode::OneImagePerLevel {
                1
            } else {
                2
            }
        );
        for file in files {
            let image = image::open(&file).unwrap().into_rgba8();
            assert_eq!(image.dimensions(), (64, 48));
            for (x, y, pixel) in image.enumerate_pixels() {
                let expected = if ((8..24).contains(&x) && (4..20).contains(&y))
                    || ((56..59).contains(&x) && (36..42).contains(&y))
                {
                    solid
                } else {
                    image::Rgba([0; 4])
                };
                assert_eq!(*pixel, expected, "{file:?} at {x},{y}");
            }
        }
    }
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
        let tmj_dir = t.path().join("tmj");
        let tmj = project.export_tiled_format_with_report_progress(
            &tmj_dir,
            ldtk_core::exports::TiledFormat::Tmj,
            |_, _| {},
        );
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
            assert_eq!(tmj.unwrap_err().to_string(), error.to_string());
        } else {
            let report = tmj.unwrap_or_else(|error| panic!("{}: {error}", file.display()));
            let read = |path: &std::path::Path| -> Value {
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
            };
            let world = report.outputs.last().unwrap();
            assert_eq!(world.extension().unwrap(), "world");
            let world = read(world.as_path());
            for map in world["maps"].as_array().unwrap() {
                let path = tmj_dir.join(map["fileName"].as_str().unwrap());
                assert_eq!(path.extension().unwrap(), "tmj");
                let map = read(path.as_path());
                for layer in map["layers"].as_array().unwrap() {
                    if layer["type"] == "tilelayer" {
                        let cells =
                            layer["width"].as_u64().unwrap() * layer["height"].as_u64().unwrap();
                        assert_eq!(layer["data"].as_array().unwrap().len() as u64, cells);
                    }
                }
            }
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
fn png_full_size_background_keeps_alpha_blending_for_all_positions() {
    let dir = tempfile::tempdir().unwrap();
    image::RgbaImage::from_raw(
        3,
        1,
        vec![11, 22, 33, 255, 240, 64, 0, 128, 200, 201, 202, 0],
    )
    .unwrap()
    .save(dir.path().join("bg.png"))
    .unwrap();
    let mut project = Project::new();
    project.path = Some(dir.path().join("map.ldtk"));
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let level = &mut project.json["levels"][0];
    level["pxWid"] = json!(3);
    level["pxHei"] = json!(1);
    level["bgColor"] = json!("#102030");
    level["bgRelPath"] = json!("bg.png");
    level["bgPivotX"] = json!(0.25);
    level["bgPivotY"] = json!(0.75);

    for mode in ["Unscaled", "Contain", "Cover", "CoverDirty", "Repeat"] {
        project.json["levels"][0]["bgPos"] = json!(mode);
        let files = project.export_png(dir.path().join(mode)).unwrap();
        let image = image::open(&files[0]).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (3, 1));
        assert_eq!(
            image.as_raw(),
            &[11, 22, 33, 255, 128, 48, 24, 255, 16, 32, 48, 255],
            "{mode}"
        );
    }
}

#[test]
fn png_repeat_preserves_alpha_and_fractional_pivots() {
    let dir = tempfile::tempdir().unwrap();
    image::RgbaImage::from_raw(
        2,
        2,
        vec![
            120, 0, 80, 128, 24, 26, 28, 255, 80, 90, 100, 0, 6, 7, 8, 255,
        ],
    )
    .unwrap()
    .save(dir.path().join("bg.png"))
    .unwrap();
    let mut project = Project::new();
    project.path = Some(dir.path().join("map.ldtk"));
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let level = &mut project.json["levels"][0];
    level["pxWid"] = json!(6);
    level["pxHei"] = json!(4);
    level["bgColor"] = json!("#10203080");
    level["bgRelPath"] = json!("bg.png");
    level["bgPos"] = json!("Repeat");
    level["bgPivotX"] = json!(0.25);
    level["bgPivotY"] = json!(0.75);

    let files = project.export_png(dir.path().join("out")).unwrap();
    let image = image::open(&files[0]).unwrap().into_rgba8();
    assert_eq!(image.dimensions(), (6, 4));
    let expected = [
        [[24, 26, 28, 255], [85, 11, 69, 192]],
        [[6, 7, 8, 255], [16, 32, 48, 128]],
    ];
    for (x, y, pixel) in image.enumerate_pixels() {
        assert_eq!(pixel.0, expected[y as usize % 2][x as usize % 2]);
    }
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
fn tiled_layer_offsets_preserve_entity_and_tile_positions_in_both_formats() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 512, 256, 0, 0).unwrap();
    let ground = project.add_layer("Ground", "Tiles", 16).unwrap();
    let solid = project.add_layer("Solid", "IntGrid", 16).unwrap();
    let actors = project.add_layer("Actors", "Entities", 16).unwrap();
    let offsets = [(actors, [15, -3]), (solid, [4, 8]), (ground, [-7, 11])];
    for (index, (definition, [x, y])) in offsets.into_iter().enumerate() {
        let layer = project.layer_mut(key, index).unwrap();
        layer["pxOffsetX"] = json!(3);
        layer["pxOffsetY"] = json!(-2);
        project
            .update_layer_properties(definition, &json!({"pxOffsetX":x-3,"pxOffsetY":y+2}))
            .unwrap();
    }
    let actor = project.add_entity_def("Actor", 16, 16).unwrap();
    project.add_entity(key, 0, actor, 24, 40).unwrap();
    project.paint_int_grid(key, 1, 0, 0, 1).unwrap();
    let atlas = project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 32, 16)
        .unwrap();
    project.assign_tileset(ground, atlas).unwrap();
    project.layer_mut(key, 2).unwrap()["gridTiles"] = json!([
        {"px":[16,16],"t":0,"f":0}, {"px":[16,16],"t":1,"f":0}
    ]);

    project.export_tiled(dir.path()).unwrap();
    let xml = fs::read_to_string(dir.path().join("Level_0.tmx")).unwrap();
    project
        .export_tiled_format_with_report_progress(dir.path(), TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(dir.path().join("Level_0.tmj")).unwrap()).unwrap();
    let layers = map["layers"].as_array().unwrap();
    let layer = |name: &str| layers.iter().find(|layer| layer["name"] == name).unwrap();
    for (name, [x, y]) in [
        ("Actors", [15, -3]),
        ("Solid", [4, 8]),
        ("Ground_1", [-7, 11]),
        ("Ground_2", [-7, 11]),
    ] {
        let name_attr = format!("name=\"{name}\"");
        let header = xml
            .split('<')
            .find(|part| {
                (part.starts_with("layer ") || part.starts_with("objectgroup "))
                    && part.contains(&name_attr)
            })
            .unwrap()
            .split_once('>')
            .unwrap()
            .0;
        assert!(
            header.contains(&format!("offsetx=\"{x}\"")),
            "{name} {header}"
        );
        assert!(
            header.contains(&format!("offsety=\"{y}\"")),
            "{name} {header}"
        );
        assert_eq!(layer(name)["offsetx"], x, "{name}");
        assert_eq!(layer(name)["offsety"], y, "{name}");
    }
    assert_eq!(layer("Ground_1")["data"][33], 1);
    assert_eq!(layer("Ground_2")["data"][33], 2);
    assert_eq!(layer("Solid")["data"][0], 3);
    assert!(xml.contains("type=\"Actor\" x=\"24\" y=\"40\""));
    let object = &layer("Actors")["objects"][0];
    assert_eq!(object["x"], 24);
    assert_eq!(object["y"], 40);
    assert_eq!(object["properties"][0]["value"], 24);
    assert_eq!(object["properties"][1]["value"], 40);
}

#[test]
fn tiled_entity_bounds_use_resized_dimensions_and_keep_the_anchor() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let mut project = Project::new();
    let key = project.level_keys()[0];
    project.resize_level(key, 128, 128, 0, 0).unwrap();
    project.add_layer("Actors", "Entities", 16).unwrap();
    let actor = project.add_entity_def("Actor", 16, 24).unwrap();
    project
        .update_entity_definition(
            actor,
            &json!({"pivotX":0.5,"pivotY":1,"resizableX":true,"resizableY":true}),
        )
        .unwrap();
    project.add_entity(key, 0, actor, 64, 80).unwrap();
    project.resize_entity(key, 0, 0, 48, 40).unwrap();
    project.add_entity(key, 0, actor, 32, 48).unwrap();
    let legacy = project.layer_mut(key, 0).unwrap()["entityInstances"][1]
        .as_object_mut()
        .unwrap();
    legacy.remove("width");
    legacy.remove("height");

    project.export_tiled(dir.path()).unwrap();
    let xml = fs::read_to_string(dir.path().join("Level_0.tmx")).unwrap();
    assert!(
        xml.contains("x=\"40\" y=\"40\" width=\"48\" height=\"40\""),
        "{xml}"
    );
    assert!(
        xml.contains("x=\"24\" y=\"24\" width=\"16\" height=\"24\""),
        "{xml}"
    );
    assert!(xml.contains("name=\"__anchorX\" type=\"int\" value=\"64\""));
    assert!(xml.contains("name=\"__anchorY\" type=\"int\" value=\"80\""));

    project
        .export_tiled_format_with_report_progress(dir.path(), TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(dir.path().join("Level_0.tmj")).unwrap()).unwrap();
    let objects = map["layers"][0]["objects"].as_array().unwrap();
    let bounds =
        |object: &Value| ["x", "y", "width", "height"].map(|key| object[key].as_i64().unwrap());
    assert_eq!(bounds(&objects[0]), [40, 40, 48, 40]);
    assert_eq!(bounds(&objects[1]), [24, 24, 16, 24]);
    assert_eq!(objects[0]["properties"][0]["value"], 64);
    assert_eq!(objects[0]["properties"][1]["value"], 80);
}

#[test]
fn tiled_entity_file_properties_resolve_from_the_exported_map_directory() {
    use ldtk_core::exports::TiledFormat;

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("project");
    let output = dir.path().join("build/maps");
    fs::create_dir_all(source.join("assets")).unwrap();
    fs::write(source.join("assets/voice & music.ogg"), b"audio fixture").unwrap();
    fs::write(dir.path().join("shared.txt"), b"shared fixture").unwrap();
    let mut project = Project::new();
    project.path = Some(source.join("Map.ldtk"));
    project.add_layer("Actors", "Entities", 16).unwrap();
    let key = project.level_keys()[0];
    project.layer_mut(key, 0).unwrap()["entityInstances"] = json!([{
        "__identifier":"Actor", "px":[0,0], "width":16, "height":16,
        "fieldInstances":[
            {"__identifier":"Voice","__type":"FilePath","__value":"assets/voice & music.ogg"},
            {"__identifier":"Files","__type":"Array<FilePath>","__value":["../shared.txt","C:\\new\\effects.ogg"]},
            {"__identifier":"Text","__type":"String","__value":"assets/voice & music.ogg"}
        ]
    }]);

    project.export_tiled(&output).unwrap();
    let xml = fs::read_to_string(output.join("Level_0.tmx")).unwrap();
    for property in [
        "name=\"Voice\" type=\"file\" value=\"../../project/assets/voice &amp; music.ogg\"",
        "name=\"Files_0\" type=\"file\" value=\"../../shared.txt\"",
        "name=\"Files_1\" type=\"file\" value=\"C:/new/effects.ogg\"",
        "name=\"Text\" value=\"assets/voice &amp; music.ogg\"",
    ] {
        assert!(xml.contains(property), "missing {property} in {xml}");
    }
    project
        .export_tiled_format_with_report_progress(&output, TiledFormat::Tmj, |_, _| {})
        .unwrap();
    let map: Value =
        serde_json::from_slice(&fs::read(output.join("Level_0.tmj")).unwrap()).unwrap();
    let properties = map["layers"][0]["objects"][0]["properties"]
        .as_array()
        .unwrap();
    assert_eq!(
        properties[4..],
        [
            json!({"name":"Voice","type":"file","value":"../../project/assets/voice & music.ogg"}),
            json!({"name":"Files_0","type":"file","value":"../../shared.txt"}),
            json!({"name":"Files_1","type":"file","value":"C:/new/effects.ogg"}),
            json!({"name":"Text","type":"string","value":"assets/voice & music.ogg"}),
        ]
    );
    assert_eq!(
        fs::read(output.join(properties[4]["value"].as_str().unwrap())).unwrap(),
        b"audio fixture"
    );
    assert_eq!(
        fs::read(output.join(properties[5]["value"].as_str().unwrap())).unwrap(),
        b"shared fixture"
    );
}

#[test]
fn gamemaker_reexport_preserves_identical_files_and_replaces_changed_rooms() {
    let mut project = project();
    project
        .add_tileset(std::path::Path::new("atlas.png"), 16, 16, 16)
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let paths = project.export_gamemaker(dir.path()).unwrap();
    let modified = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(946_684_800);
    for path in &paths {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }
    assert_eq!(project.export_gamemaker(dir.path()).unwrap(), paths);
    for path in &paths {
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
    }
    project.json["levels"][0]["pxWid"] = json!(48);
    assert_eq!(project.export_gamemaker(dir.path()).unwrap(), paths);
    let room = dir.path().join("Level_0.yy");
    let content: Value = serde_json::from_slice(&fs::read(&room).unwrap()).unwrap();
    assert_eq!(content["roomSettings"]["Width"], 48);
    for path in paths.iter().filter(|path| **path != room) {
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
    }
}

#[test]
fn switching_to_simplified_preserves_png_exports_until_success() {
    check_configured_export_switch(false);
}

#[test]
fn switching_to_png_preserves_simplified_exports_until_success() {
    check_configured_export_switch(true);
}

fn check_configured_export_switch(from_simplified: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.ldtk");
    let output = path.with_extension("");
    let mut project = project();
    project.add_level(None, "Second", 32, 32).unwrap();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    project.json["simplifiedExport"] = json!(from_simplified);
    let previous = project.save_with_exports(&path).unwrap();
    assert!(previous.export_errors.is_empty());
    let previous_files: Vec<_> = previous
        .outputs
        .into_iter()
        .filter(|file| file != &path)
        .map(|file| {
            let bytes = fs::read(&file).unwrap();
            (file, bytes)
        })
        .collect();
    assert!(previous_files.len() >= 2);

    let (first_output, blocked_output) = if from_simplified {
        ("png/Level_0.png", "png/Second.png")
    } else {
        (
            "simplified/Level_0/data.json",
            "simplified/Second/data.json",
        )
    };
    let first_output = output.join(first_output);
    let blocked_output = output.join(blocked_output);
    fs::create_dir_all(&blocked_output).unwrap();
    project.json["simplifiedExport"] = json!(!from_simplified);

    let failed = project.save_with_exports(&path).unwrap();
    assert!(!failed.export_errors.is_empty());
    assert_eq!(failed.outputs, std::slice::from_ref(&path));
    assert_eq!(
        Project::load(&path).unwrap().json["simplifiedExport"],
        !from_simplified
    );
    assert!(!first_output.exists());
    for (file, bytes) in &previous_files {
        assert!(
            fs::read(file).is_ok_and(|actual| &actual == bytes),
            "{}",
            file.display()
        );
    }

    fs::remove_dir(&blocked_output).unwrap();
    let retried = project.save_with_exports(&path).unwrap();
    assert!(
        retried.export_errors.is_empty(),
        "{:?}",
        retried.export_errors
    );
    assert!(retried.outputs.contains(&first_output) && retried.outputs.contains(&blocked_output));
    assert!(first_output.is_file() && blocked_output.is_file());
    assert!(previous_files.iter().all(|(file, _)| !file.exists()));
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
    let repeated = p.save_with_exports(&path).unwrap();
    assert!(repeated.export_errors.is_empty());
    assert!(dir.join("Level_0__Solid-int.png").exists());
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

#[cfg(unix)]
#[test]
fn configured_export_cleanup_preserves_hard_links_to_live_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.ldtk");
    let output = dir.path().join("game/png");
    let mut project = project();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let first = project.save_with_exports(&path).unwrap();
    assert!(first.export_errors.is_empty());
    let live = output.join("Level_0.png");
    let alias = output.join("copy.png");
    let stale = output.join("stale.png");
    fs::hard_link(&live, &alias).unwrap();
    fs::write(&stale, b"stale file").unwrap();

    let second = project.save_with_exports(&path).unwrap();
    assert!(second.export_errors.is_empty());
    assert_eq!(fs::read(&alias).unwrap(), fs::read(&live).unwrap());
    assert!(!stale.exists());
}

#[test]
fn configured_export_cleanup_keeps_resources_referenced_by_the_project() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.ldtk");
    let png = directory.path().join("game/png");
    let simple = directory.path().join("game/simplified");
    fs::create_dir_all(&png).unwrap();
    fs::create_dir_all(&simple).unwrap();
    let atlas = png.join("Atlas.png");
    let background = simple.join("Backdrop.png");
    fs::write(&atlas, b"atlas resource").unwrap();
    image::RgbaImage::from_pixel(32, 16, image::Rgba([8, 19, 29, 255]))
        .save(&background)
        .unwrap();
    let background_bytes = fs::read(&background).unwrap();
    let stale_png = png.join("old.png");
    let stale_json = simple.join("old.json");
    fs::write(&stale_png, b"outdated image").unwrap();
    fs::write(&stale_json, b"outdated data").unwrap();

    let mut project = project();
    project.save(&path).unwrap();
    project.add_tileset(&atlas, 16, 16, 16).unwrap();
    let first = project.level_keys()[0];
    project.level_mut(first).unwrap()["bgRelPath"] = json!("game/simplified/Backdrop.png");
    project.set_external_levels(true).unwrap();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let report = project.save_with_exports(&path).unwrap();

    assert!(
        report.export_errors.is_empty(),
        "{:?}",
        report.export_errors
    );
    assert!(report.outputs.contains(&png.join("Level_0.png")));
    assert!(!report.outputs.contains(&atlas) && !report.outputs.contains(&background));
    assert!(fs::read(&atlas).is_ok_and(|bytes| bytes == b"atlas resource"));
    assert!(fs::read(&background).is_ok_and(|bytes| bytes == background_bytes));
    assert!(!stale_png.exists() && !stale_json.exists());
    let reopened = Project::load(&path).unwrap();
    assert_eq!(
        reopened.level(first).unwrap()["bgRelPath"],
        "game/simplified/Backdrop.png"
    );
}

#[test]
fn configured_export_collision_preserves_previous_outputs_and_referenced_assets() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.ldtk");
    let output = directory.path().join("game/png");
    let mut project = project();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let first = project.save_with_exports(&path).unwrap();
    assert!(first.export_errors.is_empty());
    let previous = output.join("Level_0.png");
    let previous_bytes = fs::read(&previous).unwrap();
    let atlas = output.join("Second.png");
    fs::write(&atlas, b"referenced atlas").unwrap();
    project.add_tileset(&atlas, 16, 16, 16).unwrap();
    project.add_level(None, "Second", 32, 32).unwrap();
    project.json["defaultLevelBgColor"] = json!("#123456");

    let report = project.save_with_exports(&path).unwrap();

    assert!(fs::read(&atlas).is_ok_and(|bytes| bytes == b"referenced atlas"));
    assert_eq!(fs::read(&previous).unwrap(), previous_bytes);
    assert_eq!(report.outputs, std::slice::from_ref(&path));
    assert!(
        report
            .export_errors
            .iter()
            .any(|error| error.contains("Second.png"))
    );
    assert_eq!(Project::load(&path).unwrap().level_keys().len(), 2);

    project.json["pngFilePattern"] = json!("%level_name-alternate");
    let report = project.save_with_exports(&path).unwrap();
    assert!(
        report.export_errors.is_empty(),
        "{:?}",
        report.export_errors
    );
    assert!(output.join("Second-alternate.png").is_file());
    assert!(fs::read(&atlas).is_ok_and(|bytes| bytes == b"referenced atlas"));
}

#[test]
fn manual_export_can_create_a_missing_referenced_file_and_reuse_identical_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("Level_0.png");
    let mut project = project();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    project.add_tileset(&output, 16, 32, 16).unwrap();
    let first = project.export_png(directory.path()).unwrap();
    assert_eq!(first, std::slice::from_ref(&output));
    let original = fs::read(&output).unwrap();
    assert_eq!(project.export_png(directory.path()).unwrap(), first);

    project.json["defaultLevelBgColor"] = json!("#123456");
    let changed = project.export_png(directory.path());
    assert!(changed.is_err());
    assert_eq!(fs::read(output).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn configured_export_cleanup_preserves_contents_of_linked_output_directory() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = dir.path().join("game.ldtk");
    let output = dir.path().join("game/png");
    let shared = dir.path().join("shared");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::create_dir(&shared).unwrap();
    fs::write(shared.join("artwork.png"), b"linked artwork").unwrap();
    std::os::unix::fs::symlink(&shared, &output).unwrap();

    let mut project = project();
    project.json["imageExportMode"] = json!("OneImagePerLevel");
    let report = project.save_with_exports(&project_path).unwrap();
    assert!(
        report.export_errors.is_empty(),
        "{:?}",
        report.export_errors
    );
    assert!(report.outputs.contains(&output.join("Level_0.png")));
    assert!(shared.join("Level_0.png").is_file());
    assert!(fs::read(shared.join("artwork.png")).is_ok_and(|bytes| bytes == b"linked artwork"));
    assert!(fs::symlink_metadata(&output).unwrap().is_symlink());
}

#[cfg(unix)]
#[test]
fn configured_export_cleanup_preserves_contents_beneath_linked_project_directory() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = dir.path().join("game.ldtk");
    let output = project_path.with_extension("");
    let shared = dir.path().join("shared");
    fs::create_dir_all(shared.join("png")).unwrap();
    fs::write(shared.join("png/artwork.png"), b"linked artwork").unwrap();
    std::os::unix::fs::symlink(&shared, &output).unwrap();

    let mut project = project();
    project.json["imageExportMode"] = json!("None");
    let report = project.save_with_exports(&project_path).unwrap();
    assert!(
        report.export_errors.is_empty(),
        "{:?}",
        report.export_errors
    );
    assert_eq!(report.outputs, std::slice::from_ref(&project_path));
    assert!(fs::read(shared.join("png/artwork.png")).is_ok_and(|bytes| bytes == b"linked artwork"));
    assert!(fs::symlink_metadata(&output).unwrap().is_symlink());
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
